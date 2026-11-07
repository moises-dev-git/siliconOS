# 16: Arguments for programs (argv)

> **You will understand:** how a program receives its **command-line
> arguments** (`argc` and `argv`, Java's `String[] args` or Python's
> `sys.argv`); why, on Linux and here, they arrive **written on the program's
> stack** before its first instruction; the exact format of that initial
> stack (the **System V** one); and why this forces the `_start` entry point
> to be written in assembly.
>
> **Files:** `src/usuario/argumentos.rs` (new: builds the initial stack),
> `src/usuario/mod.rs` (`iniciar_elf` takes the argv), `src/usuario/elf.rs`
> (new error and tests), `src/shell.rs` (`rodar` splits off the arguments),
> `programas/src/lib.rs` (naked `_start`, `sos::argumento`,
> `sos::argumentos`), `programas/src/bin/args.rs` (new) and `primos.rs` (the
> limit comes from the argument)
> **Book:** chapter 11, section 11.4 (the book does not pass arguments to
> programs; the format is System V's, the one Linux uses)
> **Try it:** `./run.sh`, then `rodar args um dois tres`, `rodar primos 5000`
> and `rodar primos 300 &`; `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 13 (ELF programs, the `sos` library, `principal!`), doc 14
> (the syscall calling convention)
> **New Rust here:** `impl Iterator` as a return type, patterns that compare
> text inside a slice, `str::from_utf8`, `$crate` and `sym` in a macro
> ([§46](rust-basico.md#46-impl-iterator-patterns-in-slices-and-text-from-bytes))
>
> **Code for this chapter:** [state at the end of doc 16](https://github.com/moises-dev-git/siliconOS/tree/doc-16) · [what changed since doc 15](https://github.com/moises-dev-git/siliconOS/compare/doc-15...doc-16) · map in [Changes to the files](#changes-to-the-files)

Names in the code are in Portuguese: `argumento(s)` = argument(s), `montar` =
build, `pilha inicial` = initial stack, `topo` = top, `textos` = strings,
`palavras` = (8-byte) words, `quantos_argumentos` = how many arguments,
`rodar` = run, `primos` = primes, `limite` = limit.

---

## 1. The problem: every program always does the same thing

Up to doc 15, `primos` counts the primes below 10,000. Always. To count
below 5,000 you had to change the source code, rebuild the kernel (the
program travels inside it, doc 13) and reboot the machine.

On any real system you would type:

```
$ primos 5000
```

and the program would read the `5000`. In Java it arrives in
`main(String[] args)`; in Python, in `sys.argv`; in C, in
`main(int argc, char **argv)`; in Rust with `std`, in `std::env::args()`. But
none of these languages **invents** the arguments: the operating system
hands them over when it creates the process. The language's library only
finds them and wraps them in a nice type.

So, two questions, one for each side:

1. **Kernel:** where should it leave the strings for the program to find?
2. **Program:** how does it find them, if `main` is called by a `_start`
   that does not even know they exist?

## 2. The idea: write the arguments on the program's stack

The kernel has a problem: the strings are in **its** memory (the line the
shell built, on the kernel heap). The program cannot see that memory (doc
11: each program has its own space, and the kernel shows up without the
USUARIO bit). The strings must be **copied** into the program's space.

Where to? The kernel already creates a region of the program that is
writable and that the program uses from its very first instruction: the
**stack**. The solution of Unix systems since the 70s, standardized by the
**System V ABI** (the contract Linux, FreeBSD and others follow on x86-64),
is this:

> Before the first instruction, the kernel writes the argument strings at
> the top of the stack and, just below them, a table of pointers to them.
> RSP points to the start of the table.

The format, from the lowest address (where RSP points) to the highest:

| Where | What | Size |
|---|---|---|
| RSP | `argc`: how many arguments, counting the name | 8 bytes |
| RSP + 8 | `argv[0]`, `argv[1]`, ..., `argv[argc-1]`: pointers to the strings | 8 bytes each |
| | `0`: end of `argv` | 8 bytes |
| | `envp[0]`, `envp[1]`, ...: pointers to `NAME=value` | 8 bytes each |
| | `0`: end of `envp` | 8 bytes |
| | (on Linux) the *auxiliary vector*, (type, value) pairs | 16 bytes each |
| | (padding for alignment) | |
| top | the strings, each terminated by the byte 0 | |

Three details worth noting:

- **`argv[0]` is the program's name.** The caller decides what goes there;
  by convention, it is the name the program was called by. That is why a
  program with no arguments has `argc = 1`, not 0.
- **The strings end with the byte 0** (the "C string", *NUL-terminated*).
  No length is stored: the reader walks until it finds the 0. Consequence: an
  argument cannot contain the byte 0.
- **RSP arrives aligned to 16.** The ABI requires that at process start.
  This is different from what an ordinary function expects on entry (there,
  RSP is "a multiple of 16 minus 8", because `call` just pushed the return
  address). Section 5 shows what this changes in `_start`.

Linux does exactly this. You can check it on the development machine itself,
with a 4-line C program that prints where `argv` and `envp` are (run like
this, on the Linux host, not on SiliconOS):

```
$ ./pilha um dois
argc=3 ... argv=0x7ffd194dc608 argv[0]=0x7ffd194dcd5b "./pilha" envp=0x7ffd194dc628 (argv+argc+1=0x7ffd194dc628)
```

`envp` is exactly at `argv + argc + 1`: after the 3 pointers and the 0. And
`argv[0]` is more than 1 KiB higher up, at the top, with the other strings.
The *auxiliary vector* is there too (Linux shows it with `LD_SHOW_AUXV=1
/bin/true`: `AT_PAGESZ: 4096`, `AT_PHDR`, ...); it is how the dynamic loader
finds out, for example, where the program headers are.

SiliconOS follows the same format, with an empty environment (`envp` only
has its closing 0) and no *auxiliary vector*: nothing here needs it.

### 2.1 Why not in registers?

It would be simpler for the kernel to put `argc` in RDI and a pointer to
`argv` in RSI, as in a function call. But the strings would still have to be
copied to **some place** in the program's space, and the stack is the only
writable place every program has. Once they are there, leaving the pointer
table on the stack too costs nothing, and the format does not depend on
registers (the same layout works on any CPU; only the name of the stack
register changes). Following System V also has a practical advantage: it is
what real compilers and libraries expect. A program made for it would only
need the right syscalls, not a different entry format.

## 3. The kernel: building the initial stack

The kernel's work lives in a new module, `src/usuario/argumentos.rs`. The
main function takes the arguments and the address of the top of the stack
**in the program's space**, and returns the bytes ready to be copied:

```rust
/// The top of the stack, ready to be copied: `bytes` will live at
/// [rsp, topo) in the program's space.
#[derive(Debug)]
pub struct PilhaInicial {
    pub bytes: Vec<u8>,
    pub rsp: u64,
}
```

Why build it in a kernel `Vec` instead of writing straight onto the stack?
Because the kernel writes into the program's memory **through the physical
address** (the doc 06 offset: `fisico_para_virtual`), since the program's
space is not active while it is being created. The `argv` pointers, however,
must be **program** addresses (near `0x7FFF_FFFF_FFFF`). Building everything
in a vector, computing the pointers from `topo`, separates the two things:
the arithmetic is done once, with program addresses, and the copy is just a
`copy_nonoverlapping`. And it can be tested without creating any process
(section 7.3).

### 3.1 The checks

```rust
/// More than this, `montar` refuses (Linux accepts many more; here the
/// shell cannot even type that many).
pub const MAX_ARGUMENTOS: usize = 32;

/// Why the arguments could not be delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroArgumentos {
    /// No arguments: at least argv[0], the program name, is missing.
    Vazio,
    /// More than MAX_ARGUMENTOS.
    Demais,
    /// Strings and pointers together do not fit in the last stack page.
    GrandesDemais,
    /// An argument contains the byte 0, which marks the end of each string.
    TemZero,
}
```

(`Vazio` = empty, `Demais` = too many, `GrandesDemais` = too big, `TemZero`
= has a zero.) `GrandesDemais` exists because `montar` decides that
everything fits in a single page (the highest one of the stack). It is a
simplification: Linux reserves much more (a quarter of the stack limit: with
the default 8 MiB, `getconf ARG_MAX` answers 2097152, 2 MiB). With a shell
line limited to 70 characters (doc 09), one page is plenty. `TemZero`
exists because of the format: a 0 in the middle of a string would cut it in
two.

### 3.2 The arithmetic

```rust
pub fn montar(argumentos: &[&str], topo: u64) -> Result<PilhaInicial, ErroArgumentos> {
    if argumentos.is_empty() {
        return Err(ErroArgumentos::Vazio);
    }
    if argumentos.len() > MAX_ARGUMENTOS {
        return Err(ErroArgumentos::Demais);
    }
    if argumentos.iter().any(|a| a.as_bytes().contains(&0)) {
        return Err(ErroArgumentos::TemZero);
    }

    // 1. Where the string area starts (each string with its closing 0).
    let tam_textos: u64 = argumentos.iter().map(|a| a.len() as u64 + 1).sum();
    let inicio_textos = topo - tam_textos;

    // 2. The 8-byte words: argc, one pointer per argument, the 0 that
    //    closes argv and the 0 that closes envp. RSP at their start, aligned to 16.
    let palavras = 1 + argumentos.len() as u64 + 1 + 1;
    let rsp = (inicio_textos - palavras * 8) & !0xF;

    let tamanho = topo - rsp;
    if tamanho > TAMANHO_QUADRO {
        return Err(ErroArgumentos::GrandesDemais);
    }
```

The arithmetic goes from top to bottom, the way the stack grows:

1. The strings sit right at the top. With `["args", "um", "dois", "tres"]`:
   5 + 3 + 5 + 5 = 18 bytes, so `inicio_textos = topo - 18`.
2. Below them, the words: argc (1), one pointer per argument (4), the `argv`
   0 (1) and the `envp` 0 (1) = 7 words, 56 bytes.
3. `& !0xF` clears the lowest 4 bits: it rounds **down** to a multiple of
   16. Down, not up, because up would overwrite the strings. The padding
   (0 to 15 bytes) sits between the words and the strings.

With `topo = 0x8000_0000_0000` (doc 13's `PILHA_ELF_TOPO`), the example
gives `inicio_textos = 0x7FFF_FFFF_FFEE`, and `0x7FFF_FFFF_FFEE - 56 =
0x7FFF_FFFF_FFB6`, which rounds to `0x7FFF_FFFF_FFB0`. These are the
addresses the `args` program prints in section 7.1.

### 3.3 Filling it in

```rust
    // 3. Fill in. `pos(endereco)` = index in the vector of the byte that will
    //    live at that program address.
    let mut bytes = vec![0u8; tamanho as usize];
    let pos = |endereco: u64| (endereco - rsp) as usize;
    let escrever_u64 = |bytes: &mut Vec<u8>, endereco: u64, valor: u64| {
        bytes[pos(endereco)..pos(endereco) + 8].copy_from_slice(&valor.to_le_bytes());
    };

    escrever_u64(&mut bytes, rsp, argumentos.len() as u64); // argc
    let mut texto = inicio_textos;
    for (i, a) in argumentos.iter().enumerate() {
        escrever_u64(&mut bytes, rsp + 8 + 8 * i as u64, texto); // argv[i]
        bytes[pos(texto)..pos(texto) + a.len()].copy_from_slice(a.as_bytes());
        texto += a.len() as u64 + 1; // the closing 0 is already there (zeroed vector)
    }
    // argv[argc] = 0 and envp[0] = 0: also already zeroed.
    Ok(PilhaInicial { bytes, rsp })
}
```

The vector starts zeroed (`vec![0u8; ...]`), so four things are already
right without a single line of code: the 0 at the end of each string, the 0
that closes `argv`, the 0 that closes `envp`, and the padding. The code only
writes what is not zero: `argc`, the pointers and the bytes of the strings.

The two closures are only there to keep the arithmetic readable: `pos`
translates "address in the program" into "index in the vector", and
`escrever_u64` (write a u64) stores an 8-byte number in *little-endian* (the
least significant byte first, the way the x86 CPU reads it; see the
glossary). `escrever_u64` receives the vector as an argument instead of
capturing it, because the loop also writes to it directly
(`bytes[...].copy_from_slice`): a closure holding a `&mut bytes` would
forbid any other use of the vector while it existed (the *borrow checker*,
[§8](rust-basico.md#8-references-and-the-borrow-checker-very-short-version)).

The result, for `args um dois tres`:

```
  0x7FFF_FFFF_FFFF ┐
  0x7FFF_FFFF_FFFB │ "tres\0"
  0x7FFF_FFFF_FFF6 │ "dois\0"
  0x7FFF_FFFF_FFF3 │ "um\0"
  0x7FFF_FFFF_FFEE ┘ "args\0"          ◄─ inicio_textos
  0x7FFF_FFFF_FFE8   (6 bytes of padding)
  0x7FFF_FFFF_FFE0   0                 envp[0]
  0x7FFF_FFFF_FFD8   0                 argv[4]
  0x7FFF_FFFF_FFD0   0x7FFF_FFFF_FFFB  argv[3] ─► "tres"
  0x7FFF_FFFF_FFC8   0x7FFF_FFFF_FFF6  argv[2] ─► "dois"
  0x7FFF_FFFF_FFC0   0x7FFF_FFFF_FFF3  argv[1] ─► "um"
  0x7FFF_FFFF_FFB8   0x7FFF_FFFF_FFEE  argv[0] ─► "args"
  0x7FFF_FFFF_FFB0   4                 argc     ◄─ initial RSP
```

### 3.4 Copying onto the program's stack

`iniciar_elf` (doc 13) gets a new parameter, the argv:

```rust
pub fn iniciar_elf(nome: &'static str, dados: &[u8], argumentos: &[&str],
                   segundo_plano: bool) -> Result<Pid, elf::ErroElf> {
    // Arguments first: if they do not fit, loading is not even worth it.
    let pilha = argumentos::montar(argumentos, PILHA_ELF_TOPO).map_err(elf::ErroElf::Argumentos)?;
    let mut espaco = EspacoEnderecos::novo().ok_or(elf::ErroElf::SemMemoria)?;
```

The argv includes the name in `argv[0]`, as in Linux's `execve` syscall:
`["primos", "5000"]`. The caller decides what goes in `argv[0]`; the shell
puts the name you typed.

`montar` runs **before** any allocation: if the arguments are refused,
there is no space and no frames to give back. The error becomes a new
`ErroElf` variant:

```rust
    /// The arguments could not be delivered to the program (doc 16). It is
    /// not a defect of the file, but it is found out at load time.
    Argumentos(super::argumentos::ErroArgumentos),
```

`map_err(elf::ErroElf::Argumentos)` uses the variant itself as a function:
an enum variant with one field is also a function that takes the field and
returns the enum (`ErroElf::Argumentos(e)`).

Then, in the loop that creates the 4 stack pages, the first one (`i == 1`,
the highest) receives the bytes, at its end:

```rust
    for i in 1..=PILHA_ELF_PAGINAS {
        let q = quadros::alocar().ok_or(elf::ErroElf::SemMemoria)?;
        let pagina = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
        unsafe { core::ptr::write_bytes(pagina, 0, 4096) };
        if i == 1 {
            // The highest page receives argc, argv and the strings, at its end
            // (`montar` guarantees they fit). The kernel writes through the
            // physical address (offset, doc 06): the program's space is not active.
            let inicio = TAMANHO_QUADRO as usize - pilha.bytes.len();
            unsafe {
                core::ptr::copy_nonoverlapping(pilha.bytes.as_ptr(), pagina.add(inicio), pilha.bytes.len());
            }
        }
        espaco.mapear(PILHA_ELF_TOPO - i * TAMANHO_QUADRO, q,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO)
            .map_err(|_| elf::ErroElf::SemMemoria)?;
    }
```

And the process's initial RSP becomes what `montar` computed:

```rust
    // RSP = the start of argc, aligned to 16, as System V requires at program
    // entry. Up to doc 15 it was topo - 8, to imitate a `call`; since doc 16
    // the sos library's `_start` is hand-written and fixes the alignment
    // itself before calling Rust.
    Ok(escalonador::criar(nome, espaco, entrada, pilha.rsp, segundo_plano))
```

That `topo - 8` from doc 13 deserves the next section.

## 4. The shell: separating the argv from the `&`

`rodar` already had an optional argument, the background `&` (doc 12). Now
the line is `rodar <program> [arguments...] [&]`, and the `&` only counts if
it is the **last** word:

```rust
fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario;
    // A trailing `&` asks for background (doc 12); the rest is the program's
    // argv (doc 16), with the name in argv[0].
    let (argv, segundo_plano) = match argumentos.split_last() {
        Some((&"&", resto)) => (resto, true),
        _ => (argumentos, false),
    };
    let Some(&nome) = argv.first() else {
        kprintln!("uso: rodar <programa> [argumentos...] [&]   (veja 'programas' e 'ls')");
        return;
    };
```

`split_last` is the sibling of `split_first` (doc 09): it returns the last
element and the rest. The pattern `Some((&"&", resto))` only matches if the
last one is the string `"&"` (the `&` before the quotes is there because the
element comes by reference; see
[§46](rust-basico.md#46-impl-iterator-patterns-in-slices-and-text-from-bytes)).
For `rodar primos 300 &`, `argumentos` is `["primos", "300", "&"]` and the
argv becomes `["primos", "300"]`: the shell hands the program exactly what
it needs, already with the name in front. A real shell does the same: `&`
(and `>`, `|`...) are instructions **for the shell**, and the program never
sees them.

The built-in programs from doc 10 (in assembly) have a single stack page and
do not know how to read arguments. Instead of silently accepting and
ignoring them, the shell says so:

```rust
    let pid = if let Some(programa) = usuario::programas::procurar(nome) {
        if argv.len() > 1 {
            // Written in assembly, with a single stack page: they cannot
            // read arguments.
            kprintln!("'{}' e um programa embutido e nao recebe argumentos (os do disco recebem)", nome);
            return;
        }
        usuario::iniciar(programa, segundo_plano)
    } else if let Some(arquivo) = crate::ramdisk::abrir(nome) {
        match usuario::iniciar_elf(arquivo.nome, arquivo.dados, argv, segundo_plano) {
```

(The message says "'ola' is a built-in program and does not take arguments
(the ones on the disk do)".) The `ajuda` (help) text also changed so it
fits in an 80-column line: `rodar <prog> [args]   roda no anel 3; args: so os
do disco; & no fim = 2o plano` ("runs in ring 3; args: disk programs only;
trailing & = background").

## 5. The program: a `_start` that knows where RSP is

### 5.1 Why an ordinary Rust function does not work

Up to doc 15, the `principal!` macro generated this:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    $crate::sair($main())
}
```

An ordinary Rust function. The problem: the arguments are at `[RSP]` **at
the instant `_start` begins**, and an ordinary function has no way of
knowing that value. The compiler generates a **prologue** before the body
(pushes registers, reserves space with `sub rsp, N`), and the size of the
prologue changes with the code and the optimizations. By the time the body
runs, RSP has already moved, and there is no way to ask "what was it
before".

It is the same situation as the interrupt handlers and the context switch
(docs 10, 12 and 14), with the same solution: a **naked** function, assembly
only, with no prologue
([§35](rust-basico.md#35-naked-functions-and-naked_asm)).

### 5.2 The new `_start`

```rust
#[macro_export]
macro_rules! principal {
    ($main:path) => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn _start() -> ! {
            core::arch::naked_asm!(
                "mov rdi, rsp",           // 1st argument: where argc is
                "lea rsi, [rip + {main}]", // 2nd argument: the main function
                "and rsp, -16",           // align to 16 (the kernel already does; safety net)
                "call {iniciar}",         // call pushes 8 bytes: Rust expects that
                "ud2",                    // __iniciar does not return
                main = sym $main,
                iniciar = sym $crate::__iniciar,
            )
        }
    };
}
```

Line by line:

- **`mov rdi, rsp`**: the current RSP, untouched, points to `argc`. By the
  System V convention (doc 14,
  [§37](rust-basico.md#37-calling-conventions-extern-sysv64)), RDI is a
  function's 1st argument.
- **`lea rsi, [rip + {main}]`**: the address of the program's `main`
  function, in the 2nd argument (RSI). `lea` ("load effective address")
  computes the address without reading memory; `[rip + ...]` is relative to
  the current instruction, so it works with doc 13's `code-model=large`
  (which forbids 32-bit absolute addresses, not relative ones). `{main}`
  becomes the function's symbol thanks to `main = sym $main`.
- **`and rsp, -16`**: `-16` in binary is `...1111_0000`; the `and` clears
  the lowest 4 bits, rounding RSP down to a multiple of 16. The kernel
  already delivers it aligned (section 3.2), but `_start` should not depend
  on that.
- **`call {iniciar}`**: pushes 8 bytes (the return address) and jumps. So,
  on entry to `__iniciar`, RSP is "a multiple of 16 minus 8", **exactly**
  what the compiler assumes in every function. This is the reason for the
  `topo - 8` in doc 13: there, without assembly, the kernel imitated the
  `call` that never happened. Now the `call` really happens, and the kernel
  can follow System V to the letter (RSP a multiple of 16 at process entry).
- **`ud2`**: an intentionally invalid instruction. `__iniciar` never returns
  (`-> !`), but, if it did, the program would die with #UD (doc 04) instead
  of executing whatever came next in memory.

The two `sym`s use paths the macro receives or knows: `$main` is the
function the program passed to `principal!(main)`, and `$crate` is "the
crate where the macro was defined" (`sos`), no matter which program uses it
([§46](rust-basico.md#46-impl-iterator-patterns-in-slices-and-text-from-bytes)).

### 5.3 Storing argc and argv

```rust
/// argc and argv, stored by `__iniciar` before `main`. Atomic only to avoid
/// `static mut`: the program has a single thread.
static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());

/// Called by the `_start` from `principal!`. Not for programs to use.
#[doc(hidden)]
pub unsafe extern "C" fn __iniciar(pilha: *const u64, main: fn() -> u64) -> ! {
    // pilha[0] = argc; pilha[1..] = argv[0], argv[1], ..., 0.
    unsafe {
        ARGC.store(*pilha as usize, Ordering::Relaxed);
        ARGV.store(pilha.add(1) as *mut *const u8, Ordering::Relaxed);
    }
    sair(main())
}
```

`__iniciar` (start) is ordinary Rust, and that is why it can receive the old
RSP as an argument: the assembly already put it in RDI. It stores `argc`
(the number at `pilha[0]`) and the address of `argv[0]` (`pilha + 1`, that
is, 8 bytes higher: `add` on a pointer counts in elements, not bytes) and
calls `main`, as the old `_start` did.

The two `static`s are atomic
([§25](rust-basico.md#25-atomics-atomicu64-atomicbool)) not because of
concurrency (the program has a single thread), but because Rust does not let
you change an ordinary `static` without `unsafe` everywhere it is read
(`static mut`). `Relaxed` is enough: there is no other thread for whom the
order matters ([§28](rust-basico.md#28-orderingacquire-and-release)). The
`AtomicPtr<*const u8>` holds a "pointer to pointers to bytes", C's
`char **argv`.

The type of `main` is `fn() -> u64`
([§31](rust-basico.md#31-function-pointers-fn)): a function pointer that
takes nothing and returns the exit code. That way, the programs that already
existed (`oi`, `dados`, `soneca`...) do not change a single line; the
arguments are available to whoever wants to ask.

### 5.4 Asking for the arguments

```rust
/// How many arguments the program received, counting the name (argv[0]).
pub fn quantos_argumentos() -> usize {
    ARGC.load(Ordering::Relaxed)
}

/// Argument `i` (0 = the program's name), or `None` if it does not exist.
/// Like Python's `sys.argv[i]`, but without blowing up: it returns `None`.
pub fn argumento(i: usize) -> Option<&'static str> {
    if i >= quantos_argumentos() {
        return None;
    }
    // The kernel guarantees: argv[i] points to a 0-terminated string that
    // stays on the stack until the program ends (hence 'static).
    unsafe {
        let inicio = *ARGV.load(Ordering::Relaxed).add(i);
        let mut tamanho = 0;
        while *inicio.add(tamanho) != 0 {
            tamanho += 1;
        }
        let bytes = core::slice::from_raw_parts(inicio, tamanho);
        Some(core::str::from_utf8(bytes).unwrap_or("?"))
    }
}

/// All the arguments, in order, like Rust's `std::env::args()`.
pub fn argumentos() -> impl Iterator<Item = &'static str> {
    (0..quantos_argumentos()).filter_map(argumento)
}
```

`argumento(i)` does what C does with `strlen`: it walks byte by byte until
it finds the 0, then turns the bytes into a `&str`. Two Rust points:

- **`'static`**: the string lives on the initial stack, which is never
  popped (it sits **above** everything the program pushes later). It exists
  until the program ends, and `'static` tells the compiler exactly that.
- **`str::from_utf8`**: a `&str` in Rust must be valid UTF-8. The bytes came
  from outside (from the kernel); `from_utf8` checks them and returns `Err`
  if they are not. Here, `?` replaces an invalid string (the shell only sends
  ASCII, which is valid UTF-8, so it does not happen today).

`argumentos()` returns `impl Iterator<Item = &'static str>`: "some type that
is an iterator of strings", without saying which
([§46](rust-basico.md#46-impl-iterator-patterns-in-slices-and-text-from-bytes)).
With it, the program uses the arguments with `for`, `enumerate`, `skip(1)`
etc., as it would with `std::env::args()`.

## 6. Two programs

### 6.1 `args`: showing what arrived

```rust
//! Shows the arguments it received (docs/16-argumentos.md) and exits with
//! how many there were, counting the name: `rodar args a bb ccc` exits with 4.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    let local = 0u64; // a variable on the stack, to compare addresses
    println!("argc = {} (uma variavel local esta em {:#x})", sos::quantos_argumentos(), &local as *const u64 as u64);
    for (i, a) in sos::argumentos().enumerate() {
        // The address shows where the kernel left the string: at the top of the stack.
        println!("argv[{}] = \"{}\" (em {:#x})", i, a, a.as_ptr() as u64);
    }
    sos::quantos_argumentos() as u64
}
```

(Output: "a local variable is at ..." and "(at ...)".) The exit code is
`argc`, so the automated tests can check it without reading the screen. The
addresses let you see the drawing from section 3.3 on the screen.

### 6.2 `primos`: the limit comes from the argument

```rust
fn main() -> u64 {
    let limite = match sos::argumento(1) {
        None => 10_000,
        Some(texto) => match texto.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                println!("uso: primos [limite]   ('{}' nao e um numero)", texto);
                return u64::MAX; // the shell shows -1
            }
        },
    };
    let inicio = sos::tiques();
    let mut quantos = 0;
    print!("primeiros primos:");
    for n in 0..limite {
```

With no argument, it counts below 10,000, as before (the doc 13 test that
expects 1229 still holds). `texto.parse::<u64>()` is Java's
`Long.parseLong`, but without an exception: it returns `Result`
([§6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)). A
string that is not a number ends with exit code `u64::MAX`, which the shell
shows as `-1` (it prints the code as an `i64`, doc 10). The message reads
"usage: primos [limit] ('xyz' is not a number)".

## 7. How it was tested

### 7.1 From the shell

Real serial output, with QEMU receiving the keys through the monitor:

```
silicon> rodar args um dois tres
argc = 4 (uma variavel local esta em 0x7fffffffff60)
argv[0] = "args" (em 0x7fffffffffee)
argv[1] = "um" (em 0x7ffffffffff3)
argv[2] = "dois" (em 0x7ffffffffff6)
argv[3] = "tres" (em 0x7ffffffffffb)
'args' terminou: codigo de saida 4
silicon> rodar args
argc = 1 (uma variavel local esta em 0x7fffffffff80)
argv[0] = "args" (em 0x7ffffffffffb)
'args' terminou: codigo de saida 1
```

The string addresses are the ones from section 3.3. `main`'s local variable
is **below** them (`...ff60` < `...ffb0`, the initial RSP): the program
pushes everything it does afterwards underneath the argument area, which
stays untouched. With a single argument the area is smaller and the local
variable sits higher (`...ff80`).

`primos`, with and without a valid number, and two in the background:

```
silicon> rodar primos 5000
primeiros primos: 2 3 5 7 11 13 17 19 23 29
669 primos abaixo de 5000 (1 tiques)
'primos' terminou: codigo de saida 669
silicon> rodar primos xyz
uso: primos [limite]   ('xyz' nao e um numero)
'primos' terminou: codigo de saida -1
silicon> rodar ola extra
'ola' e um programa embutido e nao recebe argumentos (os do disco recebem)
silicon> rodar primos 300 &
[7] 'primos' em segundo plano
silicon> primeiros primos: 2 3 5 7 11 13 17 19 23 29
62 primos abaixo de 300 (2 tiques)

[7] 'primos' terminou: codigo de saida 62
```

The `&` did not reach the program (it got `["primos", "300"]`) and still
worked for the shell. Programs that do not read arguments receive them too
and simply ignore them: `rodar oi a b` runs `oi` normally.

### 7.2 Looking at the program's memory

The addresses printed by `args` show where the strings are, but not the
pointer table. To see the whole stack, the QEMU monitor was used (doc 09),
which reads memory **through the current CR3**. The trick is to run a
program that keeps the CPU busy for a while, so the active CR3 is its own:
`insone` (doc 15) busy-waits for about 2 s. And since this chapter **every**
ELF receives an argv, including `insone`, which does not read it. With
`rodar insone a bb` running, the monitor commands `x /6gx 0x7fffffffffc0`
and `x /64xb 0x7fffffffffc0` showed:

```
00007fffffffffc0: 0x0000000000000003 0x00007ffffffffff4
00007fffffffffd0: 0x00007ffffffffffb 0x00007ffffffffffd
00007fffffffffe0: 0x0000000000000000 0x0000000000000000
...
00007ffffffffff0: 0x00 0x00 0x00 0x00 0x69 0x6e 0x73 0x6f
00007ffffffffff8: 0x6e 0x65 0x00 0x61 0x00 0x62 0x62 0x00
```

Reading it: `argc = 3`; three pointers (`...fff4`, `...fffb`, `...fffd`);
the two zeros (end of `argv` and of `envp`); 4 bytes of padding (`...fff0`
to `...fff3`); and the strings, in ASCII: `69 6e 73 6f 6e 65` = "insone",
`00`, `61` = "a", `00`, `62 62` = "bb", `00`. The arithmetic in section 3.2
predicts exactly this: strings of 7 + 2 + 3 = 12 bytes (`topo - 12 =
...fff4`), 6 words (48 bytes), `...fff4 - 48 = ...ffc4`, rounded to
`...ffc0`.

### 7.3 `cargo test`

Four new tests (57 in total, all passing):

```
siliconos::usuario::argumentos::testes::argumentos_ruins_recusados ... [ok]
siliconos::usuario::argumentos::testes::pilha_inicial_no_formato_system_v ... [ok]
siliconos::usuario::elf::testes::argumentos_chegam_ao_programa ... [ok]
siliconos::usuario::elf::testes::argumentos_demais_recusados_sem_vazar ... [ok]
Todos os 57 testes passaram.
```

The first two test `montar` on its own, without creating a process:

```rust
    #[test_case]
    fn pilha_inicial_no_formato_system_v() {
        let topo = 0x0000_8000_0000_0000;
        let p = montar(&["primos", "5000"], topo).unwrap();
        assert_eq!(p.rsp % 16, 0);
        assert_eq!(p.rsp + p.bytes.len() as u64, topo);
        assert_eq!(ler_u64(&p, p.rsp), 2);                 // argc
        assert_eq!(ler_u64(&p, p.rsp + 24), 0);            // argv[2] = 0
        assert_eq!(ler_u64(&p, p.rsp + 32), 0);            // envp[0] = 0
        // The strings sit right at the top: "primos\0" (7) + "5000\0" (5).
        assert_eq!(ler_u64(&p, p.rsp + 8), topo - 12);
        assert_eq!(ler_u64(&p, p.rsp + 16), topo - 5);
        assert_eq!(&p.bytes[p.bytes.len() - 12..], b"primos\x005000\x00");
    }
```

The second checks each error and the **exact limit** of the page:

```rust
        // The largest that fits: 4096 bytes in total. 5 words (argc, 2
        // pointers, the 2 zeros: 40 bytes) + "p\0" (2) + the big string with
        // its 0 (4054) = 4096, already aligned. One byte more and it does not fit.
        let cabe: String = core::iter::repeat_n('x', 4053).collect();
        assert_eq!(montar(&["p", &cabe], topo).unwrap().bytes.len(), 4096);
        let nao_cabe: String = core::iter::repeat_n('x', 4054).collect();
        assert_eq!(montar(&["p", &nao_cabe], topo).unwrap_err(), ErroArgumentos::GrandesDemais);
```

An honest story: the first version of this test counted **4** words
(forgetting one of the zeros) and failed in QEMU with `GrandesDemais`. It
was the test that found the arithmetic mistake, not the code: that is why
you test the boundary (the largest that fits and the smallest that does
not), and not only a comfortable case in the middle.

The other two run real programs:

```rust
    #[test_case]
    fn argumentos_chegam_ao_programa() {
        assert_eq!(rodar_com(&["args"]), Resultado::Saiu(1));
        assert_eq!(rodar_com(&["args", "a", "bb", "ccc"]), Resultado::Saiu(4));
        assert_eq!(rodar_com(&["primos", "100"]), Resultado::Saiu(25));
        assert_eq!(rodar_com(&["primos", "abc"]), Resultado::Saiu(u64::MAX));
    }

    #[test_case]
    fn argumentos_demais_recusados_sem_vazar() {
        use crate::usuario::argumentos::{ErroArgumentos, MAX_ARGUMENTOS};
        let livres = quadros::livres();
        let arquivo = ramdisk::abrir("args").unwrap();
        let r = crate::usuario::iniciar_elf(arquivo.nome, arquivo.dados, &["args"; MAX_ARGUMENTOS + 1], false);
        assert_eq!(r, Err(ErroElf::Argumentos(ErroArgumentos::Demais)));
        assert_eq!(quadros::livres(), livres);
    }
```

The last one also checks that the refusal does not leak frames (none is
even allocated, because `montar` runs first). `&["args"; MAX_ARGUMENTOS + 1]`
is an array with 33 copies of the same string.

### 7.4 Regression

The 25 builds (the normal one and the 24 test and diagnostic features)
behave as before: each triggered exception shows up with the same message,
the memory, heap and timer tests print "OK", and `teste-deadlock` hangs on
purpose.

---

## What is still missing

- **Environment variables.** `envp` exists in the format, but empty. An
  environment (`PATH=/bin`, `LANG=pt`) would be the same arithmetic with more
  strings; what is missing is someone to keep them (the shell) and a use for
  them.
- **One program starting another.** Here only the shell (inside the
  kernel) creates processes, and the arguments come from kernel memory,
  which can be trusted. A syscall like Linux's `execve` would receive the
  argv **from ring 3**: every pointer, and every string up to its 0, would
  have to pass through `faixa_do_programa` (doc 10) before being copied, and
  the total size would have to be limited before the copy, not after.
- **Quotes.** The shell splits the line by spaces (`split_whitespace`):
  `rodar args "um dois"` delivers two arguments, `"um` and `dois"`. A real
  shell handles quotes and `\` before building the argv.
- **The *auxiliary vector*.** It would only be needed with a dynamic loader
  or a real libc, which look for it right after `envp`.

---

## Changes to the files

What this chapter adds to the doc 15 code (tag `doc-15`). Each file's link
goes to its state at the end of this chapter (tag `doc-16`); every changed
line is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-15...doc-16).

| File | What changes | Where |
|---|---|---|
| [`src/usuario/argumentos.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | **new** (section 3): `MAX_ARGUMENTOS`, `ErroArgumentos`, `PilhaInicial`, `montar`; 2 tests | — |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/mod.rs) | `pub mod argumentos`; `iniciar_elf` takes the argv, builds and copies the initial stack, RSP = `pilha.rsp` (section 3.4) | top; `iniciar_elf` |
| [`src/usuario/elf.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | `ErroElf::Argumentos`; 2 tests and the `rodar_com` helper; `rodar` passes `&[nome]` | end of `ErroElf`; tests |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/shell.rs) | `rodar_programa` splits off the `&` and passes the argv; refuses arguments for built-ins; `ajuda` text | `COMANDOS`; `rodar_programa` |
| [`src/escalonador.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/escalonador.rs), [`src/usuario/syscall.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/syscall.rs) | tests: the `iniciar_elf` calls pass the argv | tests |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/ramdisk.rs) | test: 7 names (`args` is the first) | tests |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | `principal!` generates a naked `_start`; `ARGC`, `ARGV`, `__iniciar`, `quantos_argumentos`, `argumento`, `argumentos` (section 5) | `use`; where `principal!` was |
| [`programas/src/bin/args.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/bin/args.rs) | **new** (section 6.1) | — |
| [`programas/src/bin/primos.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/bin/primos.rs) | the limit comes from `argumento(1)` (section 6.2) | `main` |

### The snippets the text did not show

The new `use` in `programas/src/lib.rs`:

```rust
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
```

The `use`s in `src/usuario/argumentos.rs`:

```rust
use alloc::vec;
use alloc::vec::Vec;

use crate::memoria::TAMANHO_QUADRO;
```

The test helper in `argumentos.rs`:

```rust
    fn ler_u64(p: &PilhaInicial, endereco: u64) -> u64 {
        let i = (endereco - p.rsp) as usize;
        u64::from_le_bytes(p.bytes[i..i + 8].try_into().unwrap())
    }
```

The beginning of `argumentos_ruins_recusados`:

```rust
    #[test_case]
    fn argumentos_ruins_recusados() {
        let topo = 0x0000_8000_0000_0000;
        assert_eq!(montar(&[], topo).unwrap_err(), ErroArgumentos::Vazio);
        assert_eq!(montar(&["a"; MAX_ARGUMENTOS + 1], topo).unwrap_err(), ErroArgumentos::Demais);
        assert_eq!(montar(&["a\0b"], topo).unwrap_err(), ErroArgumentos::TemZero);
        let grande: String = core::iter::repeat_n('x', 4096).collect();
        assert_eq!(montar(&["p", &grande], topo).unwrap_err(), ErroArgumentos::GrandesDemais);
```

The `rodar_com` helper, in `elf.rs`:

```rust
    /// Runs a program from the disk with these arguments (doc 16).
    fn rodar_com(argv: &[&str]) -> Resultado {
        let arquivo = ramdisk::abrir(argv[0]).unwrap();
        escalonador::esperar(crate::usuario::iniciar_elf(arquivo.nome, arquivo.dados, argv, false).unwrap())
    }
```

The other calls to `iniciar_elf` (in the tests) only gained the argv:
`&[nome]` in `elf.rs` and `escalonador.rs` (which now takes `nome:
&'static str`) and `&["medir"]` in `syscall.rs`. The command table line:

```rust
    Comando { nome: "rodar", uso: "rodar <prog> [args]", descricao: "roda no anel 3; args: so os do disco; & no fim = 2o plano", funcao: rodar_programa },
```

The rest of `rodar_programa` (error handling and background) is the one
from doc 13. The disk test:

```rust
        assert_eq!(nomes, ["args", "dados", "insone", "medir", "oi", "primos", "soneca"]);
```

And the end of `primos.rs`, which only swapped the fixed number for
`limite`:

```rust
    for n in 0..limite {
        if primo(n) {
            quantos += 1;
            if quantos <= 10 {
                print!(" {}", n);
            }
        }
    }
    println!();
    println!("{} primos abaixo de {} ({} tiques)", quantos, limite, sos::tiques() - inicio);
    quantos
}
```

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `montar` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Builds in a vector the top of a program's initial stack (argc, argv, zeros, strings) and computes RSP, aligned to 16; refuses empty, too many, too big or 0-containing arguments. |
| `_start` (from `principal!`) | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | (naked) The program's entry point: passes the initial RSP and `main` to `__iniciar`, with the stack aligned. |
| `__iniciar` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | Stores argc and argv read from the initial stack, calls `main` and exits with the returned value. |
| `quantos_argumentos` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | argc: how many arguments, counting the program's name. |
| `argumento` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | Argument `i` as a `&'static str` (walks to the byte 0), or `None` if it does not exist. |
| `argumentos` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | Iterator over all the arguments, like `std::env::args()`. |
| `main` (args) | [programas/src/bin/args.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/bin/args.rs) | Shows argc and each argument with its address; exits with argc. |

Automated tests (`#[test_case]`, run with `cargo test`, see doc 09):

| Function | File | What it does |
|---|---|---|
| `ler_u64` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Helper: reads the 8-byte number that will live at an address of the built stack. |
| `pilha_inicial_no_formato_system_v` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Test: argc, pointers, zeros and strings in the right places, RSP aligned to 16. |
| `argumentos_ruins_recusados` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Test: each `montar` error, and the exact limit of one page (4053 fits, 4054 does not). |
| `rodar_com` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | Helper: runs a program from the disk with an argv and waits for the result. |
| `argumentos_chegam_ao_programa` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | Test: `args` exits with argc; `primos 100` exits with 25; `primos abc` exits with -1. |
| `argumentos_demais_recusados_sem_vazar` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | Test: 33 arguments are refused by `iniciar_elf` without allocating any frame. |

---

## Differences from the book

The book does not pass arguments to programs. The format used here is the
System V ABI for x86-64, the same as Linux's, with two simplifications: the
environment is empty and there is no *auxiliary vector*. The argument area
is limited to the highest page of the stack (4 KiB).

## Summary

- A program's arguments do not belong to the language: the **kernel**
  delivers them when it creates the process, and the library (`std`, libc,
  here `sos`) only finds them.
- In System V (Linux and SiliconOS), the kernel **copies them to the top of
  the program's stack**: the 0-terminated strings up there and, below them,
  `argc`, the `argv` pointers, a 0, the `envp` ones and another 0. RSP
  arrives pointing at `argc`, aligned to 16.
- The kernel builds everything in a vector, with the pointers already in
  program addresses, and copies it to the highest stack page through the
  physical address.
- `_start` is now **naked**: only assembly knows RSP at the instant of
  entry. It passes it to `__iniciar` (ordinary Rust), which stores argc/argv
  and calls `main`. Its `call` pushes the 8 bytes the compiler expects, and
  doc 13's `topo - 8` is no longer needed.
- The `&` belongs to the shell, not to the program: `rodar primos 300 &`
  delivers `["primos", "300"]`.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **Why does the kernel need to copy the argument strings, instead of
   giving the program a pointer to the line the shell built?**
   Because the line is on the kernel heap, which the program cannot read:
   the kernel pages do not have the USUARIO bit in the program's space (doc
   11). A pointer there would cause a page fault. The strings must be in the
   program's own pages.

2. **A program was called as `rodar args x`. What is `argc`, and what is in
   `argv[2]`?**
   `argc = 2` (`"args"` and `"x"`). `argv[2]` is the 0 that closes the list:
   the format guarantees that `argv[argc]` is always 0, so you can walk the
   list without knowing `argc`, stopping at the 0.

3. **Why does `montar` round RSP down, not up, to a multiple of 16?**
   Up, RSP would move into the string area, and `argc` and the pointers would
   be written over them. Down leaves a 0-to-15-byte gap between the words
   and the strings, which bothers nobody.

4. **Why can't `_start` be an ordinary Rust function that reads RSP with an
   `asm!("mov {}, rsp")` on its first line?**
   Because, by the time the first line of the body runs, the
   compiler-generated prologue may already have changed RSP (pushed
   registers, reserved space for variables), and the size of the prologue is
   not guaranteed. Only a naked function, with no prologue, sees RSP exactly
   as the kernel left it.

5. **The kernel delivers RSP as a multiple of 16. On entry to `__iniciar`,
   what is RSP modulo 16, and why?**
   8. `call` pushes the return address (8 bytes) before jumping, and the
   compiler assumes that in every function: on entry, RSP + 8 is a multiple
   of 16. With RSP misaligned some other way, an instruction like `movaps`
   with a stack address would cause a #GP.

6. **Why can `argumento` return `&'static str` if the string is on the
   stack, and stack variables die when the function returns?**
   Because that string does not belong to any function: the kernel put it
   above the initial RSP, before the first instruction. Everything the
   program pushes goes below it, and nothing pops the argument area. It
   lives until the program ends, which is what `'static` means.

7. **If a program could start another one with a syscall, why couldn't the
   kernel simply call `montar` with the argv the program passed?**
   Because that argv would come from ring 3: the pointers and strings could
   point into the kernel, to unmapped pages, or never end in a 0. Before
   copying, the kernel would have to validate every pointer and every string
   with `faixa_do_programa` and limit the total size. Today's shell runs in
   the kernel, and its strings can be trusted.

</details>
