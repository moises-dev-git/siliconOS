# 14: `syscall` and `sysret`

> **You will understand:** why `int 0x80` is an expensive way to ask the
> kernel for something; what the **`syscall`** and **`sysret`** instructions
> do (and what they **don't**, such as switching stacks); how to configure
> them with four **MSRs**; why doc 03's GDT has user data before user code;
> and how much is gained, measured in cycles on the real CPU.
>
> **Files:** `src/usuario/syscall.rs` (new: the MSRs and the entry),
> `src/cpu.rs` (`escrever_msr`), `src/gdt.rs` (`RSP0_ATUAL`), `src/main.rs`
> (calls `init`), `programas/src/lib.rs` (the `sos` library switches to
> `syscall`), `programas/src/bin/medir.rs` (new: the measuring program)
> **Book:** chapter 11, section 11.4 (the book recommends starting with a
> software interrupt and switching later; the implementation is ours)
> **Try it:** `./run.sh`, then `rodar medir` and `rodar oi`;
> `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 03 (GDT), doc 10 (system calls through `int 0x80`)
> and doc 12 (a kernel stack per process)
> **New Rust here:** clobbered registers in `asm!` and `#[inline(always)]`
> ([§44](rust-basico.md#44-asm-clobbered-registers-and-inlinealways))
> **Names in the code:** `entrada_syscall_rapida` = *fast syscall entry*,
> `escrever_msr` = *write MSR*, `RSP0_ATUAL` = *current RSP0*,
> `RSP_DO_PROGRAMA` = *the program's RSP*, `chamada` = *call*, `medir` =
> *measure*, `ciclos` = *cycles*, `VEZES` = *times*
>
> **Code for this chapter:** [state at the end of doc 14](https://github.com/moises-dev-git/siliconOS/tree/doc-14) · [what changed since doc 13](https://github.com/moises-dev-git/siliconOS/compare/doc-13...doc-14) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

Since doc 10, a program asks the kernel for something with `int 0x80`: a
**software interrupt**. It works, and it was the right choice to start with
(the book recommends it: the interrupt mechanism already existed and was
familiar). But an interrupt is a **general** path, built for everything
(exceptions, hardware, calls), and the CPU does a lot of work on each one:

1. reads IDT entry 0x80 from memory (16 bytes) and checks it is valid;
2. checks the entry's DPL against the current ring (that is what allows
   doc 10's `int 0x80` from ring 3);
3. reads the new CS from the entry and its descriptor in the GDT;
4. since the ring changes, reads RSP0 from the TSS and switches stacks;
5. pushes 5 values (SS, RSP, RFLAGS, CS, RIP);
6. on the way back, `iretq` pops the 5 and redoes several of those checks.

A program that prints makes one system call per piece of formatted text
(doc 13). In a real system there are millions of calls per second: the cost
of each one matters.

## 2. The idea: a door built just for this

x86-64 CPUs have two instructions built **only** for system calls:
`syscall` (enter) and `sysret` (leave). They come from AMD, which defined
them as the standard way to call the kernel in 64-bit mode; Intel had its
own pair, `sysenter`/`sysexit`, and also implements `syscall`/`sysret` in
64-bit mode.

The difference lies in what they **skip**. `syscall`:

- **does not read the IDT, GDT or TSS from memory:** the entry address and
  the selectors come from configuration registers (MSRs), set once at boot;
- **does not check privileges:** there is only one possible destination,
  chosen by the kernel;
- **pushes nothing:** it keeps the return RIP in **RCX** and RFLAGS in
  **R11**;
- **does not switch stacks.** The entry starts running with the program's
  RSP.

In detail, `syscall` does:

```
  RCX    ← RIP           (the instruction after syscall: where to return)
  R11    ← RFLAGS
  RFLAGS ← RFLAGS without the bits in the FMASK MSR
  CS     ← STAR[47:32]          SS ← STAR[47:32] + 8      (ring 0)
  RIP    ← LSTAR
```

and `sysret` (the 64-bit version, `sysretq`), the reverse:

```
  RIP    ← RCX
  RFLAGS ← R11
  CS     ← STAR[63:48] + 16 (RPL 3)    SS ← STAR[63:48] + 8 (RPL 3)
```

Notice: the program's RCX and R11 **are lost** (the CPU uses them). That
becomes part of the contract: after `syscall`, the program cannot rely on
RCX and R11.

## 3. Configuring: four MSRs

An **MSR** (*model-specific register*, doc 08) is a configuration register
identified by a number, read with `rdmsr` and written with `wrmsr`. So far
the kernel only read them (the APIC, in doc 08); now it must write:

```rust
/// Writes an MSR (`wrmsr`: number in ECX, value in EDX:EAX). Used to
/// configure `syscall`/`sysret` (doc 14).
pub fn escrever_msr(numero: u32, valor: u64) {
    unsafe {
        asm!("wrmsr", in("ecx") numero, in("edx") (valor >> 32) as u32, in("eax") valor as u32,
             options(nostack, preserves_flags));
    }
}
```

The four that matter:

```rust
pub const IA32_EFER: u32 = 0xC000_0080;
pub const IA32_STAR: u32 = 0xC000_0081;
pub const IA32_LSTAR: u32 = 0xC000_0082;
pub const IA32_FMASK: u32 = 0xC000_0084;

/// EFER.SCE (System Call Enable): without it, `syscall` is an invalid instruction (#UD).
pub const EFER_SCE: u64 = 1 << 0;

/// RFLAGS bits that `syscall` clears on entry: IF (interrupts), TF (the
/// debugger's single-step) and DF (direction of string instructions).
pub const FMASK: u64 = (1 << 9) | (1 << 8) | (1 << 10);

/// Turns `syscall`/`sysret` on and points the CPU at our entry.
pub fn init() {
    // STAR: bits 32–47 = kernel code selector (kernel SS = that + 8);
    // bits 48–63 = base for the 64-bit sysret: CS = base + 16, SS = base + 8,
    // both with RPL 3. With doc 03's GDT (0x08 code, 0x10 kernel data,
    // 0x18 user data, 0x20 user code), the base is 0x10:
    //   syscall → CS = 0x08, SS = 0x10;   sysret → CS = 0x20|3 = 0x23, SS = 0x18|3 = 0x1B.
    let base_sysret = (gdt::KERNEL_DATA as u64) | 3; // 0x13
    let star = (base_sysret << 48) | ((gdt::KERNEL_CODE as u64) << 32);
    cpu::escrever_msr(IA32_STAR, star);
    cpu::escrever_msr(IA32_LSTAR, entrada_syscall_rapida as *const () as u64);
    cpu::escrever_msr(IA32_FMASK, FMASK);
    cpu::escrever_msr(IA32_EFER, cpu::ler_msr(IA32_EFER) | EFER_SCE);
}
```

- **EFER.SCE** turns the instructions on. (EFER is the same MSR in which
  UEFI had already turned on 64-bit mode and NXE, doc 06: hence the
  `| EFER_SCE` on top of the value read, without erasing the rest.)
- **LSTAR** is the entry address (section 4).
- **FMASK**: which RFLAGS bits to clear on entry. The most important is
  **IF**: the entry starts on the program's stack and must switch stacks
  before any interrupt arrives (section 4). TF and DF: so the kernel does not
  run single-stepped or with string instructions going backwards just
  because the program set them.
- **STAR** holds two selectors in an arithmetic that requires a particular
  order in the GDT.

### 3.1 Doc 03's promise

`sysret` does not read CS and SS from memory: it computes both from **one**
number, `STAR[63:48]`, adding 8 for SS and 16 for CS. That only works if,
in the GDT, user **data** sits right before user **code**. Doc 03 built the
GDT in that order and warned that it was because of `syscall`/`sysret`
"in the future":

```
  SiliconOS's GDT (doc 03)           the STAR arithmetic

  0x00  null
  0x08  kernel code        ◄── STAR[47:32] = 0x08      syscall: CS = 0x08
  0x10  kernel data        ◄── STAR[63:48] = 0x10|3    syscall: SS = 0x08 + 8 = 0x10
  0x18  user data          ◄── base + 8                sysret:  SS = 0x18 | 3 = 0x1B
  0x20  user code          ◄── base + 16               sysret:  CS = 0x20 | 3 = 0x23
  0x28  TSS
```

The CS and SS values are exactly the ones `iretq` used (0x23 and 0x1B, doc
10): the program notices no difference. (Why "+16" for code? Back then,
`sysret` could also return to 32-bit programs, with CS = base, and the
64-bit entry came after it.)

## 4. The entry: switching stacks on its own

This is the tricky part. When `syscall` jumps to `entrada_syscall_rapida`,
RSP is still **the program's**. Using the program's stack in the kernel
would be a serious mistake: the program chooses RSP, and could point it at
kernel memory, at an unmapped address or at something else of its own
(`int 0x80` does not have this problem because the CPU switches stacks
through the TSS, doc 10).

So the entry must switch stacks **before using the stack for anything**.
To which one? The current process's kernel stack, the same one the
scheduler puts in TSS.RSP0 (doc 12). Since the assembly cannot easily reach
the TSS (it lives in an `UnsafeCell` inside a `Once`), `definir_rsp0` now
also keeps a copy in a named `static`:

```rust
pub fn definir_rsp0(topo: u64) {
    let tss = TSS.get().expect("gdt::init antes de definir_rsp0");
    unsafe {
        (&raw mut (*tss.0.get()).rsp as *mut u64).write_unaligned(topo);
        RSP0_ATUAL = topo;
    }
}

/// A copy of TSS.rsp[0] in a place the assembly can reach by name (doc 14):
/// the `syscall` instruction, unlike `int`, does NOT switch stacks, and the
/// `syscall` entry reads the current process's kernel stack from here.
pub static mut RSP0_ATUAL: u64 = 0;
```

(and `gdt::init` puts the initial value there, the top of `PILHA_ANEL0`.)

The whole entry:

```rust
static mut RSP_DO_PROGRAMA: u64 = 0;

#[unsafe(naked)]
pub unsafe extern "sysv64" fn entrada_syscall_rapida() {
    naked_asm!(
        // 1. Switch to the current process's kernel stack (the same as TSS.RSP0).
        "mov [rip + {rsp_programa}], rsp",
        "mov rsp, [rip + {rsp0}]",
        // 2. The frame `int` would push: SS, RSP, RFLAGS (in R11), CS, RIP (in RCX).
        "push {ss}",
        "push qword ptr [rip + {rsp_programa}]",
        "push r11",
        "push {cs}",
        "push rcx",
        // 3. The registers, in the same order as entrada_syscall (struct Registros).
        "push rax", "push rcx", "push rdx", "push rbx", "push rbp", "push rsi", "push rdi",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",          // 1st argument: &mut Registros
        "call {tratar}",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rdi", "pop rsi", "pop rbp", "pop rbx", "pop rdx", "pop rcx", "pop rax",
        // 4. Return: RIP goes in RCX and RFLAGS in R11 (that is where sysret
        //    reads them from); the program's RSP last, because after it the
        //    kernel stack is no longer at hand.
        "mov rcx, [rsp]",        // RIP
        "mov r11, [rsp + 16]",   // RFLAGS
        "mov rsp, [rsp + 24]",   // the program's RSP
        "sysretq",
        rsp_programa = sym RSP_DO_PROGRAMA,
        rsp0 = sym gdt::RSP0_ATUAL,
        ss = const gdt::USER_DATA as u64,
        cs = const gdt::USER_CODE as u64,
        tratar = sym super::tratar_syscall,
    )
}
```

Step by step:

1. **Switch stacks.** The program's RSP goes into `RSP_DO_PROGRAMA` (a
   `static`, because there is no trustworthy stack to keep it on yet), and
   RSP becomes the top of the process's kernel stack. A single `static` is
   enough because there is one CPU and interrupts are off (FMASK cleared
   IF): nobody else goes through here between those two instructions.
2. **Fake the `int` frame.** Instead of inventing a new format, the entry
   pushes **the same** thing the CPU pushes on an `int 0x80`: the user SS
   and CS (constants), the program's RSP, RFLAGS (which `syscall` left in
   R11) and the return RIP (in RCX).
3. **The same registers, in the same order.** With that, the stack is
   identical to doc 10's `entrada_syscall`, and the `Registros` struct and
   the `tratar_syscall` function serve both doors **without changing a
   line**. The `sair`, `escrever`, `tiques` and `pid` calls, pointer
   validation, the scheduler's `encerrar`: all the same.
4. **Return with `sysretq`.** After restoring the registers, RCX gets the
   return RIP and R11 the RFLAGS (that is where `sysret` reads them from),
   and RSP goes back to the program's. The order matters: while RSP points
   at the kernel stack, `[rsp]`, `[rsp + 16]` and `[rsp + 24]` can be read;
   after the last `mov`, they cannot.

```
  the process's kernel stack (top = RSP0_ATUAL)
  ┌────────────────────┐
  │ SS = 0x1B          │  ┐
  │ the program's RSP  │  │ the "int frame",
  │ RFLAGS (was R11)   │  │ built by hand
  │ CS = 0x23          │  │
  │ RIP (was RCX)      │  ┘ ◄── [rsp] on the way back
  │ rax rcx rdx ...    │  ┐ struct Registros:
  │ ... r14 r15        │  ┘ the same as int 0x80
  └────────────────────┘ ◄── rsp when calling tratar_syscall
```

And if the call is `sair`? `tratar_syscall` calls `escalonador::encerrar`,
which never returns (doc 12): the process becomes `Terminado`, and its kernel
stack, with this frame on it, is freed later. Same as `int 0x80`.

> **Is the return RIP always canonical?** `sysret` on Intel CPUs has a
> famous trap: if RCX holds a non-canonical address (doc 04), the #GP
> exception happens **already in ring 0, with the program's RSP**, and the
> program gains control of the kernel stack. In 2012 this hit several
> systems at once. SiliconOS avoids it because the RCX on the way back is
> exactly what `syscall` saved (the instruction after the program's), and
> no system call changes it. A kernel that allowed changing the return RIP
> (for signals, say) would have to check first.

## 5. The programs switch to `syscall`

The built-in assembly programs (docs 10 and 12) keep using `int 0x80`: both
doors lead to the same `tratar_syscall`. The `sos` library of the Rust
programs (doc 13) switched doors, and with it every ELF switches at once,
without touching them:

```rust
/// A system call with up to two arguments, through the `syscall`
/// instruction (doc 14). Number in RAX, arguments in RDI and RSI, result in
/// RAX. The CPU itself uses RCX (return address) and R11 (RFLAGS): the
/// compiler must know both change (`lateout(...) _`).
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}

/// Ends the program with the given code. Does not return.
pub fn sair(codigo: u64) -> ! {
    chamada(SYS_SAIR, codigo, 0);
    unreachable!("o kernel voltou de sair()")
}

/// Writes the bytes to the console. Returns how many were written, or a
/// negative error (-14 if the kernel rejects the pointer).
pub fn escrever(bytes: &[u8]) -> i64 {
    chamada(SYS_ESCREVER, bytes.as_ptr() as u64, bytes.len() as u64) as i64
}

/// Timer ticks since boot (100 per second).
pub fn tiques() -> u64 {
    chamada(SYS_TIQUES, 0, 0)
}

/// This process's number.
pub fn pid() -> u64 {
    chamada(SYS_PID, 0, 0)
}
```

- **`lateout("rcx") _, lateout("r11") _`**: "these two registers come out of
  here holding garbage" ([§44](rust-basico.md#44-asm-clobbered-registers-and-inlinealways)).
  Without it, the compiler could leave an important value in RCX before the
  call and rely on it afterwards. With `int 0x80` it was not needed, because
  the kernel restores every register.
- **`sair`** can no longer use `options(noreturn)` directly (the call goes
  through `chamada`, which returns a value); `unreachable!` tells the
  compiler nothing gets past there, and if the kernel ever returns, the
  program panics instead of running on into garbage.
- Everything else in `sos` (`print!`, `println!`, `principal!`, panic) stays
  the same.

## 6. How much is gained: the `medir` program

A new program on the RAM disk measures the cost of the two doors, calling
the same system call (`tiques`, the cheapest) 100,000 times through each and
counting **CPU cycles** with the `rdtsc` instruction (*read time-stamp
counter*), which ring 3 may also use:

```rust
//! Measures how much a system call costs: `int 0x80` (doc 10) versus
//! `syscall` (doc 14), counting CPU cycles with `rdtsc`.
#![no_std]
#![no_main]

use core::arch::asm;
use sos::{println, principal};

principal!(main);

const VEZES: u64 = 100_000;

/// The CPU's cycle counter (Time Stamp Counter).
fn ciclos() -> u64 {
    let (alto, baixo): (u32, u32);
    unsafe { asm!("rdtsc", out("edx") alto, out("eax") baixo, options(nomem, nostack)) };
    (alto as u64) << 32 | baixo as u64
}

/// The same `tiques` call (number 2), through the old path.
fn tiques_int80() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") 2u64 => r, options(nostack)) };
    r
}

/// Average cycles per call of `f`.
fn medir(f: impl Fn() -> u64) -> u64 {
    f(); // the first time loads caches and TLB
    let inicio = ciclos();
    for _ in 0..VEZES {
        f();
    }
    (ciclos() - inicio) / VEZES
}

fn main() -> u64 {
    let int80 = medir(tiques_int80);
    let syscall = medir(sos::tiques);
    println!("{} chamadas de cada: int 0x80 = {} ciclos, syscall = {} ciclos por chamada",
             VEZES, int80, syscall);
    0
}
```

The work inside the kernel is the same in both measurements (reading the
tick counter); the only difference is the door. Timer ticks that land in
the middle (every 10 ms) affect both counts equally.

Real results, three times each, in two QEMU modes:

```
== TCG (QEMU emulates the CPU instruction by instruction)
100000 chamadas de cada: int 0x80 = 1751 ciclos, syscall = 748 ciclos por chamada
100000 chamadas de cada: int 0x80 = 1691 ciclos, syscall = 784 ciclos por chamada
100000 chamadas de cada: int 0x80 = 1712 ciclos, syscall = 803 ciclos por chamada

== KVM (the real CPU runs the code; QEMU with -enable-kvm -cpu host)
100000 chamadas de cada: int 0x80 = 843 ciclos, syscall = 219 ciclos por chamada
100000 chamadas de cada: int 0x80 = 887 ciclos, syscall = 229 ciclos por chamada
100000 chamadas de cada: int 0x80 = 858 ciclos, syscall = 265 ciclos por chamada
```

(*100000 calls of each: int 0x80 = N cycles, syscall = M cycles per call*.
The CPU in the KVM test, according to the shell's `cpu` command: Intel Core
i5-14450HX.)

- **On the real CPU, `syscall` costs about 230 cycles and `int 0x80` about
  860:** almost 4 times less. These numbers include everything: the two
  instructions in and out, our 15 `push`es and 15 `pop`s, the call to
  `tratar_syscall` and the loop itself.
- **Under TCG the difference drops to 2.2 times**, and the numbers say
  nothing about hardware: there, each instruction is emulated in software,
  and the cost depends on how QEMU implements each one.

## 7. How it was tested

### 7.1 From the shell

Besides `medir` (section 6), the programs from before:

```
silicon> rodar oi
Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 4.
'oi' terminou: codigo de saida 0
```

`oi`, `primos` and `dados` now enter the kernel only through `syscall`;
`ola`, `contador` and the other built-in ones, only through `int 0x80`;
`medir`, through both.

### 7.2 `cargo test`

48 tests, all `[ok]`: the 46 from before and 2 new ones in `syscall.rs`:

```rust
#[test_case]
fn msrs_configurados() {
    assert!(cpu::ler_msr(IA32_EFER) & EFER_SCE != 0);
    assert_eq!(cpu::ler_msr(IA32_LSTAR), entrada_syscall_rapida as *const () as u64);
    assert_eq!(cpu::ler_msr(IA32_STAR) >> 32, 0x0013_0008);
    assert_eq!(cpu::ler_msr(IA32_FMASK), 0x700);
}

#[test_case]
fn as_duas_portas_funcionam_no_mesmo_programa() {
    // `medir` calls `tiques` 100,000 times through `int 0x80` and 100,000
    // through `syscall`, and prints through `syscall`.
    let arquivo = crate::ramdisk::abrir("medir").unwrap();
    let pid = super::super::iniciar_elf(arquivo.nome, arquivo.dados, false).unwrap();
    assert_eq!(crate::escalonador::esperar(pid), super::super::Resultado::Saiu(0));
}
```

Two doc 13 tests needed adjusting, and both teach something:

- **`os_tres_programas_estao_no_disco`** became `os_programas_estao_no_disco`
  and expects `["dados", "medir", "oi", "primos"]`: the disk gained a file.
- **The "entry point outside the code" case** of
  `arquivos_ruins_recusados_sem_vazar` used the fixed address
  `0x2000_0000_1000`, which in doc 13 was the constants page. With the
  switch to `syscall` (and the `unreachable!` in `sair`, which brings the
  panic code along), `oi`'s code grew past 4 KiB, and that address landed
  **inside the code**, which is executable: the test got `Ok` instead of
  `EntradaInvalida`. The test now reads the second segment's address (the
  constants, R--) from the file itself, and no longer depends on the layout:

  ```rust
        // Entry point outside the code: the second segment's address (the
        // constants, R--), read from the file itself.
        assert_eq!(carregar_alterado(|d| { let p = ph(d) + 56 + 16; let v: [u8; 8] = d[p..p + 8].try_into().unwrap(); d[24..32].copy_from_slice(&v) }),
                   Err(ErroElf::EntradaInvalida));
  ```

### 7.3 And without EFER.SCE?

To see each MSR's role, the line that sets EFER.SCE was commented out (and
then restored):

```
silicon> rodar oi
[kernel] OPCODE INVALIDO no programa (RIP=0x200000000091, CS=0x23): #UD
'oi' encerrado pelo kernel: OPCODE INVALIDO em 0x200000000091
silicon> rodar ola
Ola do anel 3! (escrito por uma chamada de sistema)
'ola' terminou: codigo de saida 42
```

Disassembling `oi` (`objdump -d`), at `0x2000_0000_0091` sit the bytes
`0f 05`: the `syscall` instruction, inside `sos`'s `pid()` function. With
SCE off, the CPU treats `syscall` as an instruction that does not exist
(#UD, doc 04), and the kernel ends only the program (doc 10). `ola`, which
uses `int 0x80`, does not even notice.

### 7.4 Regression

The 21 builds with test features still pass.

## What is still missing

- **A single CPU.** With several, `RSP_DO_PROGRAMA` and `RSP0_ATUAL` would
  have to be one per CPU. Real kernels use the `swapgs` instruction to swap
  the GS register for a pointer to the current CPU's data right at the
  entry, and keep the program's RSP and the kernel stack there.
- **Non-maskable interrupts (NMI).** FMASK clears IF, but an NMI ignores IF.
  If one arrived between `syscall` and the stack switch, it would run on
  the program's RSP. SiliconOS does not use NMIs; systems that do give them
  a stack of their own through the IST (doc 03).
- **More arguments.** Linux's convention uses RDI, RSI, RDX, R10, R8 and R9
  (R10 in place of RCX, which `syscall` uses); our calls use two.

---

## Changes to the files

What this chapter adds to the code of doc 13 (tag `doc-13`). Each file's link
leads to its state at the end of this chapter (tag `doc-14`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-13...doc-14).

| File | What changes | Where |
|---|---|---|
| [`src/usuario/syscall.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | **new**: MSR constants, `init`, `RSP_DO_PROGRAMA`, `entrada_syscall_rapida`, 2 tests | — |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/mod.rs) | `pub mod syscall;` | after `pub mod programas;` |
| [`src/cpu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/cpu.rs) | `escrever_msr` | right after `ler_msr` |
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/gdt.rs) | `RSP0_ATUAL`; `definir_rsp0` updates it; `init` sets the initial value | `RSP0_ATUAL` after `definir_rsp0`; in `init`, right after `let tss = tss.0.get() ...` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/main.rs) | `usuario::syscall::init();` | right after `escalonador::init();` |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/lib.rs) | `chamada` (with `syscall`); `sair`, `escrever`, `tiques` and `pid` use it; top comment | the four functions |
| [`programas/src/bin/medir.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | **new** (section 6) | — |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/ramdisk.rs) | tests: `os_programas_estao_no_disco` with 4 names; count 4 in `cabecalho_estragado_para_a_leitura` | tests |
| [`src/usuario/elf.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/elf.rs) | test: invalid entry read from the file (section 7.2) | `arquivos_ruins_recusados_sem_vazar` |

### The snippets the text did not show

The top of `syscall.rs`:

```rust
use core::arch::naked_asm;

use crate::cpu;
use crate::gdt;
```

and the start of the tests:

```rust
#[cfg(test)]
mod testes {
    use super::*;

    // the 2 tests from section 7.2
}
```

In `gdt::init`, the line that gives `RSP0_ATUAL` its initial value:

```rust
    let tss = tss.0.get() as *const Tss;
    unsafe { RSP0_ATUAL = (&raw const (*tss).rsp as *const u64).read_unaligned() };
```

In `main.rs`:

```rust
    escalonador::init();
    // `syscall`/`sysret` (docs/14-syscall-sysret.md): STAR, LSTAR, FMASK and EFER.SCE MSRs.
    usuario::syscall::init();
```

The new top comment of `programas/src/lib.rs` (in the file, in Portuguese):

```rust
//! system library (libc on Linux). Here it is this file: every function is
//! a system call. Up to doc 13, through the `int 0x80` instruction (doc 10);
//! since doc 14, through the `syscall` instruction, built for this.
```

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `init` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | Writes STAR (selectors), LSTAR (entry), FMASK (IF, TF, DF) and sets EFER.SCE. |
| `entrada_syscall_rapida` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | (naked) Switches to the process's kernel stack, builds the same frame and registers as `int 0x80`, calls `tratar_syscall` and returns with `sysretq`. |
| `escrever_msr` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/cpu.rs) | `wrmsr`: writes an MSR (number in ECX, value in EDX:EAX). |
| `chamada` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/lib.rs) | Makes a system call through `syscall`, telling the compiler RCX and R11 change. |
| `ciclos` | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | `rdtsc`: the CPU's cycle counter. |
| `tiques_int80` | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | The `tiques` call through the old door, `int 0x80`. |
| `medir` | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | Average cycles per call of a function, over 100,000 repetitions. |
| `main` (medir) | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | Measures both doors and prints the cycles per call of each. |

Automated tests (`#[test_case]`, run with `cargo test`, see doc 09):

| Function | File | What it does |
|---|---|---|
| `msrs_configurados` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | Test: EFER.SCE set and STAR, LSTAR and FMASK holding the expected values. |
| `as_duas_portas_funcionam_no_mesmo_programa` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | Test: `medir` (which uses `int 0x80` and `syscall`) exits with 0. |

---

## Difference from the book

The book recommends starting system calls with a software interrupt, which
is easier to understand, and switching to `syscall`/`sysret` once the
behavior is right: that is what we did (doc 10 and this one). Our choices:
keeping both doors leading to the same handler (building the same frame by
hand), the copy of RSP0 in a `static` instead of `swapgs` (there is a single
CPU) and the measurement with `rdtsc`.

## Summary

- `int 0x80` goes through the general interrupt mechanism: IDT, privilege
  checks, TSS, a 5-value frame. `syscall`/`sysret` skip almost all of it:
  destinations come from **MSRs**, the return RIP goes in **RCX** and
  RFLAGS in **R11**.
- Configuration: **EFER.SCE** turns it on; **LSTAR** is the entry;
  **FMASK** clears IF, TF and DF; **STAR** gives the selectors, and its
  arithmetic requires user data before user code in the GDT (doc 03).
- `syscall` **does not switch stacks**: the entry saves the program's RSP
  and loads the process's kernel stack (`RSP0_ATUAL`, the copy of
  TSS.RSP0), with interrupts off.
- The entry builds **the same frame and registers** as `int 0x80`, and
  `tratar_syscall` serves both doors.
- The program loses RCX and R11 on every `syscall`: `sos` tells the compiler
  with `lateout(...) _`.
- Measured on the real CPU (KVM): **~230 cycles versus ~860**, almost 4
  times less.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **What does `int 0x80` do that `syscall` does not?**
   It reads the IDT and the GDT from memory, checks privileges, switches
   stacks through the TSS and pushes a 5-value frame. `syscall` takes
   everything from MSRs, keeps RIP and RFLAGS in RCX and R11 and does not
   switch stacks.

2. **Why must the `syscall` entry switch stacks before anything else, and
   why may it use a `static` to keep the program's RSP?**
   Because RSP is still the program's, which may point anywhere. The
   `static` is enough because there is a single CPU and interrupts are off
   (FMASK cleared IF): nobody else goes through there.

3. **Why does user data come before user code in the GDT?**
   Because `sysret` computes both selectors from a single number in STAR:
   SS = base + 8, CS = base + 16. With 0x18 (data) and 0x20 (code), the base
   is 0x10.

4. **Why didn't `tratar_syscall` need to change?**
   Because the new entry builds on the stack exactly what the `int 0x80`
   one builds: the frame (SS, RSP, RFLAGS, CS, RIP) and the 15 registers in
   the same order, which is the `Registros` struct.

5. **What would go wrong if `sos` did not declare `lateout("rcx") _` and
   `lateout("r11") _`?**
   The compiler could keep a value in RCX or R11 before the call and use it
   afterwards, not knowing that `syscall` overwrites them (with the return
   RIP and RFLAGS). The bug would show up far away, as a wrong value.

6. **Without EFER.SCE, what happens to a program that uses `syscall`? And to
   one that uses `int 0x80`?**
   The first dies with #UD (invalid instruction) on the `syscall`
   instruction itself; the kernel ends only that program. The second works
   normally: the `int 0x80` door does not depend on SCE.

</details>
