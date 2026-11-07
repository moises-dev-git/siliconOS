# 13: RAM disk and ELF executables

> **You will understand:** how to write user programs **in Rust** (no
> longer in assembly) for a system with no standard library; what a
> **linker script** decides about the executable; what an **ELF** file
> looks like inside and what a loader must do with each **segment**
> (including `.bss`, which does not exist in the file); how the **tar**
> format keeps several files in one; and why a loader must distrust every
> number it reads.
>
> **Files:** `programas/` (new: the user programs crate, with the `sos`
> library, three programs and `linker.ld`), `build.rs` (new: builds the
> programs and assembles the tar), `src/ramdisk.rs` (new: reads the tar),
> `src/usuario/elf.rs` (new: reads and loads ELF), `iniciar_elf` and
> `faixa_do_programa` in `src/usuario/mod.rs`, the `ls`, `elf` and `rodar`
> commands in `src/shell.rs`
> **Book:** chapter 11, section 11.4 (the roadmap starts with "RAM disk",
> "virtual file system" and "load a program"; the implementation is ours)
> **Try it:** `./run.sh`, then `ls`, `elf oi`, `elf dados`, `rodar oi`,
> `rodar primos`, `rodar dados`; `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 10 (ring 3 and system calls), doc 11 (address spaces)
> and doc 12 (scheduler)
> **New Rust here:** `build.rs` and `include_bytes!` ([§40](rust-basico.md#40-buildrs-and-include_bytes-work-done-at-compile-time)),
> implementing `Iterator` and lifetimes in structs ([§41](rust-basico.md#41-implementing-iterator-and-lifetimes-in-structs)),
> `macro_rules!` ([§42](rust-basico.md#42-macro_rules-writing-your-own-macros)),
> const generics ([§43](rust-basico.md#43-const-generics-const-n-usize))
> **Names in the code:** `programas` = *programs*, `sos` = the SiliconOS
> user library, `sair` = *exit*, `escrever` = *write*, `tiques` = *ticks*,
> `principal` = *main*, `primos` = *primes*, `dados` = *data*, `oi` = *hi*,
> `ramdisk`/`arquivo(s)` = *file(s)*, `abrir` = *open*, `ler` = *read*,
> `cabecalho` = *header*, `segmento` = *segment*, `carregar` = *load*,
> `entrada` = *entry*, `permissoes` = *permissions*, `acrescentar_ao_tar`
> = *append to tar*
>
> **Code for this chapter:** [state at the end of doc 13](https://github.com/moises-dev-git/siliconOS/tree/doc-13) · [what changed since doc 12](https://github.com/moises-dev-git/siliconOS/compare/doc-12...doc-13) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

The programs from docs 10 to 12 are hand-written assembly, inside the kernel
itself (`programas.rs`). They work, but:

- writing anything useful in assembly is slow (`contador` needed 26 lines
  to print one digit 5 times);
- they are part of the kernel: a new program means rebuilding the kernel
  and editing the `PROGRAMAS` table;
- each one fits in a page and uses the same fixed layout (code on one page,
  stack on another). A real program has code, constants, global variables
  with initial values and zeroed variables, each part with different
  permissions.

In a real system, a program is a **file**: the compiler produces an
executable in a standard format (on Linux, ELF), the file sits on a disk,
and the kernel, when asked to run it, **reads the file and builds the
process** from what the file describes.

This chapter does that in three parts:

```
  on the development machine                           in SiliconOS
  ┌─────────────────────────────────────┐   kernel    ┌───────────────────────────────┐
  │ programas/src/bin/oi.rs  ─┐         │   .efi with │ ramdisk: reads the tar        │
  │ programas/src/bin/primos.rs├ cargo ─┤ ─ the tar ► │ elf: validates, reads segments│
  │ programas/src/bin/dados.rs─┘  (ELF) │   embedded  │ iniciar_elf: space + stack →  │
  │ build.rs: ELF → ramdisk.tar         │             │   scheduler                   │
  └─────────────────────────────────────┘             └───────────────────────────────┘
```

## 2. User programs in Rust

### 2.1 A separate crate, for another target

The kernel is compiled for `x86_64-unknown-uefi` (doc 00): a PE executable
the UEFI firmware knows how to load. User programs do not run on UEFI; they
run on SiliconOS, which is not a target Rust knows about yet. The closest
target is **`x86_64-unknown-none`**: x86-64, no operating system, no
standard library, producing ELF. (To install it, if missing: `rustup target
add x86_64-unknown-none`.)

So the programs live in a crate of their own, in the `programas/` folder:

```
programas/
├── Cargo.toml          the crate, with an empty [workspace]: not part of the kernel
├── .cargo/config.toml  target and compilation flags
├── build.rs            one line: relink when linker.ld changes
├── linker.ld           the linker script (section 2.3)
└── src/
    ├── lib.rs          the `sos` library (section 2.2)
    └── bin/
        ├── oi.rs       says hi with its own pid
        ├── primos.rs   counts the primes below 10,000
        └── dados.rs    shows .data and .bss working
```

```toml
[package]
name = "programas"
version = "0.1.0"
edition = "2024"
license = "MIT"

# A workspace of its own: this crate is not part of the kernel crate.
[workspace]

[lib]
name = "sos"            # the "system library" the programs use
path = "src/lib.rs"

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
opt-level = "s"         # small programs
```

Each file in `src/bin/` becomes an executable with the same name; all of
them use the library in `src/lib.rs`. `panic = "abort"`: with no system
underneath, there is no way to "unwind" the stack on a panic (doc 02).

### 2.2 The `sos` library: what libc does on Linux

A `no_std` program has no `println!` and no `std::process::exit`. In a real
system, those come from the **system library** (on Linux, libc): ordinary
functions that make system calls inside. Ours is `programas/src/lib.rs`, and
every function is an `int 0x80` (doc 10):

```rust
#![no_std]

use core::arch::asm;
use core::fmt;

// System call numbers (the same as in the kernel's src/usuario/mod.rs).
const SYS_SAIR: u64 = 0;
const SYS_ESCREVER: u64 = 1;
const SYS_TIQUES: u64 = 2;
const SYS_PID: u64 = 3;

/// Ends the program with the given code. Does not return.
pub fn sair(codigo: u64) -> ! {
    unsafe { asm!("int 0x80", in("rax") SYS_SAIR, in("rdi") codigo, options(noreturn)) }
}

/// Writes the bytes to the console. Returns how many were written, or a
/// negative error (-14 if the kernel rejects the pointer).
pub fn escrever(bytes: &[u8]) -> i64 {
    let r: u64;
    unsafe {
        // The kernel preserves every register except RAX (the result).
        asm!("int 0x80", inlateout("rax") SYS_ESCREVER => r,
             in("rdi") bytes.as_ptr(), in("rsi") bytes.len(), options(nostack));
    }
    r as i64
}

/// Timer ticks since boot (100 per second).
pub fn tiques() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") SYS_TIQUES => r, options(nostack)) };
    r
}

/// This process's number.
pub fn pid() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") SYS_PID => r, options(nostack)) };
    r
}
```

The `asm!` (doc 01, [§15](rust-basico.md#15-asm-assembly-inside-rust)) tells
the compiler exactly doc 10's contract: the number goes in RAX, the
arguments in RDI and RSI, the result comes back in RAX (`inlateout`: one
value goes in, another comes out in the same register), and no other
register changes (the `entrada_syscall` handler restores them all).
`options(noreturn)` on `sair`: the compiler knows nothing comes after it.

On top of that, `print!` and `println!` with `core::fmt` formatting (the
same the kernel uses in `kprintln!`, doc 01):

```rust
/// Target of `print!`: each piece of formatted text becomes an `escrever`.
pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        escrever(s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::Console, $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{ $crate::print!($($arg)*); $crate::print!("\n"); }};
}
```

(`macro_rules!` is in [§42](rust-basico.md#42-macro_rules-writing-your-own-macros).)

And the two pieces every program without `std` needs: an **entry point**
and a **panic handler**:

```rust
/// Defines the `_start` entry point (what the ELF says to run first), which
/// calls `main` and passes the returned value to `sair`.
#[macro_export]
macro_rules! principal {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _start() -> ! {
            $crate::sair($main())
        }
    };
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic no programa: {}", info);
    sair(101)
}
```

- **`_start`** is the name the linker looks for as the entry (the script's
  `ENTRY(_start)`, section 2.3). `#[unsafe(no_mangle)]` keeps the name exactly
  like that in the executable (without it, Rust "decorates" names with the
  module path and a hash).
- Each program's `main` returns a `u64`, which becomes the exit code.
- A panic in the program prints the message and exits with 101 (the same
  code a Rust program uses on Linux when it panics).

With that, a program looks like this:

```rust
//! SiliconOS's first Rust program: says hi and exits with 0.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    println!("Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e {}.", sos::pid());
    0
}
```

(*"Hi! I am a Rust program, loaded from an ELF file. My pid is ..."*.)
`#![no_main]`: there is no "real" `main` for `std` to call; `_start` is where
it begins. (The text has no accented letters because `escrever` turns bytes
outside ASCII into `?`, doc 10.)

The other two:

```rust
//! Counts the primes below 10,000 and exits with the count.
#![no_std]
#![no_main]

use sos::{print, println, principal};

principal!(main);

fn primo(n: u64) -> bool {
    n >= 2 && (2..).take_while(|d| d * d <= n).all(|d| n % d != 0)
}

fn main() -> u64 {
    let inicio = sos::tiques();
    let mut quantos = 0;
    print!("primeiros primos:");
    for n in 0..10_000 {
        if primo(n) {
            quantos += 1;
            if quantos <= 10 {
                print!(" {}", n);
            }
        }
    }
    println!();
    println!("{} primos abaixo de 10000 ({} tiques)", quantos, sos::tiques() - inicio);
    quantos
}
```

```rust
//! Shows the .data and .bss sections: global variables with an initial
//! value (which come from the file) and zeroed ones (which the file does NOT
//! store; the kernel zeroes them).
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

/// In .data: the value 41 is stored in the ELF file.
static mut CONTADOR: u64 = 41;
/// In .bss: 64 KiB of zeros that take up nothing in the file.
static mut TABELA: [u64; 8192] = [0; 8192];

fn main() -> u64 {
    // `&raw` and explicit reads/writes: a `static mut` cannot become an
    // ordinary reference (two parts of the program could change it at once).
    let tabela = unsafe { &mut *&raw mut TABELA };
    let zerada = tabela.iter().all(|&x| x == 0);
    for (i, x) in tabela.iter_mut().enumerate() {
        *x = i as u64;
    }
    let soma: u64 = tabela.iter().sum();
    let contador = unsafe {
        CONTADOR += 1;
        CONTADOR
    };
    println!(".bss zerada: {}; soma depois de escrever: {}; .data: 41 -> {}",
             if zerada { "sim" } else { "NAO" }, soma, contador);
    contador
}
```

`dados` is the loader's test: if `.bss` is not zeroed, or if `.data` does
not come from the file, it says so.

### 2.3 The linker script: where each part lives

The compiler produces code **with addresses**: a `call`, a pointer to a
string, everything depends on where the program will be in memory. The
**linker** decides, following a **linker script**. Ours,
`programas/linker.ld`:

```
/* Linker script for the user programs (docs/13-disco-em-ram-e-elf.md).
 * Each section starts on a new page, so the kernel can give different
 * permissions to code (R-X), constants (R--) and data (RW-). */
ENTRY(_start)

SECTIONS
{
    . = 0x200000000000;       /* PML4 entry 64: the programs' area (doc 11) */

    /* With code-model=large, the compiler puts our code in .ltext,
     * .lrodata, .ldata and .lbss ("l" for large); the standard library's
     * core comes with the usual names. The .got (an address table the core
     * code looks up) is filled in at link time, but the linker marks it
     * writable: if it went with the constants, they would become writable
     * too. It goes with the data. */
    .text   : ALIGN(4K) { *(.text .text.* .ltext .ltext.*) }
    .rodata : ALIGN(4K) { *(.rodata .rodata.* .lrodata .lrodata.*) }
    .data   : ALIGN(4K) { *(.data .data.* .ldata .ldata.*) *(.got .got.*) }
    .bss    : ALIGN(4K) { *(.bss .bss.* .lbss .lbss.*) *(COMMON) }

    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
```

(The real file has these comments in Portuguese.) The four classic sections
of an executable:

| Section | What it holds | Permission | In the file? |
|---|---|---|---|
| `.text` | the code | read and execute | yes |
| `.rodata` | constants (the `println!` strings) | read only | yes |
| `.data` | global variables with an initial value (`CONTADOR = 41`) | read and write | yes |
| `.bss` | zeroed global variables (`TABELA`) | read and write | **no**: only its size |

`.bss` is an old saving: 64 KiB of zeros need not take 64 KiB in the file;
it is enough to note "64 KiB of zeros go here", and whoever loads the
program creates them in memory.

And the compilation flags, in `programas/.cargo/config.toml`:

```toml
# How to compile the user programs (the kernel's build.rs uses this file).
[build]
target = "x86_64-unknown-none"

[target.x86_64-unknown-none]
rustflags = [
    # Fixed addresses, chosen by our linker script (no PIE, no relocations
    # for the kernel to apply).
    "-C", "link-arg=-Tlinker.ld",
    "-C", "relocation-model=static",
    # The program lives above 2 GiB (0x2000_0000_0000): the "small" model
    # only reaches 32-bit absolute addresses.
    "-C", "code-model=large",
]
```

- **`relocation-model=static`:** by default, the `x86_64-unknown-none`
  target produces **relocatable** executables (PIE), which can be loaded at
  any address as long as the loader fixes up the internal pointers (the
  "relocations"). With a fixed address, the ELF comes out as type `EXEC`,
  and the loader only needs to copy.
- **`code-model=large`:** the default model ("small") assumes the whole
  program lives in the first 2 GiB, because it uses 32-bit absolute
  addresses in some instructions. The first 512 GiB are PML4 entry 0, which
  belongs to the kernel (doc 11); the program lives at `0x2000_0000_0000`.
  The "large" model uses 64-bit addresses where needed.

### 2.4 Three stumbles on the way (and what they taught)

The script above is the third version. The first two produced ELFs with
problems that Linux's `readelf -l` showed right away:

1. **Segments sharing a page.** The first version only listed `.text`,
   `.rodata`, `.data` and `.bss`. The result had the constants segment
   starting at `0x2000_0000_0bbb`, in the middle of the code page:

   ```
   LOAD  0x001000 0x0000200000000000 ... 0x000bbb 0x000bbb R E 0x1000
   LOAD  0x001bbb 0x0000200000000bbb ... 0x00051d 0x00051d R   0x1000
   ```

   A page has one permission; the loader could not give R-X and R-- to the
   same page. The cause: with `code-model=large`, the compiler puts our code
   in `.ltext.*`, `.lrodata.*` sections (the "l" is for *large*), which the
   script did not mention, and the linker placed them wherever it liked.
2. **The script changed, the ELF did not.** After adding `.ltext` and
   friends, `readelf`'s output was **identical**. Cargo only relinks when a
   file it knows about changes, and it did not know the executable depends
   on `linker.ld`. Hence `programas/build.rs`, one line long:

   ```rust
   // Cargo does not know the programs depend on the linker script: without
   // this line, changing linker.ld would do nothing until the next `cargo clean`.
   fn main() {
       println!("cargo:rerun-if-changed=linker.ld");
   }
   ```

3. **Writable constants.** The second version put `.got` together with the
   constants; the result had only two data segments, R-X and **RW-**: the
   constants had become writable, because the linker marks `.got` writable
   and the segment inherits the most open permission of the sections it
   holds. Moving `.got` next to `.data` fixed it.

The final result, for `dados`:

```
  LOAD  0x001000 0x0000200000000000 0x0000200000000000 0x000f32 0x000f32 R E 0x1000
  LOAD  0x002000 0x0000200000001000 0x0000200000001000 0x000160 0x000160 R   0x1000
  LOAD  0x003000 0x0000200000002000 0x0000200000002000 0x000020 0x000020 RW  0x1000
  LOAD  0x004000 0x0000200000003000 0x0000200000003000 0x000000 0x010000 RW  0x1000
```

The last line is `.bss`: **0 bytes in the file, 0x10000 (64 KiB) in
memory**.

## 3. The RAM disk: a tar inside the kernel

### 3.1 Why a "RAM disk"

The kernel cannot read a real disk yet (that would take a SATA, NVMe or
virtio driver). The classic way out, used by almost every system at boot
(Linux's *initrd* / *initramfs*), is a **RAM disk**: a file that is already
in memory when the kernel starts, holding the files it needs. Ours lives
inside the kernel executable itself.

### 3.2 The tar format

`tar` (*tape archive*) dates from 1979, made to write several files in
sequence to a magnetic tape. It is the simplest format there is for
"several files in one", and it is still everywhere (`.tar.gz`). Each file
is:

```
  ┌───────────────────── 512 bytes ─────────────────────┐
  │ header: name, size (in octal!), type, checksum...   │
  ├─────────────────────────────────────────────────────┤
  │ the file's data, padded with zeros to a multiple    │
  │ of 512                                              │
  └─────────────────────────────────────────────────────┘
  ... the next file ...
  [512 zeros][512 zeros]   ← end
```

The header fields (USTAR format) we use:

| Bytes | Field | How it is stored |
|---|---|---|
| 0–99 | name | text, ending in 0 |
| 100–107 | permissions | octal as text (`0000755`) |
| 124–135 | size | octal as text (`00000047120`) |
| 148–155 | checksum | octal as text |
| 156 | type | `'0'` = regular file |
| 257–262 | signature | `"ustar"` |

Numbers in **octal written as text**: a legacy of the machines of the time,
and a reminder that old formats last. The **checksum** is the sum of all
512 header bytes, counting the checksum field itself as 8 spaces; it
detects a corrupted header.

### 3.3 Building the tar: `build.rs`

A **build script** (`build.rs`, at the kernel crate's root) is a Rust
program that cargo compiles and runs **on the development machine**, before
compiling the crate
([§40](rust-basico.md#40-buildrs-and-include_bytes-work-done-at-compile-time)).
Ours builds the programs and assembles the tar:

```rust
fn main() {
    let raiz = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let programas = raiz.join("programas");
    let saida = PathBuf::from(env::var("OUT_DIR").unwrap());
    // A build directory separate from the kernel's: the outer cargo holds a
    // lock on its own while this script runs.
    let alvo_dir = raiz.join("target").join("programas");

    println!("cargo:rerun-if-changed=programas/src");
    println!("cargo:rerun-if-changed=programas/linker.ld");
    println!("cargo:rerun-if-changed=programas/Cargo.toml");
    println!("cargo:rerun-if-changed=programas/.cargo/config.toml");

    // 1. Build. The environment variables the outer cargo sets for this
    //    script (flags for the UEFI kernel) must not leak into the inner one.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&programas)
        .args(["build", "--release", "--target-dir"])
        .arg(&alvo_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .expect("nao consegui rodar o cargo para os programas");
    assert!(status.success(), "a compilacao dos programas de usuario falhou");

    // 2. The programs are the files in programas/src/bin, in alphabetical order.
    let mut nomes: Vec<String> = fs::read_dir(programas.join("src/bin"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    nomes.sort();

    let binarios = alvo_dir.join("x86_64-unknown-none").join("release");
    let mut tar = Vec::new();
    for nome in &nomes {
        let dados = fs::read(binarios.join(nome)).unwrap();
        acrescentar_ao_tar(&mut tar, nome, &dados);
    }
    // The end of a tar: two zeroed 512-byte blocks.
    tar.extend_from_slice(&[0u8; 1024]);

    fs::write(saida.join("ramdisk.tar"), &tar).unwrap();
}
```

- The script runs **another cargo** inside the `programas/` folder (which
  has its own `.cargo/config.toml` with the target and flags). This is
  `std`: the build script runs on Linux, with everything the standard
  library offers.
- A separate `--target-dir`: the outer cargo holds a lock on the kernel's
  `target/` while the script runs; using the same directory, the inner cargo
  would wait forever.
- `env_remove`: the outer cargo passes the script variables with the
  kernel's flags; if they leaked into the inner one, it would use them for
  the programs.
- `rerun-if-changed`: without these lines, cargo would rerun the script on
  every build (the default is "any file in the crate changed").
- **`OUT_DIR`** is a folder cargo gives each build script for the files it
  generates.

And the assembly of each entry, following the table in section 3.2:

```rust
fn acrescentar_ao_tar(tar: &mut Vec<u8>, nome: &str, dados: &[u8]) {
    assert!(nome.len() < 100, "nome grande demais para o tar: {}", nome);
    let mut cab = [0u8; 512];
    cab[0..nome.len()].copy_from_slice(nome.as_bytes());        // name
    octal(&mut cab[100..108], 0o755);                            // permissions
    octal(&mut cab[108..116], 0);                                // owner
    octal(&mut cab[116..124], 0);                                // group
    octal(&mut cab[124..136], dados.len() as u64);               // size
    octal(&mut cab[136..148], 0);                                // modification time
    cab[156] = b'0';                                             // type: regular file
    cab[257..263].copy_from_slice(b"ustar\0");                   // signature
    cab[263..265].copy_from_slice(b"00");                        // version
    // Checksum: the sum of every header byte, counting the field itself as
    // 8 spaces.
    cab[148..156].copy_from_slice(b"        ");
    let soma: u32 = cab.iter().map(|&b| b as u32).sum();
    octal(&mut cab[148..155], soma as u64);
    cab[155] = b' ';

    tar.extend_from_slice(&cab);
    tar.extend_from_slice(dados);
    let resto = dados.len() % 512;
    if resto != 0 {
        tar.extend(std::iter::repeat_n(0u8, 512 - resto));
    }
}

/// Writes `valor` in octal, with leading zeros and a 0 byte at the end.
fn octal(campo: &mut [u8], valor: u64) {
    let texto = format!("{:0largura$o}\0", valor, largura = campo.len() - 1);
    campo.copy_from_slice(texto.as_bytes());
}
```

The proof that the format is right is that Linux's `tar` reads the
generated file:

```
$ tar tvf target/x86_64-unknown-uefi/debug/build/siliconos/*/out/ramdisk.tar
-rwxr-xr-x 0/0           20544 1969-12-31 21:00 dados
-rwxr-xr-x 0/0           16024 1969-12-31 21:00 oi
-rwxr-xr-x 0/0           20896 1969-12-31 21:00 primos
```

(The date is Unix's "time zero", January 1, 1970 UTC, shown in Brasília's
time zone: we wrote 0 in the date field.)

### 3.4 Reading the tar in the kernel: `ramdisk.rs`

The kernel embeds the file with `include_bytes!`, which reads a file **at
compile time** and turns it into a `&'static [u8]` inside the executable:

```rust
/// The whole file, embedded in the kernel executable at compile time.
static RAMDISK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ramdisk.tar"));
```

(`env!("OUT_DIR")` is also read at compile time: it is the folder where
`build.rs` left the tar.)

Each file is returned as slices of `RAMDISK` itself, with no copy:

```rust
/// A file on the disk: name and contents (slices of the tar itself: nothing
/// is copied). `'a` is the tar's lifetime; for RAMDISK, `'static`.
#[derive(Clone, Copy)]
pub struct Arquivo<'a> {
    pub nome: &'a str,
    pub dados: &'a [u8],
}
```

Reading the octal fields and checking the sum:

```rust
fn ler_octal(campo: &[u8]) -> Option<u64> {
    let mut valor: u64 = 0;
    for &c in campo.iter().take_while(|&&c| c != 0 && c != b' ') {
        if !(b'0'..=b'7').contains(&c) {
            return None;
        }
        valor = valor.checked_mul(8)?.checked_add((c - b'0') as u64)?;
    }
    Some(valor)
}

fn cabecalho_valido(cab: &[u8]) -> bool {
    let soma: u64 = cab.iter().enumerate()
        .map(|(i, &b)| if (148..156).contains(&i) { b' ' as u64 } else { b as u64 })
        .sum();
    &cab[257..262] == b"ustar" && ler_octal(&cab[148..156]) == Some(soma)
}
```

`checked_mul`/`checked_add` return `None` if the arithmetic overflows: an
absurd size in a damaged header becomes "invalid file", not a wrong number.

And the walk, as an **iterator**
([§41](rust-basico.md#41-implementing-iterator-and-lifetimes-in-structs)):
callers write `for a in ramdisk::arquivos()` or `arquivos().find(...)`, and
the tar is read bit by bit, one header at a time:

```rust
pub struct Arquivos<'a> {
    tar: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Arquivos<'a> {
    type Item = Arquivo<'a>;

    fn next(&mut self) -> Option<Arquivo<'a>> {
        let cab = self.tar.get(self.pos..self.pos + BLOCO)?;
        if cab.iter().all(|&b| b == 0) || !cabecalho_valido(cab) {
            return None; // end of the file (or garbage: we stop to be safe)
        }
        let tamanho_nome = cab[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let nome = core::str::from_utf8(&cab[..tamanho_nome]).ok()?;
        let tamanho = ler_octal(&cab[124..136])? as usize;
        let inicio = self.pos + BLOCO;
        let dados = self.tar.get(inicio..inicio.checked_add(tamanho)?)?;
        // The next header starts at the first multiple of 512 after the data.
        self.pos = inicio + tamanho.div_ceil(BLOCO) * BLOCO;
        Some(Arquivo { nome, dados })
    }
}

pub fn arquivos() -> Arquivos<'static> {
    Arquivos { tar: RAMDISK, pos: 0 }
}

pub fn abrir(nome: &str) -> Option<Arquivo<'static>> {
    arquivos().find(|a| a.nome == nome)
}
```

Notice `self.tar.get(a..b)?`: `get` with a range returns `None` instead of
panicking if the range runs off the array. A size pointing past the end of
the tar ends the walk instead of bringing the kernel down.

## 4. The ELF format

### 4.1 What is inside

**ELF** (*Executable and Linkable Format*) is the executable format of
Linux, the BSDs and almost every system that is neither Windows nor macOS.
An ELF has two "views" of the same file:

- the **sections** view (`.text`, `.rodata`, `.symtab`...), useful for the
  linker and debuggers;
- the **segments** view (*program headers*), useful for whoever **loads**
  it: each `PT_LOAD` segment says "copy these bytes from the file to this
  address, with these permissions".

The kernel only needs the second. The way to it:

```
  offset 0   ┌──────────────────────────────┐
             │ ELF header (64 bytes)        │  identification, type, machine,
             │   e_entry  = 0x2000_0000_0068 │  entry point,
             │   e_phoff  = 64              ─┼─┐ where the segment table is
             │   e_phnum  = 5               │ │ and how many there are
  offset 64  ├──────────────────────────────┤ │
             │ program header 0 (56 bytes) ◄┼─┘ type, flags (R/W/X),
             │ program header 1             │   offset in the file,
             │ ...                          │   address, size in the file,
             ├──────────────────────────────┤   size in memory
  0x1000     │ .text                        │
  0x2000     │ .rodata                      │
  0x3000     │ .data                        │
             │ .symtab, .strtab... (ignored)│
             └──────────────────────────────┘
```

The fields used, with each one's offset (all little-endian):

| Header | Bytes | | Program header | Bytes |
|---|---|---|---|---|
| signature `7F 'E' 'L' 'F'` | 0–3 | | `p_type` (1 = LOAD) | 0–3 |
| class (2 = 64 bits) | 4 | | `p_flags` (X=1, W=2, R=4) | 4–7 |
| byte order (1 = little-endian) | 5 | | `p_offset` | 8–15 |
| `e_type` (2 = EXEC) | 16–17 | | `p_vaddr` | 16–23 |
| `e_machine` (0x3E = x86-64) | 18–19 | | `p_filesz` | 32–39 |
| `e_entry` | 24–31 | | `p_memsz` | 40–47 |
| `e_phoff` | 32–39 | | | |
| `e_phentsize` (56) | 54–55 | | | |
| `e_phnum` | 56–57 | | | |

### 4.2 Reading: `ler_cabecalho` and `segmento`

```rust
// Reading little-endian numbers at any position in the file.
fn ler<const N: usize>(dados: &[u8], pos: u64) -> Result<[u8; N], ErroElf> {
    let pos = usize::try_from(pos).map_err(|_| ErroElf::Truncado)?;
    let fim = pos.checked_add(N).ok_or(ErroElf::Truncado)?;
    dados.get(pos..fim).map(|b| b.try_into().unwrap()).ok_or(ErroElf::Truncado)
}
fn u16_em(d: &[u8], p: u64) -> Result<u16, ErroElf> { Ok(u16::from_le_bytes(ler(d, p)?)) }
fn u32_em(d: &[u8], p: u64) -> Result<u32, ErroElf> { Ok(u32::from_le_bytes(ler(d, p)?)) }
fn u64_em(d: &[u8], p: u64) -> Result<u64, ErroElf> { Ok(u64::from_le_bytes(ler(d, p)?)) }
```

`ler::<N>` takes `N` bytes starting at `pos`, or returns `Truncado`
(*truncated*) if the file ends first (`N` is a **const** generic parameter,
[§43](rust-basico.md#43-const-generics-const-n-usize)). `u64::from_le_bytes`
builds the number from 8 bytes in little-endian order. Unlike doc 08
(ACPI), we do not use `read_unaligned` on a pointer here: everything goes
through `get`, which checks the bounds. The file is outside data, and
reading past it would be a security bug.

```rust
pub fn ler_cabecalho(dados: &[u8]) -> Result<Cabecalho, ErroElf> {
    if dados.get(0..4) != Some(b"\x7fELF") {
        return Err(ErroElf::NaoEElf);
    }
    if ler::<1>(dados, 4)?[0] != 2 {
        return Err(ErroElf::NaoSuportado("nao e de 64 bits"));
    }
    if ler::<1>(dados, 5)?[0] != 1 {
        return Err(ErroElf::NaoSuportado("nao e little-endian"));
    }
    if u16_em(dados, 16)? != 2 {
        return Err(ErroElf::NaoSuportado("nao e executavel de endereco fixo (ET_EXEC)"));
    }
    if u16_em(dados, 18)? != 0x3E {
        return Err(ErroElf::NaoSuportado("nao e para x86-64"));
    }
    if u16_em(dados, 54)? != TAM_PROGRAM_HEADER {
        return Err(ErroElf::NaoSuportado("program header de tamanho estranho"));
    }
    Ok(Cabecalho {
        entrada: u64_em(dados, 24)?,
        ph_inicio: u64_em(dados, 32)?,
        ph_quantidade: u16_em(dados, 56)?,
    })
}

pub fn segmento(dados: &[u8], cab: &Cabecalho, i: u16) -> Result<Segmento, ErroElf> {
    let base = cab.ph_inicio
        .checked_add(i as u64 * TAM_PROGRAM_HEADER as u64)
        .ok_or(ErroElf::Truncado)?;
    Ok(Segmento {
        tipo: u32_em(dados, base)?,
        flags: u32_em(dados, base + 4)?,
        deslocamento: u64_em(dados, base + 8)?,
        endereco: u64_em(dados, base + 16)?,
        tam_arquivo: u64_em(dados, base + 32)?,
        tam_memoria: u64_em(dados, base + 40)?,
    })
}
```

(The `NaoSuportado` texts: *not 64-bit*, *not little-endian*, *not a
fixed-address executable*, *not for x86-64*, *odd program header size*.)

The possible errors are an `enum` (each one becomes a message in the shell
and a test case):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroElf {
    /// Does not start with 0x7F 'E' 'L' 'F'.
    NaoEElf,
    /// It is ELF, but not one we can run (32-bit, another CPU, not executable...).
    NaoSuportado(&'static str),
    /// The file ends before what the header promises.
    Truncado,
    /// A nonsensical segment (size in the file larger than in memory, overflowing sum).
    SegmentoInvalido,
    /// A segment wants to live in the kernel's area or outside the lower half.
    ForaDaAreaDoPrograma,
    /// Two segments want the same page.
    Sobreposto,
    /// The entry point is not in an executable segment.
    EntradaInvalida,
    /// Out of frames.
    SemMemoria,
}
```

### 4.3 Loading: page by page

```rust
pub fn carregar(dados: &[u8], espaco: &mut EspacoEnderecos) -> Result<u64, ErroElf> {
    let cab = ler_cabecalho(dados)?;
    for i in 0..cab.ph_quantidade {
        let s = segmento(dados, &cab, i)?;
        if s.tipo == PT_LOAD {
            carregar_segmento(dados, &s, espaco)?;
        }
    }
    // The first instruction must be on a loaded, executable page.
    match espaco.traduzir(cab.entrada) {
        Some(t) if t.flags & flags::SEM_EXECUCAO == 0 => Ok(cab.entrada),
        _ => Err(ErroElf::EntradaInvalida),
    }
}
```

The other segment types (`GNU_RELRO`, `GNU_STACK`, which the `elf` command
shows in section 7) are hints for more sophisticated loaders and can be
ignored.

```rust
fn carregar_segmento(dados: &[u8], s: &Segmento, espaco: &mut EspacoEnderecos) -> Result<(), ErroElf> {
    // Never trust the file: consistent sizes and no overflowing sums.
    let fim_no_arquivo = s.deslocamento.checked_add(s.tam_arquivo).ok_or(ErroElf::SegmentoInvalido)?;
    let fim_na_memoria = s.endereco.checked_add(s.tam_memoria).ok_or(ErroElf::SegmentoInvalido)?;
    if s.tam_arquivo > s.tam_memoria {
        return Err(ErroElf::SegmentoInvalido);
    }
    if fim_no_arquivo > dados.len() as u64 {
        return Err(ErroElf::Truncado);
    }

    // The page's permissions come from the segment's flags.
    let mut bits = flags::USUARIO;
    if s.flags & PF_W != 0 {
        bits |= flags::ESCRITA;
    }
    if s.flags & PF_X == 0 {
        bits |= flags::SEM_EXECUCAO;
    }

    let mut pagina = s.endereco & !(TAMANHO_QUADRO - 1);
    while pagina < fim_na_memoria {
        if !EspacoEnderecos::do_programa(pagina) {
            return Err(ErroElf::ForaDaAreaDoPrograma);
        }
        let q = quadros::alocar().ok_or(ErroElf::SemMemoria)?;
        let destino = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
        unsafe { core::ptr::write_bytes(destino, 0, TAMANHO_QUADRO as usize) };

        // The part of [endereco, endereco + tam_arquivo) that falls on this
        // page comes from the file; the rest of the page stays zeroed
        // (that is where .bss lives).
        let ini = pagina.max(s.endereco);
        let fim = (pagina + TAMANHO_QUADRO).min(s.endereco + s.tam_arquivo);
        if ini < fim {
            let origem = (s.deslocamento + (ini - s.endereco)) as usize;
            let n = (fim - ini) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(dados[origem..origem + n].as_ptr(),
                                               destino.add((ini - pagina) as usize), n);
            }
        }

        espaco.mapear(pagina, q, bits).map_err(|e| match e {
            ErroMapeamento::JaMapeado => ErroElf::Sobreposto,
            ErroMapeamento::SemMemoria => ErroElf::SemMemoria,
            _ => ErroElf::ForaDaAreaDoPrograma,
        })?;
        pagina += TAMANHO_QUADRO;
    }
    Ok(())
}
```

For each page the segment touches, in memory:

```
  segment: endereco = E, tam_arquivo = A, tam_memoria = M (M ≥ A)

  memory:   E ──────────── E+A ──────────────── E+M
            │ from the file │   zeros (.bss)     │
            └─── page 1 ─┘└─── page 2 ─┘└─── page 3 ─┘
                 copies all   copies the start,  zeros only
                              the rest stays 0
```

1. A new, **zeroed** frame. Zeroing is required for two reasons: `.bss` and
   the end of the page after the data must be zero, and the frame may hold
   leftovers from another process (doc 10).
2. The intersection between the page `[pagina, pagina + 4096)` and the part
   of the segment that comes from the file `[E, E + A)` is copied to the
   right place inside the frame. If there is no intersection (a page that
   is all `.bss`), nothing is copied.
3. The frame is mapped with the segment's permissions: USUARIO always,
   ESCRITA if it has `W`, SEM_EXECUCAO if it does **not** have `X`. This is
   where the linker script's work turns into real protection: code cannot
   be changed, and data cannot be executed.

**Why distrust everything.** A loader is the entry door for outside data (a
file anyone could have written). Every number read may be wrong by
accident or on purpose, and each one is checked: sums with `checked_add` (an
`endereco` near `u64::MAX` plus a size would wrap around and pass a `<`),
`tam_arquivo ≤ tam_memoria`, the segment inside the file, every page in the
program's area (`do_programa`, doc 11: no mapping on top of the kernel), no
page twice (`JaMapeado` becomes `Sobreposto`), and the entry in an
executable segment.

**And if something fails halfway?** Some pages are already mapped in the
space. Nothing has to be undone by hand: the caller owns the space, and
when it is destroyed (the `?` leaves the function and `Drop` runs, doc 11),
the pages go back to the allocator.

## 5. Running an ELF: `iniciar_elf`

```rust
/// Top of the ELF programs' stack (doc 13): end of PML4 entry 255, the last
/// address of the lower half, far from where the segments live.
pub const PILHA_ELF_TOPO: u64 = 0x0000_8000_0000_0000;
/// How many stack pages an ELF program gets (16 KiB).
pub const PILHA_ELF_PAGINAS: u64 = 4;

pub fn iniciar_elf(nome: &'static str, dados: &[u8], segundo_plano: bool) -> Result<Pid, elf::ErroElf> {
    let mut espaco = EspacoEnderecos::novo().ok_or(elf::ErroElf::SemMemoria)?;
    // If anything fails from here on, `espaco` is destroyed on the way out
    // (`?`) and returns everything that had already been mapped.
    let entrada = elf::carregar(dados, &mut espaco)?;
    for i in 1..=PILHA_ELF_PAGINAS {
        let q = quadros::alocar().ok_or(elf::ErroElf::SemMemoria)?;
        unsafe { core::ptr::write_bytes(paginacao::fisico_para_virtual(q.endereco()) as *mut u8, 0, 4096) };
        espaco.mapear(PILHA_ELF_TOPO - i * TAMANHO_QUADRO, q,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO)
            .map_err(|_| elf::ErroElf::SemMemoria)?;
    }
    // RSP = top - 8: the compiler generates `_start` as an ordinary function,
    // which expects to have been called with `call` (which pushes 8 bytes of
    // return address on a 16-aligned stack). With a misaligned stack, an
    // instruction that requires alignment (such as `movaps`) would #GP.
    // Today's three programs ran even with RSP = top (tested): they just use
    // none of those. The bug would stay hidden until the first one that did.
    Ok(escalonador::criar(nome, espaco, entrada, PILHA_ELF_TOPO - 8, segundo_plano))
}
```

- **The stack lives at the end of the lower half** (PML4 entry 255), far
  from the code at `0x2000_0000_0000`: the program can have as many
  segments as it likes without bumping into it. `0x0000_8000_0000_0000`
  itself is not a canonical address (doc 04), but it is never used as an
  address: only as "one past the end". The pages go from `0x7FFF_FFFF_C000`
  to `0x7FFF_FFFF_FFFF`.
- **Stack alignment** is a calling convention rule (doc 10,
  [§37](rust-basico.md#37-calling-conventions-extern-sysv64)): at the start
  of any function, `RSP + 8` is a multiple of 16. The test with
  `RSP = top` is in section 7.3: the programs ran, which shows the CPU does
  not check the rule on every instruction, only on those that depend on it.
- The rest is doc 12: `escalonador::criar` builds the kernel stack and the
  process joins the queue.

### 5.1 Validating pointers through the page tables

In doc 10, `faixa_do_programa` checked whether a pointer coming from the
program was in one of the two fixed pages (code and stack). An ELF has as
many pages as it likes, at addresses the file chooses. The right question
is a different one: **"is this memory the program's?"**, and the page tables
of the active space (that of the program making the call) know the answer:

```rust
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    let mut pagina = inicio & !(TAMANHO_QUADRO - 1);
    while pagina < fim {
        let do_programa = EspacoEnderecos::do_programa(pagina)
            && paginacao::traduzir(pagina).is_some_and(|t| t.flags & flags::USUARIO != 0);
        if !do_programa {
            return false;
        }
        pagina += TAMANHO_QUADRO;
    }
    true
}
```

Every page in the range must be mapped, with the USUARIO bit, in a PML4
entry that belongs to the program. It works for the built-in programs and
for ELF ones, and for any future layout.

## 6. The shell: `ls`, `elf` and `rodar`

```rust
fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario;
    let Some(&nome) = argumentos.first() else {
        kprintln!("uso: rodar <programa> [&]   (veja 'programas' e 'ls')");
        return;
    };
    let segundo_plano = argumentos.get(1) == Some(&"&");
    // First the built-in programs (doc 10), then the files on the RAM disk
    // (doc 13).
    let pid = if let Some(programa) = usuario::programas::procurar(nome) {
        usuario::iniciar(programa, segundo_plano)
    } else if let Some(arquivo) = crate::ramdisk::abrir(nome) {
        match usuario::iniciar_elf(arquivo.nome, arquivo.dados, segundo_plano) {
            Ok(pid) => pid,
            Err(erro) => {
                kprintln!("'{}' nao pode ser carregado: {:?}", nome, erro);
                return;
            }
        }
    } else {
        kprintln!("'{}' nao existe (veja 'programas' e 'ls')", nome);
        return;
    };
    if segundo_plano {
        // Background (doc 12): the shell does not wait. When the program
        // ends, the scheduler prints the result.
        kprintln!("[{}] '{}' em segundo plano", pid, nome);
    } else {
        let r = escalonador::esperar(pid);
        kprintln!("'{}' {}", nome, usuario::descrever(r));
    }
}
```

`ls` lists the disk, and `elf <file>` shows the header and segments, like
Linux's `readelf -l` (the code is in [Changes to the
files](#changes-to-the-files)). `arquivo.nome` is a `&'static str` because it
points inside `RAMDISK`, which lives forever; that is why it can become the
process name, which the scheduler keeps.

## 7. How it was tested

### 7.1 From the shell

Real output (serial), QEMU with 128 MiB:

```
silicon> mem
quadros: 20272 livres de 32768 (79 MiB livres)
heap: 224 bytes em uso (4 alocacoes vivas), pico 0 KiB
      mapeado 1024 KiB, 1 blocos livres, maior 1023 KiB
silicon> ls
  dados          20544 bytes  executavel ELF
  oi             16024 bytes  executavel ELF
  primos         20896 bytes  executavel ELF
3 arquivos; o disco em RAM (tar) ocupa 60928 bytes
silicon> elf oi
entrada: 0x200000000068; 5 segmentos
  TIPO   ARQUIVO   ENDERECO            NO ARQUIVO  NA MEMORIA  PERM
  LOAD   0x001000  0x0000200000000000        3013        3013  R-X
  LOAD   0x002000  0x0000200000001000         344         344  R--
  LOAD   0x003000  0x0000200000002000          16          16  RW-
  RELRO  0x003000  0x0000200000002000          16          16  R--
  STACK  0x000000  0x0000000000000000           0           0  RW-
silicon> elf dados
entrada: 0x200000000130; 6 segmentos
  TIPO   ARQUIVO   ENDERECO            NO ARQUIVO  NA MEMORIA  PERM
  LOAD   0x001000  0x0000200000000000        3890        3890  R-X
  LOAD   0x002000  0x0000200000001000         352         352  R--
  LOAD   0x003000  0x0000200000002000          32          32  RW-
  LOAD   0x004000  0x0000200000003000           0       65536  RW-
  RELRO  0x003000  0x0000200000002000          32          32  R--
  STACK  0x000000  0x0000000000000000           0           0  RW-
silicon> rodar oi
Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 1.
'oi' terminou: codigo de saida 0
silicon> rodar primos
primeiros primos: 2 3 5 7 11 13 17 19 23 29
1229 primos abaixo de 10000 (2 tiques)
'primos' terminou: codigo de saida 1229
silicon> rodar dados
.bss zerada: sim; soma depois de escrever: 33550336; .data: 41 -> 42
'dados' terminou: codigo de saida 42
silicon> rodar dados
.bss zerada: sim; soma depois de escrever: 33550336; .data: 41 -> 42
'dados' terminou: codigo de saida 42
silicon> rodar primos &
[5] 'primos' em segundo plano
silicon> primeiros primos: 2 3 5 7 11 13 17 19 23 29
1229 primos abaixo de 10000 (2 tiques)

[5] 'primos' terminou: codigo de saida 1229
rodar oi &
[6] 'oi' em segundo plano
silicon> Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 6.

[6] 'oi' terminou: codigo de saida 0
rodar nada
'nada' nao existe (veja 'programas' e 'ls')
silicon> mem
quadros: 20272 livres de 32768 (79 MiB livres)
heap: 224 bytes em uso (4 alocacoes vivas), pico 16 KiB
      mapeado 1024 KiB, 1 blocos livres, maior 1023 KiB
```

(`ARQUIVO` = *file offset*, `ENDERECO` = *address*, `NO ARQUIVO` / `NA
MEMORIA` = *in the file* / *in memory*; `primeiros primos` = *first primes*;
`.bss zerada: sim` = *.bss zeroed: yes*; `soma depois de escrever` = *sum
after writing*; `nao existe` = *does not exist*.)

Reading it:

- **The three ELFs run**, with `println!` formatting numbers.
- **`dados` twice, 41 → 42 both times.** Each run gets a fresh copy of
  `.data` from the file; what the first process wrote died with its space.
  The sum `33550336` is 0 + 1 + ... + 8191 = 8191 × 8192 / 2: `.bss` was
  zeroed and accepted writes.
- **`dados`'s `.bss`** shows up in `elf` as 0 bytes in the file and 65536 in
  memory.
- **Nothing leaks:** 20272 free frames before and after. (The boot number is
  lower than doc 12's 20298: the kernel executable grew by about 100 KiB,
  60 KiB of it from the tar.)

### 7.2 `cargo test`

46 tests, all `[ok]`: the 40 from before and 6 new ones (3 in `ramdisk.rs`,
3 in `elf.rs`). The tar ones:

```rust
#[test_case]
fn os_tres_programas_estao_no_disco() {
    let nomes: alloc::vec::Vec<&str> = arquivos().map(|a| a.nome).collect();
    assert_eq!(nomes, ["dados", "oi", "primos"]);
    assert!(arquivos().all(|a| a.dados.starts_with(b"\x7fELF")));
    assert!(abrir("nao-existe").is_none());
}

#[test_case]
fn octal_do_tar() {
    assert_eq!(ler_octal(b"00000017331\0"), Some(0o17331));
    assert_eq!(ler_octal(b"0000644 "), Some(0o644));
    assert_eq!(ler_octal(b"0009"), None);
}

#[test_case]
fn cabecalho_estragado_para_a_leitura() {
    // A copy of the tar with one byte of the first name changed: the
    // checksum no longer matches, and reading stops right at the start.
    let mut copia = alloc::vec::Vec::from(RAMDISK);
    assert_eq!(Arquivos { tar: &copia, pos: 0 }.count(), 3);
    copia[0] ^= 0x20;
    assert_eq!(Arquivos { tar: &copia, pos: 0 }.count(), 0);
}
```

The ELF ones: one reads `oi`'s header, one runs the three programs and
checks the exit codes (0, 1229 and 42), and the most important one damages
copies of `oi` in eight ways and checks that **each is rejected with the
right error and without leaking a frame**:

```rust
#[test_case]
fn arquivos_ruins_recusados_sem_vazar() {
    let livres = quadros::livres();
    let ph = |d: &Vec<u8>| u64::from_le_bytes(d[32..40].try_into().unwrap()) as usize;
    assert_eq!(carregar_alterado(|d| d[0] = 0), Err(ErroElf::NaoEElf));
    assert_eq!(carregar_alterado(|d| d[4] = 1), Err(ErroElf::NaoSuportado("nao e de 64 bits")));
    assert_eq!(carregar_alterado(|d| d[18] = 0x28), Err(ErroElf::NaoSuportado("nao e para x86-64")));
    assert_eq!(carregar_alterado(|d| d.truncate(0x1800)), Err(ErroElf::Truncado));
    // The first segment asking to live in the kernel heap:
    assert_eq!(carregar_alterado(|d| { let p = ph(d) + 16; d[p..p + 8].copy_from_slice(&0xFFFF_9000_0000_0000u64.to_le_bytes()) }),
               Err(ErroElf::ForaDaAreaDoPrograma));
    // tam_arquivo larger than tam_memoria:
    assert_eq!(carregar_alterado(|d| { let p = ph(d) + 40; d[p..p + 8].copy_from_slice(&1u64.to_le_bytes()) }),
               Err(ErroElf::SegmentoInvalido));
    // The second segment at the same address as the first:
    assert_eq!(carregar_alterado(|d| { let p = ph(d) + 56 + 16; d[p..p + 8].copy_from_slice(&0x0000_2000_0000_0000u64.to_le_bytes()) }),
               Err(ErroElf::Sobreposto));
    // Entry point outside the code:
    assert_eq!(carregar_alterado(|d| d[24..32].copy_from_slice(&0x0000_2000_0000_1000u64.to_le_bytes())),
               Err(ErroElf::EntradaInvalida));
    assert_eq!(quadros::livres(), livres);
}
```

(`0x28` is the machine number of 32-bit ARM; `0x1800` cuts the file in the
middle of the code; `0x2000_0000_1000` is the constants page, which is not
executable.)

Doc 10's `validacao_de_ponteiros` test changed along with
`faixa_do_programa`: it now builds a space with the two pages, activates it,
asks, and switches back to the kernel's CR3 before the `assert`s (like doc
11's test). It also gained a case: a range that starts on the last bytes of
the code page and runs into the next page, which does not exist, is
rejected.

### 7.3 And with a misaligned stack?

To check the comment in `iniciar_elf`, `PILHA_ELF_TOPO - 8` became
`PILHA_ELF_TOPO` (and then went back). The three programs **ran normally**:

```
silicon> rodar oi
Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 1.
'oi' terminou: codigo de saida 0
silicon> rodar primos
primeiros primos: 2 3 5 7 11 13 17 19 23 29
1229 primos abaixo de 10000 (1 tiques)
'primos' terminou: codigo de saida 1229
silicon> rodar dados
.bss zerada: sim; soma depois de escrever: 33550336; .data: 41 -> 42
'dados' terminou: codigo de saida 42
```

The first version of the comment said that, without the `- 8`, the
instructions that require alignment "would #GP". The test showed that this
only holds for code that **uses** those instructions, and none of the three
does. The comment was corrected. It is the kind of bug that does not show
today and shows months later: the `- 8` stays.

### 7.4 Regression

The 21 builds with test features still pass.

## What is still missing

- **A real file system.** The RAM disk is just a flat, read-only list of
  names, embedded in the kernel. Missing: directories, writing, a disk
  driver, and the layer the book calls **VFS** (*virtual file system*): a
  single interface (`open`, `read`, `close`) that works the same for the
  tar, a disk and devices.
- **File system calls.** Programs cannot open files; only the kernel reads
  the tar.
- **Arguments.** A program gets no arguments (`argv`) or environment
  variables; on a Unix system, the loader puts them at the top of the stack
  before jumping to `_start`.
- **Relocatable ELF (PIE) and dynamic libraries.** Only fixed-address
  `ET_EXEC` is accepted.
- **`int 0x80` on every `println!`.** Each formatted piece is a system call;
  the next chapter replaces the mechanism with `syscall`/`sysret`, built for
  this.

---

## Changes to the files

What this chapter adds to the code of doc 12 (tag `doc-12`). Each file's link
leads to its state at the end of this chapter (tag `doc-13`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-12...doc-13).

| File | What changes | Where |
|---|---|---|
| [`programas/Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/Cargo.toml) | **new** (section 2.1) | — |
| [`programas/.cargo/config.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/.cargo/config.toml) | **new** (section 2.3) | — |
| [`programas/linker.ld`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/linker.ld) | **new** (section 2.3) | — |
| [`programas/build.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/build.rs) | **new** (section 2.4) | — |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | **new**: the `sos` library (section 2.2) | — |
| [`programas/src/bin/`](https://github.com/moises-dev-git/siliconOS/tree/doc-13/programas/src/bin) | **new**: `oi.rs`, `primos.rs`, `dados.rs` (section 2.2) | — |
| [`programas/Cargo.lock`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/Cargo.lock) | **new**, generated by cargo | — |
| [`build.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | **new**: builds the programs and assembles the tar (section 3.3) | project root |
| [`.gitignore`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/.gitignore) | `/programas/target/` | after `/target/` |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | **new**: `RAMDISK`, `Arquivo`, `ler_octal`, `cabecalho_valido`, `Arquivos` (iterator), `arquivos`, `abrir`, `tamanho_total`, 3 tests | — |
| [`src/usuario/elf.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | **new**: `ErroElf`, `Cabecalho`, `Segmento`, constants, `ler`/`u16_em`/`u32_em`/`u64_em`, `ler_cabecalho`, `segmento`, `carregar`, `carregar_segmento`, `permissoes`, 3 tests | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/main.rs) | `mod ramdisk;` | after `mod escalonador;` |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/mod.rs) | `pub mod elf;`; `PILHA_ELF_TOPO`, `PILHA_ELF_PAGINAS`, `iniciar_elf`; `rodar` with `cfg_attr` (only the tests use it); `faixa_do_programa` through the tables; `validacao_de_ponteiros` test rewritten | `iniciar_elf` before `rodar` |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/shell.rs) | `rodar` accepts files from the disk; `ls` and `elf` commands; `rodar`'s description | — |

### The snippets the text did not show

The top of `build.rs` (the `use` lines):

```rust
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
```

In `ramdisk.rs`, the constant and the total size:

```rust
const BLOCO: usize = 512;

pub fn tamanho_total() -> usize {
    RAMDISK.len()
}
```

In `elf.rs`, the `use` lines, the structs and the constants:

```rust
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao::{self, flags, ErroMapeamento};
use crate::memoria::{quadros, TAMANHO_QUADRO};

/// What we care about in the ELF header (the first 64 bytes).
#[derive(Debug, Clone, Copy)]
pub struct Cabecalho {
    /// Address of the first instruction (`e_entry`).
    pub entrada: u64,
    /// Where the segment table starts in the file (`e_phoff`).
    pub ph_inicio: u64,
    /// How many segments (`e_phnum`).
    pub ph_quantidade: u16,
}

/// An entry of the segment table ("program header", 56 bytes).
#[derive(Debug, Clone, Copy)]
pub struct Segmento {
    pub tipo: u32,
    pub flags: u32,
    pub deslocamento: u64,
    pub endereco: u64,
    pub tam_arquivo: u64,
    pub tam_memoria: u64,
}

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;
const TAM_PROGRAM_HEADER: u16 = 56;

/// Short text with a segment's permissions: "R-X", "RW-"...
pub fn permissoes(flags: u32) -> [u8; 3] {
    [
        if flags & PF_R != 0 { b'R' } else { b'-' },
        if flags & PF_W != 0 { b'W' } else { b'-' },
        if flags & PF_X != 0 { b'X' } else { b'-' },
    ]
}
```

The start of `elf.rs`'s tests:

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::escalonador;
    use crate::ramdisk;
    use crate::usuario::Resultado;
    use alloc::vec::Vec;

    fn rodar(nome: &'static str) -> Resultado {
        let pid = crate::usuario::iniciar_elf(nome, ramdisk::abrir(nome).unwrap().dados, false).unwrap();
        escalonador::esperar(pid)
    }

    /// Tries to load a copy of `oi` changed by `mexer`, without running it.
    fn carregar_alterado(mexer: impl FnOnce(&mut Vec<u8>)) -> Result<u64, ErroElf> {
        let mut copia = Vec::from(ramdisk::abrir("oi").unwrap().dados);
        mexer(&mut copia);
        let mut espaco = EspacoEnderecos::novo().unwrap();
        carregar(&copia, &mut espaco)
    }

    #[test_case]
    fn cabecalho_do_oi() {
        let dados = ramdisk::abrir("oi").unwrap().dados;
        let cab = ler_cabecalho(dados).unwrap();
        let s = segmento(dados, &cab, 0).unwrap();
        assert_eq!((s.tipo, s.endereco), (PT_LOAD, 0x0000_2000_0000_0000));
        assert_eq!(&permissoes(s.flags), b"R-X");
        assert!(cab.entrada >= s.endereco && cab.entrada < s.endereco + s.tam_memoria);
    }

    #[test_case]
    fn programas_em_rust_rodam() {
        assert_eq!(rodar("oi"), Resultado::Saiu(0));
        assert_eq!(rodar("primos"), Resultado::Saiu(1229));
        assert_eq!(rodar("dados"), Resultado::Saiu(42));
    }

    // arquivos_ruins_recusados_sem_vazar: section 7.2
}
```

In `usuario/mod.rs`, `rodar` (now only the tests use it) and the new test:

```rust
/// Runs the program and waits for it to end. Since doc 13, the shell does
/// the same by hand (so it also accepts ELF files); the tests use this one.
#[cfg_attr(not(test), allow(dead_code))]
pub fn rodar(programa: &programas::Programa) -> Resultado {
    let pid = iniciar(programa, false);
    escalonador::esperar(pid)
}
```

```rust
    #[test_case]
    fn validacao_de_ponteiros() {
        // The questions go to the ACTIVE space's tables: build a space with
        // the usual two pages and activate it during the test.
        let mut espaco = EspacoEnderecos::novo().unwrap();
        espaco.mapear(CODIGO_VIRT, nova_pagina_zerada(), flags::USUARIO).unwrap();
        espaco.mapear(PILHA_VIRT, nova_pagina_zerada(), flags::USUARIO | flags::ESCRITA).unwrap();
        espaco.ativar();
        let respostas = [
            faixa_do_programa(CODIGO_VIRT, 10),
            faixa_do_programa(PILHA_VIRT + 4000, 96),
            faixa_do_programa(PILHA_VIRT + 4000, 97),          // runs past the end
            faixa_do_programa(CODIGO_VIRT + 4090, 10),         // the next page does not exist
            faixa_do_programa(0xFFFF_9000_0000_0000, 16),      // kernel
            faixa_do_programa(u64::MAX - 4, 10),               // overflows
        ];
        paginacao::ativar(paginacao::raiz_kernel());
        assert_eq!(respostas, [true, true, false, false, false, false]);
    }
```

In `shell.rs`, the command table lines and the `ls` and `elf` commands:

```rust
    Comando { nome: "rodar", uso: "rodar <programa> [&]", descricao: "roda um programa (embutido ou do disco) no anel 3; & = segundo plano", funcao: rodar_programa },
    Comando { nome: "ps", uso: "ps", descricao: "lista os processos", funcao: ps },
    Comando { nome: "ls", uso: "ls", descricao: "lista os arquivos do disco em RAM", funcao: ls },
    Comando { nome: "elf", uso: "elf <arquivo>", descricao: "mostra o cabecalho e os segmentos de um ELF", funcao: elf },
```

```rust
/// `ls` command (doc 13): the files on the RAM disk.
fn ls(_: &[&str]) {
    let mut total = 0;
    for a in crate::ramdisk::arquivos() {
        let tipo = if a.dados.starts_with(b"\x7fELF") { "executavel ELF" } else { "" };
        kprintln!("  {:<12}{:>8} bytes  {}", a.nome, a.dados.len(), tipo);
        total += 1;
    }
    kprintln!("{} arquivos; o disco em RAM (tar) ocupa {} bytes", total, crate::ramdisk::tamanho_total());
}

/// `elf <file>` command (doc 13): the header and segments, like Linux's
/// `readelf -l`.
fn elf(argumentos: &[&str]) {
    use crate::usuario::elf;
    let Some(arquivo) = argumentos.first().and_then(|n| crate::ramdisk::abrir(n)) else {
        kprintln!("uso: elf <arquivo>   (veja 'ls')");
        return;
    };
    let cab = match elf::ler_cabecalho(arquivo.dados) {
        Ok(c) => c,
        Err(e) => { kprintln!("nao e um ELF que o SiliconOS roda: {:?}", e); return; }
    };
    kprintln!("entrada: {:#x}; {} segmentos", cab.entrada, cab.ph_quantidade);
    kprintln!("  TIPO   ARQUIVO   ENDERECO            NO ARQUIVO  NA MEMORIA  PERM");
    for i in 0..cab.ph_quantidade {
        let Ok(s) = elf::segmento(arquivo.dados, &cab, i) else { break };
        let tipo = match s.tipo { elf::PT_LOAD => "LOAD", 0x6474_e551 => "STACK", 0x6474_e552 => "RELRO", _ => "outro" };
        let p = elf::permissoes(s.flags);
        kprintln!("  {:<6} {:#08x}  {:#018x}  {:>10}  {:>10}  {}", tipo, s.deslocamento, s.endereco,
                  s.tam_arquivo, s.tam_memoria, core::str::from_utf8(&p).unwrap());
    }
}
```

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `sair` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | System call 0: ends the program with the given code. |
| `escrever` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | System call 1: writes bytes to the console and returns how many (or a negative error). |
| `tiques` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | System call 2: timer ticks since boot. |
| `pid` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | System call 3: the process number. |
| `Console::write_str` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | `fmt::Write`: each piece of formatted text becomes an `escrever` (the base of `print!`/`println!`). |
| `panic` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | The programs' panic handler: prints the message and exits with 101. |
| `_start` (macro `principal!`) | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Entry point generated by the macro: calls the program's `main` and passes the result to `sair`. |
| `main` (oi) | [programas/src/bin/oi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/oi.rs) | Says hi with its own pid and returns 0. |
| `primo` | [programas/src/bin/primos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/primos.rs) | Is `n` prime? (divisions up to the square root) |
| `main` (primos) | [programas/src/bin/primos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/primos.rs) | Counts the primes below 10,000, shows the first 10 and returns the count (1229). |
| `main` (dados) | [programas/src/bin/dados.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/dados.rs) | Checks that `.bss` came zeroed, writes to it, adds 1 to `.data`'s `CONTADOR` and returns the value (42). |
| `main` (programas/build.rs) | [programas/build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/build.rs) | Tells cargo to relink the programs when `linker.ld` changes. |
| `main` (build.rs) | [build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | Builds the programs with another cargo and packs the ELFs into `OUT_DIR/ramdisk.tar`. |
| `acrescentar_ao_tar` | [build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | Writes a USTAR header (with checksum) and the data, padded to a multiple of 512. |
| `octal` | [build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | Writes a number in octal, with leading zeros and a 0 at the end, into a header field. |
| `ler_octal` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Reads an octal number written as text, rejecting invalid digits and overflows. |
| `cabecalho_valido` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Checks a header's "ustar" signature and checksum. |
| `Arquivos::next` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | The iterator: reads a header, returns name and data (slices of the tar) and jumps to the next multiple of 512. |
| `arquivos` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | An iterator over every file on the RAM disk. |
| `abrir` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | The file with that name, if it exists. |
| `tamanho_total` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Size of the whole tar, in bytes. |
| `ler` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | `N` bytes of the file starting at a position, or `Truncado`. |
| `u16_em` / `u32_em` / `u64_em` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | A little-endian number at a position in the file. |
| `ler_cabecalho` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Checks signature, class, byte order, type, machine and program header size; returns entry and table. |
| `segmento` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Reads program header number `i`. |
| `carregar` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Loads every LOAD segment into a space and checks that the entry is executable. |
| `carregar_segmento` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Validates the segment and, page by page, allocates a zeroed frame, copies the file's part and maps it with the segment's permissions. |
| `permissoes` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | A segment's flags as "R-X", "RW-"... |
| `iniciar_elf` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/mod.rs) | Creates the space, loads the ELF, maps 4 stack pages at the end of the lower half and hands it to the scheduler with RSP = top − 8. |
| `ls` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/shell.rs) | `ls` command: the files on the RAM disk. |
| `elf` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/shell.rs) | `elf <file>` command: header and segments, like `readelf -l`. |

Automated tests (`#[test_case]`, run with `cargo test`, see doc 09):

| Function | File | What it does |
|---|---|---|
| `os_tres_programas_estao_no_disco` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Test: the tar has dados, oi and primos, all ELF, and a missing name gives `None`. |
| `octal_do_tar` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Test: octal numbers ending in 0 and in a space, and a digit 9 rejected. |
| `cabecalho_estragado_para_a_leitura` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Test: changing one byte of the first header breaks the sum, and the walk returns no file. |
| `rodar` (in the tests) | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Helper: loads an ELF from the disk, runs it and waits for the result. |
| `carregar_alterado` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Helper: loads a copy of `oi` modified by a closure. |
| `cabecalho_do_oi` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Test: `oi`'s first segment is LOAD, R-X, at `0x2000_0000_0000`, and contains the entry. |
| `programas_em_rust_rodam` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Test: oi, primos and dados exit with 0, 1229 and 42. |
| `arquivos_ruins_recusados_sem_vazar` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Test: eight different kinds of damage, each rejected with the right error, without losing frames. |

---

## Difference from the book

The roadmap in the book's section 11.4 starts right here: RAM disk, virtual
file system and loading a program. Our choices: the tar format (the book
does not fix one), the disk embedded in the kernel executable by a
`build.rs` (instead of loaded by the firmware), user programs in Rust with a
library of our own, the layout fixed by a linker script, `ET_EXEC` with no
relocations, and no VFS yet.

## Summary

- User programs in **Rust**: a separate crate for `x86_64-unknown-none`,
  `no_std`/`no_main`, and a library (`sos`) playing libc's role: every
  function is a system call.
- The **linker script** fixes the addresses (`0x2000_0000_0000`) and puts
  each section on its own page, so each can have its own permission.
  `relocation-model=static` gives an `EXEC` ELF; `code-model=large` allows
  living above 2 GiB.
- The **RAM disk** is a **tar** file (512-byte headers, octal numbers,
  checksum), assembled by `build.rs` and embedded with `include_bytes!`.
- An **ELF** is loaded through its **PT_LOAD segments**: page by page, a
  zeroed frame, the part that comes from the file copied in, the rest zero
  (the `.bss`), and the R/W/X permissions become ESCRITA and SEM_EXECUCAO.
- The loader **distrusts every number**: sums checked, segment inside the
  file and in the program's area, no overlap, executable entry. On error,
  the space's `Drop` gives everything back.
- System call pointer validation now asks the **page tables** of the active
  space.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **Why do the programs need a crate separate from the kernel?**
   Because they are compiled for another target (`x86_64-unknown-none`,
   producing ELF) with other flags and another linker script. The kernel is
   a UEFI (PE) executable.

2. **What is different about `.bss` compared with the other sections?**
   It takes no space in the file: the ELF only gives its size
   (`tam_memoria` larger than `tam_arquivo`). Whoever loads it creates the
   zeros in memory.

3. **Why does the loader zero the frame before copying, if it is going to
   copy over it?**
   Because not every page is covered by the file: `.bss` and the end of the
   last page of each segment must be zero. And a reused frame holds
   leftovers from its previous user.

4. **What would happen if the linker script did not put each section on a
   new page?**
   Two segments with different permissions would share a page, and a page
   has only one permission. That was the first stumble in section 2.4.

5. **Why does `carregar_segmento` use `checked_add` instead of `+`?**
   Because the numbers come from a file that may be damaged or built to
   deceive. An address near `u64::MAX` plus a size would wrap around and
   pass a `<` comparison.

6. **Why did `faixa_do_programa` stop comparing against `CODIGO_VIRT` and
   `PILHA_VIRT`?**
   Because an ELF can have as many pages as it likes, wherever the file
   chooses. The question "is this memory the program's?" is answered by
   the active space's page tables: mapped, with USUARIO, in a program
   entry.

7. **Why is the initial RSP `PILHA_ELF_TOPO - 8`, and why didn't the test
   with `PILHA_ELF_TOPO` fail?**
   The calling convention says that, at the start of a function, `RSP + 8`
   is a multiple of 16 (as if a `call` had just pushed the return address).
   Only instructions that require alignment (such as `movaps`) fail with a
   misaligned stack, and none of the three programs uses them.

</details>
