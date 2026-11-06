# 02: Panic handler: when Rust finds an error in the kernel

> **You will understand:** what a *panic* is in Rust; why, in a kernel, we
> are the ones who decide what happens when one occurs; and the pitfalls of
> writing an error handler that must not fail.
>
> **File:** `src/panic.rs`
> **Book:** chapter 3.2
> **Try it:** `./run.sh teste-panic`
> **New Rust here:** `-> !` ([§3](rust-basico.md#3-functions-and-the-type--never-returns)),
> attributes ([§13](rust-basico.md#13-attributes--and-)),
> features and `#[cfg]` ([§12](rust-basico.md#12-modules-crates-cargo-and-features))

---

## 1. What a panic is

Rust has no exceptions. **Expected** errors (file not found, invalid
number) are values of type `Result`, which you handle explicitly
([rust-basico §6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)).

Errors that indicate **a bug in the program**, situations that "should
never happen", produce a **panic** instead. Examples:

| Code | Why it panics |
|---|---|
| `panic!("impossible state")` | explicit call |
| `v[10]` on an array of 3 elements | index out of bounds |
| `None.unwrap()` | "I guaranteed it existed", and it did not |
| `255u8 + 1` (in debug mode) | arithmetic overflow |
| `x / 0` with integers | division by zero |

To compare with languages you know: a panic is like an
`IndexOutOfBoundsException` or a `NullPointerException` **that nobody will
catch**. The default behavior of an ordinary Rust program is:

1. print the message and the location (`file:line`);
2. unwind the stack, calling destructors along the way (*unwinding*);
3. terminate the process. The operating system cleans up the rest.

## 2. The problem in the kernel

In the kernel, **none of these three steps exists**:

1. Print where? `println!` belongs to `std`, which we do not have.
2. *Unwinding* depends on support libraries from `std`.
3. Terminate the process and go back to whom? The kernel **is** the
   process, and there is nobody below it.

That is why, in a `#![no_std]` project, the compiler **requires** you to
write the function that decides what happens on a panic. Without it, the
project does not compile:

```rust
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    ...
}
```

- `#[panic_handler]` is the attribute that says "this is the panic
  function". There can be only one in the whole program.
- `info: &PanicInfo` carries the message and the location of the error.
- `-> !` says that **it never returns** ([rust-basico §3](rust-basico.md#3-functions-and-the-type--never-returns)).
  That makes sense: after a bug, there is no way to continue safely. The
  compiler checks that the function ends in an infinite loop.

## 3. Before and after

Up to the previous step, SiliconOS used the ready-made panic handler from
the `uefi` crate (enabled by the `features = ["panic_handler"]` option in
`Cargo.toml`). It writes the message using **the firmware's console**. But
after `exit_boot_services`, that console no longer exists. The result: a
panic in the kernel **froze silently**, the worst kind of bug to
investigate.

Now the option has been removed from `Cargo.toml` and SiliconOS has its own
handler. A panic shows this, on the serial port and on the screen (with a
red background):

```
!!! KERNEL PANIC !!!
Local: src/main.rs:111:25
Mensagem: index out of bounds: the len is 3 but the index is 20790
```

(`Local` = *location*, `Mensagem` = *message*.)

## 4. The handler, step by step

An error handler has a special requirement: **it must not fail itself**,
because there is nobody left to handle its error. Each step below exists to
prevent one way of failing.

### Step 1: turn off interrupts

```rust
unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
```

`cli` (*clear interrupt flag*) prevents hardware interrupts from
interrupting the handler. Today they are already off, but in chapter 9 we
will turn the timer on. Without `cli`, a timer tick could run code in the
middle of the error report, and that code might be exactly in the broken
part of the system. In a panic, the rule is: **nobody else runs**.

### Step 2: protection against a panic inside the panic

```rust
static EM_PANIC: AtomicBool = AtomicBool::new(false);   // "in panic"

if EM_PANIC.swap(true, Ordering::SeqCst) {
    let _ = Serial.write_str("\n!!! PANIC DUPLO - parando\n");   // "DOUBLE PANIC - stopping"
    parar();                                                     // "stop"
}
```

What if the handler itself has a bug, for example in the code that draws
on the screen? It would trigger another panic, which would call the handler
again, which would trigger another panic... until the stack overflows.

To avoid that, a global variable marks "I am already handling a panic":

- `AtomicBool` is a boolean that can be read and changed safely even by
  several CPUs at once, like Java's `AtomicBoolean`.
- `swap(true, ...)` writes `true` **and returns the previous value**, in a
  single operation that nothing can interrupt halfway.
- The first time, the previous value is `false`: we carry on normally. If
  the handler is called again, the previous value is `true`: we write one
  short line and stop, without trying anything else.

In the future, with several CPUs, this also guarantees that **only one** of
them reports the panic.

### Step 3: write to the serial port first

```rust
let _ = writeln!(Serial, "\n!!! KERNEL PANIC !!!");
escrever_detalhes(&mut Serial, info);     // "write details"
```

The serial port is the channel most likely to work: it depends on no
video, memory or lock at all. If the rest fails, at least the serial port
has already received the message.

### Step 4: write to the screen, careful with the lock

```rust
if CONSOLE.is_locked() {
    unsafe { CONSOLE.force_unlock() };
}
if let Some(con) = CONSOLE.lock().as_mut() {
    con.cor = 0x00FF_FFFF;
    con.fundo = 0x00B0_0000;      // red
    let _ = writeln!(con, "\n!!! KERNEL PANIC !!!");
    escrever_detalhes(con, info);
    let _ = writeln!(con, "Sistema parado.");     // "System halted."
}
```

Consider this scenario:

1. A `kprintln!` takes the console lock.
2. In the middle of drawing, a bug triggers a panic.
3. The handler calls `CONSOLE.lock()` to write the message...
4. ...and **waits forever** for a lock that will never be released,
   because whoever held it will never run again.

That is a **deadlock**: the machine freezes and the screen shows nothing.
To avoid it, if the lock is held, we force its release with
`force_unlock()`.

Forcing a lock open is normally dangerous, because the protected data may
have been left half-written. Here it is acceptable: whoever held the lock
**will never run again**, and the worst that can happen is a badly drawn
letter. That is why the call is `unsafe`.

### Step 5: stop the CPU forever

```rust
fn parar() -> ! {
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
```

- `hlt` makes the CPU **sleep** until the next interrupt. It uses less
  power than an empty loop, which would keep the CPU at 100%.
- The `cli` before it guarantees that no ordinary interrupt wakes it up.
- The `loop` exists for a rare case: there is a special interrupt (NMI)
  that cannot be turned off. If it wakes the CPU, the loop puts it back to
  sleep.

## 5. The information in `PanicInfo`

```rust
fn escrever_detalhes(w: &mut impl Write, info: &PanicInfo) {
    if let Some(loc) = info.location() {
        let _ = writeln!(w, "Local: {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    let _ = writeln!(w, "Mensagem: {}", info.message());
}
```

- `info.location()` brings the file, line and column. The compiler stores
  this information inside the binary when it compiles each `panic!`.
- `info.message()` is the already formatted message.
- The parameter `w: &mut impl Write` accepts any text destination. That is
  why the same function writes both to `Serial` and to `Console`
  ([rust-basico §5](rust-basico.md#5-traits-interfaces)).

## 6. How it was tested

### A "feature" just for testing

To see the handler working, we need to cause a panic on purpose, but
**without** leaving that code in the normal build. For that, `Cargo.toml`
declares a compilation option:

```toml
[features]
teste-panic = []
```

And the test code lives inside a block that is only compiled when that
option is on ([rust-basico §12](rust-basico.md#12-modules-crates-cargo-and-features)):

```rust
#[cfg(feature = "teste-panic")]
{
    let v: [u32; 3] = [1, 2, 3];
    let i = paginas_livres as usize;   // a large number: invalid index on purpose
    kprintln!("Testando panic: acessando v[{}]...", i);   // "Testing panic: accessing v[...]"
    kprintln!("{}", v[i]);             // ← panic here
}
```

Why does the index come from `paginas_livres` (*free pages*) and not from a
fixed number like 10? Because if we wrote `v[10]`, the compiler would
notice the error **at compile time** and refuse to compile. Using a value
that is only known at run time, the error really happens at run time.

`./run.sh teste-panic` builds with the feature on. `./run.sh` builds
without it, and the block simply does not exist in the binary.

### `-no-reboot`

`run.sh` passes `-no-reboot` to QEMU. If the CPU resets (which happens on
a *triple fault*, doc 04), QEMU **closes** instead of rebooting in a loop.
That way the last message on the serial port stays visible, and it is easy
to notice that something went very wrong.

## 7. What this handler does **not** catch yet

The panic handler is only called for **errors that Rust detects**: checks
the compiler inserted into the code (array bounds, `unwrap`...).

There is another category: errors that **the CPU itself** detects while
executing an instruction, such as accessing an address that does not
exist, executing bytes that are not an instruction, or dividing by zero in
assembly. These produce **CPU exceptions**, which **do not go through
Rust** and do not call the panic handler. To handle them we need the
**IDT** (doc 04), which in turn depends on the **GDT** (doc 03).

---

## Summary

- A **panic** is a logic error in the program (invalid index, `unwrap` on
  `None`...). In an ordinary program, `std` prints it and terminates the
  process.
- In a `no_std` kernel, **we** write the `#[panic_handler]`, which never
  returns (`-> !`).
- The handler is written so that it **cannot fail**: it turns off
  interrupts, avoids recursion (`AtomicBool`), writes to the serial port
  first, forces the screen lock open and stops the CPU with `cli; hlt`.
- Errors detected by the **CPU** (not by Rust) need another mechanism: the
  IDT.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **Why does the handler write to the serial port before the screen?**
   Because the serial port depends on neither video nor a lock; it is the
   channel most likely to work, even if the console is broken.

2. **What would happen without `force_unlock`, if the panic occurred
   inside a `kprintln!`?**
   Deadlock: the handler would wait forever for a lock that would never be
   released, and the error message would never appear on the screen.

3. **Why does the test use `paginas_livres` as the index instead of `10`?**
   With a fixed number, the compiler would detect the error at compile
   time and refuse to compile. We need a value known only at run time.

4. **Does an access to a nonexistent memory address call the panic
   handler?**
   No. The CPU detects it and raises an exception (page fault), handled by
   the IDT (doc 04).

</details>
