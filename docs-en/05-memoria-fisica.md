# 05: Physical memory: knowing what belongs to whom and handing out pieces

> **You will understand:** how the kernel finds out how much RAM there is
> and where it is; why a PC's memory is full of "holes"; what a 4 KiB
> *frame* is; and how a *bitmap*-based **frame allocator** works, the piece
> on which all memory management will be built.
>
> **Files:** `src/memoria/mod.rs`, `src/memoria/mapa.rs`, `src/memoria/quadros.rs`
> (called in `kernel_main` right after the IDT)
> **Book:** chapter 6
> **Try it:** `./run.sh teste-quadros`, `./run.sh teste-liberacao-dupla`,
> `./run.sh listar-regioes`, and with more memory: `RAM=4G ./run.sh teste-quadros`
> **New Rust here:** `enum` ([§19](rust-basico.md#19-simple-enum)),
> iterator chains ([§20](rust-basico.md#20-chained-iterators-filter-map-fold)),
> slices ([§21](rust-basico.md#21-slices-a-piece-of-an-array)),
> *newtype* ([§22](rust-basico.md#22-newtype-a-type-of-its-own-for-a-number)),
> `assert!` and `expect` ([§23](rust-basico.md#23-assert-and-expect-checks-that-become-a-panic))
>
> **Names in the code:** `memoria` = *memory*, `mapa` = *map*, `quadro(s)` =
> *frame(s)*, `regiao/regioes` = *region(s)*, `inicio`/`fim` = *start*/*end*,
> `livre(s)` = *free*, `ocupado` = *used*, `alocar`/`liberar` =
> *allocate*/*free*.

---

## 1. The problem

So far the kernel has only used memory that **others** set aside for it:

- the code and the `static` variables (GDT, IDT, emergency stack) came
  inside the executable, and the firmware loaded them;
- the stack the kernel runs on was created by the firmware.

For everything that lies ahead (page tables in chapter 7, `Vec` and
`String` in chapter 8, a stack per process later), the kernel needs to
**ask for memory at run time**. In Java, you write `new` and the JVM asks
the operating system for memory. Here **we are the operating system**: we
have to answer the question "which piece of RAM is free right now?".

That takes two things, which are the two parts of this chapter:

1. **Knowing what exists** (the map): how much RAM there is, at which
   addresses, and what is already taken by firmware, devices or the kernel
   itself.
2. **Keeping track of what was handed out** (the allocator): handing free
   pieces to whoever asks, taking them back, and never handing the same
   piece to two owners.

---

## 2. A PC's memory is not one contiguous block

The first intuition is to imagine that a machine with 128 MiB has
addresses from 0 to 128 MiB, all of them RAM. It is not like that. The
**physical address** space is shared between RAM, devices and firmware.
This is QEMU's real map with 128 MiB, produced by
`./run.sh listar-regioes` (abridged):

```
0x000000000000 - 0x000000001000        4 KiB  BootServices
0x000000001000 - 0x0000000a0000      636 KiB  Livre
               (0xa0000 - 0x100000: nothing in the map: the original PC's hole)
0x000000100000 - 0x000000800000     7168 KiB  Livre
0x000000800000 - 0x000000808000       32 KiB  ACPI
...                                   (dozens of regions alternating types)
0x000006070000 - 0x0000060a6000      216 KiB  Kernel       ← SiliconOS
...
0x000007f78000 - 0x000008000000      544 KiB  ACPI         ← end of RAM: 128 MiB
               (0x80000000: the framebuffer, outside the map)
0x00fd00000000 - 0x010000000000  12582912 KiB  Reservada   ← 12 GiB near 1 TiB: not RAM
```

(`Livre` = *free*, `Reservada` = *reserved*.)

Three things to notice:

### 2.1 There are holes

Between `0xA0000` (640 KiB) and `0x100000` (1 MiB) **there is nothing in
the map**. That range is a legacy of the 1981 IBM PC: the video card's
memory and the ROMs used to live there, and it is still reserved today.
Addresses that do not appear in the map **are not RAM** and must never be
used.

### 2.2 There are devices in the middle of the addresses

The framebuffer (doc 01) is at `0x80000000`, a **physical** address that is
not RAM: writing there goes straight to the graphics card (that is *MMIO*,
see the [glossary](glossario.md#memory)). That is why it shows up as
"outside the map".

The 12 GiB "Reservada" region near 1 TiB is not RAM either: it is a range
of addresses QEMU reserves for the chipset. It appears in the map precisely
to say "do not use".

### 2.3 With more memory, the holes get bigger

With `RAM=4G ./run.sh listar-regioes`, the end of the map looks like this:

```
0x0000bff78000 - 0x0000c0000000      544 KiB  ACPI          ← RAM up to 3 GiB
               (0xc0000000 - 0x100000000: 1 GiB hole for PCI devices;
                the framebuffer is here, at 0xc0000000)
0x000100000000 - 0x000140000000  1048576 KiB  Livre         ← the remaining 1 GiB, above 4 GiB
```

PCs reserve the last piece below 4 GiB for devices (the "PCI window"). The
RAM that would fit there **is remapped to above 4 GiB**. The result: a
machine with 4 GiB of RAM has RAM addresses up to **5 GiB**. The allocator
must handle this.

---

## 3. Part 1: the kernel's memory map

### 3.1 What UEFI hands over

In doc 00 we saw that `exit_boot_services` returns the **memory map**: a
list of *descriptors*, each with a start address, a number of 4 KiB pages
and a **type**. UEFI defines about 16 types. The ones that show up in QEMU:

| UEFI type | What it is | Can we use it? |
|---|---|---|
| `CONVENTIONAL` | free RAM | **yes** |
| `BOOT_SERVICES_CODE/DATA` | RAM used by the firmware during boot | in theory yes, after `exit_boot_services`... (section 3.4) |
| `LOADER_CODE/DATA` | the program UEFI loaded (SiliconOS) and what it allocated | **no**: it is the kernel |
| `RUNTIME_SERVICES_CODE/DATA` | parts of the firmware that stay active (clock, variables) | **never** |
| `ACPI_RECLAIM`, `ACPI_NON_VOLATILE` | tables describing the hardware | no (not yet) |
| `RESERVED`, `MMIO`... | reserved, devices | **never** |

### 3.2 Why copy into a format of our own

The UEFI map has three drawbacks:

1. **It depends on the `uefi` crate** and on a buffer the firmware
   allocated. We want the rest of the kernel to work with our own types.
2. **It does not necessarily come in address order** (the specification
   does not guarantee it).
3. **It is fragmented:** 127 entries in QEMU, many of them neighbors of the
   same type.

So `memoria::mapa::init` does three steps: copy while converting the type,
sort, and merge neighbors. The result at boot:

```
Mapa de memoria: 127 entradas do UEFI -> 31 regioes (vizinhas unidas)
```

("Memory map: 127 entries from UEFI -> 31 regions (neighbors merged)")

### 3.3 The code

**The simplified types** are an `enum` ([rust-basico §19](rust-basico.md#19-simple-enum)):

```rust
pub enum Tipo {                  // Tipo = Type
    Livre, BootServices, Kernel, Runtime, Acpi, Mmio, Reservada,
}
```

and the conversion is a `match` ([rust-basico §7](rust-basico.md#7-match-a-switch-that-forces-you-to-cover-everything)):

```rust
fn de_uefi(t: MemoryType) -> Tipo {          // "from UEFI"
    match t {
        MemoryType::CONVENTIONAL => Tipo::Livre,
        MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => Tipo::BootServices,
        MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => Tipo::Kernel,
        ...
        _ => Tipo::Reservada,      // any unknown type: when in doubt, do not use
    }
}
```

The `_ => Tipo::Reservada` is a safety decision: if some firmware reports a
type we do not know, we treat it as "do not touch".

**A region** is start + size + type:

```rust
pub struct Regiao {
    pub inicio: u64,     // physical address of the first byte
    pub quadros: u64,    // size, in 4 KiB blocks
    pub tipo: Tipo,
}

impl Regiao {
    pub fn fim(&self) -> u64 {               // first address AFTER the region
        self.inicio + self.quadros * TAMANHO_QUADRO
    }
    pub fn contem(&self, endereco: u64) -> bool {      // "contains"
        endereco >= self.inicio && endereco < self.fim()
    }
}
```

The interval is **half-open**, `[start, end)`: it includes the start and
excludes the end, like `substring(start, end)` in Java. That avoids
off-by-one errors.

**The map** keeps the regions in a fixed-size array, because we do not have
`Vec` yet (it needs an allocator, exactly what we are building):

```rust
const MAX_REGIOES: usize = 512;   // OVMF hands over ~130

pub struct Mapa {
    regioes: [Regiao; MAX_REGIOES],
    qtd: usize,                    // how many positions are in use (qtd = count)
    pub entradas_uefi: usize,
}
```

It is the same pattern as an `ArrayList` on the inside: a big array and a
counter. The `regioes()` method returns only the used part, like a
`subList(0, qtd)` ([rust-basico §21](rust-basico.md#21-slices-a-piece-of-an-array)).

**The three steps of `init`:**

```rust
// 1. Copy, converting the type.
for d in mapa_uefi.entries() {
    assert!(m.qtd < MAX_REGIOES, "mapa de memoria com mais de {} regioes", MAX_REGIOES);
    m.regioes[m.qtd] = Regiao { inicio: d.phys_start, quadros: d.page_count, tipo: Tipo::de_uefi(d.ty) };
    m.qtd += 1;
}

// 2. Sort by address.
m.regioes[..m.qtd].sort_unstable_by_key(|r| r.inicio);

// 3. Merge neighbors of the same type: [A][A][B][A] -> [AA][B][A]
let mut escrita = 0;                         // escrita = write
for leitura in 0..m.qtd {                    // leitura = read
    let r = m.regioes[leitura];
    if escrita > 0 {
        let anterior = &mut m.regioes[escrita - 1];     // anterior = previous
        if anterior.tipo == r.tipo && anterior.fim() == r.inicio {
            anterior.quadros += r.quadros;     // extend the previous one
            continue;                          // and drop this one
        }
    }
    m.regioes[escrita] = r;
    escrita += 1;
}
m.qtd = escrita;
```

Step 3 uses the **two-index** technique on the same array: `leitura` walks
through everything; `escrita` marks how far the result goes. When the
current region "touches" the previous one (same type, and the previous one
ends exactly where this one begins), it is absorbed. That way the array is
compacted in place, with no need for a second array.

The map lives in a `Once` ([rust-basico §11](rust-basico.md#11-mutex-and-once-from-the-spin-crate)),
for the same reason as the GDT: it is built once and must live forever.

### 3.4 A discovery: the stack is in "firmware" memory

The boot prints where the things the kernel is using are:

```
  codigo do kernel 0x607dee0 -> Kernel
  GDT              0x60a2070 -> Kernel
  pilha atual(RSP) 0x7e95e00 -> BootServices
  framebuffer      0x80000000 -> fora do mapa
```

(`codigo do kernel` = *kernel code*, `pilha atual` = *current stack*,
`fora do mapa` = *outside the map*.)

The code and the GDT are in the "Kernel" region, as expected. But **the
stack the kernel is running on right now is in a `BootServices` region**:
the firmware created it to run our program, with the type "firmware data".

The UEFI specification says that, after `exit_boot_services`,
`BootServices` memory may be reused by the operating system. If the
allocator handed out that memory, **it could hand out the kernel's own
stack** for another use. The next owner would write over the local
variables and return addresses, and the kernel would break in completely
mysterious ways.

That is why, **for now, the allocator uses only `Livre` regions**. The 40
MiB of `BootServices` will be reclaimed once the kernel has a stack of its
own (that comes after paging). This is a good example of why the kernel
needs to **know where its own things are** before it starts handing out
memory.

---

## 4. Part 2: the frame allocator

### 4.1 4 KiB frames

The allocator does not hand out loose bytes; it hands out **frames**:
blocks of **4 KiB** (4096 bytes), aligned, that is, starting at an address
that is a multiple of 4096.

Why 4 KiB? Because it is the size of an x86 paging **page** (chapter 7):
the CPU translates addresses 4 KiB at a time. All of an x86 kernel's memory
management is done in this unit. Smaller pieces (a 20-byte `String`) will
be cut from inside frames by a second allocator, the *heap*, in chapter 8.

```
physical address:  0x0000    0x1000    0x2000    0x3000    0x4000
                   ├─────────┼─────────┼─────────┼─────────┤
                   │ frame 0 │ frame 1 │ frame 2 │ frame 3 │ ...
                   └─────────┴─────────┴─────────┴─────────┘
frame number = address / 4096        address = number × 4096
```

### 4.2 The bitmap idea

We need to remember, for each frame, whether it is free or used. The
simplest way is a **bitmap**: **one bit per frame**, 1 = used, 0 = free.

```
bitmap[0] (u64) = ...0000 0000 0000 0111  → frames 0,1,2 used; 3,4,5... free
bitmap[1] (u64) = ...                      → frames 64 to 127
```

The bits are grouped into 64-bit numbers (`u64`). Frame `q` is in word
`q / 64`, at bit `q % 64`.

Cost: 1 bit for every 4096 bytes, that is, **1/32,768 of the memory**.

| RAM covered | Frames | Bitmap |
|---|---|---|
| 128 MiB | 32,768 | 4 KiB |
| 5 GiB (a 4 GiB machine, section 2.3) | 1,310,720 | 160 KiB |
| 9 GiB (an 8 GiB machine) | 2,359,296 | 288 KiB |

These are the real sizes the boot showed.

There are alternatives (linked lists of free frames, the *buddy
allocator*), which are faster at allocating but harder to understand and
debug. The bitmap is the classic starting point: simple, compact and easy
to inspect.

### 4.3 Where to keep the bitmap itself

The bitmap of an 8 GiB machine is 288 KiB. Where do we put that, if we do
not have an allocator yet? It is the chicken-and-egg problem. The classic
solution:

1. compute the size of the bitmap;
2. look for the **first Livre region** it fits in entirely;
3. put the bitmap at its start;
4. mark those frames as **used** in the bitmap itself.

The bitmap lives inside the memory it manages and protects itself by
marking itself as used.

```rust
let regiao = mapa
    .regioes()
    .iter()
    .filter(|r| r.tipo == Tipo::Livre)
    .find(|r| {
        let inicio = r.inicio.max(INICIO_UTILIZAVEL);      // first usable address
        inicio + quadros_bitmap * TAMANHO_QUADRO <= r.fim()
    })
    .expect("nenhuma regiao livre comporta o bitmap de quadros");   // "no free region fits the frame bitmap"
```

This `.filter().find()` chain is like a Java `stream()`: "of the regions,
filter the free ones and find the first one the bitmap fits in"
([rust-basico §20](rust-basico.md#20-chained-iterators-filter-map-fold)).

At boot: `Alocador de quadros: bitmap de 4 KiB em 0x100000` ("Frame
allocator: 4 KiB bitmap at 0x100000"). That is exactly the start of the
first free region above 1 MiB.

### 4.4 Turning a physical address into a Rust variable

The bitmap is at a physical address, for example `0x100000`. To use it as
an ordinary Rust array, we convert the address into a **slice**:

```rust
let bitmap: &'static mut [u64] = unsafe {
    core::slice::from_raw_parts_mut(endereco_bitmap as *mut u64, palavras)   // palavras = words
};
```

`from_raw_parts_mut(pointer, count)` says: "starting at this address,
there are `count` `u64` values; treat this as an array". From here on,
`bitmap[i]` works with bounds checking, like any array.

It is `unsafe` because the compiler has no way of knowing whether that
memory exists and whether nobody else uses it. We are the ones
guaranteeing it: it is a `Livre` region of the map, and we will mark it as
used right after.

> **Why does using a physical address as a pointer work?** Pointers in the
> CPU are **virtual** addresses, translated by paging. UEFI left paging in
> **identity mapping**: virtual address X points to physical address X.
> That is why, today, "physical address" and "pointer" are the same thing.
> This changed in the following chapter: when the kernel builds its own
> page tables, the bitmap starts being accessed through the upper half of
> the address space ([doc 06, section 5.7](06-paginacao.md#57-the-switch)).

### 4.5 Initialization: start with everything used

```rust
// 3. Start with EVERYTHING used. Only what we prove to be free becomes 0.
bitmap.fill(u64::MAX);
```

`u64::MAX` is a number with all 64 bits set to 1. Every frame starts
**used**. Then, only the map's `Livre` regions (above 1 MiB) are marked as
free, and finally the bitmap's own frames are marked as used again:

```rust
for r in mapa.regioes().iter().filter(|r| r.tipo == Tipo::Livre) {
    let inicio = r.inicio.max(INICIO_UTILIZAVEL);
    if inicio >= r.fim() { continue; }
    let primeiro = (inicio / TAMANHO_QUADRO) as usize;      // primeiro = first
    let n = ((r.fim() - inicio) / TAMANHO_QUADRO) as usize;
    a.marcar_intervalo(primeiro, n, false);                // "mark range"
}
a.marcar_intervalo((endereco_bitmap / TAMANHO_QUADRO) as usize, quadros_bitmap as usize, true);
```

Why start with "everything used" and not "everything free"? Because **the
two possible mistakes do not cost the same**:

- marking as used something that was free: **wastes** a little memory.
  Nobody notices;
- marking as free something that was used: the allocator **hands out
  someone's memory** (the firmware's, a device's, the kernel's own). That
  causes silent corruption, the worst kind of bug.

Starting with everything used guarantees that the holes, the devices, the
reserved regions and everything that is not in the map are never handed
out. Only what we are sure is free gets released.

**Why skip the first 1 MiB** (`INICIO_UTILIZAVEL`)? Two reasons:

1. frame 0 contains address 0. A null pointer pointing to "valid" memory
   would hide bugs;
2. when we start the other CPUs (chapter 11), they begin in 16-bit mode
   and only see the first megabyte. We will need free memory there.

The cost is negligible: 636 KiB.

### 4.6 Allocating: finding the first 0 bit

```rust
pub fn alocar(&mut self) -> Option<Quadro> {
    let palavras = self.bitmap.len();
    for i in 0..palavras {
        let p = (self.proxima_palavra + i) % palavras;     // proxima_palavra = next word
        let valor = self.bitmap[p];
        if valor != u64::MAX {                        // there is some 0 bit here
            let bit = valor.trailing_ones() as usize; // position of the first 0
            let quadro = p * 64 + bit;
            if quadro >= self.total { continue; }
            self.marcar(quadro, true);
            self.livres -= 1;
            self.proxima_palavra = p;
            return Some(Quadro(quadro as u64 * TAMANHO_QUADRO));
        }
    }
    None                                              // out of memory
}
```

The performance tricks:

- **Testing 64 frames at once:** if the whole word is `u64::MAX`, all 64
  frames are used; we skip it without looking bit by bit.
- **`trailing_ones()`** counts how many consecutive 1 bits there are
  starting from bit 0. That number is the position of the first 0 bit. The
  CPU has an instruction for this, so it costs a single cycle.
  ```
  valor = ...1111 0111   → trailing_ones = 3 → the free frame is bit 3
  ```
- **`proxima_palavra`** remembers where the last search stopped. Without
  it, every allocation would start over from the beginning and go through
  all the words that are already full. The `% palavras` makes the search
  "wrap around" at the end of the array.

The return type is `Option<Quadro>`: `Some(frame)` or `None` when memory
ran out. There is no `null` and no exception ([rust-basico §6](rust-basico.md#6-option-and-result-no-null-and-no-exceptions)):
the caller is forced to handle the "ran out" case.

### 4.7 The `Quadro` type

```rust
pub struct Quadro(pub u64);
```

A `Quadro` (*frame*) is just a `u64` with another name: the *newtype*
pattern ([rust-basico §22](rust-basico.md#22-newtype-a-type-of-its-own-for-a-number)).
The advantage is that the compiler **does not let you mix** a frame's
physical address with any other number (a size, an index, a virtual
address). From chapter 7 on we will have physical and virtual addresses
living side by side, and confusing them is a classic source of bugs in
kernels. The run-time cost is zero: in memory, a `Quadro` is exactly a
`u64`.

### 4.8 Freeing, with checks

```rust
pub fn liberar(&mut self, quadro: Quadro) {
    // "free: address ... not aligned to 4 KiB" / "is outside RAM" /
    // "frame ... was already free (double free)"
    assert!(quadro.0 % TAMANHO_QUADRO == 0, "liberar: endereco {:#x} nao alinhado em 4 KiB", quadro.0);
    let i = quadro.indice();
    assert!(i < self.total, "liberar: {:#x} esta fora da RAM", quadro.0);
    assert!(self.ocupado(i), "liberar: quadro {:#x} ja estava livre (liberacao dupla)", quadro.0);
    self.marcar(i, false);
    self.livres += 1;
    ...
}
```

The three checks catch the most common mistakes of whoever uses the
allocator. The most important is the **double free**: if a frame is freed
twice, it may end up handed to two different owners in the future, and one
will write over the other. That bug shows up far from its cause and is very
hard to track down. Detecting it **on the spot**, with a panic, turns a
mysterious bug into a clear message.

`assert!(condition, message)` is an `if !condition { panic!(message) }`
([rust-basico §23](rust-basico.md#23-assert-and-expect-checks-that-become-a-panic)).

### 4.9 The global allocator

Like the console (doc 01), the allocator must be reachable from anywhere,
so it lives in a `static Mutex<Option<...>>`:

```rust
pub static QUADROS: Mutex<Option<AlocadorQuadros>> = Mutex::new(None);

pub fn alocar() -> Option<Quadro> {
    QUADROS.lock().as_mut().expect("alocador de quadros nao inicializado").alocar()   // "not initialized"
}
```

The module-level `alocar()`, `liberar()` and `livres()` functions are
shortcuts that hide the `lock()` and the `Option`. Users just write
`memoria::quadros::alocar()`.

---

## 5. How it was tested

### 5.1 The normal boot

```
Alocador de quadros: bitmap de 4 KiB em 0x100000
  20613 quadros livres (80 MiB) de 32768 cobertos
```

("20613 free frames (80 MiB) of 32768 covered")

The map says 81 MiB free; the allocator offers 80 MiB. The difference is
the first megabyte (skipped on purpose) and the bitmap itself.

### 5.2 `./run.sh teste-quadros`

The test does five checks in sequence:

1. **allocates 1000 frames** and checks that the free counter went down by
   1000;
2. checks that each frame is **aligned** to 4 KiB, **above 1 MiB**, inside
   a **Livre** region of the map and **was not handed out twice**;
3. **writes a different pattern to each frame** (512 64-bit numbers,
   combining the frame number and the position) and then **reads it all
   back**. That proves the frames are real RAM, that identity mapping works
   and that no frame overlaps another: if two overlapped, the second
   pattern would erase part of the first;
4. **frees them all** and checks that the counter went back to its initial
   value;
5. **allocates again** and checks that the first frame handed out is the
   same as before (the search restarts from the right place after the
   frees).

```
Teste de quadros OK: 1000 alocados (0x101000..0x4e8000), escritos,
  conferidos, liberados e realocados. Livres: 20611
```

("Frame test OK: 1000 allocated (...), written, checked, freed and
reallocated. Free: 20611")

(The number of free frames varies by a few between different builds,
because the size of the kernel changes; what the test checks is that it
returns to the value it had at the start.)

### 5.3 `./run.sh teste-liberacao-dupla`

```
Teste: liberando o quadro 0x101000 duas vezes...
!!! KERNEL PANIC !!!
Local: src/memoria/quadros.rs:100:9
Mensagem: liberar: quadro 0x101000 ja estava livre (liberacao dupla)
```

("Test: freeing frame 0x101000 twice... / free: frame 0x101000 was already
free (double free)")

### 5.4 With more memory: `RAM=4G` and `RAM=8G`

`run.sh` gained the `RAM=...` option, which changes the emulated machine's
memory (QEMU's default is 128 MiB). `teste-quadros` passed in both cases:

| RAM | End of RAM | Bitmap | Free frames | Note |
|---|---|---|---|---|
| 128 MiB | 128 MiB | 4 KiB | 20,613 (80 MiB) | — |
| 4 GiB | **5 GiB** | 160 KiB | 1,036,381 (4048 MiB) | PCI hole from 3 to 4 GiB (section 2.3) |
| 8 GiB | **9 GiB** | 288 KiB | 2,084,925 (8144 MiB) | the firmware loaded the kernel **above 4 GiB** (`0x14000ba00`) |

The 4 GiB case confirms that the hole between 3 and 4 GiB stays marked as
used (it is covered by the bitmap, but was never "freed"). The 8 GiB case
shows that the kernel cannot assume it is at low addresses.

### 5.5 The old tests still pass

`teste-panic`, `teste-breakpoint` and `teste-doublefault` were run again,
with the same result as before.

---

## Difference from the book

The book receives the memory map from the **bootloader** it uses, in that
bootloader's own format. SiliconOS receives the map directly from **UEFI**,
and that is why it must convert UEFI's types and deal with the
`BootServices` memory issue (section 3.4).

The book may also present the allocator with another data structure (it is
common to start with an allocator that only moves forward and never frees).
SiliconOS uses the bitmap from the start because it already allows freeing
and is easy to inspect. The concepts (frames, regions, protecting what
belongs to the kernel) are the same.

---

## Summary

- The physical address space **is not just RAM**: there are holes inherited
  from the old PC, devices (MMIO), firmware and a PCI window below 4 GiB
  that pushes part of the RAM above 4 GiB.
- The kernel copies the UEFI map into a **format of its own**: 7 types,
  sorted, with neighbors merged.
- The current stack is in `BootServices` memory, so **for now only `Livre`
  regions are used**.
- The **frame allocator** hands out 4 KiB blocks using a **bitmap** (1 bit
  per frame). The bitmap lives in the first free region and marks itself
  as used.
- Safety principle: **start with everything used** and only free what you
  are sure is free. A double free becomes an immediate panic.

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **A machine has 4 GiB of RAM. Why does the allocator cover addresses up
   to 5 GiB?**
   Because the last GiB below 4 GiB is reserved for PCI devices, and the
   corresponding RAM is remapped to above 4 GiB.

2. **Why does the allocator not use `BootServices` memory yet, if the UEFI
   specification allows it?**
   Because the stack the kernel is running on is in a `BootServices`
   region. The allocator could hand out the kernel's own stack for another
   use.

3. **Why does the bitmap start with every bit set to 1 (used)?**
   Because marking a free frame as used only wastes memory, but marking a
   used one as free hands out someone's memory and causes corruption. When
   in doubt, "used".

4. **How big is the bitmap of a machine with 16 GiB of RAM (assuming the
   RAM goes up to 17 GiB)?**
   17 GiB / 4 KiB = 4,456,448 frames = 4,456,448 bits = 544 KiB.

5. **What does `trailing_ones()` return for `0b1011`, and what does that
   mean in the allocator?**
   2 (bits 0 and 1 are 1; bit 2 is 0). The frame at bit 2 of that word is
   the first free one.

6. **Why does using the bitmap's physical address as a pointer work today,
   and why will that change?**
   Because UEFI left paging in identity mapping (virtual = physical). When
   the kernel builds its own page tables (chapter 7), that equivalence no
   longer holds.

</details>
