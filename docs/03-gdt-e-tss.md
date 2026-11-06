# 03: GDT e TSS: colocando a CPU num estado que é nosso

> **Você vai entender:** o que são segmentos e níveis de privilégio; por
> que, mesmo num processador de 64 bits, ainda precisamos de uma tabela
> chamada GDT; o que é a TSS e por que ela guarda "pilhas de emergência";
> e como se entrega uma tabela nova à CPU.
>
> **Arquivo:** `src/gdt.rs` (chamado no início de `kernel_main`)
> **Livro:** capítulo 4
> **Rust novo aqui:** `#[repr(C)]` e `packed` ([§13](rust-basico.md#13-atributos--e-)),
> `const` e `static mut` ([§10](rust-basico.md#10-static-e-const)),
> `Once` ([§11](rust-basico.md#11-mutex-e-once-da-crate-spin)),
> closures ([§16](rust-basico.md#16-closures))
> **Leia antes:** a seção "Bits, bytes e hexadecimal" do [glossário](glossario.md#bits-bytes-e-hexadecimal-revisão-rápida)

---

## 1. O problema

Quando o `kernel_main` começa, a CPU ainda está configurada com tabelas que
o **firmware** montou para uso próprio. O próprio SiliconOS mostra isso no
boot:

```
GDT do firmware:  base=0x75dc000 CS=0x38 SS=0x30 TR=0x0
GDT do SiliconOS: base=0x60af020 limite=55 CS=0x8 SS=0x10 DS=0x10 TR=0x28
```

A primeira linha diz que, antes desta etapa, o kernel rodava usando uma
tabela localizada no endereço `0x75dc000`, que pertence ao firmware. Isso
traz três problemas:

1. **Aquela memória não é nossa.** Quando escrevermos o gerenciador de
   memória, essa região pode ser reaproveitada e sobrescrita. A CPU
   continuaria lendo a tabela dali, agora cheia de lixo, e o sistema
   quebraria de forma misteriosa.
2. **Faltam coisas que vamos precisar:** pilhas de emergência para erros
   graves (usadas no doc 04) e configurações para programas comuns (modo
   usuário).
3. **Não sabemos exatamente o que o firmware configurou.** Um kernel
   precisa saber em que estado a CPU está.

A solução é montar nossa própria tabela e mandar a CPU usá-la.

---

## 2. Um pouco de história: o que são segmentos

Para entender a GDT, é preciso saber de onde ela veio. A arquitetura x86 tem
mais de 45 anos e **nunca quebrou a compatibilidade**; cada geração
empilhou novidades sobre a anterior.

### 1978: o 8086 e os segmentos

Os registradores do 8086 tinham 16 bits, o que permite endereçar só 64 KiB.
Para chegar a 1 MiB, a Intel criou os **registradores de segmento** (CS
para código, DS para dados, SS para a pilha...). O endereço real era
calculado assim:

```
endereço real = segmento × 16 + deslocamento
```

Era como ter vários "ponteiros-base" para regiões da memória.

### 1982–1985: modo protegido e a GDT

Nos processadores seguintes (286, 386), os registradores de segmento
deixaram de guardar um endereço e passaram a guardar um **seletor**: um
número que aponta para uma linha de uma tabela na memória, a **GDT**
(*Global Descriptor Table*). Cada linha, chamada **descritor**, descreve um
segmento:

- onde ele começa (base);
- qual o tamanho (limite);
- **quem pode usá-lo** (nível de privilégio);
- **o que se pode fazer** com ele (executar? escrever?).

Foi assim que nasceu a proteção de memória: um programa comum não
conseguia acessar os segmentos do sistema operacional.

### 2003: modo de 64 bits ("long mode")

Quando a AMD criou o x86-64, a segmentação já tinha sido trocada por um
mecanismo melhor, a **paginação** (capítulo 7 do livro). Então, em 64 bits,
a CPU **ignora a base e o limite** dos segmentos de código e dados: a
memória vira um espaço único e "plano".

**Mas a GDT não foi abolida**, porque ela ainda é necessária para três
coisas:

1. **CS define o modo e o privilégio do código.** Um bit do descritor diz
   "este é código de 64 bits", e outro campo diz em que **nível de
   privilégio** o código roda.
2. **SS** precisa apontar para um descritor válido em algumas situações
   (ao voltar de uma interrupção, ao trocar de nível de privilégio).
3. **A TSS** (seção 5) só pode ser encontrada pela CPU através da GDT.

Resumindo: em 64 bits, a GDT é uma tabela **pequena e quase protocolar**,
mas obrigatória.

---

## 3. Níveis de privilégio (anéis)

A CPU x86 tem 4 níveis de privilégio, chamados **anéis**, numerados de 0 a
3. Na prática, todos os sistemas operacionais modernos usam só dois:

```
 ┌───────────────────────────────────────────────────────────────┐
 │ anel 3: programas comuns (navegador, editor, seu backend)      │
 │   • não podem executar instruções privilegiadas                │
 │   • não podem acessar a memória do kernel                      │
 │   ┌───────────────────────────────────────────────────────┐   │
 │   │ anel 0: kernel                                         │   │
 │   │   • pode tudo: cli, hlt, lgdt, in/out, mexer na memória│   │
 │   └───────────────────────────────────────────────────────┘   │
 └───────────────────────────────────────────────────────────────┘
```

O anel atual da CPU fica nos **2 bits mais baixos do registrador CS**. Se
CS = `0x08`, os 2 bits baixos são `00`, ou seja, anel 0. Se um programa no
anel 3 tentar executar `cli` ou `hlt`, a CPU recusa e gera uma exceção
(doc 04).

É essa separação que impede um programa com bug de derrubar o sistema
inteiro. Hoje o SiliconOS só tem código no anel 0, mas a GDT já prevê os
segmentos do anel 3 para quando rodarmos programas.

---

## 4. A nossa GDT

### 4.1 O conteúdo

| Linha (índice) | Seletor | Descreve | Usado para |
|---|---|---|---|
| 0 | `0x00` | nada (linha nula) | obrigatória pela especificação; carregar o seletor 0 em CS é proibido |
| 1 | `0x08` | código do kernel, 64 bits, anel 0 | CS enquanto o kernel roda |
| 2 | `0x10` | dados do kernel, anel 0 | SS, DS, ES |
| 3 | `0x1B` | dados de usuário, anel 3 | futuro: programas comuns |
| 4 | `0x23` | código de usuário, 64 bits, anel 3 | futuro: programas comuns |
| 5 e 6 | `0x28` | a TSS (ocupa duas linhas) | pilhas de emergência |

No código, a tabela é simplesmente um array de 7 números de 64 bits:

```rust
[
    0,                // linha 0: nula
    DESC_KERNEL_CODE, // linha 1
    DESC_KERNEL_DATA, // linha 2
    DESC_USER_DATA,   // linha 3
    DESC_USER_CODE,   // linha 4
    tss_baixo,        // linha 5 ┐ descritor da TSS
    tss_alto,         // linha 6 ┘ (16 bytes)
]
```

### 4.2 Como funciona um seletor

O seletor é o número que vai nos registradores de segmento (CS, DS...). Ele
**não é** o índice da linha diretamente; é o índice multiplicado por 8,
mais alguns bits de informação:

```
seletor (16 bits):   índice da linha  |  TI  |  RPL
                     bits 15 ... 3    | bit 2| bits 1-0
```

- **índice**: qual linha da GDT. Multiplicar por 8 equivale a deslocar 3
  bits para a esquerda (`1 << 3 = 8`), abrindo espaço para os outros campos.
  Cada linha tem 8 bytes, então "índice × 8" também é a posição em bytes
  dentro da tabela.
- **TI**: 0 = GDT (há uma tabela alternativa, a LDT, que não usamos).
- **RPL**: o nível de privilégio "pedido". Para segmentos de usuário, é 3.

Em Rust:

```rust
pub const KERNEL_CODE: u16 = 1 << 3;        // linha 1 → 0x08
pub const KERNEL_DATA: u16 = 2 << 3;        // linha 2 → 0x10
pub const USER_DATA: u16 = (3 << 3) | 3;    // linha 3, anel 3 → 0x18 | 3 = 0x1B
pub const USER_CODE: u16 = (4 << 3) | 3;    // linha 4, anel 3 → 0x20 | 3 = 0x23
pub const TSS_SEL: u16 = 5 << 3;            // linha 5 → 0x28
```

Por que os dados de usuário vêm **antes** do código de usuário? Porque
as instruções `syscall`/`sysret`, que usaremos no futuro para os
programas pedirem coisas ao kernel, calculam esses seletores somando
valores fixos e esperam exatamente essa ordem.

### 4.3 Montando um descritor bit a bit

Cada descritor é um número de 64 bits em que cada bit (ou grupo de bits)
tem um significado definido pelo manual da Intel/AMD. Muitos tutoriais
escrevem o valor pronto, como `0x00AF9A000000FFFF`, o que é impossível de
entender. O SiliconOS dá **nome** a cada bit e monta o valor com `|` (OR):

```rust
const P: u64 = 1 << 47;    // Present: o descritor é válido
const S: u64 = 1 << 44;    // 1 = segmento de código/dados (0 = "de sistema", como a TSS)
const E: u64 = 1 << 43;    // Executable: é código
const RW: u64 = 1 << 41;   // código: pode ser lido / dados: pode ser escrito
const L: u64 = 1 << 53;    // Long: código de 64 bits
const DB: u64 = 1 << 54;   // tamanho padrão de 32 bits (não pode ir junto com L)
const G: u64 = 1 << 55;    // Granularity: o limite conta em blocos de 4 KiB
const fn dpl(n: u64) -> u64 { n << 45 }   // nível de privilégio: bits 45 e 46

const DESC_KERNEL_CODE: u64 = P | S | E | RW | L | G | LIMITE_MAX | dpl(0);
const DESC_KERNEL_DATA: u64 = P | S | RW | DB | G | LIMITE_MAX | dpl(0);
const DESC_USER_DATA:   u64 = P | S | RW | DB | G | LIMITE_MAX | dpl(3);
const DESC_USER_CODE:   u64 = P | S | E | RW | L | G | LIMITE_MAX | dpl(3);
```

Leia `DESC_KERNEL_CODE` como uma frase: "presente, de código/dados,
executável, legível, de 64 bits, ..., anel 0". A diferença entre o código
do kernel e o do usuário é **só o DPL** (0 ou 3).

`const fn` é uma função que o compilador consegue executar **durante a
compilação**. Todos esses valores são calculados antes do programa rodar; o
binário já contém os números prontos.

Base e limite são ignorados em 64 bits, mas preenchemos o limite com o
valor máximo (`LIMITE_MAX`) por convenção: algumas ferramentas e emuladores
conferem esse campo.

---

## 5. A TSS (Task State Segment)

### 5.1 O que sobrou dela em 64 bits

O nome é histórico. Em 32 bits, a TSS guardava o estado completo de uma
tarefa (todos os registradores) para que o próprio processador trocasse de
tarefa. Os sistemas operacionais não usavam muito esse recurso, e ele foi
removido em 64 bits. O que sobrou da TSS é basicamente **uma lista de
endereços de pilha**:

```rust
#[repr(C, packed(4))]
pub struct Tss {
    _reservado0: u32,
    pub rsp: [u64; 3],     // pilhas para quando a CPU sobe de privilégio (anel 3 → 0)
    _reservado1: u64,
    pub ist: [u64; 7],     // Interrupt Stack Table: 7 pilhas de emergência
    _reservado2: u64,
    _reservado3: u16,
    pub iomap_base: u16,
}

const _: () = assert!(size_of::<Tss>() == 104);
```

Dois detalhes de Rust importantes:

- **`#[repr(C, packed(4))]`**: a CPU lê essa struct diretamente da memória,
  esperando cada campo numa posição exata (o manual diz "o campo `ist[0]`
  fica no byte 36"). `repr(C)` impede o compilador de reordenar os campos;
  `packed(4)` impede que ele insira espaços de alinhamento entre eles
  ([rust-basico §13](rust-basico.md#13-atributos--e-)).
- **`const _: () = assert!(...)`**: uma verificação feita **durante a
  compilação**. Se, por qualquer erro, a struct não tiver exatamente 104
  bytes (o tamanho definido pela Intel), o projeto não compila. É uma rede
  de segurança barata.

### 5.2 Por que pilhas de emergência

Lembre que a **pilha** guarda as variáveis locais e os endereços de retorno
das funções ([glossário: pilha](glossario.md#a-pilha-stack)). Quando ocorre
uma exceção, a CPU **empilha** informações (onde o código estava, as flags...)
antes de chamar o tratador. Agora imagine:

1. O kernel tem uma recursão infinita, e a pilha cresce até sair da região
   de memória válida.
2. A próxima chamada tenta escrever na pilha, numa memória que não existe:
   a CPU gera uma exceção (*page fault*).
3. Para chamar o tratador do page fault, a CPU precisa **empilhar** as
   informações... **na mesma pilha quebrada**. Falha de novo, e a CPU
   declara um **double fault** ("falha enquanto tratava uma falha").
4. Para chamar o tratador do double fault, a CPU tenta empilhar de novo, e
   falha de novo. Isso é um **triple fault**: a CPU desiste e **reinicia o
   computador**, sem mostrar nada.

A solução é a **IST**: a TSS guarda até 7 endereços de pilhas separadas.
Uma entrada da tabela de interrupções (doc 04) pode dizer: "para esta
exceção, **troque para a pilha IST nº 1 antes de empilhar qualquer coisa**".
Assim o tratador de double fault roda numa pilha limpa e garantidamente
válida, e consegue mostrar a mensagem de erro.

O doc 04 mostra isso funcionando, com teste.

### 5.3 A pilha de emergência no código

```rust
const TAM_PILHA_IST: usize = 5 * 4096;     // 20 KiB

#[repr(C, align(16))]
struct Pilha([u8; TAM_PILHA_IST]);

static mut PILHA_DOUBLE_FAULT: Pilha = Pilha([0; TAM_PILHA_IST]);
```

- A pilha é só um array de 20 480 bytes.
- `align(16)`: o início do array fica num endereço múltiplo de 16, um
  alinhamento que a convenção de chamada do x86-64 exige para a pilha.
- É `static`, portanto fica **dentro do próprio executável** (numa seção
  chamada `.bss`, para variáveis inicializadas com zero). Não precisamos de
  alocador de memória, que ainda não temos.
- É `static mut` (global mutável) porque a CPU vai escrever nela.

**A pilha cresce para baixo**: cada `push` *diminui* o endereço. Por isso a
TSS recebe o endereço do **fim** do array, não do começo:

```rust
let inicio = &raw const PILHA_DOUBLE_FAULT as u64;   // endereço do primeiro byte
ist[0] = inicio + TAM_PILHA_IST as u64;              // topo = primeiro byte DEPOIS do array
```

`&raw const X` obtém o endereço de `X` como ponteiro cru, sem criar uma
referência Rust. É a forma recomendada de pegar o endereço de uma
`static mut`.

> **Curiosidade vista no boot:** o topo da pilha de emergência (`0x60af020`)
> é exatamente o endereço onde começa a GDT. O compilador colocou a GDT
> logo depois da pilha na memória. Está correto: o "topo" é o primeiro
> byte **depois** da pilha, e o primeiro `push` grava em `topo - 8`.

### 5.4 Os outros campos

- **`rsp[0]`**: usado no futuro. Quando um programa do anel 3 for
  interrompido, a CPU troca automaticamente para a pilha do kernel indicada
  aqui. O kernel não pode confiar na pilha de um programa comum.
- **`iomap_base`**: definir com o tamanho da TSS significa "não há mapa de
  permissões de E/S", ou seja, programas comuns **não podem** usar `in`/`out`
  para falar com dispositivos.

### 5.5 O descritor da TSS ocupa duas linhas

Descritores normais têm 8 bytes, e a base de um descritor só tem espaço
para 32 bits (espalhados em três pedaços, herança do 286). A TSS pode estar
em qualquer endereço de 64 bits, então o descritor dela foi estendido para
**16 bytes**, ocupando duas linhas da GDT. A segunda linha guarda os 32 bits
altos do endereço:

```rust
fn descritor_tss(base: u64, limite: u64) -> (u64, u64) {
    let tipo_tss_disponivel: u64 = 0x9;      // "TSS de 64 bits, disponível"
    let baixo = (limite & 0xFFFF)              // limite, bits 0-15
        | ((base & 0xFF_FFFF) << 16)          // base, bits 0-23
        | (tipo_tss_disponivel << 40)
        | P
        | (((limite >> 16) & 0xF) << 48)      // limite, bits 16-19
        | (((base >> 24) & 0xFF) << 56);      // base, bits 24-31
    let alto = base >> 32;                    // base, bits 32-63
    (baixo, alto)
}
```

É só "picar" o endereço e o tamanho e encaixar cada pedaço no lugar que o
manual manda. A função devolve uma **tupla** com os dois números (como um
`Pair` do Kotlin).

---

## 6. Por que `spin::Once`

A GDT e a TSS têm duas exigências:

1. **Precisam existir para sempre e no mesmo endereço.** A CPU guarda só o
   **endereço** delas, não uma cópia. Se a tabela fosse uma variável local
   de `init()`, ela seria destruída quando a função terminasse, e a CPU
   ficaria apontando para lixo.
2. **Precisam ser montadas em tempo de execução.** O descritor da TSS
   contém o endereço da TSS, que só é conhecido quando o programa está
   carregado na memória.

`Once<T>` atende as duas ([rust-basico §11](rust-basico.md#11-mutex-e-once-da-crate-spin)):

```rust
static TSS: Once<Tss> = Once::new();     // vazio no início, mas com endereço fixo
static GDT: Once<[u64; 7]> = Once::new();

let tss = TSS.call_once(|| { /* monta a TSS */ });   // executa só na primeira vez
```

`call_once` executa a closure ([rust-basico §16](rust-basico.md#16-closures))
uma única vez, guarda o resultado **dentro do `static`** e devolve uma
referência `&'static`, ou seja, válida para sempre.

---

## 7. Entregando as tabelas à CPU

Montar a tabela na memória não basta: é preciso dizer à CPU onde ela está e
fazê-la usar os novos segmentos. São quatro passos, todos em assembly.

### Passo 1: `lgdt`: "a GDT está aqui"

```rust
#[repr(C, packed(2))]
pub(crate) struct PonteiroTabela {
    pub limite: u16,   // tamanho da tabela em bytes, menos 1
    pub base: u64,     // endereço da tabela
}

asm!("lgdt [{}]", in(reg) &ponteiro, ...);
```

A instrução `lgdt` (*load GDT*) lê uma estrutura de 10 bytes: o **limite**
(tamanho − 1; aqui 7 × 8 − 1 = 55) e o **endereço**. O `packed(2)` garante
que os dois campos fiquem colados, sem espaço entre eles, como a CPU
espera.

**Atenção:** depois do `lgdt`, a CPU **ainda não** está usando os novos
segmentos. Cada registrador de segmento guarda uma **cópia interna** do
descritor que foi carregado nele, e só relê a tabela quando é recarregado.
Por isso vêm os passos 2 e 3.

### Passo 2: recarregar CS com um "retorno falso"

Não existe uma instrução `mov cs, ...`. O registrador CS só muda por meio de
um salto, uma chamada ou um retorno do tipo "far" (que troca de segmento).
O truque usado por quase todo kernel é **simular um retorno de função**:

```rust
asm!(
    "push {sel}",            // empilha o novo seletor de código (0x08)
    "lea {tmp}, [rip + 2f]", // calcula o endereço do rótulo "2:" logo abaixo
    "push {tmp}",            // empilha esse endereço
    "retfq",                 // "far return": desempilha endereço + seletor e salta
    "2:",                    // ...e a execução continua aqui, com CS = 0x08
    ...
);
```

Um `retfq` (retorno "far" de 64 bits) desempilha um endereço e um seletor e
salta para lá, carregando CS. Empilhamos exatamente o que ele espera: o
endereço da linha seguinte e o seletor novo. Na prática, a execução
**continua na linha de baixo**, mas agora com o CS novo.

(`rip + 2f` significa "o endereço do rótulo `2`, procurando para frente
(*forward*)". `rip` é o registrador do endereço da instrução atual.)

### Passo 3: recarregar os segmentos de dados

```rust
asm!("mov ss, {0:x}", "mov ds, {0:x}", "mov es, {0:x}", in(reg) KERNEL_DATA, ...);
asm!("mov fs, {0:x}", "mov gs, {0:x}", in(reg) 0u16, ...);
```

SS, DS e ES recebem o seletor de dados (`0x10`). FS e GS recebem 0: em 64
bits eles têm outro uso (dados por CPU e por thread), que veremos no
futuro. O `{0:x}` pede a versão de 16 bits do registrador, porque
registradores de segmento têm 16 bits.

### Passo 4: `ltr`: "a TSS está aqui"

```rust
asm!("ltr {0:x}", in(reg) TSS_SEL, ...);
```

`ltr` (*load task register*) aponta o registrador de tarefa (TR) para o
descritor da TSS na linha 5. Um detalhe: a CPU **escreve na GDT** neste
momento, marcando o descritor como "ocupado" (*busy*). Por isso a GDT
precisa estar em memória que aceite escrita, e um segundo `ltr` com o mesmo
seletor causaria uma exceção.

---

## 8. Por que o `cli` no início do `kernel_main`

A tabela de interrupções que está ativa nesse momento ainda é a **do
firmware** (doc 04), e cada entrada dela aponta para tratadores que usam o
seletor de código **do firmware**, `0x38`.

Na nossa GDT, a linha 7 (seletor `0x38`) **não existe**: a tabela só tem 7
linhas, de 0 a 6. Se chegasse uma interrupção de hardware (um tique do timer
do firmware, por exemplo) logo depois do `lgdt`, a CPU tentaria usar um
segmento inexistente, falharia, e terminaria num triple fault.

Por isso o `kernel_main` começa com `cli`, que desliga as interrupções de
hardware. Elas continuam desligadas até configurarmos nosso próprio
controlador de interrupções (capítulo 9 do livro).

---

## 9. Como foi verificado

A função `gdt::estado()` lê os registradores **de volta da CPU**:

| Instrução | O que lê |
|---|---|
| `sgdt` | a base e o limite da GDT que a CPU está usando |
| `mov reg, cs` / `ss` / `ds` | os seletores atuais |
| `str` | o registrador de tarefa (qual TSS está carregada) |

Resultado:

- **antes:** `CS=0x38 SS=0x30 TR=0x0`: segmentos do firmware, e nenhuma TSS;
- **depois:** `CS=0x8 SS=0x10 DS=0x10 TR=0x28`, base apontando para a
  nossa tabela, limite 55.

Também conferimos pelo **monitor do QEMU** (`info registers`), que mostra
os registradores do ponto de vista do emulador, sem depender do nosso
código. Ele confirmou: CS é "código de 64 bits, anel 0", SS/DS são "dados",
e TR aponta para uma TSS de 64 bits com 104 bytes.

Se algum bit de um descritor estivesse errado, a CPU teria recusado o
`retfq` ou o `ltr` com uma exceção, e a máquina teria reiniciado (com o
QEMU em `-no-reboot`, ele simplesmente fecharia). O fato de o boot seguir
normalmente já é uma prova de que os descritores são válidos.

---

## Diferença em relação ao livro

O livro usa a crate `x86_64`, que tem `GlobalDescriptorTable` e
`TaskStateSegment` prontos. O SiliconOS monta tudo à mão para que fique visível
o que essas abstrações fazem. No futuro, se você preferir, dá para trocar
pela crate.

---

## Resumo

- Segmentos vêm do 8086. Em 64 bits, base e limite são ignorados, mas a
  **GDT** continua obrigatória: define o **anel** do código (0 = kernel,
  3 = programas) e localiza a TSS.
- **Seletor** = índice da linha × 8 + bits de privilégio. CS = `0x08`
  significa "linha 1, anel 0".
- A **TSS**, em 64 bits, é uma lista de pilhas. A **IST** dá pilhas de
  emergência para erros graves, evitando que uma pilha quebrada vire
  reinício.
- Para trocar a GDT: `lgdt` (endereço), `retfq` (recarrega CS), `mov` nos
  segmentos de dados, `ltr` (carrega a TSS).
- O `cli` no início evita que uma interrupção use o seletor do firmware,
  que não existe na nossa tabela.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Se em 64 bits a CPU ignora a base e o limite dos segmentos, para que
   serve a GDT?**
   Para definir o nível de privilégio e o modo (64 bits) do código, ter
   segmentos válidos para SS, e localizar a TSS.

2. **O que significa o seletor `0x23`?**
   `0x23` = `0x20 | 3` = linha 4 (`4 × 8 = 0x20`), com privilégio 3: o
   segmento de código de usuário.

3. **Por que a TSS recebe o endereço do *fim* do array da pilha?**
   Porque a pilha cresce para baixo: o primeiro `push` grava logo abaixo do
   topo.

4. **Por que não basta executar `lgdt`?**
   Porque os registradores de segmento guardam cópias internas dos
   descritores antigos; é preciso recarregá-los (CS via `retfq`, os outros
   via `mov`).

5. **O que aconteceria se uma interrupção de hardware chegasse logo depois
   do `lgdt`, sem o `cli`?**
   A IDT do firmware mandaria a CPU usar o seletor `0x38`, que não existe
   na nossa GDT. Isso causaria uma falha, depois um double fault e
   provavelmente um triple fault (reinício).

</details>
