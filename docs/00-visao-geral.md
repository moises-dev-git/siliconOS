# 00: Visão geral: do botão de ligar até o kernel

> **Você vai entender:** o que é um kernel e por que escrever um é
> diferente de escrever um programa comum; o que acontece entre apertar o
> botão de ligar e o nosso código rodar; e por que o código Rust do
> SiliconOS tem um "dialeto" próprio (`no_std`, `unsafe`).
>
> **Arquivo principal:** `src/main.rs`
> **Livro:** capítulos 1 e 2
>
> **Código deste capítulo:** os docs 00 a 06 estão num único commit: [estado no fim do doc 06](https://github.com/moises-dev-git/siliconOS/tree/doc-06). O que este capítulo acrescenta está em [Mudanças nos arquivos](#mudanças-nos-arquivos).

---

## 1. O que é um kernel

Pense num programa que você já escreveu, por exemplo um backend em Java
que lê um arquivo e responde requisições HTTP. Esse programa **nunca falou
diretamente com o hardware**:

- Para ler o arquivo, ele chamou `Files.readAllBytes(...)`. Lá no fundo,
  isso virou um pedido ao **sistema operacional**: "leia tais bytes do
  disco para mim".
- Para abrir a porta 8080, a JVM pediu ao sistema operacional.
- Para alocar memória (`new Objeto()`), a JVM pediu blocos de memória ao
  sistema operacional.
- Quando o programa deu `NullPointerException` e morreu, foi o sistema
  operacional que limpou a bagunça e liberou a memória.

Quem atende todos esses pedidos é o **kernel**: o núcleo do sistema
operacional. Ele é o único programa que roda com **controle total** da CPU,
da memória e dos dispositivos. Todos os outros programas pedem as coisas a
ele.

```
 ┌─────────────────────────────────────────────┐
 │  seus programas (navegador, IDE, backend)    │  ← pedem coisas
 ├─────────────────────────────────────────────┤
 │  bibliotecas (libc, JVM, Python runtime)     │  ← traduzem os pedidos
 ├─────────────────────────────────────────────┤
 │  KERNEL  ← o que estamos escrevendo          │  ← atende os pedidos
 ├─────────────────────────────────────────────┤
 │  hardware (CPU, RAM, disco, vídeo, teclado)  │
 └─────────────────────────────────────────────┘
```

### Por que escrever um kernel é diferente

Quando você escreve o kernel, **não existe ninguém abaixo de você para
pedir ajuda**. Coisas que você sempre usou sem pensar simplesmente não
existem:

| Você está acostumado a... | No kernel... |
|---|---|
| `System.out.println` / `print()` | Não há terminal. Temos que desenhar cada letra na tela, pixel a pixel (doc 01). |
| `new` / alocar listas | Não há quem forneça memória. Escrevemos o nosso próprio alocador (docs 05 e 07). |
| Exceções com *stack trace* | Um erro trava ou reinicia a máquina sem mensagem nenhuma, até escrevermos os tratadores (docs 02 e 04). |
| Depurador da IDE | Só depois de muito trabalho. No começo, depuramos imprimindo texto. |
| Threads | Não existem até nós as criarmos. |

Por isso, desenvolver um SO segue uma ordem quase obrigatória, em que
cada etapa dá ferramentas para a próxima:

1. **Conseguir escrever texto** (doc 01). Sem isso, não dá para depurar nada.
2. **Mostrar erros em vez de travar** (docs 02 e 04).
3. **Colocar a CPU num estado conhecido**, que é nosso, não do firmware (doc 03).
4. **Gerenciar a memória** (próximos capítulos).
5. **Reagir ao hardware**: timer e teclado.
6. **Rodar outros programas** de forma isolada e segura.

O SiliconOS já concluiu as etapas 1 a 5: a memória (docs 05 a 07), as
interrupções de hardware, com timer e teclado (doc 08), e um shell
interativo com testes automáticos (doc 09). A etapa 6 começou: o doc 10
roda os primeiros programas isolados, no modo usuário, o doc 11 dá a cada
programa o seu próprio espaço de endereços, o doc 12 faz vários deles
rodarem ao mesmo tempo, com um escalonador, o doc 13 os carrega de
arquivos ELF, escritos em Rust, o doc 14 troca a porta de entrada no kernel
por `syscall`/`sysret`, o doc 15 deixa um processo esperar (dormir, ou
esperar uma tecla) sem gastar a CPU, o doc 16 entrega argumentos aos
programas (`rodar primos 5000`), e o doc 17 deixa os programas abrirem e
lerem arquivos, por um sistema de arquivos virtual.

---

## 2. O que acontece quando o PC liga

### 2.1 O firmware

A CPU, ao receber energia, não sabe nada sobre discos nem sistemas
operacionais. Ela simplesmente começa a executar instruções de um endereço
fixo, que aponta para um chip de memória na placa-mãe. Esse chip contém o
**firmware**.

O firmware:

1. testa e inicializa o hardware (memória RAM, placa de vídeo, USB...);
2. procura nos discos um programa para carregar;
3. carrega esse programa na memória e passa o controle para ele.

Existem duas gerações de firmware:

| | BIOS (desde 1981) | UEFI (desde ~2010) |
|---|---|---|
| Modo da CPU ao entregar o controle | 16 bits ("modo real", como um 8086) | **64 bits**, já configurado |
| O que carrega | os primeiros **512 bytes** do disco | um **arquivo** `.efi` de uma partição FAT |
| Serviços oferecidos | rotinas antigas e limitadas | vídeo, teclado, arquivos, memória, rede... |

Com BIOS, o primeiro trabalho de um sistema operacional é sair dos 16
bits, passar por 32 bits e chegar a 64 bits. É o que faz o clássico setor
de boot (o `boot_sect.asm` de muitos tutoriais de SO). Com UEFI, a CPU já
chega em 64 bits.

O SiliconOS usa **UEFI**.

### 2.2 Como o UEFI acha o SiliconOS

O UEFI procura uma partição formatada em FAT e, dentro dela, o arquivo
`EFI/BOOT/BOOTX64.EFI`. No SiliconOS:

1. O `cargo build --target x86_64-unknown-uefi` gera
   `target/x86_64-unknown-uefi/debug/siliconos.efi`. O alvo
   `x86_64-unknown-uefi` faz o compilador gerar um executável no formato
   **PE** (o mesmo formato dos `.exe` do Windows), que é o formato que o UEFI
   entende.
2. O `run.sh` copia esse arquivo para `esp/EFI/BOOT/BOOTX64.efi`.
3. O QEMU é iniciado com `-drive format=raw,file=fat:rw:esp`, que significa
   "finja que a pasta `esp/` é um disco FAT". O firmware (OVMF) acha o
   arquivo e o executa.

Para rodar num PC de verdade, bastaria copiar a pasta `EFI` para um
pendrive FAT32 e dar boot por ele. Por enquanto só testamos no QEMU.

---

## 3. As duas fases do SiliconOS

Enquanto o firmware UEFI está "no comando", ele oferece funções úteis,
chamadas **Boot Services**: escrever texto na tela, alocar memória, ler
arquivos. Mas ele também mantém **seus próprios** timers, drivers e
tabelas rodando. Um sistema operacional de verdade precisa do controle
total, então em algum momento ele diz ao firmware: **"obrigado, pode
sair"**. Isso é a função **`ExitBootServices`**.

Por isso o SiliconOS tem duas fases bem separadas:

```
 ┌─────────── FASE 1: UEFI (função main) ───────────┐
 │ • o firmware ainda ajuda: uefi::println! funciona │
 │ • perguntamos ao firmware onde fica a tela        │
 │ • chamamos exit_boot_services()                   │
 │   └─ o firmware nos entrega o MAPA DE MEMÓRIA     │
 └───────────────────────┬───────────────────────────┘
                         ▼
 ┌──────── FASE 2: BARE-METAL (função kernel_main) ──┐
 │ • o firmware não ajuda mais em nada               │
 │ • só funciona o que NÓS escrevemos                │
 │ • esta função nunca retorna                       │
 └───────────────────────────────────────────────────┘
```

O **mapa de memória** é uma lista de regiões da RAM com o tipo de cada
uma: "livre", "usada pelo firmware", "usada pelo programa carregado",
"pertence a um dispositivo"... É a partir dele que, no próximo capítulo,
vamos decidir qual memória o kernel pode usar.

### 3.1 O código da fase 1

```rust
#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    uefi::println!("Carregador UEFI iniciado.");
```

- `#[entry]` é um **atributo** (parecido com uma *annotation* do Java)
  fornecido pela crate `uefi`. Ele marca esta função como o ponto de
  entrada que o firmware vai chamar.
- `-> Status` indica que a função devolveria um código de status ao
  firmware, embora, como veremos, ela nunca chegue a retornar.
- `uefi::helpers::init()` prepara a crate `uefi` (por exemplo, para o
  `println!` funcionar). O `.unwrap()` quer dizer "se der erro, aborte",
  porque não há o que fazer se o firmware falhar logo no início (ver
  [rust-basico §6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)).

```rust
    let fb = {
        let handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
        let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();
        let info = gop.current_mode_info();
        let (width, height) = info.resolution();
        FrameBuffer {
            base: gop.frame_buffer().as_mut_ptr() as *mut u32,
            width,
            height,
            stride: info.stride(),
        }
    };
```

Aqui perguntamos ao firmware sobre o vídeo. O UEFI organiza seus serviços
em **protocolos** (pense em "interfaces de serviço"); o de vídeo se chama
**GOP** (*Graphics Output Protocol*). Pegamos dele:

- o **endereço** da memória de vídeo (o *framebuffer*, explicado no doc 01);
- a **largura** e a **altura** da tela em pixels;
- o **stride** (quantos pixels cada linha ocupa na memória).

Guardamos tudo numa struct nossa, `FrameBuffer`.

Por que o bloco `{ ... }` em volta? Em Rust, valores são destruídos quando
saem do escopo ([rust-basico §8](rust-basico.md#8-referências-e-o-borrow-checker-versão-curtíssima)).
Ao sair do bloco, o objeto `gop` é destruído, e a destruição dele **fecha
o protocolo** no firmware. Precisamos que isso aconteça **antes** de
dispensar o firmware. Se tentássemos fechar o protocolo depois, estaríamos
chamando um firmware que não existe mais. O que sobra do bloco é só a
struct `FrameBuffer`, com números simples.

```rust
    let memory_map = unsafe { uefi::boot::exit_boot_services(MemoryType::LOADER_DATA) };
    kernel_main(fb, &memory_map);
}
```

- `exit_boot_services` dispensa o firmware e devolve o mapa de memória. O
  argumento `MemoryType::LOADER_DATA` diz em que tipo de memória guardar a
  cópia do mapa.
- Está dentro de `unsafe` porque, depois desta linha, qualquer código que
  ainda tente usar o firmware quebra, e o compilador não tem como
  verificar isso por nós ([rust-basico §9](rust-basico.md#9-ponteiros-crus-e-unsafe)).
- `kernel_main` nunca retorna, por isso `main` também nunca chega ao fim.

### 3.2 O começo da fase 2

```rust
fn kernel_main(fb: FrameBuffer, memory_map: &impl MemoryMap) -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
    Serial::init();
    *CONSOLE.lock() = Some(Console::new(fb));
    ...
```

- `-> !` significa **"esta função nunca retorna"** ([rust-basico §3](rust-basico.md#3-funções-e-o-tipo--nunca-retorna)).
  Ela termina num loop infinito.
- `cli` é uma instrução da CPU que **desliga as interrupções de hardware**.
  O motivo é explicado no doc 03.
- Depois inicializamos a serial e a tela (doc 01), a GDT (doc 03) e a IDT
  (doc 04), e imprimimos um resumo do mapa de memória.

---

## 4. Rust sem sistema operacional: `no_std` e `no_main`

As primeiras linhas do `main.rs`:

```rust
#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
```

### 4.1 `#![no_std]`: sem a biblioteca padrão

A biblioteca padrão do Rust (`std`) é como a biblioteca padrão do Java:
traz arquivos, rede, threads, `println!`, coleções. Só que **quase tudo
nela depende de um sistema operacional por baixo**: `println!` escreve no
terminal *via o SO*; `Vec` pede memória *ao SO*; `File` abre arquivos *pelo
SO*.

Como nós **somos** o sistema operacional, não podemos usá-la. Sobra a
**`core`**, a parte do Rust que não depende de nada: tipos numéricos,
`Option`, `Result`, iteradores, formatação de texto, operações atômicas
([rust-basico §17](rust-basico.md#17-no_std-core-e-alloc)).

O que a `std` fazia e agora é responsabilidade nossa:

| A `std` fornecia | No SiliconOS |
|---|---|
| `println!` | `kprintln!`, que escreve na serial e na tela (doc 01) |
| o que fazer quando o programa entra em pânico | `#[panic_handler]` (doc 02) |
| alocador de memória (`Vec`, `String`, `Box`) | o heap do kernel (doc 07) |
| ponto de entrada `main` | `#[entry]` da crate `uefi` |

### 4.2 `#![no_main]`

Num programa Rust comum, existe um código escondido que roda **antes** do
seu `main` (prepara argumentos, variáveis de ambiente, threads). Esse
código depende do SO. `#![no_main]` diz: "não gere esse código; o ponto de
entrada eu forneço". No nosso caso, quem fornece é o `#[entry]`.

### 4.3 `#![feature(...)]`

Liga um recurso experimental do Rust *nightly*. Usamos um só, explicado no
doc 04.

---

## 5. Por que tem tanto `unsafe`?

Rust garante, em tempo de compilação, que você não vai acessar memória
inválida, desde que o compilador consiga **provar** que o acesso é válido.

Um kernel faz coisas que o compilador **não tem como provar**:

- "Escreva a cor azul no endereço `0x80000000`." Nós sabemos que ali fica a
  tela porque o firmware disse; o compilador não sabe.
- "Execute a instrução `out` para mandar um byte à porta serial."
- "Troque a tabela de segmentos da CPU."

Para essas situações existe o `unsafe { ... }`, que significa:
**"compilador, aqui quem garante a correção sou eu"**. Ele não desliga as
outras verificações; só libera algumas operações extras.

A disciplina que seguimos (e que o livro defende no capítulo 8.4):

1. blocos `unsafe` **pequenos**, cada um com um comentário dizendo por que
   é seguro;
2. por cima deles, **funções seguras**. Exemplo: a função `outb`
   (mandar um byte para uma porta) é `unsafe`, mas `Serial::write_byte` não
   é. Quem usa a serial não precisa saber nada de portas.

---

## 6. Ferramentas

| Ferramenta | Para que serve |
|---|---|
| **Cargo** | compila o projeto e baixa dependências (como Maven/Gradle/npm) |
| alvo **`x86_64-unknown-uefi`** | faz o compilador gerar um `.efi` em vez de um executável Linux |
| **QEMU** | emula um PC inteiro. Liga em 1 segundo, mostra a saída serial no terminal, não estraga nada se o kernel travar |
| **OVMF** | firmware UEFI de código aberto, usado pelo QEMU no lugar do firmware de uma placa-mãe real |

Dependências (crates) do projeto, declaradas no `Cargo.toml`:

| Crate | Para que usamos |
|---|---|
| `uefi` | conversar com o firmware na fase 1 (vídeo, mapa de memória, exit_boot_services) |
| `spin` | `Mutex` e `Once` que funcionam sem sistema operacional |
| `font8x8` | o desenho das letras (uma tabela de bits) |

---

## Mudanças nos arquivos

Onde o código começa. Os docs 00 a 06 estão no primeiro commit do
repositório, então não há uma versão de cada um: o link de cada arquivo
leva ao código **no fim do doc 06**, que já tem o que os capítulos seguintes
acrescentam. Os pré-requisitos (Rust nightly, o alvo `x86_64-unknown-uefi`,
QEMU e OVMF) estão na introdução, seção "Como compilar e rodar".

| Arquivo | O que muda | Onde |
|---|---|---|
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/Cargo.toml) | **novo**: pacote `siliconos`, edição 2024, dependência `uefi = { version = "0.33.0", features = ["logger"] }` | raiz do projeto |
| [`run.sh`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/run.sh) | **novo**: compila com `cargo build --target x86_64-unknown-uefi`, copia para `esp/EFI/BOOT/BOOTX64.efi` e abre o QEMU com o OVMF | raiz do projeto |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | **novo**: `#![no_std]`, `#![no_main]`, `#[entry] fn main()` (inicia os *helpers* do UEFI, escreve uma mensagem, chama `exit_boot_services`) e `fn kernel_main(...) -> !`, que começa com `cli` e termina em `loop { hlt }` | — |

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `main` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | Ponto de entrada UEFI (`#[entry]`). Guarda o framebuffer e a RSDP, sai dos serviços de boot e chama `kernel_main`. |
| `kernel_main` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-06/src/main.rs) | O kernel propriamente dito: inicializa cada subsistema na ordem e, no fim, fica num laço. Cada capítulo acrescenta um passo (GDT, IDT, memória, heap, interrupções; desde o doc 09, entrega o controle ao shell). Nunca retorna. |


---

## Resumo

- O **kernel** é o programa que controla o hardware e atende os pedidos de
  todos os outros. Escrevendo um, você perde tudo que o SO fornecia.
- O **firmware UEFI** liga o PC, coloca a CPU em 64 bits e executa o
  arquivo `EFI/BOOT/BOOTX64.EFI`, que é o SiliconOS.
- O SiliconOS tem **duas fases**: com a ajuda do firmware (descobre a tela) e
  sem ela, depois de `exit_boot_services` (o resto do kernel).
- `#![no_std]` troca a biblioteca padrão pela `core`, que não depende de
  SO. `unsafe` marca os trechos em que o programador garante a correção.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que o SiliconOS pega as informações de vídeo *antes* de chamar
   `exit_boot_services`?**
   Porque depois dessa chamada o firmware não existe mais, e não há a quem
   perguntar onde fica a tela.

2. **Por que não dá para usar `println!` comum no kernel?**
   Porque ele faz parte da `std`, que pede ao sistema operacional para
   escrever no terminal. Não há SO abaixo do kernel.

3. **O que significa o `-> !` em `kernel_main`?**
   Que a função nunca retorna. Ela termina num loop infinito, porque não há
   para onde voltar.

</details>
