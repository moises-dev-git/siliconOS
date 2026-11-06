# 07: Heap: `Box`, `Vec` e `String` dentro do kernel

> **Você vai entender:** por que um kernel `no_std` não tem `Vec` nem
> `String`; o que é um heap e como ele se apoia no alocador de quadros e na
> paginação; como funcionam dois alocadores clássicos (o *bump* e a *lista
> de blocos livres*); o que é fragmentação; e como o Rust conecta tudo isso
> a um simples `Box::new`.
>
> **Arquivos:** `src/memoria/heap.rs` (o heap de verdade), `src/memoria/bump.rs`
> (o alocador bump, para comparação), `demonstrar_heap` e `testes_de_heap` em `src/main.rs`
> **Livro:** capítulo 8
> **Testar:** `./run.sh teste-heap`, `./run.sh teste-heap-liberacao-dupla`,
> `./run.sh teste-heap-esgotar`, `./run.sh teste-heap-executar`, `./run.sh teste-bump`
> **Leia antes:** doc 05 (quadros) e doc 06 (paginação)
> **Rust novo aqui:** `alloc`, `Box`, `Vec`, `String` ([§26](rust-basico.md#26-alloc-box-vec-e-string)),
> `GlobalAlloc`, `Layout` e `#[global_allocator]` ([§27](rust-basico.md#27-globalalloc-layout-e-global_allocator))
>
> **Código deste capítulo:** [estado no fim do doc 07](https://github.com/moises-dev-git/siliconOS/tree/doc-07) · [o que mudou desde o doc 06](https://github.com/moises-dev-git/siliconOS/compare/doc-06...doc-07) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

Até aqui, toda estrutura de dados do kernel tinha **tamanho fixo, decidido
na compilação**:

- o mapa de memória é um array de 512 regiões (doc 05), mesmo quando o
  firmware entrega 130;
- a GDT tem 7 linhas, a IDT tem 256;
- as tabelas de página são quadros pedidos um a um ao alocador.

Isso funciona para tabelas da CPU, mas não para o que vem pela frente: uma
lista de processos que cresce e diminui, o buffer de uma linha digitada no
shell, um nome de arquivo. Em Java ou Python você nem pensa nisso: escreve
`new ArrayList<>()` ou `[]` e a lista cresce sozinha. Em Rust, isso é
`Vec`, `String` e `Box`, e **eles não existem num kernel `no_std`**
(doc 00, seção 4.1).

Por quê? Porque todos eles precisam de alguém que responda à pergunta
**"me dê N bytes"** e, depois, **"pode pegar estes bytes de volta"**. Num
programa comum, quem responde é a biblioteca padrão, que pede memória ao
sistema operacional. Aqui **nós somos o sistema operacional**.

Já temos um alocador, o de quadros (doc 05), mas ele só entrega blocos de
**4 KiB**. Um `Box<u64>` precisa de 8 bytes; uma `String` de 20 bytes
precisa de 20. Usar um quadro inteiro para cada um desperdiçaria 99,8% da
memória. Falta uma segunda camada, que corta os quadros em pedaços do
tamanho pedido: o **heap**.

```
  Box::new, Vec::push, String::from     ← o código do kernel
              │  "me dê 24 bytes alinhados em 8"
              ▼
  heap (este doc)                       ← corta páginas em pedaços de qualquer tamanho
              │  "preciso de mais páginas"
              ▼
  paginação (doc 06)                    ← faz as páginas virtuais apontarem para quadros
              │  "me dê um quadro"
              ▼
  alocador de quadros (doc 05)          ← entrega blocos de 4 KiB de RAM física
```

---

## 2. Onde mora o heap

O heap é uma **faixa de endereços virtuais** reservada só para ele. O
SiliconOS escolheu começar em `0xFFFF_9000_0000_0000`:

```
 0xFFFF_9000_0400_0000  ┐  ← limite: TAMANHO_MAXIMO = 64 MiB
                        │     (não mapeado até ser necessário)
 0xFFFF_9000_0010_0000  ┤  ← fim do heap inicial (1 MiB)
                        │  heap inicial, mapeado no boot
 0xFFFF_9000_0000_0000  ┘  ← HEAP_INICIO  (entrada 288 da PML4)
        ...
 0xFFFF_8000_0000_0000  ←  mapeamento direto da RAM (doc 06; entradas 256 e 257)
```

```rust
pub const HEAP_INICIO: u64 = 0xFFFF_9000_0000_0000;
pub const TAMANHO_INICIAL: u64 = 1024 * 1024;        // 1 MiB
pub const CRESCIMENTO_MINIMO: u64 = 64 * 1024;       // 64 KiB
pub const TAMANHO_MAXIMO: u64 = 64 * 1024 * 1024;    // 64 MiB
```

Por que uma faixa virtual própria, e não pedaços espalhados da RAM?

- **Contínuo:** para o heap, a faixa é um bloco único de endereços, mesmo
  que os quadros físicos por trás estejam espalhados (é a vantagem da
  paginação, doc 06, seção 1).
- **Cresce no lugar:** quando falta espaço, basta mapear mais páginas logo
  depois do fim. Os blocos já entregues não se mexem.
- **Fácil de reconhecer:** um endereço `0xFFFF_9...` num relatório de erro
  é, quase certamente, memória do heap.

### O heap não executa código: NX

As páginas do heap são mapeadas com dois bits:

```rust
const BITS_HEAP: u64 = flags::ESCRITA | flags::SEM_EXECUCAO;
```

`SEM_EXECUCAO` é o bit **NX** (doc 06, seção 3), usado aqui pela primeira
vez. O heap guarda **dados**, nunca código. Se um bug (ou um ataque) fizer
a CPU saltar para dentro do heap, ela recusa buscar a instrução e gera um
page fault. É uma das proteções mais importantes de um sistema moderno:
mesmo que alguém consiga gravar bytes maliciosos na memória, eles não
rodam. A seção 7.5 mostra o teste.

### Mapeando a faixa

```rust
pub(super) fn mapear_faixa(inicio: u64, quantidade: u64) -> u64 {
    let mut feito = 0;
    while feito < quantidade {
        match paginacao::alocar_e_mapear(inicio + feito, BITS_HEAP) {
            Ok(_) => feito += TAMANHO_QUADRO,
            Err(ErroMapeamento::SemMemoria) => break,
            Err(e) => panic!("heap: erro ao mapear {:#x}: {:?}", inicio + feito, e),
        }
    }
    feito
}
```

Página por página: pede um quadro ao alocador do doc 05 e mapeia a página
virtual nele (doc 06). Se os quadros acabarem no meio, devolve quanto
conseguiu; qualquer outro erro (já mapeado, não alinhado) é um bug e vira
panic.

O heap usa páginas de **4 KiB**, e não de 2 MiB como o mapeamento da RAM,
para crescer em passos pequenos sem prender memória física à toa.

---

## 3. O alocador mais simples: *bump*

Antes do alocador de verdade, vale ver o mais simples possível, porque ele
mostra o que é difícil no problema. O **bump** (de "empurrar") guarda um
único ponteiro, `proximo`:

```
 heap:  [ usado | usado | usado |          livre           ]
                                ▲
                             proximo
 alocar(24):  entrega `proximo` e empurra 24 bytes para a frente
 liberar(x):  não faz nada
```

```rust
unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    let mut b = BUMP.lock();
    let inicio = alinhar_acima(b.proximo, layout.align());
    let fim = match inicio.checked_add(layout.size()) {
        Some(f) => f,
        None => return ptr::null_mut(),
    };
    if b.proximo == 0 || fim > b.fim {
        return ptr::null_mut();     // não inicializado, ou acabou
    }
    b.proximo = fim;
    b.alocacoes += 1;
    inicio as *mut u8
}

unsafe fn dealloc(&self, _ponteiro: *mut u8, _layout: Layout) {
    // Não há como devolver: o bump só sabe andar para a frente.
    BUMP.lock().liberacoes += 1;
}
```

Duas operações de inteiros por alocação: é o alocador mais rápido que
existe. O problema está em `dealloc`: **a memória liberada nunca volta a
ser usada**. O teste `./run.sh teste-bump` troca o alocador do kernel pelo
bump e cria e destrói uma caixa de 1 KiB por vez. Em nenhum momento existe
mais de uma caixa viva:

```
Teste bump: cria e destroi uma caixa de 1 KiB por vez.
  So uma caixa existe a cada momento, mas a memoria nunca volta:
   250 caixas:  250 KiB consumidos (254 alocacoes, 254 liberacoes ignoradas)
   500 caixas:  500 KiB consumidos (504 alocacoes, 504 liberacoes ignoradas)
   750 caixas:  750 KiB consumidos (754 alocacoes, 754 liberacoes ignoradas)
  1000 caixas: 1000 KiB consumidos (1004 alocacoes, 1004 liberacoes ignoradas)
!!! KERNEL PANIC !!!
Mensagem: memory allocation of 1024 bytes failed
```

(As 4 alocações a mais são da demonstração do boot, seção 7.1.)

O kernel precisava de 1 KiB e esgotou 1 MiB. Um kernel roda por dias; um
alocador que não reaproveita memória é inaceitável.

O bump ainda é útil em casos especiais: quando tudo é liberado **de uma
vez** (uma região temporária que se descarta inteira), ou quando o
programa só aloca e nunca libera (como no começo do boot). Uma variante
comum zera o ponteiro quando o contador de alocações vivas chega a zero.

---

## 4. O alocador de verdade: lista de blocos livres

### 4.1 A ideia

Para reaproveitar memória, o alocador precisa **lembrar onde estão os
buracos**. O truque clássico é guardar essa informação **dentro dos
próprios buracos**: cada bloco livre começa com um pequeno cabeçalho que
diz o tamanho dele e onde fica o próximo bloco livre.

```rust
struct NoLivre {
    tamanho: usize,        // tamanho do bloco livre, incluindo este cabeçalho
    proximo: *mut NoLivre, // próximo bloco livre (endereço maior), ou nulo
}
```

```
 heap:  [usado][NoLivre 48 →][usado][usado][NoLivre 32 →][usado][NoLivre 900 KiB → nulo]
         ▲                    ▲                          ▲
       livres ───────────────►┘─────────────────────────►┘ (uma lista encadeada)
```

A lista **não gasta memória extra**: os nós moram em memória que está
livre de qualquer jeito. Quando um bloco é entregue, o cabeçalho dele é
sobrescrito pelos dados de quem pediu; quando o bloco volta, o cabeçalho é
gravado de novo.

Duas decisões do SiliconOS tornam o código mais simples:

1. **A lista fica ordenada por endereço.** Assim, ao liberar um bloco, os
   vizinhos dele na lista são também os vizinhos na memória, e dá para
   **juntá-los** (seção 4.4).
2. **Todo bloco é múltiplo de 16 bytes e começa num múltiplo de 16.** Um
   `NoLivre` tem 16 bytes (dois `usize`), então qualquer sobra de um bloco
   é 0 ou cabe um `NoLivre`:

```rust
const BLOCO_MINIMO: usize = core::mem::size_of::<NoLivre>(); // 16
const ALINHAMENTO_MINIMO: usize = 16;
const _: () = assert!(BLOCO_MINIMO == ALINHAMENTO_MINIMO);

fn tamanho_real(layout: Layout) -> usize {
    alinhar_acima(layout.size(), ALINHAMENTO_MINIMO).max(BLOCO_MINIMO)
}
```

Um `Box<u8>` (1 byte) recebe 16; um pedido de 20 recebe 32. O
desperdício máximo é de 15 bytes por bloco, em troca de um código sem
casos especiais.

### 4.2 Arredondar para um múltiplo

```rust
pub const fn alinhar_acima(x: usize, alinhamento: usize) -> usize {
    (x + alinhamento - 1) & !(alinhamento - 1)
}
```

É o mesmo truque do doc 06, seção 5.4: somar `alinhamento - 1` e zerar os
bits de baixo. Só funciona com potências de 2, e o Rust garante que todo
alinhamento é potência de 2.

```
alinhar_acima(20, 16):  20 + 15 = 35 = 0b10_0011
                        & !15        = 0b10_0000 = 32
```

### 4.3 Alocar: o primeiro bloco que serve (*first fit*)

`procurar` percorre a lista e usa o **primeiro** bloco em que o pedido
cabe. Dentro dele, o pedido pode precisar começar um pouco depois do
início, por causa do alinhamento. O que sobra antes e depois continua
livre:

```
 bloco livre:      [inicio ......................................... fim)
 depois:           [sobra antes][   bloco entregue   ][   sobra depois   )
                   ▲            ▲                     ▲
                 inicio       bloco              bloco + tamanho
```

```rust
unsafe fn procurar(&mut self, tamanho: usize, alinhamento: usize) -> Option<usize> {
    let mut elo: *mut *mut NoLivre = &raw mut self.livres;
    unsafe {
        while !(*elo).is_null() {
            let no = *elo;
            let inicio = no as usize;
            let fim = inicio + (*no).tamanho;

            let mut bloco = alinhar_acima(inicio, alinhamento);
            if bloco > inicio && bloco - inicio < BLOCO_MINIMO {
                bloco = alinhar_acima(inicio + BLOCO_MINIMO, alinhamento);
            }

            let cabe = match bloco.checked_add(tamanho) {
                Some(fim_bloco) => fim_bloco <= fim,
                None => false,
            };
            if cabe {
                let fim_bloco = bloco + tamanho;
                let proximo = (*no).proximo;

                let mut seguinte = proximo;
                if fim > fim_bloco {                       // sobra DEPOIS: nó novo
                    let sobra = fim_bloco as *mut NoLivre;
                    (*sobra).tamanho = fim - fim_bloco;
                    (*sobra).proximo = proximo;
                    seguinte = sobra;
                }
                if bloco > inicio {                        // sobra ANTES: o nó encolhe
                    (*no).tamanho = bloco - inicio;
                    (*no).proximo = seguinte;
                } else {                                   // sem sobra antes: o nó sai
                    *elo = seguinte;
                }
                return Some(bloco);
            }
            elo = &raw mut (*no).proximo;
        }
    }
    None
}
```

O detalhe mais elegante é o **`elo`**, um ponteiro para ponteiro
(`*mut *mut NoLivre`). Ele não aponta para um nó; aponta para **o campo
que aponta para o nó**: no começo, `self.livres`; depois, o `proximo` do nó
anterior. Assim, tirar o nó atual da lista é sempre a mesma linha
(`*elo = seguinte`), seja ele o primeiro ou o do meio. Sem isso, a cabeça
da lista seria um caso especial.

`&raw mut x` pega o endereço de `x` como ponteiro cru, sem criar uma
referência Rust (o mesmo que `&raw const` no doc 03, seção 5.3, mas com
permissão de escrita).

A regra "se a sobra da frente for menor que 16, empurre o bloco para a
frente" existe porque um pedaço de 1 a 15 bytes não comporta um `NoLivre`
e ficaria perdido. Como todo bloco começa num múltiplo de 16, isso só
acontece com alinhamentos maiores que 16 (por exemplo, 4096).

### 4.4 Liberar: inserir na ordem e juntar vizinhos

```rust
unsafe fn inserir(&mut self, inicio: usize, tamanho: usize) {
    unsafe {
        let mut anterior: *mut NoLivre = ptr::null_mut();
        let mut atual = self.livres;
        while !atual.is_null() && (atual as usize) < inicio {
            anterior = atual;
            atual = (*atual).proximo;
        }

        if !atual.is_null() {
            assert!(inicio + tamanho <= atual as usize,
                    "heap: liberar {:#x}: bloco ja estava livre (liberacao dupla)", inicio);
        }
        if !anterior.is_null() {
            assert!(anterior as usize + (*anterior).tamanho <= inicio,
                    "heap: liberar {:#x}: bloco ja estava livre (liberacao dupla)", inicio);
        }

        let no = inicio as *mut NoLivre;
        (*no).tamanho = tamanho;
        (*no).proximo = atual;

        if !atual.is_null() && inicio + tamanho == atual as usize {   // junta com o de cima
            (*no).tamanho += (*atual).tamanho;
            (*no).proximo = (*atual).proximo;
        }

        if anterior.is_null() {
            self.livres = no;
        } else if anterior as usize + (*anterior).tamanho == inicio { // junta com o de baixo
            (*anterior).tamanho += (*no).tamanho;
            (*anterior).proximo = (*no).proximo;
        } else {
            (*anterior).proximo = no;
        }
    }
}
```

Os quatro casos possíveis ao devolver o bloco `B`:

```
 1. sem vizinhos livres:     [usado][ B ][usado]   → B vira um nó novo
 2. livre em cima:           [usado][ B ][livre]   → B absorve o de cima
 3. livre embaixo:           [livre][ B ][usado]   → o de baixo absorve B
 4. livres dos dois lados:   [livre][ B ][livre]   → os três viram um só
```

Juntar vizinhos (*coalescing*) é o que impede o heap de virar pó: sem
isso, depois de muitas alocações pequenas, o heap seria uma lista de
milhares de buracos de 16 bytes, e um pedido de 1 KiB não caberia em
nenhum, mesmo com memória livre de sobra. Isso se chama **fragmentação**.

Os dois `assert!` pegam a **liberação dupla**: se o bloco devolvido se
sobrepõe a um bloco que já está na lista, alguém está liberando memória
que já estava livre. É o mesmo princípio do alocador de quadros (doc 05,
seção 4.8): detectar o erro na hora, com uma mensagem clara, em vez de
deixar dois donos escreverem no mesmo lugar.

### 4.5 Crescer sob demanda

```rust
unsafe fn alocar(&mut self, layout: Layout) -> *mut u8 {
    let tamanho = tamanho_real(layout);
    let alinhamento = layout.align().max(ALINHAMENTO_MINIMO);
    loop {
        if let Some(bloco) = unsafe { self.procurar(tamanho, alinhamento) } {
            self.em_uso += tamanho;
            self.pico = self.pico.max(self.em_uso);
            self.alocacoes += 1;
            return bloco as *mut u8;
        }
        let falta = (tamanho + alinhamento).saturating_sub(self.livre_no_fim());
        if !unsafe { self.crescer(falta) } {
            return ptr::null_mut();
        }
    }
}
```

Se nenhum bloco serve, o heap **cresce**: `crescer` mapeia mais páginas logo
depois do fim (pelo menos 64 KiB de cada vez, para não mapear de página em
página) e entrega o pedaço novo a `inserir`, que o junta ao último bloco
livre se ele encostar no fim. Depois o laço tenta de novo.

`livre_no_fim()` é um ajuste que o teste da seção 7.4 revelou. Na primeira
versão, o heap crescia sempre "tamanho + alinhamento", ignorando o bloco
livre que já estava no fim e que ia se juntar ao pedaço novo. Cada
crescimento deixava 4 KiB a mais sobrando no fim, e esse desperdício se
acumulava: 63 blocos de 1 MiB custavam 64 824 KiB em vez de 64 516.

`saturating_sub` é uma subtração que para em zero em vez de dar *overflow*.

Quando o limite de 64 MiB é atingido (ou os quadros acabam), `alocar`
devolve um ponteiro nulo. O Rust trata isso chamando o *alloc error
handler*, que num kernel `no_std` dispara um panic: "memory allocation of N
bytes failed" (seção 7.4).

O heap **não devolve** quadros ao alocador de quadros quando encolhe: uma
vez mapeada, a memória fica com o heap. É uma simplificação comum; um
kernel maior desmapearia páginas inteiras livres no fim do heap.

---

## 5. Ligando o heap ao Rust: `#[global_allocator]`

Até aqui temos uma struct `Heap` com `alocar` e `liberar`. Falta dizer ao
Rust: **"é aqui que `Box`, `Vec` e `String` devem pedir memória"**.

A crate `alloc` (a parte da biblioteca padrão com `Box`, `Vec` e `String`)
não sabe de onde vem a memória. Ela chama uma interface, a trait
**`GlobalAlloc`**, que tem dois métodos obrigatórios
([rust-basico §27](rust-basico.md#27-globalalloc-layout-e-global_allocator)):

```rust
pub struct AlocadorLista;

unsafe impl GlobalAlloc for AlocadorLista {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match HEAP.lock().as_mut() {
            Some(h) => unsafe { h.alocar(layout) },
            None => ptr::null_mut(),          // heap ainda não existe
        }
    }

    unsafe fn dealloc(&self, ponteiro: *mut u8, layout: Layout) {
        let mut guarda = HEAP.lock();
        let h = guarda.as_mut().expect("heap nao inicializado");
        unsafe { h.liberar(ponteiro, layout) };
    }
}

#[cfg(not(feature = "teste-bump"))]
#[global_allocator]
static ALOCADOR: AlocadorLista = AlocadorLista;
```

- **`Layout`** descreve um pedido: **tamanho** e **alinhamento**. Um
  `Box<u64>` pede `Layout { size: 8, align: 8 }`. Note que `dealloc` também
  recebe o layout: o Rust lembra o tamanho de cada alocação, então o
  alocador não precisa guardá-lo num cabeçalho. É por isso que o bloco
  entregue não tem cabeçalho nenhum.
- **`#[global_allocator]`** marca o `static` que o programa inteiro vai
  usar. Só pode haver um. Com a feature `teste-bump`, este fica desligado e
  o de `bump.rs` assume.
- **`unsafe impl`**: implementar `GlobalAlloc` é uma promessa que o
  compilador não consegue verificar ("os blocos que eu entrego são válidos,
  alinhados e não se sobrepõem"). Se o alocador errar, todo `Vec` do kernel
  pode corromper memória.
- O `Heap` fica num `static Mutex<Option<Heap>>`, o mesmo padrão do console
  (doc 01) e do alocador de quadros (doc 05). Como ele guarda ponteiros
  crus, é preciso declarar `unsafe impl Send for Heap {}`, como foi feito
  com o `FrameBuffer` no doc 01.

E, no topo do `main.rs`, uma linha:

```rust
extern crate alloc;
```

Ela traz a crate `alloc` para o projeto. A partir daí, `alloc::vec::Vec`,
`alloc::boxed::Box`, `alloc::string::String` e a macro `alloc::format!`
funcionam como em qualquer programa Rust.

### A ordem importa

```rust
// ---- Heap (docs/07-heap.md) ----
memoria::heap::init();
demonstrar_heap();
testes_de_heap();
```

`heap::init()` só pode rodar **depois** de `paginacao::init()`: ele mapeia
páginas nas tabelas do kernel e pede quadros ao alocador. Antes disso,
qualquer `Box::new` receberia o ponteiro nulo do `alloc` e viraria panic.
Hoje nada no kernel usa o heap antes dessa linha.

```rust
pub fn init() {
    let mapeado = mapear_faixa(HEAP_INICIO, TAMANHO_INICIAL);
    assert!(mapeado == TAMANHO_INICIAL, "heap: sem quadros para o heap inicial");
    let mut h = Heap { livres: ptr::null_mut(), fim: HEAP_INICIO + mapeado, ... };
    unsafe { h.inserir(HEAP_INICIO as usize, mapeado as usize) };   // 1 bloco livre de 1 MiB
    *HEAP.lock() = Some(h);
}
```

---

## 6. Um passo de cada vez: o que acontece num `Box::new(42u64)`

```
 Box::new(42u64)
   └─ alloc::alloc(Layout { size: 8, align: 8 })            [crate alloc]
      └─ ALOCADOR.alloc(layout)                              [#[global_allocator]]
         └─ HEAP.lock()  → Heap::alocar(layout)              [heap.rs]
            ├─ tamanho_real = 16, alinhamento = 16
            ├─ procurar(16, 16): o primeiro bloco livre serve
            │    recorta 16 bytes do início; a sobra vira o novo nó livre
            └─ devolve 0xFFFF_9000_0000_0000
   └─ grava 42 nesse endereço
 ...
 fim do escopo: o Box sai de escopo (drop)
   └─ ALOCADOR.dealloc(0xFFFF_9000_0000_0000, Layout { size: 8, align: 8 })
      └─ Heap::liberar → inserir(…, 16): junta com o bloco livre vizinho
```

---

## 7. Como foi testado

### 7.1 O boot normal

O boot agora usa o heap sempre, numa pequena demonstração:

```
Heap: faixa virtual a partir de 0xffff900000000000
  Box::new(42)      -> 42 em 0xffff900000000000
  Vec (quadrados)   -> [1, 4, 9, 16, 25, 36, 49, 64]
  String            -> "SiliconOS tem heap" (18 bytes)
  em uso: 112 bytes em 3 alocacoes; mapeado: 1024 KiB; blocos livres: 2
```

Dá para conferir cada número à mão, e eles mostram a fragmentação
acontecendo em escala pequena:

| Passo | Pede | Recebe | Onde (desde o início do heap) |
|---|---|---|---|
| `Box::new(42u64)` | 8 bytes | 16 | 0–16 |
| `Vec` com 8 `u64` | 64 | 64 | 16–80 |
| `String::from("SiliconOS")` | 9 | 16 | 80–96 |
| `push_str(" tem heap")`: a String precisa de 18 bytes, então é **realocada** | 18 | 32 | 96–128, e o bloco 80–96 é liberado |

Resultado: 16 + 64 + 32 = **112 bytes em uso** em 3 alocações vivas, e **2
blocos livres**: o buraco de 16 bytes em 80–96, que a String deixou para
trás, e o resto do heap a partir de 128.

### 7.2 `./run.sh teste-heap`

```
Teste heap: Vec de 200000 u64 ok; heap cresceu de 1024 KiB para 4096 KiB
Teste heap OK: Box, Vec, String, 15128 alocacoes e 15128 liberacoes,
  pico 3072 KiB; blocos livres: 5001 com metade das caixas liberada, 1 depois de
  realocar 5 000 caixas nos buracos; alinhamento 64 e 4096 ok;
  em uso no fim = no inicio (0 bytes), 1 bloco livre de 4096 KiB;
  quadros usados pelo crescimento: 769 (o heap nao devolve quadros)
```

O teste faz seis verificações:

1. **`Box`**: o valor é lido de volta e o endereço está dentro do heap.
2. **`Vec` crescendo além do heap inicial**: 200 000 `u64` (1,6 MB, mais que
   o 1 MiB inicial). A soma confere, e o heap cresceu. O **pico de 3072
   KiB** tem explicação: um `Vec` dobra de capacidade quando enche, e
   durante a cópia o buffer antigo (1 MiB) e o novo (2 MiB) existem ao
   mesmo tempo. O heap mapeou 4 MiB no total porque os buracos deixados
   pelos buffers anteriores nunca bastam para o próximo: 32 + 64 + ... +
   N/2 é sempre menor que N.
3. **`String` e `format!`**: 100 números concatenados, conferidos pelo
   começo, pelo fim e pelo tamanho.
4. **10 000 caixas pequenas**: libera as de posição ímpar, o que cria
   **5000 buracos** de 32 bytes (5001 blocos livres, contando o resto do
   heap). Depois cria 5000 caixas novas, que caem exatamente nesses buracos
   (*first fit*), e a lista volta a ter **1 bloco**. O conteúdo de todas as
   caixas é conferido.
5. **Alinhamento**: pedidos de 100 bytes com alinhamento 64 e 4096, feitos
   direto ao alocador (`alloc::alloc::alloc`), voltam com endereços
   múltiplos do alinhamento.
6. **Nada vazou**: depois de liberar tudo, o uso volta ao valor do início,
   e os blocos livres se juntaram num só, do tamanho de todo o heap
   mapeado. É a prova de que a junção de vizinhos funciona nos quatro
   casos da seção 4.4.

Os 769 quadros são as 768 páginas do crescimento (3 MiB) mais uma tabela PT
nova, porque o heap passou a ocupar uma segunda faixa de 2 MiB.

O teste também passou com `RAM=4G` e `RAM=8G`.

### 7.3 `./run.sh teste-heap-liberacao-dupla`

```
Teste: liberando o bloco 0xffff900000000000 do heap duas vezes...
!!! KERNEL PANIC !!!
Local: src/memoria/heap.rs:177:17
Mensagem: heap: liberar 0xffff900000000000: bloco ja estava livre (liberacao dupla)
```

Em Rust seguro, uma liberação dupla é impossível: o `Box` libera a memória
exatamente uma vez, no `drop`. Por isso o teste chama o alocador
diretamente, com `unsafe`. Um bug assim pode aparecer no futuro em código
`unsafe` do kernel, e o heap agora o pega na hora.

### 7.4 `./run.sh teste-heap-esgotar`

```
Teste: alocando 1 MiB por vez ate o limite do heap (64 MiB)...
  16 MiB alocados (heap mapeado: 16388 KiB)
  32 MiB alocados (heap mapeado: 32772 KiB)
  48 MiB alocados (heap mapeado: 49156 KiB)
  60 MiB alocados (heap mapeado: 61444 KiB)
  61 MiB alocados (heap mapeado: 62468 KiB)
  62 MiB alocados (heap mapeado: 63492 KiB)
  63 MiB alocados (heap mapeado: 64516 KiB)
!!! KERNEL PANIC !!!
Local: /rustc/.../library/alloc/src/alloc.rs:673:9
Mensagem: memory allocation of 1048576 bytes failed
```

Cada bloco de 1 MiB custa exatamente 1024 KiB de heap. O 64º não cabe:
faltam só 1020 KiB até o limite (65 536 − 64 516), porque o primeiro MiB
do heap já tinha os dados da demonstração e o `Vec` que guarda os blocos.
O panic vem de dentro da crate `alloc` (por isso o `Local:` aponta para o
código do Rust, não para o nosso): é o *alloc error handler* reagindo ao
ponteiro nulo que o heap devolveu.

Antes do ajuste `livre_no_fim()` da seção 4.5, este mesmo teste mostrava
64 824 KiB mapeados para 63 blocos: foi assim que o desperdício apareceu.

### 7.5 `./run.sh teste-heap-executar`: o NX funciona

O teste grava bytes `0xC3` (a instrução `ret`) num `Box` e chama esse
endereço como se fosse uma função:

```rust
let codigo = Box::new([0xC3u8; 16]);
let endereco = codigo.as_ptr() as u64;
let funcao: extern "C" fn() = unsafe { core::mem::transmute(endereco) };
funcao();
```

(`transmute` reinterpreta os bits de um valor como outro tipo: aqui, um
número vira um ponteiro de função. É uma das operações mais perigosas do
Rust, e por isso é `unsafe`.)

```
Teste: executando um `ret` gravado no heap em 0xffff900000000000...
!!! KERNEL PANIC !!!
Mensagem: EXCECAO #14 PAGE FAULT ao executar 0xffff900000000000 (pagina presente, sem permissao, modo kernel)
RIP=0xffff900000000000 CS=0x8 RFLAGS=0x2 RSP=0x7e960a8 SS=0x10
```

O tratador de page fault do doc 04 decodificou tudo: **"ao executar"** (bit
4 do código de erro: era busca de instrução) e **"página presente, sem
permissão"** (a página existe, mas proíbe execução). O RIP é o próprio
endereço do heap.

**A prova contrária também foi feita.** Tirando temporariamente o
`SEM_EXECUCAO` de `BITS_HEAP`, o mesmo teste executou o `ret`, voltou e
imprimiu "ERRO: o heap deveria ser proibido de executar (NX)". Depois o
código foi restaurado.

### 7.6 Os testes anteriores

Os 13 testes dos capítulos anteriores (panic, as 6 exceções, quadros,
liberação dupla de quadro, os 3 de paginação) passaram de novo com o heap
ativo, e `teste-paginacao` também com `RAM=8G`.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 06 (tag `doc-06`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-07`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-06...doc-07).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/memoria/heap.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/memoria/heap.rs) | **novo**: `NoLivre`, `Heap` (procurar, inserir, crescer, alocar, liberar), `HEAP`, `AlocadorLista` com `#[global_allocator]`, `init`, `estatisticas`, `contem` | — |
| [`src/memoria/bump.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/memoria/bump.rs) | **novo**: `Bump`, `AlocadorBump` (global só com a feature `teste-bump`), `init`, `estado` | — |
| [`src/memoria/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/memoria/mod.rs) | `#[allow(dead_code)] pub mod heap;` e `#[allow(dead_code)] pub mod bump;` | junto dos outros `pub mod` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/main.rs) | `extern crate alloc;` | topo, depois dos `#![...]` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/main.rs) | em `kernel_main`: `memoria::heap::init(); demonstrar_heap(); testes_de_heap();` | depois de `testes_de_paginacao()` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/main.rs) | `fn demonstrar_heap`, `fn testes_de_heap` | fim do arquivo |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/Cargo.toml) | `license = "MIT"`; `teste-heap`, `teste-heap-liberacao-dupla`, `teste-heap-esgotar`, `teste-heap-executar`, `teste-bump` | `[package]` e `[features]` |

---

## Diferença em relação ao livro

O capítulo 8 do livro trata do mesmo tema: dar ao kernel um heap para
usar `Box`, `Vec` e `String`. Os alocadores apresentados podem ser outros
(além do bump e da lista encadeada, é comum ver um alocador de **blocos de
tamanho fixo**, que mantém uma lista para cada tamanho: 16, 32, 64...
bytes, e é bem mais rápido). Vale comparar ao ler.

As particularidades do SiliconOS:

- o heap fica numa faixa virtual própria (`0xFFFF_9000_...`), mapeada com as
  tabelas que o próprio kernel montou no doc 06, e **cresce sob demanda**
  até 64 MiB;
- as páginas do heap usam **NX**, com teste nos dois sentidos;
- a lista de blocos livres é escrita à mão, sem a crate
  `linked_list_allocator`, seguindo a linha do projeto de mostrar o que as
  bibliotecas escondem.

---

## Resumo

- `Box`, `Vec` e `String` vêm da crate **`alloc`**, que precisa de um
  **alocador global**: um `static` marcado com `#[global_allocator]` que
  implementa **`GlobalAlloc`** (`alloc` e `dealloc`, recebendo um
  **`Layout`** com tamanho e alinhamento).
- O **heap** é uma camada sobre o alocador de quadros e a paginação: uma
  faixa virtual própria (`0xFFFF_9000_0000_0000`), começando com 1 MiB e
  crescendo sob demanda até 64 MiB, com páginas **NX**.
- O alocador **bump** só avança e nunca reaproveita memória: rápido, mas
  inaceitável sozinho num kernel.
- A **lista de blocos livres** guarda os buracos dentro deles mesmos,
  ordenada por endereço. Alocar = *first fit* e recortar; liberar = inserir
  na ordem e **juntar vizinhos**, o que combate a **fragmentação**.
- Liberação dupla vira panic na hora; esgotar o heap vira "memory
  allocation of N bytes failed".

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que não usar o alocador de quadros diretamente para um `Box<u64>`?**
   Porque ele só entrega blocos de 4 KiB. Um `Box<u64>` precisa de 8 bytes:
   99,8% do quadro seria desperdiçado.

2. **Onde a lista de blocos livres guarda os seus nós?**
   Dentro dos próprios blocos livres: os primeiros 16 bytes de cada bloco
   livre são um `NoLivre` (tamanho e próximo). Por isso todo bloco tem pelo
   menos 16 bytes.

3. **Por que a lista é mantida em ordem de endereço?**
   Para que, ao liberar um bloco, os vizinhos dele na lista sejam também os
   vizinhos na memória. Assim dá para juntar blocos adjacentes e evitar a
   fragmentação.

4. **Como o alocador sabe o tamanho do bloco que está sendo liberado, se
   o bloco entregue não tem cabeçalho?**
   O Rust passa o mesmo `Layout` em `dealloc` que passou em `alloc`. O
   alocador recalcula o tamanho com a mesma função (`tamanho_real`).

5. **No boot, por que há 2 blocos livres e não 1, se nada foi liberado de
   propósito?**
   Porque a `String` cresceu de 9 para 18 bytes e foi realocada: o bloco
   novo (32 bytes) foi para depois, e o antigo (16 bytes) virou um buraco
   entre o `Vec` e a String.

6. **O que aconteceria se o heap fosse mapeado sem o bit NX?**
   Bytes gravados no heap poderiam ser executados como código. O teste
   `teste-heap-executar` mostra os dois casos: com NX, page fault "ao
   executar"; sem NX, o `ret` executa e volta.

</details>
