# 09: Shell, automated tests and debugging with GDB

> **You will understand:** how to turn the keyboard echo into a real
> **command line**; how the kernel finds out the CPU model (`cpuid`) and the
> time (RTC); how a PC is rebooted and **powered off** by software; how to
> make `cargo test` work **inside QEMU**, with a custom test framework; and
> how to stop the kernel in the middle of execution with **GDB**.
>
> **Files:** `src/shell.rs` (the shell), `src/testes.rs` and `src/qemu.rs`
> (the test framework), `testar.sh` and `.cargo/config.toml` (`cargo test`
> runs in QEMU), `depurar.sh` (QEMU + GDB), `reiniciar` in `src/cpu.rs`,
> `desligar` in `src/acpi.rs`, `hora_rtc` in `src/interrupcoes/timer.rs`
> **Book:** chapter 10
> **Try it:** `./run.sh` and type `ajuda`; `cargo test --target x86_64-unknown-uefi`;
> `cargo test --target x86_64-unknown-uefi --features teste-que-falha`; `./depurar.sh`
> **Read first:** doc 08 (keyboard and timer)
> **New Rust here:** function pointers ([§31](rust-basico.md#31-function-pointers-fn)),
> `let ... else` and `split_first` ([§32](rust-basico.md#32-let--else-and-split_first)),
> `#[cfg(test)]` and the custom test framework ([§33](rust-basico.md#33-tests-cfgtest-and-test_case)),
> `dyn Trait` and implementing for every `T` ([§34](rust-basico.md#34-dyn-trait-and-implementing-for-every-t))
>
> **Names in the code:** `linha` = *line*, `comando(s)` = *command(s)*,
> `executar` = *execute*, `ajuda` = *help*, `eco` = *echo*, `limpar` =
> *clear*, `tempo` = *time*, `traduzir` = *translate*, `ler` = *read*,
> `reiniciar` = *reboot*, `desligar` = *power off*, `testes` = *tests*,
> `testar` = *test (verb)*, `depurar` = *debug*, `sair` = *exit*, `sucesso`
> / `falha` = *success / failure*, `rodar` = *run*.
>
> **Code for this chapter:** [state at the end of doc 09](https://github.com/moises-dev-git/siliconOS/tree/doc-09) · [what changed since doc 08](https://github.com/moises-dev-git/siliconOS/compare/doc-08...doc-09) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

At the end of doc 08, SiliconOS could read the keyboard, but it only
repeated on screen what was typed. Three things are missing for it to be
usable day to day (our day to day as kernel developers):

1. **Talking to it.** Until now, seeing the state of the heap or testing an
   address translation meant writing code, building and running. A **shell**
   lets us ask on the spot: `mem`, `traduzir 0x...`.
2. **Tests that say "passed" or "failed" by themselves.** The 21 test
   features work, but each one requires reading the output and deciding
   whether it is right. The `cargo test` you know (JUnit, pytest) does that
   automatically, and we want the same for the kernel, even though it runs
   inside an emulator.
3. **A debugger.** When something freezes, printing messages does not
   always help (in doc 08, the deadlock froze `kprintln!` itself). We need to
   be able to stop the CPU and look at where it is.

---

## 2. The shell

### 2.1 From key to line

Doc 08 left a loop that translated scancodes and printed each character.
The shell steps in: instead of printing directly, each character goes to
`Shell::tecla` ("key"), which builds the line in a `String` (doc 07):

```rust
// src/shell.rs
pub fn tecla(&mut self, c: char) {
    match c {
        '\n' => {
            kprintln!();
            // `take` returns the line and leaves an empty String in its place.
            let linha = core::mem::take(&mut self.linha);
            executar(&linha);
            kprint!("{}", PROMPT);
        }
        // Backspace: only erases what was typed, never the prompt.
        '\x08' => {
            if self.linha.pop().is_some() {
                kprint!("\x08");
            }
        }
        c if (c.is_ascii_graphic() || c == ' ') && self.linha.len() < MAX_LINHA => {
            self.linha.push(c);
            kprint!("{}", c);
        }
        _ => {} // Tab, Esc, full line...: ignored
    }
}
```

Three details:

- **The echo belongs to the shell, not the keyboard.** The shell decides
  whether the character shows up on screen: a letter beyond the 70 limit
  does not appear, and a Backspace on an empty line does not erase
  `silicon> `. It is like that in real terminals too: the program (or the
  terminal, in "cooked mode") decides what to echo.
- **`String::pop`** returns `Option<char>`: `None` when the line is empty.
  The `if ... is_some()` only sends the Backspace to the screen if there was
  something to erase.
- **`core::mem::take`** ([§30](rust-basico.md#30-taking-a-closure-impl-fnonce))
  hands the finished line to `executar` and leaves the shell with an empty
  line for the next one, without copying anything.

The main loop, now in `shell::rodar`, is the same as in doc 08 with the
shell in place of `kprint!`:

```rust
pub fn rodar() -> ! {
    let mut decodificador = teclado::Decodificador::new();
    let mut shell = Shell::new();
    kprintln!("Shell do SiliconOS. Digite 'ajuda' para ver os comandos.");
    kprint!("{}", PROMPT);
    loop {
        while let Some(scancode) = teclado::proximo_scancode() {
            if let Some(c) = decodificador.processar(scancode) {
                shell.tecla(c);
            }
        }
        teclado::esperar();
    }
}
```

(*SiliconOS shell. Type 'ajuda' to see the commands.*)

Note **where** the command runs: in the main loop, outside any interrupt
handler. A command may take as long as it wants, print, allocate; the timer
and keyboard keep arriving normally (and keys typed during a slow command
wait in doc 08's queue).

### 2.2 The command table

Each command is a row in a table:

```rust
pub struct Comando {
    pub nome: &'static str,
    pub uso: &'static str,
    pub descricao: &'static str,
    pub funcao: fn(&[&str]),
}

pub const COMANDOS: &[Comando] = &[
    Comando { nome: "ajuda", uso: "ajuda", descricao: "lista os comandos", funcao: ajuda },
    Comando { nome: "eco", uso: "eco <texto>", descricao: "repete o texto", funcao: eco },
    // ...
];
```

(`nome` = *name*, `uso` = *usage*, `descricao` = *description*, `funcao` =
*function*.)

The `funcao` field is a **function pointer** ([§31](rust-basico.md#31-function-pointers-fn)):
the address of a function that takes the arguments already split
(`&[&str]`, a slice of pieces of text). It is the Rust version of a Java
`Map<String, Consumer<List<String>>>`, only fixed at compile time and
without allocating anything.

Executing a line means splitting it into words, finding the command by
name and calling its function:

```rust
pub fn executar(linha: &str) {
    let palavras: Vec<&str> = linha.split_whitespace().collect();
    let Some((&nome, argumentos)) = palavras.split_first() else {
        return; // empty line
    };
    match COMANDOS.iter().find(|c| c.nome == nome) {
        Some(comando) => (comando.funcao)(argumentos),
        None => kprintln!("comando desconhecido: '{}' (digite 'ajuda')", nome),
    }
}
```

(`palavras` = *words*, `argumentos` = *arguments*; *unknown command: '...'
(type 'ajuda')*.)

`split_first` splits the slice into "first element" and "the rest"
([§32](rust-basico.md#32-let--else-and-split_first)): `"traduzir 0x1000"`
becomes `nome = "traduzir"` and `argumentos = ["0x1000"]`. The `&str`s in
`palavras` are not copies: they are pieces of the line itself
(`split_whitespace` only points into it).

The advantage of the table shows up in `ajuda`, which does not need to be
updated by hand when a new command comes in:

```rust
fn ajuda(_: &[&str]) {
    for c in COMANDOS {
        kprintln!("  {:<22}{}", c.uso, c.descricao);
    }
}
```

### 2.3 The commands

```
silicon> ajuda
  ajuda                 lista os comandos
  eco <texto>           repete o texto
  limpar                limpa a tela
  tempo                 tempo ligado e hora do relogio CMOS
  mem                   quadros livres e estado do heap
  cpu                   fabricante e modelo (instrucao cpuid)
  int                   contadores de interrupcoes
  traduzir <endereco>   mostra a traducao de um endereco virtual
  ler <endereco>        le 8 bytes da memoria (pode dar page fault!)
  reiniciar             reinicia a maquina
  desligar              desliga a maquina (ACPI; testado so no QEMU)
```

In English: `ajuda` = help (lists the commands); `eco <texto>` = echo the
text; `limpar` = clear the screen; `tempo` = uptime and CMOS clock time;
`mem` = free frames and heap state; `cpu` = vendor and model (`cpuid`
instruction); `int` = interrupt counters; `traduzir <endereco>` = show the
translation of a virtual address; `ler <endereco>` = read 8 bytes of memory
(may page fault!); `reiniciar` = reboot; `desligar` = power off (ACPI;
tested only on QEMU).

Most of them just put together what the earlier chapters already built:

| Command | Where it comes from |
|---|---|
| `mem` | `quadros::livres()` (doc 05) and `heap::estatisticas()` (doc 07) |
| `traduzir` | `explicar_traducao` (doc 06), the same one as at boot |
| `ler` | a direct `read_volatile`: `ler 0` shows doc 06's page fault |
| `int` | the timer, keyboard and spurious counters (doc 08) |
| `tempo` | `timer::milissegundos()` (doc 08) and the RTC time |
| `limpar` | `Console::limpar` (doc 01), with interrupts off |

Typed numbers can be decimal or hexadecimal, and accept `_` as a separator,
like Rust (`0xffff_9000_0000_0000`):

```rust
pub fn ler_numero(texto: &str) -> Option<u64> {
    let limpo: String = texto.chars().filter(|&c| c != '_').collect();
    match limpo.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => limpo.parse().ok(),
    }
}
```

Three commands bring something new: `cpu`, `tempo` (the time) and the
`reiniciar`/`desligar` pair.

### 2.4 `cpu`: the `cpuid` instruction

`cpuid` is how a program asks the CPU who it is and what it can do. You put
a "leaf" number in EAX, run `cpuid`, and the answer comes back in EAX, EBX,
ECX and EDX. Rust offers the instruction ready-made in
`core::arch::x86_64::__cpuid`:

```rust
// Leaf 0: the vendor, 12 letters in EBX, EDX, ECX (in that order).
let r = __cpuid(0);
let mut fabricante = [0u8; 12];
fabricante[0..4].copy_from_slice(&r.ebx.to_le_bytes());
fabricante[4..8].copy_from_slice(&r.edx.to_le_bytes());
fabricante[8..12].copy_from_slice(&r.ecx.to_le_bytes());
```

The vendor is a 12-letter text spread over three registers, in the odd order
EBX, EDX, ECX: `"GenuineIntel"`, `"AuthenticAMD"`... Leaves `0x8000_0002` to
`0x8000_0004` carry the model's commercial name, 48 letters in 12 registers.
Leaf 1 has family/model and, in EDX, one bit per feature (bit 9 says
whether there is a local APIC).

```
silicon> cpu
fabricante: AuthenticAMD
modelo:     QEMU Virtual CPU version 2.5+
familia 15, modelo 11, revisao 1; APIC local: sim
```

(*vendor; model; family 15, model 11, stepping 1; local APIC: yes*)

That is the CPU QEMU emulates by default (`qemu64`), which presents itself
as AMD. With KVM and `-cpu host`, the computer's real CPU would show up.

> Why not use `asm!("cpuid")` directly? Because `cpuid` writes to EBX, and
> the compiler reserves RBX for its own use: `asm!` does not accept RBX as
> an output. The `__cpuid` function takes care of saving and restoring RBX.

### 2.5 `tempo`: the CMOS clock time

Doc 08's test only read the RTC's seconds. For the full time, you read
registers 4 (hour), 2 (minute) and 0 (second), with two precautions
inherited from 1984: the values may be in **BCD** (each decimal digit in 4
bits: 19 h is `0x19`) and the hour may be in **12-hour** format, with bit 7
meaning PM. Register 0x0B says which is the case:

```rust
let b = registrador(0x0B);
let pm = h & 0x80 != 0; // in 12-hour format, bit 7 of the hour = PM
h &= 0x7F;
let bcd = |v: u8| (v >> 4) * 10 + (v & 0x0F);
let (mut h, m, s) = if b & 0x04 != 0 { (h, m, s) } else { (bcd(h), bcd(m), bcd(s)) };
if b & 0x02 == 0 {
    // 12-hour format: 12 AM = 0h, 1-11 PM = 13-23h.
    h = (h % 12) + if pm { 12 } else { 0 };
}
```

```
silicon> tempo
ligado ha 0h 00m 03.970s (397 tiques a 100 Hz)
relogio CMOS (RTC): 19:04:55
```

(*on for 0h 00m 03.970s (397 ticks at 100 Hz); CMOS clock (RTC): 19:04:55*)

QEMU gives the RTC the host computer's UTC time: 19:04 UTC was 16:04 in
Brasília at the moment of the test. The RTC knows nothing about time zones.

### 2.6 `reiniciar` and `desligar`

**Rebooting** uses a PC AT trick: the keyboard controller (doc 08's 8042)
also controls the CPU's **RESET** line, because in 1984 it was the chip
with a spare pin. Command `0xFE` on port 0x64 pulses that line:

```rust
// src/cpu.rs
pub fn reiniciar() -> ! {
    desligar_interrupcoes();
    unsafe {
        while inb(0x64) & 0b10 != 0 {} // wait for the controller to accept a command
        outb(0x64, 0xFE);
    }
    // Still here? Plan B: triple fault.
    let idt_vazia = crate::gdt::PonteiroTabela { limite: 0, base: 0 };
    unsafe {
        asm!("lidt [{}]", "int3", in(reg) &idt_vazia, options(noreturn));
    }
}
```

Plan B uses what we saw in doc 04: with a zero-size IDT, `int3` finds no
handler, which becomes a double fault, which has no handler either, and
that is a **triple fault**: the CPU gives up and resets. It is the
"emergency reboot" every kernel under development knows well.

**Powering off** is more complicated, because there has never been a
standard "power off" pin. Since the 1990s **ACPI** takes care of it (doc
08): powering off means putting the machine into state **S5** (*soft off*),
writing to the **PM1 control** register the S5 code together with the
`SLP_EN` bit ("enter the state now"). The two pieces of information come
from different places:

- **the PM1 control port** is in the **FADT** table (signature `"FACP"`), at
  byte 64. On QEMU: `0xB004`;
- **the S5 code** is in the **DSDT**, and this is where it gets complicated:
  the DSDT is not a table of fixed fields, it is a program in **AML** (*ACPI
  Machine Language*), a *bytecode* the operating system is supposed to
  interpret.

Interpreting AML is a project of its own. SiliconOS uses the classic
shortcut: search the DSDT for the bytes of the `_S5_` object and read the
first number of the package that comes right after it:

```
  08      '_S5_'    12         0A or nothing    S5 code (SLP_TYPa)
  NameOp  the name  PackageOp  [size, element count, BytePrefix]  ...
```

```rust
// src/acpi.rs
if ler::<[u8; 4]>(p) == *b"_S5_" && ler::<u8>(p - 1) == 0x08 && ler::<u8>(p + 4) == 0x12 {
    let mut q = p + 5;
    let tamanho_pacote: u8 = ler(q);
    q += ((tamanho_pacote >> 6) & 0b11) as u64 + 1; // bits 6-7: extra bytes of the size
    q += 1; // number of elements
    if ler::<u8>(q) == 0x0A { q += 1; } // BytePrefix: the number comes in the next byte
    return Some(ler(q));
}
```

With that, powering off is:

```rust
outw(pm1a, (slp_typ as u16) << 10 | SLP_EN);
```

On QEMU, the S5 code is 0, so the value written is `0x2000`. QEMU closes by
itself, with exit code 0. A detail that showed up in testing: powering off
is not instantaneous. In the first version, the kernel still managed to
print "nao consegui desligar" (*could not power off*) before QEMU closed.
Now it waits 20 ticks (200 ms) before concluding that it failed.

The command's description says "tested only on QEMU" on purpose: the
`_S5_` shortcut works on most machines, but it has not been tested on real
hardware.

---

## 3. Automated tests: `cargo test` inside QEMU

### 3.1 What is missing for `cargo test` to work

In a regular Rust project, `cargo test` builds a special version of the
program in which `main` is replaced by a test runner (the *test harness*,
from the `test` crate), runs that executable and reads the result. In
SiliconOS, three things get in the way:

1. the `test` crate depends on `std` (threads to run tests in parallel,
   output capture, processes), and the kernel is `no_std`;
2. the "executable" is a `.efi`, which does not run on Linux: it needs QEMU;
3. QEMU, when the kernel finishes, has no way to say "passed" or "failed":
   it simply sits there, running `hlt` forever.

Each problem has its solution: a **custom test framework**, a **runner**
that calls QEMU, and the **`isa-debug-exit`** device.

### 3.2 The custom framework (`custom_test_frameworks`)

Nightly Rust lets you replace the test runner:

```rust
// src/main.rs
#![feature(custom_test_frameworks)]
#![test_runner(crate::testes::executar)]
#![reexport_test_harness_main = "rodar_testes"]
```

The three lines say: "in test mode, collect all functions marked
`#[test_case]`, pass the list to `crate::testes::executar`, and generate a
function called `rodar_testes` that does this". The one who calls
`rodar_testes` is the kernel itself, after initializing everything:

```rust
interrupcoes::init();
testes_de_interrupcoes();

// `cargo test`: runs the #[test_case]s and exits QEMU (does not return).
#[cfg(test)]
rodar_testes();
```

The choice of **where** to call it matters: the tests run with the GDT,
IDT, paging, heap and interrupts ready, so they can test all of that.
`#[cfg(test)]` ([§33](rust-basico.md#33-tests-cfgtest-and-test_case)) makes
that line exist only in the test build.

The runner is short:

```rust
// src/testes.rs
pub trait Testavel {
    fn rodar(&self);
}

impl<T: Fn()> Testavel for T {
    fn rodar(&self) {
        kprint!("{} ... ", core::any::type_name::<T>());
        self();
        kprintln!("[ok]");
    }
}

pub fn executar(testes: &[&dyn Testavel]) {
    kprintln!("Rodando {} testes", testes.len());
    for t in testes {
        t.rodar();
    }
    kprintln!("Todos os {} testes passaram.", testes.len());
    qemu::sair(Saida::Sucesso);
}
```

(`Testavel` = *testable*; *Running N tests*, *All N tests passed.*)

- `impl<T: Fn()> Testavel for T` implements the trait for **any** function
  with no parameters ([§34](rust-basico.md#34-dyn-trait-and-implementing-for-every-t)).
  Every `#[test_case]` function automatically becomes a `Testavel`.
- `core::any::type_name::<T>()` gives the name of the function's type, which
  in Rust is its full path:
  `siliconos::memoria::heap::testes::box_mora_no_heap`. So each test
  introduces itself.
- If a test fails, the `assert!` triggers a **panic**. There is no way to
  "catch" the panic and move on to the next test (there is no `catch` in
  `no_std` with `panic = abort`). So the panic handler, in test mode, exits
  QEMU with failure:

```rust
// src/panic.rs (at the end, after showing the message)
#[cfg(test)]
{
    let _ = writeln!(Serial, "[FALHOU]");
    crate::qemu::sair(crate::qemu::Saida::Falha);
}
#[cfg(not(test))]
parar();
```

(`[FALHOU]` = *[FAILED]*.)

### 3.3 Exiting QEMU with a code: `isa-debug-exit`

QEMU has a device made for this. Started with
`-device isa-debug-exit,iobase=0xf4,iosize=0x04`, it creates an I/O port at
`0xF4`: writing a value V to it **ends QEMU**, which returns to Linux the
exit code `(V << 1) | 1`.

```rust
// src/qemu.rs
pub enum Saida {
    /// QEMU exits with (0x10 << 1) | 1 = 33.
    Sucesso = 0x10,
    /// QEMU exits with (0x11 << 1) | 1 = 35.
    Falha = 0x11,
}

pub fn sair(codigo: Saida) -> ! {
    unsafe { outb(PORTA_SAIDA, codigo as u8) };
    // Without the device (real machine, or QEMU without the -device): stop here.
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
```

Why not 0 for success? Because the formula always produces an **odd**
number: 0 is impossible. We chose 33 and 35, and the script translates.

### 3.4 The runner: `.cargo/config.toml` and `testar.sh`

What remains is telling Cargo that, for the UEFI target, "running an
executable" means calling a script:

```toml
# .cargo/config.toml
[target.x86_64-unknown-uefi]
runner = "./testar.sh"
```

Cargo calls `./testar.sh <path-to-the-test-.efi>`. The script puts the
`.efi` in a separate EFI partition (`target/esp-teste/`, so as not to mess
up `run.sh`'s one), runs QEMU without a window with `isa-debug-exit` and
translates the code:

```bash
timeout 60 qemu-system-x86_64 -bios /usr/share/ovmf/OVMF.fd \
    -drive format=raw,file=fat:rw:target/esp-teste -net none -no-reboot \
    -display none -serial stdio \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04
codigo=$?
case $codigo in
    33)  exit 0 ;;                                  # Saida::Sucesso
    35)  echo "testar.sh: o kernel relatou FALHA"; exit 1 ;;
    124) echo "testar.sh: tempo esgotado (60 s)"; exit 2 ;;
    *)   echo "testar.sh: QEMU saiu com codigo $codigo"; exit 3 ;;
esac
```

(*the kernel reported FAILURE; time out (60 s); QEMU exited with code ...*)

`timeout 60` covers the third kind of result, which a kernel has and a
regular program almost never does: **freezing**. A test that deadlocks
(doc 08) neither passes nor fails, it just stops; after 60 s, the script
gives up and returns 2 (`timeout` exits with 124).

```
 cargo test --target x86_64-unknown-uefi
   │
   ├─ builds the kernel with cfg(test): rodar_testes() at the end of init
   ├─ calls ./testar.sh target/.../siliconos-<hash>.efi
   │     └─ QEMU ─► boot ─► initializes everything ─► runs the #[test_case]s
   │                 ├─ all ok   ─► port 0xF4 ← 0x10 ─► QEMU exits with 33 ─► script: 0
   │                 ├─ panic    ─► port 0xF4 ← 0x11 ─► QEMU exits with 35 ─► script: 1
   │                 └─ froze    ─► timeout 60 s     ─► 124                ─► script: 2
   └─ code 0 = "test result: ok"
```

### 3.5 The tests

Each module got, at its end, a block of tests:

```rust
// src/interrupcoes/teclado.rs
#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn shift_e_caps_lock() {
        let mut d = Decodificador::new();
        // Left Shift + a + 1, release Shift, a
        assert_eq!(digitar(&mut d, &[0x2A, 0x1E, 0x02, 0xAA, 0x1E]), "A!a");
        // Caps Lock on: uppercase letter, number stays a number
        assert_eq!(digitar(&mut d, &[0x3A, 0xBA, 0x1E, 0x02]), "A1");
        // ...
    }
}
```

(`digitar` = *type (keys)*.)

There are 24 tests, of two kinds:

- **pure logic**, which could run anywhere: the scancode decoder,
  `ler_numero`, editing the shell line, `alinhar_acima`;
- **the real hardware**, which only makes sense inside the kernel: mapping
  and unmapping a page, page 0 unmapped, the heap with no leaks, the APIC
  MSR, the MADT with the IRQ 0 → GSI 2 override, the DSDT with `_S5_`, the
  timer ticks advancing, `sem_interrupcoes` restoring the state (nested,
  too).

```
$ cargo test --target x86_64-unknown-uefi
     Running unittests src/main.rs (target/x86_64-unknown-uefi/debug/build/siliconos/.../siliconos-....efi)
... (the usual boot) ...
Rodando 24 testes
siliconos::acpi::testes::dsdt_tem_s5 ... [ok]
siliconos::acpi::testes::irq0_desviada_para_gsi2_no_qemu ... [ok]
siliconos::acpi::testes::irq_sem_desvio_vai_para_a_mesma_gsi ... [ok]
siliconos::acpi::testes::madt_tem_cpu_e_ioapic ... [ok]
siliconos::cpu::testes::msr_do_apic ... [ok]
siliconos::cpu::testes::sem_interrupcoes_restaura_o_estado ... [ok]
siliconos::interrupcoes::teclado::testes::enter_e_backspace ... [ok]
siliconos::interrupcoes::teclado::testes::letras_e_numeros ... [ok]
siliconos::interrupcoes::teclado::testes::shift_e_caps_lock ... [ok]
siliconos::interrupcoes::teclado::testes::teclas_estendidas_ignoradas ... [ok]
siliconos::interrupcoes::timer::testes::hora_rtc_valida ... [ok]
siliconos::interrupcoes::timer::testes::tiques_avancam ... [ok]
siliconos::memoria::heap::testes::alinhar_acima_arredonda ... [ok]
siliconos::memoria::heap::testes::box_mora_no_heap ... [ok]
siliconos::memoria::heap::testes::sem_vazamento_e_blocos_juntados ... [ok]
siliconos::memoria::paginacao::testes::erros_de_mapeamento ... [ok]
siliconos::memoria::paginacao::testes::mapear_e_desmapear ... [ok]
siliconos::memoria::paginacao::testes::metade_alta_mostra_a_ram ... [ok]
siliconos::memoria::paginacao::testes::pagina_zero_nao_mapeada ... [ok]
siliconos::memoria::quadros::testes::alocar_e_liberar_devolve_a_contagem ... [ok]
siliconos::shell::testes::editar_linha ... ecx o   [ok]
siliconos::shell::testes::ler_numero_decimal_e_hexadecimal ... [ok]
siliconos::shell::testes::linha_tem_limite ... aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[ok]
siliconos::shell::testes::nomes_de_comandos_unicos ... [ok]
Todos os 24 testes passaram.
$ echo $?
0
```

(The `ecx o` and the row of `a`s are the shell's echo during the editing
tests: the test really types, and the shell echoes as it would for you.)

To see a failure, the `teste-que-falha` feature ("test that fails") adds a
wrong test on purpose:

```
$ cargo test --target x86_64-unknown-uefi --features teste-que-falha
siliconos::testes::este_teste_falha ...
!!! KERNEL PANIC !!!
Local: src/testes.rs:40:5
Mensagem: assertion `left == right` failed: de proposito
  left: 2
 right: 3
[FALHOU]
testar.sh: o kernel relatou FALHA
error: test failed, to rerun pass `--bin siliconos`
$ echo $?
1
```

(`de proposito` = *on purpose*.)

### 3.6 What about the test features?

They stay, and for a good reason: they test what **ends execution** (page
fault, double fault, triple fault, deadlock, exhausted heap) and what needs
a human eye (the panic's red screen, the `teste-deadlock` freeze). A
`#[test_case]` that causes a page fault would bring the kernel down and
end all the tests with "failure", without saying whether the page fault
was the expected one. A fuller framework could run each of those tests in
a separate QEMU and wait for the right panic (the equivalent of
`#[should_panic]`); for now, `cargo test` covers what passes and the
features cover what breaks.

---

## 4. Debugging with GDB

### 4.1 QEMU as a GDB target

GDB, the GNU debugger, can talk to a program over a network protocol (the
*GDB remote protocol*). QEMU speaks that protocol: with the **`-s`** option,
it opens TCP port 1234 and behaves as if the emulated CPU were a program
being debugged. GDB can then stop the CPU, read registers and memory, set
breakpoints and continue. All of it without any help from the kernel: it is
the emulator that obeys.

### 4.2 The symbols problem

For GDB to say `siliconos::shell::executar` instead of `0x5ffdb00`, it
needs the **symbol table** (name → address). Two difficulties show up here,
both because the `.efi` is an executable in the Windows format (PE/COFF):

1. **Rust writes the debug information in Microsoft's format** (a separate
   `.pdb` file, in `target/`), which GDB does not read. Not even the
   function names stay inside the `.efi`. The solution found was to ask the
   linker to write the symbol table inside the `.efi`: the `/debug:symtab`
   option. With it, GDB knows the names and addresses of the functions.
   There are no line numbers or variable names (that would be in the
   `.pdb`).
2. **The address changes on every boot.** In the file, the code starts at
   `0x1_4000_0000` (the PE format's default *ImageBase*). UEFI loads the
   `.efi` wherever there is free memory and fixes up the addresses (the
   *relocations*, the file's `.reloc` section). In two consecutive runs of
   the same `.efi`, the base was `0x5fe8000` and `0x5ff8000`. That is why
   the kernel now prints the base right at the start, getting it from
   UEFI's `LoadedImage` protocol before dismissing it:

```
Imagem do kernel: base 0x6036000, 332 KiB (no arquivo, a base e 0x140000000)
```

(*Kernel image: base 0x6036000, 332 KiB (in the file, the base is
0x140000000)*)

GDB then loads the symbols with an offset:
`add-symbol-file siliconos.efi -o <base>-0x140000000`.

### 4.3 `depurar.sh`

The script does all of that:

```bash
RUSTFLAGS="-C link-arg=/debug:symtab" \
    cargo build --target x86_64-unknown-uefi --target-dir target/simbolos $FEATURES
...
qemu-system-x86_64 ... -serial file:"$SERIAL" -s &     # -s: GDB server on port 1234
...                                                    # waits for "Imagem do kernel: base 0x..."
cat > target/simbolos/gdb.txt <<FIM
target remote :1234
add-symbol-file $EFI -o $BASE-0x140000000
FIM
gdb -q -x target/simbolos/gdb.txt
```

The build with symbols goes to a separate folder (`target/simbolos`),
because changing `RUSTFLAGS` rebuilds everything and we do not want to mix
it with `run.sh`'s normal build.

### 4.4 A session

`./depurar.sh` opens the QEMU window and, in the terminal, GDB already
connected. The CPU stays stopped at the point where GDB connected (the
shell's `hlt`). A breakpoint on the command executor, then `continue`:

```
(gdb) break siliconos::shell::executar
Breakpoint 1 at 0x5ffdb00
(gdb) continue
Continuing.
```

In the QEMU window, you type `mem` and Enter. The CPU stops:

```
Breakpoint 1, 0x0000000005ffdb00 in siliconos::shell::executar ()
(gdb) bt 2
#0  0x0000000005ffdb00 in siliconos::shell::executar ()
#1  0x0000000005ffc254 in <siliconos::shell::Shell>::tecla ()
(gdb) info registers rip rsp
rip            0x5ffdb00           0x5ffdb00 <siliconos::shell::executar>
rsp            0x7e95f38           0x7e95f38
(gdb) x/4i $pc
=> 0x5ffdb00 <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar>:	sub    $0x128,%rsp
   0x5ffdb07 <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar+7>:	mov    %rdx,%r8
   0x5ffdb0a <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar+10>:	mov    %rcx,%rdx
   0x5ffdb0d <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar+13>:	mov    %rdx,0xf8(%rsp)
(gdb) continue
```

And `mem` finishes normally on screen. A few observations:

- `_RNvNtCsfGbHo577FqM_9siliconos5shell8executar` is the function's
  **mangled** name: Rust encodes the full path and a hash into a name the
  linker accepts. GDB translates it back (`siliconos::shell::executar`) where
  it matters.
- The first instruction, `sub $0x128,%rsp`, reserves 296 bytes of stack for
  `executar`'s local variables.
- `bt` (*backtrace*, the call stack) is only reliable in the first levels.
  Without the stack-unwinding information (which would be in the `.pdb`),
  GDB guesses the following levels by reading the stack, and shows garbage.
  The `bt 2` above asks for just the two levels it gets right. We also
  tried building with *frame pointers* (`-C force-frame-pointers=yes`),
  with no improvement.

A breakpoint in the timer handler shows doc 08 from the inside:

```
(gdb) break siliconos::interrupcoes::timer::tratador
(gdb) continue
Breakpoint 1, 0x0000000005fef6f0 in siliconos::interrupcoes::timer::tratador ()
(gdb) bt 2
#0  0x0000000005fef6f0 in siliconos::interrupcoes::timer::tratador ()
#1  0x000000000600aaf3 in siliconos::interrupcoes::teclado::esperar ()
(gdb) info registers eflags
eflags         0x2                 [ IOPL=0 ]
```

The timer interrupted the CPU inside `teclado::esperar` (the `sti; hlt`,
where it spends almost all its time), and inside the handler `eflags` is
`0x2`: the IF bit is **off**, as the *interrupt gate* promises (doc 04).
One caution: a breakpoint in the timer handler fires 100 times per second.
Delete it (`delete`) before continuing.

With `SEM_TELA=1`, the script runs without a window, and GDB's commands can
come from a file or a *pipe*, which is how the outputs above were produced.

---

## 5. How it was tested

- **Shell:** with the QEMU monitor and `sendkey` (doc 08), we typed
  `ajuda`, `eco ola Mundo`, `tempo`, `mem`, `cpu`, `int`,
  `traduzir 0xffff_9000_0000_0000`, `xyz` and an empty line. All outputs in
  this doc come from those runs. The screen (via `screendump`) showed the
  same as the serial port, including `limpar` and a `tempox` fixed with
  Backspace.
- **`ler 0`:** page fault "ao ler 0x0 (pagina nao mapeada, modo kernel)"
  (*on reading 0x0 (page not mapped, kernel mode)*), doc 04's handler in
  action.
- **`reiniciar`:** QEMU, started with `-no-reboot`, closes instead of
  rebooting, which proves the reset happened.
- **`desligar`:** QEMU closed by itself, with exit code 0, without the
  failure message (the FADT gave port `0xB004`; the DSDT, S5 = 0).
- **`cargo test`:** 24 tests, exit code 0. With `teste-que-falha`, code 1
  and the `assert_eq!` message.
- **GDB:** the sessions in section 4.4.
- **Regression:** the earlier 21 builds (normal boot and 20 features) gave
  the expected result.

---

## Changes to the files

What this chapter adds to the code of doc 08 (tag `doc-08`). Each file's link
leads to its state at the end of this chapter (tag `doc-09`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-08...doc-09).

| File | What changes | Where |
|---|---|---|
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/shell.rs) | **new**: `Shell` (`tecla`), `rodar`, `Comando`, `COMANDOS`, `executar`, `ler_numero` and one function per command | — |
| [`src/testes.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/testes.rs) | **new**: `Testavel`, `executar` (the runner) and the `teste-que-falha` feature's test | — |
| [`src/qemu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/qemu.rs) | **new**: `Saida`, `sair` (port 0xF4) | — |
| [`.cargo/config.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/.cargo/config.toml) | **new**: `[target.x86_64-unknown-uefi]` with `runner = "./testar.sh"` | — |
| [`testar.sh`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/testar.sh) | **new** (executable): runs the test `.efi` in QEMU with `isa-debug-exit` and translates 33/35/124 | project root |
| [`depurar.sh`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/depurar.sh) | **new** (executable): builds with `/debug:symtab`, starts QEMU with `-s`, reads the base and opens GDB | project root |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `#![feature(custom_test_frameworks)]`, `#![test_runner(crate::testes::executar)]`, `#![reexport_test_harness_main = "rodar_testes"]` | top, after `#![feature(abi_x86_interrupt)]` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `#[cfg_attr(not(test), allow(dead_code))] mod qemu;`, `mod shell;`, `#[cfg(test)] mod testes;` | with the other `mod`s |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `static IMAGEM_BASE` and `IMAGEM_TAMANHO` (`AtomicU64`); in `main()`, block that opens `LoadedImage` and fills them; in `kernel_main`, the "Imagem do kernel" `kprintln!` | after `CONSOLE`; after `guardar_rsdp()`; after the video line |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `#[cfg(test)] rodar_testes();`; `ecoar_teclado` goes away and the end of `kernel_main` becomes `shell::rodar()` | after `testes_de_interrupcoes()` |
| [`src/panic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/panic.rs) | `#[cfg(test)]` writes `[FALHOU]` and calls `qemu::sair(Falha)`; `#[cfg(not(test))] parar();` | end of the `panic` function |
| [`src/cpu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/cpu.rs) | `reiniciar` (8042 + triple fault) | end of the file |
| [`src/acpi.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/acpi.rs) | `desligar` (FADT + S5) and `tipo_s5` | end of the file |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/interrupcoes/timer.rs) | `hora_rtc` | end of the file |
| [`src/serial.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/serial.rs) | `outw` and `inw` | before `inb` |
| several | a `#[cfg(test)] mod testes { ... }` block with `#[test_case]`s in `acpi.rs`, `cpu.rs`, `interrupcoes/teclado.rs`, `interrupcoes/timer.rs`, `memoria/heap.rs`, `memoria/paginacao.rs`, `memoria/quadros.rs` and `shell.rs` | end of each file |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/Cargo.toml) | `teste-que-falha = []` | `[features]` |

---

## Difference from the book

Chapter 10 of the book covers an interactive shell, tests in QEMU and
debugging with GDB, the same three topics. It is worth comparing as you
read. It is common, in projects with a bootloader, for the kernel
executable to be ELF and carry its debug information in DWARF, which GDB
understands directly; then GDB shows even the line number.

SiliconOS's particularities:

- since the kernel is a UEFI program (PE format), the debug information
  comes out in Microsoft's format. The way out was to write only the symbol
  table (`/debug:symtab`) and compute the offset from the base UEFI reports;
- `cargo test` uses its own runner (`testar.sh`) and a separate EFI
  partition, and the kernel runs the tests **after** initializing
  everything, interrupts included;
- `desligar` reads the FADT and searches the DSDT for `_S5_`, instead of
  using a fixed QEMU port.

---

## Summary

- The **shell** gets characters from the keyboard decoder, builds the line
  in a `String` and, on Enter, looks up the command in a **table of function
  pointers**. It decides the echo (Backspace never erases the prompt) and
  runs commands in the main loop, outside the handlers.
- `cpuid` tells the CPU's vendor and model; the RTC gives the time (BCD and
  12/24 h according to register 0x0B).
- **Reboot:** command `0xFE` to the keyboard controller, with a triple fault
  as plan B. **Power off:** ACPI state S5, with the port from the FADT and
  the `_S5_` code from the DSDT.
- `cargo test` works with a **custom framework** (`custom_test_frameworks`):
  the `#[test_case]`s run at the end of initialization, a panic ends with
  failure, and **`isa-debug-exit`** turns the result into a QEMU exit code
  (33 or 35), which `testar.sh` translates to 0 or 1. A test that freezes
  hits the `timeout`.
- The test features remain for what brings the kernel down on purpose.
- **GDB:** `-s` in QEMU, a symbol table with `/debug:symtab` and the offset
  from the base printed at boot (which changes on every run). Breakpoints by
  function name work; `bt` is only reliable in the first levels.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **Why doesn't the shell's Backspace erase `silicon> `?**
   Because the shell decides the echo: it only sends the Backspace to the
   screen if `self.linha.pop()` removed a character, that is, if something
   had been typed to erase.

2. **What does the `funcao: fn(&[&str])` field hold?**
   The address of a function that takes the command's arguments. `executar`
   finds the command by name and calls `(comando.funcao)(argumentos)`.

3. **Why doesn't the kernel use `asm!("cpuid")` directly?**
   Because `cpuid` writes to EBX, and RBX is reserved by the compiler:
   `asm!` does not accept it as an output. `__cpuid` saves and restores RBX.

4. **Where do the two pieces of information needed to power off come
   from?**
   The PM1 control port comes from the FADT table (byte 64). The S5 state
   code comes from the DSDT, which is AML bytecode; the kernel searches for
   the `_S5_` object and reads the first number of its package.

5. **Why doesn't the standard `cargo test` work in a `no_std` kernel?**
   Because the standard test runner (`test` crate) depends on `std`, the
   `.efi` does not run on Linux, and QEMU has no way to report the result.
   The custom framework, the `testar.sh` runner and `isa-debug-exit` solve
   each of those points.

6. **Why are the exit codes 33 and 35, and not 0 and 1?**
   Because `isa-debug-exit` exits with `(V << 1) | 1`, always odd. 0x10
   gives 33 and 0x11 gives 35. `testar.sh` translates them to 0 and 1.

7. **How does `cargo test` deal with a test that freezes?**
   `testar.sh` runs QEMU with `timeout 60`. If the kernel does not exit in
   60 s, `timeout` ends it and the script returns 2.

8. **Why does GDB need `-o <base>-0x140000000` when loading the symbols?**
   Because the addresses in the file start at ImageBase `0x140000000`, but
   UEFI loads the `.efi` at another address (which changes on every boot)
   and fixes up the code with the relocations. The offset takes each symbol
   from its address in the file to its real address in memory.

</details>
