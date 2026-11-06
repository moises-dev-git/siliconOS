# SiliconOS

[English](README.md) | **Português**

Um sistema operacional para PCs x86-64, escrito em **Rust**, que liga direto
pelo firmware **UEFI**, sem bootloader de terceiros. É um projeto de
aprendizado: as estruturas da CPU (GDT, TSS, IDT, tabelas de página) são
montadas à mão, bit a bit, para mostrar o que as bibliotecas prontas escondem.

📖 **Documentação didática:** https://siliconos.pages.dev (português) ·
https://siliconos.pages.dev/en/ (inglês)
— cada etapa explicada para quem programa, mas nunca escreveu um SO nem
programou em Rust. Os comentários de cada capítulo ficam nas
[Discussions](https://github.com/moises-dev-git/siliconOS/discussions).

## Estado

| Etapa | Código | Doc |
|---|---|---|
| Boot UEFI e saída dos serviços do firmware | `src/main.rs` | [00](docs/00-visao-geral.md) |
| Porta serial, texto na tela, `kprintln!` | `src/serial.rs`, `src/console.rs` | [01](docs/01-saida-serial-e-video.md) |
| Panic handler próprio | `src/panic.rs` | [02](docs/02-panic-handler.md) |
| GDT e TSS | `src/gdt.rs` | [03](docs/03-gdt-e-tss.md) |
| IDT e exceções da CPU | `src/idt.rs` | [04](docs/04-idt-e-excecoes.md) |
| Mapa de memória e alocador de quadros | `src/memoria/` | [05](docs/05-memoria-fisica.md) |
| Paginação | `src/memoria/paginacao.rs` | [06](docs/06-paginacao.md) |
| Heap: lista de blocos livres, cresce sob demanda, páginas NX | `src/memoria/heap.rs` | [07](docs/07-heap.md) |
| Interrupções de hardware: APIC via ACPI, timer a 100 Hz, teclado PS/2 | `src/interrupcoes/`, `src/acpi.rs` | [08](docs/08-interrupcoes-de-hardware.md) |
| Shell, `cargo test` dentro do QEMU, depuração com GDB | `src/shell.rs`, `src/testes.rs` | [09](docs/09-shell-e-testes.md) |

## Rodar

Requer Rust nightly com o alvo `x86_64-unknown-uefi`, QEMU e OVMF
(detalhes em [docs/README.md](docs/README.md#como-compilar-e-rodar)).

```bash
./run.sh                  # compila e abre no QEMU
SEM_TELA=1 ./run.sh       # sem janela, mostra a saída serial
./run.sh teste-pagefault  # versões de teste: veja docs/README.md
cargo test --target x86_64-unknown-uefi   # testes automáticos no QEMU
./depurar.sh              # QEMU + GDB
```

O site é gerado com [mdBook](https://rust-lang.github.io/mdBook/) a partir de
`docs/` (configuração em `site/`).

## Licença

[MIT](LICENSE).
