# SiliconOS

**English** | [Português](README.pt-BR.md)

An operating system for x86-64 PCs, written in **Rust**, that boots
straight from the **UEFI** firmware, with no third-party bootloader. It is a
learning project: the CPU structures (GDT, TSS, IDT, page tables) are built
by hand, bit by bit, to show what ready-made libraries hide.

📖 **Step-by-step documentation:** https://siliconos.pages.dev/en/ (English)
· https://siliconos.pages.dev (Portuguese)
— every stage explained for people who program, but have never written an
OS or programmed in Rust. Each chapter's comments live in the
[Discussions](https://github.com/moises-dev-git/siliconOS/discussions).

> The source code (identifiers, comments and the kernel's messages) is in
> Portuguese. The English documentation translates the comments in every
> snippet it shows and explains the names it uses.

## Status

| Stage | Code | Doc |
|---|---|---|
| UEFI boot and leaving the firmware's services | `src/main.rs` | [00](docs-en/00-visao-geral.md) |
| Serial port, text on screen, `kprintln!` | `src/serial.rs`, `src/console.rs` | [01](docs-en/01-saida-serial-e-video.md) |
| Custom panic handler | `src/panic.rs` | [02](docs-en/02-panic-handler.md) |
| GDT and TSS | `src/gdt.rs` | [03](docs-en/03-gdt-e-tss.md) |
| IDT and CPU exceptions | `src/idt.rs` | [04](docs-en/04-idt-e-excecoes.md) |
| Memory map and frame allocator | `src/memoria/` | [05](docs-en/05-memoria-fisica.md) |
| Paging | `src/memoria/paginacao.rs` | [06](docs-en/06-paginacao.md) |
| Heap: free list, grows on demand, NX pages | `src/memoria/heap.rs` | [07](docs-en/07-heap.md) |
| Hardware interrupts (timer, keyboard) | — | in progress |

## Running

Requires Rust nightly with the `x86_64-unknown-uefi` target, QEMU and OVMF
(details in [docs-en/README.md](docs-en/README.md#how-to-build-and-run)).

```bash
./run.sh                  # builds and opens it in QEMU
SEM_TELA=1 ./run.sh       # no window, shows the serial output
./run.sh teste-pagefault  # test builds: see docs-en/README.md
```

The site is generated with [mdBook](https://rust-lang.github.io/mdBook/)
from `docs/` (Portuguese) and `docs-en/` (English); configuration in
`site/`.

## License

[MIT](LICENSE).
