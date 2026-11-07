# 11: One address space per program

> **You will understand:** why two programs need **different page tables**
> to use the same address without seeing each other; how to build a new
> PML4 that **shares the kernel** with every other one by copying just a few
> 8-byte entries; what happens when the kernel writes to **CR3**; and how
> the `EspacoEnderecos` type gives back, on its own, when destroyed, every
> frame the program used (including the tables doc 10 left behind).
>
> **Files:** `src/memoria/espaco.rs` (new: the `EspacoEnderecos` type),
> `src/memoria/paginacao.rs` (`raiz_kernel`, `ativar`, the `AreaDoKernel`
> error), `rodar` in `src/usuario/mod.rs`, the `espacos` command in
> `src/shell.rs`
> **Book:** chapter 11, section 11.4 (the book only names the idea; the
> implementation is ours)
> **Try it:** `./run.sh`, then `espacos`, `mem`, `rodar ola`, `mem`;
> `cargo test --target x86_64-unknown-uefi`
> **Read first:** doc 06 (paging: PML4, `mapear_em`, TLB) and doc 10 (user
> mode)
> **New Rust here:** `Drop` ([§38](rust-basico.md#38-drop-code-that-runs-when-the-value-dies))
> **Names in the code:** `espaco de enderecos` = *address space*, `novo` =
> *new*, `raiz` = *root*, `ativar` = *activate*, `mapear` = *map*,
> `traduzir` = *translate*, `do_programa` = *belongs to the program*,
> `entrada_livre` = *free entry*, `liberar_arvore` = *free tree*,
> `area do kernel` = *kernel area*, `quadro(s)` = *frame(s)*
>
> **Code for this chapter:** [state at the end of doc 11](https://github.com/moises-dev-git/siliconOS/tree/doc-11) · [what changed since doc 10](https://github.com/moises-dev-git/siliconOS/compare/doc-10...doc-11) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

In doc 10, the program lived **in the kernel's page tables**: `rodar`
mapped the code at `0x0000_2000_0000_0000` and the stack just above it, in
the same PML4 the kernel uses. It worked, but it had three flaws:

1. **Only one program at a time.** The next two steps (the scheduler, which
   alternates between several programs, and ELF executables, which choose
   their own addresses) need **two programs to use the same virtual
   address** at the same time. In a single tree, the address
   `0x2000_0000_0000` points to a single frame.
2. **One program would see the other.** If program A mapped something and B
   ran afterwards (or at the same time), A's pages would be in B's way.
   Isolation between programs is half the reason an operating system exists.
3. **Tables were left over.** `desmapear` does not free empty intermediate
   tables (doc 06), so the first run left a PDPT, a PD and a PT under entry
   64 of the kernel's PML4 forever: the **3 frames** that the
   `nada_fica_mapeado_nem_vaza` test needed a "warm-up" to ignore.

## 2. The idea: each program with its own tree, the kernel in all of them

CR3 tells the CPU where the PML4 is (doc 06). **Switching CR3 switches the
whole world at once**: every virtual address is now translated through
another tree. That is how every modern operating system separates
processes: each one has its own PML4, and switching processes includes
writing to CR3. That whole thing (the table tree and what it maps) is called
an **address space**.

There is a catch: the kernel has to keep existing **inside** every space.
When the program runs `int 0x80`, the CPU jumps to the kernel's handler
**without switching CR3**; the handler code, the ring-0 stack (TSS.RSP0), the
IDT, the heap and the console must be mapped there too. The same goes for
when the timer interrupts the program.

Copying all of the kernel's tables into each program would be expensive
and, worse, they would go stale (the heap grows, doc 07). The solution used
by practically every system is to **share**:

```
  kernel PML4                program A's PML4           program B's PML4
  [0]   identity ─────────┬─ [0]   (the same) ─────┬── [0]   (the same)
  [64]  (empty)           │  [64]  code/stack A    │   [64]  code/stack B
  [256] upper half ───────┼─ [256] (the same) ─────┼── [256] (the same)
  [288] heap ─────────────┼─ [288] (the same) ─────┼── [288] (the same)
  [320] MMIO ─────────────┘  [320] (the same) ─────┘   [320] (the same)
```

A PML4 entry is 8 bytes and points to a PDPT. If the program's PML4 has
**the same entry** (the same 8 bytes) as the kernel's, both point to **the
same PDPT**, and from there down everything is identical: the same PDs, the
same PTs, the same pages. By copying 4 entries (32 bytes), the program gets
the whole kernel. And if the heap grows later, the new page goes into a PT
that is already shared: every space sees it immediately.

The program, in turn, only touches the entries the kernel **does not use**.
The tree under them is its own.

> **Couldn't the program read the kernel, since it is mapped there?** No:
> the kernel's pages do not have the USUARIO bit (doc 10). The `invasor`
> program still dies with a page fault, as before. Being mapped is not the
> same as being accessible.

Which entries does the kernel use? Instead of guessing, the `espacos`
command (section 6) measures it, and on QEMU with 128 MiB the answer is:

```
entradas da PML4 do kernel (copiadas em todo espaco): [0, 256, 288, 320]
```

(*kernel PML4 entries (copied into every space)*)

| Entry | Addresses | What it is | Doc |
|---|---|---|---|
| 0 | 0 to 512 GiB | identity: the kernel's code, the boot stack and the framebuffer live here, where UEFI left them | 06 |
| 256 | `0xFFFF_8000_...` | all of RAM seen through the upper half (`fisico_para_virtual`) | 06 |
| 288 | `0xFFFF_9000_...` | the heap | 07 |
| 320 | `0xFFFF_A000_...` | APIC and I/O APIC registers (MMIO) | 08 |

Entry 0 is the most curious one: it is in the **lower** half, which normally
belongs to programs. It is there because the kernel still runs at the
addresses where UEFI loaded it (the "identity as scaffolding" from doc 06).
That is why the program's rule is not "lower half" but "lower half **and**
an entry the kernel does not use". The program's code is in entry 64, far
from it.

## 3. The code: `EspacoEnderecos`

The whole type stores one number: the physical address of the PML4.

```rust
pub struct EspacoEnderecos {
    /// Physical address of this space's PML4.
    raiz: u64,
}
```

### 3.1 Creating: a new PML4 with the kernel copied in

```rust
pub fn novo() -> Option<EspacoEnderecos> {
    let raiz = nova_tabela()?;
    let kernel = tabela_em(paginacao::raiz_kernel());
    let nova = tabela_em(raiz);
    for i in 0..512 {
        if kernel.entradas[i].presente() {
            nova.entradas[i] = kernel.entradas[i];
        }
    }
    Some(EspacoEnderecos { raiz })
}
```

- `nova_tabela()` (doc 06) takes a frame and zeroes it: a PML4 with no
  entries. The `?` returns `None` if frames ran out.
- `tabela_em(physical)` gives access to a table by its physical address
  (through the upper half). Both were private to `paginacao.rs`; they became
  `pub(super)` so the sibling module `espaco.rs` can use them without
  becoming visible outside `memoria`.
- `nova.entradas[i] = kernel.entradas[i]` copies **the entry**, 8 bytes, not
  the table it points to. That is the sharing from section 2.

### 3.2 Mapping: only outside the kernel's area

```rust
pub fn mapear(&mut self, virt: u64, quadro: Quadro, bits: u64) -> Result<(), ErroMapeamento> {
    let resultado = if !Self::do_programa(virt) {
        Err(ErroMapeamento::AreaDoKernel)
    } else {
        paginacao::mapear_em(self.raiz, virt, quadro.endereco(), TamanhoPagina::K4, bits)
    };
    if resultado.is_err() {
        quadros::liberar(quadro);
    }
    resultado
}

pub fn do_programa(virt: u64) -> bool {
    let i = paginacao::indices(virt)[0];
    i < 256 && Self::entrada_livre(i)
}

pub fn entrada_livre(i: usize) -> bool {
    !tabela_em(paginacao::raiz_kernel()).entradas[i].presente()
}
```

Three decisions here:

1. **The `do_programa` check.** Without it, mapping a user page at `0x1000`
   (entry 0) would walk down the identity tables, which belong **to the
   kernel and to every space**: the `else if` from doc 10 would set the
   USUARIO bit on them, and the program would gain access to part of the
   kernel. Hence the new error `ErroMapeamento::AreaDoKernel`.
2. **The space owns the frame.** `mapear` takes a `Quadro` (not an address)
   and, from then on, the frame belongs to the space: it is freed when the
   space dies. If the mapping fails, nobody else would hold it, so `mapear`
   frees it right away. That way the caller never has to remember to free
   anything.
3. **Only 4 KiB pages.** That simplifies destruction (section 3.4): every
   leaf is a frame from the allocator.

`mapear_em` is the same as in doc 06, only with the space's root instead of
the kernel's: any missing intermediate tables (the PDPT, PD and PT of entry
64) are created in **this** tree, with the USUARIO bit.

### 3.3 Activating: writing to CR3

In `paginacao.rs`:

```rust
pub fn ativar(raiz: u64) {
    if raiz_atual() != raiz {
        unsafe { asm!("mov cr3, {}", in(reg) raiz, options(nostack, preserves_flags)) };
    }
}
```

and in `EspacoEnderecos`, `ativar(&self)` just calls `paginacao::ativar(self.raiz)`.

Writing to CR3 does two things in the same instruction: the CPU starts
translating through the new PML4 **and empties the TLB** (the translation
cache, doc 06), except for entries marked GLOBAL. That is why mapping pages
in an inactive space needs no `invlpg`: when it is activated, the TLB starts
empty. The cost comes from the same place: after each switch, the first
access to each page has to walk the tree again. (Real systems mark kernel
pages GLOBAL and use PCID, an address-space tag in the TLB, to lose less.
SiliconOS uses neither.)

The `if` avoids the switch (and the flush) when CR3 already holds that root.

### 3.4 Destroying: `Drop` gives everything back

In Rust, a type can say what happens when one of its values **dies** (goes
out of scope, or is passed to `drop`) by implementing the `Drop` trait
([§38](rust-basico.md#38-drop-code-that-runs-when-the-value-dies)). This is
where the space returns its frames:

```rust
impl Drop for EspacoEnderecos {
    fn drop(&mut self) {
        assert!(paginacao::raiz_atual() != self.raiz, "destruindo o espaco de enderecos ativo");
        let kernel = tabela_em(paginacao::raiz_kernel());
        let pml4 = tabela_em(self.raiz);
        for i in 0..256 {
            let e = pml4.entradas[i];
            if e.presente() && !kernel.entradas[i].presente() {
                liberar_arvore(e.endereco(), 1);
            }
        }
        quadros::liberar(Quadro(self.raiz));
    }
}

fn liberar_arvore(tabela: u64, nivel: usize) {
    for e in tabela_em(tabela).entradas.iter().filter(|e| e.presente()) {
        assert!(!e.grande(), "pagina grande num espaco de programa");
        if nivel == 3 {
            quadros::liberar(Quadro(e.endereco())); // the program's page
        } else {
            liberar_arvore(e.endereco(), nivel + 1);
        }
    }
    quadros::liberar(Quadro(tabela));
}
```

- **The `assert`** (*"destroying the active address space"*). Freeing the
  tables the CPU is using would be sawing off the branch you are sitting on:
  the PML4's frame would go back to the allocator, other code would reuse it
  and write into it, and the next translation would read garbage. Whoever
  destroys a space must first switch back to the kernel's CR3.
- **Only the program's entries.** The condition
  `!kernel.entradas[i].presente()` skips the entries copied from the kernel:
  freeing entry 0's tree would hand the identity tables, in use by
  everyone, back to the allocator.
- **`liberar_arvore` is recursive.** Level 1 (PDPT) calls level 2 (PD),
  which calls level 3 (PT); in the PT, every present entry is a page of the
  program. Each table is freed **after** what is below it (the reverse order
  would read a table that was already freed).

```
  PML4 (root)                       freed last
   └─[64] PDPT                      liberar_arvore(level 1)
           └─[0] PD                 liberar_arvore(level 2)
                  └─[0] PT          liberar_arvore(level 3)
                         ├─[0]   code page    → free
                         └─[256] stack page   → free
```

That is the space of a doc 10 program: code at `0x2000_0000_0000` (indices
64, 0, 0, 0) and stack at `0x2000_0010_0000` (64, 0, 0, 256). Both share the
same PT, so the whole space costs **6 frames**: PML4, PDPT, PD, PT and two
pages.

### 3.5 The kernel's tables have a fixed address

With several spaces, "the active tables" (CR3) and "the kernel's tables" are
no longer the same thing. If the kernel mapped something for itself
**during** a program (the heap growing inside a system call, for example),
the old `mapear` (which used `raiz_atual()`) would write into the program's
PML4. Most of the time the write would land in a shared table and work by
luck; but if it needed a new PML4 entry, that entry would exist only in that
program's space and vanish with it.

So `paginacao::init` now remembers the kernel's PML4:

```rust
static RAIZ_KERNEL: AtomicU64 = AtomicU64::new(0);

pub fn raiz_kernel() -> u64 {
    match RAIZ_KERNEL.load(Ordering::Relaxed) {
        0 => raiz_atual(),
        r => r,
    }
}
```

(before `init`, 0 means "we do not have our own tables yet", and the active
ones are UEFI's), and `mapear`/`desmapear` now use `raiz_kernel()`.

One limitation is worth writing down: a kernel PML4 entry created **after**
a space exists does not show up in it (the copy was made at creation).
Today that does not happen: the four entries are created at boot. A bigger
kernel would solve it by creating all 256 upper-half entries up front (it
costs 256 frames, 1 MiB), so that from then on only shared tables change.

## 4. `rodar` with its own space

Doc 10's `rodar` changed in steps 1 (create the space), 3 (activate and
deactivate) and 4 (destroy):

```rust
pub fn rodar(programa: &programas::Programa) -> Resultado {
    let codigo = programa.codigo();
    assert!(codigo.len() <= TAMANHO_QUADRO as usize, "programa maior que uma pagina");

    // 1. An address space for this program only (doc 11): a new PML4,
    //    with the kernel copied in and the program's half empty.
    let mut espaco = EspacoEnderecos::novo().expect("sem quadros para a PML4 do programa");

    // 2. One code page (read-only, executable) and one stack page
    //    (writable, no execute), both reachable from ring 3. The space
    //    owns the frames.
    let q_codigo = nova_pagina_zerada();
    unsafe {
        let destino = paginacao::fisico_para_virtual(q_codigo.endereco()) as *mut u8;
        core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
    }
    espaco.mapear(CODIGO_VIRT, q_codigo, flags::USUARIO).expect("mapear o codigo do programa");
    espaco.mapear(PILHA_VIRT, nova_pagina_zerada(),
                  flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO).expect("mapear a pilha");

    // 3. Activate the space (CR3) and enter ring 3. Only comes back when
    //    the program ends.
    *RESULTADO.lock() = None;
    INICIO.store(timer::tiques(), Ordering::Relaxed);
    ATIVO.store(true, Ordering::Relaxed);
    espaco.ativar();
    unsafe { entrar_no_anel3(CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO) };
    // Back: the active tables are still the program's. Return to the
    // kernel's BEFORE destroying the program's.
    paginacao::ativar(paginacao::raiz_kernel());
    ATIVO.store(false, Ordering::Relaxed);
    // We came back from inside a handler (interrupts off).
    cpu::ligar_interrupcoes();

    // 4. Clean up: destroying the space returns pages, tables and the PML4.
    drop(espaco);
    RESULTADO.lock().take().expect("programa terminou sem resultado")
}
```

What to notice:

- **The kernel's code keeps running after `espaco.ativar()`.** The next
  instruction (the call to `entrar_no_anel3`) is fetched from an address in
  entry 0, which the program's PML4 also has. Without the copy from section
  3.1, the CPU would page-fault on the instruction right after `mov cr3`,
  and the fault could not be handled either: the IDT and the stack are at
  kernel addresses.
- **During the program, everything in the kernel runs with the program's
  CR3:** the system calls (`escrever` reads the message through the
  program's address, which only exists in that space), the timer, the
  keyboard, the exceptions.
- **The three exit paths** (exit, exception, timeout) reach the same point,
  after `entrar_no_anel3`, through `voltar_ao_kernel` (doc 10). So one line
  is enough to switch back to the kernel's CR3.
- **The order at the end matters:** `paginacao::ativar(kernel)` before
  `drop(espaco)`. Swapping the two lines (tested and undone), `cargo test`
  stops at the first program with
  `Local: src/memoria/espaco.rs:94:9` and
  `Mensagem: destruindo o espaco de enderecos ativo`.
- **`desmapear` is gone from `rodar`.** There is nothing to unmap: the whole
  tree is thrown away.

## 5. The tests

Four new tests in `espaco.rs`, and an old one simplified.

**Same address, different pages**: the core of the chapter. Two spaces map
`0x2000_0000_0000`, each to a frame holding a different value; each one's
translation gives its own frame, the kernel's gives nothing, and reading
**for real** (with each space active) shows both values:

```rust
#[test_case]
fn mesmo_endereco_paginas_diferentes() {
    let (qa, qb) = (pagina_com(0xAAAA), pagina_com(0xBBBB));
    let (mut a, mut b) = (EspacoEnderecos::novo().unwrap(), EspacoEnderecos::novo().unwrap());
    a.mapear(V, qa, flags::USUARIO).unwrap();
    b.mapear(V, qb, flags::USUARIO).unwrap();
    assert_eq!(a.traduzir(V).unwrap().fisico, qa.endereco());
    assert_eq!(b.traduzir(V).unwrap().fisico, qb.endereco());
    assert!(paginacao::traduzir_em(paginacao::raiz_kernel(), V).is_none());
    // Reading for real, with each space active:
    let ler = || unsafe { (V as *const u64).read_volatile() };
    a.ativar();
    let va = ler();
    b.ativar();
    let vb = ler();
    paginacao::ativar(paginacao::raiz_kernel());
    assert_eq!((va, vb), (0xAAAA, 0xBBBB));
}
```

(`pagina_com(value)` allocates a frame and writes `value` at its start,
through the upper half. `V` is `0x0000_2000_0000_0000`.) The final
`assert_eq!` comes after switching back to the kernel's CR3 on purpose: if
it failed with a test space active, the panic would happen outside the
kernel's tables.

**The kernel looks the same in all of them**: a `Box` (on the heap) has the
same translation in a new space and in the kernel's tables.

```rust
#[test_case]
fn kernel_aparece_igual_em_todos() {
    let a = EspacoEnderecos::novo().unwrap();
    let caixa = alloc::boxed::Box::new(7u64);
    let endereco = &*caixa as *const u64 as u64;
    assert!(heap::contem(endereco));
    assert_eq!(a.traduzir(endereco).map(|t| t.fisico), paginacao::traduzir(endereco).map(|t| t.fisico));
}
```

**Destroying returns every frame**, counted one by one:

```rust
#[test_case]
fn destruir_devolve_todos_os_quadros() {
    let livres = quadros::livres();
    let mut a = EspacoEnderecos::novo().unwrap();
    a.mapear(V, pagina_com(1), flags::USUARIO).unwrap();
    a.mapear(V + 0x20_0000, pagina_com(2), flags::USUARIO).unwrap();
    // PML4 + PDPT + PD + 2 PTs (each PT covers 2 MiB; the pages are 2 MiB apart) + 2 pages
    assert_eq!(livres - quadros::livres(), 7);
    drop(a);
    assert_eq!(quadros::livres(), livres);
}
```

> **A mistake the test caught.** The first version put the second page at
> `V + 0x10_0000` (1 MiB further) and expected 7 frames. It got 6. A PT has
> 512 entries of 4 KiB = **2 MiB**, so 1 MiB further is still the same PT.
> With 2 MiB between them, each page needs its own PT, and the count adds up.

**The kernel's area is rejected**, without leaking the rejected frame:

```rust
#[test_case]
fn area_do_kernel_recusada() {
    let livres = quadros::livres();
    let mut a = EspacoEnderecos::novo().unwrap();
    let heap = 0xFFFF_9000_0000_0000;
    assert_eq!(a.mapear(heap, pagina_com(0), flags::USUARIO), Err(ErroMapeamento::AreaDoKernel));
    assert_eq!(a.mapear(0x1000, pagina_com(0), flags::USUARIO), Err(ErroMapeamento::AreaDoKernel));
    drop(a);
    assert_eq!(quadros::livres(), livres);
}
```

And the doc 10 test that needed a warm-up no longer does:

```rust
#[test_case]
fn nada_fica_mapeado_nem_vaza() {
    // Since doc 11 no warm-up is needed: the program's tables die with
    // its space, so the first run leaves nothing behind either.
    let livres = quadros::livres();
    rodar_por_nome("ola");
    assert!(paginacao::traduzir(CODIGO_VIRT).is_none());
    assert!(paginacao::traduzir(PILHA_VIRT).is_none());
    assert_eq!(paginacao::raiz_atual(), paginacao::raiz_kernel());
    assert_eq!(quadros::livres(), livres);
}
```

## 6. See it for yourself: the `espacos` command

The command creates **two spaces alive at the same time**, copies `ola`'s
code into one and `relogio`'s into the other, both at `CODIGO_VIRT`, and
reads the first 8 bytes at that address with each space active:

```rust
fn espacos(_: &[&str]) {
    use crate::memoria::espaco::EspacoEnderecos;
    use crate::memoria::paginacao::{self, flags};
    use crate::memoria::quadros;
    use crate::usuario::{programas, CODIGO_VIRT};

    let kernel = paginacao::raiz_kernel();
    kprintln!("kernel:   PML4 em {:#x}; {:#x} -> {}", kernel, CODIGO_VIRT,
              if paginacao::traduzir_em(kernel, CODIGO_VIRT).is_some() { "mapeado" } else { "nao mapeado" });
    let usadas: Vec<usize> = (0..512).filter(|&i| !EspacoEnderecos::entrada_livre(i)).collect();
    kprintln!("entradas da PML4 do kernel (copiadas em todo espaco): {:?}", usadas);
    let mut criados: Vec<(&str, EspacoEnderecos)> = Vec::new();
    for nome in ["ola", "relogio"] {
        let codigo = programas::procurar(nome).unwrap().codigo();
        let Some(q) = quadros::alocar() else { kprintln!("sem memoria"); return };
        unsafe {
            let destino = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
            core::ptr::write_bytes(destino, 0, 4096);
            core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
        }
        let Some(mut espaco) = EspacoEnderecos::novo() else { quadros::liberar(q); kprintln!("sem memoria"); return };
        if espaco.mapear(CODIGO_VIRT, q, flags::USUARIO).is_err() { kprintln!("sem memoria"); return }
        criados.push((nome, espaco));
    }
    for (nome, espaco) in &criados {
        // Read the first bytes with this space active (the kernel is still
        // visible in it, so this code keeps running normally).
        espaco.ativar();
        let bytes = unsafe { (CODIGO_VIRT as *const [u8; 8]).read_volatile() };
        paginacao::ativar(kernel);
        kprintln!("{:<9} PML4 em {:#x}; {:#x} -> fisico {:#x}; bytes {:02x?}", nome, espaco.raiz(),
                  CODIGO_VIRT, espaco.traduzir(CODIGO_VIRT).unwrap().fisico, bytes);
    }
} // `criados` is destroyed here: PML4s, tables and pages go back to the allocator
```

The kernel can read `CODIGO_VIRT` directly (in ring 0) even though it is a
user page because SMAP is off (doc 10). And there is no explicit `drop`:
when the function ends, the `Vec` `criados` dies, and with it each
`EspacoEnderecos`, whose `Drop` returns the frames.

## 7. How it was tested

### 7.1 From the shell

Real output (serial), QEMU with 128 MiB:

```
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> espacos
kernel:   PML4 em 0x101000; 0x200000000000 -> nao mapeado
entradas da PML4 do kernel (copiadas em todo espaco): [0, 256, 288, 320]
ola       PML4 em 0x20f000; 0x200000000000 -> fisico 0x20e000; bytes [48, 8d, 3d, 15, 00, 00, 00, be]
relogio   PML4 em 0x214000; 0x200000000000 -> fisico 0x213000; bytes [b8, 02, 00, 00, 00, cd, 80, 48]
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> rodar ola
Ola do anel 3! (escrito por uma chamada de sistema)
'ola' terminou: codigo de saida 42
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> rodar invasor
[kernel] PAGE FAULT no programa (RIP=0x20000000000a, CS=0x23): ao ler 0xffff800000100000 (pagina presente, sem permissao, modo usuario)
'invasor' encerrado pelo kernel: PAGE FAULT em 0x20000000000a
silicon> rodar laco
'laco' encerrado pelo kernel: passou de 2 s sem terminar
silicon> mem
quadros: 20307 livres de 32768 (79 MiB livres)
...
silicon> traduzir 0x200000000000
  endereco 0x0000200000000000 [ 64   0   0   0] -> NAO MAPEADO
```

(`quadros: N livres` = *frames: N free*; `nao mapeado` = *not mapped*;
`fisico` = *physical*.)

Reading it:

- **The same address, two contents.** `ola`'s bytes start with `48 8d 3d`
  (`lea rdi, [rip + ...]`, the message pointer) and `relogio`'s with
  `b8 02 00 00 00` (`mov eax, 2`, the `tiques` syscall); they are each
  program's first instructions (doc 10, section 4.2).
- **Each space costs 5 frames** here (PML4, PDPT, PD, PT and the code page):
  the PML4s are at `0x20f000` and `0x214000`, `0x5000` apart, because the
  allocator handed out the frames in sequence. In `rodar`, with the stack,
  it is 6.
- **20307 free frames the whole time**, including after the **first**
  `rodar`. In doc 10, the first run took the number from 20308 to 20305 (the
  3 tables left in the kernel's PML4). Programs that end by exception
  (`invasor`) and by the timer (`laco`) leave nothing behind either.
- **The kernel's PML4 never sees the program:** `traduzir` (which uses the
  active tables, the kernel's while the shell is running) says NAO MAPEADO
  (*not mapped*).

### 7.2 `cargo test`

36 tests, all `[ok]`: the 32 from before plus the 4 from `espaco.rs`.
Excerpt:

```
siliconos::memoria::espaco::testes::area_do_kernel_recusada ... [ok]
siliconos::memoria::espaco::testes::destruir_devolve_todos_os_quadros ... [ok]
siliconos::memoria::espaco::testes::kernel_aparece_igual_em_todos ... [ok]
siliconos::memoria::espaco::testes::mesmo_endereco_paginas_diferentes ... [ok]
...
siliconos::usuario::testes::nada_fica_mapeado_nem_vaza ... Ola do anel 3! (escrito por uma chamada de sistema)
[ok]
```

### 7.3 Regression

The 21 builds with test features (`teste-panic` through `teste-timer`) still
pass.

## What is still missing

- **Still one program at a time.** Several spaces can now be alive at once
  (the `espacos` command proves it), but `rodar` still waits for the program
  to end. The next step, the **scheduler**, saves each program's state and
  alternates between them on the timer, switching CR3 at every switch.
  *(Done in [doc 12](12-escalonador.md).)*
- **No GLOBAL or PCID:** each CR3 switch empties the whole TLB, the kernel's
  translations included.
- **New kernel entries do not propagate** (section 3.5).

---

## Changes to the files

What this chapter adds to the code of doc 10 (tag `doc-10`). Each file's link
leads to its state at the end of this chapter (tag `doc-11`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-10...doc-11).

| File | What changes | Where |
|---|---|---|
| [`src/memoria/espaco.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | **new**: `EspacoEnderecos` (`novo`, `raiz`, `mapear`, `traduzir`, `ativar`, `do_programa`, `entrada_livre`), `impl Drop`, `liberar_arvore`, 4 tests | — |
| [`src/memoria/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/mod.rs) | `pub mod espaco;` | after `pub mod paginacao;` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `tabela_em` and `nova_tabela` become `pub(super)` | both functions |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `AreaDoKernel` variant | end of `enum ErroMapeamento` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `mapear`/`desmapear` use `raiz_kernel()`; new `RAIZ_KERNEL`, `raiz_kernel`, `ativar` | right after `desmapear_em` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | `RAIZ_KERNEL.store(pml4, ...)` | in `init`, right after the `mov cr3` |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/usuario/mod.rs) | `use crate::memoria::espaco::EspacoEnderecos;`; `rodar` rewritten (section 4); `nada_fica_mapeado_nem_vaza` without the warm-up; diagram at the top of the file updated | — |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/shell.rs) | `espacos` command (line in `COMANDOS` and the function) | line before `rodar`; function before `rodar_programa` |

### The snippets the text did not show

The new error variant, in `paginacao.rs`:

```rust
    /// The frame allocator has no frame for a new table.
    SemMemoria,
    /// A program's space cannot map where the kernel lives (doc 11).
    AreaDoKernel,
}
```

The comments on `mapear` and `desmapear`, and where the new functions go:

```rust
/// Maps into the KERNEL's tables (not those of whichever program is active:
/// since doc 11, each program has its own PML4, and CR3 may point to it).
pub fn mapear(virt: u64, fisico: u64, tamanho: TamanhoPagina, bits: u64) -> Result<(), ErroMapeamento> {
    mapear_em(raiz_kernel(), virt, fisico, tamanho, bits)
}

pub fn desmapear(virt: u64) -> Result<u64, ErroMapeamento> {
    desmapear_em(raiz_kernel(), virt)
}

/// Physical address of the kernel's PML4 (built by `init`). Before `init`,
/// the active tables are UEFI's.
static RAIZ_KERNEL: AtomicU64 = AtomicU64::new(0);

pub fn raiz_kernel() -> u64 { ... }   // section 3.5

pub fn ativar(raiz: u64) { ... }      // section 3.3
```

The end of `init`:

```rust
    unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };
    RAIZ_KERNEL.store(pml4, Ordering::Relaxed);
```

The `use` lines and the simple methods of `espaco.rs`:

```rust
use super::paginacao::{self, tabela_em, nova_tabela, ErroMapeamento, TamanhoPagina, Traducao};
use super::quadros::{self, Quadro};

impl EspacoEnderecos {
    // novo (section 3.1)

    pub fn raiz(&self) -> u64 {
        self.raiz
    }

    // mapear (section 3.2)

    pub fn traduzir(&self, virt: u64) -> Option<Traducao> {
        paginacao::traduzir_em(self.raiz, virt)
    }

    pub fn ativar(&self) {
        paginacao::ativar(self.raiz);
    }

    // do_programa, entrada_livre (section 3.2)
}
```

The start of `espaco.rs`'s test module:

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::memoria::heap;
    use crate::memoria::paginacao::flags;

    const V: u64 = 0x0000_2000_0000_0000;

    fn pagina_com(valor: u64) -> Quadro {
        let q = quadros::alocar().unwrap();
        unsafe { (paginacao::fisico_para_virtual(q.endereco()) as *mut u64).write_volatile(valor) };
        q
    }

    // the 4 tests from section 5
}
```

The command's line in the shell table:

```rust
    Comando { nome: "espacos", uso: "espacos", descricao: "dois espacos de enderecos, mesmo endereco, bytes diferentes", funcao: espacos },
```

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `EspacoEnderecos::novo` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Creates a zeroed PML4 and copies into it the present entries of the kernel's PML4 (sharing the tables below them). |
| `EspacoEnderecos::raiz` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Physical address of the space's PML4. |
| `EspacoEnderecos::mapear` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Maps a 4 KiB page and takes ownership of the frame; rejects addresses in the kernel's area (`AreaDoKernel`) and frees the frame on error. |
| `EspacoEnderecos::traduzir` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | `traduzir_em` with this space's PML4. |
| `EspacoEnderecos::ativar` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Starts using this space (writes the root to CR3). |
| `EspacoEnderecos::do_programa` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Is the address in the lower half, in a PML4 entry the kernel does not use? |
| `EspacoEnderecos::entrada_livre` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Is entry `i` of the kernel's PML4 empty? |
| `EspacoEnderecos::drop` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | `Drop`: checks the space is not active and frees the trees of the program's entries and the PML4. |
| `liberar_arvore` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Recursively frees a table, the tables below it and the pages the PT points to. |
| `raiz_kernel` | [memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | Physical address of the kernel's PML4 (the active tables, before `init`). |
| `ativar` | [memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/paginacao.rs) | Writes a PML4 to CR3 (unless it is already active); the CPU empties the TLB. |
| `espacos` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/shell.rs) | `espacos` command: two live spaces with `ola` and `relogio` at the same address, showing each one's PML4, physical address and first bytes. |

Automated tests (`#[test_case]`, run with `cargo test`, see doc 09):

| Function | File | What it does |
|---|---|---|
| `pagina_com` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Test helper: allocates a frame and writes a value at its start. |
| `mesmo_endereco_paginas_diferentes` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Test: two spaces with different frames at the same address; different translations and real reads, nothing in the kernel's tables. |
| `kernel_aparece_igual_em_todos` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Test: a heap address translates the same in a new space and in the kernel's tables. |
| `destruir_devolve_todos_os_quadros` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Test: two pages 2 MiB apart cost 7 frames, and `drop` returns them all. |
| `area_do_kernel_recusada` | [memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-11/src/memoria/espaco.rs) | Test: mapping on the heap or in entry 0 gives `AreaDoKernel`, without leaking frames. |

---

## Difference from the book

The book mentions per-process address spaces only as part of the roadmap in
section 11.4, without code. Everything here is ours: copying the present
entries (instead of "the upper half", because SiliconOS's kernel still uses
entry 0), rejecting the kernel's area, the `Drop` that frees the tree and
moving `mapear` to the kernel's fixed root.

## Summary

- An **address space** is a tree of page tables with its own root (PML4).
  Switching **CR3** switches the whole space and empties the TLB.
- Each `EspacoEnderecos` **copies the kernel's PML4 entries** (0, 256, 288,
  320): 8 bytes each, pointing to the same tables, so the kernel looks the
  same (and up to date) in every space, protected by the missing USUARIO bit.
- The program only maps into entries the kernel does not use; anything else
  gives `AreaDoKernel`.
- The space **owns** the frames it maps. `Drop` frees pages, tables and the
  PML4, but never the kernel's tables or the active space.
- `rodar` activates the space before the `iretq` and switches back to the
  kernel's CR3 before destroying it. Doc 10's 3-frame leak is gone.
- The kernel's `mapear`/`desmapear` use the fixed root (`raiz_kernel`), not
  the active one.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **Why does the kernel keep working after writing the program's PML4 to
   CR3?**
   Because the program's PML4 has copies of the kernel's entries (0, 256,
   288, 320). The next instruction, the stack, the IDT and the heap are
   translated through the same tables as before.

2. **If the program has the kernel mapped in its space, why can't `invasor`
   read kernel memory?**
   The kernel's pages lack the USUARIO bit. In ring 3, the CPU refuses the
   access with a page fault ("page present, no permission, user mode").

3. **Why doesn't `Drop` free entry 0's tree, if it is in the lower half?**
   Because it was copied from the kernel: those are the identity tables,
   used by every space. `Drop` only frees entries the kernel's PML4 does not
   have.

4. **What would happen if `rodar` called `drop(espaco)` before switching
   back to the kernel's CR3?**
   The `assert` in `Drop` would fire ("destruindo o espaco de enderecos
   ativo"). Without it, the PML4 in use would go back to the allocator; once
   other code reused the frame, the CPU's translations would read garbage.

5. **Why does mapping a page in an inactive space not need `invlpg`?**
   The TLB only holds translations of the active space. When the space is
   activated, the write to CR3 empties the TLB, and the new translation is
   read from the tables.

6. **Why do two pages 1 MiB apart cost one PT less than two pages 2 MiB
   apart?**
   A PT has 512 entries of 4 KiB and covers 2 MiB. At 1 MiB apart, both
   pages fall in the same PT; at 2 MiB, each needs its own.

</details>
