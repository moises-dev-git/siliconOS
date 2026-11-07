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

---

## 31. Function pointers: `fn(...)`

```rust
pub struct Comando {
    pub nome: &'static str,
    pub funcao: fn(&[&str]),
}

const COMANDOS: &[Comando] = &[
    Comando { nome: "ajuda", funcao: ajuda },
];

(comando.funcao)(argumentos);   // call it
```

`fn(&[&str])` is the **type** "function that takes a slice of `&str` and
returns nothing". A value of that type is just the function's address, and
the name of any function with that signature fits (`ajuda`, `eco`...). It
is similar to a method reference in Java (`this::ajuda` stored in a
`Consumer<List<String>>`), but with no object behind it: it captures no
variables and fits in a `const`.

The parentheses in `(comando.funcao)(argumentos)` are needed: without
them, `comando.funcao(argumentos)` would look for a **method** called
`funcao`.

The difference from a closure ([§16](#16-closures)): a closure may capture
variables from where it was created; a function pointer may not. A closure
that captures nothing can be converted into an `fn`.

---

## 32. `let ... else` and `split_first`

```rust
let palavras: Vec<&str> = linha.split_whitespace().collect();
let Some((&nome, argumentos)) = palavras.split_first() else {
    return; // empty line
};
// from here on, `nome` and `argumentos` exist
```

`let PATTERN = value else { ... };` tries to fit the value into the pattern
(like a one-arm `match`). If it fits, the pattern's variables are available
**after** the `let`; if not, it runs the `else` block, which must leave
(`return`, `break`, `panic!`...). It is the "early return" of
`if (x == null) return;` in Java, but the compiler guarantees that, after
it, the value exists.

`split_first()` splits a slice into `Some((first, rest))`, or `None` if it
is empty. The `&nome` in the pattern strips the outer `&`: `split_first`
returns a reference to the element (`&&str`), and the pattern undoes it so
we get the `&str` directly.

---

## 33. Tests: `#[cfg(test)]` and `#[test_case]`

```rust
// at the end of a module
#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn ler_numero_decimal_e_hexadecimal() {
        assert_eq!(ler_numero("0x2a"), Some(42));
    }
}
```

- **`#[cfg(test)]`**: the item only exists when the code is built by
  `cargo test`. With `cargo build`, the `testes` module disappears from the
  binary. It is the same mechanism as features
  ([§12](#12-modules-crates-cargo-and-features)), with the `test` condition
  turned on automatically by Cargo.
- **`use super::*`**: brings in everything from the module above, including
  private items. Tests in the same file can test internal functions.
- In a regular project you use `#[test]`. Here it is **`#[test_case]`**,
  from the custom framework (doc 09), wired up in `main.rs` by:

```rust
#![feature(custom_test_frameworks)]                // nightly feature
#![test_runner(crate::testes::executar)]           // who runs the list of tests
#![reexport_test_harness_main = "rodar_testes"]    // name of the generated function
```

The compiler gathers all `#[test_case]` functions of the crate into a list
and generates `rodar_testes()`, which calls
`executar(&[&test1, &test2, ...])`.

---

## 34. `dyn Trait` and implementing for every `T`

```rust
pub trait Testavel {
    fn rodar(&self);
}

impl<T: Fn()> Testavel for T {
    fn rodar(&self) { /* ... */ self(); /* ... */ }
}

pub fn executar(testes: &[&dyn Testavel]) { ... }
```

- **`impl<T: Fn()> Testavel for T`** implements the trait for **every type**
  `T` that is a function with no parameters. In Java it would be as if every
  `Runnable` lambda automatically gained a new interface. In Rust this is
  called a *blanket implementation*.
- **`&dyn Testavel`** is a reference to "some type that implements
  `Testavel`", decided at run time. Each test function has a different
  type, so a list of them can only be of `&dyn Testavel`. It is the
  equivalent of a `List<Testavel>` of an interface in Java: the call
  `t.rodar()` goes through a method table (*vtable*).
- Contrast with `impl FnOnce` ([§30](#30-taking-a-closure-impl-fnonce)):
  `impl` generates a copy of the function for each type, decided at compile
  time; `dyn` uses a single function and decides at run time.

`core::any::type_name::<T>()` returns the name of type `T` as text. For a
function, the type is its full path, and that is how each test prints its
own name.

---

## 35. *Naked* functions and `naked_asm!`

```rust
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

A regular function gets from the compiler a **prologue** (reserve stack
space, save registers) and an **epilogue** (undo everything and `ret`). A
**naked** function gets nothing: the body is **only** the `naked_asm!`
assembly, and it is the one that must end with `ret`, `iretq` or a jump. It
is for code that handles its own stack or the registers in a way the
compiler does not understand, such as entering and leaving ring 3 (doc 10).

- `#[unsafe(naked)]`: the attribute is marked `unsafe` because the compiler
  checks nothing inside.
- Difference from §15's `asm!` ([§15](#15-asm-assembly-inside-rust)): `asm!`
  sits **inside** a normal function, and the compiler takes care of the rest;
  `naked_asm!` **is** the whole function.
- Special operands, which become text in the assembly:
  - `sym NAME`: the address of a `static` or a function (`sym RSP_KERNEL`,
    `sym tratar_syscall`);
  - `const VALUE`: a number known at compile time
    (`const gdt::USER_CODE as u64` becomes `35`).

---

## 36. `global_asm!` and external symbols

```rust
global_asm!(
    ".section .rdata",
    ".global programa_ola_inicio",
    "programa_ola_inicio:",
    "    lea rdi, [rip + ola_msg]",
    ...
);

unsafe extern "C" {
    static programa_ola_inicio: u8;
}

let p: *const u8 = &raw const programa_ola_inicio;
```

- **`global_asm!`** writes assembly outside any function, straight into the
  object file: labels, data (`.ascii`), sections. Doc 10 uses it for the
  user programs, which are not kernel functions.
- **`.global NAME`** makes the label visible to the linker, like a public
  symbol.
- **`unsafe extern "C" { static NAME: u8; }`** declares, in Rust, that the
  symbol exists somewhere. The type (`u8`) is just a starting point: what
  matters is the **address**, obtained with `&raw const NAME` (a raw pointer,
  without creating a reference).
- **`unsafe impl Sync for Programa {}`**: a `static` can only hold `Sync`
  types ("safe for several threads at once"), and raw pointers are not. The
  promise here is that they point to fixed bytes of the executable, which
  nobody changes.

---

## 37. Calling conventions: `extern "sysv64"`

```rust
unsafe extern "sysv64" fn entrar_no_anel3(rip: u64, rsp: u64) -> u64 { ... }
extern "sysv64" fn tratar_syscall(r: &mut Registros) { ... }
extern "x86-interrupt" fn page_fault(quadro: QuadroInterrupcao, codigo: u64) { ... }
```

The **calling convention** is the agreement, at the assembly level, on how a
function receives arguments and returns its result: in which registers, who
saves what, what the stack looks like. In Rust, `extern "NAME"` picks the
convention:

| Convention | Arguments | Where it appears |
|---|---|---|
| `"Rust"` (default) | unspecified: the compiler decides | regular functions |
| `"C"` | the target system's. On the UEFI target: **Windows'** (RCX, RDX, R8, R9) | interfacing with C |
| `"sysv64"` | **Linux/macOS'** (RDI, RSI, RDX, RCX, R8, R9) | doc 10's assembly |
| `"x86-interrupt"` | the CPU frame; ends with `iretq` | handlers (doc 04) |

When assembly calls a Rust function (`call {tratar}`) or is called by one,
both sides must agree. Choosing `"sysv64"` explicitly guarantees that the
first argument comes in RDI, as the assembly expects, regardless of the
target. The convention also says which registers the called function must
**preserve**: in `sysv64`, RBX, RBP and R12–R15. Those are exactly the ones
`entrar_no_anel3` pushes.

## 38. `Drop`: code that runs when the value dies

```rust
impl Drop for EspacoEnderecos {
    fn drop(&mut self) {
        // return the frames to the allocator
    }
}
```

In Java, an object nobody references any more disappears "some day", when
the garbage collector comes by; `finalize` (or a `Cleaner`) may never run,
which is why resources are closed by hand with `try-with-resources`/`close()`.
Rust has no collector: each value has **one owner**, and the value dies at
an exact point in the code, when the owner goes out of scope:

```rust
{
    let espaco = EspacoEnderecos::novo().unwrap();
    // ... use espaco ...
}   // <- here: the compiler inserts the call to `drop`
```

Implementing the `Drop` trait says what to do at that moment. It is the same
pattern as `try-with-resources` or Python's `with`, but automatic and
impossible to forget: if the type has `Drop`, the compiler calls it on every
path that ends the scope (end of block, `return`, `?`, a panic with
unwinding). The traditional name for this is **RAII** (*Resource
Acquisition Is Initialization*): the resource lives exactly as long as the
value.

Details that show up in SiliconOS:

- **`drop(value)`** (the standard library function, not the method) brings
  the death forward: it moves the value into itself, and the value dies
  there. `rodar` (doc 11) uses it to destroy the address space at a chosen
  point, after switching back to the kernel's CR3. Calling the method
  `value.drop()` directly is forbidden by the compiler: it would not let
  the value die twice.
- **Collections take their contents along.** When a
  `Vec<(&str, EspacoEnderecos)>` dies, each element dies too, and each
  `Drop` runs (the `espacos` command, doc 11).
- **Moving hands over the responsibility.** `espaco.mapear(virt, quadro, ...)`
  takes the `Quadro` by value: after the call, the caller can no longer use
  it, and the address space looks after it. That is how the type says "the
  owner is someone else now".
- The `MutexGuard` from `spin::Mutex` ([§11](#11-mutex-and-once-from-the-spin-crate)) uses the same mechanism: its
  `Drop` is what unlocks the mutex.

## 39. `UnsafeCell`: mutating through a shared reference

```rust
struct CelulaTss(UnsafeCell<Tss>);
unsafe impl Sync for CelulaTss {}

static TSS: Once<CelulaTss> = Once::new();
```

Rust's central rule ([§8](#8-references-and-the-borrow-checker-very-short-version)) is: either **many read-only references** (`&T`)
or **a single one that can write** (`&mut T`), never both. The compiler
relies on it to optimize: if you hold a `&T`, it may assume the value does
not change while the reference exists, keep the value in a register and
never read it again.

Sometimes, though, a value must change even though it is reached through a
shared reference: a `static` (which everyone sees) that changes after boot,
like the TSS's RSP0 (doc 12). That is what `UnsafeCell<T>` is for: the only
"official" way to tell the compiler "what is in here may change behind a
`&`; make no assumptions". All of Rust's interior mutability is built on
it: `Cell`, `RefCell`, the atomics (`AtomicU64`), and `spin::Mutex` itself
([§11](#11-mutex-and-once-from-the-spin-crate)).

- `cell.get()` returns a **raw pointer** `*mut T` to the contents, from a
  `&UnsafeCell<T>`. Reading or writing through it is `unsafe`: whoever
  writes guarantees that nobody else is reading or writing at the same time.
- A `static` must be `Sync` (safe to share between threads), and
  `UnsafeCell` is not. Hence the `unsafe impl Sync for CelulaTss {}`: a
  promise of ours, justified in the comment (only the scheduler writes,
  always with interrupts off, on a single CPU).
- Why not a `Mutex<Tss>`? Because the **CPU** also reads the TSS, on its
  own, on every interrupt coming from ring 3; it knows nothing about locks.
  What we need is a fixed address with changeable contents, and that is
  exactly what `UnsafeCell` gives.

In Java, the rough equivalent would be a `volatile` field that many objects
read and one writes; the difference is that in Rust the need has to be
declared in the type.

## 40. `build.rs` and `include_bytes!`: work done at compile time

```rust
// build.rs, at the crate root
fn main() {
    println!("cargo:rerun-if-changed=programas/src");
    // ... builds the programs, assembles the tar in OUT_DIR ...
}

// src/ramdisk.rs
static RAMDISK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ramdisk.tar"));
```

A `build.rs` file at the root of a crate is a **build script**: an ordinary
Rust program (with `std`, running on your machine) that cargo compiles and
runs **before** compiling the crate. It is used to generate code or data,
compile C code, look for system libraries. It is the equivalent of a Gradle
task or an npm `prebuild` script.

- The script talks to cargo by printing `cargo:...` lines.
  `cargo:rerun-if-changed=path` says "only run me again if this changes"
  (with none, cargo reruns it whenever any file in the crate changes).
- Cargo passes information through environment variables: `OUT_DIR` (the
  folder for generated files), `CARGO_MANIFEST_DIR` (the crate root),
  `CARGO` (cargo itself, to run another one).

On the code side, three macros that run at compile time:

- **`env!("NAME")`** becomes the value of the environment variable at
  compile time (a `&'static str`). Compile error if it does not exist.
- **`concat!(a, b)`** joins literals into one literal.
- **`include_bytes!("path")`** reads the file and embeds it in the
  executable as a `&'static [u8; N]`. There is also `include_str!`, for text.

Together they are the standard way to embed a file generated by `build.rs`:
the script writes it to `OUT_DIR`, and the code includes it from the same
path.

## 41. Implementing `Iterator` and lifetimes in structs

```rust
pub struct Arquivo<'a> {
    pub nome: &'a str,
    pub dados: &'a [u8],
}

pub struct Arquivos<'a> {
    tar: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Arquivos<'a> {
    type Item = Arquivo<'a>;
    fn next(&mut self) -> Option<Arquivo<'a>> { /* ... */ }
}
```

**Iterator.** The iterators from §20 (`map`, `filter`, `find`...) work with
any type that implements the `Iterator` trait, and implementing it takes
only two things: the item type (`type Item`) and the `next` method, which
returns `Some(item)`, or `None` when done. Everything else (`for`, `find`,
`count`, `map`...) comes for free. It is Java's `Iterator<T>` with `hasNext`
and `next` merged into a single method that returns an `Option`.

**Lifetime (`'a`).** A struct that holds **references** must say how long
they are valid. `Arquivo<'a>` reads: "an `Arquivo` pointing into something
that lives for the time `'a`". The compiler uses this to guarantee an
`Arquivo` never outlives the tar it came from (in Java the garbage collector
would solve it by keeping the tar alive; in Rust the compiler guarantees it,
at compile time). For `RAMDISK`, which lives for the whole program, `'a` is
`'static`; in the test that reads a copy of the tar in a `Vec`, `'a` is the
`Vec`'s lifetime, and the compiler would refuse to use an `Arquivo` after
the `Vec` dies.

## 42. `macro_rules!`: writing your own macros

```rust
#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{ $crate::print!($($arg)*); $crate::print!("\n"); }};
}
```

§14 showed how to **use** macros. `macro_rules!` **defines** one: a list of
rules "if the call has this shape, replace it with this code". The
replacement happens at compile time, before type checking.

- Each rule is `(pattern) => { code }`. The first one that matches wins.
- `$name:kind` captures a piece: `$main:path` (a path, like `main`),
  `$x:expr` (an expression), `$t:tt` (a *token tree*: any token or group in
  parentheses).
- `$( ... )*` repeats: `$($arg:tt)*` captures "any sequence of tokens", and
  `$($arg)*` gives it back unchanged. That is how `println!` accepts the
  same as `format!`.
- `#[macro_export]` makes the macro visible to users of the crate, at its
  root (`sos::println!`).
- `$crate` is the path to the crate where the macro was defined. Inside a
  program, `$crate::print!` becomes `sos::print!`, even if the program
  imported the library under another name.

Doc 13's `principal!` macro generates a whole **function** (`_start`),
something an ordinary function cannot do.

## 43. Const generics: `const N: usize`

```rust
fn ler<const N: usize>(dados: &[u8], pos: u64) -> Result<[u8; N], ErroElf> { ... }

let b: [u8; 8] = ler(dados, 24)?;       // N = 8, inferred from the expected type
let classe = ler::<1>(dados, 4)?[0];    // N = 1, given explicitly
```

Ordinary generics (`Vec<T>`) take **types**. Const generics take **values**
known at compile time: here, the size of the returned array. The compiler
generates one version of `ler` for each `N` used, each returning a
fixed-size array (no allocation).

`N` can be inferred: `u64::from_le_bytes` requires a `[u8; 8]`, so in
`u64::from_le_bytes(ler(d, p)?)` the compiler concludes `N = 8`. When there
is nothing to infer from, you write it with the "turbofish": `ler::<1>(...)`.

Java has nothing like it: Java arrays do not carry their size in the type.

## 44. `asm!`: clobbered registers and `#[inline(always)]`

```rust
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}
```

`asm!` (§15) is a contract with the compiler: it knows **only** what you
declare. Each operand states a register's role:

| Operand | Meaning |
|---|---|
| `in("rdi") a` | puts `a` in RDI before; the assembly does not change RDI |
| `out("eax") x` | afterwards, reads EAX into `x` |
| `inlateout("rax") n => r` | puts `n` in RAX before and reads RAX into `r` after |
| `lateout("rcx") _` | RCX comes out of here holding garbage: keep nothing in it |

The `_` in place of the variable means "discarded" (*clobbered*). The
`syscall` instruction writes RCX and R11 on its own (doc 14); without
declaring that, the compiler could keep a value in RCX before the `asm!` and
rely on it afterwards. The bug would not show at the `asm!` but far away, as
a mysteriously wrong value. ("late" means the register is only written at
the end, so the compiler may use the same register for an input.)

**`#[inline(always)]`** asks the compiler to paste the function's body at
every call site instead of emitting a `call`. For tiny functions like
`chamada`, it avoids the cost of a function call around every system call.
Without the attribute the compiler decides on its own (and is almost always
right); with it, the decision is ours.

---

## 45. `impl Fn`, variants with fields and arithmetic that does not overflow

```rust
pub fn esperar_ate(motivo: Motivo, condicao: impl Fn() -> bool) {
    cpu::sem_interrupcoes(|| {
        while !condicao() {
            bloquear_e_trocar(motivo);
        }
    });
}

escalonador::esperar_ate(Motivo::Teclado, || teclado::tem_scancode());
```

**`impl Fn() -> bool`** takes a closure, like the `impl FnOnce` in
[§30](#30-taking-a-closure-impl-fnonce), but promises the compiler something
different: the closure will be called **several times** (here, on each turn
of the `while`). `FnOnce` would not do: an `FnOnce` closure may consume what
it captured on its first call. In practice, pick the most permissive one your
use allows: `FnOnce` if you call it once, `FnMut` if you call it several
times and the closure needs to change what it captured, `Fn` if you call it
several times and it only reads.

**Variants with named fields.** An `enum` variant can carry data in two
ways: by position, `Processo(Pid)`, or by name, like a struct:

```rust
pub enum Motivo {
    Dormindo { ate: u64 },
    Teclado,
    Processo(Pid),
}

let m = Motivo::Dormindo { ate: 500 };
if let Estado::Bloqueado(Motivo::Dormindo { ate }) = p.estado {
    // here, `ate` is the u64 that was inside
}
```

The name works as documentation: `Dormindo { ate: 500 }` ("sleeping until
500") says more than `Dormindo(500)`. The `if let` with the nested pattern
tests both layers (is it `Bloqueado`? because of `Dormindo`?) and pulls the
field out, like a JS destructuring (`const { ate } = motivo`) that only
happens if the shape matches.

**Arithmetic that does not overflow.** In Rust, an `a * b` that goes past the
type's limit panics in debug builds (and silently wraps around in release
builds). When the number comes from outside (here, the milliseconds a program
asks for), the kernel must not crash because of it. These methods say what
to do at the limit:

| Method | `u64::MAX * 100` becomes | Use |
|---|---|---|
| `a.saturating_mul(b)` | `u64::MAX` (sticks to the limit) | "a lot" stays "a lot" |
| `a.checked_mul(b)` | `None` | when overflowing is an error to handle |
| `a.wrapping_mul(b)` | the remainder modulo 2⁶⁴ | counters that wrap on purpose |

And `a.div_ceil(b)` divides **rounding up**: `1.div_ceil(10)` is 1, not 0.

`Option::is_none_or(f)`, used in `terminou`, means "it is empty, or the value
inside satisfies `f`": `find(pid).is_none_or(|p| finished(p))`.

---

## 46. `impl Iterator`, patterns in slices and text from bytes

Used in doc 16 (`programas/src/lib.rs`, `src/shell.rs`).

**Returning `impl Iterator`.** An iterator in Rust has a concrete type,
sometimes a huge one: `(0..n).filter_map(argumento)` is a
`FilterMap<Range<usize>, ...>`, where the `...` is the function
`argumento`'s own type. Writing that in the signature would be unreadable and
would tie the function to its implementation. With `impl Trait` in the
**return** position, the function says only what it promises:

```rust
pub fn argumentos() -> impl Iterator<Item = &'static str> {
    (0..quantos_argumentos()).filter_map(argumento)
}
```

"I return some type that is an `Iterator` of strings." It is similar to
returning an `Iterable<String>` in Java or a generator in Python, with one
difference: the real type is still a single one, known to the compiler (no
object and no virtual call, §34). The caller uses the `Iterator` methods
(§20): `for`, `enumerate`, `skip(1)`, `collect`.

`filter_map(f)` calls `f` on each element and keeps only the `Some`s,
already unwrapped: it is a `map` followed by a `filter` that drops the
`None`s. Passing `argumento` (the function's name, without parentheses)
works because a function is a value (§31).

**Patterns that compare values inside a slice.** A `match` pattern can
contain fixed values, not just names:

```rust
let (argv, segundo_plano) = match argumentos.split_last() {
    Some((&"&", resto)) => (resto, true),
    _ => (argumentos, false),
};
```

`split_last()` returns `Option<(&T, &[T])>`: the last element (by reference)
and the rest. Since `T` here is `&str`, the last element arrives as `&&str`;
the `&` before `"&"` in the pattern "removes" one layer of reference so it can
be compared with the string. The whole pattern reads: "if there is a last
element, and it is the string `&`, call the rest `resto`". If it does not
match, it falls into `_`.

**From bytes to `&str`: `str::from_utf8`.** A `&str` in Rust is **always**
valid UTF-8; the compiler and the library rely on that. Bytes coming from
outside (a file, the network, the kernel) only become text after being
checked:

```rust
let bytes = core::slice::from_raw_parts(inicio, tamanho); // &[u8]
Some(core::str::from_utf8(bytes).unwrap_or("?"))           // &str
```

`from_utf8` returns `Result<&str, Utf8Error>` without copying anything: the
`&str` points to the same bytes. `unwrap_or("?")` replaces an error with a
fixed value. (In Java, `new String(bytes, UTF_8)` also decodes, but silently
replaces invalid sequences with `�`.)

**`$crate` and `sym` in a macro.** A macro (§42) is expanded **in the code
of whoever uses it**. If `principal!` wrote `sos::__iniciar`, it would depend
on the program calling the crate `sos`. `$crate` is a special name that, in
the expansion, always points to the crate where the macro was defined:

```rust
main = sym $main,                   // the function the program passed
iniciar = sym $crate::__iniciar,    // sos's own function
```

`sym` (§35) is the `asm!`/`naked_asm!` operand that becomes the **symbol
name** of a function or `static`, for the assembly to use in `call` or
`lea`. `$main:path` in the macro rule accepts a path (`main`,
`my::module::main`), which is exactly what `sym` expects.

`#[doc(hidden)]` on `__iniciar` hides it from the generated documentation:
it has to be `pub` (the macro, expanded in another crate, calls it), but it
is not meant for programs to use. The two leading underscores are the same
convention: "public out of necessity, not by invitation".

---

## 47. `dyn Trait` in a `static`, arrays of `Option`, `take` and `forget`

Used in doc 17 (`src/vfs/`, `programas/src/bin/conferir.rs`).

**Trait objects in a `static` table.** The mount table stores file systems
of different types (`Sis` and `Disco`) side by side:

```rust
struct Montagem {
    caminho: &'static str,
    sistema: &'static dyn SistemaDeArquivos,
}

static MONTAGENS: &[Montagem] = &[
    Montagem { caminho: "/sis", sistema: &sis::Sis },
    Montagem { caminho: "/", sistema: &disco::Disco },
];
```

`&dyn SistemaDeArquivos` is a "fat" reference (§34): the value's address
plus the address of the trait's function table for that type. It is what
every reference to an interface already is in Java. `'static` says the value
lives forever; here, `Sis` and `Disco` are *structs* with no fields, and
`&sis::Sis` is a constant.

A `static` can be read by any code, at any moment, including from different
processes. That is why Rust requires what is inside it to be `Sync` ("can be
shared between threads"). With `dyn`, the compiler does not know which type
is behind the reference, so the requirement goes on the trait itself: `pub
trait SistemaDeArquivos: Sync` means "only types that are also `Sync` may
implement this trait".

**Arrays of values that cannot be copied: `[const { None }; N]`.** The short
way to build an array, `[value; N]`, copies `value` N times, and so only works
with `Copy` types (numbers, `bool`...). `Option<ArquivoAberto>` is not `Copy`
(it has a `Vec` inside). The way out is a `const` block: instead of copying a
value, the compiler evaluates the expression again for each position.

```rust
Descritores { abertos: [const { None }; MAX_ABERTOS] }
```

**`Option::take`: take it out and leave it empty.** `option.take()` returns
what was inside and leaves `None` in its place, in one operation. It is the
way to "take" a value out of a structure you only have borrowed (`&mut`),
without having to destroy it:

```rust
self.abertos.get_mut(i as usize).and_then(|a| a.take()).is_some()
```

The taken value dies at the end of the expression (nobody kept it), and its
`Drop` runs there.

**`find_map`: search and transform in one go.** `iter.find_map(f)` calls `f`
on each element and stops at the first one that returns `Some`, returning
that value. It is a `find` whose answer already comes transformed. Inside the
closure, `?` on an `Option` (§24) returns `None` early, and the search moves
on to the next one:

```rust
MONTAGENS.iter().find_map(|m| {
    let resto = caminho.strip_prefix(base)?;   // does not start with base? next
    ...
})
```

**Handing the value back on error.** `guardar` receives the file **by
value**: it is moved into the function (§8). If the table is full, the
function has nowhere to put it, and returning it inside the `Err` gives the
caller the chance to do something else with it:

```rust
pub fn guardar(&mut self, arquivo: ArquivoAberto) -> Result<u64, ArquivoAberto>
```

The standard library does the same in several places: `String::from_utf8`'s
error returns the bytes (`into_bytes()`), and `Rc::try_unwrap` returns the
`Rc` itself when it cannot unwrap it.

**`mem::forget`: do not run `Drop`.** `core::mem::forget(value)` consumes the
value without destroying it: `Drop` does not run. It is safe (it does not
cause undefined behavior), but it is almost always a leak. The legitimate use
is when "`Drop`'s job" has already been done some other way, as in
`conferir`, which closes the descriptor by hand and does not want the
`Arquivo` to close it again:

```rust
prova(f, "fechar o descritor", cru::fechar(d), 0);
core::mem::forget(a); // already closed by hand: Drop must not close again
```

**`bool as u64`.** Converting a `bool` to a number gives 0 (`false`) or 1
(`true`). `cat` uses this for its exit code: `falhou as u64`.

