# 05: Memória física: saber o que é de quem e entregar pedaços

> **Você vai entender:** como o kernel descobre quanta RAM existe e onde
> ela está; por que a memória de um PC é cheia de "buracos"; o que é um
> *quadro* de 4 KiB; e como funciona um **alocador de quadros** baseado em
> *bitmap*, a peça sobre a qual toda a gestão de memória será construída.
>
> **Arquivos:** `src/memoria/mod.rs`, `src/memoria/mapa.rs`, `src/memoria/quadros.rs`
> (chamados em `kernel_main` logo depois da IDT)
> **Livro:** capítulo 6
> **Testar:** `./run.sh teste-quadros`, `./run.sh teste-liberacao-dupla`,
> `./run.sh listar-regioes`, e com mais memória: `RAM=4G ./run.sh teste-quadros`
> **Rust novo aqui:** `enum` ([§19](rust-basico.md#19-enum-simples)),
> cadeias de iteradores ([§20](rust-basico.md#20-iteradores-em-cadeia-filter-map-fold)),
> slices ([§21](rust-basico.md#21-slices-um-pedaço-de-um-array)),
> *newtype* ([§22](rust-basico.md#22-newtype-um-tipo-próprio-para-um-número)),
> `assert!` e `expect` ([§23](rust-basico.md#23-assert-e-expect-verificações-que-viram-panic))
>
> **Código deste capítulo:** os docs 00 a 06 estão num único commit: [estado no fim do doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). O que este capítulo acrescenta está em [Mudanças nos arquivos](#mudanças-nos-arquivos).

---

## 1. O problema

Até agora o kernel só usou memória que **outros** reservaram para ele:

- o código e as variáveis `static` (GDT, IDT, pilha de emergência) vieram
  dentro do executável, e o firmware os carregou;
- a pilha em que o kernel roda foi criada pelo firmware.

Para tudo que vem pela frente (tabelas de página no capítulo 7, `Vec` e
`String` no capítulo 8, uma pilha por processo depois), o kernel precisa
**pedir memória durante a execução**. Em Java, você escreve `new` e a JVM
pede memória ao sistema operacional. Aqui **nós somos o sistema
operacional**: precisamos responder a pergunta "qual pedaço da RAM está
livre agora?".

Isso exige duas coisas, que são as duas partes deste capítulo:

1. **Saber o que existe** (o mapa): quanta RAM há, em que endereços, e o
   que já está ocupado por firmware, dispositivos ou pelo próprio kernel.
2. **Controlar o que foi entregue** (o alocador): entregar pedaços livres
   a quem pede, receber de volta e nunca entregar o mesmo pedaço para dois
   donos.

---

## 2. A memória de um PC não é um bloco contínuo

A primeira intuição é imaginar que uma máquina com 128 MiB tem endereços
de 0 a 128 MiB, todos de RAM. Não é assim. O espaço de **endereços físicos**
é compartilhado entre a RAM, os dispositivos e o firmware. Este é o mapa
real do QEMU com 128 MiB, gerado por `./run.sh listar-regioes` (resumido):

```
0x000000000000 - 0x000000001000        4 KiB  BootServices
0x000000001000 - 0x0000000a0000      636 KiB  Livre
               (0xa0000 - 0x100000: nada no mapa: buraco do PC original)
0x000000100000 - 0x000000800000     7168 KiB  Livre
0x000000800000 - 0x000000808000       32 KiB  ACPI
...                                   (dezenas de regiões alternando tipos)
0x000006070000 - 0x0000060a6000      216 KiB  Kernel       ← o SiliconOS
...
0x000007f78000 - 0x000008000000      544 KiB  ACPI         ← fim da RAM: 128 MiB
               (0x80000000: o framebuffer, fora do mapa)
0x00fd00000000 - 0x010000000000  12582912 KiB  Reservada   ← 12 GiB perto de 1 TiB: não é RAM
```

Três coisas a observar:

### 2.1 Há buracos

Entre `0xA0000` (640 KiB) e `0x100000` (1 MiB) **não há nada no mapa**.
Essa faixa é uma herança do IBM PC de 1981: ali ficavam a memória da placa
de vídeo e as ROMs, e até hoje ela é reservada. Endereços que não aparecem
no mapa **não são RAM** e nunca devem ser usados.

### 2.2 Há dispositivos no meio dos endereços

O framebuffer (doc 01) está em `0x80000000`, um endereço **físico** que não
é RAM: escrever ali vai direto para a placa de vídeo (é o *MMIO*, ver
[glossário](glossario.md#memória)). Por isso ele aparece como "fora do
mapa".

A região "Reservada" de 12 GiB perto de 1 TiB também não é RAM: é uma
faixa de endereços que o QEMU reserva para o chipset. Ela aparece no mapa
justamente para avisar "não use".

### 2.3 Com mais memória, os buracos ficam maiores

Com `RAM=4G ./run.sh listar-regioes`, o fim do mapa fica assim:

```
0x0000bff78000 - 0x0000c0000000      544 KiB  ACPI          ← RAM até 3 GiB
               (0xc0000000 - 0x100000000: buraco de 1 GiB para dispositivos PCI;
                o framebuffer está aqui, em 0xc0000000)
0x000100000000 - 0x000140000000  1048576 KiB  Livre         ← o 1 GiB restante, acima de 4 GiB
```

Os PCs reservam o último pedaço abaixo de 4 GiB para dispositivos (a
"janela PCI"). A RAM que caberia ali **é remapeada para depois de 4 GiB**.
Resultado: uma máquina com 4 GiB de RAM tem endereços de RAM até **5 GiB**.
O alocador precisa lidar com isso.

---

## 3. Parte 1: o mapa de memória do kernel

### 3.1 O que o UEFI entrega

No doc 00 vimos que `exit_boot_services` devolve o **mapa de memória**: uma
lista de *descritores*, cada um com endereço inicial, número de páginas de
4 KiB e um **tipo**. O UEFI define cerca de 16 tipos. Os que aparecem no
QEMU:

| Tipo UEFI | O que é | Podemos usar? |
|---|---|---|
| `CONVENTIONAL` | RAM livre | **sim** |
| `BOOT_SERVICES_CODE/DATA` | RAM usada pelo firmware durante o boot | em tese sim, depois do `exit_boot_services`... (seção 3.4) |
| `LOADER_CODE/DATA` | o programa que o UEFI carregou (o SiliconOS) e o que ele alocou | **não**: é o kernel |
| `RUNTIME_SERVICES_CODE/DATA` | partes do firmware que continuam ativas (relógio, variáveis) | **nunca** |
| `ACPI_RECLAIM`, `ACPI_NON_VOLATILE` | tabelas que descrevem o hardware | não (ainda) |
| `RESERVED`, `MMIO`... | reservado, dispositivos | **nunca** |

### 3.2 Por que copiar para um formato nosso

O mapa do UEFI tem três inconvenientes:

1. **Depende da crate `uefi`** e de um buffer que o firmware alocou.
   Queremos que o resto do kernel trabalhe com tipos nossos.
2. **Não vem necessariamente em ordem** de endereço (a especificação não
   garante).
3. **É fragmentado:** 127 entradas no QEMU, muitas delas vizinhas e do mesmo
   tipo.

Então o `memoria::mapa::init` faz três passos: copiar convertendo o tipo,
ordenar e juntar vizinhas. Resultado no boot:

```
Mapa de memoria: 127 entradas do UEFI -> 31 regioes (vizinhas unidas)
```

### 3.3 O código

**Os tipos simplificados** são um `enum` ([rust-basico §19](rust-basico.md#19-enum-simples)):

```rust
pub enum Tipo {
    Livre, BootServices, Kernel, Runtime, Acpi, Mmio, Reservada,
}
```

e a conversão é um `match` ([rust-basico §7](rust-basico.md#7-match-um-switch-que-obriga-a-cobrir-tudo)):

```rust
fn de_uefi(t: MemoryType) -> Tipo {
    match t {
        MemoryType::CONVENTIONAL => Tipo::Livre,
        MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => Tipo::BootServices,
        MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => Tipo::Kernel,
        ...
        _ => Tipo::Reservada,      // qualquer tipo desconhecido: na dúvida, não usar
    }
}
```

O `_ => Tipo::Reservada` é uma decisão de segurança: se um firmware
reportar um tipo que não conhecemos, tratamos como "não tocar".

**Uma região** é início + tamanho + tipo:

```rust
pub struct Regiao {
    pub inicio: u64,     // endereço físico do primeiro byte
    pub quadros: u64,    // tamanho, em blocos de 4 KiB
    pub tipo: Tipo,
}

impl Regiao {
    pub fn fim(&self) -> u64 {               // primeiro endereço DEPOIS da região
        self.inicio + self.quadros * TAMANHO_QUADRO
    }
    pub fn contem(&self, endereco: u64) -> bool {
        endereco >= self.inicio && endereco < self.fim()
    }
}
```

O intervalo é **semiaberto**, `[inicio, fim)`: inclui o início e exclui o
fim, como `substring(inicio, fim)` em Java. Isso evita os erros de "mais um
ou menos um".

**O mapa** guarda as regiões num array de tamanho fixo, porque ainda não
temos `Vec` (que precisa de um alocador, exatamente o que estamos
construindo):

```rust
const MAX_REGIOES: usize = 512;   // o OVMF entrega ~130

pub struct Mapa {
    regioes: [Regiao; MAX_REGIOES],
    qtd: usize,                    // quantas posições estão em uso
    pub entradas_uefi: usize,
}
```

É o mesmo padrão de um `ArrayList` por dentro: um array grande e um
contador. O método `regioes()` devolve só a parte usada, como um
`subList(0, qtd)` ([rust-basico §21](rust-basico.md#21-slices-um-pedaço-de-um-array)).

**Os três passos do `init`:**

```rust
// 1. Copiar, convertendo o tipo.
for d in mapa_uefi.entries() {
    assert!(m.qtd < MAX_REGIOES, "mapa de memoria com mais de {} regioes", MAX_REGIOES);
    m.regioes[m.qtd] = Regiao { inicio: d.phys_start, quadros: d.page_count, tipo: Tipo::de_uefi(d.ty) };
    m.qtd += 1;
}

// 2. Ordenar por endereço.
m.regioes[..m.qtd].sort_unstable_by_key(|r| r.inicio);

// 3. Juntar vizinhas do mesmo tipo: [A][A][B][A] -> [AA][B][A]
let mut escrita = 0;
for leitura in 0..m.qtd {
    let r = m.regioes[leitura];
    if escrita > 0 {
        let anterior = &mut m.regioes[escrita - 1];
        if anterior.tipo == r.tipo && anterior.fim() == r.inicio {
            anterior.quadros += r.quadros;     // estende a anterior
            continue;                          // e descarta esta
        }
    }
    m.regioes[escrita] = r;
    escrita += 1;
}
m.qtd = escrita;
```

O passo 3 usa a técnica de **dois índices** no mesmo array: `leitura`
percorre tudo; `escrita` marca até onde vai o resultado. Quando a região
atual "cola" na anterior (mesmo tipo, e a anterior termina exatamente onde
esta começa), ela é absorvida. Assim o array é compactado no próprio lugar,
sem precisar de um segundo array.

O mapa fica num `Once` ([rust-basico §11](rust-basico.md#11-mutex-e-once-da-crate-spin)),
pelo mesmo motivo da GDT: é montado uma vez e precisa viver para sempre.

### 3.4 Uma descoberta: a pilha está em memória "do firmware"

O boot imprime onde estão as coisas que o kernel está usando:

```
  codigo do kernel 0x607dee0 -> Kernel
  GDT              0x60a2070 -> Kernel
  pilha atual(RSP) 0x7e95e00 -> BootServices
  framebuffer      0x80000000 -> fora do mapa
```

O código e a GDT estão na região "Kernel", como esperado. Mas **a pilha em
que o kernel roda agora está numa região `BootServices`**: o firmware a
criou para executar o nosso programa, com o tipo "dados do firmware".

A especificação UEFI diz que, depois de `exit_boot_services`, a memória
`BootServices` pode ser reaproveitada pelo sistema operacional. Se o
alocador entregasse essa memória, **ele poderia entregar a própria pilha
do kernel** para outro uso. O próximo dono escreveria por cima das
variáveis locais e dos endereços de retorno, e o kernel quebraria de forma
completamente misteriosa.

Por isso, **por enquanto, o alocador usa só as regiões `Livre`**. Os 40 MiB
de `BootServices` serão recuperados quando o kernel tiver uma pilha própria
(fica para depois da paginação). Este é um bom exemplo de por que o kernel
precisa **saber onde estão as próprias coisas** antes de sair distribuindo
memória.

---

## 4. Parte 2: o alocador de quadros

### 4.1 Quadros de 4 KiB

O alocador não entrega bytes soltos; entrega **quadros** (*frames*): blocos
de **4 KiB** (4096 bytes), alinhados, isto é, começando num endereço
múltiplo de 4096.

Por que 4 KiB? Porque é o tamanho da **página** da paginação x86 (capítulo
7): a CPU traduz endereços de 4 em 4 KiB. Todo o gerenciamento de memória de
um kernel x86 é feito nessa unidade. Pedaços menores (um `String` de 20
bytes) serão cortados de dentro de quadros por um segundo alocador, o
*heap*, no capítulo 8.

```
endereço físico:   0x0000    0x1000    0x2000    0x3000    0x4000
                   ├─────────┼─────────┼─────────┼─────────┤
                   │quadro 0 │quadro 1 │quadro 2 │quadro 3 │ ...
                   └─────────┴─────────┴─────────┴─────────┘
número do quadro = endereço / 4096        endereço = número × 4096
```

### 4.2 A ideia do bitmap

Precisamos lembrar, para cada quadro, se ele está livre ou ocupado. A forma
mais simples é um **bitmap**: **um bit por quadro**, 1 = ocupado, 0 = livre.

```
bitmap[0] (u64) = ...0000 0000 0000 0111  → quadros 0,1,2 ocupados; 3,4,5... livres
bitmap[1] (u64) = ...                      → quadros 64 a 127
```

Os bits são agrupados em números de 64 bits (`u64`). O quadro `q` está na
palavra `q / 64`, no bit `q % 64`.

Custo: 1 bit para cada 4096 bytes, ou seja, **1/32 768 da memória**.

| RAM coberta | Quadros | Bitmap |
|---|---|---|
| 128 MiB | 32 768 | 4 KiB |
| 5 GiB (máquina de 4 GiB, seção 2.3) | 1 310 720 | 160 KiB |
| 9 GiB (máquina de 8 GiB) | 2 359 296 | 288 KiB |

Esses são os tamanhos reais que o boot mostrou.

Há alternativas (listas encadeadas de quadros livres, *buddy allocator*), que
são mais rápidas para alocar mas mais difíceis de entender e depurar. O
bitmap é o ponto de partida clássico: simples, compacto e fácil de
inspecionar.

### 4.3 Onde guardar o próprio bitmap

O bitmap de uma máquina de 8 GiB tem 288 KiB. Onde colocar isso, se ainda
não temos alocador? É o problema do ovo e da galinha. A solução clássica:

1. calcular o tamanho do bitmap;
2. procurar a **primeira região Livre** que caiba nele inteiro;
3. colocar o bitmap no começo dela;
4. marcar esses quadros como **ocupados** no próprio bitmap.

O bitmap fica dentro da memória que ele mesmo administra e se protege
marcando-se como ocupado.

```rust
let regiao = mapa
    .regioes()
    .iter()
    .filter(|r| r.tipo == Tipo::Livre)
    .find(|r| {
        let inicio = r.inicio.max(INICIO_UTILIZAVEL);
        inicio + quadros_bitmap * TAMANHO_QUADRO <= r.fim()
    })
    .expect("nenhuma regiao livre comporta o bitmap de quadros");
```

Essa cadeia de `.filter().find()` é como um `stream()` do Java: "das
regiões, filtre as livres e ache a primeira em que o bitmap cabe"
([rust-basico §20](rust-basico.md#20-iteradores-em-cadeia-filter-map-fold)).

No boot: `Alocador de quadros: bitmap de 4 KiB em 0x100000`. É exatamente o
início da primeira região livre acima de 1 MiB.

### 4.4 Transformando um endereço físico numa variável Rust

O bitmap está num endereço físico, por exemplo `0x100000`. Para usá-lo como
um array Rust comum, convertemos o endereço num **slice**:

```rust
let bitmap: &'static mut [u64] = unsafe {
    core::slice::from_raw_parts_mut(endereco_bitmap as *mut u64, palavras)
};
```

`from_raw_parts_mut(ponteiro, quantidade)` diz: "a partir deste endereço,
existem `quantidade` valores `u64`; trate isso como um array". Daqui em
diante, `bitmap[i]` funciona com verificação de limites, como qualquer
array.

É `unsafe` porque o compilador não tem como saber se aquela memória existe
e se ninguém mais a usa. Quem garante somos nós: ela é uma região `Livre` do
mapa, e vamos marcá-la como ocupada logo em seguida.

> **Por que usar um endereço físico como ponteiro funciona?** Ponteiros na
> CPU são endereços **virtuais**, traduzidos pela paginação. O UEFI deixou a
> paginação em **mapeamento identidade**: o endereço virtual X aponta para o
> endereço físico X. Por isso, hoje, "endereço físico" e "ponteiro" são a
> mesma coisa. Isso mudou no capítulo seguinte: quando o kernel monta as
> próprias tabelas de página, o bitmap passa a ser acessado pela metade alta
> do espaço de endereços ([doc 06, seção 5.7](06-paginacao.md#57-a-troca)).

### 4.5 A inicialização: começar com tudo ocupado

```rust
// 3. Começar com TUDO ocupado. Só o que provarmos ser livre vira 0.
bitmap.fill(u64::MAX);
```

`u64::MAX` é um número com todos os 64 bits em 1. Todos os quadros começam
**ocupados**. Depois, só as regiões `Livre` do mapa (acima de 1 MiB) são
marcadas como livres, e por fim os quadros do próprio bitmap voltam a ser
marcados como ocupados:

```rust
for r in mapa.regioes().iter().filter(|r| r.tipo == Tipo::Livre) {
    let inicio = r.inicio.max(INICIO_UTILIZAVEL);
    if inicio >= r.fim() { continue; }
    let primeiro = (inicio / TAMANHO_QUADRO) as usize;
    let n = ((r.fim() - inicio) / TAMANHO_QUADRO) as usize;
    a.marcar_intervalo(primeiro, n, false);
}
a.marcar_intervalo((endereco_bitmap / TAMANHO_QUADRO) as usize, quadros_bitmap as usize, true);
```

Por que começar com "tudo ocupado" e não "tudo livre"? Porque **os dois
erros possíveis não têm o mesmo custo**:

- marcar como ocupado algo que era livre: **desperdiça** um pouco de
  memória. Ninguém percebe;
- marcar como livre algo que era ocupado: o alocador **entrega a memória de
  alguém** (do firmware, de um dispositivo, do próprio kernel). Isso causa
  corrupção silenciosa, o pior tipo de bug.

Começar com tudo ocupado garante que os buracos, os dispositivos, as regiões
reservadas e tudo que não está no mapa nunca sejam entregues. Só é
liberado o que temos certeza de que é livre.

**Por que pular o primeiro 1 MiB** (`INICIO_UTILIZAVEL`)? Duas razões:

1. o quadro 0 contém o endereço 0. Um ponteiro nulo apontando para memória
   "válida" esconderia bugs;
2. quando formos ligar as outras CPUs (capítulo 11), elas começam em modo de
   16 bits e só enxergam o primeiro megabyte. Precisaremos de memória livre
   ali.

O custo é desprezível: 636 KiB.

### 4.6 Alocar: achar o primeiro bit 0

```rust
pub fn alocar(&mut self) -> Option<Quadro> {
    let palavras = self.bitmap.len();
    for i in 0..palavras {
        let p = (self.proxima_palavra + i) % palavras;
        let valor = self.bitmap[p];
        if valor != u64::MAX {                        // há algum bit 0 aqui
            let bit = valor.trailing_ones() as usize; // posição do primeiro 0
            let quadro = p * 64 + bit;
            if quadro >= self.total { continue; }
            self.marcar(quadro, true);
            self.livres -= 1;
            self.proxima_palavra = p;
            return Some(Quadro(quadro as u64 * TAMANHO_QUADRO));
        }
    }
    None                                              // memória acabou
}
```

Os truques de desempenho:

- **Testar 64 quadros de uma vez:** se a palavra inteira é `u64::MAX`, os 64
  quadros estão ocupados; pulamos sem olhar bit a bit.
- **`trailing_ones()`** conta quantos bits 1 seguidos há a partir do bit 0.
  Esse número é a posição do primeiro bit 0. A CPU tem uma instrução para
  isso, então custa um único ciclo.
  ```
  valor = ...1111 0111   → trailing_ones = 3 → o quadro livre é o bit 3
  ```
- **`proxima_palavra`** lembra onde a última busca parou. Sem isso, cada
  alocação recomeçaria do início e passaria por todas as palavras já cheias.
  O `% palavras` faz a busca "dar a volta" no fim do array.

O retorno é `Option<Quadro>`: `Some(quadro)` ou `None` quando a memória
acabou. Não existe `null` nem exceção ([rust-basico §6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)):
quem chama é obrigado a tratar o caso "acabou".

### 4.7 O tipo `Quadro`

```rust
pub struct Quadro(pub u64);
```

Um `Quadro` é só um `u64` com outro nome: o padrão *newtype*
([rust-basico §22](rust-basico.md#22-newtype-um-tipo-próprio-para-um-número)).
A vantagem é que o compilador **não deixa misturar** um endereço físico de
quadro com um número qualquer (um tamanho, um índice, um endereço virtual).
A partir do capítulo 7 teremos endereços físicos e virtuais convivendo, e
confundi-los é uma fonte clássica de bugs em kernels. O custo em tempo de
execução é zero: na memória, um `Quadro` é exatamente um `u64`.

### 4.8 Liberar, com verificações

```rust
pub fn liberar(&mut self, quadro: Quadro) {
    assert!(quadro.0 % TAMANHO_QUADRO == 0, "liberar: endereco {:#x} nao alinhado em 4 KiB", quadro.0);
    let i = quadro.indice();
    assert!(i < self.total, "liberar: {:#x} esta fora da RAM", quadro.0);
    assert!(self.ocupado(i), "liberar: quadro {:#x} ja estava livre (liberacao dupla)", quadro.0);
    self.marcar(i, false);
    self.livres += 1;
    ...
}
```

As três verificações pegam os erros mais comuns de quem usa o alocador. A
mais importante é a **liberação dupla**: se um quadro é liberado duas
vezes, ele pode acabar entregue a dois donos diferentes no futuro, e um vai
escrever por cima do outro. Esse bug aparece longe da causa e é muito
difícil de rastrear. Detectá-lo **na hora**, com panic, transforma um bug
misterioso numa mensagem clara.

`assert!(condição, mensagem)` é um `if !condição { panic!(mensagem) }`
([rust-basico §23](rust-basico.md#23-assert-e-expect-verificações-que-viram-panic)).

### 4.9 O alocador global

Como o console (doc 01), o alocador precisa ser acessível de qualquer lugar,
então fica num `static Mutex<Option<...>>`:

```rust
pub static QUADROS: Mutex<Option<AlocadorQuadros>> = Mutex::new(None);

pub fn alocar() -> Option<Quadro> {
    QUADROS.lock().as_mut().expect("alocador de quadros nao inicializado").alocar()
}
```

As funções `alocar()`, `liberar()` e `livres()` no nível do módulo são
atalhos que escondem o `lock()` e o `Option`. Quem usa só escreve
`memoria::quadros::alocar()`.

---

## 5. Como foi testado

### 5.1 O boot normal

```
Alocador de quadros: bitmap de 4 KiB em 0x100000
  20613 quadros livres (80 MiB) de 32768 cobertos
```

O mapa diz 81 MiB livres; o alocador oferece 80 MiB. A diferença é o
primeiro megabyte (pulado de propósito) e o próprio bitmap.

### 5.2 `./run.sh teste-quadros`

O teste faz cinco verificações em sequência:

1. **aloca 1000 quadros** e confere que o contador de livres diminuiu 1000;
2. confere que cada quadro está **alinhado** em 4 KiB, **acima de 1 MiB**,
   dentro de uma região **Livre** do mapa e **não foi entregue duas vezes**;
3. **escreve um padrão diferente em cada quadro** (512 números de 64 bits,
   combinando o número do quadro e a posição) e depois **lê tudo de volta**.
   Isso prova que os quadros são RAM de verdade, que o mapeamento identidade
   funciona e que nenhum quadro se sobrepõe a outro: se dois se
   sobrepusessem, o segundo padrão apagaria parte do primeiro;
4. **libera todos** e confere que o contador voltou ao valor inicial;
5. **aloca de novo** e confere que o primeiro quadro entregue é o mesmo de
   antes (a busca recomeça do lugar certo depois das liberações).

```
Teste de quadros OK: 1000 alocados (0x101000..0x4e8000), escritos,
  conferidos, liberados e realocados. Livres: 20611
```

(O número de livres varia alguns quadros entre compilações diferentes,
porque o tamanho do kernel muda; o que o teste confere é que ele volta ao
valor do início.)

### 5.3 `./run.sh teste-liberacao-dupla`

```
Teste: liberando o quadro 0x101000 duas vezes...
!!! KERNEL PANIC !!!
Local: src/memoria/quadros.rs:100:9
Mensagem: liberar: quadro 0x101000 ja estava livre (liberacao dupla)
```

### 5.4 Com mais memória: `RAM=4G` e `RAM=8G`

O `run.sh` ganhou a opção `RAM=...`, que muda a memória da máquina emulada
(o padrão do QEMU é 128 MiB). O `teste-quadros` passou nos dois casos:

| RAM | Fim da RAM | Bitmap | Quadros livres | Observação |
|---|---|---|---|---|
| 128 MiB | 128 MiB | 4 KiB | 20 613 (80 MiB) | — |
| 4 GiB | **5 GiB** | 160 KiB | 1 036 381 (4048 MiB) | buraco PCI de 3 a 4 GiB (seção 2.3) |
| 8 GiB | **9 GiB** | 288 KiB | 2 084 925 (8144 MiB) | o firmware carregou o kernel **acima de 4 GiB** (`0x14000ba00`) |

O caso de 4 GiB confirma que o buraco entre 3 e 4 GiB fica marcado como
ocupado (ele está coberto pelo bitmap, mas nunca foi "liberado"). O caso de
8 GiB mostra que o kernel não pode supor que está em endereços baixos.

### 5.5 Os testes antigos continuam passando

`teste-panic`, `teste-breakpoint` e `teste-doublefault` foram rodados de
novo, com o mesmo resultado de antes.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do anterior. Os docs 00 a 06 estão
no primeiro commit do repositório: o link de cada arquivo leva ao código
**no fim do doc 06**, que pode já ter o que os capítulos seguintes acrescentam.

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/memoria/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/memoria/mod.rs) | **novo**: `pub mod mapa; pub mod quadros;`, `TAMANHO_QUADRO`, `INICIO_UTILIZAVEL` | — |
| [`src/memoria/mapa.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/memoria/mapa.rs) | **novo**: `Tipo`, `Regiao`, `Mapa`, `init`, `get` | — |
| [`src/memoria/quadros.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/memoria/quadros.rs) | **novo**: `Quadro`, `AlocadorQuadros` (bitmap), `QUADROS`, `init`, `alocar`, `liberar`, `livres` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `mod memoria;`; em `kernel_main`: `memoria::mapa::init(memory_map)`, `imprimir_mapa(mapa, fb)`, `memoria::quadros::init(mapa)`, `imprimir_alocador()`, `testes_de_memoria()` | depois de `testes_de_excecao()` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `fn imprimir_mapa`, `fn imprimir_alocador`, `fn testes_de_memoria` | depois de `testes_de_excecao` |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/Cargo.toml) | `teste-quadros`, `teste-liberacao-dupla`, `listar-regioes` | `[features]` |

---

## Diferença em relação ao livro

O livro recebe o mapa de memória do **bootloader** que ele usa, num formato
próprio desse bootloader. O SiliconOS recebe o mapa diretamente do **UEFI**, e
por isso precisa converter os tipos do UEFI e lidar com a questão da memória
`BootServices` (seção 3.4).

O livro também pode apresentar o alocador com outra estrutura de dados (é
comum começar com um alocador que só avança e nunca libera). O SiliconOS usa o
bitmap desde o início porque ele já permite liberar e é fácil de inspecionar.
Os conceitos (quadros, regiões, proteger o que é do kernel) são os mesmos.

---

## Resumo

- O espaço de endereços físicos **não é só RAM**: há buracos herdados do PC
  antigo, dispositivos (MMIO), firmware e uma janela PCI abaixo de 4 GiB que
  empurra parte da RAM para cima de 4 GiB.
- O kernel copia o mapa do UEFI para um **formato próprio**: 7 tipos,
  ordenado, com vizinhas unidas.
- A pilha atual está em memória `BootServices`, então **por enquanto só as
  regiões `Livre` são usadas**.
- O **alocador de quadros** entrega blocos de 4 KiB usando um **bitmap**
  (1 bit por quadro). O bitmap mora na primeira região livre e se marca
  como ocupado.
- Princípio de segurança: **começar com tudo ocupado** e só liberar o que se
  tem certeza de que é livre. Liberação dupla vira panic imediato.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Uma máquina tem 4 GiB de RAM. Por que o alocador cobre endereços até
   5 GiB?**
   Porque o último GiB abaixo de 4 GiB é reservado para dispositivos PCI, e
   a RAM correspondente é remapeada para depois de 4 GiB.

2. **Por que o alocador ainda não usa a memória `BootServices`, se a
   especificação UEFI permite?**
   Porque a pilha em que o kernel está rodando fica numa região
   `BootServices`. O alocador poderia entregar a própria pilha do kernel
   para outro uso.

3. **Por que o bitmap começa com todos os bits em 1 (ocupado)?**
   Porque marcar um quadro livre como ocupado só desperdiça memória, mas
   marcar um ocupado como livre entrega memória de alguém e causa
   corrupção. Na dúvida, "ocupado".

4. **Quanto ocupa o bitmap de uma máquina com 16 GiB de RAM (supondo que a
   RAM vá até 17 GiB)?**
   17 GiB / 4 KiB = 4 456 448 quadros = 4 456 448 bits = 544 KiB.

5. **O que `trailing_ones()` devolve para `0b1011` e o que isso significa
   no alocador?**
   2 (os bits 0 e 1 são 1; o bit 2 é 0). O quadro do bit 2 dessa palavra é
   o primeiro livre.

6. **Por que usar o endereço físico do bitmap como ponteiro funciona hoje,
   e por que isso vai mudar?**
   Porque o UEFI deixou a paginação em mapeamento identidade (virtual =
   físico). Quando o kernel montar suas próprias tabelas de página
   (capítulo 7), essa equivalência deixa de valer.

</details>
