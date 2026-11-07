# 00: Overview: from the power button to the kernel

> **You will understand:** what a kernel is and why writing one is
> different from writing an ordinary program; what happens between pressing
> the power button and our code running; and why SiliconOS's Rust code has
> a "dialect" of its own (`no_std`, `unsafe`).
>
> **Main file:** `src/main.rs`
> **Book:** chapters 1 and 2
>
> **Code for this chapter:** docs 00 to 06 are in a single commit: [state at the end of doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). What this chapter adds is in [Changes to the files](#changes-to-the-files).

---

## 1. What a kernel is

Think of a program you have already written, for example a Java backend
that reads a file and answers HTTP requests. That program **never talked
directly to the hardware**:

- To read the file, it called `Files.readAllBytes(...)`. Deep down, that
  became a request to the **operating system**: "read these bytes from the
  disk for me".
- To open port 8080, the JVM asked the operating system.
- To allocate memory (`new Object()`), the JVM asked the operating system
  for blocks of memory.
- When the program threw a `NullPointerException` and died, it was the
  operating system that cleaned up the mess and freed the memory.

Whoever answers all those requests is the **kernel**: the core of the
operating system. It is the only program that runs with **full control** of
the CPU, the memory and the devices. All other programs ask it for things.

```
 ┌─────────────────────────────────────────────┐
 │  your programs (browser, IDE, backend)       │  ← ask for things
 ├─────────────────────────────────────────────┤
 │  libraries (libc, JVM, Python runtime)       │  ← translate the requests
 ├─────────────────────────────────────────────┤
 │  KERNEL  ← what we are writing               │  ← serves the requests
 ├─────────────────────────────────────────────┤
 │  hardware (CPU, RAM, disk, video, keyboard)  │
 └─────────────────────────────────────────────┘
```

### Why writing a kernel is different

When you write the kernel, **there is nobody below you to ask for help**.
Things you have always used without thinking simply do not exist:

| You are used to... | In the kernel... |
|---|---|
| `System.out.println` / `print()` | There is no terminal. We have to draw every letter on the screen, pixel by pixel (doc 01). |
| `new` / allocating lists | Nobody provides memory. We wrote our own allocator (docs 05 and 07). |
| Exceptions with a *stack trace* | An error freezes or reboots the machine with no message at all, until we write the handlers (docs 02 and 04). |
| The IDE's debugger | Only after a lot of work. At first, we debug by printing text. |
| Threads | They do not exist until we create them. |

That is why developing an OS follows an almost mandatory order, in which
each step provides tools for the next:

1. **Be able to write text** (doc 01). Without it, nothing can be debugged.
2. **Show errors instead of freezing** (docs 02 and 04).
3. **Put the CPU in a known state**, one that is ours, not the firmware's (doc 03).
4. **Manage memory** (next chapters).
5. **React to the hardware**: timer and keyboard.
6. **Run other programs** in an isolated, safe way.

SiliconOS has finished steps 1 to 5: memory (docs 05 to 07), hardware
interrupts, with the timer and the keyboard (doc 08), and an interactive
shell with automated tests (doc 09). Step 6 has begun: doc 10 runs the
first isolated programs, in user mode, doc 11 gives each program its
own address space, and doc 12 runs several of them at once, with a
scheduler.

---

## 2. What happens when the PC is turned on

### 2.1 The firmware

When the CPU receives power, it knows nothing about disks or operating
systems. It simply starts executing instructions from a fixed address,
which points to a memory chip on the motherboard. That chip contains the
**firmware**.

The firmware:

1. tests and initializes the hardware (RAM, graphics card, USB...);
2. searches the disks for a program to load;
3. loads that program into memory and hands control to it.

There are two generations of firmware:

| | BIOS (since 1981) | UEFI (since ~2010) |
|---|---|---|
| CPU mode when it hands over control | 16 bits ("real mode", like an 8086) | **64 bits**, already configured |
| What it loads | the first **512 bytes** of the disk | a `.efi` **file** from a FAT partition |
| Services offered | old, limited routines | video, keyboard, files, memory, network... |

With BIOS, an operating system's first job is to leave 16 bits, go
through 32 bits and reach 64 bits. That is what a classic boot sector
(`boot_sect.asm` in many hobby OS tutorials) starts doing. With UEFI, the
CPU already arrives in 64 bits.

SiliconOS uses **UEFI**.

### 2.2 How UEFI finds SiliconOS

UEFI looks for a FAT-formatted partition and, inside it, the file
`EFI/BOOT/BOOTX64.EFI`. In SiliconOS:

1. `cargo build --target x86_64-unknown-uefi` produces
   `target/x86_64-unknown-uefi/debug/siliconos.efi`. The
   `x86_64-unknown-uefi` target makes the compiler produce an executable in
   the **PE** format (the same format as Windows `.exe` files), which is the
   format UEFI understands.
2. `run.sh` copies that file to `esp/EFI/BOOT/BOOTX64.efi`.
3. QEMU is started with `-drive format=raw,file=fat:rw:esp`, which means
   "pretend the `esp/` folder is a FAT disk". The firmware (OVMF) finds the
   file and runs it.

To run it on a real PC, it would be enough to copy the `EFI` folder to a
FAT32 USB stick and boot from it. For now we have only tested in QEMU.

---

## 3. The two phases of SiliconOS

While the UEFI firmware is "in charge", it offers useful functions called
**Boot Services**: writing text on the screen, allocating memory, reading
files. But it also keeps **its own** timers, drivers and tables running. A
real operating system needs full control, so at some point it tells the
firmware: **"thank you, you may leave"**. That is the
**`ExitBootServices`** function.

That is why SiliconOS has two clearly separated phases:

```
 ┌─────────── PHASE 1: UEFI (function main) ────────┐
 │ • the firmware still helps: uefi::println! works  │
 │ • we ask the firmware where the screen is         │
 │ • we call exit_boot_services()                    │
 │   └─ the firmware hands us the MEMORY MAP         │
 └───────────────────────┬───────────────────────────┘
                         ▼
 ┌──────── PHASE 2: BARE-METAL (function kernel_main)┐
 │ • the firmware no longer helps with anything      │
 │ • only what WE wrote works                        │
 │ • this function never returns                     │
 └───────────────────────────────────────────────────┘
```

The **memory map** is a list of RAM regions with the type of each one:
"free", "used by the firmware", "used by the loaded program", "belongs to a
device"... It is from it that, in a later chapter, we decide which memory
the kernel may use.

### 3.1 The phase 1 code

```rust
#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    uefi::println!("Carregador UEFI iniciado.");   // "UEFI loader started."
```

- `#[entry]` is an **attribute** (similar to a Java *annotation*) provided
  by the `uefi` crate. It marks this function as the entry point the
  firmware will call.
- `-> Status` says the function would return a status code to the
  firmware, although, as we will see, it never actually returns.
- `uefi::helpers::init()` sets up the `uefi` crate (for example, so that
  `println!` works). The `.unwrap()` means "if there is an error, abort",
  because there is nothing to do if the firmware fails right at the start
  (see [rust-basico §6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)).

```rust
    let fb = {
        let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
        let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();
        let info = gop.current_mode_info();
        let (width, height) = info.resolution();
        FrameBuffer {
            base: gop.frame_buffer().as_mut_ptr() as *mut u32,
            width,
            height,
            stride: info.stride(),
        }
    };
```

Here we ask the firmware about the video. UEFI organizes its services into
**protocols** (think "service interfaces"); the video one is called **GOP**
(*Graphics Output Protocol*). From it we take:

- the **address** of the video memory (the *framebuffer*, explained in doc 01);
- the screen **width** and **height** in pixels;
- the **stride** (how many pixels each line takes in memory).

We keep it all in a struct of our own, `FrameBuffer`.

Why the `{ ... }` block around it? In Rust, values are destroyed when they
go out of scope ([rust-basico §8](rust-basico.md#8-references-and-the-borrow-checker-very-short-version)).
When the block ends, the `gop` object is destroyed, and destroying it
**closes the protocol** in the firmware. We need that to happen **before**
dismissing the firmware. If we tried to close the protocol afterwards, we
would be calling a firmware that no longer exists. What is left of the
block is only the `FrameBuffer` struct, with plain numbers.

```rust
    let memory_map = unsafe { uefi::boot::exit_boot_services(MemoryType::LOADER_DATA) };
    kernel_main(fb, &memory_map);
}
```

- `exit_boot_services` dismisses the firmware and returns the memory map.
  The `MemoryType::LOADER_DATA` argument says in which kind of memory to
  store the copy of the map.
- It is inside `unsafe` because, after this line, any code that still tries
  to use the firmware breaks, and the compiler has no way to check that for
  us ([rust-basico §9](rust-basico.md#9-raw-pointers-and-unsafe)).
- `kernel_main` never returns, so `main` never reaches its end either.

### 3.2 The start of phase 2

```rust
fn kernel_main(fb: FrameBuffer, memory_map: &impl MemoryMap) -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    Serial::init();
    *CONSOLE.lock() = Some(Console::new(fb));
    ...
```

- `-> !` means **"this function never returns"** ([rust-basico §3](rust-basico.md#3-functions-and-the-type--never-returns)).
  It ends in an infinite loop.
- `cli` is a CPU instruction that **turns off hardware interrupts**. The
  reason is explained in doc 03.
- Then we initialize the serial port and the screen (doc 01), the GDT
  (doc 03) and the IDT (doc 04), and print a summary of the memory map.

---

## 4. Rust without an operating system: `no_std` and `no_main`

The first lines of `main.rs`:

```rust
#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
```

### 4.1 `#![no_std]`: no standard library

Rust's standard library (`std`) is like Java's standard library: it brings
files, network, threads, `println!`, collections. But **almost everything
in it depends on an operating system underneath**: `println!` writes to
the terminal *through the OS*; `Vec` asks *the OS* for memory; `File`
opens files *through the OS*.

Since we **are** the operating system, we cannot use it. What is left is
**`core`**, the part of Rust that depends on nothing: numeric types,
`Option`, `Result`, iterators, text formatting, atomic operations
([rust-basico §17](rust-basico.md#17-no_std-core-and-alloc)).

What `std` did and is now our responsibility:

| `std` provided | In SiliconOS |
|---|---|
| `println!` | `kprintln!`, which writes to the serial port and the screen (doc 01) |
| what to do when the program panics | `#[panic_handler]` (doc 02) |
| memory allocator (`Vec`, `String`, `Box`) | the kernel's heap (doc 07) |
| `main` entry point | `#[entry]` from the `uefi` crate |

### 4.2 `#![no_main]`

In an ordinary Rust program, there is hidden code that runs **before**
your `main` (it prepares arguments, environment variables, threads). That
code depends on the OS. `#![no_main]` says: "do not generate that code; I
provide the entry point". In our case, `#[entry]` provides it.

### 4.3 `#![feature(...)]`

Turns on an experimental feature of *nightly* Rust. We use only one,
explained in doc 04.

---

## 5. Why there is so much `unsafe`

Rust guarantees, at compile time, that you will not access invalid memory,
as long as the compiler can **prove** the access is valid.

A kernel does things the compiler **has no way to prove**:

- "Write the color blue at address `0x80000000`." We know the screen is
  there because the firmware said so; the compiler does not.
- "Execute the `out` instruction to send a byte to the serial port."
- "Replace the CPU's segment table."

For these situations there is `unsafe { ... }`, which means: **"compiler,
here I am the one guaranteeing correctness"**. It does not turn off the
other checks; it only allows a few extra operations.

The discipline we follow (and that the book advocates in chapter 8.4):

1. **small** `unsafe` blocks, each with a comment saying why it is safe;
2. **safe functions** on top of them. Example: the `outb` function (send a
   byte to a port) is `unsafe`, but `Serial::write_byte` is not. Whoever
   uses the serial port needs to know nothing about ports.

---

## 6. Tools

| Tool | What it is for |
|---|---|
| **Cargo** | builds the project and downloads dependencies (like Maven/Gradle/npm) |
| **`x86_64-unknown-uefi`** target | makes the compiler produce a `.efi` instead of a Linux executable |
| **QEMU** | emulates a whole PC. Boots in 1 second, shows the serial output in the terminal, breaks nothing if the kernel freezes |
| **OVMF** | open source UEFI firmware, used by QEMU in place of a real motherboard's firmware |

Project dependencies (crates), declared in `Cargo.toml`:

| Crate | What we use it for |
|---|---|
| `uefi` | talking to the firmware in phase 1 (video, memory map, exit_boot_services) |
| `spin` | `Mutex` and `Once` that work without an operating system |
| `font8x8` | the drawing of the letters (a table of bits) |

---

## Changes to the files

Where the code starts. Docs 00 to 06 are in the repository's first commit, so
there is no snapshot of each one: the link in each file leads to the code
**at the end of doc 06**, which already has what the following chapters add.
The prerequisites (nightly Rust, the `x86_64-unknown-uefi` target, QEMU and
OVMF) are in the introduction, section "How to build and run".

| File | What changes | Where |
|---|---|---|
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/Cargo.toml) | **new**: package `siliconos`, edition 2024, dependency `uefi = { version = "0.33.0", features = ["logger"] }` | project root |
| [`run.sh`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/run.sh) | **new**: builds with `cargo build --target x86_64-unknown-uefi`, copies to `esp/EFI/BOOT/BOOTX64.efi` and opens QEMU with OVMF | project root |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | **new**: `#![no_std]`, `#![no_main]`, `#[entry] fn main()` (starts the UEFI helpers, writes a message, calls `exit_boot_services`) and `fn kernel_main(...) -> !`, which starts with `cli` and ends in `loop { hlt }` | — |

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `main` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | UEFI entry point (`#[entry]`). Saves the framebuffer and the RSDP, exits boot services and calls `kernel_main`. |
| `kernel_main` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | The kernel itself: initializes each subsystem in order and finally loops. Each chapter adds a step (GDT, IDT, memory, heap, interrupts; since doc 09 it hands control to the shell). Never returns. |


---

## Summary

- The **kernel** is the program that controls the hardware and serves the
  requests of all the others. Writing one, you lose everything the OS used
  to provide.
- The **UEFI firmware** turns the PC on, puts the CPU in 64 bits and runs
  the file `EFI/BOOT/BOOTX64.EFI`, which is SiliconOS.
- SiliconOS has **two phases**: with the firmware's help (it finds the
  screen) and without it, after `exit_boot_services` (the rest of the
  kernel).
- `#![no_std]` swaps the standard library for `core`, which does not depend
  on an OS. `unsafe` marks the places where the programmer guarantees
  correctness.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **Why does SiliconOS get the video information *before* calling
   `exit_boot_services`?**
   Because after that call the firmware no longer exists, and there is
   nobody left to ask where the screen is.

2. **Why can't the ordinary `println!` be used in the kernel?**
   Because it is part of `std`, which asks the operating system to write
   to the terminal. There is no OS below the kernel.

3. **What does the `-> !` in `kernel_main` mean?**
   That the function never returns. It ends in an infinite loop, because
   there is nowhere to go back to.

</details>
