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
| 07: Heap | `Box`, `Vec`, `String` dentro do kernel | 8 | ⏳ |
| 08: Interrupções de hardware | Controlador de interrupções, timer, teclado | 9 | ⏳ |
| 09: Shell e testes | Shell interativo, testes no QEMU, depuração com GDB | 10 | ⏳ |
| [Rust para quem já programa](rust-basico.md) | Referência de Rust | — | ✅ |
| [Glossário](glossario.md) | Referência de termos | — | ✅ |

---

## Como compilar e rodar

Pré-requisitos:

- **Rust nightly** com o alvo `x86_64-unknown-uefi` (o compilador gera um
  executável no formato que o UEFI entende):
  ```bash
  rustup toolchain install nightly
  rustup target add x86_64-unknown-uefi --toolchain nightly
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
```

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

Para mudar a memória da máquina emulada (padrão: 128 MiB), use `RAM`, por
exemplo `RAM=4G ./run.sh teste-quadros`.

---

## Mapa do código

```
siliconos/
├── Cargo.toml     Configuração do projeto: dependências e "features" de teste
├── run.sh         Compila, copia para esp/ e roda no QEMU
├── esp/           Pasta que o QEMU apresenta como um pendrive FAT
│   └── EFI/BOOT/BOOTX64.efi   ← o nosso kernel compilado
├── docs/          Esta documentação
└── src/
    ├── main.rs     Ponto de entrada. Fase UEFI → sai do firmware → kernel_main
    ├── serial.rs   Escreve na porta serial (canal de depuração)        [doc 01]
    ├── console.rs  Desenha texto na tela, pixel a pixel                [doc 01]
    ├── panic.rs    O que fazer quando o Rust detecta um erro           [doc 02]
    ├── gdt.rs      Tabela de segmentos e pilha de emergência           [doc 03]
    ├── idt.rs      Tabela de interrupções e tratadores de exceções     [doc 04]
    └── memoria/                                                         [doc 05]
        ├── mod.rs      constantes (tamanho do quadro, 1 MiB)
        ├── mapa.rs     cópia própria do mapa de memória do UEFI
        ├── quadros.rs  alocador de quadros físicos (bitmap)
        └── paginacao.rs  tabelas de página do kernel                    [doc 06]
```

O fluxo de execução atual, de ponta a ponta:

```
 firmware UEFI
   └─► main()                       [main.rs]  ainda com ajuda do firmware
         ├─ descobre onde fica a tela
         ├─ exit_boot_services()    ← dispensa o firmware, recebe o mapa de memória
         └─► kernel_main()          [main.rs]  a partir daqui a máquina é nossa
               ├─ cli                    desliga interrupções de hardware
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
               └─ loop { hlt }           dorme para sempre
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
