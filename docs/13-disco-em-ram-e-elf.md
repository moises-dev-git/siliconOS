# 13: Disco em RAM e executáveis ELF

> **Você vai entender:** como escrever programas de usuário **em Rust** (e
> não mais em assembly) para um sistema que não tem biblioteca padrão;
> o que um **script de ligação** decide sobre o executável; como é por
> dentro um arquivo **ELF** e o que um carregador precisa fazer com cada
> **segmento** (inclusive a `.bss`, que não existe no arquivo); como o
> formato **tar** guarda vários arquivos num só; e por que um carregador
> deve desconfiar de cada número que lê.
>
> **Arquivos:** `programas/` (novo: o crate dos programas de usuário, com a
> biblioteca `sos`, três programas e o `linker.ld`), `build.rs` (novo:
> compila os programas e monta o tar), `src/ramdisk.rs` (novo: lê o tar),
> `src/usuario/elf.rs` (novo: lê e carrega ELF), `iniciar_elf` e
> `faixa_do_programa` em `src/usuario/mod.rs`, os comandos `ls`, `elf` e
> `rodar` em `src/shell.rs`
> **Livro:** capítulo 11, seção 11.4 (o roteiro começa com "disco em RAM",
> "sistema de arquivos virtual" e "carregar um programa"; a implementação é
> nossa)
> **Testar:** `./run.sh`, depois `ls`, `elf oi`, `elf dados`, `rodar oi`,
> `rodar primos`, `rodar dados`; `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 10 (anel 3 e chamadas de sistema), doc 11 (espaços de
> endereços) e doc 12 (escalonador)
> **Rust novo aqui:** `build.rs` e `include_bytes!` ([§40](rust-basico.md#40-buildrs-e-include_bytes-trabalho-feito-na-compilação)),
> implementar `Iterator` e tempos de vida em structs ([§41](rust-basico.md#41-implementar-iterator-e-tempos-de-vida-em-structs)),
> `macro_rules!` ([§42](rust-basico.md#42-macro_rules-escrever-as-próprias-macros)),
> genéricos com constantes ([§43](rust-basico.md#43-genéricos-com-constantes-const-n-usize))
>
> **Código deste capítulo:** [estado no fim do doc 13](https://github.com/moises-dev-git/siliconOS/tree/doc-13) · [o que mudou desde o doc 12](https://github.com/moises-dev-git/siliconOS/compare/doc-12...doc-13) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

Os programas dos docs 10 a 12 são assembly escrito à mão, dentro do
próprio kernel (`programas.rs`). Funcionam, mas:

- escrever qualquer coisa útil em assembly é lento (o `contador` precisou
  de 26 linhas para imprimir um dígito 5 vezes);
- eles fazem parte do kernel: um programa novo exige recompilar o kernel e
  mexer na tabela `PROGRAMAS`;
- cada um cabe numa página e usa o mesmo leiaute fixo (código numa página,
  pilha na outra). Um programa de verdade tem código, constantes, variáveis
  globais com valor inicial e variáveis zeradas, cada parte com permissões
  diferentes.

Num sistema de verdade, um programa é um **arquivo**: o compilador gera um
executável num formato padrão (no Linux, ELF), o arquivo fica num disco, e
o kernel, quando pedem para rodá-lo, **lê o arquivo e monta o processo** a
partir do que o arquivo descreve.

Este capítulo faz isso em três partes:

```
  no computador de desenvolvimento                     no SiliconOS
  ┌─────────────────────────────────────┐   kernel    ┌───────────────────────────────┐
  │ programas/src/bin/oi.rs  ─┐         │   .efi com  │ ramdisk: lê o tar             │
  │ programas/src/bin/primos.rs├ cargo ─┤ ─ o tar ──► │ elf: valida e lê os segmentos │
  │ programas/src/bin/dados.rs─┘  (ELF) │  embutido   │ iniciar_elf: espaço + pilha → │
  │ build.rs: ELF → ramdisk.tar         │             │   escalonador                 │
  └─────────────────────────────────────┘             └───────────────────────────────┘
```

## 2. Programas de usuário em Rust

### 2.1 Um crate separado, para outro alvo

O kernel é compilado para `x86_64-unknown-uefi` (doc 00): um executável
PE que o firmware UEFI sabe carregar. Os programas de usuário não rodam no
UEFI; rodam no SiliconOS, que ainda não é um alvo conhecido pelo Rust. O
alvo mais próximo é **`x86_64-unknown-none`**: x86-64, sem sistema
operacional, sem biblioteca padrão, gerando ELF. (Instalação, se faltar:
`rustup target add x86_64-unknown-none`.)

Por isso os programas ficam num crate à parte, na pasta `programas/`:

```
programas/
├── Cargo.toml          o crate, com [workspace] vazio: não faz parte do kernel
├── .cargo/config.toml  alvo e flags de compilação
├── build.rs            uma linha: religar quando o linker.ld mudar
├── linker.ld           o script de ligação (seção 2.3)
└── src/
    ├── lib.rs          a biblioteca `sos` (seção 2.2)
    └── bin/
        ├── oi.rs       diz oi com o próprio pid
        ├── primos.rs   conta os primos abaixo de 10 000
        └── dados.rs    mostra .data e .bss funcionando
```

```toml
[package]
name = "programas"
version = "0.1.0"
edition = "2024"
license = "MIT"

# Um workspace próprio: este crate não faz parte do crate do kernel.
[workspace]

[lib]
name = "sos"            # a "biblioteca do sistema" que os programas usam
path = "src/lib.rs"

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
opt-level = "s"         # programas pequenos
```

Cada arquivo em `src/bin/` vira um executável com o mesmo nome; todos
usam a biblioteca `src/lib.rs`. `panic = "abort"`: sem sistema por baixo,
não há como "desenrolar" a pilha num panic (doc 02).

### 2.2 A biblioteca `sos`: o que a libc faz no Linux

Um programa `no_std` não tem `println!` nem `std::process::exit`. Num
sistema de verdade, quem fornece essas coisas é a **biblioteca do sistema**
(no Linux, a libc): funções comuns que, por dentro, fazem chamadas de
sistema. A nossa é `programas/src/lib.rs`, e cada função é um `int 0x80`
(doc 10):

```rust
#![no_std]

use core::arch::asm;
use core::fmt;

// Números das chamadas de sistema (os mesmos de src/usuario/mod.rs do kernel).
const SYS_SAIR: u64 = 0;
const SYS_ESCREVER: u64 = 1;
const SYS_TIQUES: u64 = 2;
const SYS_PID: u64 = 3;

/// Termina o programa com o código dado. Não volta.
pub fn sair(codigo: u64) -> ! {
    unsafe { asm!("int 0x80", in("rax") SYS_SAIR, in("rdi") codigo, options(noreturn)) }
}

/// Escreve os bytes no console. Devolve quantos foram escritos, ou um erro
/// negativo (-14 se o kernel recusar o ponteiro).
pub fn escrever(bytes: &[u8]) -> i64 {
    let r: u64;
    unsafe {
        // O kernel preserva todos os registradores, menos RAX (o resultado).
        asm!("int 0x80", inlateout("rax") SYS_ESCREVER => r,
             in("rdi") bytes.as_ptr(), in("rsi") bytes.len(), options(nostack));
    }
    r as i64
}

/// Tiques do timer desde o boot (100 por segundo).
pub fn tiques() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") SYS_TIQUES => r, options(nostack)) };
    r
}

/// O número deste processo.
pub fn pid() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") SYS_PID => r, options(nostack)) };
    r
}
```

O `asm!` (doc 01, [§15](rust-basico.md#15-asm-assembly-dentro-do-rust))
diz ao compilador exatamente o contrato do doc 10: o número vai em RAX, os
argumentos em RDI e RSI, o resultado volta em RAX (`inlateout`: entra um
valor, sai outro no mesmo registrador), e nenhum outro registrador muda
(o tratador `entrada_syscall` restaura todos). `options(noreturn)` em
`sair`: o compilador sabe que depois dali não há nada.

Em cima disso, `print!` e `println!` com a formatação do `core::fmt` (a
mesma que o kernel usa no `kprintln!`, doc 01):

```rust
/// Destino de `print!`: cada pedaço de texto formatado vira um `escrever`.
pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        escrever(s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::Console, $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{ $crate::print!($($arg)*); $crate::print!("\n"); }};
}
```

(`macro_rules!` está no [§42](rust-basico.md#42-macro_rules-escrever-as-próprias-macros).)

E as duas peças que todo programa sem `std` precisa: um **ponto de entrada**
e um **tratador de panic**:

```rust
/// Define o ponto de entrada `_start` (o que o ELF diz para executar
/// primeiro), que chama `main` e passa o valor devolvido para `sair`.
#[macro_export]
macro_rules! principal {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _start() -> ! {
            $crate::sair($main())
        }
    };
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic no programa: {}", info);
    sair(101)
}
```

- **`_start`** é o nome que o ligador procura como entrada (o `ENTRY(_start)`
  do script, seção 2.3). `#[unsafe(no_mangle)]` mantém o nome exatamente
  assim no executável (sem ele, o Rust "decora" os nomes com o caminho do
  módulo e um hash).
- O `main` de cada programa devolve um `u64`, que vira o código de saída.
- Um panic no programa imprime a mensagem e sai com 101 (o mesmo código que
  um programa Rust usa no Linux quando dá panic).

Com isso, um programa fica assim:

```rust
//! O primeiro programa em Rust do SiliconOS: diz oi e sai com 0.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    println!("Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e {}.", sos::pid());
    0
}
```

`#![no_main]`: não há o `main` "de verdade" que a `std` chamaria; quem
começa é `_start`. (O texto não tem acento porque `escrever` troca bytes
fora do ASCII por `?`, doc 10.)

Os outros dois:

```rust
//! Conta os números primos abaixo de 10 000 e sai com a quantidade.
#![no_std]
#![no_main]

use sos::{print, println, principal};

principal!(main);

fn primo(n: u64) -> bool {
    n >= 2 && (2..).take_while(|d| d * d <= n).all(|d| n % d != 0)
}

fn main() -> u64 {
    let inicio = sos::tiques();
    let mut quantos = 0;
    print!("primeiros primos:");
    for n in 0..10_000 {
        if primo(n) {
            quantos += 1;
            if quantos <= 10 {
                print!(" {}", n);
            }
        }
    }
    println!();
    println!("{} primos abaixo de 10000 ({} tiques)", quantos, sos::tiques() - inicio);
    quantos
}
```

```rust
//! Mostra as seções .data e .bss: variáveis globais com valor inicial (que
//! vêm do arquivo) e zeradas (que o arquivo NÃO guarda; o kernel zera).
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

/// Em .data: o valor 41 está gravado no arquivo ELF.
static mut CONTADOR: u64 = 41;
/// Em .bss: 64 KiB de zeros que não ocupam nada no arquivo.
static mut TABELA: [u64; 8192] = [0; 8192];

fn main() -> u64 {
    // `&raw` e leituras/escritas explícitas: `static mut` não pode virar
    // referência comum (dois pedaços do programa poderiam mudá-lo ao mesmo tempo).
    let tabela = unsafe { &mut *&raw mut TABELA };
    let zerada = tabela.iter().all(|&x| x == 0);
    for (i, x) in tabela.iter_mut().enumerate() {
        *x = i as u64;
    }
    let soma: u64 = tabela.iter().sum();
    let contador = unsafe {
        CONTADOR += 1;
        CONTADOR
    };
    println!(".bss zerada: {}; soma depois de escrever: {}; .data: 41 -> {}",
             if zerada { "sim" } else { "NAO" }, soma, contador);
    contador
}
```

`dados` é o teste do carregador: se a `.bss` não for zerada, ou se a
`.data` não vier do arquivo, ele avisa.

### 2.3 O script de ligação: onde cada parte mora

O compilador gera código **com endereços**: um `call`, um ponteiro para uma
string, tudo depende de onde o programa vai estar na memória. Quem decide é
o **ligador** (*linker*), seguindo um **script de ligação**. O nosso,
`programas/linker.ld`:

```
/* Script de ligação dos programas de usuário (docs/13-disco-em-ram-e-elf.md).
 * Cada seção começa numa página nova, para o kernel poder dar permissões
 * diferentes a código (R-X), constantes (R--) e dados (RW-). */
ENTRY(_start)

SECTIONS
{
    . = 0x200000000000;       /* entrada 64 da PML4: área dos programas (doc 11) */

    /* Com code-model=large, o compilador põe o nosso código em .ltext,
     * .lrodata, .ldata e .lbss ("l" de large); o core da biblioteca padrão
     * vem com os nomes de sempre. A .got (tabela de endereços que o código
     * do core consulta) é preenchida na ligação, mas o ligador a marca como
     * gravável: se fosse junto com as constantes, elas virariam graváveis
     * também. Vai com os dados. */
    .text   : ALIGN(4K) { *(.text .text.* .ltext .ltext.*) }
    .rodata : ALIGN(4K) { *(.rodata .rodata.* .lrodata .lrodata.*) }
    .data   : ALIGN(4K) { *(.data .data.* .ldata .ldata.*) *(.got .got.*) }
    .bss    : ALIGN(4K) { *(.bss .bss.* .lbss .lbss.*) *(COMMON) }

    /DISCARD/ : { *(.eh_frame*) *(.comment) }
}
```

As quatro seções clássicas de um executável:

| Seção | O que guarda | Permissão | No arquivo? |
|---|---|---|---|
| `.text` | o código | ler e executar | sim |
| `.rodata` | constantes (as strings dos `println!`) | só ler | sim |
| `.data` | variáveis globais com valor inicial (`CONTADOR = 41`) | ler e escrever | sim |
| `.bss` | variáveis globais zeradas (`TABELA`) | ler e escrever | **não**: só o tamanho |

A `.bss` é uma economia antiga: 64 KiB de zeros não precisam ocupar 64 KiB
no arquivo; basta anotar "aqui vão 64 KiB de zeros", e quem carrega o
programa os cria na memória.

E as flags de compilação, em `programas/.cargo/config.toml`:

```toml
# Como compilar os programas de usuário (o build.rs do kernel usa este arquivo).
[build]
target = "x86_64-unknown-none"

[target.x86_64-unknown-none]
rustflags = [
    # Endereços fixos, escolhidos pelo nosso script de ligação (sem PIE,
    # sem relocações para o kernel aplicar).
    "-C", "link-arg=-Tlinker.ld",
    "-C", "relocation-model=static",
    # O programa mora acima de 2 GiB (0x2000_0000_0000): o modelo "small"
    # só alcança endereços absolutos de 32 bits.
    "-C", "code-model=large",
]
```

- **`relocation-model=static`:** por padrão, o alvo `x86_64-unknown-none`
  gera executáveis **relocáveis** (PIE), que podem ser carregados em
  qualquer endereço desde que o carregador corrija os ponteiros internos
  (as "relocações"). Com endereço fixo, o ELF sai do tipo `EXEC`, e o
  carregador só precisa copiar.
- **`code-model=large`:** o modelo padrão ("small") supõe que todo o
  programa mora nos primeiros 2 GiB, porque usa endereços absolutos de 32
  bits em algumas instruções. Os primeiros 512 GiB são a entrada 0 da PML4,
  que é do kernel (doc 11); o programa mora em `0x2000_0000_0000`. O
  modelo "large" usa endereços de 64 bits onde precisa.

### 2.4 Três tropeços no caminho (e o que ensinaram)

O script acima é a terceira versão. As duas primeiras deram ELFs com
problemas que o `readelf -l` (do Linux) mostrou na hora:

1. **Segmentos que dividiam uma página.** A primeira versão só listava
   `.text`, `.rodata`, `.data` e `.bss`. O resultado tinha o segmento de
   constantes começando em `0x2000_0000_0bbb`, no meio da página do código:

   ```
   LOAD  0x001000 0x0000200000000000 ... 0x000bbb 0x000bbb R E 0x1000
   LOAD  0x001bbb 0x0000200000000bbb ... 0x00051d 0x00051d R   0x1000
   ```

   Uma página tem uma permissão só; o carregador não teria como dar R-X e
   R-- para a mesma página. A causa: com `code-model=large`, o compilador
   põe o nosso código em seções `.ltext.*`, `.lrodata.*` (o "l" é de
   *large*), que o script não citava, e o ligador as colocou onde quis.
2. **O script mudou, o ELF não.** Depois de acrescentar `.ltext` e cia., a
   saída do `readelf` ficou **idêntica**. O cargo só religa quando um
   arquivo que ele conhece muda, e ele não sabia que o executável depende
   do `linker.ld`. Daí o `programas/build.rs`, de uma linha:

   ```rust
   // O cargo não sabe que os programas dependem do script de ligação: sem esta
   // linha, mudar o linker.ld não faria nada até o próximo `cargo clean`.
   fn main() {
       println!("cargo:rerun-if-changed=linker.ld");
   }
   ```

3. **Constantes graváveis.** A segunda versão juntava a `.got` com as
   constantes; o resultado tinha só dois segmentos de dados, R-X e **RW-**:
   as constantes tinham virado graváveis, porque o ligador marca a `.got`
   como gravável e o segmento herda a permissão mais aberta das seções que
   contém. Mover a `.got` para junto da `.data` resolveu.

O resultado final, para `dados`:

```
  LOAD  0x001000 0x0000200000000000 0x0000200000000000 0x000f32 0x000f32 R E 0x1000
  LOAD  0x002000 0x0000200000001000 0x0000200000001000 0x000160 0x000160 R   0x1000
  LOAD  0x003000 0x0000200000002000 0x0000200000002000 0x000020 0x000020 RW  0x1000
  LOAD  0x004000 0x0000200000003000 0x0000200000003000 0x000000 0x010000 RW  0x1000
```

A última linha é a `.bss`: **0 bytes no arquivo, 0x10000 (64 KiB) na
memória**.

## 3. O disco em RAM: um tar dentro do kernel

### 3.1 Por que "disco em RAM"

O kernel ainda não sabe ler um disco de verdade (isso exigiria um driver de
SATA, NVMe ou virtio). A saída clássica, usada por quase todo sistema no
boot (o *initrd* / *initramfs* do Linux), é um **disco em RAM**: um
arquivo que já está na memória quando o kernel começa, com os arquivos de
que ele precisa. O nosso vai dentro do próprio executável do kernel.

### 3.2 O formato tar

`tar` (*tape archive*) é de 1979, feito para gravar vários arquivos em
sequência numa fita magnética. É o formato mais simples que existe para
"vários arquivos num só", e ainda está em toda parte (`.tar.gz`). Cada
arquivo é:

```
  ┌───────────────────── 512 bytes ─────────────────────┐
  │ cabeçalho: nome, tamanho (em octal!), tipo, soma... │
  ├─────────────────────────────────────────────────────┤
  │ dados do arquivo, completados com zeros até um      │
  │ múltiplo de 512                                     │
  └─────────────────────────────────────────────────────┘
  ... o próximo arquivo ...
  [512 zeros][512 zeros]   ← fim
```

Os campos do cabeçalho (formato USTAR) que usamos:

| Bytes | Campo | Como é guardado |
|---|---|---|
| 0–99 | nome | texto, terminado por 0 |
| 100–107 | permissões | octal em texto (`0000755`) |
| 124–135 | tamanho | octal em texto (`00000047120`) |
| 148–155 | soma de verificação | octal em texto |
| 156 | tipo | `'0'` = arquivo comum |
| 257–262 | assinatura | `"ustar"` |

Números em **octal escrito como texto**: herança das máquinas da época, e
uma lembrança de que formatos antigos duram. A **soma de verificação** é a
soma de todos os 512 bytes do cabeçalho, contando o próprio campo da soma
como 8 espaços; serve para detectar um cabeçalho corrompido.

### 3.3 Montar o tar: `build.rs`

Um **script de build** (`build.rs`, na raiz do crate do kernel) é um
programa Rust que o cargo compila e roda **no computador de
desenvolvimento**, antes de compilar o crate
([§40](rust-basico.md#40-buildrs-e-include_bytes-trabalho-feito-na-compilação)).
O nosso compila os programas e monta o tar:

```rust
fn main() {
    let raiz = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let programas = raiz.join("programas");
    let saida = PathBuf::from(env::var("OUT_DIR").unwrap());
    // Um diretório de compilação separado do do kernel: o cargo de fora
    // está com o dele travado enquanto este script roda.
    let alvo_dir = raiz.join("target").join("programas");

    println!("cargo:rerun-if-changed=programas/src");
    println!("cargo:rerun-if-changed=programas/linker.ld");
    println!("cargo:rerun-if-changed=programas/Cargo.toml");
    println!("cargo:rerun-if-changed=programas/.cargo/config.toml");

    // 1. Compilar. As variáveis de ambiente que o cargo de fora define para
    //    este script (flags do kernel UEFI) não podem vazar para o de dentro.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&programas)
        .args(["build", "--release", "--target-dir"])
        .arg(&alvo_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .expect("nao consegui rodar o cargo para os programas");
    assert!(status.success(), "a compilacao dos programas de usuario falhou");

    // 2. Os programas são os arquivos de programas/src/bin, em ordem alfabética.
    let mut nomes: Vec<String> = fs::read_dir(programas.join("src/bin"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    nomes.sort();

    let binarios = alvo_dir.join("x86_64-unknown-none").join("release");
    let mut tar = Vec::new();
    for nome in &nomes {
        let dados = fs::read(binarios.join(nome)).unwrap();
        acrescentar_ao_tar(&mut tar, nome, &dados);
    }
    // O fim de um tar: dois blocos de 512 bytes zerados.
    tar.extend_from_slice(&[0u8; 1024]);

    fs::write(saida.join("ramdisk.tar"), &tar).unwrap();
}
```

- O script roda **outro cargo** dentro da pasta `programas/` (que tem o seu
  `.cargo/config.toml` com o alvo e as flags). Isso é `std`: o script de
  build roda no Linux, com tudo o que a biblioteca padrão oferece.
- `--target-dir` separado: o cargo de fora segura uma trava no `target/` do
  kernel enquanto o script roda; usando o mesmo diretório, o cargo de dentro
  ficaria esperando para sempre.
- `env_remove`: o cargo de fora passa ao script variáveis com as flags do
  kernel; se vazassem para o de dentro, ele as usaria nos programas.
- `rerun-if-changed`: sem essas linhas, o cargo rodaria o script de novo a
  cada compilação (o padrão é "qualquer arquivo do crate mudou").
- **`OUT_DIR`** é uma pasta que o cargo dá a cada script de build para os
  arquivos que ele gera.

E a montagem de cada entrada, seguindo a tabela da seção 3.2:

```rust
fn acrescentar_ao_tar(tar: &mut Vec<u8>, nome: &str, dados: &[u8]) {
    assert!(nome.len() < 100, "nome grande demais para o tar: {}", nome);
    let mut cab = [0u8; 512];
    cab[0..nome.len()].copy_from_slice(nome.as_bytes());        // nome
    octal(&mut cab[100..108], 0o755);                            // permissões
    octal(&mut cab[108..116], 0);                                // dono
    octal(&mut cab[116..124], 0);                                // grupo
    octal(&mut cab[124..136], dados.len() as u64);               // tamanho
    octal(&mut cab[136..148], 0);                                // data de modificação
    cab[156] = b'0';                                             // tipo: arquivo comum
    cab[257..263].copy_from_slice(b"ustar\0");                   // assinatura
    cab[263..265].copy_from_slice(b"00");                        // versão
    // Soma de verificação: a soma de todos os bytes do cabeçalho, contando
    // o próprio campo como 8 espaços.
    cab[148..156].copy_from_slice(b"        ");
    let soma: u32 = cab.iter().map(|&b| b as u32).sum();
    octal(&mut cab[148..155], soma as u64);
    cab[155] = b' ';

    tar.extend_from_slice(&cab);
    tar.extend_from_slice(dados);
    let resto = dados.len() % 512;
    if resto != 0 {
        tar.extend(std::iter::repeat_n(0u8, 512 - resto));
    }
}

/// Escreve `valor` em octal, com zeros à esquerda e um byte 0 no fim.
fn octal(campo: &mut [u8], valor: u64) {
    let texto = format!("{:0largura$o}\0", valor, largura = campo.len() - 1);
    campo.copy_from_slice(texto.as_bytes());
}
```

A prova de que o formato está certo é que o `tar` do Linux lê o arquivo
gerado:

```
$ tar tvf target/x86_64-unknown-uefi/debug/build/siliconos/*/out/ramdisk.tar
-rwxr-xr-x 0/0           20544 1969-12-31 21:00 dados
-rwxr-xr-x 0/0           16024 1969-12-31 21:00 oi
-rwxr-xr-x 0/0           20896 1969-12-31 21:00 primos
```

(A data é o "instante zero" do Unix, 1º de janeiro de 1970 UTC, mostrado no
fuso de Brasília: escrevemos 0 no campo de data.)

### 3.4 Ler o tar no kernel: `ramdisk.rs`

O kernel embute o arquivo com `include_bytes!`, que lê um arquivo **na
compilação** e o transforma num `&'static [u8]` dentro do executável:

```rust
/// O arquivo inteiro, embutido no executável do kernel na compilação.
static RAMDISK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ramdisk.tar"));
```

(`env!("OUT_DIR")` também é lido na compilação: é a pasta onde o
`build.rs` deixou o tar.)

Cada arquivo é devolvido como fatias do próprio `RAMDISK`, sem cópia:

```rust
/// Um arquivo do disco: nome e conteúdo (fatias do próprio tar: nada é
/// copiado). `'a` é o tempo de vida do tar; para o RAMDISK, `'static`.
#[derive(Clone, Copy)]
pub struct Arquivo<'a> {
    pub nome: &'a str,
    pub dados: &'a [u8],
}
```

Ler os campos em octal e conferir a soma:

```rust
fn ler_octal(campo: &[u8]) -> Option<u64> {
    let mut valor: u64 = 0;
    for &c in campo.iter().take_while(|&&c| c != 0 && c != b' ') {
        if !(b'0'..=b'7').contains(&c) {
            return None;
        }
        valor = valor.checked_mul(8)?.checked_add((c - b'0') as u64)?;
    }
    Some(valor)
}

fn cabecalho_valido(cab: &[u8]) -> bool {
    let soma: u64 = cab.iter().enumerate()
        .map(|(i, &b)| if (148..156).contains(&i) { b' ' as u64 } else { b as u64 })
        .sum();
    &cab[257..262] == b"ustar" && ler_octal(&cab[148..156]) == Some(soma)
}
```

`checked_mul`/`checked_add` devolvem `None` se a conta estourar: um tamanho
absurdo num cabeçalho estragado vira "arquivo inválido", não um número
errado.

E o percurso, como um **iterador**
([§41](rust-basico.md#41-implementar-iterator-e-tempos-de-vida-em-structs)):
quem usa escreve `for a in ramdisk::arquivos()` ou
`arquivos().find(...)`, e a leitura do tar acontece aos poucos, um
cabeçalho por vez:

```rust
pub struct Arquivos<'a> {
    tar: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Arquivos<'a> {
    type Item = Arquivo<'a>;

    fn next(&mut self) -> Option<Arquivo<'a>> {
        let cab = self.tar.get(self.pos..self.pos + BLOCO)?;
        if cab.iter().all(|&b| b == 0) || !cabecalho_valido(cab) {
            return None; // fim do arquivo (ou lixo: paramos por segurança)
        }
        let tamanho_nome = cab[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let nome = core::str::from_utf8(&cab[..tamanho_nome]).ok()?;
        let tamanho = ler_octal(&cab[124..136])? as usize;
        let inicio = self.pos + BLOCO;
        let dados = self.tar.get(inicio..inicio.checked_add(tamanho)?)?;
        // O próximo cabeçalho começa no primeiro múltiplo de 512 depois dos dados.
        self.pos = inicio + tamanho.div_ceil(BLOCO) * BLOCO;
        Some(Arquivo { nome, dados })
    }
}

pub fn arquivos() -> Arquivos<'static> {
    Arquivos { tar: RAMDISK, pos: 0 }
}

pub fn abrir(nome: &str) -> Option<Arquivo<'static>> {
    arquivos().find(|a| a.nome == nome)
}
```

Repare em `self.tar.get(a..b)?`: `get` com uma faixa devolve `None` em vez
de dar panic se a faixa sair do array. Um tamanho que aponta para além do
fim do tar termina a leitura em vez de derrubar o kernel.

## 4. O formato ELF

### 4.1 O que tem dentro

**ELF** (*Executable and Linkable Format*) é o formato de executável do
Linux, dos BSDs e de quase todo sistema que não é Windows nem macOS. Um ELF
tem duas "visões" do mesmo arquivo:

- a de **seções** (`.text`, `.rodata`, `.symtab`...), útil para o ligador e
  para depuradores;
- a de **segmentos** (*program headers*), útil para quem **carrega**: cada
  segmento `PT_LOAD` diz "copie estes bytes do arquivo para este endereço,
  com estas permissões".

O kernel só precisa da segunda. O caminho até ela:

```
  offset 0   ┌──────────────────────────────┐
             │ cabeçalho ELF (64 bytes)     │  identificação, tipo, máquina,
             │   e_entry  = 0x2000_0000_0068 │  ponto de entrada,
             │   e_phoff  = 64              ─┼─┐ onde está a tabela de segmentos
             │   e_phnum  = 5               │ │ e quantos são
  offset 64  ├──────────────────────────────┤ │
             │ program header 0 (56 bytes) ◄┼─┘ tipo, flags (R/W/X),
             │ program header 1             │   deslocamento no arquivo,
             │ ...                          │   endereço, tamanho no arquivo,
             ├──────────────────────────────┤   tamanho na memória
  0x1000     │ .text                        │
  0x2000     │ .rodata                      │
  0x3000     │ .data                        │
             │ .symtab, .strtab... (ignorado)│
             └──────────────────────────────┘
```

Os campos usados, com o deslocamento de cada um (todos little-endian):

| Cabeçalho | Bytes | | Program header | Bytes |
|---|---|---|---|---|
| assinatura `7F 'E' 'L' 'F'` | 0–3 | | `p_type` (1 = LOAD) | 0–3 |
| classe (2 = 64 bits) | 4 | | `p_flags` (X=1, W=2, R=4) | 4–7 |
| ordem dos bytes (1 = little-endian) | 5 | | `p_offset` | 8–15 |
| `e_type` (2 = EXEC) | 16–17 | | `p_vaddr` | 16–23 |
| `e_machine` (0x3E = x86-64) | 18–19 | | `p_filesz` | 32–39 |
| `e_entry` | 24–31 | | `p_memsz` | 40–47 |
| `e_phoff` | 32–39 | | | |
| `e_phentsize` (56) | 54–55 | | | |
| `e_phnum` | 56–57 | | | |

### 4.2 Ler: `ler_cabecalho` e `segmento`

```rust
// Leitura de números little-endian em qualquer posição do arquivo.
fn ler<const N: usize>(dados: &[u8], pos: u64) -> Result<[u8; N], ErroElf> {
    let pos = usize::try_from(pos).map_err(|_| ErroElf::Truncado)?;
    let fim = pos.checked_add(N).ok_or(ErroElf::Truncado)?;
    dados.get(pos..fim).map(|b| b.try_into().unwrap()).ok_or(ErroElf::Truncado)
}
fn u16_em(d: &[u8], p: u64) -> Result<u16, ErroElf> { Ok(u16::from_le_bytes(ler(d, p)?)) }
fn u32_em(d: &[u8], p: u64) -> Result<u32, ErroElf> { Ok(u32::from_le_bytes(ler(d, p)?)) }
fn u64_em(d: &[u8], p: u64) -> Result<u64, ErroElf> { Ok(u64::from_le_bytes(ler(d, p)?)) }
```

`ler::<N>` pega `N` bytes a partir de `pos`, ou devolve `Truncado` se o
arquivo acabar antes (`N` é um parâmetro genérico **constante**,
[§43](rust-basico.md#43-genéricos-com-constantes-const-n-usize)).
`u64::from_le_bytes` monta o número a partir de 8 bytes em ordem
little-endian. Diferente do doc 08 (ACPI), aqui não usamos `read_unaligned`
num ponteiro: tudo passa por `get`, que confere os limites. O arquivo é
dado vindo de fora, e ler fora dele seria um bug de segurança.

```rust
pub fn ler_cabecalho(dados: &[u8]) -> Result<Cabecalho, ErroElf> {
    if dados.get(0..4) != Some(b"\x7fELF") {
        return Err(ErroElf::NaoEElf);
    }
    if ler::<1>(dados, 4)?[0] != 2 {
        return Err(ErroElf::NaoSuportado("nao e de 64 bits"));
    }
    if ler::<1>(dados, 5)?[0] != 1 {
        return Err(ErroElf::NaoSuportado("nao e little-endian"));
    }
    if u16_em(dados, 16)? != 2 {
        return Err(ErroElf::NaoSuportado("nao e executavel de endereco fixo (ET_EXEC)"));
    }
    if u16_em(dados, 18)? != 0x3E {
        return Err(ErroElf::NaoSuportado("nao e para x86-64"));
    }
    if u16_em(dados, 54)? != TAM_PROGRAM_HEADER {
        return Err(ErroElf::NaoSuportado("program header de tamanho estranho"));
    }
    Ok(Cabecalho {
        entrada: u64_em(dados, 24)?,
        ph_inicio: u64_em(dados, 32)?,
        ph_quantidade: u16_em(dados, 56)?,
    })
}

pub fn segmento(dados: &[u8], cab: &Cabecalho, i: u16) -> Result<Segmento, ErroElf> {
    let base = cab.ph_inicio
        .checked_add(i as u64 * TAM_PROGRAM_HEADER as u64)
        .ok_or(ErroElf::Truncado)?;
    Ok(Segmento {
        tipo: u32_em(dados, base)?,
        flags: u32_em(dados, base + 4)?,
        deslocamento: u64_em(dados, base + 8)?,
        endereco: u64_em(dados, base + 16)?,
        tam_arquivo: u64_em(dados, base + 32)?,
        tam_memoria: u64_em(dados, base + 40)?,
    })
}
```

Os erros possíveis são um `enum` (cada um vira uma mensagem no shell e um
caso de teste):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroElf {
    /// Não começa com 0x7F 'E' 'L' 'F'.
    NaoEElf,
    /// É ELF, mas não um que sabemos rodar (32 bits, outra CPU, não executável...).
    NaoSuportado(&'static str),
    /// O arquivo acaba antes do que o cabeçalho promete.
    Truncado,
    /// Um segmento sem sentido (tamanho no arquivo maior que na memória, soma que estoura).
    SegmentoInvalido,
    /// Um segmento quer morar na área do kernel ou fora da metade baixa.
    ForaDaAreaDoPrograma,
    /// Dois segmentos querem a mesma página.
    Sobreposto,
    /// O ponto de entrada não está num segmento executável.
    EntradaInvalida,
    /// Faltaram quadros.
    SemMemoria,
}
```

### 4.3 Carregar: página por página

```rust
pub fn carregar(dados: &[u8], espaco: &mut EspacoEnderecos) -> Result<u64, ErroElf> {
    let cab = ler_cabecalho(dados)?;
    for i in 0..cab.ph_quantidade {
        let s = segmento(dados, &cab, i)?;
        if s.tipo == PT_LOAD {
            carregar_segmento(dados, &s, espaco)?;
        }
    }
    // A primeira instrução precisa estar numa página carregada e executável.
    match espaco.traduzir(cab.entrada) {
        Some(t) if t.flags & flags::SEM_EXECUCAO == 0 => Ok(cab.entrada),
        _ => Err(ErroElf::EntradaInvalida),
    }
}
```

Os outros tipos de segmento (`GNU_RELRO`, `GNU_STACK`, que o comando `elf`
mostra na seção 7) são avisos para carregadores mais sofisticados e podem
ser ignorados.

```rust
fn carregar_segmento(dados: &[u8], s: &Segmento, espaco: &mut EspacoEnderecos) -> Result<(), ErroElf> {
    // Nunca confiar no arquivo: tamanhos coerentes e nenhuma soma estourando.
    let fim_no_arquivo = s.deslocamento.checked_add(s.tam_arquivo).ok_or(ErroElf::SegmentoInvalido)?;
    let fim_na_memoria = s.endereco.checked_add(s.tam_memoria).ok_or(ErroElf::SegmentoInvalido)?;
    if s.tam_arquivo > s.tam_memoria {
        return Err(ErroElf::SegmentoInvalido);
    }
    if fim_no_arquivo > dados.len() as u64 {
        return Err(ErroElf::Truncado);
    }

    // As permissões da página vêm das flags do segmento.
    let mut bits = flags::USUARIO;
    if s.flags & PF_W != 0 {
        bits |= flags::ESCRITA;
    }
    if s.flags & PF_X == 0 {
        bits |= flags::SEM_EXECUCAO;
    }

    let mut pagina = s.endereco & !(TAMANHO_QUADRO - 1);
    while pagina < fim_na_memoria {
        if !EspacoEnderecos::do_programa(pagina) {
            return Err(ErroElf::ForaDaAreaDoPrograma);
        }
        let q = quadros::alocar().ok_or(ErroElf::SemMemoria)?;
        let destino = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
        unsafe { core::ptr::write_bytes(destino, 0, TAMANHO_QUADRO as usize) };

        // A parte de [endereco, endereco + tam_arquivo) que cai nesta página
        // vem do arquivo; o resto da página fica zerado (é aí que mora a .bss).
        let ini = pagina.max(s.endereco);
        let fim = (pagina + TAMANHO_QUADRO).min(s.endereco + s.tam_arquivo);
        if ini < fim {
            let origem = (s.deslocamento + (ini - s.endereco)) as usize;
            let n = (fim - ini) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(dados[origem..origem + n].as_ptr(),
                                               destino.add((ini - pagina) as usize), n);
            }
        }

        espaco.mapear(pagina, q, bits).map_err(|e| match e {
            ErroMapeamento::JaMapeado => ErroElf::Sobreposto,
            ErroMapeamento::SemMemoria => ErroElf::SemMemoria,
            _ => ErroElf::ForaDaAreaDoPrograma,
        })?;
        pagina += TAMANHO_QUADRO;
    }
    Ok(())
}
```

Para cada página que o segmento toca, na memória:

```
  segmento: endereco = E, tam_arquivo = A, tam_memoria = M (M ≥ A)

  memória:  E ──────────── E+A ──────────────── E+M
            │  do arquivo   │   zeros (.bss)     │
            └─ página 1 ─┘└─ página 2 ─┘└─ página 3 ─┘
                 copia tudo   copia o começo,    só zeros
                              o resto fica zero
```

1. Um quadro novo, **zerado**. Zerar é obrigatório por dois motivos: a
   `.bss` e o fim de página depois dos dados têm que ser zero, e o quadro
   pode ter restos de outro processo (doc 10).
2. A interseção entre a página `[pagina, pagina + 4096)` e a parte do
   segmento que vem do arquivo `[E, E + A)` é copiada para o lugar certo
   dentro do quadro. Se não houver interseção (uma página só de `.bss`),
   nada é copiado.
3. O quadro é mapeado com as permissões do segmento: USUARIO sempre,
   ESCRITA se tiver `W`, SEM_EXECUCAO se **não** tiver `X`. É aqui que o
   trabalho do script de ligação vira proteção de verdade: o código não
   pode ser alterado, e os dados não podem ser executados.

**Por que desconfiar de tudo.** Um carregador é a porta de entrada de dados
vindos de fora (um arquivo que qualquer um pode ter escrito). Cada número
lido pode estar errado por acidente ou de propósito, e cada um é
conferido: somas com `checked_add` (um `endereco` perto de `u64::MAX` mais um
tamanho daria a volta e passaria num `<`), `tam_arquivo ≤ tam_memoria`, o
segmento dentro do arquivo, cada página na área do programa
(`do_programa`, doc 11: nada de mapear por cima do kernel), nenhuma página
duas vezes (`JaMapeado` vira `Sobreposto`), e a entrada num segmento
executável.

**E se der erro no meio?** Algumas páginas já estão mapeadas no espaço. Não
é preciso desfazer nada à mão: quem chamou é dono do espaço, e quando ele
for destruído (o `?` sai da função e o `Drop` roda, doc 11), as páginas
voltam ao alocador.

## 5. Rodar um ELF: `iniciar_elf`

```rust
/// Topo da pilha dos programas ELF (doc 13): fim da entrada 255 da PML4, o
/// último endereço da metade baixa, longe de onde os segmentos moram.
pub const PILHA_ELF_TOPO: u64 = 0x0000_8000_0000_0000;
/// Quantas páginas de pilha um programa ELF ganha (16 KiB).
pub const PILHA_ELF_PAGINAS: u64 = 4;

pub fn iniciar_elf(nome: &'static str, dados: &[u8], segundo_plano: bool) -> Result<Pid, elf::ErroElf> {
    let mut espaco = EspacoEnderecos::novo().ok_or(elf::ErroElf::SemMemoria)?;
    // Se der erro daqui em diante, `espaco` é destruído na saída (`?`) e
    // devolve tudo o que já tinha sido mapeado.
    let entrada = elf::carregar(dados, &mut espaco)?;
    for i in 1..=PILHA_ELF_PAGINAS {
        let q = quadros::alocar().ok_or(elf::ErroElf::SemMemoria)?;
        unsafe { core::ptr::write_bytes(paginacao::fisico_para_virtual(q.endereco()) as *mut u8, 0, 4096) };
        espaco.mapear(PILHA_ELF_TOPO - i * TAMANHO_QUADRO, q,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO)
            .map_err(|_| elf::ErroElf::SemMemoria)?;
    }
    // RSP = topo - 8: o compilador gera `_start` como uma função comum, que
    // espera ter sido chamada com `call` (que empilha 8 bytes de endereço de
    // retorno numa pilha alinhada em 16). Com a pilha desalinhada, uma
    // instrução que exige alinhamento (como `movaps`) daria #GP. Os três
    // programas de hoje rodaram mesmo com RSP = topo (testado): só não usam
    // nenhuma dessas. O erro ficaria escondido até o primeiro que usasse.
    Ok(escalonador::criar(nome, espaco, entrada, PILHA_ELF_TOPO - 8, segundo_plano))
}
```

- **A pilha mora no fim da metade baixa** (entrada 255 da PML4), longe do
  código em `0x2000_0000_0000`: o programa pode ter quantos segmentos
  quiser sem esbarrar nela. `0x0000_8000_0000_0000` em si não é um endereço
  canônico (doc 04), mas nunca é usado como endereço: só como "um depois do
  fim". As páginas vão de `0x7FFF_FFFF_C000` a `0x7FFF_FFFF_FFFF`.
- **O alinhamento da pilha** é uma regra da convenção de chamada (doc 10,
  [§37](rust-basico.md#37-convenções-de-chamada-extern-sysv64)): no início
  de qualquer função, `RSP + 8` é múltiplo de 16. O teste com `RSP = topo`
  está na seção 8.3: os programas rodaram, o que mostra que a regra não é
  testada pela CPU a cada instrução, só pelas que dependem dela.
- O resto é o doc 12: `escalonador::criar` monta a pilha do kernel e o
  processo entra na fila.

### 5.1 Validar ponteiros pelas tabelas de página

No doc 10, `faixa_do_programa` conferia se um ponteiro vindo do programa
estava numa das duas páginas fixas (código e pilha). Um ELF tem quantas
páginas quiser, em endereços que o arquivo escolhe. A pergunta certa é
outra: **"essa memória é do programa?"**, e quem sabe responder são as
tabelas de página do espaço ativo (o do programa que fez a chamada):

```rust
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    let mut pagina = inicio & !(TAMANHO_QUADRO - 1);
    while pagina < fim {
        let do_programa = EspacoEnderecos::do_programa(pagina)
            && paginacao::traduzir(pagina).is_some_and(|t| t.flags & flags::USUARIO != 0);
        if !do_programa {
            return false;
        }
        pagina += TAMANHO_QUADRO;
    }
    true
}
```

Cada página da faixa precisa estar mapeada, com o bit USUARIO, numa entrada
da PML4 que é do programa. Vale para os programas embutidos e para os ELF,
e para qualquer leiaute futuro.

## 6. O shell: `ls`, `elf` e `rodar`

```rust
fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario;
    let Some(&nome) = argumentos.first() else {
        kprintln!("uso: rodar <programa> [&]   (veja 'programas' e 'ls')");
        return;
    };
    let segundo_plano = argumentos.get(1) == Some(&"&");
    // Primeiro os programas embutidos (doc 10), depois os arquivos do disco
    // em RAM (doc 13).
    let pid = if let Some(programa) = usuario::programas::procurar(nome) {
        usuario::iniciar(programa, segundo_plano)
    } else if let Some(arquivo) = crate::ramdisk::abrir(nome) {
        match usuario::iniciar_elf(arquivo.nome, arquivo.dados, segundo_plano) {
            Ok(pid) => pid,
            Err(erro) => {
                kprintln!("'{}' nao pode ser carregado: {:?}", nome, erro);
                return;
            }
        }
    } else {
        kprintln!("'{}' nao existe (veja 'programas' e 'ls')", nome);
        return;
    };
    if segundo_plano {
        // Segundo plano (doc 12): o shell não espera. Quando o programa
        // terminar, o escalonador imprime o resultado.
        kprintln!("[{}] '{}' em segundo plano", pid, nome);
    } else {
        let r = escalonador::esperar(pid);
        kprintln!("'{}' {}", nome, usuario::descrever(r));
    }
}
```

`ls` lista o disco, e `elf <arquivo>` mostra o cabeçalho e os segmentos,
como o `readelf -l` do Linux (o código está em [Mudanças nos
arquivos](#mudanças-nos-arquivos)). `arquivo.nome` é `&'static str` porque
aponta para dentro do `RAMDISK`, que vive para sempre; por isso pode virar o
nome do processo, que o escalonador guarda.

## 7. Como foi testado

### 7.1 Pelo shell

Saída real (serial), QEMU com 128 MiB:

```
silicon> mem
quadros: 20272 livres de 32768 (79 MiB livres)
heap: 224 bytes em uso (4 alocacoes vivas), pico 0 KiB
      mapeado 1024 KiB, 1 blocos livres, maior 1023 KiB
silicon> ls
  dados          20544 bytes  executavel ELF
  oi             16024 bytes  executavel ELF
  primos         20896 bytes  executavel ELF
3 arquivos; o disco em RAM (tar) ocupa 60928 bytes
silicon> elf oi
entrada: 0x200000000068; 5 segmentos
  TIPO   ARQUIVO   ENDERECO            NO ARQUIVO  NA MEMORIA  PERM
  LOAD   0x001000  0x0000200000000000        3013        3013  R-X
  LOAD   0x002000  0x0000200000001000         344         344  R--
  LOAD   0x003000  0x0000200000002000          16          16  RW-
  RELRO  0x003000  0x0000200000002000          16          16  R--
  STACK  0x000000  0x0000000000000000           0           0  RW-
silicon> elf dados
entrada: 0x200000000130; 6 segmentos
  TIPO   ARQUIVO   ENDERECO            NO ARQUIVO  NA MEMORIA  PERM
  LOAD   0x001000  0x0000200000000000        3890        3890  R-X
  LOAD   0x002000  0x0000200000001000         352         352  R--
  LOAD   0x003000  0x0000200000002000          32          32  RW-
  LOAD   0x004000  0x0000200000003000           0       65536  RW-
  RELRO  0x003000  0x0000200000002000          32          32  R--
  STACK  0x000000  0x0000000000000000           0           0  RW-
silicon> rodar oi
Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 1.
'oi' terminou: codigo de saida 0
silicon> rodar primos
primeiros primos: 2 3 5 7 11 13 17 19 23 29
1229 primos abaixo de 10000 (2 tiques)
'primos' terminou: codigo de saida 1229
silicon> rodar dados
.bss zerada: sim; soma depois de escrever: 33550336; .data: 41 -> 42
'dados' terminou: codigo de saida 42
silicon> rodar dados
.bss zerada: sim; soma depois de escrever: 33550336; .data: 41 -> 42
'dados' terminou: codigo de saida 42
silicon> rodar primos &
[5] 'primos' em segundo plano
silicon> primeiros primos: 2 3 5 7 11 13 17 19 23 29
1229 primos abaixo de 10000 (2 tiques)

[5] 'primos' terminou: codigo de saida 1229
rodar oi &
[6] 'oi' em segundo plano
silicon> Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 6.

[6] 'oi' terminou: codigo de saida 0
rodar nada
'nada' nao existe (veja 'programas' e 'ls')
silicon> mem
quadros: 20272 livres de 32768 (79 MiB livres)
heap: 224 bytes em uso (4 alocacoes vivas), pico 16 KiB
      mapeado 1024 KiB, 1 blocos livres, maior 1023 KiB
```

Lendo:

- **Os três ELF rodam**, com `println!` formatando números.
- **`dados` duas vezes, 41 → 42 nas duas.** Cada execução ganha uma cópia
  nova da `.data` vinda do arquivo; o que o primeiro processo escreveu
  morreu com o espaço dele. A soma `33550336` é 0 + 1 + ... + 8191 =
  8191 × 8192 / 2: a `.bss` estava zerada e aceitou escrita.
- **A `.bss` de `dados`** aparece no `elf` como 0 bytes no arquivo e 65536
  na memória.
- **Nada vaza:** 20272 quadros livres antes e depois. (O número no boot é
  menor que os 20298 do doc 12: o executável do kernel cresceu uns 100 KiB,
  60 KiB deles do tar.)

### 7.2 `cargo test`

46 testes, todos `[ok]`: os 40 de antes e 6 novos (3 em `ramdisk.rs`, 3 em
`elf.rs`). Os do tar:

```rust
#[test_case]
fn os_tres_programas_estao_no_disco() {
    let nomes: alloc::vec::Vec<&str> = arquivos().map(|a| a.nome).collect();
    assert_eq!(nomes, ["dados", "oi", "primos"]);
    assert!(arquivos().all(|a| a.dados.starts_with(b"\x7fELF")));
    assert!(abrir("nao-existe").is_none());
}

#[test_case]
fn octal_do_tar() {
    assert_eq!(ler_octal(b"00000017331\0"), Some(0o17331));
    assert_eq!(ler_octal(b"0000644 "), Some(0o644));
    assert_eq!(ler_octal(b"0009"), None);
}

#[test_case]
fn cabecalho_estragado_para_a_leitura() {
    // Uma cópia do tar com um byte do primeiro nome trocado: a soma de
    // verificação não bate mais, e a leitura para logo no começo.
    let mut copia = alloc::vec::Vec::from(RAMDISK);
    assert_eq!(Arquivos { tar: &copia, pos: 0 }.count(), 3);
    copia[0] ^= 0x20;
    assert_eq!(Arquivos { tar: &copia, pos: 0 }.count(), 0);
}
```

Os do ELF: um lê o cabeçalho de `oi`, um roda os três programas e confere
os códigos de saída (0, 1229 e 42), e o mais importante estraga cópias de
`oi` de oito jeitos e confere que **cada uma é recusada com o erro certo e
sem vazar um quadro**:

```rust
#[test_case]
fn arquivos_ruins_recusados_sem_vazar() {
    let livres = quadros::livres();
    let ph = |d: &Vec<u8>| u64::from_le_bytes(d[32..40].try_into().unwrap()) as usize;
    assert_eq!(carregar_alterado(|d| d[0] = 0), Err(ErroElf::NaoEElf));
    assert_eq!(carregar_alterado(|d| d[4] = 1), Err(ErroElf::NaoSuportado("nao e de 64 bits")));
    assert_eq!(carregar_alterado(|d| d[18] = 0x28), Err(ErroElf::NaoSuportado("nao e para x86-64")));
    assert_eq!(carregar_alterado(|d| d.truncate(0x1800)), Err(ErroElf::Truncado));
    // O primeiro segmento pedindo para morar no heap do kernel:
    assert_eq!(carregar_alterado(|d| { let p = ph(d) + 16; d[p..p + 8].copy_from_slice(&0xFFFF_9000_0000_0000u64.to_le_bytes()) }),
               Err(ErroElf::ForaDaAreaDoPrograma));
    // tam_arquivo maior que tam_memoria:
    assert_eq!(carregar_alterado(|d| { let p = ph(d) + 40; d[p..p + 8].copy_from_slice(&1u64.to_le_bytes()) }),
               Err(ErroElf::SegmentoInvalido));
    // O segundo segmento no mesmo endereço do primeiro:
    assert_eq!(carregar_alterado(|d| { let p = ph(d) + 56 + 16; d[p..p + 8].copy_from_slice(&0x0000_2000_0000_0000u64.to_le_bytes()) }),
               Err(ErroElf::Sobreposto));
    // Ponto de entrada fora do código:
    assert_eq!(carregar_alterado(|d| d[24..32].copy_from_slice(&0x0000_2000_0000_1000u64.to_le_bytes())),
               Err(ErroElf::EntradaInvalida));
    assert_eq!(quadros::livres(), livres);
}
```

(`0x28` é o número de máquina do ARM de 32 bits; `0x1800` corta o arquivo
no meio do código; `0x2000_0000_1000` é a página das constantes, que não é
executável.)

O teste `validacao_de_ponteiros` do doc 10 mudou junto com
`faixa_do_programa`: agora monta um espaço com as duas páginas, ativa-o,
pergunta, e volta ao CR3 do kernel antes dos `assert` (como o teste do doc
11). E ganhou um caso: uma faixa que começa na última linha da página de
código e passa para a página seguinte, que não existe, é recusada.

### 7.3 E com a pilha desalinhada?

Para conferir o comentário de `iniciar_elf`, o `PILHA_ELF_TOPO - 8` virou
`PILHA_ELF_TOPO` (e depois voltou). Os três programas **rodaram normalmente**:

```
silicon> rodar oi
Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 1.
'oi' terminou: codigo de saida 0
silicon> rodar primos
primeiros primos: 2 3 5 7 11 13 17 19 23 29
1229 primos abaixo de 10000 (1 tiques)
'primos' terminou: codigo de saida 1229
silicon> rodar dados
.bss zerada: sim; soma depois de escrever: 33550336; .data: 41 -> 42
'dados' terminou: codigo de saida 42
```

A primeira versão do comentário dizia que, sem o `- 8`, as instruções que
exigem alinhamento "dariam #GP". O teste mostrou que isso só vale para
código que **usa** essas instruções, e nenhum dos três usa. O comentário foi
corrigido. É o tipo de erro que não aparece hoje e aparece meses depois: o
`- 8` fica.

### 7.4 Regressão

As 21 builds com features de teste continuam passando.

## O que ainda falta

- **Um sistema de arquivos de verdade.** O disco em RAM só tem uma lista
  plana de nomes, só leitura, embutida no kernel. Faltam diretórios,
  escrita, um driver de disco e a camada que o livro chama de **VFS**
  (*virtual file system*): uma interface única (`abrir`, `ler`, `fechar`)
  que funcione igual para o tar, para um disco e para dispositivos.
- **Chamadas de sistema de arquivos.** Os programas não podem abrir
  arquivos; só o kernel lê o tar.
- **Argumentos.** Um programa não recebe argumentos (`argv`) nem variáveis
  de ambiente; num sistema Unix, o carregador os põe no topo da pilha antes
  de pular para `_start`.
- **ELF relocável (PIE) e bibliotecas dinâmicas.** Só aceitamos `ET_EXEC`
  com endereço fixo.
- **`int 0x80` em todo `println!`.** Cada pedaço formatado é uma chamada de
  sistema; o próximo capítulo troca o mecanismo por `syscall`/`sysret`,
  feito para isso. *(Feito no [doc 14](14-syscall-sysret.md).)*

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 12 (tag `doc-12`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-13`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-12...doc-13).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`programas/Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/Cargo.toml) | **novo** (seção 2.1) | — |
| [`programas/.cargo/config.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/.cargo/config.toml) | **novo** (seção 2.3) | — |
| [`programas/linker.ld`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/linker.ld) | **novo** (seção 2.3) | — |
| [`programas/build.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/build.rs) | **novo** (seção 2.4) | — |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | **novo**: a biblioteca `sos` (seção 2.2) | — |
| [`programas/src/bin/`](https://github.com/moises-dev-git/siliconOS/tree/doc-13/programas/src/bin) | **novos**: `oi.rs`, `primos.rs`, `dados.rs` (seção 2.2) | — |
| [`programas/Cargo.lock`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/Cargo.lock) | **novo**, gerado pelo cargo | — |
| [`build.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | **novo**: compila os programas e monta o tar (seção 3.3) | raiz do projeto |
| [`.gitignore`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/.gitignore) | `/programas/target/` | depois de `/target/` |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | **novo**: `RAMDISK`, `Arquivo`, `ler_octal`, `cabecalho_valido`, `Arquivos` (iterador), `arquivos`, `abrir`, `tamanho_total`, 3 testes | — |
| [`src/usuario/elf.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | **novo**: `ErroElf`, `Cabecalho`, `Segmento`, constantes, `ler`/`u16_em`/`u32_em`/`u64_em`, `ler_cabecalho`, `segmento`, `carregar`, `carregar_segmento`, `permissoes`, 3 testes | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/main.rs) | `mod ramdisk;` | depois de `mod escalonador;` |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/mod.rs) | `pub mod elf;`; `PILHA_ELF_TOPO`, `PILHA_ELF_PAGINAS`, `iniciar_elf`; `rodar` com `cfg_attr` (só os testes a usam); `faixa_do_programa` pelas tabelas; teste `validacao_de_ponteiros` reescrito | `iniciar_elf` antes de `rodar` |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/shell.rs) | `rodar` aceita arquivos do disco; comandos `ls` e `elf`; descrição de `rodar` | — |

### Os trechos que o texto não mostrou

O cabeçalho de `build.rs` (os `use`):

```rust
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
```

Em `ramdisk.rs`, a constante e o tamanho total:

```rust
const BLOCO: usize = 512;

pub fn tamanho_total() -> usize {
    RAMDISK.len()
}
```

Em `elf.rs`, os `use`, as estruturas e as constantes:

```rust
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao::{self, flags, ErroMapeamento};
use crate::memoria::{quadros, TAMANHO_QUADRO};

/// O que nos interessa do cabeçalho ELF (os primeiros 64 bytes).
#[derive(Debug, Clone, Copy)]
pub struct Cabecalho {
    /// Endereço da primeira instrução (`e_entry`).
    pub entrada: u64,
    /// Onde começa a tabela de segmentos no arquivo (`e_phoff`).
    pub ph_inicio: u64,
    /// Quantos segmentos (`e_phnum`).
    pub ph_quantidade: u16,
}

/// Uma entrada da tabela de segmentos ("program header", 56 bytes).
#[derive(Debug, Clone, Copy)]
pub struct Segmento {
    pub tipo: u32,
    pub flags: u32,
    pub deslocamento: u64,
    pub endereco: u64,
    pub tam_arquivo: u64,
    pub tam_memoria: u64,
}

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;
const TAM_PROGRAM_HEADER: u16 = 56;

/// Texto curto com as permissões de um segmento: "R-X", "RW-"...
pub fn permissoes(flags: u32) -> [u8; 3] {
    [
        if flags & PF_R != 0 { b'R' } else { b'-' },
        if flags & PF_W != 0 { b'W' } else { b'-' },
        if flags & PF_X != 0 { b'X' } else { b'-' },
    ]
}
```

O começo dos testes de `elf.rs`:

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::escalonador;
    use crate::ramdisk;
    use crate::usuario::Resultado;
    use alloc::vec::Vec;

    fn rodar(nome: &'static str) -> Resultado {
        let pid = crate::usuario::iniciar_elf(nome, ramdisk::abrir(nome).unwrap().dados, false).unwrap();
        escalonador::esperar(pid)
    }

    /// Tenta carregar uma cópia de `oi` alterada por `mexer`, sem rodar.
    fn carregar_alterado(mexer: impl FnOnce(&mut Vec<u8>)) -> Result<u64, ErroElf> {
        let mut copia = Vec::from(ramdisk::abrir("oi").unwrap().dados);
        mexer(&mut copia);
        let mut espaco = EspacoEnderecos::novo().unwrap();
        carregar(&copia, &mut espaco)
    }

    #[test_case]
    fn cabecalho_do_oi() {
        let dados = ramdisk::abrir("oi").unwrap().dados;
        let cab = ler_cabecalho(dados).unwrap();
        let s = segmento(dados, &cab, 0).unwrap();
        assert_eq!((s.tipo, s.endereco), (PT_LOAD, 0x0000_2000_0000_0000));
        assert_eq!(&permissoes(s.flags), b"R-X");
        assert!(cab.entrada >= s.endereco && cab.entrada < s.endereco + s.tam_memoria);
    }

    #[test_case]
    fn programas_em_rust_rodam() {
        assert_eq!(rodar("oi"), Resultado::Saiu(0));
        assert_eq!(rodar("primos"), Resultado::Saiu(1229));
        assert_eq!(rodar("dados"), Resultado::Saiu(42));
    }

    // arquivos_ruins_recusados_sem_vazar: seção 7.2
}
```

Em `usuario/mod.rs`, `rodar` (agora só os testes a usam) e o teste novo:

```rust
/// Roda o programa e espera ele terminar. Desde o doc 13, o shell faz o
/// mesmo à mão (para também aceitar arquivos ELF); quem usa esta são os testes.
#[cfg_attr(not(test), allow(dead_code))]
pub fn rodar(programa: &programas::Programa) -> Resultado {
    let pid = iniciar(programa, false);
    escalonador::esperar(pid)
}
```

```rust
    #[test_case]
    fn validacao_de_ponteiros() {
        // As perguntas são feitas às tabelas do espaço ATIVO: montar um
        // espaço com as duas páginas de sempre e ativá-lo durante o teste.
        let mut espaco = EspacoEnderecos::novo().unwrap();
        espaco.mapear(CODIGO_VIRT, nova_pagina_zerada(), flags::USUARIO).unwrap();
        espaco.mapear(PILHA_VIRT, nova_pagina_zerada(), flags::USUARIO | flags::ESCRITA).unwrap();
        espaco.ativar();
        let respostas = [
            faixa_do_programa(CODIGO_VIRT, 10),
            faixa_do_programa(PILHA_VIRT + 4000, 96),
            faixa_do_programa(PILHA_VIRT + 4000, 97),          // passa do fim
            faixa_do_programa(CODIGO_VIRT + 4090, 10),         // a página seguinte não existe
            faixa_do_programa(0xFFFF_9000_0000_0000, 16),      // kernel
            faixa_do_programa(u64::MAX - 4, 10),               // estoura
        ];
        paginacao::ativar(paginacao::raiz_kernel());
        assert_eq!(respostas, [true, true, false, false, false, false]);
    }
```

Em `shell.rs`, as linhas da tabela de comandos e os comandos `ls` e `elf`:

```rust
    Comando { nome: "rodar", uso: "rodar <programa> [&]", descricao: "roda um programa (embutido ou do disco) no anel 3; & = segundo plano", funcao: rodar_programa },
    Comando { nome: "ps", uso: "ps", descricao: "lista os processos", funcao: ps },
    Comando { nome: "ls", uso: "ls", descricao: "lista os arquivos do disco em RAM", funcao: ls },
    Comando { nome: "elf", uso: "elf <arquivo>", descricao: "mostra o cabecalho e os segmentos de um ELF", funcao: elf },
```

```rust
/// Comando `ls` (doc 13): os arquivos do disco em RAM.
fn ls(_: &[&str]) {
    let mut total = 0;
    for a in crate::ramdisk::arquivos() {
        let tipo = if a.dados.starts_with(b"\x7fELF") { "executavel ELF" } else { "" };
        kprintln!("  {:<12}{:>8} bytes  {}", a.nome, a.dados.len(), tipo);
        total += 1;
    }
    kprintln!("{} arquivos; o disco em RAM (tar) ocupa {} bytes", total, crate::ramdisk::tamanho_total());
}

/// Comando `elf <arquivo>` (doc 13): o cabeçalho e os segmentos, como o
/// `readelf -l` do Linux.
fn elf(argumentos: &[&str]) {
    use crate::usuario::elf;
    let Some(arquivo) = argumentos.first().and_then(|n| crate::ramdisk::abrir(n)) else {
        kprintln!("uso: elf <arquivo>   (veja 'ls')");
        return;
    };
    let cab = match elf::ler_cabecalho(arquivo.dados) {
        Ok(c) => c,
        Err(e) => { kprintln!("nao e um ELF que o SiliconOS roda: {:?}", e); return; }
    };
    kprintln!("entrada: {:#x}; {} segmentos", cab.entrada, cab.ph_quantidade);
    kprintln!("  TIPO   ARQUIVO   ENDERECO            NO ARQUIVO  NA MEMORIA  PERM");
    for i in 0..cab.ph_quantidade {
        let Ok(s) = elf::segmento(arquivo.dados, &cab, i) else { break };
        let tipo = match s.tipo { elf::PT_LOAD => "LOAD", 0x6474_e551 => "STACK", 0x6474_e552 => "RELRO", _ => "outro" };
        let p = elf::permissoes(s.flags);
        kprintln!("  {:<6} {:#08x}  {:#018x}  {:>10}  {:>10}  {}", tipo, s.deslocamento, s.endereco,
                  s.tam_arquivo, s.tam_memoria, core::str::from_utf8(&p).unwrap());
    }
}
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `sair` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Chamada de sistema 0: termina o programa com o código dado. |
| `escrever` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Chamada de sistema 1: escreve bytes no console e devolve quantos (ou um erro negativo). |
| `tiques` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Chamada de sistema 2: tiques do timer desde o boot. |
| `pid` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Chamada de sistema 3: o número do processo. |
| `Console::write_str` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | `fmt::Write`: cada pedaço de texto formatado vira um `escrever` (base de `print!`/`println!`). |
| `panic` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Tratador de panic dos programas: imprime a mensagem e sai com 101. |
| `_start` (macro `principal!`) | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/lib.rs) | Ponto de entrada gerado pela macro: chama o `main` do programa e passa o resultado a `sair`. |
| `main` (oi) | [programas/src/bin/oi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/oi.rs) | Diz oi com o próprio pid e devolve 0. |
| `primo` | [programas/src/bin/primos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/primos.rs) | `n` é primo? (divisões até a raiz quadrada) |
| `main` (primos) | [programas/src/bin/primos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/primos.rs) | Conta os primos abaixo de 10 000, mostra os 10 primeiros e devolve a quantidade (1229). |
| `main` (dados) | [programas/src/bin/dados.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/src/bin/dados.rs) | Confere que a `.bss` veio zerada, escreve nela, soma 1 ao `CONTADOR` da `.data` e devolve o valor (42). |
| `main` (programas/build.rs) | [programas/build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/programas/build.rs) | Avisa o cargo para religar os programas quando o `linker.ld` mudar. |
| `main` (build.rs) | [build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | Compila os programas com outro cargo e empacota os ELF em `OUT_DIR/ramdisk.tar`. |
| `acrescentar_ao_tar` | [build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | Escreve um cabeçalho USTAR (com soma de verificação) e os dados, completados até múltiplo de 512. |
| `octal` | [build.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/build.rs) | Escreve um número em octal, com zeros à esquerda e um 0 no fim, num campo do cabeçalho. |
| `ler_octal` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Lê um número octal em texto, recusando dígitos inválidos e estouros. |
| `cabecalho_valido` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Confere a assinatura "ustar" e a soma de verificação de um cabeçalho. |
| `Arquivos::next` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | O iterador: lê um cabeçalho, devolve nome e dados (fatias do tar) e pula para o próximo múltiplo de 512. |
| `arquivos` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Um iterador sobre todos os arquivos do disco em RAM. |
| `abrir` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | O arquivo com esse nome, se existir. |
| `tamanho_total` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Tamanho do tar inteiro, em bytes. |
| `ler` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | `N` bytes do arquivo a partir de uma posição, ou `Truncado`. |
| `u16_em` / `u32_em` / `u64_em` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Um número little-endian numa posição do arquivo. |
| `ler_cabecalho` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Confere assinatura, classe, ordem dos bytes, tipo, máquina e tamanho dos program headers; devolve entrada e tabela. |
| `segmento` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Lê o program header número `i`. |
| `carregar` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Carrega todos os segmentos LOAD num espaço e confere que a entrada é executável. |
| `carregar_segmento` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Valida o segmento e, página por página, aloca um quadro zerado, copia a parte do arquivo e mapeia com as permissões do segmento. |
| `permissoes` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | As flags de um segmento como "R-X", "RW-"... |
| `iniciar_elf` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/mod.rs) | Cria o espaço, carrega o ELF, mapeia 4 páginas de pilha no fim da metade baixa e entrega ao escalonador com RSP = topo − 8. |
| `ls` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/shell.rs) | Comando `ls`: os arquivos do disco em RAM. |
| `elf` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/shell.rs) | Comando `elf <arquivo>`: cabeçalho e segmentos, como o `readelf -l`. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `os_tres_programas_estao_no_disco` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Teste: o tar tem dados, oi e primos, todos ELF, e um nome inexistente dá `None`. |
| `octal_do_tar` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Teste: números octais com 0 e com espaço no fim, e um dígito 9 recusado. |
| `cabecalho_estragado_para_a_leitura` | [ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/ramdisk.rs) | Teste: trocar um byte do primeiro cabeçalho quebra a soma, e a leitura não devolve nenhum arquivo. |
| `rodar` (dos testes) | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Ajudante: carrega um ELF do disco, roda e espera o resultado. |
| `carregar_alterado` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Ajudante: carrega uma cópia de `oi` modificada por uma closure. |
| `cabecalho_do_oi` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Teste: o primeiro segmento de `oi` é LOAD, R-X, em `0x2000_0000_0000`, e contém a entrada. |
| `programas_em_rust_rodam` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Teste: oi, primos e dados saem com 0, 1229 e 42. |
| `arquivos_ruins_recusados_sem_vazar` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-13/src/usuario/elf.rs) | Teste: oito estragos diferentes, cada um recusado com o erro certo, sem perder quadros. |

---

## Diferença em relação ao livro

O roteiro da seção 11.4 do livro começa justamente por aqui: disco em RAM,
sistema de arquivos virtual e carregar um programa. Escolhas nossas: o
formato tar (o livro não fixa um), o disco embutido no executável do kernel
por um `build.rs` (em vez de carregado pelo firmware), programas de usuário
em Rust com uma biblioteca própria, o leiaute fixado por script de ligação,
`ET_EXEC` sem relocações, e nenhum VFS ainda.

## Resumo

- Programas de usuário em **Rust**: crate à parte para
  `x86_64-unknown-none`, `no_std`/`no_main`, e uma biblioteca (`sos`) que
  faz o papel da libc: cada função é uma chamada de sistema.
- O **script de ligação** fixa os endereços (`0x2000_0000_0000`) e põe cada
  seção numa página, para cada uma ter a sua permissão. `relocation-model=
  static` dá um ELF `EXEC`; `code-model=large` permite morar acima de 2 GiB.
- O **disco em RAM** é um arquivo **tar** (cabeçalhos de 512 bytes,
  números em octal, soma de verificação), montado pelo `build.rs` e embutido
  com `include_bytes!`.
- Um **ELF** é carregado pelos **segmentos PT_LOAD**: página por página, um
  quadro zerado, a parte que vem do arquivo copiada, o resto zero (a `.bss`),
  e as permissões R/W/X viram ESCRITA e SEM_EXECUCAO.
- O carregador **desconfia de cada número**: somas conferidas, segmento
  dentro do arquivo e na área do programa, sem sobreposição, entrada
  executável. Em caso de erro, o `Drop` do espaço devolve tudo.
- A validação de ponteiros das chamadas de sistema passou a perguntar às
  **tabelas de página** do espaço ativo.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que os programas precisam de um crate separado do kernel?**
   Porque são compilados para outro alvo (`x86_64-unknown-none`, gerando
   ELF) com outras flags e outro script de ligação. O kernel é um
   executável UEFI (PE).

2. **O que a `.bss` tem de diferente das outras seções?**
   Ela não ocupa espaço no arquivo: o ELF só diz o tamanho (`tam_memoria`
   maior que `tam_arquivo`). Quem carrega cria os zeros na memória.

3. **Por que o carregador zera o quadro antes de copiar, se vai copiar
   por cima?**
   Porque nem toda página é coberta pelo arquivo: a `.bss` e o fim da
   última página de cada segmento têm que ser zero. E um quadro
   reaproveitado tem restos de quem o usou antes.

4. **O que aconteceria se o script de ligação não pusesse cada seção numa
   página nova?**
   Dois segmentos com permissões diferentes dividiriam uma página, e uma
   página só tem uma permissão. Foi o primeiro tropeço da seção 2.4.

5. **Por que `carregar_segmento` usa `checked_add` em vez de `+`?**
   Porque os números vêm de um arquivo que pode estar estragado ou ter
   sido feito para enganar. Um endereço perto de `u64::MAX` mais um
   tamanho daria a volta e passaria numa comparação com `<`.

6. **Por que `faixa_do_programa` deixou de comparar com `CODIGO_VIRT` e
   `PILHA_VIRT`?**
   Porque um ELF pode ter quantas páginas quiser, onde o arquivo
   escolher. A pergunta "essa memória é do programa?" é respondida pelas
   tabelas de página do espaço ativo: mapeada, com USUARIO, numa entrada
   do programa.

7. **Por que o RSP inicial é `PILHA_ELF_TOPO - 8`, e por que o teste com
   `PILHA_ELF_TOPO` não falhou?**
   A convenção de chamada diz que, no início de uma função, `RSP + 8` é
   múltiplo de 16 (como se um `call` tivesse acabado de empilhar o
   retorno). Só as instruções que exigem alinhamento (como `movaps`) falham
   com a pilha desalinhada, e nenhum dos três programas as usa.

</details>
