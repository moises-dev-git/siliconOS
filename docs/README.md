# SiliconOS: documentação

O SiliconOS é um sistema operacional para PCs de 64 bits (arquitetura
**x86-64**), escrito em **Rust**, que liga diretamente pelo firmware
**UEFI**. Ele é um projeto de aprendizado: cada parte do código existe para
ensinar uma peça de como um sistema operacional funciona.

Estes documentos explicam **o que** cada parte faz, **por que** ela é
necessária e **como** o código a implementa, linha por linha quando vale a
pena. Eles partem do princípio de que você:

- já programa (em Java, Kotlin, JavaScript, Python...),
- **nunca** escreveu um sistema operacional,
- **nunca** programou em Rust.

O código-fonte completo está em
[github.com/moises-dev-git/siliconOS](https://github.com/moises-dev-git/siliconOS).

Os textos são originais. A ordem dos temas acompanha o livro *Writing
Operating Systems in Rust* (Aura Fenwick), para você poder ler o capítulo
do livro e depois o documento correspondente.

---

## Por onde começar

1. Leia o **[00: Visão geral](00-visao-geral.md)** inteiro. Ele dá o mapa
   de tudo: o que é um kernel, como o PC liga e o que o SiliconOS faz do boot
   até agora.
2. Mantenha abertos, para consulta:
   - **[Rust para quem já programa](rust-basico.md)**: só os recursos de
     Rust usados no projeto, comparados com Java, Kotlin, JS e Python;
   - **[Glossário](glossario.md)**: registradores, pilha, anéis,
     interrupções, hexadecimal...
3. Siga os capítulos em ordem. Cada um começa dizendo o que você vai
   aprender e termina com um resumo e perguntas para fixar.

---

## Índice

| Doc | Assunto | Capítulo do livro | Estado |
|---|---|---|---|
| [00: Visão geral](00-visao-geral.md) | O que é um kernel, como o PC liga, UEFI, Rust sem sistema operacional | 1 e 2 | ✅ |
| [01: Serial e vídeo](01-saida-serial-e-video.md) | Como o kernel escreve texto: porta serial, framebuffer, fonte, `kprintln!` | 3.1 e 3.3 | ✅ |
| [02: Panic handler](02-panic-handler.md) | O que fazer quando o kernel encontra um erro do Rust | 3.2 | ✅ |
| [03: GDT e TSS](03-gdt-e-tss.md) | Segmentos, níveis de privilégio, pilhas de emergência | 4 | ✅ |
| [04: IDT e exceções](04-idt-e-excecoes.md) | Como tratar erros detectados pela CPU (page fault, double fault...) | 5 | ✅ |
| [05: Memória física](05-memoria-fisica.md) | Mapa de memória, buracos, alocador de quadros (bitmap) | 6 | ✅ |
| [06: Paginação](06-paginacao.md) | Endereços virtuais, tabelas de 4 níveis, TLB, as tabelas do kernel | 7 | ✅ |
| [07: Heap](07-heap.md) | `Box`, `Vec`, `String` dentro do kernel: alocadores bump e lista de blocos livres | 8 | ✅ |
| [08: Interrupções de hardware](08-interrupcoes-de-hardware.md) | 8259, APIC e ACPI; timer a 100 Hz; teclado PS/2; deadlock | 9 | ✅ |
| [09: Shell e testes](09-shell-e-testes.md) | Shell interativo, `cargo test` no QEMU, depuração com GDB | 10 | ✅ |
| [10: Modo usuário](10-modo-usuario.md) | Anel 3, `iretq`, chamadas de sistema (`int 0x80`), programas isolados | 11.4 | ✅ |
| [11: Espaços de endereços](11-espacos-de-enderecos.md) | Uma PML4 por programa, kernel compartilhado, troca de CR3, `Drop` que devolve os quadros | 11.4 | ✅ |
| [12: O escalonador](12-escalonador.md) | Processos, troca de contexto, pilha do kernel por processo, preempção pelo timer, `ps` e `&` | 11.4 | ✅ |
| [13: Disco em RAM e ELF](13-disco-em-ram-e-elf.md) | Programas de usuário em Rust, script de ligação, formato tar, carregador de ELF | 11.4 | ✅ |
| [14: syscall e sysret](14-syscall-sysret.md) | A porta rápida para o kernel: MSRs STAR/LSTAR/FMASK, troca de pilha manual, medição em ciclos | 11.4 | ✅ |
| [Rust para quem já programa](rust-basico.md) | Referência de Rust | — | ✅ |
| [Glossário](glossario.md) | Referência de termos | — | ✅ |

---

## Como compilar e rodar

Pré-requisitos:

- **Rust nightly** com o alvo `x86_64-unknown-uefi` (o compilador gera um
  executável no formato que o UEFI entende) e, desde o doc 13, o alvo
  `x86_64-unknown-none` (para os programas de usuário em Rust):
  ```bash
  rustup toolchain install nightly
  rustup target add x86_64-unknown-uefi x86_64-unknown-none --toolchain nightly
  ```
- **QEMU** (`qemu-system-x86_64`), o emulador de PC;
- **OVMF**, o firmware UEFI que o QEMU usa. No Ubuntu/Debian, os dois vêm
  com `sudo apt install qemu-system-x86 ovmf`, e o firmware fica em
  `/usr/share/ovmf/OVMF.fd` (em outras distribuições o caminho pode mudar).

O script `run.sh` faz tudo: compila, copia o executável para a pasta `esp/`
e abre o QEMU.

```bash
./run.sh                    # roda normalmente: janela do QEMU + mensagens no terminal
SEM_TELA=1 ./run.sh         # sem janela; depois de 15 s mostra o que o kernel escreveu
./run.sh teste-pagefault    # compila uma versão que provoca um erro de propósito
cargo test --target x86_64-unknown-uefi   # testes automáticos, dentro do QEMU (doc 09)
./depurar.sh                # QEMU + GDB conectado, com os nomes das funções (doc 09)
```

Com o kernel rodando, clique na janela do QEMU e digite `ajuda` para ver os
comandos do shell.

Versões de teste disponíveis (cada uma provoca um tipo de erro para provar
que o kernel o trata):

| Comando | O que provoca | Doc |
|---|---|---|
| `./run.sh teste-panic` | erro detectado pelo Rust (índice fora do array) | 02 |
| `./run.sh teste-breakpoint` | instrução de breakpoint (o kernel deve **continuar**) | 04 |
| `./run.sh teste-divisao` | divisão por zero | 04 |
| `./run.sh teste-opcode` | instrução inválida | 04 |
| `./run.sh teste-gpf` | endereço impossível (proteção geral) | 04 |
| `./run.sh teste-pagefault` | leitura de memória que não existe | 04 |
| `./run.sh teste-doublefault` | pilha destruída (erro durante o tratamento de outro erro) | 04 |
| `./run.sh teste-quadros` | aloca, escreve, confere, libera e realoca 1000 quadros | 05 |
| `./run.sh teste-liberacao-dupla` | libera o mesmo quadro duas vezes (deve dar panic) | 05 |
| `./run.sh listar-regioes` | não é teste: lista o mapa de memória completo no boot | 05 |
| `./run.sh teste-paginacao` | mapeia, traduz, lê/escreve por 2 caminhos, desmapeia | 06 |
| `./run.sh teste-desmapeado` | acessa uma página depois de desmapeá-la (deve dar page fault) | 06 |
| `./run.sh teste-nulo` | lê o endereço 0 (deve dar page fault) | 06 |
| `./run.sh teste-heap` | `Box`, `Vec` crescendo, `String`, 15 mil alocações, alinhamento, vazamento zero | 07 |
| `./run.sh teste-heap-liberacao-dupla` | libera o mesmo bloco do heap duas vezes (deve dar panic) | 07 |
| `./run.sh teste-heap-esgotar` | aloca 1 MiB por vez até o limite de 64 MiB (deve dar panic) | 07 |
| `./run.sh teste-heap-executar` | executa código gravado no heap (deve dar page fault: NX) | 07 |
| `./run.sh teste-bump` | troca o alocador pelo bump: a memória liberada nunca volta | 07 |
| `./run.sh teste-timer` | confere os tiques do timer contra o relógio CMOS (RTC) | 08 |
| `./run.sh teste-sem-eoi` | tratador do timer sem EOI (o timer para no 1º tique) | 08 |
| `./run.sh teste-print-interrupcao` | `kprint!` dentro do tratador do timer (funciona) | 08 |
| `./run.sh teste-deadlock` | o mesmo sem a proteção do `kprintln!` (deve travar) | 08 |
| `./run.sh teste-teclado` | mostra cada scancode recebido do teclado | 08 |
| `cargo test --target x86_64-unknown-uefi --features teste-que-falha` | inclui um `#[test_case]` que falha (o `cargo test` deve falhar) | 09 |

Para mudar a memória da máquina emulada (padrão: 128 MiB), use `RAM`, por
exemplo `RAM=4G ./run.sh teste-quadros`.

---

## Mapa do código

```
siliconos/
├── Cargo.toml     Configuração do projeto: dependências e "features" de teste
├── run.sh         Compila, copia para esp/ e roda no QEMU
├── testar.sh      Runner do `cargo test`: roda o .efi de teste no QEMU   [doc 09]
├── depurar.sh     QEMU com servidor do GDB + GDB com os símbolos         [doc 09]
├── .cargo/config.toml  diz ao Cargo para usar o testar.sh
├── esp/           Pasta que o QEMU apresenta como um pendrive FAT
│   └── EFI/BOOT/BOOTX64.efi   ← o nosso kernel compilado
├── docs/          Esta documentação
└── src/
    ├── main.rs     Ponto de entrada. Fase UEFI → sai do firmware → kernel_main
    ├── serial.rs   Escreve na porta serial (canal de depuração)        [doc 01]
    ├── console.rs  Desenha texto na tela, pixel a pixel                [doc 01]
    ├── panic.rs    O que fazer quando o Rust detecta um erro           [doc 02]
    ├── shell.rs    A linha de comando: edição, tabela de comandos      [doc 09]
    ├── usuario/                                                         [doc 10]
    │   ├── mod.rs      entrar/sair do anel 3, chamadas de sistema
    │   └── programas.rs  os programas de usuário (assembly)
    ├── testes.rs   Framework de testes do `cargo test`                 [doc 09]
    ├── qemu.rs     Sai do QEMU com código (isa-debug-exit)             [doc 09]
    ├── gdt.rs      Tabela de segmentos e pilha de emergência           [doc 03]
    ├── idt.rs      Tabela de interrupções e tratadores de exceções     [doc 04]
    ├── cpu.rs      cli/sti, sem_interrupcoes, leitura de MSR           [doc 08]
    ├── acpi.rs     acha a MADT: onde estão os controladores            [doc 08]
    ├── interrupcoes/                                                    [doc 08]
    │   ├── mod.rs      vetores e a sequência de inicialização
    │   ├── pic.rs      desliga o 8259 antigo
    │   ├── apic.rs     APIC local (dentro da CPU)
    │   ├── ioapic.rs   I/O APIC (IRQs dos dispositivos)
    │   ├── timer.rs    timer do APIC calibrado com o PIT; RTC
    │   └── teclado.rs  PS/2: fila sem lock e tradução de scancodes
    └── memoria/                                                         [doc 05]
        ├── mod.rs      constantes (tamanho do quadro, 1 MiB)
        ├── mapa.rs     cópia própria do mapa de memória do UEFI
        ├── quadros.rs  alocador de quadros físicos (bitmap)
        ├── paginacao.rs  tabelas de página do kernel                    [doc 06]
        ├── heap.rs     o heap: lista de blocos livres + #[global_allocator] [doc 07]
        └── bump.rs     alocador bump, só para comparação               [doc 07]
```

O fluxo de execução atual, de ponta a ponta:

```
 firmware UEFI
   └─► main()                       [main.rs]  ainda com ajuda do firmware
         ├─ descobre onde fica a tela
         ├─ exit_boot_services()    ← dispensa o firmware, recebe o mapa de memória
         └─► kernel_main()          [main.rs]  a partir daqui a máquina é nossa
               ├─ cli                    desliga interrupções de hardware (até o doc 08)
               ├─ Serial::init()         [serial.rs]
               ├─ Console::new()         [console.rs]
               ├─ gdt::init()            [gdt.rs]
               ├─ idt::init()            [idt.rs]
               ├─ (testes de exceção, se compilado com alguma feature de teste)
               ├─ memoria::mapa::init()  [memoria/mapa.rs]     + resumo do mapa
               ├─ memoria::quadros::init() [memoria/quadros.rs] + resumo do alocador
               ├─ (testes de memória, se compilado com alguma feature de teste)
               ├─ memoria::paginacao::init() monta as tabelas do kernel e troca o CR3
               ├─ (testes de paginação, se compilado com alguma feature de teste)
               ├─ memoria::heap::init()  mapeia 1 MiB e liga o alocador global
               ├─ demonstrar_heap()      Box, Vec e String no boot normal
               ├─ (testes do heap, se compilado com alguma feature de teste)
               ├─ interrupcoes::init()   ACPI, 8259, APICs, timer, teclado e sti
               ├─ (testes de interrupção, se compilado com alguma feature de teste)
               ├─ (cargo test: rodar_testes() roda os #[test_case] e sai do QEMU)
               └─ shell::rodar()         dorme (hlt); cada tecla vai para o shell
                    └─ rodar <programa>  usuario::rodar(): anel 3 até o programa terminar
```

---

## Onde o SiliconOS difere do livro

O livro usa um **bootloader pronto** (um programa de terceiros que carrega
o kernel e entrega as informações para ele) e um alvo de compilação
personalizado, como no projeto de exemplo que acompanha o livro.

O SiliconOS não usa bootloader: **ele mesmo é um programa UEFI**. O firmware o
carrega, ele coleta as informações de vídeo e memória e então dispensa o
firmware para assumir a máquina. O livro aborda esse caminho no capítulo
11.

Na prática, muda só **como o kernel recebe as informações iniciais**. Do
capítulo 3 ao 10, os conceitos são os mesmos. Quando algum detalhe for
diferente, o documento avisa numa seção "Diferença em relação ao livro".

Outra diferença: o livro usa a crate (biblioteca) `x86_64`, que já traz
estruturas prontas para GDT, IDT etc. O SiliconOS monta essas estruturas **à
mão, bit a bit**, para que fique visível o que essas abstrações escondem.
