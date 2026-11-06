# 06: Paging: virtual addresses and the kernel's tables

> **You will understand:** the difference between a virtual address and a
> physical address; how the CPU translates one into the other using 4
> levels of tables; why this is the foundation of all memory protection in
> an operating system; and how SiliconOS replaces the firmware's tables with
> its own without knocking itself down halfway through.
>
> **File:** `src/memoria/paginacao.rs` (called in `kernel_main` after the frame allocator)
> **Book:** chapter 7
> **Try it:** `./run.sh teste-paginacao`, `./run.sh teste-desmapeado`, `./run.sh teste-nulo`
> (and with more memory: `RAM=8G ./run.sh teste-paginacao`)
> **Read first:** doc 05 (frames and allocator)
> **New Rust here:** the `?` operator ([§24](rust-basico.md#24-the--operator-propagating-errors)),
> atomics ([§25](rust-basico.md#25-atomics-atomicu64-atomicbool)),
> `#[repr(transparent)]` ([§13](rust-basico.md#13-attributes--and-))
>
> **Names in the code:** `paginacao` = *paging*, `mapear`/`desmapear` =
> *map*/*unmap*, `traduzir` = *translate*, `tabela` = *table*, `entrada` =
> *entry*, `fisico` = *physical*, `deslocamento` = *offset*, `nivel` =
> *level*, `tamanho` = *size*.

---

## 1. The idea: every pointer goes through a "translator"

When your Java program reads a field of an object, the JVM uses a memory
address, say `0x7f3a_1000`. If you open two programs at the same time,
**both can use exactly the same address** `0x7f3a_1000` without one seeing
the other's data. How?

Because that address **is not the real position in RAM**. It is a
**virtual address**. On every memory access, the CPU translates it into a
**physical address** (the true position in the RAM chips), using a set of
tables that the **kernel** builds:

```
 program A:  pointer 0x7f3a_1000 ──► program A's tables ──► physical 0x0230_5000
 program B:  pointer 0x7f3a_1000 ──► program B's tables ──► physical 0x0811_2000
```

This mechanism, called **paging**, solves several problems at once:

| Problem | How paging solves it |
|---|---|
| Isolation: one program must not read another's memory | each program has its own tables; what is not in its tables simply does not exist for it |
| Protecting the kernel | the kernel's pages are marked "ring 0 only"; a program that tries to access them gets a page fault |
| Fragmented physical memory (doc 05) | the program sees a contiguous block of virtual addresses, built from frames scattered across RAM |
| Detecting bugs | a page with no mapping causes a page fault: null pointers and overflowing stacks are caught on the spot |
| On-demand loading, swap, `mmap`... | the kernel maps a page only when it is used for the first time (the page fault tells it) |

The unit of translation is the **page**: a 4 KiB block of virtual
addresses, pointing to a 4 KiB **frame** of physical RAM (doc 05). The 12
lowest bits of the address (the offset inside the page, 0 to 4095) pass
straight through; only the rest is translated.

### What SiliconOS was already using without knowing

Paging **is mandatory** in 64-bit mode: there is no x86-64 without it. Since
the very first boot, SiliconOS ran with tables that **UEFI** built. They
were in **identity mapping**: virtual X → physical X. That is why, until
now, we could treat a physical address and a pointer as the same thing
(doc 05, section 4.4).

---

## 2. How translation works: 4 levels of tables

### 2.1 Why not a single table

A virtual address in x86-64 has 48 useful bits: 256 TiB of space. With 4 KiB
pages, that is 2³⁶ = 68 billion pages. A simple table, with one 8-byte entry
per page, would take **512 GiB**. Impossible.

The solution is a **tree**: small tables (one per 4 KiB frame), in 4 levels,
and only the parts of the tree that are used get created. A program that
uses little memory needs few tables.

### 2.2 The virtual address is chopped into 5 pieces

```
 virtual address (64 bits)
 ┌────────────┬─────────┬─────────┬─────────┬─────────┬──────────────┐
 │ 63 ... 48  │ 47...39 │ 38...30 │ 29...21 │ 20...12 │  11 ... 0    │
 │ copy of    │ PML4    │ PDPT    │ PD      │ PT      │ offset       │
 │ bit 47     │ index   │ index   │ index   │ index   │ in the page  │
 │ (canonical)│ 9 bits  │ 9 bits  │ 9 bits  │ 9 bits  │ 12 bits      │
 └────────────┴─────────┴─────────┴─────────┴─────────┴──────────────┘
```

Each index has 9 bits, that is, it goes from 0 to 511. Each table has **512
entries of 8 bytes = 4096 bytes**: exactly one frame. It is no coincidence:
the format was designed for that.

The four levels have names inherited from Intel's manuals:

| Level | Name | Each entry covers |
|---|---|---|
| 4 | **PML4** (*Page Map Level 4*) | 512 GiB |
| 3 | **PDPT** (*Page Directory Pointer Table*) | 1 GiB |
| 2 | **PD** (*Page Directory*) | 2 MiB |
| 1 | **PT** (*Page Table*) | 4 KiB (one page) |

In the code:

```rust
pub fn indices(virt: u64) -> [usize; 4] {
    [
        ((virt >> 39) & 0x1FF) as usize, // PML4: bits 39–47
        ((virt >> 30) & 0x1FF) as usize, // PDPT: bits 30–38
        ((virt >> 21) & 0x1FF) as usize, // PD:   bits 21–29
        ((virt >> 12) & 0x1FF) as usize, // PT:   bits 12–20
    ]
}
```

`>> 39` shifts the address so that the desired piece ends up in the lowest
bits, and `& 0x1FF` (= 511 = nine 1 bits) drops the rest.

### 2.3 The walk (*page walk*)

The **CR3** register holds the **physical** address of the PML4. On every
memory access, the CPU does:

```
 CR3 ──► PML4 ─[PML4 index]─► entry: "the PDPT is at physical X"
                                  │
          PDPT (at X) ─[PDPT index]─► entry: "the PD is at physical Y"
                                  │
          PD (at Y) ─[PD index]───► entry: "the PT is at physical Z"
                                  │
          PT (at Z) ─[PT index]───► entry: "the page is in frame Q"
                                  │
          final physical = Q + offset (low 12 bits of the virtual address)
```

If, at any level, the entry is marked **not present**, the CPU stops and
raises a **page fault** (doc 04), with the virtual address in CR2.

SiliconOS has this same walk in software, in the `traduzir_em` function. It
is not used by the CPU (the CPU does this in hardware); it lets the kernel
**look up** the mapping, and is used for reports and tests:

```rust
pub fn traduzir_em(raiz: u64, virt: u64) -> Option<Traducao> {    // raiz = root
    if !canonico(virt) { return None; }
    let idx = indices(virt);
    let mut tabela = raiz;
    for nivel in 0..4 {
        let e = tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() { return None; }
        let tamanho = match nivel {
            1 if e.grande() => Some(TamanhoPagina::G1),
            2 if e.grande() => Some(TamanhoPagina::M2),
            3 => Some(TamanhoPagina::K4),
            _ => None,                          // not there yet: go down one more level
        };
        if let Some(t) = tamanho {
            let mascara_pagina = t.bytes() - 1;                 // page mask
            let base = e.endereco() & !mascara_pagina;
            return Some(Traducao { fisico: base + (virt & mascara_pagina), tamanho: t, flags: e.flags() });
        }
        tabela = e.endereco();                  // the next level
    }
    None
}
```

(`1 if e.grande() =>` is a `match` with an extra condition, a *guard*: "if
it is level 1 **and** the entry is large".)

### 2.4 Large pages: shortcuts in the tree

Mapping 8 GiB of RAM with 4 KiB pages would require 2 million entries and
4096 PT tables (16 MiB of tables alone). For large, contiguous regions, the
CPU lets you **stop the walk earlier**:

- a **PD** entry with the **GRANDE** (*large*) bit points directly to a
  **2 MiB** page (skips the PT);
- a **PDPT** entry with the **GRANDE** bit points to a **1 GiB** page
  (skips the PD and the PT; not every CPU supports it).

In those cases, the bits of the virtual address that would be the indices
of the skipped levels become part of the offset: 21 bits of offset for
2 MiB, 30 bits for 1 GiB. That is why `traduzir_em` uses `t.bytes() - 1` as
the mask.

SiliconOS uses **2 MiB** pages to map all of RAM (section 5) and 4 KiB
pages where it needs fine control.

### 2.5 The TLB: the translation cache

Doing 4 memory reads on every access would be extremely slow. The CPU keeps
recent translations in a cache called the **TLB** (*Translation Lookaside
Buffer*). An important consequence for whoever writes a kernel:

> **When you change a table entry, the CPU may keep using the old
> translation that is in the TLB.**

Two ways to clear it:

- **`invlpg address`**: removes one address from the TLB. SiliconOS does
  this on every `mapear`/`desmapear`;
- **writing to CR3**: clears the whole TLB (except pages marked GLOBAL). It
  happens when we switch tables.

Forgetting the `invlpg` is a classic bug: the code works "sometimes",
depending on what was in the cache.

---

## 3. The table entry

Each entry is a `u64` with a physical address and control bits:

```
 bit:  63  62..52  51 ................ 12  11..9  8  7  6  5  4  3  2  1  0
      ┌──┬───────┬────────────────────────┬──────┬──┬──┬──┬──┬──┬──┬──┬──┬──┐
      │NX│  ---  │ physical address of    │ free │G │PS│D │A │CD│WT│US│RW│P │
      │  │       │ next level/page        │      │  │  │  │  │  │  │  │  │  │
      └──┴───────┴────────────────────────┴──────┴──┴──┴──┴──┴──┴──┴──┴──┴──┘
```

The address takes bits 12–51. Since everything is aligned to 4 KiB, the low
12 bits of a frame address are always zero, and Intel used that space for
the control bits. In the code, each bit has a name:

```rust
pub mod flags {
    pub const PRESENTE: u64 = 1 << 0;       // the entry is valid (present)
    pub const ESCRITA: u64 = 1 << 1;        // allows writing (otherwise read-only)
    pub const USUARIO: u64 = 1 << 2;        // accessible from ring 3 (user)
    pub const ESCRITA_DIRETA: u64 = 1 << 3; // write-through cache
    pub const SEM_CACHE: u64 = 1 << 4;      // turns the cache off for this page
    pub const ACESSADA: u64 = 1 << 5;       // the CPU sets it when the page is read (accessed)
    pub const SUJA: u64 = 1 << 6;           // the CPU sets it when the page is written (dirty)
    pub const GRANDE: u64 = 1 << 7;         // 2 MiB page (PD level) or 1 GiB (PDPT level)
    pub const GLOBAL: u64 = 1 << 8;         // does not leave the TLB when CR3 is switched
    pub const SEM_EXECUCAO: u64 = 1 << 63;  // executing code from here is forbidden (needs EFER.NXE)
}
```

The most important to understand:

- **PRESENTE**: if it is 0, the CPU ignores all the rest and raises a page
  fault.
- **ESCRITA** and **USUARIO**: the permissions. The CPU **combines** the
  permissions of every level of the walk: to write, **all** of them must
  allow it. That is why SiliconOS creates the intermediate tables with open
  permissions and leaves the restriction to the final entry.
- **ACESSADA** and **SUJA**: the **CPU** sets them by itself. Operating
  systems use this to decide which pages to send to disk (swap) when memory
  runs low: a "dirty" page must be written out first; one not accessed for
  a long time is a good candidate.
- **SEM_EXECUCAO** (NX): prevents executing code in data pages. It is an
  important protection against attacks that inject code. It only works if
  the NXE bit of the EFER register is set; in QEMU, UEFI leaves it on.

The entry in Rust is a *newtype* over `u64` ([rust-basico §22](rust-basico.md#22-newtype-a-type-of-its-own-for-a-number)):

```rust
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Entrada(u64);
```

`#[repr(transparent)]` guarantees that, in memory, an `Entrada` is exactly a
`u64`, with nothing extra, which is necessary because the CPU reads these
tables directly. A `Tabela` is an array of 512 entries, aligned to 4 KiB:

```rust
#[repr(C, align(4096))]
pub struct Tabela {
    pub entradas: [Entrada; 512],
}

const _: () = assert!(core::mem::size_of::<Tabela>() == 4096);
```

---

## 4. Why we could not just "add" to UEFI's tables

The simplest idea would be to take the tables UEFI left and add what we
need. Before writing the code, we inspected the machine through the QEMU
monitor (`info registers` and `info mem`) and found:

```
CR0=80010033  CR3=0000000007801000  CR4=00000668  EFER=0000000000000d00

0000000006a00000-0000000006c00000  -r-
0000000007800000-0000000007e00000  -r-     ← UEFI's tables (CR3 = 0x7801000) are here
```

- The pages holding **UEFI's own tables are marked read-only** (`-r-`). It
  is the firmware's protection against corruption.
- The **WP** bit (*write protect*, bit 16) of CR0 is set (`0x80010033`):
  with it, not even the kernel (ring 0) can write to read-only pages.

Any attempt to change UEFI's tables would cause a page fault. And even if it
worked, they live in `BootServices` memory, which we want to reuse one day.
**The kernel needs its own tables.**

The other values confirmed the environment: CR4 without bit 12 (no 5-level
paging) and EFER with bit 11 (NXE on).

---

## 5. The design of SiliconOS's tables

### 5.1 The problem with the switch

Switching tables is like changing a tire while the car is moving. At the
instant of `mov cr3`, the CPU starts translating **everything** with the new
tables, including:

- the address of the kernel's **next instruction**;
- the **stack** (local variables, return addresses);
- the GDT, the IDT, the TSS and the emergency stack, which the CPU looks up
  on any exception;
- the framebuffer and the allocator's bitmap.

If any of them is not mapped in the new tables, **at the same virtual
address as before**, the kernel falls over on the spot. And the page fault
handler is also in memory that has to be mapped...

### 5.2 The solution: three regions

```
 virtual space (48 bits)
 0xFFFF_FFFF_FFFF_FFFF ┐
                       │
 0xFFFF_8000_0000_0000 ├─ (3) ALL of physical RAM again: virtual = OFFSET + physical
                       │     this is how the kernel accesses physical memory from now on
                       │
       ╳ non-canonical hole ╳
                       │
 0x0000_0000_8000_0000 ├─ (2) framebuffer (identity)
                       │
 0x0000_0000_0000_1000 ├─ (1) ALL of RAM in identity: virtual = physical
 0x0000_0000_0000_0000 ┘     ...except page 0 (null pointer → page fault)
```

**(1) Identity.** Keeps valid every address the kernel is already using
(code, stack, statics), because UEFI loaded the kernel and created the stack
at physical addresses. It is what allows switching "while the car is
moving". It is **temporary scaffolding**: when there are ordinary programs,
the lower half will be theirs, and the kernel will have to move out.

**(2) Framebuffer.** It is not RAM, so it does not appear in the memory map
(doc 05). Mapped separately, in identity.

**(3) All of RAM in the upper half**, starting at
`DESLOCAMENTO_FISICO = 0xFFFF_8000_0000_0000` (*physical offset*). This is
the permanent part of the design, called *offset mapping*. For the kernel to
access physical frame `F` (a page table, the bitmap, a frame it has just
allocated), it is enough to use the pointer `DESLOCAMENTO_FISICO + F`.
Almost every 64-bit kernel does something similar. Linux calls it the
*direct map*.

### 5.3 Page 0 with no mapping

The first 2 MiB are mapped in 4 KiB pages, **skipping page 0**:

```rust
for p in 1..512u64 {      // starts at 1, not 0
    mapear_em(pml4, p * TAMANHO_QUADRO, p * TAMANHO_QUADRO, TamanhoPagina::K4, rw).unwrap();
}
```

The result: **reading or writing through a null pointer now causes a page
fault**. With UEFI's tables, address 0 was valid memory, and a null pointer
bug would go unnoticed, silently corrupting data. (It is the same reason
your Java or C program on Linux gets a `NullPointerException` / *segmentation
fault* instead of reading garbage.)

### 5.4 RAM in 2 MiB pages

```rust
for r in mapa.regioes().iter().filter(|r| !matches!(r.tipo, Tipo::Mmio | Tipo::Reservada)) {
    let mut a = r.inicio & !(DOIS_MIB - 1);                  // round down
    let fim = (r.fim() + DOIS_MIB - 1) & !(DOIS_MIB - 1);    // round up
    while a < fim {
        if a != 0 && traduzir_em(pml4, a).is_none() {
            mapear_em(pml4, a, a, TamanhoPagina::M2, rw).unwrap();
        }
        if traduzir_em(pml4, DESLOCAMENTO_FISICO + a).is_none() {
            mapear_em(pml4, DESLOCAMENTO_FISICO + a, a, TamanhoPagina::M2, rw).unwrap();
        }
        a += DOIS_MIB;
    }
}
```

For each RAM region of the map (doc 05), each 2 MiB block is mapped twice:
in identity and in the upper half. Details:

- **`x & !(DOIS_MIB - 1)`** rounds `x` down to a multiple of 2 MiB (clears
  the low 21 bits; `DOIS_MIB` = *two MiB*). Adding `DOIS_MIB - 1` first
  makes it round up. It is the standard alignment trick with powers of 2.
- The **`traduzir_em(...).is_none()`** avoids mapping the same block twice:
  several small regions of the map fall inside the same 2 MiB block.
- Block 0 (`a != 0`) is skipped in the identity because it was already done
  in 4 KiB pages (section 5.3), but **not** in the upper half: there,
  physical 0 is not a null pointer.
- Rounding to 2 MiB blocks may map a few KiB of "hole" next to a RAM
  region. It is harmless, because the kernel never uses those addresses
  (the allocator only hands out what the map says is free).

### 5.5 Creating tables on demand: `mapear_em`

```rust
pub fn mapear_em(raiz: u64, virt: u64, fisico: u64, tamanho: TamanhoPagina, bits: u64)
    -> Result<(), ErroMapeamento>
{
    // ... checks alignment and whether the address is canonical ...
    let idx = indices(virt);
    let folha = tamanho.nivel_folha();       // leaf level: 1 for 1 GiB, 2 for 2 MiB, 3 for 4 KiB
    let mut tabela = raiz;

    for nivel in 0..folha {                  // go down to the leaf level
        let e = &mut tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() {
            let nova = nova_tabela().ok_or(ErroMapeamento::SemMemoria)?;    // new table
            *e = Entrada(nova | PRESENTE | ESCRITA | (bits & USUARIO));
        } else if e.grande() {
            return Err(ErroMapeamento::JaMapeado);
        }
        tabela = e.endereco();
    }

    let e = &mut tabela_em(tabela).entradas[idx[folha]];
    if e.presente() { return Err(ErroMapeamento::JaMapeado); }
    let grande = if tamanho == TamanhoPagina::K4 { 0 } else { GRANDE };
    *e = Entrada(fisico | bits | grande | PRESENTE);
    invalidar(virt);                         // invlpg: remove from the TLB
    Ok(())
}
```

It is the walk from section 2.3 with one difference: **when a level is
missing, it is created**. `nova_tabela()` asks the allocator from doc 05 for
a frame and zeroes it (a zeroed table = 512 "not present" entries). This is
where the two chapters meet: page tables are made of physical frames.

The return type is a `Result` ([rust-basico §6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions))
with an `enum` of possible errors:

```rust
// Misaligned, NonCanonical, AlreadyMapped, NotMapped, OutOfMemory
pub enum ErroMapeamento { Desalinhado, NaoCanonico, JaMapeado, NaoMapeado, SemMemoria }
```

The `?` after `ok_or(...)` means "if it is an error, return that error
now" ([rust-basico §24](rust-basico.md#24-the--operator-propagating-errors)).

`desmapear_em` does the walk, zeroes the final entry, calls `invlpg` and
returns the physical address that was mapped (so the caller can free the
frame). Intermediate tables that end up empty are **not** returned to the
allocator for now; it is a small waste, noted in the code.

### 5.6 Accessing a table through its physical address

The entries hold **physical** addresses, but the code can only use
**virtual** addresses. The bridge is this function:

```rust
static DESLOCAMENTO_ATUAL: AtomicU64 = AtomicU64::new(0);      // current offset

pub fn fisico_para_virtual(fisico: u64) -> u64 {               // physical to virtual
    fisico + DESLOCAMENTO_ATUAL.load(Ordering::Relaxed)
}

fn tabela_em(fisico: u64) -> &'static mut Tabela {
    unsafe { &mut *(fisico_para_virtual(fisico) as *mut Tabela) }
}
```

The elegant detail: the offset **changes in the middle of execution**.

- While the tables are being built, UEFI's tables (identity) are still
  active. Offset = **0**: physical F is accessed through pointer F.
- After the switch, the offset becomes `DESLOCAMENTO_FISICO`: physical F is
  accessed through pointer `0xFFFF_8000_0000_0000 + F`.

The same code (`mapear_em`, `traduzir_em`, `nova_tabela`) works at both
moments without knowing it. The variable is an `AtomicU64`
([rust-basico §25](rust-basico.md#25-atomics-atomicu64-atomicbool)) because
it is a global changed at run time.

### 5.7 The switch

```rust
// 4. The switch
unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };

// 5. From here on, physical memory is accessed through the upper half.
DESLOCAMENTO_ATUAL.store(DESLOCAMENTO_FISICO, Ordering::Relaxed);
quadros::reposicionar_bitmap(DESLOCAMENTO_FISICO);
```

One instruction. Writing to CR3 also empties the TLB, so UEFI's old
translations do not survive.

`reposicionar_bitmap` (*reposition bitmap*) settles the loose end left in
doc 05: the allocator's bitmap was accessed through its physical address
(identity). Now the slice is recreated pointing to the upper half. It would
still work through the identity, but this way the allocator no longer
depends on the temporary scaffolding.

---

## 6. How it was tested

### 6.1 Before and after, at boot

The kernel prints the translation of some addresses with UEFI's tables and
then with its own. The numbers in brackets are the 4 indices (PML4, PDPT,
PD, PT):

```
Paginacao: tabelas do UEFI (CR3=0x7801000)
  codigo   0x00000000060711a0 [  0   0  48 113] -> 0x60711a0 (2 MiB)
  pilha    0x0000000007e96150 [  0   0  63 150] -> 0x7e96150 (4 KiB)
  nulo     0x0000000000000000 [  0   0   0   0] -> 0x0 (2 MiB)
Paginacao: tabelas do KERNEL ativas (CR3 0x7801000 -> 0x101000, 7 quadros usados)
  codigo   0x00000000060711a0 [  0   0  48 113] -> 0x60711a0 (2 MiB)
  pilha    0x0000000007e96150 [  0   0  63 150] -> 0x7e96150 (2 MiB)
  nulo     0x0000000000000000 [  0   0   0   0] -> NAO MAPEADO
  fis 1MiB 0xffff800000100000 [256   0   0 256] -> 0x100000 (2 MiB)
```

(`codigo` = *code*, `pilha` = *stack*, `nulo` = *null*, `NAO MAPEADO` =
*NOT MAPPED*, `fis` = *physical*; "KERNEL tables active (..., 7 frames
used)".)

How to read it:

- **code and stack** still translate to the same physical address: the
  switch was transparent. UEFI used 4 KiB pages near the stack; we use
  2 MiB.
- **null**: it used to be valid memory; now it causes a page fault.
- **fis 1MiB**: address `0xFFFF_8000_0010_0000` (index 256 in the PML4, the
  first entry of the upper half) reaches physical `0x100000`, where the
  bitmap lives.
- The whole kernel, with 128 MiB of RAM, needed only **7 frames** (28 KiB)
  of tables, thanks to the 2 MiB pages.

### 6.2 From QEMU's point of view

The QEMU monitor (`info mem`), which reads the tables without going through
our code, showed exactly the three regions of section 5.2:

```
CR3=0000000000101000
0000000000001000-0000000008000000 0000000007fff000 -rw     ← (1) identity, without page 0
0000000080000000-0000000080400000 0000000000400000 -rw     ← (2) framebuffer (4 MiB)
ffff800000000000-ffff800008000000 0000000008000000 -rw     ← (3) RAM in the upper half
```

### 6.3 `./run.sh teste-paginacao`

1. maps an unused virtual address (`0x5000_0000_0000`, 80 TiB) to a new
   frame and checks that `traduzir` returns that frame, with a 4 KiB page;
2. **writes through that address and reads the same data through the upper
   half**: two different virtual paths, the same physical frame;
3. maps a **second** virtual address to the same frame (*aliasing*) and
   checks that what is written through one shows up through the other;
4. checks the errors: mapping again → `JaMapeado`; unaligned address →
   `Desalinhado`; non-canonical address → `NaoCanonico`;
5. unmaps both, checks that they no longer translate and that unmapping
   again gives `NaoMapeado`, and frees the frame.

```
Teste de paginacao OK: mapear, traduzir, escrever/ler por 2 caminhos,
  aliasing, erros e desmapear. Quadros em tabelas novas: 3
```

("Paging test OK: map, translate, write/read through 2 paths, aliasing,
errors and unmap. Frames in new tables: 3")

The 3 frames are the PDPT, the PD and the PT created for the test address
(nothing else existed in that region).

### 6.4 `./run.sh teste-desmapeado` and `./run.sh teste-nulo`

```
Teste: escrevi em 0x500000000000; desmapeando e lendo de novo...
Mensagem: EXCECAO #14 PAGE FAULT ao ler 0x500000000000 (pagina nao mapeada, modo kernel)

Teste: lendo o endereco 0 (ponteiro nulo)...
Mensagem: EXCECAO #14 PAGE FAULT ao ler 0x0 (pagina nao mapeada, modo kernel)
```

("Test: I wrote to 0x500000000000; unmapping and reading again..." / "Test:
reading address 0 (null pointer)...")

The first proves that `desmapear` + `invlpg` work: the page had been used
(and was in the TLB), and even so the next read failed.

The null pointer test uses **assembly** (`mov rax, [0]`) on purpose. In
Rust, reading a null pointer is *undefined behavior*: the compiler assumes
it never happens and may generate any code at all (including removing the
read). In debug mode, Rust would still check the pointer and panic before
the CPU tried the access. To test the **CPU**, you have to talk to it
directly.

### 6.5 With more memory

| RAM | Table frames | Note |
|---|---|---|
| 128 MiB | 7 | — |
| 4 GiB | 13 | code and stack in PDPT 2 (between 2 and 3 GiB) |
| 8 GiB | 21 | **code above 4 GiB** (`0x140009a00`, PDPT 5) and stack below: the identity covers both |

`teste-paginacao` passed in all three configurations.

### 6.6 All the previous tests

The 12 tests from the previous chapters (panic, the 6 exceptions, frames,
double free) passed with the new tables. In particular, the **double fault
still works**: the IST1 stack is a kernel `static`, covered by the
identity.

---

## Difference from the book

Judging by its table of contents, chapter 7 of the book covers the same
topics: pages, frames and translation (7.1), mapping and unmapping (7.2),
the *higher-half*, *offset mapping* and identity strategies (7.3), and how
to debug page faults and the TLB (7.4). Since the book starts from a
ready-made bootloader, its starting point is probably different from ours.
It is worth comparing as you read.

SiliconOS's particularities:

- it starts from **UEFI's** tables, which are **read-only** (section 4), and
  so **builds its own tables from scratch** and switches CR3;
- it builds everything by hand, without the `x86_64` crate;
- it uses **identity + offset mapping** (section 5.2). The kernel's code
  still runs at low addresses, because UEFI loaded it there. Moving the
  kernel to the upper half (*higher-half kernel*) is left for when ordinary
  programs need the lower half.

---

## Summary

- Every pointer is a **virtual address**; the CPU translates it into a
  **physical** one by walking **4 levels of tables** (PML4 → PDPT → PD →
  PT), starting from **CR3**. Each table is a 4 KiB frame with 512 entries.
- The virtual address is chopped into **4 indices of 9 bits + 12 bits of
  offset**. **2 MiB** and **1 GiB** pages stop the walk earlier.
- Each entry has a physical address and control bits (present, write, user,
  NX...). The **TLB** caches translations; when changing an entry, you need
  `invlpg`.
- UEFI's tables are read-only; SiliconOS builds its own with **identity**
  (scaffolding for the switch), the **framebuffer** and **all of RAM in the
  upper half** (`0xFFFF_8000_0000_0000 + physical`), without **page 0**.
- `fisico_para_virtual` uses an offset that is 0 before the switch and
  `DESLOCAMENTO_FISICO` after it, so the same code works at both moments.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **What are the 4 indices of address `0xFFFF_8000_0010_0000`?**
   PML4 = 256, PDPT = 0, PD = 0, PT = 256 (`0x100000 >> 12 = 256`). It is
   what the boot shows on the "fis 1MiB" line.

2. **Why does each table have exactly 512 entries?**
   Because each index has 9 bits (2⁹ = 512), and 512 entries × 8 bytes =
   4096 bytes, exactly one frame.

3. **The kernel changes a PT entry to point to another frame, but keeps
   reading the old data. What was missing?**
   The `invlpg`: the old translation was still in the TLB.

4. **Why did SiliconOS not add its mappings to UEFI's tables?**
   Because the pages holding them are marked read-only, and CR0's WP bit
   makes the CPU respect that even in ring 0. Besides, they live in
   BootServices memory.

5. **What is the identity for, if the kernel starts accessing physical
   memory through the upper half?**
   So that the code, the stack and the CPU tables (GDT, IDT, TSS), which
   are at low physical addresses, stay valid at the instant CR3 is
   switched. It is temporary scaffolding.

6. **How many PT tables would mapping 8 GiB with 4 KiB pages cost? And with
   2 MiB pages?**
   8 GiB / 4 KiB = 2,097,152 pages, that is, 4096 PTs (16 MiB). With 2 MiB,
   no PT at all: just 8 PDs (8 GiB / 1 GiB per PD).

</details>
