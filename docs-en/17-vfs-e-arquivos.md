# 17: VFS and files (open, read, close)

> **You will understand:** what a **virtual file system** (VFS) is, the
> layer that lets the same `open` work for files that come from completely
> different places; what a **mount** is; what a **file descriptor** is (the
> number `open` returns) and why each process has its own table of them; the
> **`abrir`** (open), **`ler`** (read) and **`fechar`** (close) syscalls;
> files the kernel **makes up on the spot** (like Linux's `/proc`); and why
> the kernel has to check that it may **write** to an address before
> copying data to a program.
>
> **Files:** `src/vfs/mod.rs`, `src/vfs/disco.rs` and `src/vfs/sis.rs`
> (new), `src/usuario/mod.rs` (syscalls 5, 6 and 7), `src/escalonador.rs`
> (per-process file table), `src/shell.rs` (`ls [directory]`), `build.rs`
> (puts text files on the disk), `programas/src/lib.rs` (`Arquivo`,
> `sos::cru`), `programas/src/bin/cat.rs`, `conferir.rs` and
> `programas/arquivos/leiame.txt` (new)
> **Book:** chapter 11, section 11.4 (the book's roadmap mentions the VFS
> between the ramdisk and the loader; the implementation is ours)
> **Try it:** `./run.sh`, then `ls`, `ls /sis`, `rodar cat /leiame.txt`,
> `rodar cat /sis/tempo /sis/processos` and `rodar conferir`;
> `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 13 (the RAM disk), doc 10 (validating program
> pointers), doc 16 (arguments, which `cat` uses)
> **New Rust here:** `&'static dyn Trait` in a `static`, arrays with
> `[const { None }; N]`, `Option::take`, `find_map`, a `Result` that hands
> the value back on error, and `mem::forget`
> ([§47](rust-basico.md#47-dyn-trait-in-a-static-arrays-of-option-take-and-forget))
>
> **Code for this chapter:** [state at the end of doc 17](https://github.com/moises-dev-git/siliconOS/tree/doc-17) · [what changed since doc 16](https://github.com/moises-dev-git/siliconOS/compare/doc-16...doc-17) · map in [Changes to the files](#changes-to-the-files)

Names in the code are in Portuguese: `abrir` = open, `ler` = read, `fechar`
= close, `arquivo` = file, `aberto` = open(ed), `descritores` = descriptors,
`montagem` = mount, `sistema de arquivos` = file system, `conteúdo` =
content, `fixo` = fixed, `gerado` = generated, `procurar` = look up,
`listar` = list, `pai` = parent, `guardar` = store, `restante` = remaining,
`disco` = disk, `sis` = sys(tem), `tempo` = time, `processos` = processes,
`conferir` = check, `prova` = test/trial, `cru` = raw, `leiame` = readme.

---

## 1. The problem: programs cannot read anything

Since doc 13, SiliconOS has a disk: a tar file inside the kernel, with the
programs. But only the **kernel** reads it: the shell looks up the
executable in the tar and loads it. A program, up in ring 3, has no way to
read a file. There is no syscall for it, and it cannot see the kernel
memory where the tar lives (doc 11).

A `cat` (the program that shows a file's contents) is impossible. And
programs that read data (configuration, text, images) are almost all of
them.

The obvious solution would be a `read_from_tar(name, destination)` syscall.
It works, but it ties programs to the tar. The day there is a real disk, or
a USB stick, or files the kernel generates, each would need its own
syscalls, and every program would have to know where each file comes from.

## 2. The idea: a single layer, many sources

Every Unix system solves this the same way, and the idea is old (the Unix of
the 70s already did it; the name **VFS**, *virtual file system*, comes from
Sun, in 1985):

1. **Programs only know paths** (`/leiame.txt`, `/sis/tempo`) and three
   operations: open, read, close.
2. **Each source of files is a "file system"** that knows how to do those
   operations its own way: the tar looks in the tar; a disk would read
   sectors; Linux's `/proc` makes the content up on the spot.
3. **The VFS sits in the middle.** A table, the **mount** table, says which
   file system is responsible for which part of the path tree. The VFS looks
   at the path, finds the mount and hands the rest of the path to it.

```
   program: abrir("/sis/tempo")            abrir("/leiame.txt")
                    │                               │
   ════════ syscall ╪═══════════════════════════════╪═════════════
                    ▼                               ▼
   VFS:        mount "/sis"                     mount "/"
               rest: "tempo"                    rest: "leiame.txt"
                    │                               │
                    ▼                               ▼
   systems:    Sis (generates the text now)     Disco (points into the tar)
```

On Linux, `mount` is the command that adds a line to that table: `mount
/dev/sdb1 /mnt/usb` says "whatever is below `/mnt/usb` is handled by this
USB stick's file system". Here the table is fixed, with two mounts: the RAM
disk at the root `/` and a new file system at `/sis`.

### 2.1 Files that do not exist

`/sis` is the fun part. Its files are not stored anywhere: when someone opens
`/sis/tempo`, the kernel **writes the text right then**, with the current
time. It is the idea of Linux's `/proc`, where `/proc/uptime` (how long the
machine has been on) and `/proc/meminfo` (memory) are generated on every
read. On Linux, `ls -l` shows those files with size 0, because the size only
exists after generating:

```
$ ls -l /proc/uptime /proc/meminfo      (on the development machine)
... 0 /proc/meminfo
... 0 /proc/uptime
$ cat /proc/uptime
16154.35 253265.61
```

To the program, there is no difference at all: it opens, reads and closes
as with any file. That is what the VFS buys: this chapter's `cat` shows
`leiame.txt` (from the tar) and `/sis/processos` (generated) without knowing
that one is different from the other.

### 2.2 Descriptors

Opening a file creates state: **which** file, and **how far** it has been
read. The kernel keeps that state and gives the program only a number, the
**file descriptor** (C's `fd`). In the following calls, the program names
the number: "read 64 bytes from 3".

Why a number and not a pointer to the state? For the usual reason: the state
lives in kernel memory, which the program cannot touch. A number is just an
index into a table the kernel keeps **per process**: one program's
descriptor 3 has nothing to do with another's 3. A program cannot read
another program's files by guessing numbers.

Two Unix details SiliconOS follows:

- **0, 1 and 2 are reserved**: standard input, output and error (the
  keyboard and the screen, in a terminal). SiliconOS does not have them yet
  (writing to the screen uses doc 10's `escrever` syscall), but keeps the
  numbers free. The first file opened gets 3.
- **The lowest free number is reused.** Closing 3 and opening another file
  gives 3 again.

You can see both on Linux, with three lines of Python that open, close and
reopen (on the development machine):

```python
a = os.open('/etc/hostname', os.O_RDONLY); b = os.open(...); os.close(a); c = os.open(...)
```
```
fds: 3 4 3
```

## 3. The VFS: `src/vfs/mod.rs`

### 3.1 The contract of a file system

```rust
/// What a file system must be able to do to be mounted.
/// `Sync`: the mounts are `static` and can be used from any process.
pub trait SistemaDeArquivos: Sync {
    /// Opens a file. `nome` is the path INSIDE the mount, without the
    /// leading slash ("leiame.txt", "tempo"). Our systems have no
    /// subdirectories.
    fn abrir(&self, nome: &str) -> Option<Conteudo>;
    /// The files in this mount: (name, size in bytes).
    fn listar(&self) -> Vec<(String, usize)>;
}
```

A *trait* is an interface
([§5](rust-basico.md#5-traits-interfaces)): any type that implements `abrir`
and `listar` can be mounted. The `: Sync` requires that the type can be used
by several processes at once; it is needed because the mounts live in a
`static` variable, visible from anywhere
([§47](rust-basico.md#47-dyn-trait-in-a-static-arrays-of-option-take-and-forget)).

What `abrir` returns is the content:

```rust
/// The bytes of an open file. The disk ones are already in memory (the tar
/// is inside the kernel): pointing at them is enough. The /sis ones are
/// generated at `abrir` time, a "snapshot" of the kernel's state.
pub enum Conteudo {
    Fixo(&'static [u8]),
    Gerado(Vec<u8>),
}

impl Conteudo {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Conteudo::Fixo(b) => b,
            Conteudo::Gerado(v) => v,
        }
    }
}
```

That is this VFS's big simplification: **every open file is already entirely
in memory**. With the tar it is true for free. With `/sis`, the whole text is
generated in `abrir`, and the following reads read from that snapshot
(opening `/sis/tempo` and reading 10 minutes later shows the time of the
`abrir`). On a real disk this would be impossible (a file can be gigabytes),
and Linux's VFS asks each file system for a `read` function that fetches
only the requested piece. The "What is still missing" section comes back to
this.

### 3.2 The mounts

```rust
/// A mounted path and the file system responsible for it.
struct Montagem {
    caminho: &'static str,
    sistema: &'static dyn SistemaDeArquivos,
}

/// The mount table. Order matters: the most specific first
/// ("/sis" before "/"), because `procurar` takes the first one that fits.
static MONTAGENS: &[Montagem] = &[
    Montagem { caminho: "/sis", sistema: &sis::Sis },
    Montagem { caminho: "/", sistema: &disco::Disco },
];
```

`&'static dyn SistemaDeArquivos` is "a reference to something that
implements `SistemaDeArquivos`, that lives forever"
([§34](rust-basico.md#34-dyn-trait-and-implementing-for-every-t)): `Sis` and
`Disco` are different types, but they fit in the same table because what is
stored is the reference plus the trait's function table. It is the
equivalent of an array of `SistemaDeArquivos` in Java, where each element is
of a class implementing the interface.

The lookup:

```rust
fn procurar(caminho: &str) -> Option<(&'static Montagem, &str)> {
    let caminho = caminho.strip_prefix('/').unwrap_or(caminho);
    MONTAGENS.iter().find_map(|m| {
        let base = m.caminho.trim_start_matches('/');
        if base.is_empty() {
            return Some((m, caminho)); // the root fits everything
        }
        let resto = caminho.strip_prefix(base)?;
        // "/sis/tempo" and "/sis" belong to /sis; "/sistema" does not.
        if resto.is_empty() {
            Some((m, resto))
        } else {
            resto.strip_prefix('/').map(|r| (m, r))
        }
    })
}
```

`find_map` walks the mounts and returns the first `Some` the closure
produces. Inside it, `strip_prefix(base)?` removes the start of the path or,
if the path does not start with it, makes the closure return `None` (`?`
works with `Option` too,
[§24](rust-basico.md#24-the--operator-propagating-errors)), and the search
moves on to the next mount.

The care with `/sistema` is the detail people most often get wrong in this
kind of code: `"sistema".strip_prefix("sis")` gives `"tema"`, and without
checking that the rest starts with `/` (or is empty), `/sistema` would land
in the `/sis` mount, as a file called `tema`. A test covers exactly that
(section 8.3).

Paths without the leading slash (`leiame.txt`) are treated as if they had
one. On Unix they would be relative to the process's **current directory**;
SiliconOS has no current directory yet, so "relative" and "from the root"
are the same thing.

With the mount in hand, opening is passing it along:

```rust
/// Opens a file by its full path.
pub fn abrir(caminho: &str) -> Option<ArquivoAberto> {
    let (montagem, nome) = procurar(caminho)?;
    if nome.is_empty() {
        return None; // it is a directory ("/sis"), not a file
    }
    montagem.sistema.abrir(nome).map(|conteudo| ArquivoAberto { conteudo, posicao: 0 })
}
```

### 3.3 Listing, with the mounts in between

The shell's `ls` must show, in `/`, the tar's files **and** the `sis/`
directory. But the tar does not know `/sis` exists: the mount table does.
So the VFS combines the two:

```rust
pub fn listar(diretorio: &str) -> Option<Vec<(String, usize)>> {
    let (montagem, nome) = procurar(diretorio)?;
    if !nome.is_empty() {
        return None; // our systems have no subdirectories
    }
    let mut itens = montagem.sistema.listar();
    for m in MONTAGENS {
        // A mount "/a" shows up in "/" as "a/".
        if m.caminho != montagem.caminho && pai(m.caminho) == montagem.caminho {
            itens.push((String::from(m.caminho.rsplit('/').next().unwrap()) + "/", 0));
        }
    }
    Some(itens)
}

/// The directory that contains `caminho`: pai("/sis") = "/".
fn pai(caminho: &str) -> &str {
    match caminho.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &caminho[..i],
    }
}
```

On Linux it is the same: the `/proc` directory shows up in `ls /` because it
is a **mount point**, not because the root file system has a `proc` folder
with something in it. (It has an empty folder, which `/proc` covers when
mounted; here, not even that.)

### 3.4 The open file

```rust
/// An open file: the content and how far it has been read.
pub struct ArquivoAberto {
    conteudo: Conteudo,
    posicao: usize,
}

impl ArquivoAberto {
    /// Copies the next bytes into `destino` and advances the position.
    /// Returns how many it copied: fewer than `destino.len()` near the end,
    /// 0 at the end.
    pub fn ler(&mut self, destino: &mut [u8]) -> usize {
        let restante = &self.conteudo.bytes()[self.posicao..];
        let n = restante.len().min(destino.len());
        destino[..n].copy_from_slice(&restante[..n]);
        self.posicao += n;
        n
    }

    /// How many bytes are still left to read.
    pub fn restante(&self) -> usize {
        self.conteudo.bytes().len() - self.posicao
    }
}
```

`ler` follows the contract of Unix's `read`, which has a catch: it can
return **fewer** bytes than requested without that being an error. Near the
end of the file it returns what is left; at the end it returns 0. It is the
0 (not an error) that says "done". That is why every read loop has the form
"read until you get 0", like `cat`'s (section 6.2).

### 3.5 The descriptor table

```rust
/// How many files a process can have open at the same time.
pub const MAX_ABERTOS: usize = 16;
/// The first descriptor. 0, 1 and 2 are, by Unix convention, standard
/// input, output and error; they are reserved for when SiliconOS has a
/// terminal.
pub const PRIMEIRO_DESCRITOR: u64 = 3;

/// A process's table of open files. Descriptor `d` is position
/// `d - PRIMEIRO_DESCRITOR`. When the process is reaped, the table dies with
/// it and every file left open is freed (the `Drop` of the `Vec`s).
pub struct Descritores {
    abertos: [Option<ArquivoAberto>; MAX_ABERTOS],
}

impl Descritores {
    pub const fn novo() -> Self {
        Descritores { abertos: [const { None }; MAX_ABERTOS] }
    }
```

An array of 16 slots, each one empty (`None`) or holding a file.
`[const { None }; MAX_ABERTOS]` is the way to build an array by repeating a
value that cannot be copied (`ArquivoAberto` has a `Vec` inside;
[§47](rust-basico.md#47-dyn-trait-in-a-static-arrays-of-option-take-and-forget)).

16 is very little: on the development machine, `ulimit -n` (Linux's
per-process limit of open files) answers 1048576. But a limit must exist:
without one, a program could open files in a loop until it exhausted the
kernel heap, and the whole machine would pay for it.

```rust
    /// Stores the file in the first free slot and returns the descriptor, or
    /// hands the file back if the table is full.
    pub fn guardar(&mut self, arquivo: ArquivoAberto) -> Result<u64, ArquivoAberto> {
        match self.abertos.iter().position(|a| a.is_none()) {
            Some(i) => {
                self.abertos[i] = Some(arquivo);
                Ok(i as u64 + PRIMEIRO_DESCRITOR)
            }
            None => Err(arquivo),
        }
    }

    /// The file for descriptor `d`, if it is open.
    pub fn arquivo(&mut self, d: u64) -> Option<&mut ArquivoAberto> {
        let i = d.checked_sub(PRIMEIRO_DESCRITOR)? as usize;
        self.abertos.get_mut(i)?.as_mut()
    }

    /// Closes descriptor `d` (and frees the file). False if it was not open.
    pub fn fechar(&mut self, d: u64) -> bool {
        let Some(i) = d.checked_sub(PRIMEIRO_DESCRITOR) else { return false };
        self.abertos.get_mut(i as usize).and_then(|a| a.take()).is_some()
    }
```

Three short functions, and each one has a point of care:

- **`guardar`** (store) looks for the **first** empty slot: that is what
  makes the lowest free descriptor the one reused. If the table is full, it
  hands the file **back** inside the `Err`. In Rust, `arquivo` was moved into
  the function; if it did not give it back, the caller would lose it for
  good (here it would not matter, it would be freed, but the type leaves the
  choice to the caller;
  [§47](rust-basico.md#47-dyn-trait-in-a-static-arrays-of-option-take-and-forget)).
- **`arquivo`** receives a number that came from the program, and so can be
  anything: 0, 2, 99, `u64::MAX`. `checked_sub` returns `None` instead of
  wrapping around (a `0 - 3` in `u64` would become a huge number,
  [§45](rust-basico.md#45-impl-fn-variants-with-fields-and-arithmetic-that-does-not-overflow)),
  and `get_mut` returns `None` for an index outside the array, instead of
  panicking as `abertos[i]` would. No number makes the kernel fall over.
- **`fechar`** uses `take()`, which takes the value out of the `Option` and
  leaves `None` in its place. The removed file dies right there, and a
  generated content's `Vec` goes back to the heap.

## 4. The two file systems

### 4.1 The disk: `src/vfs/disco.rs`

```rust
pub struct Disco;

impl SistemaDeArquivos for Disco {
    fn abrir(&self, nome: &str) -> Option<Conteudo> {
        // The data is already in the tar, inside the kernel: nothing is copied.
        ramdisk::abrir(nome).map(|a| Conteudo::Fixo(a.dados))
    }

    fn listar(&self) -> Vec<(String, usize)> {
        ramdisk::arquivos().map(|a| (String::from(a.nome), a.dados.len())).collect()
    }
}
```

An **adapter**: all the work of reading the tar already existed (doc 13);
`Disco` just dresses it in the VFS interface. `pub struct Disco;` is a
*struct* with no fields (there is no state: the tar is a `static`), which
exists only so there is somewhere to hang the trait implementation.

### 4.2 The kernel's insides: `src/vfs/sis.rs`

```rust
pub struct Sis;

/// The files in /sis and the function that generates each one.
const ARQUIVOS: &[(&str, fn() -> String)] = &[
    ("tempo", tempo),
    ("mem", mem),
    ("processos", processos),
];

impl SistemaDeArquivos for Sis {
    fn abrir(&self, nome: &str) -> Option<Conteudo> {
        let (_, gerar) = ARQUIVOS.iter().find(|(n, _)| *n == nome)?;
        Some(Conteudo::Gerado(gerar().into_bytes()))
    }

    fn listar(&self) -> Vec<(String, usize)> {
        // The size is the current one: generate and measure.
        ARQUIVOS.iter().map(|(n, gerar)| (String::from(*n), gerar().len())).collect()
    }
}
```

The same idea as the shell's command table (doc 09): a name and a function
pointer ([§31](rust-basico.md#31-function-pointers-fn)). Adding a file to
`/sis` is one line here and a function that builds the text. Today's three:

```rust
fn tempo() -> String {
    format!("{} ms ({} tiques a {} Hz)\n", timer::milissegundos(), timer::tiques(), timer::FREQUENCIA_HZ)
}

fn mem() -> String {
    let e = heap::estatisticas();
    format!("quadros livres: {}\nheap em uso: {} bytes\n", quadros::livres(), e.em_uso)
}

fn processos() -> String {
    let mut texto = String::from("PID NOME\n");
    for (pid, nome, _, _) in escalonador::listar() {
        texto += &format!("{:>3} {}\n", pid, nome);
    }
    texto
}
```

(The texts are in Portuguese: "ticks", "free frames", "heap in use",
"NAME".) Unlike Linux, `listar` measures the real size, by generating the
text (and throwing it away). It gets expensive if the file is large; for
three texts of a few dozen bytes, it does not matter.

`processos` calls `escalonador::listar`, which takes the process table lock.
That will matter in section 5.

### 4.3 Text files on the disk

Until now the tar only had executables. To give `cat` something to show,
`build.rs` (doc 13) now also packs the files in the `programas/arquivos/`
folder, as they are:

```rust
    let mut conteudo: Vec<(String, Vec<u8>)> = nomes.iter()
        .map(|nome| (nome.clone(), fs::read(binarios.join(nome)).unwrap()))
        .collect();
    // Since doc 17: the ordinary (text) files in programas/arquivos,
    // copied as they are.
    for entrada in fs::read_dir(programas.join("arquivos")).unwrap() {
        let caminho = entrada.unwrap().path();
        let nome = caminho.file_name().unwrap().to_string_lossy().into_owned();
        conteudo.push((nome, fs::read(&caminho).unwrap()));
    }
    conteudo.sort();
```

Today there is a single one, `leiame.txt` (readme), 420 bytes, in ASCII.
(Only ASCII because doc 10's `escrever` syscall replaces any non-ASCII byte
with `?`, and the console only draws ASCII.)

## 5. The syscalls

Three new numbers in `src/usuario/mod.rs`, and five error codes, all with
Linux's numbers (which is where the names in parentheses come from; C's
`errno`):

```rust
pub const SYS_ABRIR: u64 = 5;   // doc 17
pub const SYS_LER: u64 = 6;
pub const SYS_FECHAR: u64 = 7;
```
```rust
pub const ERRO_NAO_EXISTE: u64 = -2i64 as u64;      // ENOENT: file does not exist (doc 17)
pub const ERRO_DESCRITOR: u64 = -9i64 as u64;       // EBADF: descriptor is not open
pub const ERRO_ENDERECO: u64 = -14i64 as u64;       // EFAULT: invalid pointer
pub const ERRO_ABERTOS_DEMAIS: u64 = -24i64 as u64; // EMFILE: file table full
pub const ERRO_NOME_LONGO: u64 = -36i64 as u64;     // ENAMETOOLONG: path too long
pub const ERRO_SYSCALL: u64 = -38i64 as u64;        // ENOSYS: unknown call

/// The longest path `abrir` accepts, in bytes.
pub const MAX_CAMINHO: u64 = 255;
```

And three lines in `tratar_syscall`. `ler` is the first syscall with
**three** arguments: the third arrives in RDX, which both entries (the
`int 0x80` one and the `syscall` one) have saved in `Registros` since doc 10.

```rust
        SYS_ABRIR => abrir(r.rdi, r.rsi),
        SYS_LER => ler(r.rdi, r.rsi, r.rdx),
        SYS_FECHAR => fechar(r.rdi),
```

### 5.1 Whose descriptor table it is

Each process has its own, and it lives in the scheduler's `Processo`:

```rust
    /// The open files (doc 17). Dies with the process: whatever was left
    /// open is freed when it is reaped.
    arquivos: Descritores,
```

The field is not `pub`: outside the scheduler, the only door is this
function, which gives access to the table **of the current process** (the
one that made the syscall):

```rust
/// Runs `f` with the current process's table of open files (doc 17), with
/// interrupts off and the table lock held: `f` must be short and cannot call
/// anything that takes the lock again (such as `listar`).
pub fn com_arquivos<R>(f: impl FnOnce(&mut Descritores) -> R) -> R {
    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let a = t.atual;
        f(&mut t.processos[a].arquivos)
    })
}
```

The same shape as `cpu::sem_interrupcoes`
([§30](rust-basico.md#30-taking-a-closure-impl-fnonce)): instead of returning
a reference to the table (which would stay alive after the lock was
released), the function **receives the code** that wants to use the table
and runs it with the lock held. There is no way to forget to release the
lock, nor to keep the reference for later.

The warning in the comment is serious. The process table lock (a
`spin::Mutex`, [§11](rust-basico.md#11-mutex-and-once-from-the-spin-crate))
is not reentrant: if `f` called something that takes the same lock, the
process would spin forever waiting for itself, with interrupts off. The
machine would hang (it is doc 08's `teste-deadlock`, by another route).

### 5.2 `abrir`

```rust
fn abrir(ponteiro: u64, tamanho: u64) -> u64 {
    if tamanho > MAX_CAMINHO {
        return ERRO_NOME_LONGO;
    }
    if !faixa_do_programa(ponteiro, tamanho) {
        return ERRO_ENDERECO;
    }
    // Copy the path into the kernel before using it: the memory belongs to
    // the program, and the kernel should not work on top of it.
    let bytes = unsafe { core::slice::from_raw_parts(ponteiro as *const u8, tamanho as usize) };
    let Ok(caminho) = core::str::from_utf8(bytes).map(String::from) else {
        return ERRO_NAO_EXISTE; // no file has a name that is not UTF-8
    };
    // Open OUTSIDE the process table lock: generating /sis/processos
    // queries the table (escalonador::listar).
    let Some(arquivo) = crate::vfs::abrir(&caminho) else {
        return ERRO_NAO_EXISTE;
    };
    escalonador::com_arquivos(|d| d.guardar(arquivo)).unwrap_or(ERRO_ABERTOS_DEMAIS)
    // (if the table was full, `guardar` hands the file back, and it dies here)
}
```

In order:

1. **Size before anything else.** Before looking at memory, refuse absurd
   paths. 255 bytes is the limit for a **name** (one piece of the path) on
   Linux, and 4096 the limit for the whole path (`getconf NAME_MAX /` and
   `getconf PATH_MAX /`, on the development machine). Here, with no
   subdirectories, the path is practically just the name.
2. **Does the pointer belong to the program?** `faixa_do_programa` (doc 10,
   redone in doc 13): every page must be mapped with the USUARIO bit in a
   PML4 entry of the program's. Reading the path from a pointer into the
   kernel would be a way for the program to peek at kernel memory.
3. **Copy.** The path is copied into a kernel `String` before being used.
   Today, with a single CPU, this is prudence: nothing can change the
   program's memory during the syscall. With several CPUs, another *thread*
   of the same program could swap the text mid-way, between the check and
   the use (the attack is called TOCTOU, *time of check to time of use*).
   Copying once and working only on the copy closes that door.
4. **Open outside the lock.** `vfs::abrir("/sis/processos")` calls
   `escalonador::listar`, which takes the process table lock. If `abrir` ran
   inside `com_arquivos`, the lock would already be held: the hang
   described in the previous section. So the file is opened first and only
   then, with the lock, stored in the table.

The path arrives as a pointer **and length**, the format of a Rust `&str`.
Linux's `open` takes a 0-terminated string (the C format, doc 16), and the
kernel must look for the 0 byte by byte, checking every page along the way.
With the length, it is a single check.

### 5.3 `ler`, and the page that can only be read

```rust
fn ler(descritor: u64, destino: u64, tamanho: u64) -> u64 {
    escalonador::com_arquivos(|d| {
        let Some(arquivo) = d.arquivo(descritor) else { return ERRO_DESCRITOR };
        // Only what will actually be copied needs checking (and so a huge
        // `tamanho` does not make the check walk millions of pages).
        let n = (tamanho as usize).min(arquivo.restante());
        if !faixa_gravavel_do_programa(destino, n as u64) {
            return ERRO_ENDERECO;
        }
        let destino = unsafe { core::slice::from_raw_parts_mut(destino as *mut u8, n) };
        arquivo.ler(destino) as u64
    })
}
```

Here the kernel does something new: it **writes** into the program's memory.
Up to doc 16, syscalls only read from it (`escrever` reads the text to
show). And writing requires an extra check that `faixa_do_programa` did not
do: the page must be **writable**.

Why, if the kernel can do anything? Because it cannot. Doc 06 left the
**WP** (*write protect*) bit of CR0 on: with it, not even ring 0 writes to a
page marked read-only. That protects the kernel from its own mistakes, but
it also means that a program passing as destination the address of its own
code (R-X pages, doc 13) would make the **kernel** take a page fault. A page
fault in ring 0 is not the program's fault (doc 10's `falha_do_programa`
only ends the program if the exception came from ring 3): it is a kernel
panic.

This was tested. With the check temporarily swapped for the old one (not
requiring write), the `conferir` program (section 6.3) brought the machine
down:

```
silicon> rodar conferir
  abrir um arquivo que nao existe                -2 ok
  ...
  fechar o descritor 0 (reservado)               -9 ok

!!! KERNEL PANIC !!!
Local: src/idt.rs:197:5
Mensagem: EXCECAO #14 PAGE FAULT ao escrever em 0x200000000015 (pagina presente, sem permissao, modo kernel)
RIP=0x5fcaca5 CS=0x8 RFLAGS=0x6 RSP=0xffff900000004fc8 SS=0x10
```

("PAGE FAULT writing to 0x200000000015 (page present, no permission, kernel
mode)".) `0x2000_0000_0015` is an address inside `conferir`'s code (which
starts at `0x2000_0000_0000`, doc 13). One bad pointer from **one program**
and the **whole system** went down. With the right check, the kernel returns
-14 and the program carries on.

The new check reuses the old one:

```rust
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    faixa_com_flags(inicio, tamanho, flags::USUARIO)
}

/// Like `faixa_do_programa`, but also requires that the program can WRITE
/// to every page (doc 17): it is where the kernel will copy data to. With
/// CR0.WP on (doc 06), the kernel writing to a read-only page takes a page
/// fault, even in ring 0.
pub fn faixa_gravavel_do_programa(inicio: u64, tamanho: u64) -> bool {
    faixa_com_flags(inicio, tamanho, flags::USUARIO | flags::ESCRITA)
}

/// Is every page in [inicio, inicio + tamanho) the program's, with all the
/// bits of `exigidas` set?
fn faixa_com_flags(inicio: u64, tamanho: u64, exigidas: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    let mut pagina = inicio & !(TAMANHO_QUADRO - 1);
    while pagina < fim {
        let do_programa = EspacoEnderecos::do_programa(pagina)
            && paginacao::traduzir(pagina).is_some_and(|t| t.flags & exigidas == exigidas);
```

The test changed from "`flags & USUARIO != 0`" to "`flags & exigidas ==
exigidas`": all requested bits set, not just some. (`traduzir` returns the
flags of the final entry; the program's intermediate tables are always
created with ESCRITA and with USUARIO (`paginacao.rs`, doc 10), so the final
entry is what decides.)

Another detail of `ler`: the checked size is `n`, what **will be copied**,
not what the program asked for. Asking for 1 GiB from a 420-byte file checks
one page, not 262,144. And at the end of the file (`n = 0`), there is nothing
to check: it returns 0, like Linux.

Finally, the copy happens inside `com_arquivos`, with the lock held and
interrupts off. It is safe because the copy is short (at most the whole
file, 420 bytes today) and because, after the check, it cannot fail.

### 5.4 `fechar`

```rust
fn fechar(descritor: u64) -> u64 {
    if escalonador::com_arquivos(|d| d.fechar(descritor)) { 0 } else { ERRO_DESCRITOR }
}
```

And what about those who do not close? When the process ends and is reaped,
the whole `Processo` is destroyed, and with it the descriptor table and every
file still in it (the `Drop`,
[§38](rust-basico.md#38-drop-code-that-runs-when-the-value-dies)). A buggy
program that forgets to close does not leak kernel memory forever: only
while it exists. Section 8.3 checks this.

## 6. The program side

### 6.1 `sos::Arquivo`

In the `sos` library, the syscalls get two layers. The raw one, which just
passes numbers:

```rust
pub mod cru {
    use super::*;

    /// Opens the path [ponteiro, ponteiro + tamanho). Descriptor or negative error.
    pub fn abrir(ponteiro: u64, tamanho: u64) -> i64 {
        chamada(SYS_ABRIR, ponteiro, tamanho) as i64
    }

    /// Reads up to `tamanho` bytes into `destino`. How many it read (0 = end) or error.
    pub fn ler(descritor: u64, destino: u64, tamanho: u64) -> i64 {
        chamada3(SYS_LER, descritor, destino, tamanho) as i64
    }

    /// Closes the descriptor. 0 or error.
    pub fn fechar(descritor: u64) -> i64 {
        chamada(SYS_FECHAR, descritor, 0) as i64
    }
}
```

(`chamada3` is doc 14's `chamada` with one more `in("rdx") c`; the
two-argument one now just calls it with `c = 0`.)

And the one a normal program uses:

```rust
/// An open file. Closes itself when it goes out of scope (`Drop`), like a
/// Rust `File` or a Java try-with-resources.
pub struct Arquivo {
    descritor: u64,
}

impl Arquivo {
    /// Opens the file `caminho` ("/leiame.txt", "/sis/tempo"). The error is
    /// the kernel's negative code (see `nome_do_erro`).
    pub fn abrir(caminho: &str) -> Result<Arquivo, i64> {
        let r = cru::abrir(caminho.as_ptr() as u64, caminho.len() as u64);
        if r < 0 { Err(r) } else { Ok(Arquivo { descritor: r as u64 }) }
    }

    /// Reads the next bytes into `destino`. `Ok(0)` = end of file.
    pub fn ler(&mut self, destino: &mut [u8]) -> Result<usize, i64> {
        let r = cru::ler(self.descritor, destino.as_mut_ptr() as u64, destino.len() as u64);
        if r < 0 { Err(r) } else { Ok(r as usize) }
    }
```
```rust
impl Drop for Arquivo {
    fn drop(&mut self) {
        cru::fechar(self.descritor);
    }
}
```

Two improvements over the raw numbers. The error becomes a `Result`
([§6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)): the
caller is forced to handle it, and `?` works (section 6.2). And the file
**closes itself** when the variable goes out of scope, through `Drop`
([§38](rust-basico.md#38-drop-code-that-runs-when-the-value-dies)): it is
the pattern Rust calls RAII and that Java imitates with try-with-resources
and Python with `with open(...)`. Forgetting to close becomes impossible,
even when leaving the function mid-way through a `?`.

Error codes get readable names:

```rust
pub fn nome_do_erro(codigo: i64) -> &'static str {
    match codigo {
        -2 => "nao existe",
        -9 => "descritor nao esta aberto",
        -14 => "endereco invalido",
        -24 => "arquivos abertos demais",
        -36 => "caminho grande demais",
        -38 => "chamada desconhecida",
        _ => "erro desconhecido",
    }
}
```

("does not exist", "descriptor is not open", "invalid address", "too many
open files", "path too long", "unknown call", "unknown error".)

### 6.2 `cat`

```rust
fn main() -> u64 {
    if sos::quantos_argumentos() < 2 {
        println!("uso: cat <arquivo>...");
        return 2;
    }
    let mut falhou = false;
    for caminho in sos::argumentos().skip(1) {
        if let Err(e) = mostrar(caminho) {
            println!("cat: {}: {}", caminho, sos::nome_do_erro(e));
            falhou = true;
        }
    }
    falhou as u64
}

/// Reads the file in 64-byte pieces and writes each piece to the screen.
/// The file is closed at the end of the function, by `Arquivo`'s `Drop`.
fn mostrar(caminho: &str) -> Result<(), i64> {
    let mut arquivo = Arquivo::abrir(caminho)?;
    let mut pedaco = [0u8; 64];
    loop {
        let n = arquivo.ler(&mut pedaco)?;
        if n == 0 {
            return Ok(()); // end of file
        }
        sos::escrever(&pedaco[..n]);
    }
}
```

The real `cat` has this same shape: for each argument (doc 16), open, read
until you get 0, write, close. Reading in 64-byte pieces, and not the whole
file at once, is on purpose: the program does not know the file's size, and
a fixed piece on the stack works for any size. `leiame.txt` comes in 7 reads
(6 of 64 bytes and one of 36) plus one that returns 0.

`falhou as u64` turns the `bool` (`falhou` = failed) into 0 or 1: the exit
code is 1 if any file failed, like Linux's `cat`.

### 6.3 `conferir`: an ill-intentioned program

A kernel cannot trust anything that comes from ring 3. To check that ours
does not, the `conferir` program does on purpose everything a buggy (or
malicious) program would do, through the raw functions, and compares the
answer with the expected one:

```rust
/// A test: what was tried, the kernel's answer and the expected one.
fn prova(falhas: &mut u64, descricao: &str, obtido: i64, esperado: i64) {
    let ok = obtido == esperado;
    println!("  {:<44} {:>4} {}", descricao, obtido, if ok { "ok" } else { "FALHOU" });
    if !ok {
        *falhas += 1;
    }
}
```

The most interesting tests:

```rust
    prova(f, "abrir com o caminho na memoria do kernel", cru::abrir(0xFFFF_8000_0000_0000, 8), -14);
    let longo = [b'a'; 300];
    prova(f, "abrir com um caminho de 300 bytes", cru::abrir(longo.as_ptr() as u64, 300), -36);
```
```rust
    // Bad destinations for `ler`: the kernel would have to WRITE there.
    let mut a = Arquivo::abrir("/leiame.txt").unwrap();
    let d = a.descritor();
    let codigo = main as *const () as u64; // a code page: read-only
    prova(f, "ler para dentro do proprio codigo", cru::ler(d, codigo, 16), -14);
    prova(f, "ler para dentro do kernel", cru::ler(d, 0xFFFF_8000_0000_0000, 16), -14);
    let mut buf = [0u8; 16];
    prova(f, "ler 16 bytes para a pilha (certo)", a.ler(&mut buf).unwrap_or(0) as i64, 16);
    prova(f, "...e as leituras recusadas nao avancaram", buf.starts_with(b"Este arquivo mor") as i64, 1);
```

(The descriptions: "open with the path in kernel memory", "open with a
300-byte path", "read into its own code", "read into the kernel", "read 16
bytes onto the stack (correct)", "...and the refused reads did not
advance".) `main as *const () as u64` is the address of the `main` function
itself: a place the program can read but not write. The last test checks a
fine detail: the two refused reads did not "use up" bytes of the file. The
correct read that follows still starts at the first byte ("Este arquivo
mor...", "This file lives...").

```rust
    // Close twice.
    prova(f, "fechar o descritor", cru::fechar(d), 0);
    prova(f, "fechar o mesmo descritor de novo", cru::fechar(d), -9);
    core::mem::forget(a); // already closed by hand: Drop must not close again
```

`mem::forget(a)` "forgets" the `Arquivo` without running `Drop`
([§47](rust-basico.md#47-dyn-trait-in-a-static-arrays-of-option-take-and-forget)).
Without it, `Drop` would close descriptor `d` a third time. Here that would
be harmless (the kernel would say -9), but in a real program it would be a
serious bug: if another file had been opened in the meantime, it would have
received the same number (the lowest free), and the late `Drop` would close
the **wrong** file.

Finally, filling the table, and exiting without closing:

```rust
    // Fill the table (16 files per process).
    let mut abertos = 0;
    let mut ultimo = 0;
    for _ in 0..20 {
        ultimo = cru::abrir("/sis/tempo".as_ptr() as u64, 10);
        if ultimo < 0 { break; }
        abertos += 1;
    }
    prova(f, "quantos arquivos abrem ao mesmo tempo", abertos, 16);
    prova(f, "o 17o e recusado", ultimo, -24);

    // The 16 stay open on purpose: the kernel must free them when the
    // process is reaped (the kernel test checks the heap).
```

The exit code is the number of tests that failed: 0 means the kernel
defended itself against all of them.

## 7. The shell: `ls` through the VFS

Doc 13's `ls` read the tar directly. Now it asks the VFS and accepts a
directory:

```rust
fn ls(argumentos: &[&str]) {
    let diretorio = argumentos.first().copied().unwrap_or("/");
    let Some(itens) = crate::vfs::listar(diretorio) else {
        kprintln!("'{}' nao e um diretorio (veja 'ls /')", diretorio);
        return;
    };
    for (nome, tamanho) in &itens {
        if nome.ends_with('/') {
            kprintln!("  {:<12}{:>8}        diretorio", nome, "");
            continue;
        }
        // The first bytes tell the type: only the disk's can be ELF.
        let caminho = alloc::format!("{}/{}", diretorio.trim_end_matches('/'), nome);
        let mut inicio = [0u8; 4];
        let elf = crate::vfs::abrir(&caminho).is_some_and(|mut a| a.ler(&mut inicio) == 4 && &inicio == b"\x7fELF");
        kprintln!("  {:<12}{:>8} bytes  {}", nome, tamanho, if elf { "executavel ELF" } else { "" });
    }
    kprintln!("{} itens em {}", itens.len(), diretorio);
}
```

The shell runs in the kernel and uses the VFS directly, with no descriptors
and no syscall: `vfs::abrir` returns the `ArquivoAberto`, which dies at the
end of the expression. To know whether a file is executable, `ls` reads the
first 4 bytes and looks for the ELF signature (doc 13), instead of trusting
the name.

`rodar` stays as it was: it looks the program up directly in the tar, by
name (`rodar primos`). It needs the ELF bytes as a `&'static [u8]`, which is
exactly what the tar provides.

## 8. How it was tested

### 8.1 From the shell

Real serial output (keys sent through the QEMU monitor):

```
silicon> ls
  args           25816 bytes  executavel ELF
  cat            26240 bytes  executavel ELF
  conferir       26624 bytes  executavel ELF
  dados          25480 bytes  executavel ELF
  insone         25392 bytes  executavel ELF
  leiame.txt       420 bytes  
  medir          25248 bytes  executavel ELF
  oi             25240 bytes  executavel ELF
  primos         25688 bytes  executavel ELF
  soneca         25440 bytes  executavel ELF
  sis/                        diretorio
11 itens em /
silicon> ls /sis
  tempo             30 bytes  
  mem               46 bytes  
  processos         31 bytes  
3 itens em /sis
silicon> rodar cat /leiame.txt
Este arquivo mora no disco em RAM do SiliconOS.

Ele nao e um programa: e texto puro, empacotado no tar pelo build.rs junto
com os executaveis (docs/17-vfs-e-arquivos.md). Um programa o le com as
chamadas de sistema abrir, ler e fechar, por exemplo:

    rodar cat /leiame.txt

Experimente tambem os arquivos de /sis, que o kernel escreve na hora em
que alguem os abre:

    rodar cat /sis/tempo /sis/mem /sis/processos
'cat' terminou: codigo de saida 0
```

(The file says, in Portuguese: it lives on the RAM disk, it is plain text
packed by `build.rs`, a program reads it with the `abrir`, `ler` and
`fechar` system calls, and suggests trying the `/sis` files.)

`/sis`, read by a program:

```
silicon> rodar cat /sis/tempo /sis/mem /sis/processos
10670 ms (1067 tiques a 100 Hz)
quadros livres: 20201
heap em uso: 22592 bytes
PID NOME
  0 kernel
  1 ocioso
  3 cat
'cat' terminou: codigo de saida 0
```

`cat` shows up in the process list it is itself reading: the text was
generated at `abrir` time, while it was running. (Pid 2 was the previous
`cat`, which has already ended.)

The errors, and a path without the slash:

```
silicon> rodar cat /nada /sis
cat: /nada: nao existe
cat: /sis: nao existe
'cat' terminou: codigo de saida 1
silicon> rodar cat sis/tempo
17740 ms (1774 tiques a 100 Hz)
'cat' terminou: codigo de saida 0
silicon> ls /oi
'/oi' nao e um diretorio (veja 'ls /')
```

### 8.2 The kernel defends itself

```
silicon> rodar conferir
  abrir um arquivo que nao existe                -2 ok
  abrir um diretorio (/sis)                      -2 ok
  abrir com o caminho na memoria do kernel      -14 ok
  abrir com um caminho de 300 bytes             -36 ok
  ler do descritor 99                            -9 ok
  ler do descritor 1 (reservado)                 -9 ok
  fechar o descritor 0 (reservado)               -9 ok
  ler para dentro do proprio codigo             -14 ok
  ler para dentro do kernel                     -14 ok
  ler 16 bytes para a pilha (certo)              16 ok
  ...e as leituras recusadas nao avancaram        1 ok
  fechar o descritor                              0 ok
  fechar o mesmo descritor de novo               -9 ok
  quantos arquivos abrem ao mesmo tempo          16 ok
  o 17o e recusado                              -24 ok
conferir: 0 falhas (16 arquivos ficam abertos de proposito)
'conferir' terminou: codigo de saida 0
```

And section 5.3 showed what happens with the write check turned off: a
kernel panic on the eighth test.

### 8.3 `cargo test`

Six new tests (63 in total, all passing):

```
siliconos::vfs::testes::abrir_e_ler_aos_pedacos ... [ok]
siliconos::vfs::testes::cat_le_do_disco_e_de_sis ... [ok]
siliconos::vfs::testes::descritores_reaproveitam_o_menor_livre ... [ok]
siliconos::vfs::testes::kernel_se_defende_e_libera_o_que_ficou_aberto ... [ok]
siliconos::vfs::testes::montagens_escolhidas_pelo_caminho ... [ok]
siliconos::vfs::testes::raiz_mostra_a_montagem_sis ... [ok]
Todos os 63 testes passaram.
```

Choosing the mount, including the `/sistema` case:

```rust
    #[test_case]
    fn montagens_escolhidas_pelo_caminho() {
        let nome = |c| procurar(c).map(|(m, r)| (m.caminho, r));
        assert_eq!(nome("/sis/tempo"), Some(("/sis", "tempo")));
        assert_eq!(nome("/sis"), Some(("/sis", "")));
        assert_eq!(nome("/sistema"), Some(("/", "sistema")));
        assert_eq!(nome("/leiame.txt"), Some(("/", "leiame.txt")));
        assert_eq!(nome("leiame.txt"), Some(("/", "leiame.txt")));
        assert_eq!(nome("/"), Some(("/", "")));
    }
```

The descriptor table, without any process:

```rust
    #[test_case]
    fn descritores_reaproveitam_o_menor_livre() {
        let mut d = Descritores::novo();
        let a = d.guardar(abrir("/leiame.txt").unwrap()).ok().unwrap();
        let b = d.guardar(abrir("/leiame.txt").unwrap()).ok().unwrap();
        assert_eq!((a, b), (3, 4));
        assert!(d.fechar(3));
        assert!(!d.fechar(3));                     // already closed
        assert!(!d.fechar(0) && !d.fechar(99));    // never existed
        assert_eq!(d.guardar(abrir("/leiame.txt").unwrap()).ok(), Some(3));
        for _ in 2..MAX_ABERTOS {
            assert!(d.guardar(abrir("/leiame.txt").unwrap()).is_ok());
        }
        assert!(d.guardar(abrir("/leiame.txt").unwrap()).is_err()); // full
        assert_eq!(d.quantos(), MAX_ABERTOS);
    }
```

And what a program leaves open goes back to the heap:

```rust
    #[test_case]
    fn kernel_se_defende_e_libera_o_que_ficou_aberto() {
        // `conferir` ends with 16 files open (generated, on the heap).
        let (livres, heap_antes) = (quadros::livres(), heap::estatisticas().em_uso);
        assert_eq!(rodar_com(&["conferir"]), Resultado::Saiu(0));
        assert_eq!(quadros::livres(), livres);
        assert_eq!(heap::estatisticas().em_uso, heap_antes);
    }
```

The 16 files `conferir` leaves open are `/sis/tempo` ones, each with a `Vec`
of generated text on the kernel heap. If the table did not die with the
process, `em_uso` (in use) would end higher than at the start.

### 8.4 Regression

The 25 builds (the normal one and the 24 test and diagnostic features)
behave as before.

---

## What is still missing

- **Really reading in pieces.** Every open file is entirely in memory
  (section 3.1). A real disk would require `SistemaDeArquivos` to have a
  `ler(file, position, destination)` that fetched only the requested piece,
  and `ArquivoAberto` would keep a reference to the file, not the bytes.
- **Writing.** There is no `write(descriptor)` and no file creation: the tar
  is read-only (it is inside the kernel executable). A writable file system
  in RAM (Linux's `tmpfs`) would be the natural next step.
- **Real directories.** Each file system here is a flat list; there is no
  `/a/b/c`, no current directory, no `..`.
- **0, 1 and 2.** Standard output as a descriptor: `write(1, ...)` instead of
  doc 10's `escrever` syscall, and a keyboard read with `ler(0, ...)`. That
  is what links this chapter to doc 15's terminal idea.
- **SMAP.** With the SMAP protection on (off so far, doc 10), the kernel
  could neither read nor write program pages without asking permission with
  the `stac`/`clac` instructions. It is a second lock, from the CPU, for the
  mistake `faixa_gravavel_do_programa` prevents in software.

---

## Changes to the files

What this chapter adds to the doc 16 code (tag `doc-16`). Each file's link
goes to its state at the end of this chapter (tag `doc-17`); every changed
line is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-16...doc-17).

| File | What changes | Where |
|---|---|---|
| [`src/vfs/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | **new** (section 3): `SistemaDeArquivos`, `Conteudo`, `MONTAGENS`, `procurar`, `abrir`, `listar`, `pai`, `ArquivoAberto`, `Descritores`; 6 tests | — |
| [`src/vfs/disco.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/disco.rs), [`src/vfs/sis.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/sis.rs) | **new** (section 4) | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/main.rs) | `mod vfs;` | module list |
| [`src/escalonador.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/escalonador.rs) | `arquivos` field in `Processo` (in the 3 places that create one); `com_arquivos` (section 5.1) | `use`; `Processo`; `init`; `criar`; before `listar` |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | `SYS_ABRIR`, `SYS_LER`, `SYS_FECHAR`, 4 new errors, `MAX_CAMINHO`; `tratar_syscall`; `faixa_gravavel_do_programa`, `faixa_com_flags`; `abrir`, `ler`, `fechar` (section 5) | constants; `tratar_syscall`; `faixa_do_programa`; before `escrever` |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/shell.rs) | `ls [directory]` through the VFS (section 7) | `COMANDOS`; `ls` |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/ramdisk.rs) | `tamanho_total` goes away (`ls` no longer uses it); test: 10 names, the `.txt` is not ELF | end; tests |
| [`build.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/build.rs) | also packs `programas/arquivos/` (section 4.3) | `rerun-if-changed`; building the tar |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | `chamada3`; `SYS_ABRIR`/`LER`/`FECHAR`; `cru`, `nome_do_erro`, `Arquivo` and its `Drop` (section 6.1) | constants; `chamada`; before `Console` |
| [`programas/src/bin/cat.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/cat.rs), [`conferir.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/conferir.rs), [`programas/arquivos/leiame.txt`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/arquivos/leiame.txt) | **new** (sections 6.2, 6.3, 4.3) | — |

### The snippets the text did not show

The beginning of `src/vfs/mod.rs`:

```rust
pub mod disco;
pub mod sis;

use alloc::string::String;
use alloc::vec::Vec;
```

The rest of `Descritores`:

```rust
    /// How many files are open.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn quantos(&self) -> usize {
        self.abertos.iter().filter(|a| a.is_some()).count()
    }
}
```

The `use`s in `disco.rs` and `sis.rs`:

```rust
use alloc::string::String;
use alloc::vec::Vec;

use super::{Conteudo, SistemaDeArquivos};
use crate::ramdisk;
```
```rust
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::{Conteudo, SistemaDeArquivos};
use crate::escalonador;
use crate::interrupcoes::timer;
use crate::memoria::{heap, quadros};
```

In the scheduler, the `use` and the new field in the three `Processo`s
created (`init` creates 0 and 1, `criar` the others):

```rust
use crate::vfs::Descritores;
```
```rust
            arquivos: Descritores::novo(),
```

`sos`'s `chamada3` and the two-argument `chamada`, which now uses it:

```rust
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    chamada3(numero, a, b, 0)
}

/// The same, with a third argument in RDX (doc 17: `ler` needs three).
#[inline(always)]
fn chamada3(numero: u64, a: u64, b: u64, c: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b, in("rdx") c,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}
```

The rest of `Arquivo` and the `sos` constants:

```rust
    /// The number the kernel gave this file.
    pub fn descritor(&self) -> u64 {
        self.descritor
    }
}
```
```rust
const SYS_ABRIR: u64 = 5;
const SYS_LER: u64 = 6;
const SYS_FECHAR: u64 = 7;
```

The beginning of `conferir` (the path and descriptor tests the text did not
show) and its end:

```rust
fn main() -> u64 {
    let mut falhas = 0;
    let f = &mut falhas;

    // Bad paths.
    let erro = |r: Result<Arquivo, i64>| r.err().unwrap_or(0);
    prova(f, "abrir um arquivo que nao existe", erro(Arquivo::abrir("/nao-existe")), -2);
    prova(f, "abrir um diretorio (/sis)", erro(Arquivo::abrir("/sis")), -2);
```
```rust
    // Descriptors that are not open.
    prova(f, "ler do descritor 99", cru::ler(99, longo.as_ptr() as u64, 1), -9);
    prova(f, "ler do descritor 1 (reservado)", cru::ler(1, longo.as_ptr() as u64, 1), -9);
    prova(f, "fechar o descritor 0 (reservado)", cru::fechar(0), -9);
```
```rust
    println!("conferir: {} falhas (16 arquivos ficam abertos de proposito)", falhas);
    falhas
}
```

The tests the text did not show, and their helper:

```rust
    /// Runs a program from the disk with these arguments.
    fn rodar_com(argv: &[&str]) -> Resultado {
        let a = crate::ramdisk::abrir(argv[0]).unwrap();
        escalonador::esperar(usuario::iniciar_elf(a.nome, a.dados, argv, false).unwrap())
    }

    #[test_case]
    fn cat_le_do_disco_e_de_sis() {
        assert_eq!(rodar_com(&["cat", "/leiame.txt"]), Resultado::Saiu(0));
        assert_eq!(rodar_com(&["cat", "/sis/tempo", "/sis/processos"]), Resultado::Saiu(0));
        assert_eq!(rodar_com(&["cat", "/sis/mem", "/nao-existe"]), Resultado::Saiu(1));
        assert_eq!(rodar_com(&["cat"]), Resultado::Saiu(2));
    }
```
```rust
    #[test_case]
    fn abrir_e_ler_aos_pedacos() {
        let mut a = abrir("/leiame.txt").unwrap();
        let total = a.restante();
        assert!(total > 100);
        let mut pedaco = [0u8; 7];
        let mut lidos = 0;
        loop {
            let n = a.ler(&mut pedaco);
            if n == 0 { break; }
            lidos += n;
        }
        assert_eq!(lidos, total);
        assert_eq!(a.ler(&mut pedaco), 0); // stays at the end
        assert!(abrir("/nao-existe").is_none());
        assert!(abrir("/sis").is_none());           // directory
        assert!(abrir("/sis/tempo").unwrap().restante() > 0);
    }

    #[test_case]
    fn raiz_mostra_a_montagem_sis() {
        let raiz = listar("/").unwrap();
        assert!(raiz.iter().any(|(n, _)| n == "sis/"));
        assert!(raiz.iter().any(|(n, _)| n == "leiame.txt"));
        let sis = listar("/sis").unwrap();
        assert!(sis.iter().any(|(n, _)| n == "tempo"));
        assert!(listar("/leiame.txt").is_none());
    }
```

The `use`s of the tests in `vfs/mod.rs`:

```rust
    use super::*;
    use crate::escalonador;
    use crate::memoria::{heap, quadros};
    use crate::usuario::{self, Resultado};
```

The disk test in `ramdisk.rs`:

```rust
        assert_eq!(nomes, ["args", "cat", "conferir", "dados", "insone", "leiame.txt", "medir", "oi", "primos", "soneca"]);
        // All are executables, except the text (doc 17).
        assert!(arquivos().all(|a| a.dados.starts_with(b"\x7fELF") == !a.nome.ends_with(".txt")));
```

In `build.rs`, the new `rerun-if-changed` line and the loop that builds the
tar (which now walks `conteudo` instead of reading each binary):

```rust
    println!("cargo:rerun-if-changed=programas/arquivos");
```
```rust
    let mut tar = Vec::new();
    for (nome, dados) in &conteudo {
        acrescentar_ao_tar(&mut tar, nome, dados);
    }
```

And the `ls` line in the command table:

```rust
    Comando { nome: "ls", uso: "ls [diretorio]", descricao: "lista os arquivos (/ = disco em RAM, /sis = do kernel)", funcao: ls },
```

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `SistemaDeArquivos::abrir` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | (trait) Opens a file by its name inside the mount and returns the content. |
| `SistemaDeArquivos::listar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | (trait) The mount's files, with their sizes. |
| `Conteudo::bytes` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | The content's bytes, fixed (from the tar) or generated. |
| `procurar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Finds the mount responsible for a path and the rest of the path after it. |
| `abrir` (VFS) | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Opens a file by its full path, passing it to the mount's file system. |
| `listar` (VFS) | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | A directory's contents, with the mounts inside it shown as subdirectories. |
| `pai` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | The directory that contains a path. |
| `ArquivoAberto::ler` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Copies the next bytes and advances the position; 0 at the end. |
| `ArquivoAberto::restante` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | How many bytes are left to read. |
| `Descritores::novo` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | An empty table of open files. |
| `Descritores::guardar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Stores the file under the lowest free descriptor (from 3), or hands it back if the table is full. |
| `Descritores::arquivo` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | The file of a descriptor, if open; accepts any number without crashing. |
| `Descritores::fechar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Closes a descriptor and frees the file. |
| `Descritores::quantos` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | How many files are open (used in the tests). |
| `Disco::abrir`, `Disco::listar` | [vfs/disco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/disco.rs) | The RAM disk (tar) as seen by the VFS. |
| `Sis::abrir`, `Sis::listar` | [vfs/sis.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/sis.rs) | The /sis files, generated on the spot by each one's function. |
| `tempo`, `mem`, `processos` | [vfs/sis.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/sis.rs) | Generate the text of /sis/tempo, /sis/mem and /sis/processos. |
| `com_arquivos` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/escalonador.rs) | Runs a closure with the current process's file table, with the lock held and interrupts off. |
| `faixa_gravavel_do_programa` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Like `faixa_do_programa`, but requires writable pages (the destination of a kernel copy). |
| `faixa_com_flags` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Is every page in the range the program's, with the required bits? |
| `abrir` (syscall) | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Syscall 5: validates and copies the path, opens through the VFS and returns the descriptor. |
| `ler` (syscall) | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Syscall 6: copies the file's next bytes to a writable destination in the program. |
| `fechar` (syscall) | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Syscall 7: closes the descriptor. |
| `chamada3` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | A syscall with three arguments (RDI, RSI, RDX). |
| `cru::abrir`, `cru::ler`, `cru::fechar` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | The file syscalls with raw numbers and pointers. |
| `nome_do_erro` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | The text of a kernel error code. |
| `Arquivo::abrir`, `Arquivo::ler`, `Arquivo::descritor` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | An open file, with errors in a `Result`. |
| `drop` (Arquivo) | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | Closes the file when it goes out of scope. |
| `main`, `mostrar` (cat) | [programas/src/bin/cat.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/cat.rs) | Shows each requested file, reading in 64-byte pieces. |
| `main`, `prova` (conferir) | [programas/src/bin/conferir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/conferir.rs) | Sends bad file requests and checks every answer from the kernel. |

Automated tests (`#[test_case]`, run with `cargo test`, see doc 09):

| Function | File | What it does |
|---|---|---|
| `rodar_com` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Helper: runs a program from the disk with an argv and waits for the result. |
| `montagens_escolhidas_pelo_caminho` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Test: each path lands in the right mount, including `/sistema` (which is not `/sis`'s). |
| `abrir_e_ler_aos_pedacos` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Test: reading 7 bytes at a time gives the whole file and then 0; a directory and a missing file do not open. |
| `raiz_mostra_a_montagem_sis` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Test: `listar("/")` includes `sis/` and `leiame.txt`; a file is not a directory. |
| `descritores_reaproveitam_o_menor_livre` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Test: 3 and 4, 3 comes back after being closed, invalid numbers refused, table full at 16. |
| `cat_le_do_disco_e_de_sis` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Test: `cat` exits with 0, 1 (missing file) or 2 (no arguments). |
| `kernel_se_defende_e_libera_o_que_ficou_aberto` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Test: `conferir` passes every test, and the 16 files it leaves open go back to the heap. |

---

## Differences from the book

The roadmap in the book's section 11.4 mentions a VFS between the RAM disk
and the program loader, without implementing it. The design here is ours,
with Unix's names and error numbers: mounts in a fixed table, a pseudo file
system (`/sis`) instead of a second disk, per-process descriptors starting at
3, and each file's whole content in memory.

## Summary

- The **VFS** separates "what the program asks for" (paths; open, read,
  close) from "who keeps the file". A **mount** table says which file system
  is responsible for each part of the tree.
- A file system is any type that implements the `SistemaDeArquivos` trait.
  There are two: the RAM disk at `/` and `/sis`, whose files the kernel
  generates on the spot, like Linux's `/proc`.
- Opening a file returns a **descriptor**, an index into the **process's**
  table of open files. Numbers start at 3 (0, 1 and 2 are reserved) and the
  lowest free one is reused. The table dies with the process.
- `ler` returns **how many** bytes it copied; 0 means end of file.
- Copying data **to** the program requires **writable** pages: with CR0.WP,
  a read-only destination would make the kernel take a page fault. Tested:
  without that check, one program's pointer brought the kernel down.
- `sos` wraps descriptors in an `Arquivo` that closes itself (`Drop`), and
  `conferir` shows the kernel withstands 15 bad requests.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **Why can `cat` show `/leiame.txt` and `/sis/processos` without a single
   line of code specific to each?**
   Because it only talks to the VFS, through paths and the same three
   syscalls. The file systems behind each mount are the ones that know one
   comes from the tar and the other is generated on the spot, and the VFS
   picks by the path.

2. **Why does the kernel return a number (the descriptor) and not a pointer
   to the `ArquivoAberto`?**
   `ArquivoAberto` lives on the kernel heap, which the program can neither
   read nor write. A number is just an index the kernel checks on every use,
   in a table that belongs to the process itself: a program cannot reach
   another's files or kernel memory by guessing numbers.

3. **In the mount table, why does `/sis` come before `/`? And why can't
   `/sistema` land in `/sis`?**
   `procurar` takes the first mount that fits, and `/` fits everything: if
   it came first, `/sis/tempo` would go to the tar. As for `/sistema`:
   removing "sis" from the start leaves "tema", which does not start with
   `/`; only an empty rest or one starting with `/` means the path is inside
   `/sis`.

4. **Why does `abrir` call `vfs::abrir` before `com_arquivos`, and not inside
   it?**
   Because opening `/sis/processos` calls `escalonador::listar`, which takes
   the process table lock, and `com_arquivos` runs with that same lock held.
   `spin::Mutex` is not reentrant: the process would wait for itself, with
   interrupts off, and the machine would hang.

5. **The program passes the address of its own `main` function as `ler`'s
   destination. Why would this bring the kernel down, if the address is the
   program's?**
   The code page is the program's, but read-only (R-X). With CR0.WP on, not
   even the kernel writes to such a page: the copy would cause a page fault
   in ring 0, which is treated as a kernel error (panic), not a program
   error. That is why `ler` requires `USUARIO | ESCRITA` on every page.

6. **A program opens 5 files and ends without closing any. What happens to
   them?**
   Nothing leaks. The descriptor table is a field of `Processo`; when the
   process is reaped it is destroyed, the table with it, and each
   `ArquivoAberto` (with the generated ones' `Vec`) goes back to the heap.
   The `kernel_se_defende_e_libera_o_que_ficou_aberto` test checks the heap
   before and after.

7. **In `conferir`, what is the `mem::forget(a)` after closing the descriptor
   by hand for?**
   So that `Arquivo`'s `Drop` does not close the same number again. If
   another file had been opened between the manual close and the end of the
   scope, it would have received the same descriptor (the lowest free), and
   the late `Drop` would close the wrong file.

</details>
