# 11: Um espaço de endereços por programa

> **Você vai entender:** por que dois programas precisam de **tabelas de
> página diferentes** para poderem usar o mesmo endereço sem se enxergar;
> como criar uma PML4 nova que **compartilha o kernel** com todas as outras
> copiando só algumas entradas de 8 bytes; o que acontece quando o kernel
> escreve no **CR3**; e como o tipo `EspacoEnderecos` devolve sozinho, ao
> ser destruído, cada quadro que o programa usou (inclusive as tabelas que o
> doc 10 deixava para trás).
>
> **Arquivos:** `src/memoria/espaco.rs` (novo: o tipo `EspacoEnderecos`),
> `src/memoria/paginacao.rs` (`raiz_kernel`, `ativar`, erro `AreaDoKernel`),
> `rodar` em `src/usuario/mod.rs`, o comando `espacos` em `src/shell.rs`
> **Livro:** capítulo 11, seção 11.4 (o livro só cita a ideia; a
> implementação é nossa)
> **Testar:** `./run.sh`, depois `espacos`, `mem`, `rodar ola`, `mem`;
> `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 06 (paginação: PML4, `mapear_em`, TLB) e doc 10 (modo
> usuário)
> **Rust novo aqui:** `Drop` ([§38](rust-basico.md#38-drop-código-que-roda-quando-o-valor-morre))
>
> **Código deste capítulo:** [estado no fim do doc 11](https://github.com/moises-dev-git/siliconOS/tree/doc-11) · [o que mudou desde o doc 10](https://github.com/moises-dev-git/siliconOS/compare/doc-10...doc-11) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

No doc 10, o programa morava **nas tabelas de página do kernel**: `rodar`
mapeava o código em `0x0000_2000_0000_0000` e a pilha logo acima, na mesma
PML4 que o kernel usa. Funcionava, mas tinha três defeitos:

1. **Só um programa por vez.** Os dois próximos passos (o escalonador, que
   alterna vários programas, e os executáveis ELF, que escolhem os próprios
   endereços) precisam que **dois programas usem o mesmo endereço virtual**
   ao mesmo tempo. Numa tabela só, o endereço `0x2000_0000_0000` aponta para
   um quadro só.
2. **Um programa veria o outro.** Se o programa A mapeasse algo e o B
   rodasse em seguida (ou ao mesmo tempo), as páginas do A estariam no
   caminho do B. Isolamento entre programas é metade da razão de existir de
   um sistema operacional.
3. **Sobravam tabelas.** `desmapear` não libera tabelas intermediárias
   vazias (doc 06), então a primeira execução deixava para sempre uma PDPT,
   uma PD e uma PT na entrada 64 da PML4 do kernel: os **3 quadros** que o
   teste `nada_fica_mapeado_nem_vaza` precisava de um "aquecimento" para
   não contar.

## 2. A ideia: cada programa com a sua árvore, o kernel em todas

O CR3 diz à CPU onde está a PML4 (doc 06). **Trocar o CR3 troca o mundo
inteiro de uma vez**: cada endereço virtual passa a ser traduzido por outra
árvore. É assim que todo sistema operacional moderno separa processos: cada
um tem a sua PML4, e trocar de processo inclui escrever no CR3. A esse
conjunto (a árvore de tabelas e o que ela mapeia) se dá o nome de **espaço
de endereços**.

Mas há um detalhe: o kernel precisa continuar existindo **dentro** de cada
espaço. Quando o programa faz `int 0x80`, a CPU pula para o tratador do
kernel **sem trocar o CR3**; o código do tratador, a pilha do anel 0 (TSS.RSP0),
a IDT, o heap e o console precisam estar mapeados ali também. E quando o
timer interrompe o programa, a mesma coisa.

Copiar todas as tabelas do kernel para cada programa seria caro e, pior,
elas ficariam desatualizadas (o heap cresce, doc 07). A solução usada por
praticamente todo sistema é **compartilhar**:

```
  PML4 do kernel             PML4 do programa A        PML4 do programa B
  [0]   identidade ───────┬─ [0]   (a mesma) ──────┬── [0]   (a mesma)
  [64]  (vazia)           │  [64]  código/pilha A  │   [64]  código/pilha B
  [256] metade alta ──────┼─ [256] (a mesma) ──────┼── [256] (a mesma)
  [288] heap ─────────────┼─ [288] (a mesma) ──────┼── [288] (a mesma)
  [320] MMIO ─────────────┘  [320] (a mesma) ──────┘   [320] (a mesma)
```

Uma entrada da PML4 tem 8 bytes e aponta para uma PDPT. Se a PML4 do
programa tiver **a mesma entrada** (os mesmos 8 bytes) que a do kernel, as
duas apontam para **a mesma PDPT**, e daí para baixo tudo é igual: as
mesmas PD, as mesmas PT, as mesmas páginas. Copiando 4 entradas (32 bytes),
o programa ganha o kernel inteiro. E se o heap crescer depois, a página nova
entra numa PT que já é compartilhada: todos os espaços a veem na hora.

O programa, por sua vez, só mexe nas entradas que o kernel **não usa**. A
árvore abaixo delas é só dele.

> **E o programa não poderia ler o kernel, já que ele está mapeado ali?**
> Não: as páginas do kernel não têm o bit USUARIO (doc 10). O programa
> `invasor` continua morrendo com page fault, como antes. Estar mapeado não
> é o mesmo que estar acessível.

Quais entradas o kernel usa? Em vez de supor, o comando `espacos` (seção 6)
mede, e no QEMU com 128 MiB a resposta é:

```
entradas da PML4 do kernel (copiadas em todo espaco): [0, 256, 288, 320]
```

| Entrada | Endereços | O que é | Doc |
|---|---|---|---|
| 0 | 0 a 512 GiB | identidade: o código do kernel, a pilha de boot e o framebuffer moram aqui, onde o UEFI os deixou | 06 |
| 256 | `0xFFFF_8000_...` | toda a RAM vista pela metade alta (`fisico_para_virtual`) | 06 |
| 288 | `0xFFFF_9000_...` | o heap | 07 |
| 320 | `0xFFFF_A000_...` | registradores do APIC e do I/O APIC (MMIO) | 08 |

A entrada 0 é a mais curiosa: está na metade **baixa**, que em geral é dos
programas. Ela está ali porque o kernel ainda roda nos endereços onde o UEFI
o carregou (a "identidade como andaime" do doc 06). Por isso a regra do
programa não é "metade baixa", e sim "metade baixa **e** entrada que o
kernel não usa". O código do programa está na entrada 64, longe dela.

## 3. O código: `EspacoEnderecos`

O tipo inteiro guarda um número: o endereço físico da PML4.

```rust
pub struct EspacoEnderecos {
    /// Endereço físico da PML4 deste espaço.
    raiz: u64,
}
```

### 3.1 Criar: uma PML4 nova com o kernel copiado

```rust
pub fn novo() -> Option<EspacoEnderecos> {
    let raiz = nova_tabela()?;
    let kernel = tabela_em(paginacao::raiz_kernel());
    let nova = tabela_em(raiz);
    for i in 0..512 {
        if kernel.entradas[i].presente() {
            nova.entradas[i] = kernel.entradas[i];
        }
    }
    Some(EspacoEnderecos { raiz })
}
```

- `nova_tabela()` (doc 06) pega um quadro e o zera: uma PML4 sem nenhuma
  entrada. O `?` devolve `None` se os quadros acabaram.
- `tabela_em(fisico)` dá acesso a uma tabela pelo endereço físico (pela
  metade alta). Eram privadas de `paginacao.rs`; viraram `pub(super)` para o
  módulo irmão `espaco.rs` poder usá-las, sem ficar visíveis fora de
  `memoria`.
- `nova.entradas[i] = kernel.entradas[i]` copia **a entrada**, 8 bytes, e
  não a tabela para a qual ela aponta. É o compartilhamento da seção 2.

### 3.2 Mapear: só fora da área do kernel

```rust
pub fn mapear(&mut self, virt: u64, quadro: Quadro, bits: u64) -> Result<(), ErroMapeamento> {
    let resultado = if !Self::do_programa(virt) {
        Err(ErroMapeamento::AreaDoKernel)
    } else {
        paginacao::mapear_em(self.raiz, virt, quadro.endereco(), TamanhoPagina::K4, bits)
    };
    if resultado.is_err() {
        quadros::liberar(quadro);
    }
    resultado
}

pub fn do_programa(virt: u64) -> bool {
    let i = paginacao::indices(virt)[0];
    i < 256 && Self::entrada_livre(i)
}

pub fn entrada_livre(i: usize) -> bool {
    !tabela_em(paginacao::raiz_kernel()).entradas[i].presente()
}
```

Três decisões aqui:

1. **A verificação `do_programa`.** Sem ela, mapear uma página de usuário em
   `0x1000` (entrada 0) desceria pelas tabelas de identidade, que são **do
   kernel e de todos os espaços**: o `else if` do doc 10 ligaria o bit
   USUARIO nelas, e o programa ganharia acesso a uma parte do kernel. Por
   isso o erro novo `ErroMapeamento::AreaDoKernel`.
2. **O espaço fica dono do quadro.** `mapear` recebe um `Quadro` (e não um
   endereço) e, a partir daí, o quadro pertence ao espaço: será liberado
   quando o espaço morrer. Se o mapeamento falhar, ninguém mais ficaria com
   ele, então `mapear` o libera na hora. Assim quem chama nunca precisa
   lembrar de liberar nada.
3. **Só páginas de 4 KiB.** Simplifica a destruição (seção 3.4): toda folha
   é um quadro do alocador.

`mapear_em` é a mesma do doc 06, só que com a raiz do espaço em vez da do
kernel: as tabelas intermediárias que faltarem (PDPT, PD, PT da entrada 64)
são criadas **nesta** árvore, com o bit USUARIO.

### 3.3 Ativar: escrever no CR3

Em `paginacao.rs`:

```rust
pub fn ativar(raiz: u64) {
    if raiz_atual() != raiz {
        unsafe { asm!("mov cr3, {}", in(reg) raiz, options(nostack, preserves_flags)) };
    }
}
```

e em `EspacoEnderecos`, `ativar(&self)` só chama `paginacao::ativar(self.raiz)`.

Escrever no CR3 faz duas coisas na mesma instrução: a CPU passa a traduzir
pela PML4 nova **e esvazia a TLB** (o cache de traduções, doc 06), menos as
entradas marcadas GLOBAL. É por isso que mapear páginas num espaço inativo
não precisa de `invlpg`: quando ele for ativado, a TLB começa vazia. O custo
também vem daí: depois de cada troca, as primeiras leituras de cada página
têm que percorrer a árvore de novo. (Sistemas de verdade marcam as páginas
do kernel como GLOBAL e usam PCID, um número de espaço na TLB, para perder
menos. O SiliconOS não usa nenhum dos dois.)

O `if` evita a troca (e o esvaziamento) quando o CR3 já é aquele.

### 3.4 Destruir: `Drop` devolve tudo

Em Rust, um tipo pode dizer o que acontece quando um valor dele **morre**
(sai de escopo, ou é passado para `drop`), implementando o trait `Drop`
([§38](rust-basico.md#38-drop-código-que-roda-quando-o-valor-morre)). É
aqui que o espaço devolve os quadros:

```rust
impl Drop for EspacoEnderecos {
    fn drop(&mut self) {
        assert!(paginacao::raiz_atual() != self.raiz, "destruindo o espaco de enderecos ativo");
        let kernel = tabela_em(paginacao::raiz_kernel());
        let pml4 = tabela_em(self.raiz);
        for i in 0..256 {
            let e = pml4.entradas[i];
            if e.presente() && !kernel.entradas[i].presente() {
                liberar_arvore(e.endereco(), 1);
            }
        }
        quadros::liberar(Quadro(self.raiz));
    }
}

fn liberar_arvore(tabela: u64, nivel: usize) {
    for e in tabela_em(tabela).entradas.iter().filter(|e| e.presente()) {
        assert!(!e.grande(), "pagina grande num espaco de programa");
        if nivel == 3 {
            quadros::liberar(Quadro(e.endereco())); // a página do programa
        } else {
            liberar_arvore(e.endereco(), nivel + 1);
        }
    }
    quadros::liberar(Quadro(tabela));
}
```

- **O `assert`.** Liberar as tabelas que a CPU está usando seria serrar o
  galho em que se está sentado: o quadro da PML4 voltaria ao alocador, outro
  código o reaproveitaria e escreveria nele, e a próxima tradução leria lixo.
  Quem destrói um espaço tem que voltar antes ao CR3 do kernel.
- **Só as entradas do programa.** A condição
  `!kernel.entradas[i].presente()` pula as entradas copiadas do kernel:
  liberar a árvore da entrada 0 devolveria ao alocador as tabelas de
  identidade, em uso por todo mundo.
- **`liberar_arvore` é recursiva.** Nível 1 (PDPT) chama nível 2 (PD), que
  chama nível 3 (PT); na PT, cada entrada presente é uma página do programa.
  Cada tabela é liberada **depois** do que está abaixo dela (a ordem inversa
  seria ler uma tabela já liberada).

```
  PML4 (raiz)                       liberada por último
   └─[64] PDPT                      liberar_arvore(nível 1)
           └─[0] PD                 liberar_arvore(nível 2)
                  └─[0] PT          liberar_arvore(nível 3)
                         ├─[0]   página de código   → liberar
                         └─[256] página de pilha    → liberar
```

Esse é o espaço de um programa do doc 10: código em `0x2000_0000_0000`
(índices 64, 0, 0, 0) e pilha em `0x2000_0010_0000` (64, 0, 0, 256). Os dois
dividem a mesma PT, então o espaço inteiro custa **6 quadros**: PML4, PDPT,
PD, PT e duas páginas.

### 3.5 As tabelas do kernel têm endereço fixo

Com vários espaços, "as tabelas ativas" (o CR3) e "as tabelas do kernel"
deixam de ser a mesma coisa. Se o kernel mapeasse algo para si **durante** um
programa (o heap crescendo dentro de uma chamada de sistema, por exemplo), o
antigo `mapear` (que usava `raiz_atual()`) escreveria na PML4 do programa.
Na maioria dos casos a escrita cairia numa tabela compartilhada e daria
certo por sorte; mas se precisasse de uma entrada nova da PML4, ela só
existiria no espaço daquele programa e sumiria com ele.

Por isso `paginacao::init` agora guarda a PML4 do kernel:

```rust
static RAIZ_KERNEL: AtomicU64 = AtomicU64::new(0);

pub fn raiz_kernel() -> u64 {
    match RAIZ_KERNEL.load(Ordering::Relaxed) {
        0 => raiz_atual(),
        r => r,
    }
}
```

(antes do `init`, o 0 quer dizer "ainda não temos tabelas próprias", e as
ativas são as do UEFI), e `mapear`/`desmapear` passaram a usar
`raiz_kernel()`.

Fica uma limitação, que vale anotar: uma entrada da PML4 do kernel criada
**depois** de um espaço existir não aparece nele (a cópia foi feita na
criação). Hoje isso não acontece: as quatro entradas nascem no boot. Um
kernel maior resolveria criando de antemão todas as 256 entradas da metade
alta (custa 256 quadros, 1 MiB), para que daí em diante só tabelas
compartilhadas mudem.

## 4. `rodar` com um espaço próprio

O `rodar` do doc 10 mudou nos passos 1 (criar o espaço), 3 (ativar e
desativar) e 4 (destruir):

```rust
pub fn rodar(programa: &programas::Programa) -> Resultado {
    let codigo = programa.codigo();
    assert!(codigo.len() <= TAMANHO_QUADRO as usize, "programa maior que uma pagina");

    // 1. Um espaço de endereços só deste programa (doc 11): uma PML4 nova,
    //    com o kernel copiado e a metade do programa vazia.
    let mut espaco = EspacoEnderecos::novo().expect("sem quadros para a PML4 do programa");

    // 2. Uma página de código (só leitura, executável) e uma de pilha
    //    (escrita, sem execução), as duas acessíveis pelo anel 3. O espaço
    //    fica dono dos quadros.
    let q_codigo = nova_pagina_zerada();
    unsafe {
        let destino = paginacao::fisico_para_virtual(q_codigo.endereco()) as *mut u8;
        core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
    }
    espaco.mapear(CODIGO_VIRT, q_codigo, flags::USUARIO).expect("mapear o codigo do programa");
    espaco.mapear(PILHA_VIRT, nova_pagina_zerada(),
                  flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO).expect("mapear a pilha");

    // 3. Ativar o espaço (CR3) e entrar no anel 3. Só volta quando o
    //    programa terminar.
    *RESULTADO.lock() = None;
    INICIO.store(timer::tiques(), Ordering::Relaxed);
    ATIVO.store(true, Ordering::Relaxed);
    espaco.ativar();
    unsafe { entrar_no_anel3(CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO) };
    // De volta: as tabelas ativas ainda são as do programa. Voltar às do
    // kernel ANTES de destruir as do programa.
    paginacao::ativar(paginacao::raiz_kernel());
    ATIVO.store(false, Ordering::Relaxed);
    // Voltamos de dentro de um tratador (interrupções desligadas).
    cpu::ligar_interrupcoes();

    // 4. Limpar: destruir o espaço devolve páginas, tabelas e a PML4.
    drop(espaco);
    RESULTADO.lock().take().expect("programa terminou sem resultado")
}
```

O que reparar:

- **O código do kernel continua rodando depois de `espaco.ativar()`.** A
  instrução seguinte (a chamada de `entrar_no_anel3`) é buscada num endereço
  da entrada 0, que a PML4 do programa também tem. Sem a cópia da seção
  3.1, a CPU daria page fault na instrução logo depois do `mov cr3`, e a
  falha também não teria como ser tratada: a IDT e a pilha estão em
  endereços do kernel.
- **Durante o programa, tudo do kernel roda com o CR3 do programa:** as
  chamadas de sistema (o `escrever` lê a mensagem pelo endereço do programa,
  que só existe nesse espaço), o timer, o teclado, as exceções.
- **Os três caminhos de saída** (sair, exceção, tempo esgotado) chegam ao
  mesmo ponto, depois de `entrar_no_anel3`, por `voltar_ao_kernel` (doc 10).
  Por isso basta uma linha para voltar ao CR3 do kernel.
- **A ordem do fim importa:** `paginacao::ativar(kernel)` antes de
  `drop(espaco)`. Invertendo as duas linhas (testado e desfeito), o
  `cargo test` para no primeiro programa com
  `Local: src/memoria/espaco.rs:94:9` e
  `Mensagem: destruindo o espaco de enderecos ativo`.
- **`desmapear` sumiu de `rodar`.** Não há o que desmapear: a árvore inteira
  é jogada fora.

## 5. Os testes

Quatro testes novos em `espaco.rs` e um antigo simplificado.

**Mesmo endereço, páginas diferentes**: o núcleo do capítulo. Dois espaços
mapeiam `0x2000_0000_0000`, cada um num quadro com um valor diferente; a
tradução de cada um dá o seu quadro, a do kernel não dá nada, e lendo **de
verdade** (com cada espaço ativo) aparecem os dois valores:

```rust
#[test_case]
fn mesmo_endereco_paginas_diferentes() {
    let (qa, qb) = (pagina_com(0xAAAA), pagina_com(0xBBBB));
    let (mut a, mut b) = (EspacoEnderecos::novo().unwrap(), EspacoEnderecos::novo().unwrap());
    a.mapear(V, qa, flags::USUARIO).unwrap();
    b.mapear(V, qb, flags::USUARIO).unwrap();
    assert_eq!(a.traduzir(V).unwrap().fisico, qa.endereco());
    assert_eq!(b.traduzir(V).unwrap().fisico, qb.endereco());
    assert!(paginacao::traduzir_em(paginacao::raiz_kernel(), V).is_none());
    // Lendo de verdade, com cada espaço ativo:
    let ler = || unsafe { (V as *const u64).read_volatile() };
    a.ativar();
    let va = ler();
    b.ativar();
    let vb = ler();
    paginacao::ativar(paginacao::raiz_kernel());
    assert_eq!((va, vb), (0xAAAA, 0xBBBB));
}
```

(`pagina_com(valor)` aloca um quadro e escreve `valor` no começo dele, pela
metade alta. `V` é `0x0000_2000_0000_0000`.) O `assert_eq!` final fica
depois da volta ao CR3 do kernel de propósito: se falhasse com um espaço de
teste ativo, o panic aconteceria fora das tabelas do kernel.

**O kernel aparece igual em todos**: um `Box` (no heap) tem a mesma tradução
num espaço novo e nas tabelas do kernel.

```rust
#[test_case]
fn kernel_aparece_igual_em_todos() {
    let a = EspacoEnderecos::novo().unwrap();
    let caixa = alloc::boxed::Box::new(7u64);
    let endereco = &*caixa as *const u64 as u64;
    assert!(heap::contem(endereco));
    assert_eq!(a.traduzir(endereco).map(|t| t.fisico), paginacao::traduzir(endereco).map(|t| t.fisico));
}
```

**Destruir devolve todos os quadros**, contando um por um:

```rust
#[test_case]
fn destruir_devolve_todos_os_quadros() {
    let livres = quadros::livres();
    let mut a = EspacoEnderecos::novo().unwrap();
    a.mapear(V, pagina_com(1), flags::USUARIO).unwrap();
    a.mapear(V + 0x20_0000, pagina_com(2), flags::USUARIO).unwrap();
    // PML4 + PDPT + PD + 2 PTs (cada PT cobre 2 MiB; as páginas estão a 2 MiB uma da outra) + 2 páginas
    assert_eq!(livres - quadros::livres(), 7);
    drop(a);
    assert_eq!(quadros::livres(), livres);
}
```

> **Um engano que o teste pegou.** A primeira versão punha a segunda página
> em `V + 0x10_0000` (1 MiB adiante) e esperava 7 quadros. Deu 6. Uma PT tem
> 512 entradas de 4 KiB = **2 MiB**, então 1 MiB adiante ainda é a mesma PT.
> Com 2 MiB de distância, cada página precisa da sua PT, e a conta fecha.

**A área do kernel é recusada**, sem vazar o quadro recusado:

```rust
#[test_case]
fn area_do_kernel_recusada() {
    let livres = quadros::livres();
    let mut a = EspacoEnderecos::novo().unwrap();
    let heap = 0xFFFF_9000_0000_0000;
    assert_eq!(a.mapear(heap, pagina_com(0), flags::USUARIO), Err(ErroMapeamento::AreaDoKernel));
    assert_eq!(a.mapear(0x1000, pagina_com(0), flags::USUARIO), Err(ErroMapeamento::AreaDoKernel));
    drop(a);
    assert_eq!(quadros::livres(), livres);
}
```

E o teste do doc 10 que precisava de aquecimento agora não precisa:

```rust
#[test_case]
fn nada_fica_mapeado_nem_vaza() {
    // Desde o doc 11 não precisa de aquecimento: as tabelas do programa
    // morrem com o espaço dele, então a primeira execução também não
    // deixa nada para trás.
    let livres = quadros::livres();
    rodar_por_nome("ola");
    assert!(paginacao::traduzir(CODIGO_VIRT).is_none());
    assert!(paginacao::traduzir(PILHA_VIRT).is_none());
    assert_eq!(paginacao::raiz_atual(), paginacao::raiz_kernel());
    assert_eq!(quadros::livres(), livres);
}
```

## 6. Ver com os próprios olhos: o comando `espacos`

O comando cria **dois espaços vivos ao mesmo tempo**, copia o código de
`ola` para um e o de `relogio` para o outro, os dois em `CODIGO_VIRT`, e lê
os primeiros 8 bytes desse endereço com cada espaço ativo:

```rust
fn espacos(_: &[&str]) {
    use crate::memoria::espaco::EspacoEnderecos;
    use crate::memoria::paginacao::{self, flags};
    use crate::memoria::quadros;
    use crate::usuario::{programas, CODIGO_VIRT};

    let kernel = paginacao::raiz_kernel();
    kprintln!("kernel:   PML4 em {:#x}; {:#x} -> {}", kernel, CODIGO_VIRT,
              if paginacao::traduzir_em(kernel, CODIGO_VIRT).is_some() { "mapeado" } else { "nao mapeado" });
    let usadas: Vec<usize> = (0..512).filter(|&i| !EspacoEnderecos::entrada_livre(i)).collect();
    kprintln!("entradas da PML4 do kernel (copiadas em todo espaco): {:?}", usadas);
    let mut criados: Vec<(&str, EspacoEnderecos)> = Vec::new();
    for nome in ["ola", "relogio"] {
        let codigo = programas::procurar(nome).unwrap().codigo();
        let Some(q) = quadros::alocar() else { kprintln!("sem memoria"); return };
        unsafe {
            let destino = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
            core::ptr::write_bytes(destino, 0, 4096);
            core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
        }
        let Some(mut espaco) = EspacoEnderecos::novo() else { quadros::liberar(q); kprintln!("sem memoria"); return };
        if espaco.mapear(CODIGO_VIRT, q, flags::USUARIO).is_err() { kprintln!("sem memoria"); return }
        criados.push((nome, espaco));
    }
    for (nome, espaco) in &criados {
        // Ler os primeiros bytes com este espaço ativo (o kernel continua
        // visível nele, então este código segue rodando normalmente).
        espaco.ativar();
        let bytes = unsafe { (CODIGO_VIRT as *const [u8; 8]).read_volatile() };
        paginacao::ativar(kernel);
        kprintln!("{:<9} PML4 em {:#x}; {:#x} -> fisico {:#x}; bytes {:02x?}", nome, espaco.raiz(),
                  CODIGO_VIRT, espaco.traduzir(CODIGO_VIRT).unwrap().fisico, bytes);
    }
} // `criados` é destruído aqui: PML4, tabelas e páginas voltam ao alocador
```

O kernel pode ler `CODIGO_VIRT` direto (no anel 0) mesmo sendo uma página de
usuário porque o SMAP está desligado (doc 10). E não há `drop` explícito:
quando a função termina, o `Vec` `criados` morre, e com ele cada
`EspacoEnderecos`, cujo `Drop` devolve os quadros.

## 7. Como foi testado

### 7.1 Pelo shell

Saída real (serial), QEMU com 128 MiB:

```
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> espacos
kernel:   PML4 em 0x101000; 0x200000000000 -> nao mapeado
entradas da PML4 do kernel (copiadas em todo espaco): [0, 256, 288, 320]
ola       PML4 em 0x20f000; 0x200000000000 -> fisico 0x20e000; bytes [48, 8d, 3d, 15, 00, 00, 00, be]
relogio   PML4 em 0x214000; 0x200000000000 -> fisico 0x213000; bytes [b8, 02, 00, 00, 00, cd, 80, 48]
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> rodar ola
Ola do anel 3! (escrito por uma chamada de sistema)
'ola' terminou: codigo de saida 42
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> rodar invasor
[kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
'invasor' encerrado pelo kernel: PAGE FAULT em 0x20000000000a
silicon> rodar laco
'laco' encerrado pelo kernel: passou de 2 s sem terminar
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> traduzir 0x200000000000
  endereco 0x0000200000000000 [ 64   0   0   0] -> NAO MAPEADO
```

Lendo:

- **O mesmo endereço, dois conteúdos.** Os bytes de `ola` começam com
  `48 8d 3d` (`lea rdi, [rip + ...]`, o ponteiro da mensagem) e os de
  `relogio` com `b8 02 00 00 00` (`mov eax, 2`, a syscall `tiques`); são as
  primeiras instruções de cada programa (doc 10, seção 4.2).
- **Cada espaço custa 5 quadros** aqui (PML4, PDPT, PD, PT e a página de
  código): as PML4 estão em `0x20f000` e `0x214000`, a `0x5000` uma da
  outra, porque o alocador entregou os quadros em sequência. Em `rodar`,
  com a pilha, são 6.
- **20307 quadros livres o tempo todo**, inclusive depois do **primeiro**
  `rodar`. No doc 10, a primeira execução levava o número de 20308 para
  20305 (as 3 tabelas que ficavam na PML4 do kernel). Os programas que
  terminam por exceção (`invasor`) e pelo timer (`laco`) também não deixam
  nada.
- **A PML4 do kernel nunca vê o programa:** `traduzir` (que usa as tabelas
  ativas, as do kernel quando o shell está rodando) diz NAO MAPEADO.

### 7.2 `cargo test`

36 testes, todos `[ok]`: os 32 de antes mais os 4 de `espaco.rs`. Trecho:

```
siliconos::memoria::espaco::testes::area_do_kernel_recusada ... [ok]
siliconos::memoria::espaco::testes::destruir_devolve_todos_os_quadros ... [ok]
siliconos::memoria::espaco::testes::kernel_aparece_igual_em_todos ... [ok]
siliconos::memoria::espaco::testes::mesmo_endereco_paginas_diferentes ... [ok]
...
siliconos::usuario::testes::nada_fica_mapeado_nem_vaza ... Ola do anel 3! (escrito por uma chamada de sistema)
[ok]
```

### 7.3 Regressão

As 21 builds com features de teste (`teste-panic` a `teste-timer`) continuam
passando.

## O que ainda falta

- **Ainda um programa de cada vez.** Agora é possível ter vários espaços
  vivos (o comando `espacos` prova), mas o `rodar` ainda espera o programa
  terminar. O próximo passo, o **escalonador**, guarda o estado de cada
  programa e alterna entre eles no timer, trocando o CR3 a cada troca.
  *(Feito no [doc 12](12-escalonador.md).)*
- **Sem GLOBAL nem PCID:** cada troca de CR3 esvazia a TLB inteira,
  inclusive as traduções do kernel.
- **Entradas novas do kernel não se propagam** (seção 3.5).

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 10 (tag `doc-10`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-11`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-10...doc-11).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/memoria/espaco.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | **novo**: `EspacoEnderecos` (`novo`, `raiz`, `mapear`, `traduzir`, `ativar`, `do_programa`, `entrada_livre`), `impl Drop`, `liberar_arvore`, 4 testes | — |
| [`src/memoria/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/mod.rs) | `pub mod espaco;` | depois de `pub mod paginacao;` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `tabela_em` e `nova_tabela` viram `pub(super)` | as duas funções |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | variante `AreaDoKernel` | fim de `enum ErroMapeamento` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `mapear`/`desmapear` usam `raiz_kernel()`; novos `RAIZ_KERNEL`, `raiz_kernel`, `ativar` | logo depois de `desmapear_em` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `RAIZ_KERNEL.store(pml4, ...)` | em `init`, logo depois do `mov cr3` |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/usuario/mod.rs) | `use crate::memoria::espaco::EspacoEnderecos;`; `rodar` reescrito (seção 4); `nada_fica_mapeado_nem_vaza` sem aquecimento; diagrama do topo do arquivo atualizado | — |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/shell.rs) | comando `espacos` (linha em `COMANDOS` e a função) | linha antes de `rodar`; função antes de `rodar_programa` |

### Os trechos que o texto não mostrou

A variante nova do erro, em `paginacao.rs`:

```rust
    /// O alocador de quadros não tem quadro para uma tabela nova.
    SemMemoria,
    /// O espaço de um programa não pode mapear onde o kernel mora (doc 11).
    AreaDoKernel,
}
```

Os comentários de `mapear` e `desmapear`, e onde entram as funções novas:

```rust
/// Mapeia nas tabelas do KERNEL (não nas do programa que estiver ativo:
/// desde o doc 11, cada programa tem a sua PML4, e o CR3 pode apontar para ela).
pub fn mapear(virt: u64, fisico: u64, tamanho: TamanhoPagina, bits: u64) -> Result<(), ErroMapeamento> {
    mapear_em(raiz_kernel(), virt, fisico, tamanho, bits)
}

pub fn desmapear(virt: u64) -> Result<u64, ErroMapeamento> {
    desmapear_em(raiz_kernel(), virt)
}

/// Endereço físico da PML4 do kernel (montada por `init`). Antes do `init`,
/// as tabelas ativas são as do UEFI.
static RAIZ_KERNEL: AtomicU64 = AtomicU64::new(0);

pub fn raiz_kernel() -> u64 { ... }   // seção 3.5

pub fn ativar(raiz: u64) { ... }      // seção 3.3
```

O fim de `init`:

```rust
    unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };
    RAIZ_KERNEL.store(pml4, Ordering::Relaxed);
```

Os `use` e os métodos simples de `espaco.rs`:

```rust
use super::paginacao::{self, tabela_em, nova_tabela, ErroMapeamento, TamanhoPagina, Traducao};
use super::quadros::{self, Quadro};

impl EspacoEnderecos {
    // novo (seção 3.1)

    pub fn raiz(&self) -> u64 {
        self.raiz
    }

    // mapear (seção 3.2)

    pub fn traduzir(&self, virt: u64) -> Option<Traducao> {
        paginacao::traduzir_em(self.raiz, virt)
    }

    pub fn ativar(&self) {
        paginacao::ativar(self.raiz);
    }

    // do_programa, entrada_livre (seção 3.2)
}
```

O começo do módulo de testes de `espaco.rs`:

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::memoria::heap;
    use crate::memoria::paginacao::flags;

    const V: u64 = 0x0000_2000_0000_0000;

    fn pagina_com(valor: u64) -> Quadro {
        let q = quadros::alocar().unwrap();
        unsafe { (paginacao::fisico_para_virtual(q.endereco()) as *mut u64).write_volatile(valor) };
        q
    }

    // os 4 testes da seção 5
}
```

A linha do comando na tabela do shell:

```rust
    Comando { nome: "espacos", uso: "espacos", descricao: "dois espacos de enderecos, mesmo endereco, bytes diferentes", funcao: espacos },
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `EspacoEnderecos::novo` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Cria uma PML4 zerada e copia para ela as entradas presentes da PML4 do kernel (compartilhando as tabelas abaixo delas). |
| `EspacoEnderecos::raiz` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Endereço físico da PML4 do espaço. |
| `EspacoEnderecos::mapear` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Mapeia uma página de 4 KiB e fica dono do quadro; recusa endereços da área do kernel (`AreaDoKernel`) e, em caso de erro, libera o quadro. |
| `EspacoEnderecos::traduzir` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | `traduzir_em` com a PML4 deste espaço. |
| `EspacoEnderecos::ativar` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Passa a usar este espaço (escreve a raiz no CR3). |
| `EspacoEnderecos::do_programa` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | O endereço está na metade baixa, numa entrada da PML4 que o kernel não usa? |
| `EspacoEnderecos::entrada_livre` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | A entrada `i` da PML4 do kernel está vazia? |
| `EspacoEnderecos::drop` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | `Drop`: confere que o espaço não está ativo e libera as árvores das entradas do programa e a PML4. |
| `liberar_arvore` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Libera recursivamente uma tabela, as tabelas abaixo dela e as páginas apontadas pela PT. |
| `raiz_kernel` | [memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | Endereço físico da PML4 do kernel (as tabelas ativas, antes de `init`). |
| `ativar` | [memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | Escreve uma PML4 no CR3 (se já não for a ativa); a CPU esvazia a TLB. |
| `espacos` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/shell.rs) | Comando `espacos`: dois espaços vivos com `ola` e `relogio` no mesmo endereço, mostrando PML4, físico e os primeiros bytes de cada. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `pagina_com` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Ajudante dos testes: aloca um quadro e escreve um valor no começo dele. |
| `mesmo_endereco_paginas_diferentes` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Teste: dois espaços com quadros diferentes no mesmo endereço; traduções e leituras reais diferentes, nada nas tabelas do kernel. |
| `kernel_aparece_igual_em_todos` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Teste: um endereço do heap traduz igual num espaço novo e nas tabelas do kernel. |
| `destruir_devolve_todos_os_quadros` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Teste: duas páginas a 2 MiB uma da outra custam 7 quadros, e o `drop` devolve todos. |
| `area_do_kernel_recusada` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Teste: mapear no heap ou na entrada 0 dá `AreaDoKernel`, sem vazar quadros. |

---

## Diferença em relação ao livro

O livro menciona o espaço de endereços por processo só como parte do roteiro
da seção 11.4, sem código. Tudo aqui é nosso: a cópia das entradas presentes
(em vez de "a metade alta", porque o kernel do SiliconOS ainda usa a entrada
0), a recusa da área do kernel, o `Drop` que libera a árvore e a mudança de
`mapear` para a raiz fixa do kernel.

## Resumo

- Um **espaço de endereços** é uma árvore de tabelas de página com raiz
  própria (PML4). Trocar o **CR3** troca o espaço inteiro e esvazia a TLB.
- Cada `EspacoEnderecos` **copia as entradas da PML4 do kernel** (0, 256,
  288, 320): 8 bytes cada, que apontam para as mesmas tabelas, então o kernel
  aparece igual (e atualizado) em todos os espaços, protegido pela falta do
  bit USUARIO.
- O programa só mapeia em entradas que o kernel não usa; tentar outra dá
  `AreaDoKernel`.
- O espaço é **dono** dos quadros que mapeia. O `Drop` libera páginas,
  tabelas e a PML4, mas nunca as tabelas do kernel nem o espaço ativo.
- `rodar` ativa o espaço antes do `iretq` e volta ao CR3 do kernel antes de
  destruí-lo. O vazamento de 3 quadros do doc 10 acabou.
- `mapear`/`desmapear` do kernel usam a raiz fixa (`raiz_kernel`), não a
  ativa.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que o kernel continua funcionando depois de escrever a PML4 do
   programa no CR3?**
   Porque a PML4 do programa tem cópias das entradas do kernel (0, 256, 288,
   320). A próxima instrução, a pilha, a IDT e o heap são traduzidos pelas
   mesmas tabelas de antes.

2. **Se o programa tem o kernel mapeado no espaço dele, por que `invasor`
   não consegue ler a memória do kernel?**
   As páginas do kernel não têm o bit USUARIO. No anel 3, a CPU recusa o
   acesso com page fault ("pagina presente, sem permissao, modo usuario").

3. **Por que o `Drop` não libera a árvore da entrada 0, se ela está na
   metade baixa?**
   Porque ela foi copiada do kernel: são as tabelas de identidade, usadas
   por todos os espaços. O `Drop` só libera entradas que a PML4 do kernel
   não tem.

4. **O que aconteceria se `rodar` chamasse `drop(espaco)` antes de voltar
   ao CR3 do kernel?**
   O `assert` do `Drop` dispararia ("destruindo o espaco de enderecos
   ativo"). Sem ele, a PML4 em uso voltaria ao alocador; quando outro
   código reaproveitasse o quadro, as traduções da CPU passariam a ler lixo.

5. **Por que mapear uma página num espaço inativo não precisa de `invlpg`?**
   A TLB só guarda traduções do espaço ativo. Quando o espaço for ativado,
   a escrita no CR3 esvazia a TLB, e a tradução nova é lida das tabelas.

6. **Por que duas páginas a 1 MiB de distância custam uma PT a menos que
   duas a 2 MiB?**
   Uma PT tem 512 entradas de 4 KiB e cobre 2 MiB. A 1 MiB de distância, as
   duas páginas caem na mesma PT; a 2 MiB, cada uma precisa da sua.

</details>
