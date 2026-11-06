# 01: Como o kernel escreve texto: serial e tela

> **Você vai entender:** como um kernel consegue mostrar texto sem nenhum
> sistema operacional por baixo; o que é uma porta de E/S; como a tela
> funciona como um grande array de pixels; como as letras são desenhadas;
> e como o `kprintln!` junta tudo.
>
> **Arquivos:** `src/serial.rs`, `src/console.rs`, e as macros no começo de `src/main.rs`
> **Livro:** capítulo 3.1 (serial) e 3.3 (VGA/framebuffer)
> **Rust novo aqui:** traits ([§5](rust-basico.md#5-traits-interfaces)),
> `Option` ([§6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)),
> ponteiros crus e `unsafe` ([§9](rust-basico.md#9-ponteiros-crus-e-unsafe)),
> `static` ([§10](rust-basico.md#10-static-e-const)),
> `Mutex` ([§11](rust-basico.md#11-mutex-e-once-da-crate-spin)),
> `asm!` ([§15](rust-basico.md#15-asm-assembly-dentro-do-rust))

---

## 1. O problema

Na fase 1 (doc 00), escrever texto é fácil: `uefi::println!` pede ao
firmware para escrever. Depois de `exit_boot_services`, o firmware foi
embora. **Não existe mais nenhuma função que escreva texto.** Se o kernel
travar, a tela fica parada e você não tem a menor pista do que aconteceu.

Precisamos de dois canais de saída que nós mesmos controlamos:

| Canal | Vantagem | Desvantagem |
|---|---|---|
| **Porta serial** | Funciona com uma única instrução da CPU, antes de qualquer outra coisa. O QEMU grava tudo num arquivo ou mostra no terminal. | Num PC real, precisa de um cabo/adaptador serial para ver. |
| **Tela (framebuffer)** | É o que o usuário vê. | Precisamos desenhar cada letra pixel a pixel. |

O SiliconOS escreve **nos dois ao mesmo tempo**.

---

## 2. A porta serial

### 2.1 O que é

A porta serial (COM1, aquele conector de 9 pinos dos PCs antigos) é a forma
mais simples de um computador mandar bytes para fora, um bit de cada vez.
Placas-mãe modernas quase não têm o conector, mas o **chip** continua
existindo e todo emulador o implementa. Para depuração ela é perfeita:

- não depende de vídeo, memória ou interrupções configuradas;
- o QEMU pode mandar tudo para o terminal (`-serial stdio`) ou para um
  arquivo (`-serial file:serial.log`). O arquivo fica salvo **mesmo que a
  máquina reinicie logo depois**, e muitas vezes é a única pista de um
  travamento.

### 2.2 Como se fala com ela: portas de E/S

O processador x86 tem dois "espaços de endereços" separados:

1. **a memória**, acessada com instruções comuns (`mov`);
2. **as portas de E/S**: 65 536 endereços (de `0x0000` a `0xFFFF`),
   acessados **só** com duas instruções especiais:
   - `out`: envia um byte para uma porta;
   - `in`: lê um byte de uma porta.

Cada dispositivo antigo "escuta" em algumas dessas portas. Escrever na
porta certa é como chamar uma função do dispositivo. A COM1 usa as portas
`0x3F8` a `0x3FF`.

Em Rust não existe função pronta para `in`/`out`, então usamos assembly
embutido ([rust-basico §15](rust-basico.md#15-asm-assembly-dentro-do-rust)):

```rust
pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value,
            options(nomem, nostack, preserves_flags));
    }
}
```

Lendo a linha do `asm!`:

- `"out dx, al"` é a instrução: "mande o byte que está no registrador AL
  para a porta cujo número está em DX". O x86 exige exatamente esses
  registradores para essa instrução.
- `in("dx") port` significa "antes de executar, coloque a variável `port`
  em DX".
- `in("al") value` significa "coloque `value` em AL".
- `options(...)` são promessas ao compilador: a instrução não lê nem
  escreve memória (`nomem`), não usa a pilha (`nostack`) e não altera as
  flags da CPU (`preserves_flags`). Com isso ele pode otimizar melhor o
  código ao redor.

A função é `unsafe fn` porque mandar um byte para a porta errada pode
fazer qualquer coisa (desligar um dispositivo, por exemplo); quem chama
precisa saber o que está fazendo. `inb` é igual, no sentido inverso.

### 2.3 Configurando o chip (UART 16550)

O chip da serial se chama UART. Cada uma das 8 portas a partir de `0x3F8`
é um **registrador** do chip, uma espécie de "variável" interna dele:

| Porta | Registrador | O que fazemos com ele |
|---|---|---|
| `0x3F8` (+0) | dados, ou parte baixa da velocidade | enviar o byte; definir a velocidade |
| `0x3F9` (+1) | interrupções, ou parte alta da velocidade | desligamos as interrupções do chip |
| `0x3FA` (+2) | controle do buffer (FIFO) | ligamos e limpamos |
| `0x3FB` (+3) | formato da transmissão | 8 bits por caractere, sem paridade, 1 bit de parada ("8N1") |
| `0x3FC` (+4) | controle do modem | sinaliza "estou pronto" (DTR + RTS) |
| `0x3FD` (+5) | status | o bit 5 diz "pode mandar o próximo byte" |

A função `Serial::init()` é só essa configuração, porta por porta:

```rust
pub fn init() {
    unsafe {
        outb(COM1 + 1, 0x00); // desliga interrupções da UART
        outb(COM1 + 3, 0x80); // DLAB=1: próximos 2 registradores são o divisor
        outb(COM1 + 0, 0x03); // divisor = 3 -> 115200 / 3 = 38400 baud
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03); // DLAB=0, formato 8N1
        outb(COM1 + 2, 0xC7); // liga e limpa as FIFOs
        outb(COM1 + 4, 0x03); // DTR + RTS
    }
}
```

O trecho mais estranho é o da velocidade. O chip tem um relógio de 115 200
"batidas" por segundo, e a velocidade é definida por um **divisor**: 3 dá
115 200 / 3 = 38 400 bits por segundo (*baud*). Como só há 8 portas para
muitas configurações, o chip reaproveita portas: ligar o bit **DLAB**
(valor `0x80` na porta +3) faz as portas +0 e +1 passarem a significar
"divisor" em vez de "dados". Gravamos o divisor e desligamos o DLAB.

### 2.4 Enviando um byte

```rust
pub fn write_byte(b: u8) {
    unsafe {
        while inb(COM1 + 5) & 0x20 == 0 {   // bit 5 do status ainda é 0?
            core::hint::spin_loop();        // então espere
        }
        outb(COM1, b);                      // pronto: envia
    }
}
```

O chip só aceita um byte novo quando terminou de enviar o anterior. O bit
5 (`0x20` = `0010 0000`) da porta de status diz se ele está livre. Ficamos
em loop até estar. `spin_loop()` é uma dica para a CPU de que estamos
esperando em loop (ela economiza energia).

> **Curiosidade:** a primeira versão do SiliconOS mandava "KO" para a porta
> sem configurar nem esperar. Funcionava porque o QEMU é tolerante; num
> chip real, bytes se perderiam.

### 2.5 Transformando a serial num "destino de texto"

Para escrever texto formatado (`"x = {}"`), implementamos a **trait**
`core::fmt::Write`, a "interface" de destinos de texto do Rust
([rust-basico §5](rust-basico.md#5-traits-interfaces)):

```rust
pub struct Serial;           // struct sem campos: só um "nome" para pendurar métodos

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                Serial::write_byte(b'\r');   // terminais seriais querem \r\n
            }
            Serial::write_byte(b);
        }
        Ok(())
    }
}
```

Implementando só `write_str`, a trait nos dá `write_fmt` de graça, e com
ele o `writeln!(Serial, "valor = {}", x)` funciona.

---

## 3. A tela: framebuffer

### 3.1 A tela é um array de pixels na memória

Com UEFI, o vídeo funciona assim: o firmware reserva uma região da memória
em que **cada pixel da tela é um número de 32 bits** (`u32`). Escrever um
número nessa memória **muda a cor do pixel na hora**. Essa região se chama
**framebuffer**.

```
 framebuffer (memória)                       tela
 ┌────┬────┬────┬─────┬────┐              ┌─────────────┐
 │ px │ px │ px │ ... │ px │  linha 0  →  │ ▓▓▓▓▓▓▓▓▓▓  │
 ├────┼────┼────┼─────┼────┤              │             │
 │ px │ px │ px │ ... │ px │  linha 1  →  │             │
 └────┴────┴────┴─────┴────┘              └─────────────┘
```

É um array de uma dimensão representando uma imagem de duas dimensões.
Para achar o pixel da coluna `x`, linha `y`:

```
índice = y * stride + x
```

**`stride`** é quantos pixels cada linha ocupa **na memória**. Pode ser
maior que a largura visível, porque algumas placas de vídeo arredondam o
tamanho das linhas. No QEMU, stride = largura = 1280. Usar a largura no
lugar do stride é um bug clássico: num hardware em que eles diferem, a
imagem sai "inclinada".

Cada `u32` é uma cor no formato `0x00RRGGBB`: `0x00FF0000` é vermelho,
`0x0000FF00` é verde, `0x00FFFFFF` é branco. (O UEFI pode informar outra
ordem, BGR; no QEMU é RGB, e é o que assumimos por enquanto.)

### 3.2 A struct `FrameBuffer`

```rust
#[derive(Clone, Copy)]
pub struct FrameBuffer {
    pub base: *mut u32,    // endereço do primeiro pixel
    pub width: usize,
    pub height: usize,
    pub stride: usize,
}
```

`*mut u32` é um **ponteiro cru**: um endereço de memória, como em C
([rust-basico §9](rust-basico.md#9-ponteiros-crus-e-unsafe)). Usamos ponteiro
cru porque o Rust não tem como saber que esse endereço é válido: foi o
firmware que nos disse.

### 3.3 Pintando um pixel

```rust
fn pixel(&mut self, x: usize, y: usize, cor: u32) {
    if x < self.fb.width && y < self.fb.height {
        unsafe { self.fb.base.add(y * self.fb.stride + x).write_volatile(cor) };
    }
}
```

- `base.add(n)` calcula o endereço do n-ésimo `u32` depois de `base`
  (aritmética de ponteiro).
- `write_volatile(cor)` grava a cor nesse endereço. O **volatile** é
  importante: diz ao compilador **"não otimize esta escrita"**. Sem ele, o
  otimizador poderia pensar "ninguém lê essa memória depois, então a
  escrita é inútil" e removê-la. Ele não sabe que uma placa de vídeo está
  lendo.
- O `if` evita escrever fora da tela, o que corromperia outra memória.

### 3.4 Desenhando letras: fonte bitmap

Não há fonte nenhuma disponível. Usamos a crate `font8x8`, que é
basicamente uma **tabela de bits**: para cada um dos 128 caracteres ASCII,
8 bytes, um por linha; em cada byte, cada bit diz se aquele pixel fica
aceso.

Por exemplo, a letra **A**:

```
byte   binário      desenho (bit 0 à esquerda)
0x0C   0000 1100    . . # # . . . .
0x1E   0001 1110    . # # # # . . .
0x33   0011 0011    # # . . # # . .
0x33   0011 0011    # # . . # # . .
0x3F   0011 1111    # # # # # # . .
0x33   0011 0011    # # . . # # . .
0x33   0011 0011    # # . . # # . .
0x00   0000 0000    . . . . . . . .
```

Nessa fonte, o **bit 0** é o pixel **mais à esquerda**, por isso o desenho
parece "espelhado" em relação ao binário escrito.

A função que desenha um caractere percorre os 8×8 bits:

```rust
fn desenhar_char(&mut self, c: u8, col: usize, row: usize) {
    let glifo = BASIC_LEGACY[(c & 0x7F) as usize];   // os 8 bytes da letra
    for (gy, linha) in glifo.iter().enumerate() {    // gy = 0..7 (linha do desenho)
        for gx in 0..8 {                             // gx = 0..7 (coluna do desenho)
            let cor = if linha & (1 << gx) != 0 { self.cor } else { self.fundo };
            for dy in 0..ESCALA {                    // cada bit vira um bloco ESCALA×ESCALA
                for dx in 0..ESCALA {
                    self.pixel(col * CHAR_W + gx * ESCALA + dx,
                               row * CHAR_H + gy * ESCALA + dy, cor);
                }
            }
        }
    }
}
```

- `c & 0x7F` garante um índice entre 0 e 127, o tamanho da tabela.
- `linha & (1 << gx) != 0` testa se o bit `gx` está ligado
  ([glossário: bits](glossario.md#bits-bytes-e-hexadecimal-revisão-rápida)).
- Uma letra de 8×8 pixels é minúscula numa tela de 1280×800, então cada
  bit vira um quadrado de 2×2 (`ESCALA = 2`). Cada letra ocupa 16×16
  pixels, mais 4 pixels de espaço entre linhas (`CHAR_H = 8*2 + 4`).
- `.iter().enumerate()` é como um `for (i, item)` com índice: devolve pares
  (posição, valor).

A tabela só tem ASCII. Letras acentuadas aparecem como `?`, por isso as
mensagens do kernel são escritas sem acento ("memoria", "excecao").

### 3.5 O `Console`: cursor, quebra de linha e rolagem

```rust
pub struct Console {
    fb: FrameBuffer,
    col: usize,       // coluna do cursor (em caracteres, não em pixels)
    row: usize,       // linha do cursor
    pub cor: u32,     // cor do texto
    pub fundo: u32,   // cor do fundo
}
```

O `Console` funciona como um terminal simples:

- **`write_byte(b)`**: se for `\n`, pula para a próxima linha; senão,
  desenha o caractere na posição do cursor e avança uma coluna. Se a linha
  encher, quebra automaticamente.
- **`rolar()`**: quando o cursor passa da última linha, copia toda a imagem
  uma linha de texto para cima (`core::ptr::copy`, que equivale ao
  `System.arraycopy` do Java) e pinta a última linha com a cor de fundo.

Assim como a `Serial`, o `Console` implementa `fmt::Write`, então
`writeln!(console, ...)` também funciona.

---

## 4. `kprintln!`: escrevendo nos dois lugares

### 4.1 A variável global do console

O console precisa ser acessível de **qualquer lugar** do kernel: hoje do
`kernel_main`, amanhã de dentro de um tratador de erro ou de interrupção.
Ou seja, precisa ser uma variável global:

```rust
pub static CONSOLE: Mutex<Option<Console>> = Mutex::new(None);
```

Lendo de dentro para fora:

| Parte | Por quê |
|---|---|
| `Console` | o objeto que desenha na tela |
| `Option<...>` | ele começa **vazio** (`None`), porque o endereço da tela só é conhecido durante a execução. O `kernel_main` coloca `Some(Console::new(fb))` |
| `Mutex<...>` | garante que só um trecho de código use o console por vez ([rust-basico §11](rust-basico.md#11-mutex-e-once-da-crate-spin)) |
| `static` | variável global com endereço fixo ([rust-basico §10](rust-basico.md#10-static-e-const)) |

Por que um `Mutex` se ainda não há threads? Porque logo haverá
**interrupções** (capítulo 9). Uma interrupção pode chegar **no meio** de um
`kprintln!` e o tratador dela também querer imprimir. Sem proteção, as duas
escritas se misturariam. Além disso, o Rust simplesmente **não deixa**
criar uma global mutável sem proteção, a não ser com `unsafe`.

O `Mutex` é da crate `spin`: em vez de "pôr a thread para dormir" (o que
exigiria um sistema operacional), ele fica em loop tentando pegar o lock.

Um detalhe técnico: o Rust só permite colocar num `static` compartilhado
tipos que ele considera seguros para compartilhar entre threads (a trait
`Send`). Um ponteiro cru (`*mut u32`) não é considerado seguro
automaticamente, então declaramos manualmente:

```rust
unsafe impl Send for FrameBuffer {}
```

"Eu garanto que é seguro", e é mesmo, porque todo acesso passa pelo `Mutex`.

### 4.2 A macro

```rust
#[macro_export]
macro_rules! kprintln {
    () => { $crate::kprint!("\n") };
    ($($arg:tt)*) => { $crate::kprint!("{}\n", format_args!($($arg)*)) };
}
```

A sintaxe de `macro_rules!` é densa, mas a ideia é simples: `kprintln!` é
"`kprint!` com um `\n` no fim", e `kprint!` empacota os argumentos com
`format_args!` (a formatação da `core`, que **não aloca memória**) e chama
a função `_print`:

```rust
pub fn _print(args: core::fmt::Arguments) {
    let _ = Serial.write_fmt(args);                      // 1. serial, sempre
    if let Some(con) = CONSOLE.lock().as_mut() {         // 2. tela, se já existir
        let _ = con.write_fmt(args);
    }
}
```

- `CONSOLE.lock()` pega o lock; `.as_mut()` dá acesso ao `Console` de
  dentro do `Option`; `if let Some(con)` só entra se o console já foi
  criado ([rust-basico §6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)).
- O lock é **liberado sozinho** no fim do `if`, quando o "guarda" devolvido
  por `lock()` sai de escopo.
- `let _ =` ignora de propósito o resultado da escrita: se a tela falhar,
  não há o que fazer.

Uso, igual a um `println!`:

```rust
kprintln!("Video: {}x{} (stride {})", fb.width, fb.height, fb.stride);
```

---

## 5. Como foi testado

Rodando no QEMU sem janela (`SEM_TELA=1 ./run.sh`), o arquivo `serial.log`
recebe todas as linhas, e uma captura da tela feita pelo monitor do QEMU
mostra o mesmo texto desenhado em branco sobre azul-escuro.

---

## Diferença em relação ao livro

O livro também mostra o **modo texto VGA**: um modo antigo em que a placa de
vídeo desenha as letras sozinha e o programa só escreve códigos de
caracteres no endereço `0xB8000`. Esse modo **só existe com boot por BIOS**.
Com UEFI, o único vídeo disponível é o framebuffer de pixels, por isso o
SiliconOS vai direto para ele.

---

## Resumo

- A **serial** é o canal de depuração mais confiável: uma instrução `out`
  manda um byte; o QEMU mostra ou grava tudo.
- **Portas de E/S** são um espaço de endereços separado da memória,
  acessado com `in`/`out`.
- O **framebuffer** é a tela vista como um array de `u32` na memória:
  `índice = y * stride + x`.
- Letras são desenhadas a partir de uma **tabela de bits** (8 bytes por
  caractere).
- **`kprintln!`** formata o texto e o envia à serial e ao console global
  (`static Mutex<Option<Console>>`).

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que a escrita no framebuffer usa `write_volatile` em vez de uma
   atribuição normal?**
   Para o compilador não remover a escrita achando que ela é inútil. Quem
   lê aquela memória é a placa de vídeo, que o compilador não conhece.

2. **O que acontece se usarmos `width` no lugar de `stride` num hardware
   em que eles são diferentes?**
   Cada linha começa no lugar errado e a imagem sai inclinada e embaralhada.

3. **Por que o console é um `Option`?**
   Porque ele só pode ser criado quando sabemos o endereço da tela, em
   tempo de execução. Antes disso, ele é `None` e o `kprintln!` escreve só
   na serial.

4. **Por que o `Serial::write_byte` espera o bit 5 da porta de status?**
   Porque o chip só aceita um novo byte depois de terminar de enviar o
   anterior. Sem a espera, bytes se perdem.

</details>
