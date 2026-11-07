# 10: Modo usuário e chamadas de sistema

> **Você vai entender:** por que um sistema operacional não deixa os
> programas rodarem com o mesmo poder do kernel; o que a CPU muda quando o
> código passa para o **anel 3**; como o kernel entra no anel 3 (com um
> `iretq` "falso") e como o programa volta a ele (com uma **chamada de
> sistema**, `int 0x80`); por que **nenhum ponteiro vindo do programa** pode
> ser usado sem conferir; e como o kernel sobrevive a um programa que lê
> memória proibida, executa uma instrução privilegiada ou nunca termina.
>
> **Arquivos:** `src/usuario/mod.rs` (entrar e sair do anel 3, chamadas de
> sistema), `src/usuario/programas.rs` (os programas de teste, em assembly),
> `src/gdt.rs` (TSS.RSP0), `src/idt.rs` (vetor 0x80 e exceções no anel 3),
> `mapear_em` em `src/memoria/paginacao.rs`, os comandos `programas` e
> `rodar` em `src/shell.rs`
> **Livro:** capítulo 11, seção 11.4 (o roteiro para modo usuário e
> chamadas de sistema)
> **Testar:** `./run.sh`, depois `programas` e `rodar ola` (ou `relogio`,
> `ponteiro`, `invasor`, `privilegiado`, `laco`); `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 03 (anéis, GDT e TSS), doc 04 (IDT), doc 06 (bits das
> páginas) e doc 08 (timer)
> **Rust novo aqui:** funções *naked* e `naked_asm!` ([§35](rust-basico.md#35-funções-naked-e-naked_asm)),
> `global_asm!` e símbolos externos ([§36](rust-basico.md#36-global_asm-e-símbolos-externos)),
> convenções de chamada `extern "..."` ([§37](rust-basico.md#37-convenções-de-chamada-extern-sysv64))
>
> **Código deste capítulo:** [estado no fim do doc 10](https://github.com/moises-dev-git/siliconOS/tree/doc-10) · [o que mudou desde o doc 09](https://github.com/moises-dev-git/siliconOS/compare/doc-09...doc-10) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

Até aqui, **todo** código do SiliconOS roda no anel 0, o nível de privilégio
máximo da CPU (doc 03). O shell, o heap, os tratadores: tudo pode ler e
escrever qualquer endereço, desligar as interrupções, reprogramar o APIC ou
trocar as tabelas de página. Isso é o certo para o kernel, mas seria um
desastre para os **programas**:

- um programa com um bug (um ponteiro errado) estragaria a memória do
  kernel, e a máquina inteira cairia;
- um programa malicioso leria a memória de outros programas, ou desligaria
  as interrupções e nunca mais devolveria a CPU;
- qualquer erro de qualquer programa seria um *kernel panic*.

Num Java ou Python você nunca pensa nisso porque o sistema operacional já
resolveu: o seu programa roda no **modo usuário** (anel 3), e se ele acessar
um endereço inválido o sistema o encerra com `Segmentation fault`, sem
afetar o resto. Este capítulo constrói exatamente esse mecanismo: o
SiliconOS passa a rodar **programas no anel 3**, isolados, que só conseguem
falar com o kernel por uma porta controlada.

---

## 2. A ideia: três barreiras e uma porta

A CPU sabe em que anel está pelo **CPL** (*current privilege level*), que
são os 2 bits baixos do registrador CS. No anel 3, três coisas mudam ao
mesmo tempo:

| Barreira | O que a CPU faz | Visto em |
|---|---|---|
| **Instruções privilegiadas** | `cli`, `sti`, `hlt`, `lgdt`, `mov cr3`, `in`/`out`... viram **#GP** | doc 03, glossário |
| **Páginas** | só pode acessar páginas com o bit **USUARIO** em todos os níveis da tabela; senão, **page fault** | doc 06 |
| **Portas da IDT** | `int N` só funciona se a entrada N da IDT tiver **DPL 3**; senão, **#GP** | doc 04 |

A terceira barreira é também a **porta**: o kernel marca **uma** entrada da
IDT (a 0x80) como acessível pelo anel 3. Um `int 0x80` no programa é uma
**chamada de sistema**: a CPU sobe para o anel 0 e pula para o tratador
escolhido pelo kernel. O programa escolhe **o que** pedir (o número em RAX),
mas não **onde** entrar no kernel.

```
                  ANEL 3 (programa)                       ANEL 0 (kernel)
             ┌──────────────────────────┐          ┌──────────────────────────────┐
  rodar ola  │                          │          │                              │
  ──────────►│  lea rdi, [mensagem]     │          │                              │
   iretq     │  mov esi, tamanho        │ int 0x80 │  entrada_syscall:            │
  (CS=0x23)  │  mov eax, 1  (escrever) ─┼─────────►│   salva registradores        │
             │                          │          │   tratar_syscall: confere o  │
             │  ◄───────────────────────┼──────────┤   ponteiro, escreve na tela  │
             │  (RAX = bytes escritos)  │  iretq   │                              │
             │  mov edi, 42             │          │                              │
             │  xor eax, eax (sair)    ─┼─────────►│  encerrar(Saiu(42))          │
             └──────────────────────────┘ int 0x80 │   └─► volta para o shell     │
                                                   └──────────────────────────────┘
             ✗ cli            → #GP          → o kernel encerra o programa
             ✗ ler o kernel   → page fault   → o kernel encerra o programa
             ✗ laço infinito  → o timer interrompe → passou de 2 s → encerra
```

---

## 3. Preparando o terreno

Quase tudo de que o anel 3 precisa já existia, montado nos docs 03 a 06 "para
o futuro". Faltavam três detalhes.

### 3.1 Os segmentos de usuário (já estavam na GDT)

A GDT do doc 03 já tinha, nas entradas 3 e 4, os descritores de dados e de
código com **DPL 3**. Os seletores carregam o nível pedido (RPL) nos 2 bits
baixos:

```rust
pub const USER_DATA: u16 = (3 << 3) | 3; // 0x1B: entrada 3, RPL 3
pub const USER_CODE: u16 = (4 << 3) | 3; // 0x23: entrada 4, RPL 3
```

Quando o CS vale `0x23`, a CPU está no anel 3. É esse número que vai
aparecer nas mensagens de erro dos programas, como prova.

### 3.2 A pilha para voltar ao anel 0: TSS.RSP0

Quando uma interrupção chega com a CPU no anel 3, ela **não pode** usar a
pilha do programa: o programa controla o RSP e poderia apontá-lo para
qualquer lugar (ou para a memória de outro programa). Então a CPU troca de
pilha sozinha antes de empilhar o quadro da interrupção, e pega o endereço
da nova pilha no campo **RSP0** da TSS (doc 03). Até aqui ele valia zero, o
que não importava porque nada rodava no anel 3. Agora:

```rust
// src/gdt.rs
/// Pilha do anel 0 para interrupções vindas do anel 3 (TSS.rsp[0]).
/// Programas de usuário nunca a veem: a CPU troca de pilha sozinha.
static mut PILHA_ANEL0: Pilha = Pilha([0; TAM_PILHA_IST]);
...
        let topo_anel0 = &raw const PILHA_ANEL0 as u64 + TAM_PILHA_IST as u64;
        Tss {
            _reservado0: 0,
            rsp: [topo_anel0, 0, 0],
```

```
Pilha do anel 0 para o anel 3 (TSS.RSP0): topo em 0x607a0a0
```

Isso vale para **toda** interrupção vinda do anel 3: o `int 0x80`, as
exceções e também o timer e o teclado, que continuam chegando enquanto o
programa roda.

### 3.3 O bit USUARIO em todos os níveis

O doc 06 explicou que a CPU combina as permissões dos 4 níveis da tabela de
páginas: para o anel 3 acessar uma página, o bit **USUARIO** precisa estar
ligado na PML4, na PDPT, na PD **e** na PT. `mapear_em` já ligava o bit nas
tabelas intermediárias que **criava**. Faltava o caso de uma tabela que já
existia, criada antes para o kernel:

```rust
// src/memoria/paginacao.rs, em mapear_em, no laço que desce os níveis
} else if e.grande() {
    return Err(ErroMapeamento::JaMapeado);
} else if bits & USUARIO != 0 && e.0 & USUARIO == 0 {
    // A tabela já existia, criada para o kernel. Para o anel 3 chegar
    // à página, TODOS os níveis precisam do bit USUARIO (doc 10).
    *e = Entrada(e.0 | USUARIO);
}
```

Ligar o bit numa tabela intermediária não abre nada sozinho: as páginas do
kernel abaixo dela continuam sem o bit na entrada final, e a CPU exige os
quatro.

### 3.4 Onde o programa mora

Os programas usam uma faixa da **metade baixa** do espaço de endereços, que
nada do kernel usa:

```
0x0000_2000_0000_0000   código do programa (1 página: USUARIO, só leitura)   PML4 64
0x0000_2000_0010_0000   pilha do programa  (1 página: USUARIO, ESCRITA, NX)
...
0xFFFF_8000_0000_0000   toda a RAM física, sem USUARIO (doc 06)              PML4 256
0xFFFF_9000_0000_0000   heap do kernel, sem USUARIO (doc 07)                 PML4 288
```

A página de código não tem ESCRITA (o programa não pode se modificar), e a
de pilha tem NX (não dá para executar dados da pilha), as mesmas proteções
do heap do doc 07.

---

## 4. Os programas

Ainda não existe sistema de arquivos nem formato de executável, então os
programas são **pedaços de código de máquina dentro do próprio kernel**,
escritos em assembly com `global_asm!` ([§36](rust-basico.md#36-global_asm-e-símbolos-externos)).
O kernel nunca executa esses bytes onde eles estão: copia-os para a página
de código do programa e só então salta para lá. Por isso eles ficam na seção
`.rdata` (dados somente leitura) e não em `.text`.

```rust
// src/usuario/programas.rs
global_asm!(
    ".section .rdata",

    // ---- ola: escreve uma mensagem e sai com código 42 ----
    ".global programa_ola_inicio",
    ".global programa_ola_fim",
    "programa_ola_inicio:",
    "    lea rdi, [rip + ola_msg]", // ponteiro para a mensagem
    "    mov esi, offset OLA_TAM",  // tamanho (offset = o número em si, não "ler a memória ali")
    "    mov eax, 1",               // escrever
    "    int 0x80",
    "    mov edi, 42",              // código de saída
    "    xor eax, eax",             // sair
    "    int 0x80",
    "ola_msg: .ascii \"Ola do anel 3! (escrito por uma chamada de sistema)\\n\"",
    ".set OLA_TAM, . - ola_msg",   // "." = posição atual: fim - início da mensagem
    "programa_ola_fim:",
    ...
```

Duas regras para esse código:

1. **Independente de posição.** O programa foi montado num endereço (dentro
   do kernel) e roda noutro (`0x2000_0000_0000`). Por isso a mensagem é
   achada com `lea rdi, [rip + ola_msg]`: "o endereço de `ola_msg` relativo
   à instrução atual", que continua certo depois de copiado.
2. **Nada de chamar o kernel.** O programa não pode chamar `kprintln!`: a
   página do kernel nem está acessível. A única saída é `int 0x80`.
3. **Rótulos numéricos a partir de `2:`.** Rótulos feitos só de `0` e `1`
   (`1:`, `1b`) podem ser confundidos com números binários pelo montador do
   LLVM. Num `asm!` dentro de função, o Rust os recusa (o lint
   `binary_asm_labels`, erro por padrão); no `global_asm!` ele não reclama
   (num teste, `1: jmp 1b` saiu certo), mas o projeto usa `2:`, `3:`,
   `4:`... nos dois, por segurança.

### 4.1 Um bug de verdade: `offset`

A primeira versão tinha `mov esi, OLA_TAM`. O programa `ola` caiu na hora:

```
silicon> rodar ola
[kernel] PAGE FAULT no programa (RIP=0x200000000007): ao ler 0x34 (pagina nao mapeada, modo usuario)
'ola' encerrado pelo kernel: PAGE FAULT em 0x200000000007
```

(Nessa primeira versão a mensagem ainda não mostrava o CS; ele entrou
depois, seção 7.1.)

O endereço `0x34` é 52, exatamente o tamanho da mensagem. Na sintaxe Intel,
`mov esi, NOME` com um símbolo quer dizer "**ler a memória** no endereço
NOME", e não "carregar o número NOME". O certo é `mov esi, offset OLA_TAM`.
Duas lições num só erro: a sintaxe Intel tem essa ambiguidade, e o mecanismo
deste capítulo funcionou antes mesmo de o programa funcionar: o bug do
programa virou uma mensagem, não um *kernel panic*.

### 4.2 Os seis programas

```
silicon> programas
  ola           escreve uma mensagem e sai com codigo 42 (80 bytes)
  relogio       espera 1 s perguntando os tiques ao kernel (32 bytes)
  ponteiro      passa um endereco do kernel para escrever() (89 bytes)
  invasor       tenta ler a memoria do kernel (19 bytes)
  privilegiado  tenta executar cli (7 bytes)
  laco          laco infinito (o timer o encerra em 2 s) (2 bytes)
```

Os três primeiros se comportam; os três últimos testam cada barreira. O
`laco` é o menor programa possível: `2: jmp 2b`, um salto para si mesmo, 2
bytes.

No Rust, cada programa é um par de rótulos visto como símbolo externo, e o
`Programa` guarda os dois ponteiros:

```rust
unsafe extern "C" {
    static programa_ola_inicio: u8;
    static programa_ola_fim: u8;
    ...
}

pub static PROGRAMAS: [Programa; 6] = [
    Programa { nome: "ola", descricao: "escreve uma mensagem e sai com codigo 42",
               inicio: &raw const programa_ola_inicio, fim: &raw const programa_ola_fim },
    ...
];
```

---

## 5. Entrar no anel 3

Não existe uma instrução "vá para o anel 3". O que existe é o `iretq`, a
volta de uma interrupção (doc 04): ele desempilha RIP, CS, RFLAGS, RSP e SS
e continua dali, **no anel indicado pelo CS desempilhado**. O truque, usado
por todos os sistemas x86, é montar à mão um quadro de interrupção que
nunca aconteceu e fazer o `iretq`:

```
   pilha do kernel antes do iretq        o que o iretq faz com cada valor
   ┌──────────────────────────┐
   │ SS     = 0x1B            │  ← SS do anel 3
   │ RSP    = 0x2000_0010_1000│  ← o fim da página de pilha do programa
   │ RFLAGS = 0x202           │  ← IF ligado: o timer continua chegando
   │ CS     = 0x23            │  ← código do anel 3: CPL passa a ser 3
   │ RIP    = 0x2000_0000_0000│  ← a primeira instrução do programa
   └──────────────────────────┘  ← RSP
```

```rust
// src/usuario/mod.rs
#[unsafe(naked)]
unsafe extern "sysv64" fn entrar_no_anel3(rip: u64, rsp: u64) -> u64 {
    naked_asm!(
        // Registradores que a convenção manda preservar: voltar_ao_kernel os restaura.
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov [rip + {rsp_kernel}], rsp",
        // O quadro que o iretq espera, de baixo para cima: RIP, CS, RFLAGS, RSP, SS.
        "push {ss}",
        "push rsi",
        "push {rflags}",
        "push {cs}",
        "push rdi",
        // Não deixar valores do kernel nos registradores para o programa ver.
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor edx, edx",
        "xor esi, esi", "xor edi, edi", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
        rsp_kernel = sym RSP_KERNEL,
        ss = const gdt::USER_DATA as u64,
        cs = const gdt::USER_CODE as u64,
        rflags = const 0x202u64, // bit 9 (IF): o programa roda com interrupções ligadas
    )
}
```

Partes que merecem atenção:

- **Uma função *naked*** ([§35](rust-basico.md#35-funções-naked-e-naked_asm)):
  o compilador não gera nada além do assembly escrito, nem prólogo, nem
  epílogo. Aqui isso é obrigatório: a função manipula a pilha diretamente e
  nunca retorna pelo caminho normal.
- **`extern "sysv64"`** ([§37](rust-basico.md#37-convenções-de-chamada-extern-sysv64)):
  os argumentos chegam em RDI (o RIP do programa) e RSI (o RSP). O alvo UEFI
  usa por padrão a convenção do Windows (RCX, RDX), então escolhemos uma
  convenção explicitamente para o assembly bater.
- **Salvar o RSP do kernel.** Os 6 `push` guardam os registradores que o
  chamador espera encontrar intactos, e `RSP_KERNEL` guarda onde eles
  ficaram. É o "marcador" para voltar.
- **Zerar os registradores.** Sem isso, o programa começaria com restos do
  kernel em RAX, RBX..., que podem ser endereços ou dados. Vazar
  informação é um problema de segurança, mesmo que pequeno.

O `rodar` que chama tudo isso prepara as páginas, entra, e limpa na volta:

```rust
pub fn rodar(programa: &programas::Programa) -> Resultado {
    let codigo = programa.codigo();
    assert!(codigo.len() <= TAMANHO_QUADRO as usize, "programa maior que uma pagina");

    // 1. Uma página de código (só leitura, executável) e uma de pilha
    //    (escrita, sem execução), as duas acessíveis pelo anel 3.
    let q_codigo = nova_pagina_zerada();
    let q_pilha = nova_pagina_zerada();
    ... copia o código para q_codigo ...
    paginacao::mapear(CODIGO_VIRT, q_codigo.endereco(), paginacao::TamanhoPagina::K4,
                      flags::USUARIO).expect("mapear o codigo do programa");
    paginacao::mapear(PILHA_VIRT, q_pilha.endereco(), paginacao::TamanhoPagina::K4,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO).expect("mapear a pilha");

    // 2. Entrar no anel 3. Só volta quando o programa terminar.
    ...
    unsafe { entrar_no_anel3(CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO) };
    ...
    // Voltamos de dentro de um tratador (interrupções desligadas).
    cpu::ligar_interrupcoes();

    // 3. Limpar: nada do programa fica mapeado.
    for (virt, q) in [(CODIGO_VIRT, q_codigo), (PILHA_VIRT, q_pilha)] {
        paginacao::desmapear(virt).expect("desmapear");
        quadros::liberar(q);
    }
    RESULTADO.lock().take().expect("programa terminou sem resultado")
}
```

As páginas novas são **zeradas** antes de tudo: um quadro reaproveitado tem
os dados de quem o usou antes (talvez o próprio kernel), e o programa não
deve ver isso.

---

## 6. Chamadas de sistema: `int 0x80`

### 6.1 A porta na IDT

A entrada 0x80 da IDT aponta para `entrada_syscall`, com uma diferença em
relação a todas as outras: **DPL 3**.

```rust
// src/idt.rs
/// O mesmo com DPL=3 (bits 5-6): o anel 3 pode usar `int` com este vetor.
/// Com DPL=0, um `int 0x80` vindo do anel 3 vira #GP (docs/10-modo-usuario.md).
const INTERRUPT_GATE_DPL3: u8 = 0xEE;
...
t[usuario::VETOR_SYSCALL as usize] =
    Entrada::new(usuario::entrada_syscall as *const () as u64, 0).para_usuario();
```

`0x8E` = `1000 1110` (presente, DPL 0, interrupt gate); `0xEE` =
`1110 1110`, os bits 5 e 6 viram `11` = DPL 3. As outras entradas continuam
com DPL 0: um programa que tentar `int 14` (fingir um page fault) ou `int 3`
leva #GP. O DPL controla só quem pode usar `int`; as exceções de verdade e
as interrupções de hardware chegam de qualquer anel.

### 6.2 O contrato: quais registradores

Cada sistema define a sua ABI (*application binary interface*) de chamadas.
O SiliconOS segue a escolha de registradores do Linux:

| Registrador | Na ida | Na volta |
|---|---|---|
| RAX | número da chamada | resultado (negativo = erro) |
| RDI | 1º argumento | preservado |
| RSI | 2º argumento | preservado |

| RAX | Chamada | Argumentos | Devolve |
|---|---|---|---|
| 0 | `sair` | RDI = código de saída | não volta |
| 1 | `escrever` | RDI = ponteiro, RSI = tamanho | bytes escritos, ou -14 |
| 2 | `tiques` | nenhum | tiques do timer desde o boot |
| outro | | | -38 |

Os erros negativos copiam os números do Linux: -14 é `EFAULT` ("endereço
inválido") e -38 é `ENOSYS` ("chamada não existe").

### 6.3 A entrada: salvar tudo

O tratador de uma chamada de sistema precisa **ler e alterar** os
registradores do programa (ler RAX, RDI, RSI; escrever o resultado em RAX).
A convenção `x86-interrupt` dos docs 04 e 08 não dá acesso a eles. Então a
entrada é escrita à mão:

```rust
#[unsafe(naked)]
pub extern "sysv64" fn entrada_syscall() {
    naked_asm!(
        "push rax", "push rcx", "push rdx", "push rbx", "push rbp", "push rsi", "push rdi",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",          // 1º argumento: &mut Registros
        "call {tratar}",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rdi", "pop rsi", "pop rbp", "pop rbx", "pop rdx", "pop rcx", "pop rax",
        "iretq",
        tratar = sym tratar_syscall,
    )
}
```

Depois dos 15 `push`, a pilha (a do anel 0, vinda do TSS.RSP0) contém
exatamente, do endereço mais baixo para o mais alto, os 15 registradores e
o quadro que a CPU empilhou. Uma `struct` com `#[repr(C)]` e os campos
nessa ordem **é** a pilha, e `mov rdi, rsp` passa um ponteiro para ela:

```
   endereços altos
   ┌────────┐
   │ SS     │ ┐
   │ RSP    │ │ empilhados pela CPU
   │ RFLAGS │ │ (o "quadro" do doc 04)
   │ CS     │ │
   │ RIP    │ ┘
   │ RAX    │ ← 1º push
   │ RCX    │
   │  ...   │
   │ R15    │ ← 15º push = RSP = &Registros
   └────────┘
   endereços baixos
```

```rust
#[repr(C)]
pub struct Registros {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rdi: u64, pub rsi: u64, pub rbp: u64, pub rbx: u64,
    pub rdx: u64, pub rcx: u64, pub rax: u64,
    // empilhado pela CPU:
    pub rip: u64, pub cs: u64, pub rflags: u64, pub rsp: u64, pub ss: u64,
}

extern "sysv64" fn tratar_syscall(r: &mut Registros) {
    r.rax = match r.rax {
        SYS_SAIR => encerrar(Resultado::Saiu(r.rdi)),
        SYS_ESCREVER => escrever(r.rdi, r.rsi),
        SYS_TIQUES => timer::tiques(),
        _ => ERRO_SYSCALL,
    };
}
```

O resultado escrito em `r.rax` vai para o lugar da pilha de onde o `pop rax`
vai ler: ao voltar do `iretq`, o programa encontra o resultado em RAX.

Um detalhe de alinhamento que esse arranjo resolve de graça: a convenção
exige que o RSP seja múltiplo de 16 na hora do `call`. A CPU alinha a pilha
em 16 e empilha 5 valores (40 bytes); os 15 `push` somam 120. São 160 bytes,
múltiplo de 16.

### 6.4 Nunca confiar num ponteiro do programa

`escrever(ponteiro, tamanho)` recebe um endereço **escolhido pelo
programa**. Se o kernel simplesmente lesse dali, o programa poderia pedir
"escreva na tela os 16 bytes em `0xFFFF_9000_0000_0000`" e o kernel
mostraria um pedaço do próprio heap. A barreira das páginas não ajuda,
porque quem lê é o kernel, no anel 0, e o anel 0 pode ler tudo. Então o
kernel confere antes:

```rust
/// O ponteiro [inicio, inicio + tamanho) está inteiro dentro de uma página
/// do programa? Nunca confiar num ponteiro vindo do anel 3.
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    [CODIGO_VIRT, PILHA_VIRT].iter().any(|&p| inicio >= p && fim <= p + TAMANHO_QUADRO)
}
```

Três armadilhas que a conferência cobre:

- o **início** fora das páginas do programa (o caso do `ponteiro`);
- o início dentro, mas o **fim** fora (um tamanho grande "escorrega" para a
  memória vizinha);
- `inicio + tamanho` **estourando** os 64 bits e dando a volta para um
  número pequeno, que passaria num teste ingênuo. Daí o `checked_add`.

```
silicon> rodar ponteiro
[kernel] escrever(): ponteiro 0xffff900000000000 fora da memoria do programa, recusado
o kernel recusou o ponteiro
'ponteiro' terminou: codigo de saida 1
```

O programa recebeu -14, percebeu (`test rax, rax` / `jns`) e saiu com 1.

> Em CPUs modernas existe uma proteção extra de hardware, o **SMAP**
> (*supervisor mode access prevention*): ligado no CR4, ele faz o próprio
> kernel tomar page fault ao tocar numa página USUARIO, a não ser que
> declare a intenção com a instrução `stac`. O SiliconOS ainda não usa SMAP
> (nem o SMEP, que impede o kernel de **executar** páginas de usuário).
> O monitor do QEMU mostrou `CR4=00000668`: os bits 20 (SMEP) e 21 (SMAP)
> estão desligados. Por isso a validação em software é a única proteção, e precisa estar
> certa.

---

## 7. Sair: três caminhos, um destino

Um programa termina de três formas, e as três acabam na mesma função:

| Caminho | Quem chama `encerrar` | Resultado |
|---|---|---|
| chamada `sair(codigo)` | `tratar_syscall` | `Saiu(codigo)` |
| exceção da CPU no anel 3 | o tratador da exceção (`idt.rs`) | `Falha { excecao, rip }` |
| tempo esgotado | o tratador do timer | `TempoEsgotado` |

```rust
pub fn encerrar(resultado: Resultado) -> ! {
    *RESULTADO.lock() = Some(resultado);
    unsafe { voltar_ao_kernel() }
}

#[unsafe(naked)]
unsafe extern "sysv64" fn voltar_ao_kernel() -> ! {
    naked_asm!(
        "mov rsp, [rip + {rsp_kernel}]",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "xor eax, eax",
        "ret",
        rsp_kernel = sym RSP_KERNEL,
    )
}
```

`voltar_ao_kernel` é o espelho de `entrar_no_anel3`: põe de volta o RSP
salvo, desempilha os 6 registradores e faz `ret`. Esse `ret` usa o endereço
de retorno que o `call entrar_no_anel3` deixou lá no começo, então, para o
`rodar`, é como se `entrar_no_anel3` tivesse simplesmente terminado.

A pilha do tratador em que estávamos (a do TSS.RSP0) é **abandonada** no
meio: o quadro da interrupção do programa nunca vai ser desempilhado, e
tudo bem, porque o programa não vai continuar. Na próxima vez que o anel 3
for interrompido, a CPU recomeça do topo dessa pilha.

Duas consequências:

- o `rodar` volta com as **interrupções desligadas** (estávamos dentro de
  um *interrupt gate*), e por isso religa com `cpu::ligar_interrupcoes()`;
- o tratador do timer, quando encerra um programa, **manda o EOI antes**
  (doc 08). O caminho normal (o fim do tratador) nunca vai acontecer, e sem
  o EOI o timer pararia.

### 7.1 Exceções: a culpa é do programa ou do kernel?

Cada tratador de exceção do doc 04 ganhou uma linha no começo:

```rust
extern "x86-interrupt" fn protecao_geral(quadro: QuadroInterrupcao, codigo: u64) {
    // No anel 3, a causa mais comum: uma instrução privilegiada (cli, hlt, in, out...).
    usuario::falha_do_programa(&quadro, "PROTECAO GERAL", format_args!("#GP, codigo={:#x}", codigo));
    ... o panic de antes ...
```

```rust
pub fn falha_do_programa(quadro: &QuadroInterrupcao, excecao: &'static str, detalhes: core::fmt::Arguments) {
    if quadro.cs & 3 == 3 {
        kprintln!("[kernel] {} no programa (RIP={:#x}, CS={:#x}): {}", excecao, quadro.rip, quadro.cs, detalhes);
        encerrar(Resultado::Falha { excecao, rip: quadro.rip });
    }
}
```

A pergunta "de quem é a culpa?" se responde pelo **CS empilhado** no quadro:
se os 2 bits baixos são 3, a CPU estava no anel 3 quando a exceção
aconteceu, e quem errou foi o programa. Ele é encerrado e o kernel segue.
Se são 0, o erro é do kernel, e continua sendo *kernel panic*, como antes.

```
silicon> rodar invasor
[kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
'invasor' encerrado pelo kernel: PAGE FAULT em 0x20000000000a
silicon> rodar privilegiado
[kernel] PROTECAO GERAL no programa (RIP=0x200000000000, CS=0x23): #GP, codigo=0x0
'privilegiado' encerrado pelo kernel: PROTECAO GERAL em 0x200000000000
```

O page fault do `invasor` mostra a barreira das páginas: o endereço
`0xFFFF_8000_0010_0000` **está** mapeado (é a RAM vista pela metade alta,
doc 06), por isso "pagina presente, sem permissao": a página existe, mas
sem o bit USUARIO. O bit 2 do código de erro, "modo usuario", é a CPU
confirmando de onde veio o acesso. O `privilegiado` morre na primeira
instrução (`cli`, RIP = início do programa) com #GP de código 0, o "você não
pode fazer isso" do doc 04.

### 7.2 O timer devolve a CPU ao kernel

O programa `laco` não erra nada: só nunca termina, e nunca chama o kernel.
Num sistema sem timer, ele seria dono da CPU para sempre. Mas o timer do
doc 08 continua interrompendo 100 vezes por segundo, inclusive no anel 3, e
o tratador agora confere:

```rust
// src/interrupcoes/timer.rs
pub extern "x86-interrupt" fn tratador(quadro: QuadroInterrupcao) {
    let t = TIQUES.fetch_add(1, Ordering::Relaxed) + 1;

    // Interrompemos um programa de usuário que já passou do limite? Então o
    // kernel retoma a CPU e o encerra (docs/10-modo-usuario.md).
    if crate::usuario::tempo_esgotado(&quadro) {
        apic::eoi();
        crate::usuario::encerrar(crate::usuario::Resultado::TempoEsgotado);
    }
    ...
```

```
silicon> rodar laco
'laco' encerrado pelo kernel: passou de 2 s sem terminar
```

Esse é o embrião da **preempção**: em vez de encerrar, um escalonador
guardaria os registradores do programa e daria a CPU a outro. É o assunto
natural do próximo passo.

---

## 8. Como foi testado

### 8.1 Pelo shell

Com o monitor do QEMU e `sendkey` (doc 08), todos os programas, um depois
do outro, e `mem` antes e depois:

```
silicon> rodar ola
Ola do anel 3! (escrito por uma chamada de sistema)
'ola' terminou: codigo de saida 42
silicon> rodar relogio
'relogio' terminou: codigo de saida 100
silicon> rodar ponteiro
[kernel] escrever(): ponteiro 0xffff900000000000 fora da memoria do programa, recusado
o kernel recusou o ponteiro
'ponteiro' terminou: codigo de saida 1
silicon> rodar invasor
[kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
'invasor' encerrado pelo kernel: PAGE FAULT em 0x20000000000a
silicon> rodar privilegiado
[kernel] PROTECAO GERAL no programa (RIP=0x200000000000, CS=0x23): #GP, codigo=0x0
'privilegiado' encerrado pelo kernel: PROTECAO GERAL em 0x200000000000
silicon> rodar laco
'laco' encerrado pelo kernel: passou de 2 s sem terminar
silicon> mem
quadros: 20305 livres de 32768 (79 MiB livres)
```

Numa execução separada, `mem` mostrou 20308 quadros livres antes do primeiro
programa, 20305 depois dele e 20305 depois do segundo. Os 3 quadros da
primeira execução são as tabelas intermediárias (PDPT, PD e PT) criadas para
a entrada 64 da PML4, que `desmapear` não libera (doc 06) e que os programas
seguintes reaproveitam. Daí em diante a contagem não muda mais: as duas
páginas de cada programa voltam para o alocador. O kernel continuou
respondendo depois de cada falha.

### 8.2 `cargo test`

Oito testes novos em `src/usuario/mod.rs`, um por programa, mais
"nada fica mapeado nem vaza" (as duas páginas desmapeadas e os quadros de
volta) e a validação de ponteiros (dentro, escorregando no fim, kernel,
estouro). Agora são 32 testes:

```
siliconos::usuario::testes::invasor_encerrado_por_page_fault ... [kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
[ok]
siliconos::usuario::testes::laco_encerrado_pelo_timer ... [ok]
siliconos::usuario::testes::nada_fica_mapeado_nem_vaza ... Ola do anel 3! (escrito por uma chamada de sistema)
[ok]
...
Todos os 32 testes passaram.
```

### 8.3 Regressão

As 21 builds dos capítulos anteriores (boot normal e 20 features) deram o
resultado esperado.

---

## O que ainda falta

Este é o modo usuário mínimo, e vale ser honesto sobre o que ele não faz:

- **Um espaço de endereços só.** O programa usa as mesmas tabelas de página
  do kernel; o que o protege é o bit USUARIO. Um sistema de verdade dá a
  cada processo a **sua própria PML4** (e troca o CR3 a cada troca de
  processo), para que dois programas possam usar o mesmo endereço sem se
  ver.
- **Um programa de cada vez,** e o shell espera ele terminar. Rodar vários
  ao mesmo tempo exige guardar o estado de cada um: um escalonador.
- **Programas embutidos no kernel.** Carregar de um arquivo exige um formato
  de executável (ELF) e um lugar de onde ler (um disco em RAM, um sistema
  de arquivos).
- **`int 0x80` é lento.** CPUs x86-64 têm as instruções `syscall`/`sysret`,
  feitas para isso, configuradas por MSRs (STAR, LSTAR, SFMASK). O livro
  recomenda começar pela interrupção de software, mais fácil de entender, e
  trocar depois que o comportamento estiver certo, que é o que fizemos.
- **SMAP e SMEP** desligados.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 09 (tag `doc-09`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-10`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-09...doc-10).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | **novo**: constantes, `Resultado`, `rodar`, `encerrar`, `falha_do_programa`, `tempo_esgotado`, `entrar_no_anel3`, `voltar_ao_kernel`, `Registros`, `entrada_syscall`, `tratar_syscall`, `faixa_do_programa`, `escrever` e os testes | — |
| [`src/usuario/programas.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/programas.rs) | **novo**: o `global_asm!` com os 6 programas, os símbolos `unsafe extern "C"`, `Programa`, `PROGRAMAS`, `procurar` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/main.rs) | `mod usuario;` | junto dos outros `mod` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/main.rs) | `kprintln!` com o topo de `gdt::topo_pilha_anel0()` | depois do `kprintln!` da pilha IST1 |
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/gdt.rs) | tirar o `#[allow(dead_code)]` de `USER_DATA` e `USER_CODE`; `static mut PILHA_ANEL0`; `rsp: [topo_anel0, 0, 0]` na TSS; `fn topo_pilha_anel0` | seletores; depois de `PILHA_DOUBLE_FAULT`; em `init`; antes de `topo_pilha_double_fault` |
| [`src/idt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/idt.rs) | `use crate::usuario;`; `INTERRUPT_GATE_DPL3`; `Entrada::para_usuario`; a entrada 0x80 em `init`; uma chamada a `usuario::falha_do_programa` no começo de `divisao`, `opcode_invalido`, `protecao_geral` e `page_fault` | veja os trechos abaixo |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/interrupcoes/timer.rs) | o parâmetro do `tratador` passa a se chamar `quadro`; o teste `tempo_esgotado` + `eoi` + `encerrar` | logo depois do `fetch_add` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/memoria/paginacao.rs) | o `else if` que liga USUARIO numa tabela que já existia | `mapear_em`, no laço que desce os níveis |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/shell.rs) | duas linhas em `COMANDOS` (`programas` e `rodar`) e as funções `programas` e `rodar_programa` | antes da linha do `reiniciar`; antes de `fn reiniciar` |

### Os trechos que o texto não mostrou

Para quem está reproduzindo sem abrir o GitHub, estas são as partes de
`src/usuario/mod.rs` que as seções acima citaram sem mostrar por inteiro:

```rust
use core::arch::naked_asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::gdt;
use crate::idt::QuadroInterrupcao;
use crate::interrupcoes::timer;
use crate::memoria::paginacao::{self, flags};
use crate::memoria::{quadros, TAMANHO_QUADRO};
use crate::{cpu, kprint, kprintln};

pub mod programas;

pub const CODIGO_VIRT: u64 = 0x0000_2000_0000_0000;
pub const PILHA_VIRT: u64 = 0x0000_2000_0010_0000;
pub const VETOR_SYSCALL: u8 = 0x80;
/// Um programa que passar disto sem devolver a CPU é encerrado pelo timer.
pub const LIMITE_TIQUES: u64 = 2 * timer::FREQUENCIA_HZ;

pub const SYS_SAIR: u64 = 0;
pub const SYS_ESCREVER: u64 = 1;
pub const SYS_TIQUES: u64 = 2;
pub const ERRO_ENDERECO: u64 = -14i64 as u64;  // EFAULT: ponteiro inválido
pub const ERRO_SYSCALL: u64 = -38i64 as u64;   // ENOSYS: chamada desconhecida

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resultado {
    Saiu(u64),
    Falha { excecao: &'static str, rip: u64 },
    TempoEsgotado,
}

static ATIVO: AtomicBool = AtomicBool::new(false);
static INICIO: AtomicU64 = AtomicU64::new(0);
static RESULTADO: Mutex<Option<Resultado>> = Mutex::new(None);
/// RSP do kernel no momento em que entrou no anel 3 (lido por voltar_ao_kernel).
static mut RSP_KERNEL: u64 = 0;
```

O miolo de `rodar` (entre preparar as páginas e entrar no anel 3), e a
função que zera um quadro novo:

```rust
    unsafe {
        let destino = paginacao::fisico_para_virtual(q_codigo.endereco()) as *mut u8;
        core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
    }
    ...
    *RESULTADO.lock() = None;
    INICIO.store(timer::tiques(), Ordering::Relaxed);
    ATIVO.store(true, Ordering::Relaxed);
    unsafe { entrar_no_anel3(CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO) };
    ATIVO.store(false, Ordering::Relaxed);

fn nova_pagina_zerada() -> quadros::Quadro {
    let q = quadros::alocar().expect("sem quadros para o programa");
    unsafe { core::ptr::write_bytes(paginacao::fisico_para_virtual(q.endereco()) as *mut u8, 0, 4096) };
    q
}
```

A pergunta que o tratador do timer faz:

```rust
pub fn tempo_esgotado(quadro: &QuadroInterrupcao) -> bool {
    quadro.cs & 3 == 3
        && ATIVO.load(Ordering::Relaxed)
        && timer::tiques() - INICIO.load(Ordering::Relaxed) >= LIMITE_TIQUES
}
```

E `escrever`, depois da validação:

```rust
fn escrever(ponteiro: u64, tamanho: u64) -> u64 {
    if !faixa_do_programa(ponteiro, tamanho) {
        kprintln!("[kernel] escrever(): ponteiro {:#x} fora da memoria do programa, recusado", ponteiro);
        return ERRO_ENDERECO;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ponteiro as *const u8, tamanho as usize) };
    for &b in bytes {
        kprint!("{}", if b.is_ascii() { b as char } else { '?' });
    }
    tamanho
}
```

Em `src/usuario/programas.rs`, o tipo `Programa`:

```rust
pub struct Programa {
    pub nome: &'static str,
    pub descricao: &'static str,
    inicio: *const u8,
    fim: *const u8,
}

// Os ponteiros apontam para dados fixos do executável: seguros entre CPUs.
unsafe impl Sync for Programa {}

impl Programa {
    /// Os bytes do código de máquina do programa.
    pub fn codigo(&self) -> &'static [u8] {
        unsafe { core::slice::from_raw_parts(self.inicio, self.fim as usize - self.inicio as usize) }
    }
}

pub fn procurar(nome: &str) -> Option<&'static Programa> {
    PROGRAMAS.iter().find(|p| p.nome == nome)
}
```

Os outros cinco programas, completos (o `ola` está na seção 4):

```rust
    // ---- invasor: tenta ler a memória do kernel ----
    ".global programa_invasor_inicio",
    ".global programa_invasor_fim",
    "programa_invasor_inicio:",
    "    movabs rax, 0xffff800000100000", // RAM física vista pelo kernel (doc 06)
    "    mov rax, [rax]",
    "    xor edi, edi",
    "    xor eax, eax",
    "    int 0x80",
    "programa_invasor_fim:",

    // ---- privilegiado: tenta desligar as interrupções ----
    ".global programa_privilegiado_inicio",
    ".global programa_privilegiado_fim",
    "programa_privilegiado_inicio:",
    "    cli",
    "    xor edi, edi",
    "    xor eax, eax",
    "    int 0x80",
    "programa_privilegiado_fim:",

    // ---- ponteiro: pede ao kernel para escrever a partir de um endereço do kernel ----
    ".global programa_ponteiro_inicio",
    ".global programa_ponteiro_fim",
    "programa_ponteiro_inicio:",
    "    movabs rdi, 0xffff900000000000", // o heap do kernel (doc 07)
    "    mov esi, 16",
    "    mov eax, 1",
    "    int 0x80",
    "    test rax, rax",             // negativo = o kernel recusou
    "    jns 4f",
    "    lea rdi, [rip + ponteiro_msg]",
    "    mov esi, offset PONTEIRO_TAM",
    "    mov eax, 1",
    "    int 0x80",
    "    mov edi, 1",
    "    xor eax, eax",
    "    int 0x80",
    "4:  xor edi, edi",
    "    xor eax, eax",
    "    int 0x80",
    "ponteiro_msg: .ascii \"o kernel recusou o ponteiro\\n\"",
    ".set PONTEIRO_TAM, . - ponteiro_msg",
    "programa_ponteiro_fim:",

    // ---- laco: nunca devolve a CPU ----
    ".global programa_laco_inicio",
    ".global programa_laco_fim",
    "programa_laco_inicio:",
    "2:  jmp 2b",
    "programa_laco_fim:",

    // ---- relogio: espera 1 s (100 tiques) perguntando ao kernel, e sai ----
    ".global programa_relogio_inicio",
    ".global programa_relogio_fim",
    "programa_relogio_inicio:",
    "    mov eax, 2",               // tiques()
    "    int 0x80",
    "    lea rbx, [rax + 100]",     // tique de chegada
    "2:  mov eax, 2",
    "    int 0x80",
    "    cmp rax, rbx",
    "    jb 2b",
    "    mov edi, 100",             // código de saída fixo: 100 tiques esperados
    "    xor eax, eax",
    "    int 0x80",
    "programa_relogio_fim:",

    ".text",
);
```

O `global_asm!` termina com `".text"` para devolver o montador à seção de
código, e os símbolos de todos os programas são declarados no mesmo bloco
`unsafe extern "C"` mostrado na seção 4.2.

E dois dos testes, para mostrar a forma (os outros seis seguem o mesmo
padrão):

```rust
#[cfg(test)]
mod testes {
    use super::*;

    fn rodar_por_nome(nome: &str) -> Resultado {
        rodar(programas::procurar(nome).unwrap())
    }

    #[test_case]
    fn invasor_encerrado_por_page_fault() {
        assert!(matches!(rodar_por_nome("invasor"), Resultado::Falha { excecao: "PAGE FAULT", .. }));
    }

    #[test_case]
    fn nada_fica_mapeado_nem_vaza() {
        // A primeira execução cria as tabelas intermediárias da entrada 64
        // da PML4 (PDPT, PD e PT: 3 quadros), que `desmapear` não libera
        // (doc 06). Rodar uma vez antes torna o teste independente da ordem.
        rodar_por_nome("ola");
        let livres = quadros::livres();
        rodar_por_nome("ola");
        assert!(paginacao::traduzir(CODIGO_VIRT).is_none());
        assert!(paginacao::traduzir(PILHA_VIRT).is_none());
        assert_eq!(quadros::livres(), livres);
    }
}
```

No `src/gdt.rs`, a função que o `main.rs` usa para mostrar o topo da pilha
(copia o array para fora da `Tss`, que é `packed`, como em
`topo_pilha_double_fault`):

```rust
/// Topo da pilha usada ao vir do anel 3 (TSS.rsp[0]).
pub fn topo_pilha_anel0() -> u64 {
    TSS.get().map(|t| { let rsp = t.rsp; rsp[0] }).unwrap_or(0)
}
```

No `src/idt.rs`, o método novo de `Entrada` e o começo do page fault (as
mensagens viram variáveis, para servir ao programa e ao panic):

```rust
    /// A mesma entrada, mas chamável pelo anel 3 (chamadas de sistema).
    fn para_usuario(mut self) -> Entrada {
        self.tipo_atributos = INTERRUPT_GATE_DPL3;
        self
    }
...
extern "x86-interrupt" fn page_fault(quadro: QuadroInterrupcao, codigo: u64) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)) };
    let acao = if codigo & 0x10 != 0 { "executar" } else if codigo & 0x2 != 0 { "escrever em" } else { "ler" };
    let causa = if codigo & 0x1 != 0 { "pagina presente, sem permissao" } else { "pagina nao mapeada" };
    let modo = if codigo & 0x4 != 0 { "modo usuario" } else { "modo kernel" };
    let reservado = if codigo & 0x8 != 0 { ", bit reservado na tabela" } else { "" };
    usuario::falha_do_programa(&quadro, "PAGE FAULT",
                               format_args!("ao {} {:#x} ({}, {}{})", acao, cr2, causa, modo, reservado));
    panic!("EXCECAO #14 PAGE FAULT ao {} {:#x} ({}, {}{})\n{}", acao, cr2, causa, modo, reservado, quadro);
}
```

Nos outros três tratadores a linha nova é igual, mudando o nome e o
detalhe: `"DIVISAO POR ZERO"` com `format_args!("#DE")`, `"OPCODE INVALIDO"`
com `format_args!("#UD")` e `"PROTECAO GERAL"` com
`format_args!("#GP, codigo={:#x}", codigo)`.

No `src/shell.rs`, os dois comandos:

```rust
    Comando { nome: "programas", uso: "programas", descricao: "lista os programas de usuario", funcao: programas },
    Comando { nome: "rodar", uso: "rodar <programa>", descricao: "roda um programa no anel 3", funcao: rodar_programa },
...
fn programas(_: &[&str]) {
    for p in &crate::usuario::programas::PROGRAMAS {
        kprintln!("  {:<14}{} ({} bytes)", p.nome, p.descricao, p.codigo().len());
    }
}

fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario::{self, Resultado};
    let Some(programa) = argumentos.first().and_then(|n| usuario::programas::procurar(n)) else {
        kprintln!("uso: rodar <programa>   (veja 'programas')");
        return;
    };
    match usuario::rodar(programa) {
        Resultado::Saiu(codigo) =>
            kprintln!("'{}' terminou: codigo de saida {}", programa.nome, codigo as i64),
        Resultado::Falha { excecao, rip } =>
            kprintln!("'{}' encerrado pelo kernel: {} em {:#x}", programa.nome, excecao, rip),
        Resultado::TempoEsgotado =>
            kprintln!("'{}' encerrado pelo kernel: passou de {} s sem terminar",
                      programa.nome, usuario::LIMITE_TIQUES / timer::FREQUENCIA_HZ),
    }
}
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `topo_pilha_anel0` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/gdt.rs) | Topo da pilha que a CPU usa ao vir do anel 3 (TSS.rsp[0]). |
| `Entrada::para_usuario` | [idt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/idt.rs) | Muda o DPL da entrada para 3 (tipo 0xEE), para que `int 0x80` funcione a partir do anel 3. |
| `programas` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/shell.rs) | Comando `programas`: lista os programas embutidos e seus tamanhos. |
| `rodar_programa` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/shell.rs) | Comando `rodar <programa>`: roda no anel 3 e mostra o resultado (código de saída, falha ou tempo esgotado). |
| `rodar` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Mapeia código e pilha do programa com o bit USUARIO, entra no anel 3, e quando ele termina desmapeia tudo, libera os quadros e devolve o `Resultado`. |
| `nova_pagina_zerada` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Aloca um quadro e o zera (para o programa não ver lixo de quem usou o quadro antes). |
| `encerrar` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Guarda o resultado e pula para `voltar_ao_kernel`; chamada de dentro de um tratador, nunca retorna. |
| `falha_do_programa` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Chamada pelos tratadores de exceção: se CS indica anel 3, imprime a falha e encerra o programa; senão retorna e o tratador dá panic. |
| `tempo_esgotado` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Chamada pelo timer: o tique interrompeu o anel 3 e o programa já passou de `LIMITE_TIQUES`? |
| `entrar_no_anel3` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | (naked) Salva os registradores do kernel e o RSP, monta à mão um quadro de interrupção (SS, RSP, RFLAGS, CS, RIP do programa) e executa `iretq`. |
| `voltar_ao_kernel` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | (naked) Restaura o RSP salvo por `entrar_no_anel3`, desempilha os registradores e faz `ret`: parece que `entrar_no_anel3` retornou. |
| `entrada_syscall` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | (naked) Tratador do vetor 0x80: empilha os registradores numa `Registros`, chama `tratar_syscall` e volta com `iretq`. |
| `tratar_syscall` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Despacha pelo número em rax: `sair`, `escrever` ou `tiques`; número desconhecido devolve -38. |
| `faixa_do_programa` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Confere se um ponteiro + tamanho vindo do programa está inteiro dentro das páginas dele (sem estouro de soma). |
| `escrever` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | A syscall `escrever`: valida o ponteiro, imprime os bytes e devolve quantos foram escritos (ou -14). |
| `rodar_por_nome` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Ajudante dos testes: procura o programa pelo nome e o roda. |
| `Programa::codigo` | [usuario/programas.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/programas.rs) | Os bytes de máquina do programa (uma fatia de `.rdata` entre dois rótulos). |
| `procurar` | [usuario/programas.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/programas.rs) | Procura um programa embutido pelo nome. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `ola_sai_com_42` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `ola` escreve e sai com 42. |
| `relogio_usa_syscall_de_tiques` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `relogio` espera 100 tiques pela syscall e sai com 100. |
| `ponteiro_do_kernel_recusado` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `ponteiro` passa um endereço do kernel, recebe -14 e sai com 1. |
| `invasor_encerrado_por_page_fault` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `invasor` (lê memória do kernel) termina por #PF. |
| `privilegiado_encerrado_por_gp` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `privilegiado` (`cli` no anel 3) termina por #GP. |
| `laco_encerrado_pelo_timer` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `laco` (laço infinito) é encerrado pelo timer. |
| `nada_fica_mapeado_nem_vaza` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: depois de um aquecimento, rodar `ola` deixa os endereços do programa desmapeados e os quadros livres no mesmo número. |
| `validacao_de_ponteiros` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | Teste: `faixa_do_programa` aceita e recusa os casos de borda certos. |


---

## Diferença em relação ao livro

O livro termina no capítulo 11, e a seção 11.4 traça um roteiro: disco em
RAM, um VFS simples, carregar um programa pequeno da memória, entrar no
modo usuário com um espaço de endereços fixo, implementar uma ou duas
chamadas de sistema (primeiro por interrupção de software, depois com as
instruções rápidas), e só então a troca de tarefas. Ele também recomenda
testar acessos inválidos do modo usuário e distinguir as falhas do kernel
das do usuário. Vale reler a seção 11.4 com este doc ao lado.

O SiliconOS inverteu a ordem num ponto: veio direto para o modo usuário,
com os programas embutidos no kernel, deixando disco em RAM e VFS para
depois. O resto segue o roteiro: espaço de endereços fixo, `int 0x80`
antes de `syscall`, e os testes de acesso inválido (`invasor`,
`privilegiado`, `ponteiro`). Como sempre, tudo à mão, sem a crate `x86_64`.

---

## Resumo

- No **anel 3**, a CPU bloqueia instruções privilegiadas (#GP), páginas sem
  o bit **USUARIO** em todos os níveis (page fault) e `int N` para vetores
  com DPL 0 (#GP). O CS = 0x23 marca o anel 3.
- A **TSS.RSP0** diz qual pilha a CPU usa quando uma interrupção chega no
  anel 3; o programa nunca a controla.
- Os programas de teste são assembly **independente de posição**, copiados
  para uma página de código (só leitura) com uma página de pilha (NX), na
  metade baixa. Na sintaxe Intel, um número que vem de um símbolo precisa
  de `offset`.
- **Entrar** no anel 3 é um `iretq` com um quadro montado à mão; o RSP do
  kernel fica salvo. **Voltar** é restaurar esse RSP e fazer `ret`, a
  partir de qualquer tratador.
- A **chamada de sistema** é `int 0x80` (entrada da IDT com **DPL 3**): RAX =
  número, RDI/RSI = argumentos, RAX = resultado. A entrada salva os 15
  registradores, e eles viram uma `struct Registros` para o Rust.
- **Todo ponteiro vindo do programa é conferido** (início, fim e estouro)
  antes de o kernel usá-lo.
- Exceção com **CS & 3 == 3** é culpa do programa: ele é encerrado e o kernel
  segue. O **timer** encerra quem passa de 2 s: o embrião da preempção.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Quais são as três barreiras do anel 3?**
   Instruções privilegiadas viram #GP; só se acessam páginas com o bit
   USUARIO em todos os níveis da tabela (senão, page fault); e `int N` só
   funciona para vetores com DPL 3 na IDT (senão, #GP).

2. **Por que a CPU troca de pilha (TSS.RSP0) ao receber uma interrupção no
   anel 3?**
   Porque o RSP do anel 3 é controlado pelo programa, que poderia apontá-lo
   para memória inválida ou para dados de outro programa. A CPU empilha o
   quadro numa pilha do kernel, que o programa não consegue ver nem mexer.

3. **Como o kernel entra no anel 3 se não existe instrução para isso?**
   Monta na pilha um quadro de interrupção falso (RIP do programa, CS =
   0x23, RFLAGS com IF, RSP do programa, SS = 0x1B) e executa `iretq`, que
   "volta" para o anel indicado pelo CS.

4. **Por que a entrada 0x80 da IDT tem DPL 3 e as outras não?**
   O DPL diz qual o anel menos privilegiado que pode usar `int` com aquele
   vetor. Só a porta das chamadas de sistema deve ser aberta ao programa;
   com as outras em DPL 0, um `int 14` do programa (fingindo um page fault)
   vira #GP.

5. **O que aconteceria se `escrever` não conferisse o ponteiro?**
   O programa poderia passar um endereço do kernel (como o heap) e o kernel,
   que roda no anel 0 e lê qualquer coisa, mostraria aquela memória. A
   barreira das páginas não ajuda, porque quem faz o acesso é o kernel.

6. **Por que `checked_add` na validação do ponteiro?**
   Porque `inicio + tamanho` pode passar de 2^64 e dar a volta para um
   número pequeno. Sem a conferência, um tamanho enorme passaria no teste
   "o fim está dentro da página".

7. **Como o kernel decide se uma exceção é culpa do programa?**
   Pelo CS empilhado no quadro: se os 2 bits baixos valem 3, a CPU estava no
   anel 3. O programa é encerrado; se valem 0, é um erro do kernel e vira
   panic.

8. **Por que o tratador do timer manda o EOI antes de encerrar o programa?**
   Porque `encerrar` não volta: o fim do tratador, onde o EOI normalmente
   acontece, nunca é executado. Sem o EOI, o APIC não entregaria mais o
   timer (doc 08).

9. **Por que `mov esi, OLA_TAM` fez o programa `ola` tomar page fault em
   0x34?**
   Na sintaxe Intel, um símbolo sozinho como operando significa "a memória
   naquele endereço". O assembler gerou uma leitura do endereço 52 (o valor
   de OLA_TAM). `mov esi, offset OLA_TAM` carrega o número.

</details>
