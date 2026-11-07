# 03: GDT and TSS: putting the CPU in a state that is ours

> **You will understand:** what segments and privilege levels are; why,
> even on a 64-bit processor, we still need a table called the GDT; what
> the TSS is and why it holds "emergency stacks"; and how a new table is
> handed to the CPU.
>
> **File:** `src/gdt.rs` (called at the start of `kernel_main`)
> **Book:** chapter 4
> **New Rust here:** `#[repr(C)]` and `packed` ([§13](rust-basico.md#13-attributes--and-)),
> `const` and `static mut` ([§10](rust-basico.md#10-static-and-const)),
> `Once` ([§11](rust-basico.md#11-mutex-and-once-from-the-spin-crate)),
> closures ([§16](rust-basico.md#16-closures))
> **Read first:** the "Bits, bytes and hexadecimal" section of the [glossary](glossario.md#bits-bytes-and-hexadecimal-quick-review)
>
> **Code for this chapter:** docs 00 to 06 are in a single commit: [state at the end of doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). What this chapter adds is in [Changes to the files](#changes-to-the-files).

---

## 1. The problem

When `kernel_main` starts, the CPU is still configured with tables that the
**firmware** built for its own use. SiliconOS itself shows this at boot:

```
GDT do firmware:  base=0x75dc000 CS=0x38 SS=0x30 TR=0x0
GDT do SiliconOS: base=0x60af020 limite=55 CS=0x8 SS=0x10 DS=0x10 TR=0x28
```

(`GDT do firmware` = *firmware's GDT*, `limite` = *limit*.)

The first line says that, before this step, the kernel ran using a table
located at address `0x75dc000`, which belongs to the firmware. That brings
three problems:

1. **That memory is not ours.** When we write the memory manager, that
   region may be reused and overwritten. The CPU would keep reading the
   table from there, now full of garbage, and the system would break in
   mysterious ways.
2. **Things we will need are missing:** emergency stacks for serious
   errors (used in doc 04) and settings for ordinary programs (user mode).
3. **We do not know exactly what the firmware configured.** A kernel needs
   to know what state the CPU is in.

The solution is to build our own table and tell the CPU to use it.

---

## 2. A bit of history: what segments are

To understand the GDT, you need to know where it came from. The x86
architecture is more than 45 years old and **has never broken
compatibility**; each generation stacked new features on top of the
previous one.

### 1978: the 8086 and segments

The 8086's registers were 16 bits wide, which can address only 64 KiB. To
reach 1 MiB, Intel created the **segment registers** (CS for code, DS for
data, SS for the stack...). The real address was computed like this:

```
real address = segment × 16 + offset
```

It was like having several "base pointers" to regions of memory.

### 1982–1985: protected mode and the GDT

In the following processors (286, 386), the segment registers stopped
holding an address and started holding a **selector**: a number that
points to a line of a table in memory, the **GDT** (*Global Descriptor
Table*). Each line, called a **descriptor**, describes a segment:

- where it starts (base);
- how big it is (limit);
- **who may use it** (privilege level);
- **what may be done** with it (execute? write?).

That is how memory protection was born: an ordinary program could not
access the operating system's segments.

### 2003: 64-bit mode ("long mode")

By the time AMD created x86-64, segmentation had already been replaced by
a better mechanism, **paging** (chapter 7 of the book). So, in 64 bits, the
CPU **ignores the base and limit** of code and data segments: memory
becomes a single, "flat" space.

**But the GDT was not abolished**, because it is still needed for three
things:

1. **CS defines the mode and privilege of the code.** One bit of the
   descriptor says "this is 64-bit code", and another field says at which
   **privilege level** the code runs.
2. **SS** must point to a valid descriptor in some situations (when
   returning from an interrupt, when switching privilege level).
3. **The TSS** (section 5) can only be found by the CPU through the GDT.

In short: in 64 bits, the GDT is a **small, almost ceremonial** table, but
a mandatory one.

---

## 3. Privilege levels (rings)

The x86 CPU has 4 privilege levels, called **rings**, numbered 0 to 3. In
practice, all modern operating systems use only two:

```
 ┌───────────────────────────────────────────────────────────────┐
 │ ring 3: ordinary programs (browser, editor, your backend)      │
 │   • cannot execute privileged instructions                     │
 │   • cannot access the kernel's memory                          │
 │   ┌───────────────────────────────────────────────────────┐   │
 │   │ ring 0: kernel                                         │   │
 │   │   • can do anything: cli, hlt, lgdt, in/out, memory    │   │
 │   └───────────────────────────────────────────────────────┘   │
 └───────────────────────────────────────────────────────────────┘
```

The CPU's current ring lives in the **2 lowest bits of the CS register**.
If CS = `0x08`, the 2 low bits are `00`, that is, ring 0. If a program in
ring 3 tries to execute `cli` or `hlt`, the CPU refuses and raises an
exception (doc 04).

This separation is what prevents a buggy program from bringing down the
whole system. Today SiliconOS only has code in ring 0, but the GDT already
provides the ring 3 segments for when we run programs.

---

## 4. Our GDT

### 4.1 The contents

| Line (index) | Selector | Describes | Used for |
|---|---|---|---|
| 0 | `0x00` | nothing (null line) | required by the specification; loading selector 0 into CS is forbidden |
| 1 | `0x08` | kernel code, 64 bits, ring 0 | CS while the kernel runs |
| 2 | `0x10` | kernel data, ring 0 | SS, DS, ES |
| 3 | `0x1B` | user data, ring 3 | future: ordinary programs |
| 4 | `0x23` | user code, 64 bits, ring 3 | future: ordinary programs |
| 5 and 6 | `0x28` | the TSS (takes two lines) | emergency stacks |

In the code, the table is simply an array of 7 64-bit numbers:

```rust
[
    0,                // line 0: null
    DESC_KERNEL_CODE, // line 1
    DESC_KERNEL_DATA, // line 2
    DESC_USER_DATA,   // line 3
    DESC_USER_CODE,   // line 4
    tss_baixo,        // line 5 ┐ TSS descriptor
    tss_alto,         // line 6 ┘ (16 bytes)
]
```

(`baixo`/`alto` = *low*/*high*.)

### 4.2 How a selector works

The selector is the number that goes into the segment registers (CS,
DS...). It **is not** the line index directly; it is the index multiplied
by 8, plus a few bits of information:

```
selector (16 bits):  line index       |  TI  |  RPL
                     bits 15 ... 3    | bit 2| bits 1-0
```

- **index**: which GDT line. Multiplying by 8 is the same as shifting 3
  bits left (`1 << 3 = 8`), making room for the other fields. Each line is
  8 bytes, so "index × 8" is also the byte position inside the table.
- **TI**: 0 = GDT (there is an alternative table, the LDT, which we do not
  use).
- **RPL**: the "requested" privilege level. For user segments, it is 3.

In Rust:

```rust
pub const KERNEL_CODE: u16 = 1 << 3;        // line 1 → 0x08
pub const KERNEL_DATA: u16 = 2 << 3;        // line 2 → 0x10
pub const USER_DATA: u16 = (3 << 3) | 3;    // line 3, ring 3 → 0x18 | 3 = 0x1B
pub const USER_CODE: u16 = (4 << 3) | 3;    // line 4, ring 3 → 0x20 | 3 = 0x23
pub const TSS_SEL: u16 = 5 << 3;            // line 5 → 0x28
```

Why does user data come **before** user code? Because the
`syscall`/`sysret` instructions, which we will use in the future for
programs to ask the kernel for things, compute these selectors by adding
fixed values and expect exactly this order.

### 4.3 Building a descriptor bit by bit

Each descriptor is a 64-bit number in which each bit (or group of bits)
has a meaning defined by the Intel/AMD manual. Many tutorials write the
value ready-made, like `0x00AF9A000000FFFF`, which is impossible to
understand. SiliconOS gives each bit a **name** and builds the value with
`|` (OR):

```rust
const P: u64 = 1 << 47;    // Present: the descriptor is valid
const S: u64 = 1 << 44;    // 1 = code/data segment (0 = "system", like the TSS)
const E: u64 = 1 << 43;    // Executable: it is code
const RW: u64 = 1 << 41;   // code: can be read / data: can be written
const L: u64 = 1 << 53;    // Long: 64-bit code
const DB: u64 = 1 << 54;   // 32-bit default size (cannot go together with L)
const G: u64 = 1 << 55;    // Granularity: the limit counts in 4 KiB blocks
const fn dpl(n: u64) -> u64 { n << 45 }   // privilege level: bits 45 and 46

const DESC_KERNEL_CODE: u64 = P | S | E | RW | L | G | LIMITE_MAX | dpl(0);
const DESC_KERNEL_DATA: u64 = P | S | RW | DB | G | LIMITE_MAX | dpl(0);
const DESC_USER_DATA:   u64 = P | S | RW | DB | G | LIMITE_MAX | dpl(3);
const DESC_USER_CODE:   u64 = P | S | E | RW | L | G | LIMITE_MAX | dpl(3);
```

Read `DESC_KERNEL_CODE` as a sentence: "present, code/data, executable,
readable, 64-bit, ..., ring 0". The difference between kernel code and user
code is **only the DPL** (0 or 3).

`const fn` is a function the compiler can execute **during compilation**.
All these values are computed before the program runs; the binary already
contains the final numbers.

Base and limit are ignored in 64 bits, but we fill the limit with the
maximum value (`LIMITE_MAX`) by convention: some tools and emulators check
this field.

---

## 5. The TSS (Task State Segment)

### 5.1 What is left of it in 64 bits

The name is historical. In 32 bits, the TSS held the complete state of a
task (all registers) so that the processor itself could switch tasks.
Operating systems did not use this feature much, and it was removed in
64 bits. What is left of the TSS is basically **a list of stack
addresses**:

```rust
#[repr(C, packed(4))]
pub struct Tss {
    _reservado0: u32,                 // reserved
    pub rsp: [u64; 3],     // stacks for when the CPU goes up in privilege (ring 3 → 0)
    _reservado1: u64,
    pub ist: [u64; 7],     // Interrupt Stack Table: 7 emergency stacks
    _reservado2: u64,
    _reservado3: u16,
    pub iomap_base: u16,
}

const _: () = assert!(size_of::<Tss>() == 104);
```

Two important Rust details:

- **`#[repr(C, packed(4))]`**: the CPU reads this struct directly from
  memory, expecting each field at an exact position (the manual says "the
  `ist[0]` field is at byte 36"). `repr(C)` stops the compiler from
  reordering the fields; `packed(4)` stops it from inserting alignment
  padding between them
  ([rust-basico §13](rust-basico.md#13-attributes--and-)).
- **`const _: () = assert!(...)`**: a check done **during compilation**.
  If, by any mistake, the struct is not exactly 104 bytes (the size defined
  by Intel), the project does not compile. It is a cheap safety net.

### 5.2 Why emergency stacks

Remember that the **stack** holds local variables and function return
addresses ([glossary: stack](glossario.md#the-stack)). When an
exception occurs, the CPU **pushes** information (where the code was, the
flags...) before calling the handler. Now imagine:

1. The kernel has infinite recursion, and the stack grows until it leaves
   the valid memory region.
2. The next call tries to write to the stack, in memory that does not
   exist: the CPU raises an exception (*page fault*).
3. To call the page fault handler, the CPU must **push** the
   information... **onto the same broken stack**. It fails again, and the
   CPU declares a **double fault** ("a fault while handling a fault").
4. To call the double fault handler, the CPU tries to push again, and
   fails again. That is a **triple fault**: the CPU gives up and **resets
   the computer**, showing nothing.

The solution is the **IST**: the TSS holds up to 7 addresses of separate
stacks. An entry of the interrupt table (doc 04) can say: "for this
exception, **switch to IST stack no. 1 before pushing anything**". That way
the double fault handler runs on a clean, guaranteed-valid stack, and can
show the error message.

Doc 04 shows this working, with a test.

### 5.3 The emergency stack in the code

```rust
const TAM_PILHA_IST: usize = 5 * 4096;     // 20 KiB  (TAM_PILHA = stack size)

#[repr(C, align(16))]
struct Pilha([u8; TAM_PILHA_IST]);         // Pilha = Stack

static mut PILHA_DOUBLE_FAULT: Pilha = Pilha([0; TAM_PILHA_IST]);
```

- The stack is just an array of 20,480 bytes.
- `align(16)`: the start of the array is at an address that is a multiple
  of 16, an alignment the x86-64 calling convention requires for the
  stack.
- It is `static`, so it lives **inside the executable itself** (in a
  section called `.bss`, for zero-initialized variables). We do not need a
  memory allocator, which we do not have yet.
- It is `static mut` (mutable global) because the CPU will write to it.

**The stack grows downward**: each `push` *decreases* the address. That is
why the TSS receives the address of the **end** of the array, not the
start:

```rust
let inicio = &raw const PILHA_DOUBLE_FAULT as u64;   // address of the first byte (inicio = start)
ist[0] = inicio + TAM_PILHA_IST as u64;              // top = first byte AFTER the array
```

`&raw const X` gets the address of `X` as a raw pointer, without creating a
Rust reference. It is the recommended way to take the address of a
`static mut`.

> **Curiosity seen at boot:** the top of the emergency stack (`0x60af020`)
> is exactly the address where the GDT begins. The compiler placed the GDT
> right after the stack in memory. That is correct: the "top" is the first
> byte **after** the stack, and the first `push` writes at `top - 8`.

### 5.4 The other fields

- **`rsp[0]`**: used in the future. When a ring 3 program is interrupted,
  the CPU automatically switches to the kernel stack given here. The kernel
  cannot trust an ordinary program's stack.
- **`iomap_base`**: setting it to the size of the TSS means "there is no
  I/O permission map", that is, ordinary programs **cannot** use `in`/`out`
  to talk to devices.

### 5.5 The TSS descriptor takes two lines

Normal descriptors are 8 bytes, and a descriptor's base only has room for
32 bits (scattered in three pieces, inherited from the 286). The TSS can be
at any 64-bit address, so its descriptor was extended to **16 bytes**,
taking two lines of the GDT. The second line holds the upper 32 bits of
the address:

```rust
fn descritor_tss(base: u64, limite: u64) -> (u64, u64) {
    let tipo_tss_disponivel: u64 = 0x9;      // "64-bit TSS, available"
    let baixo = (limite & 0xFFFF)              // limit, bits 0-15
        | ((base & 0xFF_FFFF) << 16)          // base, bits 0-23
        | (tipo_tss_disponivel << 40)
        | P
        | (((limite >> 16) & 0xF) << 48)      // limit, bits 16-19
        | (((base >> 24) & 0xFF) << 56);      // base, bits 24-31
    let alto = base >> 32;                    // base, bits 32-63
    (baixo, alto)
}
```

It is just "chopping up" the address and the size and fitting each piece
where the manual says. The function returns a **tuple** with the two
numbers (like a Kotlin `Pair`).

---

## 6. Why `spin::Once`

The GDT and the TSS have two requirements:

1. **They must exist forever, at the same address.** The CPU keeps only
   their **address**, not a copy. If the table were a local variable of
   `init()`, it would be destroyed when the function ended, and the CPU
   would be left pointing at garbage.
2. **They must be built at run time.** The TSS descriptor contains the
   address of the TSS, which is only known once the program is loaded in
   memory.

`Once<T>` meets both ([rust-basico §11](rust-basico.md#11-mutex-and-once-from-the-spin-crate)):

```rust
static TSS: Once<Tss> = Once::new();     // empty at first, but with a fixed address
static GDT: Once<[u64; 7]> = Once::new();

let tss = TSS.call_once(|| { /* builds the TSS */ });   // runs only the first time
```

`call_once` runs the closure ([rust-basico §16](rust-basico.md#16-closures))
exactly once, stores the result **inside the `static`** and returns a
`&'static` reference, that is, one valid forever.

---

## 7. Handing the tables to the CPU

Building the table in memory is not enough: we must tell the CPU where it
is and make it use the new segments. There are four steps, all in
assembly.

### Step 1: `lgdt`: "the GDT is here"

```rust
#[repr(C, packed(2))]
pub(crate) struct PonteiroTabela {      // "table pointer"
    pub limite: u16,   // table size in bytes, minus 1
    pub base: u64,     // table address
}

asm!("lgdt [{}]", in(reg) &ponteiro, ...);
```

The `lgdt` instruction (*load GDT*) reads a 10-byte structure: the
**limit** (size − 1; here 7 × 8 − 1 = 55) and the **address**. The
`packed(2)` guarantees that the two fields are packed together, with no
gap between them, as the CPU expects.

**Watch out:** after `lgdt`, the CPU is **not yet** using the new segments.
Each segment register keeps an **internal copy** of the descriptor that
was loaded into it, and only rereads the table when it is reloaded. Hence
steps 2 and 3.

### Step 2: reload CS with a "fake return"

There is no `mov cs, ...` instruction. The CS register only changes through
a "far" jump, call or return (one that switches segment). The trick used by
almost every kernel is to **simulate a function return**:

```rust
asm!(
    "push {sel}",            // push the new code selector (0x08)
    "lea {tmp}, [rip + 2f]", // compute the address of the "2:" label just below
    "push {tmp}",            // push that address
    "retfq",                 // "far return": pops address + selector and jumps
    "2:",                    // ...and execution continues here, with CS = 0x08
    ...
);
```

A `retfq` (64-bit "far" return) pops an address and a selector and jumps
there, loading CS. We push exactly what it expects: the address of the
next line and the new selector. In practice, execution **continues on the
line below**, but now with the new CS.

(`rip + 2f` means "the address of label `2`, searching *forward*". `rip` is
the register holding the address of the current instruction.)

### Step 3: reload the data segments

```rust
asm!("mov ss, {0:x}", "mov ds, {0:x}", "mov es, {0:x}", in(reg) KERNEL_DATA, ...);
asm!("mov fs, {0:x}", "mov gs, {0:x}", in(reg) 0u16, ...);
```

SS, DS and ES receive the data selector (`0x10`). FS and GS receive 0: in
64 bits they have another use (per-CPU and per-thread data), which we will
see in the future. The `{0:x}` asks for the 16-bit version of the register,
because segment registers are 16 bits wide.

### Step 4: `ltr`: "the TSS is here"

```rust
asm!("ltr {0:x}", in(reg) TSS_SEL, ...);
```

`ltr` (*load task register*) points the task register (TR) at the TSS
descriptor on line 5. One detail: the CPU **writes to the GDT** at this
moment, marking the descriptor as *busy*. That is why the GDT must be in
writable memory, and a second `ltr` with the same selector would cause an
exception.

---

## 8. Why the `cli` at the start of `kernel_main`

The interrupt table active at this moment is still **the firmware's**
(doc 04), and each of its entries points to handlers that use **the
firmware's** code selector, `0x38`.

In our GDT, line 7 (selector `0x38`) **does not exist**: the table has only
7 lines, 0 to 6. If a hardware interrupt arrived (a tick of the firmware's
timer, for example) right after `lgdt`, the CPU would try to use a
nonexistent segment, fail, and end in a triple fault.

That is why `kernel_main` starts with `cli`, which turns off hardware
interrupts. They stay off until we configure our own interrupt controller
(chapter 9 of the book).

---

## 9. How it was verified

The `gdt::estado()` function (*state*) reads the registers **back from the
CPU**:

| Instruction | What it reads |
|---|---|
| `sgdt` | the base and limit of the GDT the CPU is using |
| `mov reg, cs` / `ss` / `ds` | the current selectors |
| `str` | the task register (which TSS is loaded) |

Result:

- **before:** `CS=0x38 SS=0x30 TR=0x0`: the firmware's segments, and no TSS;
- **after:** `CS=0x8 SS=0x10 DS=0x10 TR=0x28`, base pointing to our
  table, limit 55.

We also checked through the **QEMU monitor** (`info registers`), which
shows the registers from the emulator's point of view, without depending on
our code. It confirmed: CS is "64-bit code, ring 0", SS/DS are "data", and
TR points to a 64-bit TSS of 104 bytes.

If any bit of a descriptor were wrong, the CPU would have refused the
`retfq` or the `ltr` with an exception, and the machine would have reset
(with QEMU in `-no-reboot`, it would simply close). The fact that boot
continues normally is already proof that the descriptors are valid.

---

## Changes to the files

What this chapter adds to the code of the previous one. Docs 00 to 06 are in
the repository's first commit: each file's link leads to the code **at the
end of doc 06**, which may already contain what later chapters add.

| File | What changes | Where |
|---|---|---|
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/gdt.rs) | **new**: selectors, descriptors, `Tss`, IST1 stack, `init`, `estado`, `topo_pilha_double_fault` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | `mod gdt;`; in `kernel_main`: `gdt::estado()` (before), `gdt::init()`, `gdt::estado()` (after) and the `kprintln!`s for both GDTs and IST1 | after the first messages |

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `dpl` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/gdt.rs) | Builds the privilege level bits (DPL, bits 45–46) of a segment descriptor. |
| `descritor_tss` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/gdt.rs) | Builds the TSS descriptor, which takes two GDT entries (128 bits) because the base is 64 bits wide. |
| `init` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/gdt.rs) | Fills the TSS (the double-fault IST1 stack and, since doc 10, RSP0), loads the GDT with `lgdt`, switches CS with `retfq`, reloads SS/DS/ES and loads the TSS with `ltr`. |
| `estado` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/gdt.rs) | Reads back the active GDT (`sgdt`) and the CS, SS, DS and TR selectors, so the boot can show that the switch worked. |
| `topo_pilha_double_fault` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/gdt.rs) | Address of the top of the IST1 stack (the double-fault one), printed at boot. |


---

## Difference from the book

The book uses the `x86_64` crate, which has ready-made
`GlobalDescriptorTable` and `TaskStateSegment` types. SiliconOS builds
everything by hand so that what those abstractions do becomes visible. In
the future, if you prefer, it can be swapped for the crate.

---

## Summary

- Segments come from the 8086. In 64 bits, base and limit are ignored, but
  the **GDT** is still mandatory: it defines the code's **ring** (0 =
  kernel, 3 = programs) and locates the TSS.
- **Selector** = line index × 8 + privilege bits. CS = `0x08` means "line
  1, ring 0".
- In 64 bits, the **TSS** is a list of stacks. The **IST** provides
  emergency stacks for serious errors, preventing a broken stack from
  turning into a reset.
- To switch the GDT: `lgdt` (address), `retfq` (reloads CS), `mov` into the
  data segments, `ltr` (loads the TSS).
- The `cli` at the start prevents an interrupt from using the firmware's
  selector, which does not exist in our table.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **If in 64 bits the CPU ignores the base and limit of segments, what is
   the GDT for?**
   To define the privilege level and the mode (64 bits) of the code, to
   have valid segments for SS, and to locate the TSS.

2. **What does selector `0x23` mean?**
   `0x23` = `0x20 | 3` = line 4 (`4 × 8 = 0x20`), with privilege 3: the
   user code segment.

3. **Why does the TSS receive the address of the *end* of the stack
   array?**
   Because the stack grows downward: the first `push` writes just below the
   top.

4. **Why is running `lgdt` not enough?**
   Because the segment registers keep internal copies of the old
   descriptors; they must be reloaded (CS through `retfq`, the others
   through `mov`).

5. **What would happen if a hardware interrupt arrived right after `lgdt`,
   without the `cli`?**
   The firmware's IDT would tell the CPU to use selector `0x38`, which does
   not exist in our GDT. That would cause a fault, then a double fault and
   probably a triple fault (reset).

</details>
