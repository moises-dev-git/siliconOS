# SiliconOS: documentation

SiliconOS is an operating system for 64-bit PCs (the **x86-64**
architecture), written in **Rust**, that boots directly from the **UEFI**
firmware. It is a learning project: every part of the code exists to teach
one piece of how an operating system works.

These documents explain **what** each part does, **why** it is needed and
**how** the code implements it, line by line when that is worth it. They
assume that you:

- already program (in Java, Kotlin, JavaScript, Python...),
- have **never** written an operating system,
- have **never** programmed in Rust.

The full source code is at
[github.com/moises-dev-git/siliconOS](https://github.com/moises-dev-git/siliconOS).

The texts are original. The order of the topics follows the book *Writing
Operating Systems in Rust* (Aura Fenwick), so you can read a chapter of the
book and then the matching document.

> **About the code in this English version:** the source code is written in
> Portuguese. In the snippets shown here, the **comments are translated**,
> but function and variable names are kept exactly as they appear in the
> source (for example `quadros` = *frames*, `mapa` = *map*, `paginacao` =
> *paging*), so you can find them in the repository. The kernel's real
> output is also shown as it is printed, in Portuguese, with a translation
> when it matters.

---

## Where to start

1. Read **[00: Overview](00-visao-geral.md)** in full. It gives the map of
   everything: what a kernel is, how the PC boots and what SiliconOS does
   from boot until now.
2. Keep these open for reference:
   - **[Rust for people who already program](rust-basico.md)**: only the
     Rust features used in the project, compared with Java, Kotlin, JS and
     Python;
   - **[Glossary](glossario.md)**: registers, stack, rings, interrupts,
     hexadecimal...
3. Follow the chapters in order. Each one starts by saying what you will
   learn and ends with a summary and review questions.

---

## Contents

| Doc | Topic | Book chapter | Status |
|---|---|---|---|
| [00: Overview](00-visao-geral.md) | What a kernel is, how the PC boots, UEFI, Rust without an operating system | 1 and 2 | ✅ |
| [01: Serial and video](01-saida-serial-e-video.md) | How the kernel writes text: serial port, framebuffer, font, `kprintln!` | 3.1 and 3.3 | ✅ |
| [02: Panic handler](02-panic-handler.md) | What to do when the kernel hits a Rust error | 3.2 | ✅ |
| [03: GDT and TSS](03-gdt-e-tss.md) | Segments, privilege levels, emergency stacks | 4 | ✅ |
| [04: IDT and exceptions](04-idt-e-excecoes.md) | Handling errors detected by the CPU (page fault, double fault...) | 5 | ✅ |
| [05: Physical memory](05-memoria-fisica.md) | Memory map, holes, frame allocator (bitmap) | 6 | ✅ |
| [06: Paging](06-paginacao.md) | Virtual addresses, 4-level tables, TLB, the kernel's tables | 7 | ✅ |
| [07: Heap](07-heap.md) | `Box`, `Vec`, `String` inside the kernel: bump and free-list allocators | 8 | ✅ |
| [08: Hardware interrupts](08-interrupcoes-de-hardware.md) | 8259, APIC and ACPI; 100 Hz timer; PS/2 keyboard; deadlock | 9 | ✅ |
| [09: Shell and tests](09-shell-e-testes.md) | Interactive shell, `cargo test` in QEMU, debugging with GDB | 10 | ✅ |
| [Rust for people who already program](rust-basico.md) | Rust reference | — | ✅ |
| [Glossary](glossario.md) | Term reference | — | ✅ |

---

## How to build and run

Prerequisites:

- **Rust nightly** with the `x86_64-unknown-uefi` target (the compiler
  produces an executable in the format UEFI understands):
  ```bash
  rustup toolchain install nightly
  rustup target add x86_64-unknown-uefi --toolchain nightly
  ```
- **QEMU** (`qemu-system-x86_64`), the PC emulator;
- **OVMF**, the UEFI firmware QEMU uses. On Ubuntu/Debian both come with
  `sudo apt install qemu-system-x86 ovmf`, and the firmware is at
  `/usr/share/ovmf/OVMF.fd` (the path may differ on other distributions).

The `run.sh` script does everything: it builds, copies the executable to
the `esp/` folder and starts QEMU.

```bash
./run.sh                    # normal run: QEMU window + messages in the terminal
SEM_TELA=1 ./run.sh         # no window; after 15 s shows what the kernel wrote
./run.sh teste-pagefault    # builds a version that causes an error on purpose
cargo test --target x86_64-unknown-uefi   # automated tests, inside QEMU (doc 09)
./depurar.sh                # QEMU + connected GDB, with function names (doc 09)
```

With the kernel running, click on the QEMU window and type `ajuda` ("help")
to see the shell's commands.

(`SEM_TELA` means "no screen"; `teste` means "test".)

Available test builds (each one causes a kind of error to prove that the
kernel handles it):

| Command | What it causes | Doc |
|---|---|---|
| `./run.sh teste-panic` | error detected by Rust (array index out of bounds) | 02 |
| `./run.sh teste-breakpoint` | breakpoint instruction (the kernel must **continue**) | 04 |
| `./run.sh teste-divisao` | division by zero | 04 |
| `./run.sh teste-opcode` | invalid instruction | 04 |
| `./run.sh teste-gpf` | impossible address (general protection) | 04 |
| `./run.sh teste-pagefault` | read of memory that does not exist | 04 |
| `./run.sh teste-doublefault` | destroyed stack (an error while handling another error) | 04 |
| `./run.sh teste-quadros` | allocates, writes, checks, frees and reallocates 1000 frames | 05 |
| `./run.sh teste-liberacao-dupla` | frees the same frame twice (must panic) | 05 |
| `./run.sh listar-regioes` | not a test: lists the full memory map at boot | 05 |
| `./run.sh teste-paginacao` | maps, translates, reads/writes through 2 paths, unmaps | 06 |
| `./run.sh teste-desmapeado` | accesses a page after unmapping it (must page fault) | 06 |
| `./run.sh teste-nulo` | reads address 0 (must page fault) | 06 |
| `./run.sh teste-heap` | `Box`, growing `Vec`, `String`, 15 thousand allocations, alignment, zero leaks | 07 |
| `./run.sh teste-heap-liberacao-dupla` | frees the same heap block twice (must panic) | 07 |
| `./run.sh teste-heap-esgotar` | allocates 1 MiB at a time up to the 64 MiB limit (must panic) | 07 |
| `./run.sh teste-heap-executar` | executes code written to the heap (must page fault: NX) | 07 |
| `./run.sh teste-bump` | swaps the allocator for bump: freed memory never comes back | 07 |
| `./run.sh teste-timer` | checks the timer's ticks against the CMOS clock (RTC) | 08 |
| `./run.sh teste-sem-eoi` | timer handler without EOI (the timer stops at the 1st tick) | 08 |
| `./run.sh teste-print-interrupcao` | `kprint!` inside the timer handler (works) | 08 |
| `./run.sh teste-deadlock` | the same without `kprintln!`'s protection (must freeze) | 08 |
| `./run.sh teste-teclado` | shows every scancode received from the keyboard | 08 |
| `cargo test --target x86_64-unknown-uefi --features teste-que-falha` | includes a `#[test_case]` that fails (`cargo test` must fail) | 09 |

To change the emulated machine's memory (default: 128 MiB), use `RAM`, for
example `RAM=4G ./run.sh teste-quadros`.

---

## Code map

```
siliconos/
├── Cargo.toml     Project configuration: dependencies and test "features"
├── run.sh         Builds, copies to esp/ and runs in QEMU
├── testar.sh      `cargo test` runner: runs the test .efi in QEMU        [doc 09]
├── depurar.sh     QEMU with GDB server + GDB with the symbols            [doc 09]
├── .cargo/config.toml  tells Cargo to use testar.sh
├── esp/           Folder that QEMU presents as a FAT USB stick
│   └── EFI/BOOT/BOOTX64.efi   ← our compiled kernel
├── docs/          This documentation (Portuguese); docs-en/ is the English one
└── src/
    ├── main.rs     Entry point. UEFI phase → leaves the firmware → kernel_main
    ├── serial.rs   Writes to the serial port (debug channel)           [doc 01]
    ├── console.rs  Draws text on the screen, pixel by pixel            [doc 01]
    ├── panic.rs    What to do when Rust detects an error               [doc 02]
    ├── shell.rs    The command line: editing, command table            [doc 09]
    ├── testes.rs   ("tests") the `cargo test` framework                [doc 09]
    ├── qemu.rs     Exits QEMU with a code (isa-debug-exit)             [doc 09]
    ├── gdt.rs      Segment table and emergency stack                   [doc 03]
    ├── idt.rs      Interrupt table and exception handlers              [doc 04]
    ├── cpu.rs      cli/sti, sem_interrupcoes, reading MSRs             [doc 08]
    ├── acpi.rs     finds the MADT: where the controllers are           [doc 08]
    ├── interrupcoes/   ("interrupts")                                   [doc 08]
    │   ├── mod.rs      vectors and the initialization sequence
    │   ├── pic.rs      turns off the old 8259
    │   ├── apic.rs     local APIC (inside the CPU)
    │   ├── ioapic.rs   I/O APIC (the devices' IRQs)
    │   ├── timer.rs    APIC timer calibrated with the PIT; RTC
    │   └── teclado.rs  ("keyboard") PS/2: lock-free queue and scancode translation
    └── memoria/    ("memory")                                           [doc 05]
        ├── mod.rs      constants (frame size, 1 MiB)
        ├── mapa.rs     our own copy of the UEFI memory map
        ├── quadros.rs  physical frame allocator (bitmap)
        ├── paginacao.rs  the kernel's page tables                       [doc 06]
        ├── heap.rs     the heap: free list + #[global_allocator]        [doc 07]
        └── bump.rs     bump allocator, for comparison only             [doc 07]
```

The current execution flow, end to end:

```
 UEFI firmware
   └─► main()                       [main.rs]  still with the firmware's help
         ├─ finds out where the screen is
         ├─ exit_boot_services()    ← dismisses the firmware, receives the memory map
         └─► kernel_main()          [main.rs]  from here on the machine is ours
               ├─ cli                    turns off hardware interrupts (until doc 08)
               ├─ Serial::init()         [serial.rs]
               ├─ Console::new()         [console.rs]
               ├─ gdt::init()            [gdt.rs]
               ├─ idt::init()            [idt.rs]
               ├─ (exception tests, if built with a test feature)
               ├─ memoria::mapa::init()  [memoria/mapa.rs]     + map summary
               ├─ memoria::quadros::init() [memoria/quadros.rs] + allocator summary
               ├─ (memory tests, if built with a test feature)
               ├─ memoria::paginacao::init() builds the kernel's tables and switches CR3
               ├─ (paging tests, if built with a test feature)
               ├─ memoria::heap::init()  maps 1 MiB and turns on the global allocator
               ├─ demonstrar_heap()      Box, Vec and String in the normal boot
               ├─ (heap tests, if built with a test feature)
               ├─ interrupcoes::init()   ACPI, 8259, APICs, timer, keyboard and sti
               ├─ (interrupt tests, if built with a test feature)
               ├─ (cargo test: rodar_testes() runs the #[test_case]s and exits QEMU)
               └─ shell::rodar()         sleeps (hlt); each key goes to the shell
```

---

## Where SiliconOS differs from the book

The book uses a **ready-made bootloader** (a third-party program that loads
the kernel and hands it the information it needs) and a custom compilation
target, as in the example project that comes with the book.

SiliconOS does not use a bootloader: **it is a UEFI program itself**. The
firmware loads it, it collects the video and memory information and then
dismisses the firmware to take over the machine. The book covers this path
in chapter 11.

In practice, only **how the kernel receives its initial information**
changes. From chapter 3 to 10 the concepts are the same. When some detail
differs, the document says so in a "Difference from the book" section.

Another difference: the book uses the `x86_64` crate (library), which
already provides ready-made structures for the GDT, IDT etc. SiliconOS
builds these structures **by hand, bit by bit**, so that what those
abstractions hide becomes visible.
