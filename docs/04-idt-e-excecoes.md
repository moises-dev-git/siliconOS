# 04: IDT e exceções: quando a CPU encontra um erro

> **Você vai entender:** a diferença entre um erro detectado pelo Rust e
> um erro detectado pela CPU; o que é a tabela de interrupções (IDT); o
> que a CPU faz, passo a passo, quando algo dá errado; e como o SiliconOS
> transforma cada tipo de erro numa mensagem útil, inclusive o temido
> *double fault*.
>
> **Arquivo:** `src/idt.rs` (chamado em `kernel_main` logo depois de `gdt::init()`)
> **Livro:** capítulo 5
> **Testar:** `./run.sh teste-breakpoint` (ou `teste-divisao`, `teste-opcode`,
> `teste-gpf`, `teste-pagefault`, `teste-doublefault`)
> **Leia antes:** doc 02 (panic) e doc 03 (GDT, TSS e IST)
> **Rust novo aqui:** `extern "x86-interrupt"` (seção 5 deste doc),
> `impl Display` (seção 4)
>
> **Código deste capítulo:** os docs 00 a 06 estão num único commit: [estado no fim do doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). O que este capítulo acrescenta está em [Mudanças nos arquivos](#mudanças-nos-arquivos).

---

## 1. Dois tipos de erro

O doc 02 tratou dos erros que **o Rust** detecta: quando você escreve
`v[i]`, o compilador insere uma verificação "i é menor que o tamanho?"
antes do acesso, e chama `panic!` se não for.

Existe outra classe de erros, que **o Rust nem chega a ver**, porque quem os
detecta é a **própria CPU**, no meio da execução de uma instrução:

| Situação | Quem detecta | O que acontece |
|---|---|---|
| `v[10]` num array de 3 | o Rust (verificação gerada pelo compilador) | `panic!` → doc 02 |
| ler um endereço de memória que não existe | a CPU | **exceção nº 14 (page fault)** |
| instrução `div` com divisor 0 (em assembly) | a CPU | **exceção nº 0** |
| bytes que não formam uma instrução válida | a CPU | **exceção nº 6** |

Uma comparação com Java: o panic é como uma exceção lançada pelo seu código
(`throw`). A exceção da CPU é como um *segmentation fault* que derruba a
JVM inteira: acontece abaixo da linguagem.

Quando a CPU detecta um problema desses, ela **interrompe** o que estava
fazendo e salta para uma rotina de tratamento, o **tratador** (*handler*).
A pergunta é: **qual** rotina? A resposta está numa tabela que o kernel
precisa fornecer, a **IDT**.

Sem uma IDT nossa, essas exceções caem nos tratadores do firmware (que não
funcionam mais depois de `exit_boot_services`) ou viram reinício.

---

## 2. O que é a IDT

A **IDT** (*Interrupt Descriptor Table*) é uma tabela com **256 linhas**,
uma para cada **número de vetor** (0 a 255). Cada linha diz:

> "Quando acontecer o evento nº N, salte para **este endereço** (o
> tratador), usando **este segmento de código** e, opcionalmente, **esta
> pilha de emergência**."

É parecida com um mapa de *event listeners* do JavaScript: para cada tipo
de evento, a função que deve ser chamada. A diferença é que quem dispara os
eventos é o hardware, e o "mapa" é uma tabela binária com formato
definido pela Intel.

### Quem usa a IDT

Os 256 vetores são compartilhados por três tipos de evento:

| Tipo | Origem | Exemplo | Vetores |
|---|---|---|---|
| **Exceções** | a CPU, por causa da instrução atual | divisão por zero, page fault | 0–31 (fixos, definidos pela Intel) |
| **Interrupções de hardware** | um dispositivo pedindo atenção | tecla pressionada, tique do timer | escolhidos pelo kernel (32+) |
| **Interrupções de software** | a instrução `int N` | breakpoint (`int3`), chamadas de sistema antigas | qualquer um |

Neste capítulo tratamos só de **exceções**. Interrupções de hardware vêm
no capítulo 9 do livro.

### As exceções que o SiliconOS trata

| Vetor | Sigla | Nome | Quando acontece | Empilha código de erro? | Tipo |
|---|---|---|---|---|---|
| 0 | #DE | *Divide Error* | divisão inteira por zero | não | fault |
| 3 | #BP | *Breakpoint* | instrução `int3` | não | **trap** |
| 6 | #UD | *Undefined Opcode* | bytes que não são uma instrução | não | fault |
| 8 | #DF | *Double Fault* | erro durante o tratamento de outro erro | sim (sempre 0) | abort |
| 13 | #GP | *General Protection* | "você não pode fazer isso" (vários motivos) | sim | fault |
| 14 | #PF | *Page Fault* | acesso a memória não mapeada ou proibida | sim | fault |

A coluna **Tipo** diz o que acontece *depois* do tratador:

- **Fault** ("falha"): a CPU guarda o endereço **da instrução que falhou**.
  Se o tratador corrigir o problema e retornar, a CPU **tenta a instrução de
  novo**. É assim que funciona a memória virtual sob demanda: o page fault
  acontece, o kernel carrega a página que faltava, retorna, e o programa
  nem percebe.
- **Trap** ("armadilha"): a CPU guarda o endereço **da instrução seguinte**.
  Ao retornar, a execução **continua normalmente**. Breakpoints de
  depurador funcionam assim.
- **Abort**: não há como continuar; o estado da máquina não é confiável.

A coluna **código de erro** é crucial e volta na seção 5: algumas exceções
empilham um número extra com detalhes do erro, e o tratador precisa saber
disso.

### E as exceções que não tratamos?

As linhas da tabela que não preenchemos ficam marcadas como "não
presentes". Se acontecer uma exceção sem tratador, a CPU não consegue
entregá-la e gera uma **#GP** (proteção geral), com o código de erro
indicando qual vetor estava faltando. Como a #GP tem tratador, a mensagem
aparece mesmo assim. Por isso o tratador de #GP decodifica o código de erro
(seção 6.4).

---

## 3. O formato de uma linha da IDT

Cada linha tem 16 bytes:

```rust
#[derive(Clone, Copy)]
#[repr(C)]
struct Entrada {
    endereco_0_15: u16,    // endereço do tratador, bits 0-15
    seletor: u16,          // segmento de código do tratador: 0x08 (KERNEL_CODE)
    ist: u8,               // 0 = usar a pilha atual; 1 a 7 = trocar para a pilha IST n
    tipo_atributos: u8,    // 0x8E (explicado abaixo)
    endereco_16_31: u16,   // endereço, bits 16-31
    endereco_32_63: u32,   // endereço, bits 32-63
    _reservado: u32,
}

const _: () = assert!(size_of::<Entrada>() == 16);   // garantido na compilação
```

O endereço do tratador vem **partido em três pedaços** pela mesma razão da
GDT (doc 03): o formato de 32 bits foi estendido para 64 sem mudar a
posição dos campos antigos.

`#[repr(C)]` garante que os campos fiquem nessa ordem exata na memória
([rust-basico §13](rust-basico.md#13-atributos--e-)), e o `assert!`
garante que a struct tenha exatamente os 16 bytes que a CPU espera.

### O byte `0x8E`

```
0x8E = 1000 1110
       │└┤ └──┴─ bits 0-3 = 1110 (0xE): "interrupt gate" de 64 bits
       │ └────── bits 5-6 = 00: DPL 0 (só o kernel pode disparar com `int N`)
       └──────── bit 7 = 1: presente
```

- **Presente (P = 1):** a linha é válida.
- **DPL = 0:** se um programa comum (anel 3) tentar `int 14` para fingir um
  page fault, a CPU recusa com #GP. O breakpoint às vezes usa DPL 3 para que
  depuradores de programas comuns funcionem; não precisamos disso agora.
- **Tipo `0xE`, "interrupt gate":** a alternativa seria `0xF`, "trap gate".
  A única diferença: o *interrupt gate* **desliga as interrupções de
  hardware ao entrar** no tratador, para que um tique do timer não
  interrompa o tratamento de um erro. É a escolha mais segura e a que quase
  todo kernel usa.

### O seletor: por que a GDT veio antes

O campo `seletor` diz em que segmento de código o tratador vai rodar, e
precisa ser o nosso `0x08`. Este é o motivo de o doc 03 vir primeiro: a IDT
do firmware apontava para o seletor `0x38` dele, que não existe na nossa
GDT.

### Montando uma linha

```rust
fn new(tratador: u64, ist: u16) -> Entrada {
    Entrada {
        endereco_0_15: tratador as u16,              // os 16 bits de baixo
        seletor: gdt::KERNEL_CODE,
        ist: ist as u8,
        tipo_atributos: INTERRUPT_GATE,              // 0x8E
        endereco_16_31: (tratador >> 16) as u16,     // descarta 16 bits, pega os próximos 16
        endereco_32_63: (tratador >> 32) as u32,
        _reservado: 0,
    }
}
```

Lembre de [rust-basico §1](rust-basico.md#1-tipos-numéricos-o-tamanho-é-explícito):
converter com `as u16` **descarta os bits altos**. É exatamente o que
queremos para picar o endereço.

---

## 4. O que a CPU faz ao receber uma exceção

Este é o coração do capítulo. Quando, por exemplo, uma instrução tenta ler
um endereço inválido, a CPU executa sozinha, em hardware, esta sequência:

```
 1. consulta a linha 14 da IDT
 2. se a linha pede IST n: troca RSP para tss.ist[n-1]       (pilha de emergência)
 3. alinha RSP num múltiplo de 16
 4. empilha: SS, RSP antigo, RFLAGS, CS, RIP                 (o "quadro de interrupção")
 5. se a exceção tem código de erro: empilha o código de erro
 6. se a linha é um "interrupt gate": desliga as interrupções (IF = 0)
 7. salta para o endereço do tratador
```

O tratador, quando termina, executa a instrução **`iretq`** (*interrupt
return*), que faz o caminho inverso: desempilha RIP, CS, RFLAGS, RSP e SS
de uma vez e volta exatamente para onde estava, com o estado restaurado.
Dois detalhes importantes:

- **não é `ret`**: um `ret` comum desempilharia só o endereço de retorno, e
  a pilha ficaria desalinhada;
- o código de erro, se existir, **precisa ser removido da pilha antes do
  `iretq`**, senão o `iretq` interpretaria o código de erro como se fosse o
  RIP.

### O quadro em Rust

```rust
#[repr(C)]
pub struct QuadroInterrupcao {
    pub rip: u64,      // onde o código estava (ou a próxima instrução, nas traps)
    pub cs: u64,       // segmento de código de quem foi interrompido
    pub rflags: u64,   // as flags da CPU naquele momento
    pub rsp: u64,      // a pilha de quem foi interrompido
    pub ss: u64,
}
```

Os campos estão na ordem em que ficam na memória, do endereço mais baixo
(o último empilhado, RIP) para o mais alto (o primeiro empilhado, SS).

Para imprimir o quadro com `{}` em mensagens, implementamos a trait
`Display`, o equivalente ao `toString()` do Java
([rust-basico §5](rust-basico.md#5-traits-interfaces)):

```rust
impl fmt::Display for QuadroInterrupcao {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RIP={:#x} CS={:#x} RFLAGS={:#x} RSP={:#x} SS={:#x}",
               self.rip, self.cs, self.rflags, self.rsp, self.ss)
    }
}
```

`{:#x}` formata em hexadecimal com o prefixo `0x`.

---

## 5. Escrevendo tratadores em Rust: `extern "x86-interrupt"`

Uma função Rust comum **não serve** como tratador, por três motivos:

1. **Ela termina em `ret`**, não em `iretq`.
2. **Ela pode alterar registradores sem salvá-los.** Numa chamada de função
   comum existe um combinado (a *convenção de chamada*): quem chama sabe que
   alguns registradores podem ser alterados e salva o que precisa. Mas aqui
   **ninguém chamou**: a CPU interrompeu, no meio de uma instrução qualquer,
   um código que não esperava por isso. Se o tratador alterar RAX, o código
   interrompido volta com RAX trocado e se comporta de forma imprevisível.
   O tratador precisa salvar e restaurar **todos** os registradores que usar.
3. **Ela não sabe** que há um quadro e, às vezes, um código de erro na
   pilha.

Fazer isso à mão exige um trecho de assembly para cada tratador. O Rust
*nightly* oferece um atalho: a **convenção de chamada `x86-interrupt`**,
ligada no topo do `main.rs`:

```rust
#![feature(abi_x86_interrupt)]
```

Com ela, basta declarar a função assim:

```rust
extern "x86-interrupt" fn breakpoint(quadro: QuadroInterrupcao) { ... }
extern "x86-interrupt" fn page_fault(quadro: QuadroInterrupcao, codigo: u64) { ... }
extern "x86-interrupt" fn double_fault(quadro: QuadroInterrupcao, codigo: u64) -> ! { ... }
```

e o compilador gera tudo: salva os registradores usados, entrega o quadro
como parâmetro, retira o código de erro da pilha e termina com `iretq`.

> **Cuidado: a assinatura tem que bater com o que a CPU empilha.** Se você
> declarar `codigo: u64` numa exceção que **não** empilha código de erro
> (ou esquecer numa que empilha), o tratador vai ler a pilha deslocada em 8
> bytes, e o `iretq` vai voltar para um endereço aleatório. Por isso a
> coluna "Empilha código de erro?" da tabela da seção 2 é tão importante.
> O compilador não tem como verificar isso.

`extern "..."` em Rust especifica a **convenção de chamada** de uma
função: como os parâmetros são passados e como ela retorna. Você já usou
algo parecido sem perceber: quando Java chama código nativo via JNI, existe
uma convenção específica para isso.

---

## 6. Os tratadores, um a um

### 6.1 Montando a tabela

```rust
static IDT: Once<[Entrada; 256]> = Once::new();

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut t = [Entrada::VAZIA; 256];      // 256 linhas "não presentes"
        t[DIVISAO]         = Entrada::new(divisao as *const () as u64, 0);
        t[BREAKPOINT]      = Entrada::new(breakpoint as *const () as u64, 0);
        t[OPCODE_INVALIDO] = Entrada::new(opcode_invalido as *const () as u64, 0);
        t[DOUBLE_FAULT]    = Entrada::new(double_fault as *const () as u64, gdt::IST_DOUBLE_FAULT);
        t[PROTECAO_GERAL]  = Entrada::new(protecao_geral as *const () as u64, 0);
        t[PAGE_FAULT]      = Entrada::new(page_fault as *const () as u64, 0);
        t
    });
    // ... lidt (seção 7)
}
```

- `divisao as *const () as u64` converte **a função** no **endereço** dela.
  O Rust exige o passo intermediário por um ponteiro (`*const ()`) para
  deixar claro que estamos tratando a função como endereço.
- Só o double fault recebe IST (`gdt::IST_DOUBLE_FAULT` = 1); os outros
  usam a pilha atual (0).
- A tabela fica num `Once` pelo mesmo motivo da GDT (doc 03, seção 6):
  precisa ter endereço fixo para sempre e é montada em tempo de execução.

### 6.2 #BP: breakpoint, o único que continua

```rust
extern "x86-interrupt" fn breakpoint(quadro: QuadroInterrupcao) {
    kprintln!("[excecao] BREAKPOINT (#3) em {:#x} - continuando", quadro.rip);
}
```

Imprime e **retorna**. Saída do `./run.sh teste-breakpoint`:

```
Teste: executando int3...
[excecao] BREAKPOINT (#3) em 0x608984e - continuando
Teste: voltamos do breakpoint normalmente.
```

Esse teste é a prova de que **o caminho completo** funciona: a CPU salvou
o estado, consultou a IDT, chamou o tratador, e o `iretq` devolveu o
controle exatamente para a instrução depois do `int3`, com todos os
registradores intactos.

É assim que depuradores como o GDB funcionam: para pôr um breakpoint numa
linha, eles trocam o primeiro byte da instrução por `0xCC`, o código de
máquina do `int3`.

### 6.3 #DE e #UD: erros fatais simples

```rust
extern "x86-interrupt" fn divisao(quadro: QuadroInterrupcao) {
    panic!("EXCECAO #0 DIVISAO POR ZERO\n{}", quadro);
}
```

Esses erros não têm conserto, então o tratador simplesmente chama `panic!`,
reaproveitando todo o trabalho do doc 02 (serial, tela vermelha, CPU
parada). Saída:

```
!!! KERNEL PANIC !!!
Local: src/idt.rs:131:5
Mensagem: EXCECAO #0 DIVISAO POR ZERO
RIP=0x608e546 CS=0x8 RFLAGS=0x46 RSP=0x7e96130 SS=0x10
```

**Atenção ao ler:** o `Local:` é a linha do `panic!` dentro do `idt.rs`, não
onde o erro aconteceu. **O local real do erro é o `RIP`.** Para traduzir um
RIP em "arquivo e linha", vamos usar o GDB ou a ferramenta `addr2line` mais
adiante (capítulo 10 do livro).

**Por que o teste de divisão usa assembly?** Em Rust, `x / 0` com inteiros
é verificado pelo compilador e vira panic **antes** de chegar à CPU. Para
ver a exceção da CPU, precisamos executar a instrução `div` diretamente:

```rust
core::arch::asm!("xor edx, edx", "mov eax, 1", "xor ecx, ecx", "div ecx", ...);
//                 EDX = 0        EAX = 1       ECX = 0        EDX:EAX / ECX → divide por 0
```

O #UD é testado com `ud2`, uma instrução que existe justamente para ser
inválida: a Intel garante que ela sempre gera #UD.

### 6.4 #GP: proteção geral

É a exceção "genérica" do x86: a CPU a usa para dezenas de situações do tipo
"isso não é permitido". As mais comuns:

- carregar um seletor inválido num registrador de segmento;
- acessar um **endereço não-canônico** (explicado abaixo);
- executar uma instrução privilegiada no anel 3;
- disparar um vetor sem tratador na IDT.

O **código de erro** ajuda a diferenciar. Quando não é zero, ele identifica
o seletor ou vetor envolvido, no mesmo formato de seletor do doc 03:

```
bits 3-15: índice    bits 1-2: qual tabela (00 = GDT, 01 ou 11 = IDT, 10 = LDT)
```

```rust
extern "x86-interrupt" fn protecao_geral(quadro: QuadroInterrupcao, codigo: u64) {
    if codigo == 0 {
        panic!("EXCECAO #13 PROTECAO GERAL (codigo=0: nenhum seletor envolvido)\n{}", quadro);
    }
    let origem = match (codigo >> 1) & 0b11 {
        0b00 => "GDT",
        0b10 => "LDT",
        _ => "IDT",
    };
    panic!("EXCECAO #13 PROTECAO GERAL (codigo={:#x}: indice {} na {})\n{}",
           codigo, codigo >> 3, origem, quadro);
}
```

**Endereço não-canônico.** Ponteiros em x86-64 têm 64 bits, mas a CPU usa
só 48 deles para endereços. A regra é que os bits 48 a 63 sejam **cópias do
bit 47**. Isso divide o espaço em duas metades válidas, com um enorme
"buraco" proibido no meio:

```
0xFFFF_FFFF_FFFF_FFFF ┐
                      │ metade de cima (válida) ← o kernel costuma morar aqui
0xFFFF_8000_0000_0000 ┘
                       ╳ buraco: endereços "não-canônicos" → #GP
0x0000_7FFF_FFFF_FFFF ┐
                      │ metade de baixo (válida) ← programas comuns
0x0000_0000_0000_0000 ┘
```

O teste `teste-gpf` lê o endereço `0x8000_0000_0000_0000`, que está no
buraco. A CPU nem tenta traduzir: dá #GP direto, com código 0 (não há
seletor envolvido).

### 6.5 #PF: page fault

Acontece quando a CPU tenta traduzir um endereço e não encontra a página
correspondente, ou encontra mas o acesso não é permitido. A tradução de
endereços (paginação) é o tema do capítulo 7 do livro; por enquanto,
basta saber que a memória é dividida em páginas de 4 KiB e que o firmware
deixou mapeada só a parte que existe.

Duas informações tornam o diagnóstico preciso:

- **CR2**: um registrador de controle onde a CPU grava **o endereço que se
  tentou acessar**;
- **o código de erro**, em que cada bit conta uma parte da história:

| Bit | Se 1 | Se 0 |
|---|---|---|
| 0 (P) | a página existe, mas a permissão foi violada | a página não está mapeada |
| 1 (W) | foi uma escrita | foi uma leitura |
| 2 (U) | aconteceu em modo usuário (anel 3) | aconteceu no kernel |
| 3 (R) | um bit reservado estava ligado numa tabela de páginas (tabela corrompida) | — |
| 4 (I) | foi a busca de uma instrução (tentou executar) | — |

```rust
extern "x86-interrupt" fn page_fault(quadro: QuadroInterrupcao, codigo: u64) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, ...) };   // lê o endereço culpado
    panic!("EXCECAO #14 PAGE FAULT ao {} {:#x} ({}, {}{})\n{}",
           if codigo & 0x10 != 0 { "executar" } else if codigo & 0x2 != 0 { "escrever em" } else { "ler" },
           cr2,
           if codigo & 0x1 != 0 { "pagina presente, sem permissao" } else { "pagina nao mapeada" },
           if codigo & 0x4 != 0 { "modo usuario" } else { "modo kernel" },
           if codigo & 0x8 != 0 { ", bit reservado na tabela" } else { "" },
           quadro);
}
```

Saída do `./run.sh teste-pagefault`:

```
EXCECAO #14 PAGE FAULT ao ler 0x400000000000 (pagina nao mapeada, modo kernel)
```

`0x4000_0000_0000` (64 TiB) é um endereço **canônico** (bit 47 = 0, então
está na metade de baixo), mas o firmware não mapeou nada lá. Por isso dá
#PF e não #GP.

Hoje o tratador só relata. No capítulo 7 ele pode ficar inteligente: se a
página que faltou pertence a uma região válida (por exemplo, a pilha
crescendo), o kernel mapeia uma página nova e **retorna**. A CPU repete a
instrução, que agora funciona, e o código interrompido nem percebe.

### 6.6 #DF: double fault, e a pilha de emergência em ação

Um double fault acontece quando **ocorre uma exceção enquanto a CPU tenta
entregar outra**. É o cenário descrito no doc 03, seção 5.2, e o teste
`teste-doublefault` o reproduz de propósito:

```rust
core::arch::asm!("mov rsp, 0x400000000000", "push rax", options(noreturn));
```

- `mov rsp, 0x400000000000`: aponta a pilha para memória que não existe.
- `push rax`: tenta escrever na pilha.

O que a CPU faz:

```
 push rax → escrita em memória inexistente → #PF
   └─ para entregar o #PF, a CPU precisa empilhar o quadro...
      na MESMA pilha inexistente (a linha do #PF não usa IST) → falha de novo
        └─ a CPU desiste do #PF e gera #DF
           └─ a linha do #DF tem ist = 1:
              a CPU TROCA para a pilha de emergência ANTES de empilhar
              └─ empilha o quadro na pilha boa → chama o tratador ✔
```

O tratador:

```rust
extern "x86-interrupt" fn double_fault(quadro: QuadroInterrupcao, _codigo: u64) -> ! {
    let rsp_atual: u64;
    unsafe { asm!("mov {}, rsp", out(reg) rsp_atual, ...) };
    panic!("EXCECAO #8 DOUBLE FAULT\n{}\nTratador rodando na pilha IST1 (RSP={:#x})",
           quadro, rsp_atual);
}
```

Saída:

```
Pilha IST1 (double fault): topo em 0x60ad070          ← impresso no boot
...
EXCECAO #8 DOUBLE FAULT
RIP=0x608c827 CS=0x8 RFLAGS=0x6 RSP=0x400000000000 SS=0x10
Tratador rodando na pilha IST1 (RSP=0x60acf78)
```

Como ler isso:

- `RSP=0x400000000000` no quadro: a pilha **destruída** de quem falhou,
  exatamente o valor que colocamos;
- `RSP=0x60acf78` do tratador: 248 bytes abaixo do topo da pilha de
  emergência (`0x60ad070`). **A troca de pilha aconteceu.**

**A prova contrária também foi feita.** Trocamos temporariamente o
`gdt::IST_DOUBLE_FAULT` por `0` (sem pilha de emergência) e rodamos o mesmo
teste. O QEMU **fechou 2 segundos depois do boot, sem nenhuma mensagem**: a
última linha na serial era "Teste: apontando RSP...". Foi um **triple
fault**: o #DF também tentou usar a pilha inexistente, falhou, e a CPU
reiniciou. (Com `-no-reboot`, o QEMU fecha em vez de reiniciar.) Depois do
teste, o código foi restaurado.

Dois detalhes:

- O tipo de retorno `-> !` é obrigatório: a Intel define que não há como
  retomar a execução depois de um double fault.
- Nem toda combinação de duas exceções gera #DF, só os pares graves (por
  exemplo, #PF durante a entrega de #PF, ou #GP durante a entrega de #GP).
  Um #DE durante a entrega de um #PF é tratado em sequência, normalmente.

---

## 7. Carregando a IDT

Igual à GDT (doc 03, seção 7): a instrução `lidt` recebe uma estrutura de 10
bytes com o **limite** (tamanho − 1 = 256 × 16 − 1 = **4095**) e o
**endereço** da tabela. Reaproveitamos a struct `gdt::PonteiroTabela`:

```rust
let ponteiro = gdt::PonteiroTabela {
    limite: (size_of::<[Entrada; 256]>() - 1) as u16,
    base: idt.as_ptr() as u64,
};
unsafe { asm!("lidt [{}]", in(reg) &ponteiro, ...) };
```

Diferente da GDT, não é preciso "recarregar" nada: a partir da próxima
exceção, a CPU já consulta a tabela nova.

---

## 8. As interrupções de hardware continuam desligadas

Uma dúvida comum: "se o `kernel_main` começa com `cli` (interrupções
desligadas), como as exceções funcionam?"

- O `cli` desliga só as **interrupções de hardware** (bit IF).
- As **exceções não podem ser desligadas**: elas são consequência da
  instrução atual, e a CPU não teria como continuar sem tratá-las.

As interrupções de hardware continuam desligadas até o capítulo 9, porque
ainda não configuramos o **controlador de interrupções** (o chip que decide
qual vetor cada dispositivo usa). Se as ligássemos agora, o timer do
firmware ainda poderia disparar num vetor sem tratador, e cairíamos em #GP.

---

## 9. Resultados dos testes

Todos rodados no QEMU com `SEM_TELA=1 ./run.sh <feature>`:

| Feature | Resultado |
|---|---|
| (nenhuma) | boot normal; IDT carregada com limite 4095 |
| `teste-breakpoint` | tratador imprime o RIP e o kernel **continua** |
| `teste-divisao` | #DE com o RIP da instrução `div` |
| `teste-opcode` | #UD com o RIP do `ud2` |
| `teste-gpf` | #GP com código 0 (endereço não-canônico) |
| `teste-pagefault` | #PF com CR2 = `0x400000000000`: "não mapeada, leitura, kernel" |
| `teste-doublefault` | #DF tratado na pilha IST1 (RSP dentro dela) |
| `teste-doublefault` **sem IST** (teste temporário) | triple fault: QEMU fecha em 2 s sem mensagem |

Em todos os casos houve um único boot, sem reinício inesperado.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do anterior. Os docs 00 a 06 estão
no primeiro commit do repositório: o link de cada arquivo leva ao código
**no fim do doc 06**, que pode já ter o que os capítulos seguintes acrescentam.

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/idt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/idt.rs) | **novo**: `Entrada`, `QuadroInterrupcao`, `init`, `estado` e os tratadores #DE #BP #UD #DF #GP #PF | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `#![feature(abi_x86_interrupt)]` | topo, depois de `#![no_main]` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `mod idt;`; em `kernel_main`: `idt::init()`, o `kprintln!` da IDT e `testes_de_excecao()` | logo depois do bloco da GDT |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `fn testes_de_excecao()` | depois de `kernel_main` |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/Cargo.toml) | `teste-breakpoint`, `teste-divisao`, `teste-opcode`, `teste-pagefault`, `teste-gpf`, `teste-doublefault` | `[features]` |

---

## Diferença em relação ao livro

O livro usa a crate `x86_64`, que tem `InterruptDescriptorTable` e
`InterruptStackFrame` prontos. Uma vantagem dela: seus tipos **impedem**
registrar um tratador com a assinatura errada (com ou sem código de erro).
O SiliconOS monta a tabela à mão, como fez com a GDT, então **essa
responsabilidade é nossa**: confira a tabela da seção 2 antes de adicionar
um tratador novo.

---

## Resumo

- **Exceções** são erros detectados pela **CPU** (não pelo Rust). Elas não
  passam pelo panic handler; precisam da **IDT**.
- A IDT tem **256 linhas**. Cada uma aponta para um tratador, com o
  seletor de código (`0x08`) e, opcionalmente, uma pilha IST.
- Ao receber uma exceção, a CPU empilha o **quadro** (RIP, CS, RFLAGS, RSP,
  SS) e às vezes um **código de erro**; o tratador volta com **`iretq`**.
- **`extern "x86-interrupt"`** faz o compilador gerar esse protocolo; a
  assinatura precisa bater com o que a CPU empilha.
- **Fault** repete a instrução, **trap** continua depois dela, **abort** não
  continua.
- O **double fault** usa a pilha IST1 e, por isso, uma pilha destruída vira
  uma mensagem em vez de um reinício. Isso foi provado nos dois sentidos.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que um acesso a `v[10]` gera panic, mas um acesso ao endereço
   `0x400000000000` gera page fault?**
   No primeiro, o Rust inseriu uma verificação de limite antes do acesso.
   No segundo, é um ponteiro cru: não há verificação do Rust, e quem
   descobre o problema é a CPU, ao tentar traduzir o endereço.

2. **Depois do tratador de breakpoint, a execução continua na instrução
   `int3` ou na seguinte? E depois de um page fault corrigido?**
   Breakpoint é *trap*: continua na seguinte. Page fault é *fault*: a CPU
   repete a instrução que falhou.

3. **O que aconteceria se o tratador de page fault fosse declarado sem o
   parâmetro `codigo`?**
   O tratador leria a pilha deslocada em 8 bytes, e o `iretq` interpretaria
   o código de erro como endereço de retorno: a execução saltaria para um
   lugar aleatório.

4. **Por que `0x8000_0000_0000_0000` dá #GP e `0x4000_0000_0000` dá #PF?**
   O primeiro é não-canônico (os bits altos não repetem o bit 47); a CPU o
   rejeita antes de traduzir. O segundo é canônico, mas não está mapeado.

5. **No teste de double fault, como sabemos que a pilha IST1 foi usada?**
   O RSP lido dentro do tratador (`0x60acf78`) está logo abaixo do topo da
   IST1 informado no boot (`0x60ad070`). E, sem a IST, o mesmo teste vira
   triple fault.

</details>
