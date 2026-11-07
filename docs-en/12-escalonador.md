# 12: The scheduler

> **You will understand:** how a single CPU runs several programs "at the
> same time"; what a process's **context** is and how to switch it with a
> 9-instruction function that "returns" into another process's history;
> why each process needs **its own kernel stack**; how a new process starts
> from a **fabricated** stack; how the timer takes the CPU away from whoever
> does not give it back (**preemption**); and the rule that keeps the
> kernel from locking up when it is interrupted in the middle of a critical
> section.
>
> **Files:** `src/escalonador.rs` (new: processes, switching, round robin),
> `src/usuario/mod.rs` (`iniciar`, `rodar`, the `pid` syscall),
> `src/usuario/programas.rs` (the `contador` program), `src/gdt.rs`
> (`definir_rsp0`), the handler in `src/interrupcoes/timer.rs`, the
> shortcuts in `src/memoria/quadros.rs` and `heap.rs`, and the shell
> (`rodar ... &`, `ps`)
> **Book:** chapter 11, section 11.4 (the roadmap ends with "cooperative
> switching, then preemptive"; the implementation is ours)
> **Try it:** `./run.sh`, then `rodar contador &`, `rodar contador &`,
> `ps`, `rodar laco &`, `eco hi`; `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 08 (timer, `sem_interrupcoes`), doc 10 (ring 3,
> TSS.RSP0, `iretq`) and doc 11 (address spaces)
> **New Rust here:** `UnsafeCell` ([§39](rust-basico.md#39-unsafecell-mutating-through-a-shared-reference))
> **Names in the code:** `escalonador` = *scheduler*, `processo` =
> *process*, `estado` = *state* (`Pronto` = *ready*, `Rodando` = *running*,
> `Terminado` = *finished*), `tabela` = *table*, `criar` = *create*,
> `primeira_entrada` = *first entry*, `trocar` = *switch*, `ceder` =
> *yield*, `escolher_proximo` = *pick next*, `tique` = *tick*, `encerrar` =
> *terminate*, `recolher` = *reap*, `esperar` = *wait*, `segundo plano` =
> *background*, `pilha` = *stack*, `iniciar` = *start*, `contador` =
> *counter*, `laco` = *loop*
>
> **Code for this chapter:** [state at the end of doc 12](https://github.com/moises-dev-git/siliconOS/tree/doc-12) · [what changed since doc 11](https://github.com/moises-dev-git/siliconOS/compare/doc-11...doc-12) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

Up to doc 11, `rodar ola` worked like this: the shell entered ring 3 and
**stood still** until the program ended. With `rodar laco`, the shell
vanished for 2 seconds; no key was handled. One program at a time, and the
whole kernel waiting on it.

Doc 11 already gave each program its own address space, so two programs
**can** exist at the same time at the same virtual address. The other half
is missing: sharing **the CPU** between them.

## 2. The idea: sharing time

A CPU executes one instruction at a time. "Several programs at once" is an
illusion: the system runs a bit of one, **stops**, saves where it was, runs
a bit of another, and so on, fast enough to look simultaneous. The idea has
a name and a date: **time-sharing**, demonstrated in MIT's CTSS in 1961, so
that several users could share one computer through terminals.

Three words to keep in mind:

- **Process:** a running program, with everything it needs to continue
  where it stopped. In SiliconOS: the address space (doc 11), a kernel
  stack and the saved registers.
- **Context:** the CPU state that belongs to the process: the registers
  (RIP and RSP included), the flags, and which PML4 is in CR3. A **context
  switch** saves one and loads another.
- **Scheduler:** whoever decides who runs now. Ours uses **round robin**:
  each ready process runs for one tick (10 ms), and the turn passes to the
  next in line.

And two ways to take the CPU away from someone:

- **Cooperative:** the process gives the CPU back when it wants to (by
  calling something like `ceder()`, *yield*). Simple, but a program with an
  infinite loop never gives it back.
- **Preemptive:** the **timer** interrupts the process, whether it likes it
  or not, and the handler switches processes. Doc 10 already had the seed
  of this: the timer ended a program that went past 2 s. Now it does not
  end it: it **passes the turn**.

SiliconOS does both: the timer switches on every tick (preemptive), and the
kernel, when it has nothing to do, yields without waiting for the tick
(cooperative).

## 3. Each process needs its own kernel stack

When the timer interrupts a program in ring 3, the CPU switches to the
stack in **TSS.RSP0** (doc 10) and pushes the interrupt frame there (the
program's RIP, CS, RFLAGS, RSP, SS). The handler runs on top of that frame.
If the handler decides to switch to another process, **that frame has to
stay put** until the process comes back: it is what tells the `iretq` where
to return the program to.

Doc 10 had a single stack (`PILHA_ANEL0`). With two processes, it is not
enough:

```
   a single stack (wrong)                one per process (right)

   A is interrupted:                     A is interrupted:
     [A's frame] ◄─ RSP0                   A's stack: [A's frame]
   switch to B; B is interrupted:        switch to B; B is interrupted:
     [B's frame] ◄─ RSP0 (on top!)         B's stack: [B's frame]
   back to A: iretq uses B's frame       back to A: A's frame is intact
   → A continues in B's code,            on A's stack
     with B's stack
```

So each process gets a 16 KiB kernel stack, and the scheduler, on each
switch, writes into TSS.RSP0 the top of the stack of whoever is about to
run. The CPU reads the TSS from memory on every interrupt coming from ring
3, so the change takes effect immediately.

The TSS lived in a `Once<Tss>`, which only hands out shared references
(`&Tss`): no field can be changed through it. Rust's answer to "a value
that changes, reached through a shared reference" is `UnsafeCell`
([§39](rust-basico.md#39-unsafecell-mutating-through-a-shared-reference)):

```rust
struct CelulaTss(UnsafeCell<Tss>);
// Only the scheduler writes, always with interrupts off.
unsafe impl Sync for CelulaTss {}

static TSS: Once<CelulaTss> = Once::new();

pub fn definir_rsp0(topo: u64) {
    let tss = TSS.get().expect("gdt::init antes de definir_rsp0");
    unsafe { (&raw mut (*tss.0.get()).rsp as *mut u64).write_unaligned(topo) };
}
```

`UnsafeCell::get()` returns a raw pointer (`*mut Tss`), and the write is
`unsafe`: we guarantee nobody reads the TSS at the same time. The TSS is
`packed` (doc 03), so the `rsp` field is not 8-byte aligned; hence
`&raw mut` (which creates no reference, only a pointer) and
`write_unaligned`. `rsp` is an array of 3; `rsp[0]` is the first u64, at
the same address as the array.

## 4. The switch: a function that returns somewhere else

The heart of the scheduler is 9 instructions:

```rust
#[unsafe(naked)]
unsafe extern "sysv64" fn trocar(salvar: *mut u64, novo_rsp: u64) {
    naked_asm!(
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "ret",
    )
}
```

Reading it slowly, with process A calling `trocar(&A.rsp, B.rsp)`:

1. `call trocar` pushed, on A's stack, the return address (where A
   continues).
2. The 6 `push`es save the registers the `sysv64` convention says the
   called function must preserve (doc 10,
   [§37](rust-basico.md#37-calling-conventions-extern-sysv64)). The others
   (RAX, RCX, RDX...) the caller already considers lost after any call, so
   they need not be saved.
3. `mov [rdi], rsp`: A's RSP (now pointing at those 6 values and the return
   address) goes into `A.rsp`. **That number is A's whole context**: the
   rest is on the stack it points to.
4. `mov rsp, rsi`: the CPU starts using B's stack. From here on, we are
   "in B".
5. The 6 `pop`s take from B's stack the registers **B** saved when it
   called `trocar`, long ago.
6. `ret` pops **B's** return address: the CPU continues at the instruction
   after the `trocar` that B called, a long time ago.

```
  A's stack                          B's stack
  ┌──────────────────┐               ┌──────────────────┐
  │ ... (handler)    │               │ ... (handler)    │
  │ return to A      │               │ return to B      │ ◄─ ret comes back here
  │ rbx rbp r12..r15 │ ◄─ A.rsp      │ rbx rbp r12..r15 │ ◄─ B.rsp: the pops read here
  └──────────────────┘               └──────────────────┘
```

For B, the call to `trocar` simply "took a while": it went in, and one day
it returned. For A, the same will happen when someone calls
`trocar(&X.rsp, A.rsp)`. Every process sees `trocar` as an ordinary function
that takes an unpredictable amount of time. It is the same trick as doc 10's
`voltar_ao_kernel` (restore a saved RSP and `ret`), now generalized: instead
of one kernel RSP, one per process.

## 5. A new process: the fabricated stack

And the **first** time? A process that was just created never called
`trocar`; there are no registers or return address on its stack. The
solution is to fabricate the stack as if it had:

```rust
pub fn criar(nome: &'static str, espaco: EspacoEnderecos, rip: u64, rsp: u64, segundo_plano: bool) -> Pid {
    // The process's kernel stack, prepared as if it had already run and
    // been switched out: the first `trocar` into it pops the 6 registers,
    // and the `ret` lands in `primeira_entrada`, which does the iretq.
    //
    //   top ─► [ SS          ]  ┐
    //          [ user RSP    ]  │ frame the iretq pops
    //          [ RFLAGS      ]  │
    //          [ CS          ]  │
    //          [ RIP         ]  ┘
    //          [ primeira_entrada ]   return address of `trocar`
    //          [ rbx rbp r12 r13 r14 r15 = 0 ]   ◄─ saved rsp
    let mut pilha = alloc::vec![0u64; TAM_PILHA_KERNEL / 8].into_boxed_slice();
    let n = pilha.len();
    pilha[n - 1] = gdt::USER_DATA as u64;
    pilha[n - 2] = rsp;
    pilha[n - 3] = 0x202; // IF on
    pilha[n - 4] = gdt::USER_CODE as u64;
    pilha[n - 5] = rip;
    pilha[n - 6] = primeira_entrada as *const () as u64;
    // pilha[n - 12 .. n - 6]: the 6 registers, already zeroed.
    let rsp_kernel = &pilha[n - 12] as *const u64 as u64;

    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let pid = t.proximo_pid;
        t.proximo_pid += 1;
        t.processos.push(Box::new(Processo {
            pid, nome, estado: Estado::Pronto, rsp: rsp_kernel,
            pilha: Some(pilha), espaco: Some(espaco), tiques_cpu: 0, segundo_plano,
        }));
        pid
    })
}

#[unsafe(naked)]
unsafe extern "sysv64" fn primeira_entrada() -> ! {
    naked_asm!(
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor edx, edx",
        "xor esi, esi", "xor edi, edi", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
    )
}
```

When the scheduler picks this process for the first time, `trocar` loads
`rsp_kernel`, the 6 `pop`s read zeros, and the `ret` jumps to
`primeira_entrada`. It zeroes the registers (the program must not see kernel
values) and runs `iretq`, which pops the hand-built frame and enters ring 3
at `rip`. It is exactly doc 10's fake `iretq`, just built ahead of time, on
the process's own stack.

Details:

- **The stack is a `Box<[u64]>`** on the heap: 8-byte aligned and indexable
  like an array. `vec![0u64; N].into_boxed_slice()` creates the zeroed
  vector and turns it into a fixed-size block.
- **RFLAGS = 0x202:** bit 9 (IF) on, so the program can be interrupted by
  the timer (bit 1 is always 1).
- **`primeira_entrada as *const () as u64`:** the function's address as a
  number. Rust asks for the intermediate step through a pointer
  (`*const ()`) to make it clear this is an address, not a value
  conversion.
- **The space comes ready:** `usuario::iniciar` (section 9) creates the
  space, copies the code and maps the program's stack, and only then calls
  `criar`. The scheduler looks after the CPU; the `usuario` module prepares
  the memory.

## 6. The process table and round robin

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// Can run; waiting for its turn.
    Pronto,
    /// The process on the CPU right now.
    Rodando,
    /// Ended; waiting for someone to collect the result and free the memory.
    Terminado(Resultado),
}

pub struct Processo {
    pub pid: Pid,
    pub nome: &'static str,
    pub estado: Estado,
    /// RSP saved by `trocar` when the process leaves the CPU.
    rsp: u64,
    /// Kernel stack (None for process 0, which uses the boot stack).
    pilha: Option<Box<[u64]>>,
    /// Address space (None for process 0: it uses the kernel's tables).
    espaco: Option<EspacoEnderecos>,
    /// Timer ticks that caught this process in ring 3.
    pub tiques_cpu: u64,
    /// Nobody waits for it: the result is printed when it ends.
    pub segundo_plano: bool,
}

struct Tabela {
    /// Box: the address of each `Processo` does not change when the Vec
    /// grows (`trocar` keeps a pointer to the `rsp` field).
    processos: Vec<Box<Processo>>,
    /// Index (in `processos`) of whoever is running.
    atual: usize,
    proximo_pid: Pid,
}

static TABELA: Mutex<Tabela> = Mutex::new(Tabela { processos: Vec::new(), atual: 0, proximo_pid: 1 });
```

**Process 0 is the kernel.** The code that is running when the scheduler
starts (the boot, then the shell) also needs a slot in the table: when the
timer interrupts it and switches to a program, `trocar` needs an `rsp` in
which to save its context. `init` registers it:

```rust
pub fn init() {
    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        assert!(t.processos.is_empty(), "escalonador::init chamado duas vezes");
        t.processos.push(Box::new(Processo {
            pid: 0, nome: "kernel", estado: Estado::Rodando, rsp: 0,
            pilha: None, espaco: None, tiques_cpu: 0, segundo_plano: false,
        }));
    });
}
```

It has no stack of its own (it uses the boot one), no space of its own (it
uses the kernel's tables), and never goes to ring 3. `main.rs` calls `init`
right before `interrupcoes::init`, because the very first tick already calls
the scheduler.

**Why `Vec<Box<Processo>>` and not `Vec<Processo>`?** When a `Vec` grows, it
may move its elements around in memory. `trocar` gets a pointer to a
process's `rsp` field and writes through it **after** the lock has been
released; if the process had moved in the meantime, the write would land in
freed memory. With `Box`, the `Vec` holds only pointers; the `Processo`
itself stays put on the heap.

### 6.1 Picking and switching

```rust
pub fn ceder() {
    cpu::sem_interrupcoes(|| {
        if let Some((salvar, novo_rsp)) = escolher_proximo() {
            unsafe { trocar(salvar, novo_rsp) };
        }
    });
}

fn escolher_proximo() -> Option<(*mut u64, u64)> {
    let mut t = TABELA.lock();
    let n = t.processos.len();
    let atual = t.atual;
    let proximo = (1..n).map(|d| (atual + d) % n).find(|&i| t.processos[i].estado == Estado::Pronto)?;

    if t.processos[atual].estado == Estado::Rodando {
        t.processos[atual].estado = Estado::Pronto;
    }
    t.processos[proximo].estado = Estado::Rodando;
    t.atual = proximo;

    let p = &t.processos[proximo];
    if p.pilha.is_some() {
        gdt::definir_rsp0(p.topo_pilha());
    }
    paginacao::ativar(p.espaco.as_ref().map_or(paginacao::raiz_kernel(), |e| e.raiz()));

    let novo_rsp = p.rsp;
    let salvar = &mut t.processos[atual].rsp as *mut u64;
    Some((salvar, novo_rsp))
}
```

- **Round robin** is the `find` line: starting from whoever is running, it
  looks at the next one, the one after, ... wrapping around (`% n`), and
  takes the first `Pronto`. If there is no other, the `?` returns `None`
  and `ceder` returns without switching.
- **CR3 changes before the stack switch.** It looks dangerous (we are
  running on the old process's stack with the new one's tables), but it is
  not: kernel stacks live on the heap, which belongs to the kernel and looks
  the same in every space (doc 11). Process 0 uses the kernel's tables
  (`raiz_kernel()`).
- **The lock is released before `trocar`.** The `MutexGuard` `t` dies at
  the end of `escolher_proximo`. If `trocar` happened with the lock held,
  the next process (which one day calls `ceder`) would spin forever waiting
  for a lock only the old, stopped process can release.
- **All with interrupts off** (`sem_interrupcoes`): a tick in the middle of
  `escolher_proximo` would call `ceder` again, with the table half updated.
  And notice a nice detail: `sem_interrupcoes` keeps the IF state in a local
  variable, **on the caller's stack**. When the process runs again (its
  `trocar` "returns"), the closure ends and IF goes back to what **that
  process** had. Each process has its own.

### 6.2 The timer passes the turn

The timer handler (doc 08) now ends like this:

```rust
    #[cfg(not(feature = "teste-sem-eoi"))]
    apic::eoi();

    // The next process's turn (docs/12-escalonador.md). After the EOI:
    // this handler only finishes when this process runs again, and until
    // then the APIC must keep delivering ticks.
    crate::escalonador::tique(&quadro);
}
```

```rust
pub fn tique(quadro: &QuadroInterrupcao) {
    if quadro.cs & 3 == 3 {
        // The tick caught a program in ring 3: count its CPU time.
        let excedeu = cpu::sem_interrupcoes(|| {
            let mut t = TABELA.lock();
            let a = t.atual;
            let p = &mut t.processos[a];
            p.tiques_cpu += 1;
            p.tiques_cpu >= usuario::LIMITE_TIQUES
        });
        if excedeu {
            encerrar(Resultado::TempoEsgotado);
        }
    }
    ceder();
}
```

- **The EOI comes first.** If the handler switched processes before the
  EOI, the APIC would think the timer interrupt was still in progress and
  would not deliver the next tick (that is doc 08's `teste-sem-eoi`). The
  new process would run forever.
- **The 2 s limit became CPU time.** In doc 10, "2 s" was wall-clock time
  since the program started. With several processes that would be unfair:
  a program waiting for its turn would age too. Now only the ticks that
  caught the process **running** in ring 3 count.
- **The handler "stays suspended".** When `ceder` switches processes, A's
  timer handler stays stopped in the middle, on A's stack. It only finishes
  (and `iretq`s back to program A) when A is picked again.

The full path of a switch between two programs:

```
  A in ring 3 ──tick──► CPU pushes A's frame on A's stack (TSS.RSP0)
                        timer handler: tiques++, EOI, tique()
                        ceder() → escolher_proximo(): B; RSP0 = B's stack; CR3 = B
                        trocar(&A.rsp, B.rsp)
                               │
                               └──► ... continues inside B's ceder() (the previous tick)
                                    tique() ends, handler ends
  B in ring 3 ◄─────────────────────  iretq with B's frame
```

## 7. Ending and reaping

A program ends in three ways (doc 10: `sair`, an exception, timeout), and
all three call `escalonador::encerrar`:

```rust
pub fn encerrar(resultado: Resultado) -> ! {
    cpu::desligar_interrupcoes();
    let (pid, nome, segundo_plano) = {
        let mut t = TABELA.lock();
        let a = t.atual;
        assert!(a != 0, "o kernel (processo 0) nao pode ser encerrado");
        let p = &mut t.processos[a];
        p.estado = Estado::Terminado(resultado);
        (p.pid, p.nome, p.segundo_plano)
    };
    if segundo_plano {
        kprintln!("\n[{}] '{}' {}", pid, nome, usuario::descrever(resultado));
    }
    ceder();
    unreachable!("processo encerrado voltou a rodar");
}
```

The process becomes `Terminado` and yields. Since `escolher_proximo` only
picks `Pronto` processes, it never runs again.

But nobody frees its memory here, on purpose: `encerrar` is running **on the
process's own kernel stack**. Freeing that stack now would be sawing off the
branch again (doc 11). The process stays in the table as `Terminado`, with
its result stored, until someone **reaps** it from another process. (Unix
systems call a process in this state a **zombie**: it has died but still
takes up a row in the table until its parent reads the result.)

```rust
fn retirar(t: &mut Tabela, i: usize) -> (Box<Processo>, Resultado) {
    let Estado::Terminado(r) = t.processos[i].estado else { unreachable!() };
    if i < t.atual {
        t.atual -= 1; // the indices after `i` shift by one
    }
    (t.processos.remove(i), r)
}

pub fn recolher(pid: Pid) -> Option<Resultado> {
    let (processo, r) = cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let i = t.processos.iter().position(|p| p.pid == pid && matches!(p.estado, Estado::Terminado(_)))?;
        Some(retirar(&mut t, i))
    })?;
    // Free outside the lock: the space's Drop uses the frame allocator.
    drop(processo);
    Some(r)
}

pub fn esperar(pid: Pid) -> Resultado {
    loop {
        if let Some(r) = recolher(pid) {
            return r;
        }
        ceder();
    }
}
```

- `drop(processo)` frees everything at once: the `Box<Processo>`, the stack
  (`Box<[u64]>`, back to the heap) and the `EspacoEnderecos`, whose `Drop`
  (doc 11) returns pages, tables and the PML4. Whoever reaps runs in another
  process, so neither the dead one's stack nor its CR3 is in use.
- **`esperar` does not spin for nothing:** while the awaited process has not
  ended, it yields. The kernel only looks again when round robin comes back
  to it.
- `recolher_segundo_plano` does the same for every finished background
  process, without returning a result (they already printed theirs in
  `encerrar`).

## 8. The lock rule

With preemption, the timer can switch processes **between any two
instructions** where interrupts are on. Picture the shell (process 0)
asking for a frame:

```
  shell: quadros::alocar() → QUADROS.lock() → got the lock
         ── tick! ── the scheduler switches to program A
  A:     int 0x80 → sair → encerrar → ...
  shell (later)...
```

So far, nothing bad. But if, after the tick, another process tried to take
`QUADROS` **with interrupts off** (inside a system call, for example: `int
0x80` clears IF), it would spin waiting for the lock, and with IF off no
tick would come to hand the CPU back to the shell, the only one who can
release the lock. Locked up forever.

The rule that prevents this is simple: **every kernel lock is taken with
interrupts off.** That way, whoever holds a lock is never switched out in
the middle. The heap and `_print` already followed the rule since doc 08
(because of the handlers); now the frame allocator's shortcuts do too:

```rust
/// Shortcuts to use the global allocator without dealing with Mutex/Option.
/// With interrupts off (since doc 12): with the scheduler, the timer may
/// switch processes at any moment; if it switched with the lock held, the
/// next one asking for a frame would spin forever.
pub fn alocar() -> Option<Quadro> {
    cpu::sem_interrupcoes(|| QUADROS.lock().as_mut().expect("alocador de quadros nao inicializado").alocar())
}
```

(and the same in `liberar`, `livres`, `heap::estatisticas`, `heap::contem`
and in the `mem` command's read of `QUADROS`). The process table is also
only touched inside `sem_interrupcoes`.

## 9. The programs and the shell

### 9.1 `iniciar` and `rodar`

Doc 11's `rodar` did everything: prepared the memory, entered ring 3 and
waited. Now it is split in two:

```rust
pub fn iniciar(programa: &programas::Programa, segundo_plano: bool) -> Pid {
    // ... creates the space, copies the code, maps code and stack (as in doc 11) ...
    escalonador::criar(programa.nome, espaco, CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO, segundo_plano)
}

pub fn rodar(programa: &programas::Programa) -> Resultado {
    let pid = iniciar(programa, false);
    escalonador::esperar(pid)
}
```

Gone from the `usuario` module: `entrar_no_anel3`, `voltar_ao_kernel`,
`encerrar`, `tempo_esgotado` and the variables `ATIVO`, `INICIO`,
`RESULTADO` and `RSP_KERNEL`. Their roles went to `criar`/`primeira_entrada`,
`trocar`, `escalonador::encerrar` and `tique`.

### 9.2 A new system call: `pid`

```rust
pub const SYS_PID: u64 = 3;
...
        SYS_PID => escalonador::pid_atual() as u64,
```

And a program that uses it, `contador`: it writes the last digit of its own
pid 5 times, waiting 50 ticks (500 ms) between them:

```rust
    "programa_contador_inicio:",
    "    mov eax, 3",               // pid()
    "    int 0x80",
    "    xor edx, edx",
    "    mov ecx, 10",
    "    div rcx",                  // rdx = pid % 10 (the last digit)
    "    add dl, 0x30",             // becomes the ASCII character ('0' = 0x30)
    "    sub rsp, 16",              // 2 bytes on the program's stack: digit and space
    "    mov byte ptr [rsp], dl",
    "    mov byte ptr [rsp + 1], 0x20",
    "    mov r12d, 5",              // 5 times (int 0x80 preserves everything but rax)
    "3:  mov rdi, rsp",
    "    mov esi, 2",
    "    mov eax, 1",               // escrever(rsp, 2)
    "    int 0x80",
    "    mov eax, 2",               // tiques()
    "    int 0x80",
    "    lea rbx, [rax + 50]",      // wait 50 ticks (500 ms)
    "4:  mov eax, 2",
    "    int 0x80",
    "    cmp rax, rbx",
    "    jb 4b",
    "    dec r12d",
    "    jnz 3b",
    "    xor edi, edi",             // sair(0)
    "    xor eax, eax",
    "    int 0x80",
    "programa_contador_fim:",
```

`div rcx` divides RDX:RAX (128 bits) by RCX and leaves the quotient in RAX
and the **remainder** in RDX; that is why RDX is zeroed first. The text that
`escrever` receives sits on the program's stack, which `faixa_do_programa`
accepts (doc 10).

### 9.3 The shell: `&` and `ps`

`rodar <program> &` starts it in the background and returns to the prompt:

```rust
    if argumentos.get(1) == Some(&"&") {
        // Background (doc 12): the shell does not wait. When the program
        // ends, the scheduler prints the result.
        let pid = usuario::iniciar(programa, true);
        kprintln!("[{}] '{}' em segundo plano", pid, programa.nome);
    } else {
        let r = usuario::rodar(programa);
        kprintln!("'{}' {}", programa.nome, usuario::descrever(r));
    }
```

`ps` lists the table (`escalonador::listar`). And the shell's main loop (doc
09) gained three lines: reap the background programs that have ended and,
if another process is ready, **yield** instead of sleeping:

```rust
        escalonador::recolher_segundo_plano();
        if escalonador::ha_outros_prontos() {
            escalonador::ceder();
        } else {
            teclado::esperar();
        }
```

Without this, the shell would spend a whole tick asleep in `hlt` on every
round, time the programs could use. With `ceder`, the shell checks the
keyboard and passes the turn right away; a key waits at most one tick
(10 ms) to be handled.

## 10. How it was tested

### 10.1 From the shell

Real output (serial). The keys were sent through the QEMU monitor, one every
80 ms, so the programs' digits show up **in the middle** of what is being
typed: that is preemption happening on screen.

```
silicon> rodar contador &
[1] 'contador' em segundo plano
1 silicon> rodar 1 contad1 or &
[2] 'contador' em segundo plano
2 silicon> p1 s
  PID  ESTADO      CPU (tiques)  NOME
    0  rodando                0  kernel
    1  pronto               153  contador
    2  pronto                13  contador
silicon> 2 1 2 
[1] 'contador' terminou: codigo de saida 0
2 2 
[2] 'contador' terminou: codigo de saida 0
rodar laco &
[3] 'laco' em segundo plano
silicon> eco ainda respondo
ainda respondo
silicon> ps
  PID  ESTADO      CPU (tiques)  NOME
    0  rodando                0  kernel
    3  pronto               181  laco
silicon> 
[3] 'laco' encerrado pelo kernel: usou 2 s de CPU sem terminar
ps
  PID  ESTADO      CPU (tiques)  NOME
    0  rodando                0  kernel
silicon> mem
quadros: 20298 livres de 32768 (79 MiB livres)
heap: 224 bytes em uso (4 alocacoes vivas), pico 32 KiB
      mapeado 1024 KiB, 1 blocos livres, maior 1023 KiB
```

(`em segundo plano` = *in the background*; `terminou: codigo de saida` =
*ended: exit code*; `eco ainda respondo` = *echo I still answer*; `usou 2 s
de CPU sem terminar` = *used 2 s of CPU without finishing*.)

Reading it:

- **Two counters at once:** the digits `1` and `2` alternate.
- **The shell stays alive** with `laco` running: `eco ainda respondo` was
  handled right away. In doc 11 the shell would be silent for 2 s.
- **`ps` shows who uses CPU.** Process 0 has 0 ticks because only ring-3
  time is counted. `contador` 1 already had 153 ticks: it does **not
  sleep** while waiting, it keeps asking `tiques()` nonstop (busy waiting).
  Whoever waits like this spends other processes' CPU; the remedy would be
  a `dormir` (*sleep*) system call that keeps the process out of the
  rotation until the right time (see "What is still missing").
- **Nothing leaks:** before this session, `mem` also showed 20298 free
  frames and 224 bytes of heap in use. (That is 4 live allocations, 2 more
  than in doc 11: the process table's `Vec` and process 0's `Box`.)

### 10.2 `cargo test`

40 tests, all `[ok]` (the 36 from before and 4 new ones in
`escalonador.rs`). The concurrency ones print the measured time:

```
siliconos::escalonador::testes::dois_contadores_ao_mesmo_tempo ... 1 2 1 2 1 2 1 2 1 2 (251 tiques) [ok]
siliconos::escalonador::testes::dois_lacos_dividem_a_cpu ... (400 tiques) [ok]
siliconos::escalonador::testes::o_kernel_e_o_processo_zero ... [ok]
siliconos::escalonador::testes::processo_terminado_devolve_pilha_e_quadros ... Ola do anel 3! (escrito por uma chamada de sistema)
[ok]
```

- **Two counters, 251 ticks.** Each one takes 5 × 50 = 250 wall-clock
  ticks. One after the other would be 500; together, 251.
- **Two loops, 400 ticks.** Each loop only dies after 200 **CPU** ticks.
  Since they split the CPU half and half, together they take 400 wall-clock
  ticks: proof that round robin is fair.

```rust
#[test_case]
fn dois_contadores_ao_mesmo_tempo() {
    // Each counter takes 5 x 50 = 250 ticks. One after the other: 500.
    // At the same time, the total is close to 250.
    let inicio = timer::tiques();
    let (a, b) = (iniciar("contador"), iniciar("contador"));
    assert_eq!(esperar(a), Resultado::Saiu(0));
    assert_eq!(esperar(b), Resultado::Saiu(0));
    let total = timer::tiques() - inicio;
    crate::kprint!("({} tiques) ", total);
    assert!(total < 300, "levou {} tiques", total);
}

#[test_case]
fn dois_lacos_dividem_a_cpu() {
    // Each loop is only ended after 200 ticks OF CPU. Sharing the CPU,
    // both together need about 400 wall-clock ticks.
    let inicio = timer::tiques();
    let (a, b) = (iniciar("laco"), iniciar("laco"));
    assert_eq!(esperar(a), Resultado::TempoEsgotado);
    assert_eq!(esperar(b), Resultado::TempoEsgotado);
    let total = timer::tiques() - inicio;
    crate::kprint!("({} tiques) ", total);
    assert!(total >= 2 * usuario::LIMITE_TIQUES - 10, "levou so {} tiques", total);
}

#[test_case]
fn processo_terminado_devolve_pilha_e_quadros() {
    let (livres, heap_antes) = (quadros::livres(), heap::estatisticas().em_uso);
    let pid = iniciar("ola");
    assert_eq!(esperar(pid), Resultado::Saiu(42));
    assert_eq!(quadros::livres(), livres);
    assert_eq!(heap::estatisticas().em_uso, heap_antes);
    assert!(listar().iter().all(|p| p.0 != pid));
}
```

Doc 10's 8 tests (`ola_sai_com_42`, `laco_encerrado_pelo_timer`...) passed
without changing a line: `usuario::rodar` still returns the same
`Resultado`, only now through the scheduler.

### 10.3 And without `definir_rsp0`?

To see section 3 happen, the line `gdt::definir_rsp0(p.topo_pilha());` was
commented out (and then restored). Every program now uses the same ring-0
stack (doc 10's `PILHA_ANEL0`, which is still in RSP0). `cargo test` falls
over on the first test with two programs:

```
siliconos::escalonador::testes::dois_contadores_ao_mesmo_tempo ... 2 2 2 2 2 [kernel] PAGE FAULT no programa (RIP=0x200000000057, CS=0x23): ao ler 0x0 (pagina nao mapeada, modo usuario)

!!! KERNEL PANIC !!!
Local: src/escalonador.rs:343:9
Mensagem: assertion `left == right` failed
  left: Falha { excecao: "PAGE FAULT", rip: 35184372088919 }
 right: Saiu(0)
```

What happened: only the `2`s show up, and process 1 dies at
`0x2000_0000_0057`. Disassembling `contador`, the last instruction is the
`int 0x80` of `sair`, at bytes `0x55`–`0x56`; **`0x57` is that call's return
address**, right after the end of the program. When process 2 called `sair`,
the CPU pushed its frame (RIP = `...0057`) on the shared stack, **on top of
the frame process 1 had left there**. When 1's turn came back, the `iretq`
used 2's frame and sent 1 to `...0057`. The page holds zeros there, which
the CPU reads as `add [rax], al`; with RAX = 0, a page fault reading address
0. It is section 3's diagram, happening for real.

### 10.4 Regression

The 21 builds with test features still pass.

## What is still missing

- **Busy waiting.** A program cannot sleep: `contador` and `relogio` burn
  CPU asking for the time. A `Dormindo` (or `Bloqueado`) state and a
  `dormir(ticks)` call are missing; the same would apply to the shell
  waiting for the keyboard.
- **Pure round robin.** No priorities: everyone gets the same slice.
- **One CPU.** With several (SMP), each would have its own "current", and
  locks taken with interrupts off would not be enough (another CPU keeps
  running).
- **Processes only from built-in programs.** The next step loads programs
  from "files" in the ELF format. *(Done in [doc 13](13-disco-em-ram-e-elf.md).)*

---

## Changes to the files

What this chapter adds to the code of doc 11 (tag `doc-11`). Each file's link
leads to its state at the end of this chapter (tag `doc-12`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-11...doc-12).

| File | What changes | Where |
|---|---|---|
| [`src/escalonador.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | **new**: `Estado`, `Processo`, `Tabela`, `TABELA`, `init`, `criar`, `primeira_entrada`, `trocar`, `ceder`, `escolher_proximo`, `tique`, `encerrar`, `retirar`, `recolher`, `recolher_segundo_plano`, `esperar`, `pid_atual`, `ha_outros_prontos`, `listar`, 4 tests | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/main.rs) | `mod escalonador;`; `escalonador::init()` | `mod` after `mod usuario;`; `init` right before `interrupcoes::init()` |
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/gdt.rs) | `use core::cell::UnsafeCell;`; `CelulaTss`; `TSS` becomes `Once<CelulaTss>`; `init` wraps the TSS; `topo_pilha_anel0` and `topo_pilha_double_fault` read through the cell; new `definir_rsp0` | see the snippets below |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/interrupcoes/timer.rs) | the `tempo_esgotado` block goes away; `escalonador::tique(&quadro)` comes in at the end | `tratador` |
| [`src/memoria/quadros.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/memoria/quadros.rs) | `use crate::cpu;`; `alocar`, `liberar`, `livres` inside `sem_interrupcoes` | the three shortcuts |
| [`src/memoria/heap.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/memoria/heap.rs) | `estatisticas` and `contem` inside `sem_interrupcoes` | both functions |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/mod.rs) | out: `ATIVO`, `INICIO`, `RESULTADO`, `RSP_KERNEL`, `encerrar`, `tempo_esgotado`, `entrar_no_anel3`, `voltar_ao_kernel`; in: `SYS_PID`, `descrever`, `iniciar`; `rodar` = `iniciar` + `esperar`; `falha_do_programa` calls `escalonador::encerrar`; `tratar_syscall` with `SYS_PID` | see the snippets below |
| [`src/usuario/programas.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/programas.rs) | the `contador` program (assembly, symbols, entry in `PROGRAMAS`, which now has 7); `laco`'s description | end of the `global_asm!`; end of the `extern` block; end of `PROGRAMAS` |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/shell.rs) | `use crate::escalonador;`; `rodar ... &`; `ps` command; `rodar()`'s loop reaps and yields; `mem` reads `QUADROS` with interrupts off | — |

### The snippets the text did not show

In `gdt.rs`, the start and end of `init` with the cell:

```rust
pub fn init() {
    let tss = TSS.call_once(|| CelulaTss(UnsafeCell::new({
        // The stack grows down: the CPU needs the address of the TOP.
        // ... (same as before) ...
            iomap_base: size_of::<Tss>() as u16,
        }
    })));
    let tss = tss.0.get() as *const Tss;

    let gdt = GDT.call_once(|| {
        let (tss_baixo, tss_alto) =
            descritor_tss(tss as u64, (size_of::<Tss>() - 1) as u64);
        // ...
```

and the reads:

```rust
pub fn topo_pilha_anel0() -> u64 {
    TSS.get().map(|t| unsafe { (&raw const (*t.0.get()).rsp as *const u64).read_unaligned() }).unwrap_or(0)
}

pub fn topo_pilha_double_fault() -> u64 {
    TSS.get().map(|t| { let ist = unsafe { (*t.0.get()).ist }; ist[(IST_DOUBLE_FAULT - 1) as usize] }).unwrap_or(0)
}
```

In `escalonador.rs`, the `use` lines, the stack size and the top:

```rust
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::arch::naked_asm;
use spin::Mutex;

use crate::cpu;
use crate::gdt;
use crate::idt::QuadroInterrupcao;
use crate::kprintln;
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao;
use crate::usuario::{self, Resultado};

pub type Pid = u32;

/// Size of each process's kernel stack.
const TAM_PILHA_KERNEL: usize = 16 * 1024;

impl Processo {
    /// Address of the top of the kernel stack (where the CPU starts pushing).
    fn topo_pilha(&self) -> u64 {
        self.pilha.as_ref().map_or(0, |p| p.as_ptr() as u64 + (p.len() * 8) as u64)
    }
}
```

the queries:

```rust
pub fn recolher_segundo_plano() {
    loop {
        let retirado = cpu::sem_interrupcoes(|| {
            let mut t = TABELA.lock();
            let i = t.processos.iter().position(|p| p.segundo_plano && matches!(p.estado, Estado::Terminado(_)))?;
            Some(retirar(&mut t, i))
        });
        match retirado {
            Some((processo, _)) => drop(processo),
            None => return,
        }
    }
}

pub fn pid_atual() -> Pid {
    cpu::sem_interrupcoes(|| {
        let t = TABELA.lock();
        t.processos.get(t.atual).map_or(0, |p| p.pid)
    })
}

pub fn ha_outros_prontos() -> bool {
    cpu::sem_interrupcoes(|| {
        let t = TABELA.lock();
        t.processos.iter().enumerate().any(|(i, p)| i != t.atual && p.estado == Estado::Pronto)
    })
}

pub fn listar() -> Vec<(Pid, &'static str, Estado, u64)> {
    cpu::sem_interrupcoes(|| {
        TABELA.lock().processos.iter().map(|p| (p.pid, p.nome, p.estado, p.tiques_cpu)).collect()
    })
}
```

and the start of the tests:

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::interrupcoes::timer;
    use crate::memoria::{heap, quadros};
    use crate::usuario::programas;

    fn iniciar(nome: &str) -> Pid {
        usuario::iniciar(programas::procurar(nome).unwrap(), false)
    }

    #[test_case]
    fn o_kernel_e_o_processo_zero() {
        assert_eq!(pid_atual(), 0);
        let lista = listar();
        assert_eq!((lista[0].0, lista[0].1, lista[0].2), (0, "kernel", Estado::Rodando));
    }

    // the other 3 are in section 10.2
}
```

In `usuario/mod.rs`, the `use` lines (the atomics, `Mutex`, `gdt` and `cpu`
go away):

```rust
use alloc::format;
use alloc::string::String;
use core::arch::naked_asm;

use crate::escalonador::{self, Pid};
use crate::idt::QuadroInterrupcao;
use crate::interrupcoes::timer;
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao::{self, flags};
use crate::memoria::{quadros, TAMANHO_QUADRO};
use crate::{kprint, kprintln};
```

`descrever` (used by the shell and by `encerrar`) and the new version of
`falha_do_programa`:

```rust
pub fn descrever(r: Resultado) -> String {
    match r {
        Resultado::Saiu(codigo) => format!("terminou: codigo de saida {}", codigo as i64),
        Resultado::Falha { excecao, rip } => format!("encerrado pelo kernel: {} em {:#x}", excecao, rip),
        Resultado::TempoEsgotado => format!("encerrado pelo kernel: usou {} s de CPU sem terminar",
                                            LIMITE_TIQUES / timer::FREQUENCIA_HZ),
    }
}

pub fn falha_do_programa(quadro: &QuadroInterrupcao, excecao: &'static str, detalhes: core::fmt::Arguments) {
    if quadro.cs & 3 == 3 {
        kprintln!("[kernel] {} no programa (RIP={:#x}, CS={:#x}): {}", excecao, quadro.rip, quadro.cs, detalhes);
        escalonador::encerrar(Resultado::Falha { excecao, rip: quadro.rip });
    }
}
```

and the system call dispatch:

```rust
    r.rax = match r.rax {
        SYS_SAIR => escalonador::encerrar(Resultado::Saiu(r.rdi)),
        SYS_ESCREVER => escrever(r.rdi, r.rsi),
        SYS_TIQUES => timer::tiques(),
        SYS_PID => escalonador::pid_atual() as u64,
        _ => ERRO_SYSCALL,
    };
```

In `programas.rs`, the symbols and the table entry:

```rust
    static programa_contador_inicio: u8;
    static programa_contador_fim: u8;
...
pub static PROGRAMAS: [Programa; 7] = [
        // ...
        Programa { nome: "laco", descricao: "laco infinito (o timer o encerra depois de 2 s de CPU)",
                   inicio: &raw const programa_laco_inicio, fim: &raw const programa_laco_fim },
        Programa { nome: "contador", descricao: "escreve o proprio pid 5 vezes, a cada 500 ms",
                   inicio: &raw const programa_contador_inicio, fim: &raw const programa_contador_fim },
];
```

In `shell.rs`, the command table lines and `ps`:

```rust
    Comando { nome: "rodar", uso: "rodar <programa> [&]", descricao: "roda um programa no anel 3 (& = segundo plano)", funcao: rodar_programa },
    Comando { nome: "ps", uso: "ps", descricao: "lista os processos", funcao: ps },
```

```rust
fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario;
    let Some(programa) = argumentos.first().and_then(|n| usuario::programas::procurar(n)) else {
        kprintln!("uso: rodar <programa> [&]   (veja 'programas')");
        return;
    };
    // ... the if/else from section 9.3 ...
}

/// `ps` command (doc 12): the process table.
fn ps(_: &[&str]) {
    use crate::escalonador::Estado;
    kprintln!("  PID  ESTADO      CPU (tiques)  NOME");
    for (pid, nome, estado, tiques) in escalonador::listar() {
        let estado = match estado {
            Estado::Pronto => "pronto",
            Estado::Rodando => "rodando",
            Estado::Terminado(_) => "terminado",
        };
        kprintln!("  {:>3}  {:<10}  {:>12}  {}", pid, estado, tiques, nome);
    }
}
```

and, in `shell::rodar()`'s loop, the comment on the three lines from section
9.3:

```rust
        // Free the memory of background programs that have ended, and give
        // them the CPU while nobody is typing (doc 12). With no other ready
        // process, sleep until the next interrupt.
```

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `init` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Registers the code that is running (the kernel) as process 0. |
| `criar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Creates a process with a fabricated kernel stack (6 registers, return to `primeira_entrada`, an `iretq` frame) and puts it in the table as `Pronto`. |
| `primeira_entrada` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | (naked) Where a new process's first `trocar` "returns": zeroes the registers and does `iretq` into ring 3. |
| `trocar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | (naked) The context switch: pushes 6 registers, saves RSP, loads the other process's RSP, pops and `ret`s. |
| `ceder` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | With interrupts off, picks the next process and switches to it; returns when this one's turn comes again. |
| `escolher_proximo` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Round robin: finds the next `Pronto`, updates the states, switches TSS.RSP0 and CR3 and returns the two RSPs for `trocar`. |
| `tique` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Called by the timer: counts the CPU time of the program interrupted in ring 3, ends whoever went over the limit and yields. |
| `encerrar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Marks the current process `Terminado`, prints the result if it is a background one and yields never to return. |
| `retirar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Removes a finished process from the table (fixing the current index) and returns the process and its result. |
| `recolher` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | If process `pid` has ended, removes it, frees stack and space and returns the result. |
| `recolher_segundo_plano` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Frees every background process that has already ended. |
| `esperar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Yields the CPU until process `pid` ends and returns the result. |
| `pid_atual` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | The pid of whoever is running (the `pid` syscall). |
| `ha_outros_prontos` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Is any process besides the current one ready to run? |
| `listar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Pid, name, state and CPU ticks of each process (the `ps` command). |
| `Processo::topo_pilha` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Address of the top of the process's kernel stack (the TSS.RSP0 value). |
| `definir_rsp0` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/gdt.rs) | Writes TSS.rsp[0] (through the `UnsafeCell`, with an unaligned write): the stack used on the next interrupt from ring 3. |
| `descrever` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/mod.rs) | Short text describing a program's result ("terminou: codigo de saida 42"...). |
| `iniciar` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/mod.rs) | Prepares the program's address space (code and stack) and hands it to the scheduler; returns the pid. |
| `ps` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/shell.rs) | `ps` command: the process table. |

Automated tests (`#[test_case]`, run with `cargo test`, see doc 09):

| Function | File | What it does |
|---|---|---|
| `iniciar` (in the tests) | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Helper: starts a built-in program by name, not in the background. |
| `o_kernel_e_o_processo_zero` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Test: whoever runs the tests is process 0, "kernel", `Rodando`. |
| `dois_contadores_ao_mesmo_tempo` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Test: two `contador`s together take under 300 ticks (one after the other would be 500). |
| `dois_lacos_dividem_a_cpu` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Test: two `laco`s together take at least 390 wall-clock ticks, because each needs 200 CPU ticks. |
| `processo_terminado_devolve_pilha_e_quadros` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Test: once reaped, the process is gone from the table and frames and heap are back to what they were. |

---

## Difference from the book

The roadmap in the book's section 11.4 ends with "cooperative switching,
then preemptive". Here both came together: `ceder` is the cooperative one
(the shell uses it when it has nothing to do), and the timer calling `ceder`
on every tick is the preemptive one. The per-process kernel stack, the
fabricated stack with `primeira_entrada`, process 0 as the kernel itself,
per-process CPU time and the lock rule are our choices.

## Summary

- A **process** is a running program: address space, kernel stack and
  saved context. The **scheduler** decides who runs; ours does **round
  robin**, one tick (10 ms) each.
- **Each process has its own kernel stack**, because the interrupt frame of
  whoever was switched out must stay saved. On every switch, TSS.RSP0 gets
  the top of the next process's stack.
- **`trocar`** saves 6 registers on the current stack, stores RSP, loads
  the other one's RSP and pops its registers: the `ret` returns into the
  other process's history.
- A new process starts from a **fabricated stack** that ends in
  `primeira_entrada`, whose `iretq` enters ring 3.
- The **timer** (after the EOI) calls `tique`, which counts CPU time and
  yields: preemption. The 2 s limit became CPU time.
- A process that ends becomes **`Terminado`** and is freed by another one
  (`recolher`), never by itself, since it is running on the stack to be
  freed.
- **Every kernel lock is taken with interrupts off**, so nobody is switched
  out while holding a lock.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **Why isn't a single ring-0 stack enough with two processes?**
   Because the switched-out process's interrupt frame stays on it until it
   comes back. When the second process is interrupted, the CPU would push
   its frame in the same place, on top; the first one would come back with
   the second one's RIP and RSP.

2. **Why does `trocar` save only 6 registers?**
   Because, to its caller, `trocar` is an ordinary function: by the sysv64
   convention, RBX, RBP and R12–R15 must survive the call, and the others
   may be destroyed. What matters of the rest of the context (the ring-3
   frame, the program's registers) is already on the process's kernel stack.

3. **How does a process that never ran "return" from `trocar`?**
   `criar` fabricates its stack: 6 zeros, the address of `primeira_entrada`
   in place of the return address and, above that, an `iretq` frame.
   `trocar`'s `ret` lands in `primeira_entrada`, which does the `iretq` into
   ring 3.

4. **Why does the timer handler send the EOI before calling `tique`?**
   Because `tique` may switch processes, and the handler only finishes when
   this process comes back. Without the EOI, the APIC would deliver no more
   ticks, and the next process would run forever.

5. **Why does `escolher_proximo` release the lock before `trocar`?**
   Because the process about to run also uses the table (on the next tick,
   or in `esperar`). If the lock stayed held by the process that left the
   CPU, the next one would spin forever waiting.

6. **Why doesn't a finished process free its own memory in `encerrar`?**
   Because `encerrar` runs on that process's kernel stack (and with its
   CR3). Freeing both while in use would make the next instruction read
   memory the allocator may have handed to someone else.

7. **What would go wrong if `quadros::alocar` took the lock with interrupts
   on?**
   The timer could switch processes with the lock held. If the next process
   asked for a frame with interrupts off (in a system call, for example),
   it would spin forever: no tick would come to give the CPU back to whoever
   holds the lock.

</details>
