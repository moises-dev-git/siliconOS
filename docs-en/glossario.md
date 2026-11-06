# Glossary: hardware and operating systems

Terms that appear in the chapters, explained for people who program but
have never worked "below" the operating system. The **Rust** terms are in
[rust-basico.md](rust-basico.md).

---

## Bits, bytes and hexadecimal (quick review)

Almost everything in OSDev is described in **hexadecimal** (base 16, `0x`
prefix), because each hex digit corresponds to exactly 4 bits:

```
0x8E  =  1000 1110  (binary)  =  142 (decimal)
   8      E
```

That makes it easy to read "which bits are set" in a number. Bits are
numbered **from right to left, starting at 0**:

```
bit:   7 6 5 4 3 2 1 0
0x8E:  1 0 0 0 1 1 1 0     → bits 7, 3, 2 and 1 set
```

Bit operations used in the code:

| Expression | Meaning |
|---|---|
| `1 << 47` | a number with only bit 47 set |
| `a \| b` | combines the set bits of `a` and `b` (OR) |
| `x & 0x20` | isolates bit 5 of `x` (gives 0 if it is clear) |
| `x >> 16` | shifts right: drops the low 16 bits |

Sizes: **KiB** = 1024 bytes, **MiB** = 1024 KiB, **GiB**, **TiB**... (the
"i" indicates a power of 2, to tell it apart from KB = 1000).

---

## The CPU inside

**Register**
: A "variable" inside the CPU itself, much faster than memory. In x86-64
there are 16 general-purpose 64-bit registers (RAX, RBX, RCX, RDX, RSI,
RDI, RSP, RBP, R8–R15) and several special ones. Smaller parts have other
names: EAX = the low 32 bits of RAX, AX = 16 bits, AL = 8 bits.

**RIP** (*instruction pointer*)
: Register with the address of the **next instruction** to execute. When an
error shows "RIP=0x608c827", that is where the code was.

**RSP** (*stack pointer*)
: Address of the **top of the current stack**.

**RFLAGS**
: The "flags" register: bits that indicate the result of the last operation
(zero? negative? overflow?) and CPU states. The **IF** bit (*interrupt
flag*) turns hardware interrupts on/off.

**Control registers (CR0, CR2, CR3, CR4)**
: Configure the CPU's operating mode. Example: **CR2** holds the address
that caused the last page fault; **CR3** points to the page tables.

**Segment registers (CS, DS, SS, ES, FS, GS)**
: Relics of the 16/32-bit architecture. In 64 bits they serve almost only
to indicate the privilege level (CS) and a few special uses (FS/GS). See
chapter 03.

**Privileged instruction**
: An instruction only the kernel (ring 0) can execute: `cli`, `sti`, `hlt`,
`lgdt`, `lidt`, `in`, `out`... If an ordinary program tries, the CPU raises
an exception.

---

## The stack

The memory region holding local variables, parameters and function return
addresses. It is the same stack that shows up in the *stack trace* of an
error in Java/Python.

On x86 the stack **grows downward**: each `push` decreases RSP and writes
at the new address; each `pop` reads and increases RSP. That is why, when
reserving a 20 KiB stack between addresses `A` and `A + 20480`, we give the
CPU the **top** `A + 20480`, and it works its way down from there.

If infinite recursion consumes the whole stack, a **stack overflow**
happens, which in a kernel can turn into a *double fault* (chapter 04).

---

## Memory

**Physical address**
: The real position of a byte in the RAM chips (or in a device).

**Virtual address**
: The address the program uses. The CPU translates virtual addresses into
physical ones using the **page tables**, set up by the kernel. Each program
can have its own translation, which is why two programs can use the same
virtual address without seeing each other. (Chapter 7 of the book.)

**Page / frame**
: Memory is managed in **4 KiB** blocks. "Page" is the virtual block;
"frame" is the corresponding physical block.

**Page tables (PML4, PDPT, PD, PT)**
: The 4-level tree the CPU walks to translate a virtual address. Each table
takes one 4 KiB frame and has 512 entries (doc 06).

**CR3**
: Register holding the physical address of the top-level table (PML4).
Switching CR3 switches the whole address space.

**Huge page**
: A 2 MiB or 1 GiB page, which stops the walk through the tables earlier.
It saves tables and TLB entries.

**TLB** (*Translation Lookaside Buffer*)
: The CPU's cache of recent address translations. After changing a table
entry, the kernel must invalidate it (`invlpg`), otherwise the CPU may keep
using the old translation.

**Offset mapping** (*direct map*)
: All of physical RAM mapped starting at a fixed virtual address: physical F
is at virtual `offset + F`. In SiliconOS, starting at
`0xFFFF_8000_0000_0000`.

**Higher-half kernel**
: A kernel that runs at the high virtual addresses, leaving the lower half
to ordinary programs.

**NX** (*no-execute*)
: A page entry bit that forbids executing code in that page.

**WP** (*write protect*, bit 16 of CR0)
: When set, even the kernel gets a page fault when writing to a read-only
page.

**Page fault**
: Exception raised when the CPU tries to translate a virtual address and
does not find the page (or it does not allow the access).

**Memory map**
: A list, provided by the firmware, of which RAM regions are free, which
belong to the firmware, which belong to devices, etc.

**Identity mapping**
: A paging setup in which virtual address X points to physical address X.
UEFI hands over the machine like this, which is why, today, a physical
address can be used directly as a pointer.

**Frame allocator**
: The part of the kernel that hands out free 4 KiB physical frames to
whoever asks and takes them back (doc 05).

**Bitmap**
: A structure in which each bit represents one item. In the frame
allocator, one bit per frame: 1 = used, 0 = free.

**PCI window** (*PCI hole*)
: A range of physical addresses just below 4 GiB reserved for devices
(graphics card, controllers). The RAM that would be there is remapped above
4 GiB.

**Low memory**
: The first megabyte of addresses, a legacy of the original PC (up to
640 KiB of RAM, then video and ROMs). It has special uses, such as starting
the other CPUs.

**MMIO** (*memory-mapped I/O*)
: A device that appears as if it were memory: writing to a certain address
sends data to the device. The video framebuffer is an example.

**Heap**
: The memory region from which variable-size allocations (`Box`, `Vec`,
`String`) come, handed out and returned in any order. It has nothing to do
with the "heap" data structure (priority queue). (Doc 07.)

**Global allocator**
: The part of the kernel that serves all of Rust's memory requests. In
SiliconOS, the free list from doc 07.

**Bump allocator**
: An allocator that only bumps a pointer forward and never reuses freed
memory.

**Free list**
: A linked list of the heap's holes, with the nodes stored inside the holes
themselves.

**First fit**
: An allocation strategy that uses the first free block the request fits
in.

**Fragmentation**
: Free memory chopped into holes too small to serve a request, even if
their sum is large. Merging neighboring blocks on free (*coalescing*)
fights fragmentation.

**Memory leak**
: Memory that is allocated and never given back. In a kernel, which runs
for days, leaks end up exhausting RAM.

**I/O port**
: The older way of talking to devices: a separate space of 65,536
"addresses" accessed by the `in` and `out` instructions. The serial port and
the PS/2 keyboard use ports.

---

## Low-level software

**Firmware**
: A program stored in a chip on the motherboard, which runs when the
computer is turned on. It initializes the hardware and loads the operating
system.

**BIOS**
: Old firmware (since 1981). It hands over the CPU in 16-bit mode and loads
512 bytes from the disk.

**UEFI**
: Modern firmware. It hands over the CPU in 64 bits, reads files from FAT
partitions and offers services (video, keyboard, memory) until the OS
dismisses them.

**OVMF**
: The open source UEFI implementation QEMU uses.

**ESP** (*EFI System Partition*)
: The FAT partition where UEFI looks for `EFI/BOOT/BOOTX64.EFI`. In
SiliconOS it is the `esp/` folder.

**Bootloader**
: A program the firmware runs and that loads the kernel. In SiliconOS, the
kernel itself plays this role.

**Kernel**
: The core of the operating system: the only code that runs with full
privilege.

**Bare-metal**
: "Straight on the metal": code running with no operating system
underneath.

**QEMU**
: A PC emulator. It runs SiliconOS in a window and reboots in seconds.

**Shell**
: The program that reads a command line, interprets it and runs it. In
SiliconOS, it runs inside the kernel (doc 09).

**CPUID**
: The instruction a program uses to ask the CPU for its vendor, model and
features.

**FADT**
: The ACPI table (signature `"FACP"`) with the power control ports and the
address of the DSDT.

**DSDT / AML**
: The DSDT is an ACPI table that contains a program in AML (*ACPI Machine
Language*), a *bytecode* that describes devices and actions, such as the
code of the S5 (off) state.

**S5** (*soft off*)
: The ACPI "powered off by software" state. You enter it by writing the S5
code and the SLP_EN bit to the PM1 control register.

**`isa-debug-exit`**
: A QEMU device for testing: writing V to its port ends QEMU with exit code
`(V << 1) | 1`.

**GDB**
: The GNU debugger. With QEMU started with `-s`, it connects to the
emulated CPU through TCP port 1234.

**Breakpoint**
: A point in the code where the debugger stops execution.

**Symbol table**
: The name → address list of an executable's functions. It is what lets the
debugger show `siliconos::shell::executar` instead of a number.

**Mangled name**
: The name the compiler gives a function in the executable, encoding the
full path (e.g. `_RNvNtCs..._9siliconos5shell8executar`).

**Relocation**
: An address fix-up the loader applies when the executable is placed at an
address other than the one expected in the file (the *ImageBase*).

---

## Privilege and protection

**Rings**
: The x86 CPU's privilege levels. **Ring 0** = kernel (can do anything);
**ring 3** = ordinary programs (restricted). Rings 1 and 2 are hardly used.

**User mode / kernel mode**
: The same as ring 3 / ring 0.

**System call (*syscall*)**
: When a program asks the kernel for something (open a file, allocate
memory), it executes a special instruction that moves the CPU to ring 0 in
a controlled way.

---

## Interrupts and exceptions

**Interrupt**
: A signal that makes the CPU stop what it is doing, run a routine (the
**handler**) and then come back. It can come from hardware (key pressed,
timer) or software (the `int` instruction).

**Exception**
: An interrupt raised by the CPU itself because the current instruction has
a problem (division by zero, invalid access...).

**Vector**
: A number from 0 to 255 identifying each interrupt/exception.

**IDT**
: The table saying which handler serves each vector (chapter 04).

**Double fault / triple fault**
: Double fault = a fault **while handling** another fault. Triple fault = a
fault while handling the double fault; the CPU gives up and **resets the
computer**.

**`cli` / `sti`**
: Turn hardware interrupts off / on (the IF bit).

**`hlt`**
: Makes the CPU sleep until the next interrupt.

**IRQ** (*interrupt request*)
: A device's interrupt line. On the PC, numbers inherited from 1981: 0 =
timer, 1 = keyboard, 4 = COM1, 8 = RTC, 12 = mouse (doc 08).

**Interrupt handler**
: The function the CPU runs when an interrupt arrives. It should be short
and end with an EOI.

**8259 PIC**
: The original PC's interrupt controller: two cascaded chips, 15 IRQs, one
CPU. Today it is turned off in favor of the APIC.

**Local APIC**
: The interrupt controller inside each CPU. It delivers interrupts to the
core and has its own timer. MMIO registers at `0xFEE0_0000`.

**I/O APIC**
: A controller in the chipset that receives the devices' IRQs and forwards
them, as vectors, to the local APIC of some CPU.

**GSI** (*global system interrupt*)
: The number of an I/O APIC input (pin). Normally IRQ *n* = GSI *n*, except
for the overrides listed in the MADT.

**EOI** (*end of interrupt*)
: The handler's notice to the controller that it is done. Without it, the
APIC delivers no more interrupts of that priority class.

**Spurious interrupt**
: A "ghost" interrupt: the controller started delivering it, but the signal
disappeared. It has its own vector and takes no EOI.

**Edge / level**
: Edge-triggered: the interrupt is the change of the signal (one per
event). Level-triggered: it continues while the signal is active.

**MSR** (*model-specific register*)
: CPU configuration registers identified by number, read with `rdmsr`.
E.g. `0x1B` = the local APIC's address.

**ACPI**
: Tables the firmware leaves in memory describing the hardware. The path is
RSDP → XSDT → tables such as the MADT.

**MADT**
: The ACPI table (signature `"APIC"`) that lists the CPUs, the I/O APICs
and the IRQ overrides.

**PIT** (8253/8254)
: A 1981 timer with a fixed frequency of 1,193,182 Hz. Used to calibrate the
APIC timer.

**RTC** (CMOS clock)
: A battery-backed clock that keeps the date and time, accessed through
ports 0x70 and 0x71.

**Tick**
: Each timer interrupt. In SiliconOS, 100 per second.

**Scancode**
: The byte the keyboard sends for each key pressed (*make code*) or
released (*break code* = make + 0x80). It says the key's position, not the
letter.

**PS/2 controller** ("8042")
: The chip that receives the bytes from the PS/2 keyboard and mouse and
hands them over through port 0x60, raising IRQ 1 (keyboard) or 12 (mouse).

**Deadlock**
: Two sides waiting for each other forever. In the kernel: a handler
waiting for a lock that the code it interrupted is holding.

**Spinlock**
: A lock that waits by "spinning" in a loop (with the `pause` instruction)
instead of sleeping. It is SiliconOS's `spin::Mutex`.

---

## CPU tables

**GDT** (*Global Descriptor Table*)
: The segment table. In 64 bits it mainly defines the privilege levels
(chapter 03).

**Descriptor**
: An 8 (or 16) byte entry in one of these tables.

**Selector**
: A number pointing to a GDT descriptor (index × 8 + privilege bits). It is
what is kept in CS, DS, SS.

**TSS** (*Task State Segment*)
: A structure that, in 64 bits, mainly holds addresses of stacks to use in
special situations (chapter 03).

**IST** (*Interrupt Stack Table*)
: The TSS's 7 emergency stacks.
