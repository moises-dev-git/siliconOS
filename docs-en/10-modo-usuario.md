# 10: User mode and system calls

> **You will understand:** why an operating system does not let programs run
> with the same power as the kernel; what the CPU changes when code moves to
> **ring 3**; how the kernel enters ring 3 (with a "fake" `iretq`) and how the
> program comes back to it (with a **system call**, `int 0x80`); why **no
> pointer coming from the program** can be used without checking; and how
> the kernel survives a program that reads forbidden memory, runs a
> privileged instruction or never finishes.
>
> **Files:** `src/usuario/mod.rs` (entering and leaving ring 3, system
> calls), `src/usuario/programas.rs` (the test programs, in assembly),
> `src/gdt.rs` (TSS.RSP0), `src/idt.rs` (vector 0x80 and exceptions in ring
> 3), `mapear_em` in `src/memoria/paginacao.rs`, the `programas` and `rodar`
> commands in `src/shell.rs`
> **Book:** chapter 11, section 11.4 (the roadmap to user mode and system
> calls)
> **Try it:** `./run.sh`, then `programas` and `rodar ola` (or `relogio`,
> `ponteiro`, `invasor`, `privilegiado`, `laco`); `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 03 (rings, GDT and TSS), doc 04 (IDT), doc 06 (page
> bits) and doc 08 (timer)
> **New Rust here:** *naked* functions and `naked_asm!` ([§35](rust-basico.md#35-naked-functions-and-naked_asm)),
> `global_asm!` and external symbols ([§36](rust-basico.md#36-global_asm-and-external-symbols)),
> calling conventions `extern "..."` ([§37](rust-basico.md#37-calling-conventions-extern-sysv64))
>
> **Names in the code:** `usuario` = *user*, `programa(s)` = *program(s)*,
> `rodar` = *run*, `entrar_no_anel3` = *enter ring 3*, `voltar_ao_kernel` =
> *return to the kernel*, `encerrar` = *terminate*, `resultado` = *result*,
> `saiu` = *exited*, `falha` = *fault*, `tempo esgotado` = *time out*,
> `escrever` = *write*, `sair` = *exit*, `tiques` = *ticks*, `pilha` =
> *stack*, `codigo` = *code*, `faixa_do_programa` = *program's range*,
> `ola` = *hello*, `relogio` = *clock*, `ponteiro` = *pointer*, `invasor` =
> *intruder*, `privilegiado` = *privileged*, `laco` = *loop*.
>
> **Code for this chapter:** [state at the end of doc 10](https://github.com/moises-dev-git/siliconOS/tree/doc-10) · [what changed since doc 09](https://github.com/moises-dev-git/siliconOS/compare/doc-09...doc-10) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

Until now, **all** of SiliconOS's code runs in ring 0, the CPU's highest
privilege level (doc 03). The shell, the heap, the handlers: all of them can
read and write any address, turn interrupts off, reprogram the APIC or
switch the page tables. That is right for the kernel, but it would be a
disaster for **programs**:

- a program with a bug (a wrong pointer) would corrupt the kernel's memory,
  and the whole machine would go down;
- a malicious program would read other programs' memory, or turn interrupts
  off and never give the CPU back;
- any error in any program would be a *kernel panic*.

In Java or Python you never think about this because the operating system
already solved it: your program runs in **user mode** (ring 3), and if it
accesses an invalid address the system ends it with `Segmentation fault`,
without affecting anything else. This chapter builds exactly that
mechanism: SiliconOS starts running **programs in ring 3**, isolated, that
can only talk to the kernel through a controlled door.

---

## 2. The idea: three barriers and one door

The CPU knows which ring it is in from the **CPL** (*current privilege
level*), the low 2 bits of the CS register. In ring 3, three things change
at once:

| Barrier | What the CPU does | Seen in |
|---|---|---|
| **Privileged instructions** | `cli`, `sti`, `hlt`, `lgdt`, `mov cr3`, `in`/`out`... become **#GP** | doc 03, glossary |
| **Pages** | it can only access pages with the **USUARIO** ("user") bit at every table level; otherwise, **page fault** | doc 06 |
| **IDT gates** | `int N` only works if IDT entry N has **DPL 3**; otherwise, **#GP** | doc 04 |

The third barrier is also the **door**: the kernel marks **one** IDT entry
(0x80) as usable from ring 3. An `int 0x80` in the program is a **system
call**: the CPU goes up to ring 0 and jumps to the handler the kernel chose.
The program picks **what** to ask for (the number in RAX), but not **where**
to enter the kernel.

```
                  RING 3 (program)                        RING 0 (kernel)
             ┌──────────────────────────┐          ┌──────────────────────────────┐
  rodar ola  │                          │          │                              │
  ──────────►│  lea rdi, [message]      │          │                              │
   iretq     │  mov esi, size           │ int 0x80 │  entrada_syscall:            │
  (CS=0x23)  │  mov eax, 1  (write)    ─┼─────────►│   saves registers            │
             │                          │          │   tratar_syscall: checks the │
             │  ◄───────────────────────┼──────────┤   pointer, writes to screen  │
             │  (RAX = bytes written)   │  iretq   │                              │
             │  mov edi, 42             │          │                              │
             │  xor eax, eax (exit)    ─┼─────────►│  encerrar(Saiu(42))          │
             └──────────────────────────┘ int 0x80 │   └─► back to the shell      │
                                                   └──────────────────────────────┘
             ✗ cli              → #GP             → the kernel ends the program
             ✗ read the kernel  → page fault      → the kernel ends the program
             ✗ infinite loop    → the timer interrupts → over 2 s → ends it
```

---

## 3. Preparing the ground

Almost everything ring 3 needs already existed, built in docs 03 to 06 "for
the future". Three details were missing.

### 3.1 The user segments (already in the GDT)

Doc 03's GDT already had, in entries 3 and 4, the data and code descriptors
with **DPL 3**. The selectors carry the requested level (RPL) in their low 2
bits:

```rust
pub const USER_DATA: u16 = (3 << 3) | 3; // 0x1B: entry 3, RPL 3
pub const USER_CODE: u16 = (4 << 3) | 3; // 0x23: entry 4, RPL 3
```

When CS is `0x23`, the CPU is in ring 3. That is the number that will show
up in the programs' error messages, as proof.

### 3.2 The stack for going back to ring 0: TSS.RSP0

When an interrupt arrives with the CPU in ring 3, it **cannot** use the
program's stack: the program controls RSP and could point it anywhere (or
at another program's memory). So the CPU switches stacks by itself before
pushing the interrupt frame, and takes the new stack's address from the
TSS's **RSP0** field (doc 03). Until now it was zero, which did not matter
because nothing ran in ring 3. Now:

```rust
// src/gdt.rs
/// Ring 0 stack for interrupts coming from ring 3 (TSS.rsp[0]).
/// User programs never see it: the CPU switches stacks by itself.
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

(*Ring 0 stack for ring 3 (TSS.RSP0): top at 0x607a0a0*)

This holds for **every** interrupt coming from ring 3: `int 0x80`, the
exceptions, and also the timer and keyboard, which keep arriving while the
program runs.

### 3.3 The USUARIO bit at every level

Doc 06 explained that the CPU combines the permissions of the page table's
4 levels: for ring 3 to access a page, the **USUARIO** bit must be set in the
PML4, the PDPT, the PD **and** the PT. `mapear_em` already set the bit on the
intermediate tables it **created**. What was missing was the case of a table
that already existed, created earlier for the kernel:

```rust
// src/memoria/paginacao.rs
} else if bits & USUARIO != 0 && e.0 & USUARIO == 0 {
    // The table already existed, created for the kernel. For ring 3 to
    // reach the page, ALL levels need the USUARIO bit (doc 10).
    *e = Entrada(e.0 | USUARIO);
}
```

Setting the bit on an intermediate table opens nothing by itself: the
kernel pages below it still lack the bit in their final entry, and the CPU
requires all four.

### 3.4 Where the program lives

Programs use a range in the **lower half** of the address space, which
nothing in the kernel uses:

```
0x0000_2000_0000_0000   program code  (1 page: USUARIO, read-only)       PML4 64
0x0000_2000_0010_0000   program stack (1 page: USUARIO, ESCRITA, NX)
...
0xFFFF_8000_0000_0000   all physical RAM, without USUARIO (doc 06)       PML4 256
0xFFFF_9000_0000_0000   kernel heap, without USUARIO (doc 07)            PML4 288
```

The code page has no ESCRITA ("write"; the program cannot modify itself),
and the stack page has NX (data on the stack cannot be executed), the same
protections as doc 07's heap.

---

## 4. The programs

There is no filesystem or executable format yet, so the programs are
**pieces of machine code inside the kernel itself**, written in assembly with
`global_asm!` ([§36](rust-basico.md#36-global_asm-and-external-symbols)). The
kernel never executes those bytes where they are: it copies them to the
program's code page and only then jumps there. That is why they sit in the
`.rdata` section (read-only data) and not in `.text`.

```rust
// src/usuario/programas.rs
global_asm!(
    ".section .rdata",

    // ---- ola: writes a message and exits with code 42 ----
    ".global programa_ola_inicio",
    ".global programa_ola_fim",
    "programa_ola_inicio:",
    "    lea rdi, [rip + ola_msg]", // pointer to the message
    "    mov esi, offset OLA_TAM",  // size (offset = the number itself, not "read memory there")
    "    mov eax, 1",               // write
    "    int 0x80",
    "    mov edi, 42",              // exit code
    "    xor eax, eax",             // exit
    "    int 0x80",
    "ola_msg: .ascii \"Ola do anel 3! (escrito por uma chamada de sistema)\\n\"",
    ".set OLA_TAM, . - ola_msg",   // "." = current position: end - start of the message
    "programa_ola_fim:",
    ...
```

(The message: *Hello from ring 3! (written by a system call)*.)

Two rules for this code:

1. **Position-independent.** The program was assembled at one address
   (inside the kernel) and runs at another (`0x2000_0000_0000`). That is why
   the message is found with `lea rdi, [rip + ola_msg]`: "the address of
   `ola_msg` relative to the current instruction", which stays correct
   after copying.
2. **No calling the kernel.** The program cannot call `kprintln!`: the
   kernel's pages are not even accessible. The only way out is `int 0x80`.

### 4.1 A real bug: `offset`

The first version had `mov esi, OLA_TAM`. The `ola` program crashed right
away:

```
silicon> rodar ola
[kernel] PAGE FAULT no programa (RIP=0x200000000007): ao ler 0x34 (pagina nao mapeada, modo usuario)
'ola' encerrado pelo kernel: PAGE FAULT em 0x200000000007
```

(*PAGE FAULT in the program: on reading 0x34 (page not mapped, user mode);
'ola' ended by the kernel*)

Address `0x34` is 52, exactly the message's size. In Intel syntax,
`mov esi, NAME` with a symbol means "**read memory** at address NAME", not
"load the number NAME". The right form is `mov esi, offset OLA_TAM`. Two
lessons in one mistake: Intel syntax has this ambiguity, and this chapter's
mechanism worked before the program itself did: the program's bug became a
message, not a *kernel panic*.

### 4.2 The six programs

```
silicon> programas
  ola           escreve uma mensagem e sai com codigo 42 (82 bytes)
  relogio       espera 1 s perguntando os tiques ao kernel (32 bytes)
  ponteiro      passa um endereco do kernel para escrever() (91 bytes)
  invasor       tenta ler a memoria do kernel (19 bytes)
  privilegiado  tenta executar cli (7 bytes)
  laco          laco infinito (o timer o encerra em 2 s) (2 bytes)
```

(*ola: writes a message and exits with code 42; relogio: waits 1 s asking
the kernel for the ticks; ponteiro: passes a kernel address to write();
invasor: tries to read kernel memory; privilegiado: tries to run cli; laco:
infinite loop (the timer ends it in 2 s)*)

The first three behave; the last three test each barrier. `laco` is the
smallest possible program: `2: jmp 2b`, a jump to itself, 2 bytes.

In Rust, each program is a pair of labels seen as external symbols, and
`Programa` keeps both pointers:

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

## 5. Entering ring 3

There is no "go to ring 3" instruction. What exists is `iretq`, the return
from an interrupt (doc 04): it pops RIP, CS, RFLAGS, RSP and SS and carries
on from there, **in the ring indicated by the popped CS**. The trick, used by
every x86 system, is to build by hand an interrupt frame that never
happened and do the `iretq`:

```
   kernel stack before the iretq         what iretq does with each value
   ┌──────────────────────────┐
   │ SS     = 0x1B            │  ← ring 3 SS
   │ RSP    = 0x2000_0010_1000│  ← the end of the program's stack page
   │ RFLAGS = 0x202           │  ← IF on: the timer keeps arriving
   │ CS     = 0x23            │  ← ring 3 code: CPL becomes 3
   │ RIP    = 0x2000_0000_0000│  ← the program's first instruction
   └──────────────────────────┘  ← RSP
```

```rust
// src/usuario/mod.rs
#[unsafe(naked)]
unsafe extern "sysv64" fn entrar_no_anel3(rip: u64, rsp: u64) -> u64 {
    naked_asm!(
        // Registers the convention says to preserve: voltar_ao_kernel restores them.
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov [rip + {rsp_kernel}], rsp",
        // The frame iretq expects, bottom to top: RIP, CS, RFLAGS, RSP, SS.
        "push {ss}",
        "push rsi",
        "push {rflags}",
        "push {cs}",
        "push rdi",
        // Do not leave kernel values in the registers for the program to see.
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor edx, edx",
        ...
        "iretq",
        rsp_kernel = sym RSP_KERNEL,
        ss = const gdt::USER_DATA as u64,
        cs = const gdt::USER_CODE as u64,
        rflags = const 0x202u64, // bit 9 (IF): the program runs with interrupts on
    )
}
```

Parts that deserve attention:

- **A *naked* function** ([§35](rust-basico.md#35-naked-functions-and-naked_asm)):
  the compiler generates nothing beyond the written assembly, no prologue,
  no epilogue. Here that is mandatory: the function manipulates the stack
  directly and never returns the normal way.
- **`extern "sysv64"`** ([§37](rust-basico.md#37-calling-conventions-extern-sysv64)):
  the arguments arrive in RDI (the program's RIP) and RSI (its RSP). The
  UEFI target uses the Windows convention by default (RCX, RDX), so we pick
  a convention explicitly for the assembly to match.
- **Saving the kernel's RSP.** The 6 `push`es keep the registers the caller
  expects to find intact, and `RSP_KERNEL` keeps where they ended up. It is
  the "bookmark" for coming back.
- **Zeroing the registers.** Without it, the program would start with kernel
  leftovers in RAX, RBX..., which may be addresses or data. Leaking
  information is a security problem, even if a small one.

`rodar`, which calls all of this, prepares the pages, enters, and cleans up
on return:

```rust
pub fn rodar(programa: &programas::Programa) -> Resultado {
    // 1. A code page (read-only, executable) and a stack page
    //    (writable, no execute), both accessible from ring 3.
    let q_codigo = nova_pagina_zerada();
    let q_pilha = nova_pagina_zerada();
    ... copies the code into q_codigo ...
    paginacao::mapear(CODIGO_VIRT, q_codigo.endereco(), paginacao::TamanhoPagina::K4,
                      flags::USUARIO).expect("mapear o codigo do programa");
    paginacao::mapear(PILHA_VIRT, q_pilha.endereco(), paginacao::TamanhoPagina::K4,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO).expect("mapear a pilha");

    // 2. Enter ring 3. Only returns when the program finishes.
    ...
    unsafe { entrar_no_anel3(CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO) };
    ...
    // We came back from inside a handler (interrupts off).
    cpu::ligar_interrupcoes();

    // 3. Clean up: nothing of the program stays mapped.
    for (virt, q) in [(CODIGO_VIRT, q_codigo), (PILHA_VIRT, q_pilha)] {
        paginacao::desmapear(virt).expect("desmapear");
        quadros::liberar(q);
    }
    RESULTADO.lock().take().expect("programa terminou sem resultado")
}
```

The new pages are **zeroed** first: a reused frame holds the data of whoever
used it before (maybe the kernel itself), and the program must not see that.

---

## 6. System calls: `int 0x80`

### 6.1 The door in the IDT

IDT entry 0x80 points to `entrada_syscall`, with one difference from all the
others: **DPL 3**.

```rust
// src/idt.rs
/// The same with DPL=3 (bits 5-6): ring 3 may use `int` with this vector.
/// With DPL=0, an `int 0x80` from ring 3 becomes #GP (docs/10-modo-usuario.md).
const INTERRUPT_GATE_DPL3: u8 = 0xEE;
...
t[usuario::VETOR_SYSCALL as usize] =
    Entrada::new(usuario::entrada_syscall as *const () as u64, 0).para_usuario();
```

`0x8E` = `1000 1110` (present, DPL 0, interrupt gate); `0xEE` = `1110 1110`,
bits 5 and 6 become `11` = DPL 3. The other entries keep DPL 0: a program
that tries `int 14` (faking a page fault) or `int 3` gets #GP. The DPL only
controls who may use `int`; real exceptions and hardware interrupts arrive
from any ring.

### 6.2 The contract: which registers

Every system defines its own call ABI (*application binary interface*).
SiliconOS follows Linux's choice of registers:

| Register | Going in | Coming back |
|---|---|---|
| RAX | call number | result (negative = error) |
| RDI | 1st argument | preserved |
| RSI | 2nd argument | preserved |

| RAX | Call | Arguments | Returns |
|---|---|---|---|
| 0 | `sair` (exit) | RDI = exit code | does not return |
| 1 | `escrever` (write) | RDI = pointer, RSI = size | bytes written, or -14 |
| 2 | `tiques` (ticks) | none | timer ticks since boot |
| other | | | -38 |

The negative errors copy Linux's numbers: -14 is `EFAULT` ("bad address")
and -38 is `ENOSYS` ("no such call").

### 6.3 The entry: saving everything

A system call handler needs to **read and change** the program's registers
(read RAX, RDI, RSI; write the result to RAX). Docs 04 and 08's
`x86-interrupt` convention gives no access to them. So the entry is written
by hand:

```rust
#[unsafe(naked)]
pub extern "sysv64" fn entrada_syscall() {
    naked_asm!(
        "push rax", "push rcx", "push rdx", "push rbx", "push rbp", "push rsi", "push rdi",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",          // 1st argument: &mut Registros
        "call {tratar}",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rdi", "pop rsi", "pop rbp", "pop rbx", "pop rdx", "pop rcx", "pop rax",
        "iretq",
        tratar = sym tratar_syscall,
    )
}
```

After the 15 `push`es, the stack (the ring 0 one, from TSS.RSP0) holds
exactly, from the lowest address to the highest, the 15 registers and the
frame the CPU pushed. A `struct` with `#[repr(C)]` and its fields in that
order **is** the stack, and `mov rdi, rsp` passes a pointer to it:

```
   high addresses
   ┌────────┐
   │ SS     │ ┐
   │ RSP    │ │ pushed by the CPU
   │ RFLAGS │ │ (doc 04's "frame")
   │ CS     │ │
   │ RIP    │ ┘
   │ RAX    │ ← 1st push
   │ RCX    │
   │  ...   │
   │ R15    │ ← 15th push = RSP = &Registros
   └────────┘
   low addresses
```

```rust
#[repr(C)]
pub struct Registros {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rdi: u64, pub rsi: u64, pub rbp: u64, pub rbx: u64,
    pub rdx: u64, pub rcx: u64, pub rax: u64,
    // pushed by the CPU:
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

(`Registros` = *registers*.)

The result written to `r.rax` goes to the stack slot `pop rax` will read
from: after the `iretq`, the program finds the result in RAX.

An alignment detail this arrangement solves for free: the convention
requires RSP to be a multiple of 16 at the `call`. The CPU aligns the stack
to 16 and pushes 5 values (40 bytes); the 15 `push`es add 120. That is 160
bytes, a multiple of 16.

### 6.4 Never trust a pointer from the program

`escrever(ponteiro, tamanho)` receives an address **chosen by the program**.
If the kernel simply read from there, the program could ask "write to the
screen the 16 bytes at `0xFFFF_9000_0000_0000`" and the kernel would show a
piece of its own heap. The page barrier does not help, because the one
reading is the kernel, in ring 0, and ring 0 can read everything. So the
kernel checks first:

```rust
/// Is the pointer [start, start + size) entirely inside one of the
/// program's pages? Never trust a pointer coming from ring 3.
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    [CODIGO_VIRT, PILHA_VIRT].iter().any(|&p| inicio >= p && fim <= p + TAMANHO_QUADRO)
}
```

Three traps the check covers:

- the **start** outside the program's pages (the `ponteiro` case);
- the start inside, but the **end** outside (a large size "slides" into the
  neighboring memory);
- `inicio + tamanho` **overflowing** 64 bits and wrapping around to a small
  number, which would pass a naive test. Hence the `checked_add`.

```
silicon> rodar ponteiro
[kernel] escrever(): ponteiro 0xffff900000000000 fora da memoria do programa, recusado
o kernel recusou o ponteiro
'ponteiro' terminou: codigo de saida 1
```

(*write(): pointer ... outside the program's memory, refused; the kernel
refused the pointer; 'ponteiro' finished: exit code 1*)

The program got -14, noticed it (`test rax, rax` / `jns`) and exited with 1.

> Modern CPUs have an extra hardware protection, **SMAP** (*supervisor mode
> access prevention*): turned on in CR4, it makes the kernel itself take a
> page fault when touching a USUARIO page, unless it declares the intention
> with the `stac` instruction. SiliconOS does not use SMAP yet (nor SMEP,
> which stops the kernel from **executing** user pages).
> The QEMU monitor showed `CR4=00000668`: bits 20 (SMEP) and 21 (SMAP) are
> off. That is why the software check is the only protection, and it must be
> right.

---

## 7. Exiting: three paths, one destination

A program ends in three ways, and all three end in the same function:

| Path | Who calls `encerrar` | Result |
|---|---|---|
| `sair(codigo)` call | `tratar_syscall` | `Saiu(codigo)` |
| CPU exception in ring 3 | the exception's handler (`idt.rs`) | `Falha { excecao, rip }` |
| time out | the timer handler | `TempoEsgotado` |

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

`voltar_ao_kernel` is the mirror of `entrar_no_anel3`: it puts back the
saved RSP, pops the 6 registers and does `ret`. That `ret` uses the return
address that `call entrar_no_anel3` left there at the start, so, for
`rodar`, it is as if `entrar_no_anel3` had simply finished.

The stack of the handler we were in (the TSS.RSP0 one) is **abandoned**
halfway: the program's interrupt frame will never be popped, and that is
fine, because the program will not continue. The next time ring 3 is
interrupted, the CPU starts again from the top of that stack.

Two consequences:

- `rodar` comes back with **interrupts off** (we were inside an *interrupt
  gate*), which is why it turns them back on with
  `cpu::ligar_interrupcoes()`;
- the timer handler, when it ends a program, **sends the EOI first** (doc
  08). The normal path (the end of the handler) will never happen, and
  without the EOI the timer would stop.

### 7.1 Exceptions: is the program or the kernel to blame?

Each exception handler from doc 04 got a line at the start:

```rust
extern "x86-interrupt" fn protecao_geral(quadro: QuadroInterrupcao, codigo: u64) {
    // In ring 3, the most common cause: a privileged instruction (cli, hlt, in, out...).
    usuario::falha_do_programa(&quadro, "PROTECAO GERAL", format_args!("#GP, codigo={:#x}", codigo));
    ... the panic from before ...
```

```rust
pub fn falha_do_programa(quadro: &QuadroInterrupcao, excecao: &'static str, detalhes: core::fmt::Arguments) {
    if quadro.cs & 3 == 3 {
        kprintln!("[kernel] {} no programa (RIP={:#x}, CS={:#x}): {}", excecao, quadro.rip, quadro.cs, detalhes);
        encerrar(Resultado::Falha { excecao, rip: quadro.rip });
    }
}
```

(`falha_do_programa` = *the program's fault*.)

The question "who is to blame?" is answered by the **CS pushed** in the
frame: if its low 2 bits are 3, the CPU was in ring 3 when the exception
happened, and the program made the mistake. It is ended and the kernel
carries on. If they are 0, the error is the kernel's, and it is still a
*kernel panic*, as before.

```
silicon> rodar invasor
[kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
'invasor' encerrado pelo kernel: PAGE FAULT em 0x20000000000a
silicon> rodar privilegiado
[kernel] PROTECAO GERAL no programa (RIP=0x200000000000, CS=0x23): #GP, codigo=0x0
'privilegiado' encerrado pelo kernel: PROTECAO GERAL em 0x200000000000
```

(*PAGE FAULT in the program: on reading 0xffff800000100000 (page present, no
permission, user mode); GENERAL PROTECTION in the program*)

`invasor`'s page fault shows the page barrier: address
`0xFFFF_8000_0010_0000` **is** mapped (it is RAM seen through the upper
half, doc 06), hence "page present, no permission": the page exists, but
without the USUARIO bit. Bit 2 of the error code, "user mode", is the CPU
confirming where the access came from. `privilegiado` dies on its first
instruction (`cli`, RIP = start of the program) with #GP code 0, doc 04's
"you cannot do that".

### 7.2 The timer gives the CPU back to the kernel

The `laco` program does nothing wrong: it just never finishes, and never
calls the kernel. In a system without a timer, it would own the CPU
forever. But doc 08's timer keeps interrupting 100 times per second, in ring
3 too, and the handler now checks:

```rust
// src/interrupcoes/timer.rs
pub extern "x86-interrupt" fn tratador(quadro: QuadroInterrupcao) {
    let t = TIQUES.fetch_add(1, Ordering::Relaxed) + 1;

    // Did we interrupt a user program that is already over the limit? Then
    // the kernel takes the CPU back and ends it (docs/10-modo-usuario.md).
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

(*'laco' ended by the kernel: over 2 s without finishing*)

This is the seed of **preemption**: instead of ending the program, a
scheduler would save its registers and give the CPU to another one. It is
the natural topic of the next step.

---

## 8. How it was tested

### 8.1 From the shell

With the QEMU monitor and `sendkey` (doc 08), all the programs, one after
another, and `mem` before and after:

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

The number of free frames is the same as before running the programs: each
program's pages went back to the allocator. The kernel kept responding
after every fault.

### 8.2 `cargo test`

Eight new tests in `src/usuario/mod.rs`, one per program, plus "nothing
stays mapped or leaks" (both pages unmapped and the frames back) and pointer
validation (inside, sliding past the end, kernel, overflow). There are now
32 tests:

```
siliconos::usuario::testes::invasor_encerrado_por_page_fault ... [kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
[ok]
siliconos::usuario::testes::laco_encerrado_pelo_timer ... [ok]
siliconos::usuario::testes::nada_fica_mapeado_nem_vaza ... Ola do anel 3! (escrito por uma chamada de sistema)
[ok]
...
Todos os 32 testes passaram.
```

### 8.3 Regression

The 21 builds from the earlier chapters (normal boot and 20 features) gave
the expected result.

---

## What is still missing

This is minimal user mode, and it is worth being honest about what it does
not do:

- **A single address space.** The program uses the kernel's own page
  tables; what protects it is the USUARIO bit. A real system gives each
  process **its own PML4** (and switches CR3 on every process switch), so
  two programs can use the same address without seeing each other.
- **One program at a time,** and the shell waits for it to finish. Running
  several at once requires saving each one's state: a scheduler.
- **Programs built into the kernel.** Loading from a file requires an
  executable format (ELF) and a place to read from (a RAM disk, a
  filesystem).
- **`int 0x80` is slow.** x86-64 CPUs have the `syscall`/`sysret`
  instructions, made for this, configured through MSRs (STAR, LSTAR,
  SFMASK). The book recommends starting with the software interrupt, which
  is easier to understand, and switching once the behavior is right, which
  is what we did.
- **SMAP and SMEP** off.

---

## Changes to the files

What this chapter adds to the code of doc 09 (tag `doc-09`). Each file's link
leads to its state at the end of this chapter (tag `doc-10`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-09...doc-10).

| File | What changes | Where |
|---|---|---|
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/mod.rs) | **new**: constants, `Resultado`, `rodar`, `encerrar`, `falha_do_programa`, `tempo_esgotado`, `entrar_no_anel3`, `voltar_ao_kernel`, `Registros`, `entrada_syscall`, `tratar_syscall`, `faixa_do_programa`, `escrever` and the tests | — |
| [`src/usuario/programas.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/usuario/programas.rs) | **new**: the `global_asm!` with the 6 programs, the `unsafe extern "C"` symbols, `Programa`, `PROGRAMAS`, `procurar` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/main.rs) | `mod usuario;` | with the other `mod`s |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/main.rs) | `kprintln!` with the top of `gdt::topo_pilha_anel0()` | after the IST1 stack `kprintln!` |
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/gdt.rs) | remove `#[allow(dead_code)]` from `USER_DATA` and `USER_CODE`; `static mut PILHA_ANEL0`; `rsp: [topo_anel0, 0, 0]` in the TSS; `fn topo_pilha_anel0` | selectors; after `PILHA_DOUBLE_FAULT`; in `init`; before `topo_pilha_double_fault` |
| [`src/idt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/idt.rs) | `use crate::usuario;`; `INTERRUPT_GATE_DPL3`; `Entrada::para_usuario`; entry 0x80 in `init`; a call to `usuario::falha_do_programa` at the start of `divisao`, `opcode_invalido`, `protecao_geral` and `page_fault` | see the snippets below |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/interrupcoes/timer.rs) | the `tratador` parameter is now called `quadro`; the `tempo_esgotado` check + `eoi` + `encerrar` | right after the `fetch_add` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/memoria/paginacao.rs) | the `else if` that sets USUARIO on an existing table | `mapear_em`, in the loop that walks down the levels |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-10/src/shell.rs) | two lines in `COMANDOS` (`programas` and `rodar`) and the `programas` and `rodar_programa` functions | before the `reiniciar` line; before `fn reiniciar` |

### The snippets the text did not show

For anyone reproducing without opening GitHub, these are the parts of
`src/usuario/mod.rs` that the sections above mentioned without showing in full:

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
/// A program that goes past this without giving the CPU back is ended by the timer.
pub const LIMITE_TIQUES: u64 = 2 * timer::FREQUENCIA_HZ;

pub const SYS_SAIR: u64 = 0;
pub const SYS_ESCREVER: u64 = 1;
pub const SYS_TIQUES: u64 = 2;
pub const ERRO_ENDERECO: u64 = -14i64 as u64;  // EFAULT: invalid pointer
pub const ERRO_SYSCALL: u64 = -38i64 as u64;   // ENOSYS: unknown call

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resultado {
    Saiu(u64),
    Falha { excecao: &'static str, rip: u64 },
    TempoEsgotado,
}

static ATIVO: AtomicBool = AtomicBool::new(false);
static INICIO: AtomicU64 = AtomicU64::new(0);
static RESULTADO: Mutex<Option<Resultado>> = Mutex::new(None);
/// The kernel's RSP when it entered ring 3 (read by voltar_ao_kernel).
static mut RSP_KERNEL: u64 = 0;
```

The core of `rodar` (between preparing the pages and entering ring 3), and
the function that zeroes a new frame:

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

The question the timer handler asks:

```rust
pub fn tempo_esgotado(quadro: &QuadroInterrupcao) -> bool {
    quadro.cs & 3 == 3
        && ATIVO.load(Ordering::Relaxed)
        && timer::tiques() - INICIO.load(Ordering::Relaxed) >= LIMITE_TIQUES
}
```

And `escrever`, after the validation:

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

In `src/usuario/programas.rs`, the `Programa` type:

```rust
pub struct Programa {
    pub nome: &'static str,
    pub descricao: &'static str,
    inicio: *const u8,
    fim: *const u8,
}

// The pointers point to fixed data in the executable: safe across CPUs.
unsafe impl Sync for Programa {}

impl Programa {
    /// The bytes of the program's machine code.
    pub fn codigo(&self) -> &'static [u8] {
        unsafe { core::slice::from_raw_parts(self.inicio, self.fim as usize - self.inicio as usize) }
    }
}

pub fn procurar(nome: &str) -> Option<&'static Programa> {
    PROGRAMAS.iter().find(|p| p.nome == nome)
}
```

In `src/idt.rs`, `Entrada`'s new method and the start of the page fault (the
messages become variables, to serve both the program and the panic):

```rust
    /// The same entry, but callable from ring 3 (system calls).
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

In the other three handlers the new line is the same, changing the name and
the detail: `"DIVISAO POR ZERO"` with `format_args!("#DE")`, `"OPCODE INVALIDO"`
with `format_args!("#UD")` and `"PROTECAO GERAL"` with
`format_args!("#GP, codigo={:#x}", codigo)`.

In `src/shell.rs`, the two commands:

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

## Difference from the book

The book ends at chapter 11, and section 11.4 lays out a roadmap: a RAM
disk, a simple VFS, loading a small program from memory, entering user mode
with a fixed address space, implementing one or two system calls (first
through a software interrupt, later with the fast instructions), and only
then task switching. It also recommends testing invalid accesses from user
mode and telling kernel faults apart from user faults. It is worth
rereading section 11.4 alongside this doc.

SiliconOS inverted the order in one point: it went straight to user mode,
with the programs built into the kernel, leaving the RAM disk and VFS for
later. The rest follows the roadmap: a fixed address space, `int 0x80`
before `syscall`, and tests of invalid access (`invasor`, `privilegiado`,
`ponteiro`). As always, everything by hand, without the `x86_64` crate.

---

## Summary

- In **ring 3**, the CPU blocks privileged instructions (#GP), pages without
  the **USUARIO** bit at every level (page fault) and `int N` for vectors with
  DPL 0 (#GP). CS = 0x23 marks ring 3.
- **TSS.RSP0** says which stack the CPU uses when an interrupt arrives in
  ring 3; the program never controls it.
- The test programs are **position-independent** assembly, copied to a code
  page (read-only) with a stack page (NX), in the lower half. In Intel
  syntax, a number that comes from a symbol needs `offset`.
- **Entering** ring 3 is an `iretq` with a hand-built frame; the kernel's RSP
  is saved. **Coming back** is restoring that RSP and doing `ret`, from any
  handler.
- The **system call** is `int 0x80` (an IDT entry with **DPL 3**): RAX =
  number, RDI/RSI = arguments, RAX = result. The entry saves the 15
  registers, and they become a `struct Registros` for Rust.
- **Every pointer coming from the program is checked** (start, end and
  overflow) before the kernel uses it.
- An exception with **CS & 3 == 3** is the program's fault: it is ended and
  the kernel carries on. The **timer** ends whoever goes over 2 s: the seed
  of preemption.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **What are ring 3's three barriers?**
   Privileged instructions become #GP; only pages with the USUARIO bit at
   every table level can be accessed (otherwise, page fault); and `int N`
   only works for vectors with DPL 3 in the IDT (otherwise, #GP).

2. **Why does the CPU switch stacks (TSS.RSP0) when it gets an interrupt in
   ring 3?**
   Because ring 3's RSP is controlled by the program, which could point it
   at invalid memory or at another program's data. The CPU pushes the frame
   onto a kernel stack, which the program can neither see nor touch.

3. **How does the kernel enter ring 3 if there is no instruction for it?**
   It builds a fake interrupt frame on the stack (the program's RIP, CS =
   0x23, RFLAGS with IF, the program's RSP, SS = 0x1B) and runs `iretq`,
   which "returns" to the ring indicated by CS.

4. **Why does IDT entry 0x80 have DPL 3 and the others do not?**
   The DPL says which is the least privileged ring that may use `int` with
   that vector. Only the system call door should be open to the program;
   with the others at DPL 0, an `int 14` from the program (faking a page
   fault) becomes #GP.

5. **What would happen if `escrever` did not check the pointer?**
   The program could pass a kernel address (such as the heap) and the
   kernel, which runs in ring 0 and reads anything, would show that memory.
   The page barrier does not help, because the one doing the access is the
   kernel.

6. **Why `checked_add` in the pointer check?**
   Because `inicio + tamanho` may go past 2^64 and wrap around to a small
   number. Without the check, a huge size would pass the "the end is inside
   the page" test.

7. **How does the kernel decide whether an exception is the program's
   fault?**
   From the CS pushed in the frame: if its low 2 bits are 3, the CPU was in
   ring 3. The program is ended; if they are 0, it is a kernel error and
   becomes a panic.

8. **Why does the timer handler send the EOI before ending the program?**
   Because `encerrar` does not return: the end of the handler, where the EOI
   normally happens, never runs. Without the EOI, the APIC would deliver no
   more timer interrupts (doc 08).

9. **Why did `mov esi, OLA_TAM` make the `ola` program take a page fault at
   0x34?**
   In Intel syntax, a bare symbol as an operand means "the memory at that
   address". The assembler generated a read of address 52 (OLA_TAM's
   value). `mov esi, offset OLA_TAM` loads the number.

</details>
