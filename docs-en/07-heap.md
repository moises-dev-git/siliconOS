# 07: Heap: `Box`, `Vec` and `String` inside the kernel

> **You will understand:** why a `no_std` kernel has no `Vec` or `String`;
> what a heap is and how it builds on the frame allocator and paging; how
> two classic allocators work (the *bump* allocator and the *free list*);
> what fragmentation is; and how Rust connects all of this to a simple
> `Box::new`.
>
> **Files:** `src/memoria/heap.rs` (the real heap), `src/memoria/bump.rs`
> (the bump allocator, for comparison), `demonstrar_heap` and `testes_de_heap` in `src/main.rs`
> **Book:** chapter 8
> **Try it:** `./run.sh teste-heap`, `./run.sh teste-heap-liberacao-dupla`,
> `./run.sh teste-heap-esgotar`, `./run.sh teste-heap-executar`, `./run.sh teste-bump`
> **Read first:** doc 05 (frames) and doc 06 (paging)
> **New Rust here:** `alloc`, `Box`, `Vec`, `String` ([§26](rust-basico.md#26-alloc-box-vec-and-string)),
> `GlobalAlloc`, `Layout` and `#[global_allocator]` ([§27](rust-basico.md#27-globalalloc-layout-and-global_allocator))
>
> **Names in the code:** `livre(s)` = *free*, `no` = *node*, `proximo` =
> *next*, `anterior` = *previous*, `atual` = *current*, `procurar` =
> *search*, `inserir` = *insert*, `crescer` = *grow*, `alocar`/`liberar` =
> *allocate*/*free*, `em_uso` = *in use*, `pico` = *peak*, `mapeado` =
> *mapped*, `sobra` = *leftover*, `caixa(s)` = *box(es)*.
>
> **Code for this chapter:** [state at the end of doc 07](https://github.com/moises-dev-git/siliconOS/tree/doc-07) · [what changed since doc 06](https://github.com/moises-dev-git/siliconOS/compare/doc-06...doc-07) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

Until now, every kernel data structure had a **fixed size, decided at
compile time**:

- the memory map is an array of 512 regions (doc 05), even when the
  firmware hands over 130;
- the GDT has 7 lines, the IDT has 256;
- the page tables are frames requested one by one from the allocator.

That works for CPU tables, but not for what lies ahead: a list of processes
that grows and shrinks, the buffer of a line typed into the shell, a file
name. In Java or Python you do not even think about it: you write
`new ArrayList<>()` or `[]` and the list grows by itself. In Rust, that is
`Vec`, `String` and `Box`, and **they do not exist in a `no_std` kernel**
(doc 00, section 4.1).

Why? Because all of them need someone to answer the question **"give me N
bytes"** and, later, **"you can have these bytes back"**. In an ordinary
program, the standard library answers, by asking the operating system for
memory. Here **we are the operating system**.

We already have an allocator, the frame allocator (doc 05), but it only
hands out **4 KiB** blocks. A `Box<u64>` needs 8 bytes; a 20-byte `String`
needs 20. Using a whole frame for each would waste 99.8% of the memory. A
second layer is missing, one that cuts frames into pieces of the requested
size: the **heap**.

```
  Box::new, Vec::push, String::from     ← the kernel's code
              │  "give me 24 bytes aligned to 8"
              ▼
  heap (this doc)                       ← cuts pages into pieces of any size
              │  "I need more pages"
              ▼
  paging (doc 06)                       ← makes virtual pages point to frames
              │  "give me a frame"
              ▼
  frame allocator (doc 05)              ← hands out 4 KiB blocks of physical RAM
```

---

## 2. Where the heap lives

The heap is a **range of virtual addresses** reserved just for it.
SiliconOS chose to start it at `0xFFFF_9000_0000_0000`:

```
 0xFFFF_9000_0400_0000  ┐  ← limit: TAMANHO_MAXIMO = 64 MiB
                        │     (not mapped until needed)
 0xFFFF_9000_0010_0000  ┤  ← end of the initial heap (1 MiB)
                        │  initial heap, mapped at boot
 0xFFFF_9000_0000_0000  ┘  ← HEAP_INICIO  (PML4 entry 288)
        ...
 0xFFFF_8000_0000_0000  ←  direct map of RAM (doc 06; entries 256 and 257)
```

```rust
pub const HEAP_INICIO: u64 = 0xFFFF_9000_0000_0000;      // heap start
pub const TAMANHO_INICIAL: u64 = 1024 * 1024;            // initial size: 1 MiB
pub const CRESCIMENTO_MINIMO: u64 = 64 * 1024;           // minimum growth: 64 KiB
pub const TAMANHO_MAXIMO: u64 = 64 * 1024 * 1024;        // maximum size: 64 MiB
```

Why a virtual range of its own, and not pieces scattered across RAM?

- **Contiguous:** to the heap, the range is a single block of addresses,
  even if the physical frames behind it are scattered (that is the
  advantage of paging, doc 06, section 1).
- **Grows in place:** when space runs out, it is enough to map more pages
  right after the end. The blocks already handed out do not move.
- **Easy to recognize:** an address `0xFFFF_9...` in an error report is
  almost certainly heap memory.

### The heap does not run code: NX

The heap's pages are mapped with two bits:

```rust
const BITS_HEAP: u64 = flags::ESCRITA | flags::SEM_EXECUCAO;   // writable | no-execute
```

`SEM_EXECUCAO` is the **NX** bit (doc 06, section 3), used here for the
first time. The heap holds **data**, never code. If a bug (or an attack)
makes the CPU jump into the heap, it refuses to fetch the instruction and
raises a page fault. It is one of the most important protections of a
modern system: even if someone manages to write malicious bytes into
memory, they do not run. Section 7.5 shows the test.

### Mapping the range

```rust
pub(super) fn mapear_faixa(inicio: u64, quantidade: u64) -> u64 {   // "map range"
    let mut feito = 0;                                              // feito = done
    while feito < quantidade {
        match paginacao::alocar_e_mapear(inicio + feito, BITS_HEAP) {
            Ok(_) => feito += TAMANHO_QUADRO,
            Err(ErroMapeamento::SemMemoria) => break,
            Err(e) => panic!("heap: erro ao mapear {:#x}: {:?}", inicio + feito, e),
        }
    }
    feito
}
```

Page by page: it asks the allocator from doc 05 for a frame and maps the
virtual page to it (doc 06). If frames run out halfway, it returns how much
it managed; any other error (already mapped, misaligned) is a bug and
becomes a panic.

The heap uses **4 KiB** pages, not 2 MiB ones like the RAM mapping, to
grow in small steps without tying up physical memory for nothing.

---

## 3. The simplest allocator: *bump*

Before the real allocator, it is worth looking at the simplest one
possible, because it shows what is hard about the problem. The **bump**
allocator keeps a single pointer, `proximo` (*next*):

```
 heap:  [ used | used | used |          free            ]
                              ▲
                           proximo
 alloc(24):  hands out `proximo` and bumps it 24 bytes forward
 free(x):    does nothing
```

```rust
unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    let mut b = BUMP.lock();
    let inicio = alinhar_acima(b.proximo, layout.align());   // round up to the alignment
    let fim = match inicio.checked_add(layout.size()) {
        Some(f) => f,
        None => return ptr::null_mut(),
    };
    if b.proximo == 0 || fim > b.fim {
        return ptr::null_mut();     // not initialized, or ran out
    }
    b.proximo = fim;
    b.alocacoes += 1;
    inicio as *mut u8
}

unsafe fn dealloc(&self, _ponteiro: *mut u8, _layout: Layout) {
    // There is no way to give it back: bump only knows how to move forward.
    BUMP.lock().liberacoes += 1;
}
```

Two integer operations per allocation: it is the fastest allocator there
is. The problem is in `dealloc`: **freed memory is never used again**. The
`./run.sh teste-bump` test swaps the kernel's allocator for the bump one and
creates and destroys one 1 KiB box at a time. At no moment is there more
than one live box:

```
Teste bump: cria e destroi uma caixa de 1 KiB por vez.
  So uma caixa existe a cada momento, mas a memoria nunca volta:
   250 caixas:  250 KiB consumidos (254 alocacoes, 254 liberacoes ignoradas)
   500 caixas:  500 KiB consumidos (504 alocacoes, 504 liberacoes ignoradas)
   750 caixas:  750 KiB consumidos (754 alocacoes, 754 liberacoes ignoradas)
  1000 caixas: 1000 KiB consumidos (1004 alocacoes, 1004 liberacoes ignoradas)
!!! KERNEL PANIC !!!
Mensagem: memory allocation of 1024 bytes failed
```

("Bump test: creates and destroys one 1 KiB box at a time. / Only one box
exists at any moment, but the memory never comes back: / 250 boxes: 250
KiB consumed (254 allocations, 254 frees ignored)...")

(The 4 extra allocations come from the boot demonstration, section 7.1.)

The kernel needed 1 KiB and used up 1 MiB. A kernel runs for days; an
allocator that does not reuse memory is unacceptable.

Bump is still useful in special cases: when everything is freed **at once**
(a temporary region that is thrown away whole), or when the program only
allocates and never frees (like the start of boot). A common variant
resets the pointer when the count of live allocations reaches zero.

---

## 4. The real allocator: a free list

### 4.1 The idea

To reuse memory, the allocator must **remember where the holes are**. The
classic trick is to keep that information **inside the holes themselves**:
each free block starts with a small header saying its size and where the
next free block is.

```rust
struct NoLivre {                // "free node"
    tamanho: usize,        // size of the free block, including this header
    proximo: *mut NoLivre, // next free block (higher address), or null
}
```

```
 heap:  [used][NoLivre 48 →][used][used][NoLivre 32 →][used][NoLivre 900 KiB → null]
         ▲                   ▲                         ▲
       livres ──────────────►┘────────────────────────►┘ (a linked list)
```

The list **costs no extra memory**: the nodes live in memory that is free
anyway. When a block is handed out, its header is overwritten by the
caller's data; when the block comes back, the header is written again.

Two decisions in SiliconOS make the code simpler:

1. **The list is kept sorted by address.** That way, when a block is freed,
   its neighbors in the list are also its neighbors in memory, and they can
   be **merged** (section 4.4).
2. **Every block is a multiple of 16 bytes and starts at a multiple of
   16.** A `NoLivre` is 16 bytes (two `usize`s), so any leftover of a block
   is either 0 or fits a `NoLivre`:

```rust
const BLOCO_MINIMO: usize = core::mem::size_of::<NoLivre>(); // 16 (minimum block)
const ALINHAMENTO_MINIMO: usize = 16;                        // minimum alignment
const _: () = assert!(BLOCO_MINIMO == ALINHAMENTO_MINIMO);

fn tamanho_real(layout: Layout) -> usize {                   // "real size"
    alinhar_acima(layout.size(), ALINHAMENTO_MINIMO).max(BLOCO_MINIMO)
}
```

A `Box<u8>` (1 byte) gets 16; a request for 20 gets 32. The maximum waste
is 15 bytes per block, in exchange for code with no special cases.

### 4.2 Rounding up to a multiple

```rust
pub const fn alinhar_acima(x: usize, alinhamento: usize) -> usize {   // "align up"
    (x + alinhamento - 1) & !(alinhamento - 1)
}
```

It is the same trick as doc 06, section 5.4: add `alignment - 1` and clear
the low bits. It only works with powers of 2, and Rust guarantees every
alignment is a power of 2.

```
alinhar_acima(20, 16):  20 + 15 = 35 = 0b10_0011
                        & !15        = 0b10_0000 = 32
```

### 4.3 Allocating: the first block that fits (*first fit*)

`procurar` walks the list and uses the **first** block the request fits
in. Inside it, the request may need to start a little after the beginning,
because of alignment. Whatever is left before and after stays free:

```
 free block:       [inicio ......................................... fim)
 afterwards:       [left before][   block handed out  ][   left after    )
                   ▲            ▲                      ▲
                 inicio       bloco              bloco + tamanho
```

```rust
unsafe fn procurar(&mut self, tamanho: usize, alinhamento: usize) -> Option<usize> {
    let mut elo: *mut *mut NoLivre = &raw mut self.livres;     // elo = link
    unsafe {
        while !(*elo).is_null() {
            let no = *elo;
            let inicio = no as usize;
            let fim = inicio + (*no).tamanho;

            let mut bloco = alinhar_acima(inicio, alinhamento);
            if bloco > inicio && bloco - inicio < BLOCO_MINIMO {
                bloco = alinhar_acima(inicio + BLOCO_MINIMO, alinhamento);
            }

            let cabe = match bloco.checked_add(tamanho) {      // cabe = fits
                Some(fim_bloco) => fim_bloco <= fim,
                None => false,
            };
            if cabe {
                let fim_bloco = bloco + tamanho;
                let proximo = (*no).proximo;

                let mut seguinte = proximo;
                if fim > fim_bloco {                       // leftover AFTER: new node
                    let sobra = fim_bloco as *mut NoLivre;
                    (*sobra).tamanho = fim - fim_bloco;
                    (*sobra).proximo = proximo;
                    seguinte = sobra;
                }
                if bloco > inicio {                        // leftover BEFORE: the node shrinks
                    (*no).tamanho = bloco - inicio;
                    (*no).proximo = seguinte;
                } else {                                   // nothing before: the node leaves
                    *elo = seguinte;
                }
                return Some(bloco);
            }
            elo = &raw mut (*no).proximo;
        }
    }
    None
}
```

The most elegant detail is **`elo`**, a pointer to a pointer
(`*mut *mut NoLivre`). It does not point to a node; it points to **the
field that points to the node**: at first, `self.livres`; later, the
`proximo` of the previous node. That way, removing the current node from
the list is always the same line (`*elo = seguinte`), whether it is the
first one or one in the middle. Without it, the head of the list would be a
special case.

`&raw mut x` takes the address of `x` as a raw pointer, without creating a
Rust reference (the same as `&raw const` in doc 03, section 5.3, but with
write permission).

The rule "if the leftover in front is smaller than 16, push the block
forward" exists because a piece of 1 to 15 bytes cannot hold a `NoLivre`
and would be lost. Since every block starts at a multiple of 16, this only
happens with alignments larger than 16 (for example, 4096).

### 4.4 Freeing: insert in order and merge neighbors

```rust
unsafe fn inserir(&mut self, inicio: usize, tamanho: usize) {
    unsafe {
        let mut anterior: *mut NoLivre = ptr::null_mut();
        let mut atual = self.livres;
        while !atual.is_null() && (atual as usize) < inicio {
            anterior = atual;
            atual = (*atual).proximo;
        }

        // "heap: free ...: block was already free (double free)"
        if !atual.is_null() {
            assert!(inicio + tamanho <= atual as usize,
                    "heap: liberar {:#x}: bloco ja estava livre (liberacao dupla)", inicio);
        }
        if !anterior.is_null() {
            assert!(anterior as usize + (*anterior).tamanho <= inicio,
                    "heap: liberar {:#x}: bloco ja estava livre (liberacao dupla)", inicio);
        }

        let no = inicio as *mut NoLivre;
        (*no).tamanho = tamanho;
        (*no).proximo = atual;

        if !atual.is_null() && inicio + tamanho == atual as usize {   // merge with the one above
            (*no).tamanho += (*atual).tamanho;
            (*no).proximo = (*atual).proximo;
        }

        if anterior.is_null() {
            self.livres = no;
        } else if anterior as usize + (*anterior).tamanho == inicio { // merge with the one below
            (*anterior).tamanho += (*no).tamanho;
            (*anterior).proximo = (*no).proximo;
        } else {
            (*anterior).proximo = no;
        }
    }
}
```

The four possible cases when giving block `B` back:

```
 1. no free neighbors:       [used][ B ][used]   → B becomes a new node
 2. free above:              [used][ B ][free]   → B absorbs the one above
 3. free below:              [free][ B ][used]   → the one below absorbs B
 4. free on both sides:      [free][ B ][free]   → all three become one
```

Merging neighbors (*coalescing*) is what keeps the heap from turning to
dust: without it, after many small allocations, the heap would be a list
of thousands of 16-byte holes, and a 1 KiB request would fit in none of
them, even with plenty of free memory. This is called **fragmentation**.

The two `assert!`s catch the **double free**: if the block being returned
overlaps a block that is already in the list, someone is freeing memory
that was already free. It is the same principle as the frame allocator
(doc 05, section 4.8): detect the error on the spot, with a clear message,
instead of letting two owners write to the same place.

### 4.5 Growing on demand

```rust
unsafe fn alocar(&mut self, layout: Layout) -> *mut u8 {
    let tamanho = tamanho_real(layout);
    let alinhamento = layout.align().max(ALINHAMENTO_MINIMO);
    loop {
        if let Some(bloco) = unsafe { self.procurar(tamanho, alinhamento) } {
            self.em_uso += tamanho;
            self.pico = self.pico.max(self.em_uso);
            self.alocacoes += 1;
            return bloco as *mut u8;
        }
        let falta = (tamanho + alinhamento).saturating_sub(self.livre_no_fim());   // falta = missing
        if !unsafe { self.crescer(falta) } {
            return ptr::null_mut();
        }
    }
}
```

If no block fits, the heap **grows**: `crescer` maps more pages right after
the end (at least 64 KiB at a time, so as not to map page by page) and
hands the new piece to `inserir`, which merges it with the last free block
if that one touches the end. Then the loop tries again.

`livre_no_fim()` (*free at the end*) is a fix that the test in section 7.4
revealed. In the first version, the heap always grew by "size +
alignment", ignoring the free block already at the end that would merge
with the new piece. Each growth left 4 KiB extra at the end, and that waste
piled up: 63 blocks of 1 MiB cost 64,824 KiB instead of 64,516.

`saturating_sub` is a subtraction that stops at zero instead of
overflowing.

When the 64 MiB limit is reached (or frames run out), `alocar` returns a
null pointer. Rust handles that by calling the *alloc error handler*, which
in a `no_std` kernel triggers a panic: "memory allocation of N bytes
failed" (section 7.4).

The heap **does not give** frames back to the frame allocator when it
shrinks: once mapped, the memory stays with the heap. It is a common
simplification; a larger kernel would unmap whole free pages at the end of
the heap.

---

## 5. Connecting the heap to Rust: `#[global_allocator]`

So far we have a `Heap` struct with `alocar` and `liberar`. We still have
to tell Rust: **"this is where `Box`, `Vec` and `String` must ask for
memory"**.

The `alloc` crate (the part of the standard library with `Box`, `Vec` and
`String`) does not know where memory comes from. It calls an interface, the
**`GlobalAlloc`** trait, which has two required methods
([rust-basico §27](rust-basico.md#27-globalalloc-layout-and-global_allocator)):

```rust
pub struct AlocadorLista;                     // "list allocator"

unsafe impl GlobalAlloc for AlocadorLista {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match HEAP.lock().as_mut() {
            Some(h) => unsafe { h.alocar(layout) },
            None => ptr::null_mut(),          // the heap does not exist yet
        }
    }

    unsafe fn dealloc(&self, ponteiro: *mut u8, layout: Layout) {
        let mut guarda = HEAP.lock();
        let h = guarda.as_mut().expect("heap nao inicializado");   // "heap not initialized"
        unsafe { h.liberar(ponteiro, layout) };
    }
}

#[cfg(not(feature = "teste-bump"))]
#[global_allocator]
static ALOCADOR: AlocadorLista = AlocadorLista;
```

- **`Layout`** describes a request: **size** and **alignment**. A
  `Box<u64>` asks for `Layout { size: 8, align: 8 }`. Note that `dealloc`
  also receives the layout: Rust remembers the size of each allocation, so
  the allocator does not need to store it in a header. That is why the
  block handed out has no header at all.
- **`#[global_allocator]`** marks the `static` the whole program will use.
  There can be only one. With the `teste-bump` feature, this one is turned
  off and the one in `bump.rs` takes over.
- **`unsafe impl`**: implementing `GlobalAlloc` is a promise the compiler
  cannot check ("the blocks I hand out are valid, aligned and do not
  overlap"). If the allocator gets it wrong, every `Vec` in the kernel may
  corrupt memory.
- The `Heap` lives in a `static Mutex<Option<Heap>>`, the same pattern as
  the console (doc 01) and the frame allocator (doc 05). Since it holds raw
  pointers, we must declare `unsafe impl Send for Heap {}`, as was done
  with `FrameBuffer` in doc 01.

And, at the top of `main.rs`, one line:

```rust
extern crate alloc;
```

It brings the `alloc` crate into the project. From then on,
`alloc::vec::Vec`, `alloc::boxed::Box`, `alloc::string::String` and the
`alloc::format!` macro work as in any Rust program.

### The order matters

```rust
// ---- Heap (docs/07-heap.md) ----
memoria::heap::init();
demonstrar_heap();       // "demonstrate heap"
testes_de_heap();        // "heap tests"
```

`heap::init()` can only run **after** `paginacao::init()`: it maps pages
in the kernel's tables and asks the allocator for frames. Before that, any
`Box::new` would get the null pointer from `alloc` and turn into a panic.
Today nothing in the kernel uses the heap before that line.

```rust
pub fn init() {
    let mapeado = mapear_faixa(HEAP_INICIO, TAMANHO_INICIAL);
    assert!(mapeado == TAMANHO_INICIAL, "heap: sem quadros para o heap inicial");
    let mut h = Heap { livres: ptr::null_mut(), fim: HEAP_INICIO + mapeado, ... };
    unsafe { h.inserir(HEAP_INICIO as usize, mapeado as usize) };   // 1 free block of 1 MiB
    *HEAP.lock() = Some(h);
}
```

---

## 6. One step at a time: what happens in a `Box::new(42u64)`

```
 Box::new(42u64)
   └─ alloc::alloc(Layout { size: 8, align: 8 })            [alloc crate]
      └─ ALOCADOR.alloc(layout)                              [#[global_allocator]]
         └─ HEAP.lock()  → Heap::alocar(layout)              [heap.rs]
            ├─ tamanho_real = 16, alinhamento = 16
            ├─ procurar(16, 16): the first free block fits
            │    cuts 16 bytes from its start; the leftover becomes the new free node
            └─ returns 0xFFFF_9000_0000_0000
   └─ writes 42 at that address
 ...
 end of scope: the Box goes out of scope (drop)
   └─ ALOCADOR.dealloc(0xFFFF_9000_0000_0000, Layout { size: 8, align: 8 })
      └─ Heap::liberar → inserir(…, 16): merges with the neighboring free block
```

---

## 7. How it was tested

### 7.1 The normal boot

The boot now always uses the heap, in a small demonstration:

```
Heap: faixa virtual a partir de 0xffff900000000000
  Box::new(42)      -> 42 em 0xffff900000000000
  Vec (quadrados)   -> [1, 4, 9, 16, 25, 36, 49, 64]
  String            -> "SiliconOS tem heap" (18 bytes)
  em uso: 112 bytes em 3 alocacoes; mapeado: 1024 KiB; blocos livres: 2
```

("Heap: virtual range starting at ... / Box::new(42) -> 42 at ... / Vec
(squares) / String -> "SiliconOS has a heap" / in use: 112 bytes in 3
allocations; mapped: 1024 KiB; free blocks: 2")

Every number can be checked by hand, and they show fragmentation happening
on a small scale:

| Step | Asks for | Gets | Where (from the start of the heap) |
|---|---|---|---|
| `Box::new(42u64)` | 8 bytes | 16 | 0–16 |
| `Vec` with 8 `u64` | 64 | 64 | 16–80 |
| `String::from("SiliconOS")` | 9 | 16 | 80–96 |
| `push_str(" tem heap")`: the String needs 18 bytes, so it is **reallocated** | 18 | 32 | 96–128, and block 80–96 is freed |

Result: 16 + 64 + 32 = **112 bytes in use** in 3 live allocations, and **2
free blocks**: the 16-byte hole at 80–96, which the String left behind, and
the rest of the heap from 128 on.

### 7.2 `./run.sh teste-heap`

```
Teste heap: Vec de 200000 u64 ok; heap cresceu de 1024 KiB para 4096 KiB
Teste heap OK: Box, Vec, String, 15128 alocacoes e 15128 liberacoes,
  pico 3072 KiB; blocos livres: 5001 com metade das caixas liberada, 1 depois de
  realocar 5 000 caixas nos buracos; alinhamento 64 e 4096 ok;
  em uso no fim = no inicio (0 bytes), 1 bloco livre de 4096 KiB;
  quadros usados pelo crescimento: 769 (o heap nao devolve quadros)
```

("Heap test: Vec of 200000 u64 ok; heap grew from 1024 KiB to 4096 KiB /
Heap test OK: Box, Vec, String, 15128 allocations and 15128 frees, / peak
3072 KiB; free blocks: 5001 with half of the boxes freed, 1 after /
reallocating 5,000 boxes into the holes; alignment 64 and 4096 ok; / in
use at the end = at the start (0 bytes), 1 free block of 4096 KiB; /
frames used by the growth: 769 (the heap does not give frames back)")

The test does six checks:

1. **`Box`**: the value is read back and the address is inside the heap.
2. **`Vec` growing beyond the initial heap**: 200,000 `u64`s (1.6 MB, more
   than the initial 1 MiB). The sum checks out, and the heap grew. The
   **3072 KiB peak** has an explanation: a `Vec` doubles its capacity when
   it fills up, and during the copy the old buffer (1 MiB) and the new one
   (2 MiB) exist at the same time. The heap mapped 4 MiB in total because
   the holes left by the previous buffers are never enough for the next
   one: 32 + 64 + ... + N/2 is always less than N.
3. **`String` and `format!`**: 100 numbers concatenated, checked by the
   start, the end and the length.
4. **10,000 small boxes**: frees those at odd positions, which creates
   **5000 holes** of 32 bytes (5001 free blocks, counting the rest of the
   heap). Then it creates 5000 new boxes, which land exactly in those holes
   (*first fit*), and the list is back to **1 block**. The contents of every
   box are checked.
5. **Alignment**: 100-byte requests with alignment 64 and 4096, made
   directly to the allocator (`alloc::alloc::alloc`), come back with
   addresses that are multiples of the alignment.
6. **Nothing leaked**: after freeing everything, usage returns to its
   initial value, and the free blocks merged into one, the size of the
   whole mapped heap. That proves neighbor merging works in all four cases
   of section 4.4.

The 769 frames are the 768 pages of growth (3 MiB) plus one new PT table,
because the heap started occupying a second 2 MiB range.

The test also passed with `RAM=4G` and `RAM=8G`.

### 7.3 `./run.sh teste-heap-liberacao-dupla`

```
Teste: liberando o bloco 0xffff900000000000 do heap duas vezes...
!!! KERNEL PANIC !!!
Local: src/memoria/heap.rs:177:17
Mensagem: heap: liberar 0xffff900000000000: bloco ja estava livre (liberacao dupla)
```

("Test: freeing heap block ... twice... / heap: free ...: block was
already free (double free)")

In safe Rust, a double free is impossible: a `Box` frees its memory
exactly once, on `drop`. That is why the test calls the allocator directly,
with `unsafe`. A bug like this may show up in the future in the kernel's
`unsafe` code, and the heap now catches it on the spot.

### 7.4 `./run.sh teste-heap-esgotar`

```
Teste: alocando 1 MiB por vez ate o limite do heap (64 MiB)...
  16 MiB alocados (heap mapeado: 16388 KiB)
  32 MiB alocados (heap mapeado: 32772 KiB)
  48 MiB alocados (heap mapeado: 49156 KiB)
  60 MiB alocados (heap mapeado: 61444 KiB)
  61 MiB alocados (heap mapeado: 62468 KiB)
  62 MiB alocados (heap mapeado: 63492 KiB)
  63 MiB alocados (heap mapeado: 64516 KiB)
!!! KERNEL PANIC !!!
Local: /rustc/.../library/alloc/src/alloc.rs:673:9
Mensagem: memory allocation of 1048576 bytes failed
```

("Test: allocating 1 MiB at a time up to the heap limit (64 MiB)... / 16
MiB allocated (heap mapped: 16388 KiB)...")

Each 1 MiB block costs exactly 1024 KiB of heap. The 64th does not fit:
only 1020 KiB are left before the limit (65,536 − 64,516), because the
first MiB of the heap already held the demonstration's data and the `Vec`
that keeps the blocks. The panic comes from inside the `alloc` crate (which
is why `Local:` points to Rust's code, not ours): it is the *alloc error
handler* reacting to the null pointer the heap returned.

Before the `livre_no_fim()` fix in section 4.5, this same test showed
64,824 KiB mapped for 63 blocks: that is how the waste showed up.

### 7.5 `./run.sh teste-heap-executar`: NX works

The test writes `0xC3` bytes (the `ret` instruction) into a `Box` and calls
that address as if it were a function:

```rust
let codigo = Box::new([0xC3u8; 16]);
let endereco = codigo.as_ptr() as u64;
let funcao: extern "C" fn() = unsafe { core::mem::transmute(endereco) };
funcao();
```

(`transmute` reinterprets the bits of a value as another type: here, a
number becomes a function pointer. It is one of Rust's most dangerous
operations, which is why it is `unsafe`.)

```
Teste: executando um `ret` gravado no heap em 0xffff900000000000...
!!! KERNEL PANIC !!!
Mensagem: EXCECAO #14 PAGE FAULT ao executar 0xffff900000000000 (pagina presente, sem permissao, modo kernel)
RIP=0xffff900000000000 CS=0x8 RFLAGS=0x2 RSP=0x7e960a8 SS=0x10
```

("Test: running a `ret` written to the heap at ... / EXCEPTION #14 PAGE
FAULT executing ... (page present, no permission, kernel mode)")

The page fault handler from doc 04 decoded everything: **"executing"** (bit
4 of the error code: it was an instruction fetch) and **"page present, no
permission"** (the page exists, but forbids execution). RIP is the heap
address itself.

**The opposite proof was also done.** Temporarily removing `SEM_EXECUCAO`
from `BITS_HEAP`, the same test executed the `ret`, came back and printed
"ERRO: o heap deveria ser proibido de executar (NX)" ("ERROR: the heap
should be forbidden from executing"). Then the code was restored.

### 7.6 The previous tests

The 13 tests from the previous chapters (panic, the 6 exceptions, frames,
frame double free, the 3 paging tests) passed again with the heap active,
and `teste-paginacao` also with `RAM=8G`.

---

## Changes to the files

What this chapter adds to the code of doc 06 (tag `doc-06`). Each file's link
leads to its state at the end of this chapter (tag `doc-07`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-06...doc-07).

| File | What changes | Where |
|---|---|---|
| [`src/memoria/heap.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/memoria/heap.rs) | **new**: `NoLivre`, `Heap` (procurar, inserir, crescer, alocar, liberar), `HEAP`, `AlocadorLista` with `#[global_allocator]`, `init`, `estatisticas`, `contem` | — |
| [`src/memoria/bump.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/memoria/bump.rs) | **new**: `Bump`, `AlocadorBump` (global only with the `teste-bump` feature), `init`, `estado` | — |
| [`src/memoria/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/memoria/mod.rs) | `#[allow(dead_code)] pub mod heap;` and `#[allow(dead_code)] pub mod bump;` | with the other `pub mod`s |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/main.rs) | `extern crate alloc;` | top, after the `#![...]`s |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/main.rs) | in `kernel_main`: `memoria::heap::init(); demonstrar_heap(); testes_de_heap();` | after `testes_de_paginacao()` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/src/main.rs) | `fn demonstrar_heap`, `fn testes_de_heap` | end of the file |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-07/Cargo.toml) | `license = "MIT"`; `teste-heap`, `teste-heap-liberacao-dupla`, `teste-heap-esgotar`, `teste-heap-executar`, `teste-bump` | `[package]` and `[features]` |

---

## Difference from the book

Chapter 8 of the book deals with the same topic: giving the kernel a heap
so it can use `Box`, `Vec` and `String`. The allocators presented may be
different (besides bump and the linked list, it is common to see a
**fixed-size block** allocator, which keeps one list per size: 16, 32,
64... bytes, and is much faster). It is worth comparing as you read.

SiliconOS's particularities:

- the heap lives in a virtual range of its own (`0xFFFF_9000_...`), mapped
  with the tables the kernel itself built in doc 06, and **grows on
  demand** up to 64 MiB;
- the heap's pages use **NX**, tested both ways;
- the free list is written by hand, without the `linked_list_allocator`
  crate, in keeping with the project's aim of showing what libraries hide.

---

## Summary

- `Box`, `Vec` and `String` come from the **`alloc`** crate, which needs a
  **global allocator**: a `static` marked with `#[global_allocator]` that
  implements **`GlobalAlloc`** (`alloc` and `dealloc`, receiving a
  **`Layout`** with size and alignment).
- The **heap** is a layer on top of the frame allocator and paging: a
  virtual range of its own (`0xFFFF_9000_0000_0000`), starting with 1 MiB
  and growing on demand up to 64 MiB, with **NX** pages.
- The **bump** allocator only moves forward and never reuses memory: fast,
  but unacceptable on its own in a kernel.
- The **free list** keeps the holes inside themselves, sorted by address.
  Allocating = *first fit* and cutting; freeing = inserting in order and
  **merging neighbors**, which fights **fragmentation**.
- A double free becomes an immediate panic; running out of heap becomes
  "memory allocation of N bytes failed".

<details>
<summary><b>Review (click to see the answers)</b></summary>

1. **Why not use the frame allocator directly for a `Box<u64>`?**
   Because it only hands out 4 KiB blocks. A `Box<u64>` needs 8 bytes:
   99.8% of the frame would be wasted.

2. **Where does the free list keep its nodes?**
   Inside the free blocks themselves: the first 16 bytes of each free block
   are a `NoLivre` (size and next). That is why every block is at least 16
   bytes.

3. **Why is the list kept in address order?**
   So that, when a block is freed, its neighbors in the list are also its
   neighbors in memory. That way adjacent blocks can be merged and
   fragmentation avoided.

4. **How does the allocator know the size of the block being freed, if the
   block handed out has no header?**
   Rust passes the same `Layout` to `dealloc` that it passed to `alloc`.
   The allocator recomputes the size with the same function
   (`tamanho_real`).

5. **At boot, why are there 2 free blocks and not 1, if nothing was freed
   on purpose?**
   Because the String grew from 9 to 18 bytes and was reallocated: the new
   block (32 bytes) went after it, and the old one (16 bytes) became a hole
   between the `Vec` and the String.

6. **What would happen if the heap were mapped without the NX bit?**
   Bytes written to the heap could be executed as code. The
   `teste-heap-executar` test shows both cases: with NX, a page fault
   "executing"; without NX, the `ret` runs and returns.

</details>
