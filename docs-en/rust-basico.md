# Rust for people who already program (only what SiliconOS uses)

This guide does not teach all of Rust. It explains **only the features that
appear in SiliconOS's code**, comparing them with languages you already know
(Java, Kotlin, JavaScript/TypeScript, Python). The chapters point here
whenever a feature appears for the first time.

If something here seems abstract, skip it and come back when a chapter asks
for it.

---

## 1. Numeric types: the size is explicit

In Java you have `int` (32 bits) and `long` (64 bits). In Python, integers
have no size. In Rust, **every integer says how many bits it has and whether
it is signed**:

| Rust | Meaning | Rough equivalent |
|---|---|---|
| `u8` | unsigned, 8 bits (0 to 255) | unsigned `byte` |
| `u16` | unsigned, 16 bits | Java `char` (as a number) |
| `u32` | unsigned, 32 bits | unsigned `int` |
| `u64` | unsigned, 64 bits | unsigned `long` |
| `i32`, `i64` | signed | `int`, `long` |
| `usize` | unsigned, pointer-sized (64 bits here) | used for indices and sizes |

In a kernel this is essential: the hardware requires fields of an exact size
("this field is 16 bits"), so the types map one to one onto Intel's manual.

Numbers may contain `_` for readability, and base prefixes:

```rust
0x00FF_FFFF        // hexadecimal (the _ is only visual)
0b0000_1100        // binary
5 * 4096           // ordinary expressions
```

**Conversions are always explicit**, with `as`:

```rust
let x: u64 = 300;
let y = x as u16;   // like a Java cast: (short) x
let z = x as u8;    // 300 does not fit in 8 bits: becomes 44 (drops the high bits)
```

There is no automatic conversion between `u32` and `u64`, as there is from
`int` to `long` in Java. That seems annoying at first, but it prevents
subtle truncation bugs.

---

## 2. Variables: immutable by default

```rust
let a = 5;          // immutable: like Kotlin's `val` or Java's `final`
let mut b = 5;      // mutable: like Kotlin's `var`
b += 1;             // ok
// a += 1;          // compile ERROR
```

The type is usually inferred, as in Kotlin and TypeScript. When needed, it
goes after the name: `let x: u64 = 0;`.

---

## 3. Functions and the type `!` ("never returns")

```rust
fn soma(a: u32, b: u32) -> u32 {       // soma = sum
    a + b           // the last expression WITHOUT a semicolon is the return value
}
```

As in Kotlin, almost everything is an expression: a `{ ... }` block "is
worth" the value of its last line.

SiliconOS makes heavy use of a special type: **`!`**, called *never*.

```rust
fn kernel_main(...) -> ! {
    ...
    loop { ... }    // infinite loop: never exits
}
```

`-> !` means **"this function never returns"**. It is not `void` (which
returns without a value); it is a promise, checked by the compiler, that the
function ends in an infinite loop, stops the machine or calls another `-> !`
function. In a kernel this shows up all the time: there is nowhere "to go
back to" once the kernel has started.

---

## 4. Structs and `impl`: classes without inheritance

Rust has no classes. It has **structs** (data) and **`impl`** blocks
(methods for that data):

```rust
pub struct Console {        // like a Java class with only fields
    col: usize,
    row: usize,
    pub cor: u32,           // pub = public; without pub = private to the module
}

impl Console {
    pub fn new(...) -> Self { ... }        // the "constructor" is just a naming convention
    fn cols(&self) -> usize { ... }        // method that only reads the object
    pub fn write_byte(&mut self, b: u8) { ... }  // method that changes the object
}
```

- **`Self`** = the type itself (here, `Console`).
- **`&self`** = like Java's `this`, but read-only.
- **`&mut self`** = `this` with permission to change fields.
- A function inside `impl` without `self` is like a `static` method: called
  with `Console::new(...)`.

---

## 5. Traits: interfaces

A **trait** is like a Java/Kotlin `interface`. SiliconOS implements the
standard library trait `core::fmt::Write`, which requires a single method:

```rust
impl fmt::Write for Serial {           // "Serial implements Write"
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() { ... }
        Ok(())
    }
}
```

Just as Java interfaces can have `default` methods, the `Write` trait
brings the `write_fmt` method for free, which formats text (`"{} + {}"`)
and calls our `write_str`. **We implement 1 method and get all of the
formatting.** That is why `writeln!(Serial, "x = {}", x)` works.

`impl Trait` in a parameter means "any type that implements this trait",
like a bounded generic (`<T extends MemoryMap>` in Java):

```rust
fn kernel_main(fb: FrameBuffer, memory_map: &impl MemoryMap) -> ! { ... }
```

---

## 6. `Option` and `Result`: no `null` and no exceptions

Rust **has no `null`** and no exceptions (`try/catch`). Instead:

- **`Option<T>`** is `Some(value)` or `None`. It is Java's `Optional<T>` or
  Kotlin's `T?`.
- **`Result<T, E>`** is `Ok(value)` or `Err(error)`. It is a "return value
  that may be an error", which you are forced to examine.

```rust
static CONSOLE: Mutex<Option<Console>> = ...;   // may not exist yet

if let Some(con) = CONSOLE.lock().as_mut() {    // "if it exists, call it con"
    con.write_fmt(args);
}
```

`if let Some(x) = value` is like Kotlin's `value?.let { x -> ... }`.

**`.unwrap()`** means "I guarantee it is `Some`/`Ok`; if it is not, abort".
If it fails, it produces a **panic** (chapter 02). We use `unwrap()` in the
boot phase, where there is nothing to do if the firmware fails.

`let _ = something();` means "I am ignoring the result on purpose". The
compiler warns if you ignore a `Result` without saying so.

---

## 7. `match`: a `switch` that forces you to cover everything

```rust
let origem = match (codigo >> 1) & 0b11 {
    0b00 => "GDT",
    0b10 => "LDT",
    _    => "IDT",       // _ = "any other value" (the default)
};
```

Similar to Kotlin's `when`. `match` is an expression (it returns a value),
and the compiler **requires** that every possible case is covered.

---

## 8. References and the "borrow checker" (very short version)

In Java, objects are always accessed by reference and the garbage collector
takes care of memory. Rust **has no garbage collector**. Instead, the
compiler keeps track of who "owns" each value and who has borrowed it:

- `&x`: a **read-only** borrow (there can be several at once);
- `&mut x`: a borrow **with write access** (only one at a time, and no read
  borrow alongside it).

This rule ("either many readers or one writer") is checked **at compile
time** and eliminates a whole class of bugs (two pieces of code changing the
same data at the same time). That is why SiliconOS uses a `Mutex` for the
console's global variable (section 11).

When a value goes out of scope, it is destroyed automatically (the
`drop`). It is like Java's `try-with-resources`, but for everything.
SiliconOS relies on this, for example, to release the console lock by
itself.

---

## 9. Raw pointers and `unsafe`

References (`&T`) are always valid: the compiler guarantees it. But a kernel
needs to do things the compiler **has no way to check**, such as "write to
address `0x80000000`, which I know is the screen". For that there are **raw
pointers**:

```rust
*mut u32     // pointer to u32, writable (like `uint32_t*` in C)
*const u64   // read-only pointer
```

Raw pointers can be created freely, but **can only be used inside an
`unsafe` block**:

```rust
unsafe {
    self.fb.base.add(y * self.fb.stride + x).write_volatile(cor);
}
```

`unsafe` **does not turn off** Rust's checks. It only allows a few extra
operations (using a raw pointer, calling an `unsafe` function, running
assembly, touching a `static mut`) and means: **"compiler, here I am the one
guaranteeing correctness"**. The good practice, followed in SiliconOS, is to
keep `unsafe` blocks small and commented, and to expose safe functions on
top. Example: `outb` is `unsafe`, but `Serial::write_byte` is not, so
whoever uses the serial port does not need to worry.

An **`unsafe fn`** is a function that can only be called inside
`unsafe { }`, because the caller must guarantee some condition.

---

## 10. `static` and `const`

```rust
const ESCALA: usize = 2;              // constant: substituted into the code, like #define
static CONSOLE: Mutex<...> = ...;     // global variable: a single address in memory
static mut PILHA: [u8; 20480] = ...;  // MUTABLE global: every use requires unsafe
```

In Java, `static` is a class field. In Rust, `static` is a **global variable
with a fixed address** that lives for the whole program. That matters to
the CPU, which keeps **addresses** of tables (GDT, IDT): they must stay in
the same place forever.

`static mut` is dangerous, because any part of the code could change it at
the same time. That is why Rust requires `unsafe` on every access and
recommends wrapping globals in a `Mutex` or `Once`.

**`'static`** (with an apostrophe) is a *lifetime* meaning "valid until the
end of the program". `&'static T` is a reference that never becomes
invalid.

---

## 11. `Mutex` and `Once` (from the `spin` crate)

- **`Mutex<T>`** protects shared data, like Java's `synchronized`.
  `CONSOLE.lock()` returns a "guard"; while it exists, nobody else can take
  the lock. When the guard goes out of scope, the lock is released by
  itself (section 8).
  - The standard library's `Mutex` puts the thread to *sleep* by asking the
    operating system, but **we are the operating system**. The `spin` crate
    offers a *spinlock*: whoever wants the lock loops, trying until it gets
    it.
- **`Once<T>`** holds a value that is computed **exactly once**, on the
  first call to `call_once`, like Kotlin's `lazy` or a singleton. It returns
  `&'static T`.

---

## 12. Modules, crates, Cargo and features

| Rust | Analogy |
|---|---|
| **crate** | a package/library (like a `.jar` or an npm package) |
| **Cargo** | the build and dependency manager (Maven/Gradle/npm combined) |
| `Cargo.toml` | the `pom.xml`/`package.json` |
| `mod serial;` | "there is a module in the file `serial.rs`" (Rust does not discover files by itself) |
| `use spin::Mutex;` | `import` |
| `crate::gdt` | path from the root of our project |
| `pub` | `public`; without `pub` it is private to the module |

**Features** are compilation options declared in `Cargo.toml`:

```toml
[features]
teste-pagefault = []
```

In the code, `#[cfg(feature = "teste-pagefault")]` makes the next block
**exist only** when the feature is turned on (`cargo build --features
teste-pagefault`). It is C's `#ifdef`. The test code does not even make it
into the normal binary.

---

## 13. Attributes: `#[...]` and `#![...]`

They are like Java *annotations* (`@Override`):

| Attribute | Effect |
|---|---|
| `#![no_std]` | (applies to the whole crate, because of the `!`) do not use the standard library |
| `#![no_main]` | do not use the default `main` |
| `#![feature(...)]` | turn on an experimental feature (only in *nightly* Rust) |
| `#[entry]` | (from the `uefi` crate) this function is the UEFI entry point |
| `#[panic_handler]` | this function handles panics |
| `#[repr(C)]` | lay out the fields in memory exactly in the order written, as in C |
| `#[repr(C, packed(2))]` | same, and without alignment gaps (*padding*) between fields |
| `#[repr(align(16))]` | the start of this type is at an address that is a multiple of 16 |
| `#[repr(transparent)]` | a struct with a single field has exactly the same memory representation as that field |
| `#[derive(Clone, Copy)]` | automatically generate copying of the value |
| `#[allow(dead_code)]` | do not warn that this is unused |

`#[repr(...)]` is crucial in a kernel. Without it, the Rust compiler may
**reorder the fields** of a struct to save memory. When the struct describes
something the **CPU** will read (an IDT entry, the TSS), the order and size
must match the manual bit for bit.

---

## 14. Macros: the `!` at the end of the name

`println!`, `panic!`, `writeln!`, `format_args!`, `asm!`: the `!` indicates
a **macro**, not a function. Macros are expanded into code at compile time.
That is how `println!("{} {}", a, b)` accepts any number of arguments and
checks the format string during compilation.

SiliconOS defines its own macros (`kprintln!`, `kprint!`) with
`macro_rules!`. You do not need to understand their internal syntax: it is
enough to know that `kprintln!("x = {}", x)` works like the normal
`println!`.

---

## 15. `asm!`: assembly inside Rust

```rust
core::arch::asm!("out dx, al", in("dx") port, in("al") value);
```

- The text in quotes is the x86 assembly instruction (Intel syntax:
  destination first).
- `in("dx") port` means "before running, put the variable `port` in the DX
  register".
- `out("al") valor` means "after running, copy the AL register into
  `valor`".
- `in(reg) x` means "put `x` in any free register", and the `{}` in the text
  becomes the name of the chosen register.
- `options(nomem, nostack, ...)` are promises to the optimizer ("this
  instruction does not touch memory or the stack"), which let it generate
  better code.

It always requires `unsafe`.

---

## 16. Closures

```rust
TSS.call_once(|| { ... })
```

`|| { ... }` is an anonymous function with no parameters, like a Java lambda
(`() -> { ... }`) or a JS arrow function (`() => { ... }`). With
parameters: `|x, y| x + y`.

---

## 17. `no_std`, `core` and `alloc`

| Library | What it has | Depends on an OS? |
|---|---|---|
| `core` | types, `Option`, `Result`, iterators, formatting, atomics | **no** |
| `alloc` | `Box`, `Vec`, `String` (needs someone to provide memory) | no, but requires an allocator |
| `std` | files, network, threads, `println!`... | **yes** |

SiliconOS is `#![no_std]`: it uses `core` and, since the heap in doc 07,
also `alloc` ([§26](#26-alloc-box-vec-and-string)), with `Vec` and `String`
inside the kernel. `std` stays out.

---

## 18. Rust nightly

Rust has three channels: *stable*, *beta* and *nightly*. Nightly allows
experimental features enabled with `#![feature(...)]`. SiliconOS uses one
of them: `abi_x86_interrupt` (chapter 04), which lets you write interrupt
handlers in pure Rust.

---

## 19. Simple `enum`

```rust
pub enum Tipo { Livre, BootServices, Kernel, Runtime, Acpi, Mmio, Reservada }
```

Like a Java or Kotlin `enum`: a type that can only take one of the listed
values. It is used with the type name in front (`Tipo::Livre`) and almost
always with `match`, which forces you to handle every value
([§7](#7-match-a-switch-that-forces-you-to-cover-everything)).

Rust enums can also carry data in each variant (that is how `Option` and
`Result` are made: `Some(value)`, `Ok(value)`), but SiliconOS has not needed
that yet.

To compare two values with `==`, the enum needs
`#[derive(PartialEq, Eq)]`. In Java, comparing enums works without
declaring anything; in Rust, every capability is requested explicitly.

`matches!(value, A | B)` is a shortcut for "is the value A or B?", returning
a `bool`.

---

## 20. Chained iterators: `filter`, `map`, `fold`...

```rust
self.regioes()
    .iter()                                  // walk through the regions
    .filter(|r| r.tipo == tipo)              // only those of the requested type
    .fold((0, 0), |(n, q), r| (n + 1, q + r.quadros))   // accumulate (count, sum)
```

It is the same idea as Java `Stream`s, Kotlin's collection functions or
JavaScript's `filter`/`map`/`reduce`. The names:

| Rust | Java Stream / JS |
|---|---|
| `.iter()` | `.stream()` |
| `.filter(\|x\| ...)` | `.filter(x -> ...)` |
| `.map(\|x\| ...)` | `.map(x -> ...)` |
| `.find(\|x\| ...)` | `.filter(...).findFirst()` / `.find()` |
| `.fold(initial, \|acc, x\| ...)` | `.reduce(initial, ...)` |
| `.max()` | `.max()` |
| `.count()` | `.count()` |
| `.enumerate()` | (index + value pairs) |

One important difference: in Rust this **does not allocate memory** or
create intermediate objects. The compiler turns the chain into an ordinary
loop, as fast as writing the `for` by hand. That is why it can be used
inside the kernel before an allocator even exists.

---

## 21. Slices: a piece of an array

```rust
&self.regioes[..self.qtd]     // from the start up to qtd (exclusive)
&lista[..i]                   // the first i
```

A **slice** (`&[T]` or `&mut [T]`) is a "window" over a sequence of values
in memory: a pointer plus a length. It copies nothing. It is like Java's
`subList` or a `view` over an array. It works with range syntax: `[a..b]`
(from `a` up to before `b`), `[..b]`, `[a..]`.

Slices know their own length, so `slice[i]` checks the bound (and panics if
`i` is too large).

`core::slice::from_raw_parts_mut(pointer, n)` creates a slice from a raw
address and a count. It is `unsafe`, because the compiler has no way of
knowing whether that memory exists. SiliconOS uses it to see the
allocator's bitmap as an ordinary array.

Useful slice methods used in the project: `.len()`, `.fill(value)`,
`.contains(&x)`, `.sort_unstable_by_key(|x| key)`, `.iter()`,
`.iter_mut()`.

---

## 22. *Newtype*: a type of its own for a number

```rust
pub struct Quadro(pub u64);       // a "tuple struct" with a single field (Quadro = frame)

let q = Quadro(0x101000);
q.0                               // accesses the field (by position, not by name)
```

Wrapping a number in a type of its own makes the compiler **reject wrong
mixes**: a function that asks for a `Quadro` does not accept just any
`u64`, nor a future `EnderecoVirtual` (*virtual address*). In Java, the
equivalent would be creating a class just for that, with the cost of an
object. In Rust, the cost is **zero**: in memory, a `Quadro` is just a
`u64`.

---

## 23. `assert!` and `expect`: checks that become a panic

```rust
assert!(condicao, "message {}", valor);    // if the condition is false: panic with the message
assert_eq!(a, b);                          // if a != b: panic showing both values
opcao.expect("message")                    // like unwrap(), but with an explanatory message
```

They are used for **invariants**: things that, if false, mean a bug. Unlike
Java, Rust's `assert!`s **stay on in the release build too** (there is
`debug_assert!` for those that only apply in debug mode). In a kernel that
is an advantage: an impossible state becomes a clear message on the spot,
instead of silent corruption later.

`expect("...")` is preferable to `unwrap()` when failure is possible in
theory: the message appears in the panic and says **what** was missing.

---

## 24. The `?` operator: propagating errors

```rust
let nova = nova_tabela().ok_or(ErroMapeamento::SemMemoria)?;
```

Reading from left to right:

1. `nova_tabela()` returns `Option<u64>` (`None` if memory ran out);
2. `.ok_or(error)` converts it into a `Result`: `Some(x)` becomes `Ok(x)`,
   `None` becomes `Err(error)`;
3. the **`?`** at the end: if it is `Ok(x)`, the expression is worth `x` and
   the code continues; if it is `Err(e)`, **the current function returns
   `Err(e)` immediately**.

It is the equivalent of letting an exception "bubble up" in Java, but
visible in the code (the `?` marks exactly where the function may exit
early) and checked by the compiler: `?` can only be used in a function that
also returns a `Result` (or `Option`).

Without the `?`, you would have to write:

```rust
let nova = match nova_tabela() {
    Some(x) => x,
    None => return Err(ErroMapeamento::SemMemoria),
};
```

---

## 25. Atomics: `AtomicU64`, `AtomicBool`

```rust
static DESLOCAMENTO_ATUAL: AtomicU64 = AtomicU64::new(0);

DESLOCAMENTO_ATUAL.store(valor, Ordering::Relaxed);   // write
let d = DESLOCAMENTO_ATUAL.load(Ordering::Relaxed);   // read
```

The `Atomic*` types are like Java's `AtomicLong`/`AtomicBoolean`: values
that can be read and changed safely even by several CPUs at once, **without
a lock**. They are the simplest way to have a mutable global without
`unsafe` and without a `Mutex`, when the data is a single number.

The `Ordering` parameter controls the ordering guarantees between different
CPUs. `Relaxed` (the weakest) is enough when only the variable's own value
matters; `SeqCst` (the strongest, used in the panic handler) guarantees a
global order. As long as SiliconOS runs on a single CPU, the difference does
not show.

---

## 26. `alloc`: `Box`, `Vec` and `String`

With a global allocator (doc 07), the `alloc` crate starts working. It is
brought in with one line at the top of `main.rs`:

```rust
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
```

| Rust | What it is | Rough equivalent |
|---|---|---|
| `Box<T>` | a single value `T` stored on the heap; the `Box` is its owner | a Java object (always on the heap), but with a single owner |
| `Vec<T>` | a growable list, with the elements contiguous on the heap | `ArrayList<T>`, a Python `list`, a JS array |
| `String` | growable UTF-8 text, on the heap | Java's `StringBuilder` |
| `format!(...)` | like `kprintln!`, but returns a `String` | `String.format(...)`, f-strings |

```rust
let b = Box::new(42u64);                      // allocates 8 bytes and stores 42
let mut v: Vec<u64> = Vec::new();
v.push(1);                                    // may reallocate if it is full
let q: Vec<u64> = (1..=8).map(|x| x * x).collect();   // builds a Vec from an iterator
let mut s = String::from("Silicon");
s.push_str("OS");
let t = format!("{} = {}", "x", 10);
```

The big difference from Java: **there is no garbage collector**. When a
`Box`, `Vec` or `String` goes out of scope, `drop` (section 8) calls the
allocator and gives the memory back **at that moment**. `drop(v)` does it
before the end of the scope.

A `Vec` keeps three numbers: a pointer to the elements, the count (`len`)
and the capacity. When `len` reaches the capacity, the next `push` asks for
a bigger block (usually double), copies everything and frees the old one.
`Vec::with_capacity(n)` reserves space at once and avoids those copies.

`vec![value; n]` creates a `Vec` with `n` copies of `value` directly on the
heap. Useful for big buffers: `Box::new([0u8; 1 << 20])` would build the
1 MiB array **on the stack** before copying it, and could overflow it.

---

## 27. `GlobalAlloc`, `Layout` and `#[global_allocator]`

The `alloc` crate does not know where memory comes from. It calls the
`static` marked with `#[global_allocator]`, which must implement the
`GlobalAlloc` trait:

```rust
unsafe impl GlobalAlloc for AlocadorLista {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 { ... }      // null = out of memory
    unsafe fn dealloc(&self, ponteiro: *mut u8, layout: Layout) { ... }
}

#[global_allocator]
static ALOCADOR: AlocadorLista = AlocadorLista;
```

- **`Layout`** describes a request: `layout.size()` (bytes) and
  `layout.align()` (the address must be a multiple of this; always a power
  of 2). `Layout::new::<u64>()` is 8 and 8;
  `Layout::from_size_align(100, 4096)` builds one by hand.
- `dealloc` receives the **same** `Layout` used in `alloc`, so the
  allocator does not need to record the size of each block.
- **`unsafe impl`**: implementing an `unsafe` trait is a promise the
  compiler does not check. Here: "the blocks are valid, aligned and do not
  overlap". `unsafe impl Send for X {}` (doc 01) is the same kind of
  promise.
- Returning a null pointer from `alloc` means "out of memory". Rust then
  calls the *alloc error handler*, which in `no_std` triggers a panic
  "memory allocation of N bytes failed".
- You can also call the allocator directly:
  `alloc::alloc::alloc(layout)` and `alloc::alloc::dealloc(p, layout)`
  (always `unsafe`). Doc 07 uses this in the alignment and double free
  tests.

`core::mem::transmute::<A, B>(x)` reinterprets the bits of `x` as another
type, with no conversion at all. Doc 07 uses it to turn a heap address into
a function pointer, in the NX test. It is one of Rust's most dangerous
operations: use it only when there is no other way.

---

## 28. `Ordering::Acquire` and `Release`

```rust
// producer (the keyboard handler)
FILA[e].store(scancode, Ordering::Relaxed);
ESCRITA.store(proxima, Ordering::Release);

// consumer (the main loop)
if l == ESCRITA.load(Ordering::Acquire) { return None; }
let scancode = FILA[l].load(Ordering::Relaxed);
```

§25 said that `Relaxed` is enough "when only the variable's own value
matters". In the keyboard queue (doc 08) what matters is the **order
between two variables**: the byte in `FILA[e]` must be written **before**
`ESCRITA` moves forward. Otherwise the consumer would see the new position
and read an old byte.

Both the compiler and the CPU may reorder memory accesses that do not
depend on each other. The `Release`/`Acquire` pair forbids that at the point
that matters:

- a **`Release`** write guarantees: everything that came before it in the
  code becomes visible before it;
- an **`Acquire`** read that sees that value guarantees: everything that
  comes after it in the code sees what the other side wrote before the
  `Release`.

It is the same mechanism a `Mutex` uses inside (taking the lock =
`Acquire`, releasing it = `Release`), and it is equivalent, in Java, to
what `volatile` guarantees about earlier writes. With a single CPU, the
risk comes from the compiler; with several, from the CPU too.

To create an array of atomics in a `static`, where each element must be
constructed (atomics are not `Copy`), you use a `const` block:

```rust
static FILA: [AtomicU8; 64] = [const { AtomicU8::new(0) }; 64];
```

---

## 29. `#[repr(C, packed)]` and `read_unaligned`

```rust
#[repr(C, packed)]
struct Cabecalho {
    assinatura: [u8; 4],
    tamanho: u32,
    revisao: u8,
    checksum: u8,
    // ...
}

fn ler<T: Copy>(fisico: u64) -> T {
    unsafe { (fisico_para_virtual(fisico) as *const T).read_unaligned() }
}
```

Normally the compiler puts each field at an address that is a multiple of
its size (a `u32` at a multiple of 4), inserting empty bytes (*padding*)
between fields if needed. `packed` forbids that: the fields are glued
together, exactly as in a format defined by someone else (here, the ACPI
specification). `repr(C)` guarantees the order of the fields; `packed`, the
absence of gaps.

The price: a field may end up **misaligned**, and in Rust reading a
misaligned value with `*pointer` or `pointer.read()` is undefined behavior.
`read_unaligned()` is the read that accepts any address. For the same
reason, Rust does not let you create a reference (`&cab.tamanho`) to a field
of a `packed` struct; copying the value (`cab.tamanho`) is allowed.

`<T: Copy>` makes the function **generic**: `ler::<u32>(p)` reads 4 bytes,
`ler::<[u8; 4]>(p)` reads a signature, `ler::<Cabecalho>(p)` reads 36 bytes.
It is like `<T> T ler(long p)` in Java, with the restriction that `T` can be
copied bit by bit.

---

## 30. Taking a closure: `impl FnOnce`

```rust
pub fn sem_interrupcoes<R>(f: impl FnOnce() -> R) -> R {
    // ... turns interrupts off
    let resultado = f();
    // ... turns them back on
    resultado
}

cpu::sem_interrupcoes(|| escrever(args));
```

`impl FnOnce() -> R` means "any closure ([§16](#16-closures)) with no
parameters that returns an `R`". It is like taking a `Supplier<R>` in Java
or a function in JS, and it serves the "do something before, run the
caller's code, do something after" pattern, like `try`/`finally` or
Python's `with`.

There are three variants: `FnOnce` (can be called once, the most
permissive), `FnMut` (several times, may change what it captures) and `Fn`
(several times, only reads). The compiler generates one version of
`sem_interrupcoes` for each closure used, with no indirect-call cost.

`core::mem::take(&mut x)` returns the value of `x` and leaves the default
value in its place (`false`, `0`, `None`...). The keyboard decoder uses
`if core::mem::take(&mut self.estendido) { ... }` to read and clear the
flag in one operation.
