# 01: How the kernel writes text: serial and screen

> **You will understand:** how a kernel can show text with no operating
> system underneath; what an I/O port is; how the screen works as one big
> array of pixels; how letters are drawn; and how `kprintln!` ties it all
> together.
>
> **Files:** `src/serial.rs`, `src/console.rs`, and the macros at the top of `src/main.rs`
> **Book:** chapter 3.1 (serial) and 3.3 (VGA/framebuffer)
> **New Rust here:** traits ([§5](rust-basico.md#5-traits-interfaces)),
> `Option` ([§6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)),
> raw pointers and `unsafe` ([§9](rust-basico.md#9-raw-pointers-and-unsafe)),
> `static` ([§10](rust-basico.md#10-static-and-const)),
> `Mutex` ([§11](rust-basico.md#11-mutex-and-once-from-the-spin-crate)),
> `asm!` ([§15](rust-basico.md#15-asm-assembly-inside-rust))
>
> **Code for this chapter:** docs 00 to 06 are in a single commit: [state at the end of doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). What this chapter adds is in [Changes to the files](#changes-to-the-files).

---

## 1. The problem

In phase 1 (doc 00), writing text is easy: `uefi::println!` asks the
firmware to write. After `exit_boot_services`, the firmware is gone.
**There is no longer any function that writes text.** If the kernel
freezes, the screen stays still and you have no clue whatsoever about what
happened.

We need two output channels that we control ourselves:

| Channel | Advantage | Disadvantage |
|---|---|---|
| **Serial port** | Works with a single CPU instruction, before anything else. QEMU saves everything to a file or shows it in the terminal. | On a real PC, you need a serial cable/adapter to see it. |
| **Screen (framebuffer)** | It is what the user sees. | We have to draw every letter pixel by pixel. |

SiliconOS writes **to both at the same time**.

---

## 2. The serial port

### 2.1 What it is

The serial port (COM1, that 9-pin connector on old PCs) is the simplest way
for a computer to send bytes out, one bit at a time. Modern motherboards
rarely have the connector, but the **chip** still exists and every emulator
implements it. For debugging it is perfect:

- it does not depend on video, memory or configured interrupts;
- QEMU can send everything to the terminal (`-serial stdio`) or to a file
  (`-serial file:serial.log`). The file is kept **even if the machine
  reboots right afterwards**, and it is often the only clue about a freeze.

### 2.2 How to talk to it: I/O ports

The x86 processor has two separate "address spaces":

1. **memory**, accessed with ordinary instructions (`mov`);
2. **I/O ports**: 65,536 addresses (from `0x0000` to `0xFFFF`), accessed
   **only** with two special instructions:
   - `out`: sends a byte to a port;
   - `in`: reads a byte from a port.

Each old device "listens" on some of these ports. Writing to the right
port is like calling a function of the device. COM1 uses ports `0x3F8` to
`0x3FF`.

Rust has no ready-made function for `in`/`out`, so we use inline assembly
([rust-basico §15](rust-basico.md#15-asm-assembly-inside-rust)):

```rust
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value,
            options(nomem, nostack, preserves_flags));
    }
}
```

Reading the `asm!` line:

- `"out dx, al"` is the instruction: "send the byte in register AL to the
  port whose number is in DX". x86 requires exactly these registers for
  this instruction.
- `in("dx") port` means "before running, put the variable `port` in DX".
- `in("al") value` means "put `value` in AL".
- `options(...)` are promises to the compiler: the instruction neither
  reads nor writes memory (`nomem`), does not use the stack (`nostack`) and
  does not change the CPU flags (`preserves_flags`). That lets it optimize
  the surrounding code better.

The function is an `unsafe fn` because sending a byte to the wrong port can
do anything (turn off a device, for example); the caller must know what it
is doing. `inb` is the same, in the opposite direction.

### 2.3 Configuring the chip (UART 16550)

The serial chip is called a UART. Each of the 8 ports starting at `0x3F8`
is a **register** of the chip, a kind of internal "variable" of it:

| Port | Register | What we do with it |
|---|---|---|
| `0x3F8` (+0) | data, or low part of the speed | send the byte; set the speed |
| `0x3F9` (+1) | interrupts, or high part of the speed | we turn the chip's interrupts off |
| `0x3FA` (+2) | buffer (FIFO) control | we enable and clear it |
| `0x3FB` (+3) | transmission format | 8 bits per character, no parity, 1 stop bit ("8N1") |
| `0x3FC` (+4) | modem control | signals "I am ready" (DTR + RTS) |
| `0x3FD` (+5) | status | bit 5 says "you may send the next byte" |

The `Serial::init()` function is just this configuration, port by port:

```rust
pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // turn off UART interrupts
        outb(COM1 + 3, 0x80); // DLAB=1: the next 2 registers are the divisor
        outb(COM1 + 0, 0x03); // divisor = 3 -> 115200 / 3 = 38400 baud
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // DLAB=0, 8N1 format
        outb(COM1 + 2, 0xC7); // enable and clear the FIFOs
        outb(COM1 + 4, 0x03); // DTR + RTS
    }
}
```

The strangest part is the speed. The chip has a clock of 115,200 "ticks"
per second, and the speed is set by a **divisor**: 3 gives
115,200 / 3 = 38,400 bits per second (*baud*). Since there are only 8 ports
for many settings, the chip reuses ports: turning on the **DLAB** bit
(value `0x80` on port +3) makes ports +0 and +1 mean "divisor" instead of
"data". We write the divisor and turn DLAB off.

### 2.4 Sending a byte

```rust
pub fn write_byte(b: u8) {
    unsafe {
        while inb(COM1 + 5) & 0x20 == 0 {   // is status bit 5 still 0?
            core::hint::spin_loop();        // then wait
        }
        outb(COM1, b);                      // ready: send
    }
}
```

The chip only accepts a new byte when it has finished sending the previous
one. Bit 5 (`0x20` = `0010 0000`) of the status port says whether it is
free. We loop until it is. `spin_loop()` is a hint to the CPU that we are
waiting in a loop (it saves power).

> **Fun fact:** the first version of SiliconOS sent "KO" to the port
> without configuring or waiting. It worked because QEMU is forgiving; on a
> real chip, bytes would be lost.

### 2.5 Turning the serial port into a "text destination"

To write formatted text (`"x = {}"`), we implement the **trait**
`core::fmt::Write`, Rust's "interface" for text destinations
([rust-basico §5](rust-basico.md#5-traits-interfaces)):

```rust
pub struct Serial;           // struct with no fields: just a "name" to hang methods on

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                Serial::write_byte(b'\r');   // serial terminals want \r\n
            }
            Serial::write_byte(b);
        }
        Ok(())
    }
}
```

By implementing only `write_str`, the trait gives us `write_fmt` for free,
and with it `writeln!(Serial, "value = {}", x)` works.

---

## 3. The screen: framebuffer

### 3.1 The screen is an array of pixels in memory

With UEFI, video works like this: the firmware reserves a region of memory
in which **each pixel of the screen is a 32-bit number** (`u32`). Writing a
number to that memory **changes the pixel's color immediately**. That
region is called the **framebuffer**.

```
 framebuffer (memory)                        screen
 ┌────┬────┬────┬─────┬────┐              ┌─────────────┐
 │ px │ px │ px │ ... │ px │  line 0   →  │ ▓▓▓▓▓▓▓▓▓▓  │
 ├────┼────┼────┼─────┼────┤              │             │
 │ px │ px │ px │ ... │ px │  line 1   →  │             │
 └────┴────┴────┴─────┴────┘              └─────────────┘
```

It is a one-dimensional array representing a two-dimensional image. To
find the pixel at column `x`, line `y`:

```
index = y * stride + x
```

**`stride`** is how many pixels each line takes **in memory**. It may be
larger than the visible width, because some graphics cards round up the
size of the lines. In QEMU, stride = width = 1280. Using the width instead
of the stride is a classic bug: on hardware where they differ, the image
comes out "slanted".

Each `u32` is a color in the format `0x00RRGGBB`: `0x00FF0000` is red,
`0x0000FF00` is green, `0x00FFFFFF` is white. (UEFI may report another
order, BGR; in QEMU it is RGB, and that is what we assume for now.)

### 3.2 The `FrameBuffer` struct

```rust
#[derive(Clone, Copy)]
pub struct FrameBuffer {
    pub base: *mut u32,    // address of the first pixel
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}
```

`*mut u32` is a **raw pointer**: a memory address, as in C
([rust-basico §9](rust-basico.md#9-raw-pointers-and-unsafe)). We use a raw
pointer because Rust has no way of knowing this address is valid: it was
the firmware that told us.

### 3.3 Painting a pixel

```rust
fn pixel(&mut self, x: usize, y: usize, cor: u32) {      // cor = color
    if x < self.fb.width && y < self.fb.height {
        unsafe { self.fb.base.add(y * self.fb.stride + x).write_volatile(cor) };
    }
}
```

- `base.add(n)` computes the address of the n-th `u32` after `base`
  (pointer arithmetic).
- `write_volatile(cor)` writes the color to that address. The **volatile**
  matters: it tells the compiler **"do not optimize this write away"**.
  Without it, the optimizer could think "nobody reads this memory later, so
  the write is useless" and remove it. It does not know a graphics card is
  reading.
- The `if` avoids writing outside the screen, which would corrupt other
  memory.

### 3.4 Drawing letters: bitmap font

There is no font available at all. We use the `font8x8` crate, which is
basically a **table of bits**: for each of the 128 ASCII characters, 8
bytes, one per line; in each byte, each bit says whether that pixel is lit.

For example, the letter **A**:

```
byte   binary       drawing (bit 0 on the left)
0x0C   0000 1100    . . # # . . . .
0x1E   0001 1110    . # # # # . . .
0x33   0011 0011    # # . . # # . .
0x33   0011 0011    # # . . # # . .
0x3F   0011 1111    # # # # # # . .
0x33   0011 0011    # # . . # # . .
0x33   0011 0011    # # . . # # . .
0x00   0000 0000    . . . . . . . .
```

In this font, **bit 0** is the **leftmost** pixel, which is why the drawing
looks "mirrored" compared with the binary as written.

The function that draws a character walks through the 8×8 bits
(`desenhar_char` = *draw char*, `glifo` = *glyph*, `linha` = *line*,
`fundo` = *background*):

```rust
fn desenhar_char(&mut self, c: u8, col: usize, row: usize) {
    let glifo = BASIC_LEGACY[(c & 0x7F) as usize];   // the letter's 8 bytes
    for (gy, linha) in glifo.iter().enumerate() {    // gy = 0..7 (line of the drawing)
        for gx in 0..8 {                             // gx = 0..7 (column of the drawing)
            let cor = if linha & (1 << gx) != 0 { self.cor } else { self.fundo };
            for dy in 0..ESCALA {                    // each bit becomes an ESCALA×ESCALA block
                for dx in 0..ESCALA {
                    self.pixel(col * CHAR_W + gx * ESCALA + dx,
                               row * CHAR_H + gy * ESCALA + dy, cor);
                }
            }
        }
    }
}
```

- `c & 0x7F` guarantees an index between 0 and 127, the size of the table.
- `linha & (1 << gx) != 0` tests whether bit `gx` is set
  ([glossary: bits](glossario.md#bits-bytes-and-hexadecimal-quick-review)).
- An 8×8-pixel letter is tiny on a 1280×800 screen, so each bit becomes a
  2×2 square (`ESCALA` = *scale* = 2). Each letter takes 16×16 pixels, plus
  4 pixels of space between lines (`CHAR_H = 8*2 + 4`).
- `.iter().enumerate()` is like a `for (i, item)` with an index: it returns
  (position, value) pairs.

The table has ASCII only. Accented letters show up as `?`, which is why the
kernel's (Portuguese) messages are written without accents ("memoria",
"excecao").

### 3.5 The `Console`: cursor, line wrapping and scrolling

```rust
pub struct Console {
    fb: FrameBuffer,
    col: usize,       // cursor column (in characters, not pixels)
    row: usize,       // cursor line
    pub cor: u32,     // text color
    pub fundo: u32,   // background color
}
```

The `Console` works like a simple terminal:

- **`write_byte(b)`**: if it is `\n`, jump to the next line; otherwise,
  draw the character at the cursor position and move one column forward. If
  the line fills up, it wraps automatically.
- **`rolar()`** (*scroll*): when the cursor goes past the last line, copy
  the whole image one text line up (`core::ptr::copy`, the equivalent of
  Java's `System.arraycopy`) and paint the last line with the background
  color.

Just like `Serial`, `Console` implements `fmt::Write`, so
`writeln!(console, ...)` also works.

---

## 4. `kprintln!`: writing to both places

### 4.1 The console's global variable

The console must be reachable from **anywhere** in the kernel: today from
`kernel_main`, tomorrow from inside an error or interrupt handler. In other
words, it has to be a global variable:

```rust
pub static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);
```

Reading from the inside out:

| Part | Why |
|---|---|
| `Console` | the object that draws on the screen |
| `Option<...>` | it starts **empty** (`None`), because the screen's address is only known at run time. `kernel_main` puts `Some(Console::new(fb))` in it |
| `Mutex<...>` | guarantees that only one piece of code uses the console at a time ([rust-basico §11](rust-basico.md#11-mutex-and-once-from-the-spin-crate)) |
| `static` | global variable with a fixed address ([rust-basico §10](rust-basico.md#10-static-and-const)) |

Why a `Mutex` if there are no threads yet? Because soon there will be
**interrupts** (chapter 9). An interrupt may arrive **in the middle of** a
`kprintln!`, and its handler may also want to print. Without protection,
the two writes would get mixed up. Besides, Rust simply **does not let** you
create an unprotected mutable global, except with `unsafe`.

The `Mutex` comes from the `spin` crate: instead of "putting the thread to
sleep" (which would require an operating system), it loops trying to take
the lock.

A technical detail: Rust only lets you put into a shared `static` types it
considers safe to share between threads (the `Send` trait). A raw pointer
(`*mut u32`) is not automatically considered safe, so we declare it
manually:

```rust
unsafe impl Send for FrameBuffer {}
```

"I guarantee it is safe", and it truly is, because every access goes
through the `Mutex`.

### 4.2 The macro

```rust
#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => { $crate::kprint!("{}\n", format_args!($($arg)*)) };
}
```

The `macro_rules!` syntax is dense, but the idea is simple: `kprintln!` is
"`kprint!` with a `\n` at the end", and `kprint!` packs the arguments with
`format_args!` (the formatting from `core`, which **does not allocate
memory**) and calls the `_print` function:

```rust
pub fn _print(args: core::fmt::Arguments) {
    let _ = Serial.write_fmt(args);                      // 1. serial, always
    if let Some(con) = CONSOLE.lock().as_mut() {         // 2. screen, if it already exists
        let _ = con.write_fmt(args);
    }
}
```

- `CONSOLE.lock()` takes the lock; `.as_mut()` gives access to the
  `Console` inside the `Option`; `if let Some(con)` only enters if the
  console has already been created
  ([rust-basico §6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)).
- The lock is **released by itself** at the end of the `if`, when the
  "guard" returned by `lock()` goes out of scope.
- `let _ =` deliberately ignores the result of the write: if the screen
  fails, there is nothing to do.

Usage, just like a `println!`:

```rust
kprintln!("Video: {}x{} (stride {})", fb.width, fb.height, fb.stride);
```

---

## 5. How it was tested

Running in QEMU with no window (`SEM_TELA=1 ./run.sh`), the `serial.log`
file receives every line, and a screenshot taken through the QEMU monitor
shows the same text drawn in white on dark blue.

---

## Changes to the files

What this chapter adds to the code of the previous one. Docs 00 to 06 are in
the repository's first commit: each file's link leads to the code **at the
end of doc 06**, which may already contain what later chapters add.

| File | What changes | Where |
|---|---|---|
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/Cargo.toml) | dependencies `font8x8` (no default features) and `spin` (features `spin_mutex` and `once`) | `[dependencies]` |
| [`src/serial.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/serial.rs) | **new**: `Serial::init`, `write_byte`, `impl fmt::Write`, `outb`/`inb` | — |
| [`src/console.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | **new**: `FrameBuffer`, `Console` (draw character, scroll, new line), `impl fmt::Write` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `mod console; mod serial;`, the `static CONSOLE`, the `kprintln!`/`kprint!` macros and `fn _print` | top of the file, after the `use`s |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | in `main()`: block that opens GOP and builds the `FrameBuffer`; `fb` becomes an argument of `kernel_main` | before `exit_boot_services` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | in `kernel_main`: `Serial::init();`, `*CONSOLE.lock() = Some(Console::new(fb));` and the first messages | right after `cli` |

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `Console::new` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | Creates the console from the framebuffer info (address, width, height, pixels per line), with the cursor at the top left. |
| `Console::cols` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | How many text columns fit on the screen (width in pixels ÷ character width). |
| `Console::rows` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | How many text rows fit on the screen (height in pixels ÷ character height). |
| `Console::pixel` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | Paints one pixel: computes the offset `y * stride + x` and writes the color (u32) straight into framebuffer memory. |
| `Console::limpar` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | Paints the whole screen with the background color and moves the cursor back to the start. |
| `Console::desenhar_char` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | Draws one character at cell (column, row): for each bit of the 8×16 font glyph, one pixel in the text or background color. |
| `Console::rolar` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | When the screen is full, copies every pixel row one text line up and clears the last one. |
| `Console::nova_linha` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | Moves the cursor to the start of the next line; if it was already on the last one, calls `rolar`. |
| `Console::write_byte` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | Writes one byte to the screen: `\n` calls `nova_linha`, `\r` returns to column 0; anything else becomes `desenhar_char`, wrapping at the end of the line. (Since doc 08, backspace erases the previous character.) |
| `Console::write_str` | [console.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/console.rs) | `fmt::Write` implementation: passes each byte of the string to `write_byte`. This is what lets `write!` target the console. |
| `_print` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | What `kprint!`/`kprintln!` call: formats the text and sends it to the serial port and the screen. (Since doc 08 with interrupts off, via `escrever`.) |
| `Serial::init` | [serial.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/serial.rs) | Configures the COM1 UART (port 0x3F8): 38400 baud, 8 bits, no parity, 1 stop bit, FIFOs on, UART interrupts off. |
| `Serial::write_byte` | [serial.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/serial.rs) | Waits for the transmit register to be empty (bit 5 of the status port) and sends one byte. |
| `Serial::write_str` | [serial.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/serial.rs) | `fmt::Write` implementation for the serial port: sends byte by byte, turning `\n` into `\r\n`. (Since doc 08, backspace becomes "back, space, back".) |
| `outb` | [serial.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/serial.rs) | The `out` instruction: writes one byte to an I/O port. `unsafe` because a wrong port can do anything to the hardware. |
| `inb` | [serial.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/serial.rs) | The `in` instruction: reads one byte from an I/O port. |


---

## Difference from the book

The book also shows **VGA text mode**: an old mode in which the graphics
card draws the letters by itself and the program only writes character
codes at address `0xB8000`. That mode **only exists when booting through
BIOS**. With UEFI, the only video available is the pixel framebuffer,
which is why SiliconOS goes straight to it.

---

## Summary

- The **serial port** is the most reliable debug channel: one `out`
  instruction sends a byte; QEMU shows or saves everything.
- **I/O ports** are an address space separate from memory, accessed with
  `in`/`out`.
- The **framebuffer** is the screen seen as an array of `u32` in memory:
  `index = y * stride + x`.
- Letters are drawn from a **table of bits** (8 bytes per character).
- **`kprintln!`** formats the text and sends it to the serial port and to
  the global console (`static Mutex<Option<Console>>`).

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **Why does the write to the framebuffer use `write_volatile` instead of
   a normal assignment?**
   So that the compiler does not remove the write thinking it is useless.
   Whoever reads that memory is the graphics card, which the compiler does
   not know about.

2. **What happens if we use `width` instead of `stride` on hardware where
   they are different?**
   Each line starts in the wrong place and the image comes out slanted and
   scrambled.

3. **Why is the console an `Option`?**
   Because it can only be created once we know the screen's address, at
   run time. Before that, it is `None` and `kprintln!` writes only to the
   serial port.

4. **Why does `Serial::write_byte` wait for bit 5 of the status port?**
   Because the chip only accepts a new byte after it has finished sending
   the previous one. Without the wait, bytes get lost.

</details>
