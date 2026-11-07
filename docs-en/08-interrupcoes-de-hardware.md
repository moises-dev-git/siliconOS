# 08: Hardware interrupts: timer and keyboard

> **You will understand:** what a hardware interrupt (IRQ) is and why it is
> better than asking "is it here yet?" over and over; why a modern PC has
> **two** interrupt controller systems (the 1981 8259 and the APIC); how the
> kernel finds out where they are by reading the **ACPI** tables; how the
> timer starts "ticking" 100 times per second; how a key press becomes a
> character on the screen; and the new danger all of this brings: the
> **deadlock**.
>
> **Files:** `src/interrupcoes/` (`mod.rs`, `pic.rs`, `apic.rs`,
> `ioapic.rs`, `timer.rs`, `teclado.rs`), `src/acpi.rs`, `src/cpu.rs`,
> `mapear_mmio` in `src/memoria/paginacao.rs`, `ecoar_teclado` and
> `testes_de_interrupcoes` in `src/main.rs`
> **Book:** chapter 9
> **Try it:** `./run.sh` (type in the QEMU window), `./run.sh teste-timer`,
> `./run.sh teste-sem-eoi`, `./run.sh teste-print-interrupcao`,
> `./run.sh teste-deadlock`, `./run.sh teste-teclado`
> **Read first:** doc 04 (IDT and exceptions) and doc 06 (paging)
> **New Rust here:** `Acquire` and `Release` ([§28](rust-basico.md#28-orderingacquire-and-release)),
> `#[repr(C, packed)]` and `read_unaligned` ([§29](rust-basico.md#29-reprc-packed-and-read_unaligned)),
> taking a closure with `impl FnOnce` ([§30](rust-basico.md#30-taking-a-closure-impl-fnonce))
>
> **Names in the code:** `interrupcoes` = *interrupts*, `teclado` =
> *keyboard*, `tratador` = *handler*, `tiques` = *ticks*, `fila` = *queue*,
> `escrita`/`leitura` = *write/read (position)*, `desvio` = *override*,
> `rotear` = *route*, `calibrar` = *calibrate*, `esperar` = *wait*,
> `ligar`/`desligar` = *turn on/off*, `sem_interrupcoes` = *without
> interrupts*, `espuria` = *spurious*, `ecoar` = *echo*.
>
> **Code for this chapter:** [state at the end of doc 08](https://github.com/moises-dev-git/siliconOS/tree/doc-08) · [what changed since doc 07](https://github.com/moises-dev-git/siliconOS/compare/doc-07...doc-08) · map in [Changes to the files](#changes-to-the-files)

---

## 1. The problem

Since the first line of `kernel_main`, SiliconOS has run with hardware
interrupts **turned off** (`cli`, doc 04). That made sense: the firmware's
IDT pointed to code that no longer existed, and there was nothing to
handle. But it has a price: the kernel is **deaf**. It does not know that
time passes, and it does not know that someone pressed a key.

There are two ways for a program to find out that something happened in a
device:

1. **Ask all the time** (*polling*): "key yet? key yet? key yet?". That is
   what the serial port does today when writing: it sits in a loop reading
   a status bit until the UART is free (doc 01). It works, but the CPU
   spends 100% of its time asking and does nothing else.
2. **Be told** (*interrupt*): the device sends an electrical signal, the CPU
   **interrupts** whatever it is doing, runs a function of ours (the
   **handler**) and comes back exactly where it stopped.

It is the difference between refreshing your inbox every second and
getting a notification. In a Java or JS program, it is the difference
between a `while (!ready)` loop and a *callback*
(`addEventListener("keydown", ...)`). The interrupt is the hardware's
*callback*.

In this chapter SiliconOS gets two sources of interrupts:

- the **timer**, which interrupts the CPU 100 times per second. That is how
  an operating system knows that time passes (and, later on, switches
  programs: it is the timer that takes the CPU away from whoever has been
  using it for too long);
- the **keyboard**, which interrupts on every key pressed or released.

---

## 2. The idea: from the device to the IDT

The CPU has few "pins" for receiving interrupts, and the PC has dozens of
devices. That is why there is an **interrupt controller** in the middle:
it receives the signals from all devices, decides which is the most
important, and tells the CPU **which vector** to use (the IDT entry number,
doc 04).

Each interrupt line of a device is called an **IRQ** (*interrupt
request*). On the PC, IRQs have traditional numbers, inherited from 1981:

| IRQ | Device |
|---|---|
| 0 | timer (PIT) |
| 1 | keyboard |
| 4 | COM1 serial port |
| 8 | CMOS clock (RTC) |
| 12 | PS/2 mouse |
| 14, 15 | IDE disks |

### A story in two layers

**1981: the 8259.** The original IBM PC had one **8259** chip (the PIC,
*programmable interrupt controller*) with 8 inputs. The PC AT, from 1984,
connected a second 8259 to one of the first one's inputs (**cascaded**):
15 IRQs in total. Simple, but made for **a single CPU**.

**1990s: the APIC.** With the Pentium and machines with several CPUs,
Intel created the **APIC** (*Advanced PIC*), split into two parts:

- the **local APIC**: one per CPU, **inside** it. It receives interrupts and
  delivers them to the core. It also has a **timer** of its own;
- the **I/O APIC**: it sits in the chipset, receives the devices' IRQs and
  forwards them to the local APIC of some CPU.

Today's computers have **both systems**. The 8259 is still there (emulated
inside the chipset) only because an old system, such as DOS, may still
need it. A modern system does what SiliconOS does in this chapter:
**turns off the 8259 and uses the APIC**.

```
     DEVICES                      CHIPSET                       CPU
  ┌──────────────┐  IRQ 1   ┌──────────────┐  message   ┌──────────────────┐
  │ PS/2 keyboard│─────────►│              │"vector 0x31"│    local APIC    │
  └──────────────┘          │   I/O APIC   │────────────►│  ┌────────────┐  │
  ┌──────────────┐  IRQ 4   │  (24 pins)   │            │  │   timer    │  │
  │ COM1 serial  │─────────►│              │            │  │(vector 0x30)│ │
  └──────────────┘          └──────────────┘            │  └────────────┘  │
                                                         │        │         │
  ┌──────────────┐                                       │        ▼         │
  │ 8259 (PIC)   │  turned off: all lines                │   core ──► IDT[vector]
  │ x2, cascaded │  masked                               └──────────────────┘
  └──────────────┘
```

When an interrupt arrives, the CPU does the same as it already does for an
exception (doc 04): it pushes RIP, CS, RFLAGS, RSP and SS, looks up the IDT
at the vector's position and jumps to the handler. The difference is the
**end**: the handler of a hardware interrupt must tell the controller that
it is done (**EOI**, *end of interrupt*, section 5). Without that, the
controller sends nothing more.

### SiliconOS's vectors

| Vectors | Use |
|---|---|
| 0x00–0x1F | CPU exceptions (doc 04). Reserved by Intel. |
| 0x20–0x2F | the turned-off 8259 (only "spurious" interrupts can show up, section 4) |
| **0x30** | local APIC **timer** |
| **0x31** | **keyboard** (IRQ 1, through the I/O APIC) |
| 0xFF | APIC spurious interrupt |

---

## 3. Where are the controllers? The ACPI tables

The local APIC is, by default, at physical address `0xFEE0_0000`, and the
I/O APIC is usually at `0xFEC0_0000`. "Usually" is not enough: on a machine
with two I/O APICs, or with a different chipset, the addresses change. The
one who tells the truth is the **firmware**, through the **ACPI** tables
(*Advanced Configuration and Power Interface*): structures it leaves in
memory describing the machine's hardware (CPUs, controllers, power button,
sensors...).

The table that matters here is the **MADT** (*Multiple APIC Description
Table*). Getting to it means following a chain of pointers:

```
  UEFI: configuration table
    └─ GUID "ACPI 2.0" ──► RSDP  ("RSD PTR ", address of the XSDT)
                              └──► XSDT  (list of table addresses)
                                     ├──► FACP
                                     ├──► APIC  ◄── the MADT (historical name)
                                     ├──► HPET
                                     └──► ...
```

### Step 1: ask UEFI, before dismissing it

Only UEFI knows where the **RSDP** (*Root System Description Pointer*), the
start of the chain, is. It keeps that in its "configuration table", a list
of (GUID, address) pairs. That is why `main` now calls, **before**
`exit_boot_services`:

```rust
// src/acpi.rs
pub fn guardar_rsdp() {
    let endereco = uefi::system::with_config_table(|tabelas| {
        tabelas.iter()
            .find(|t| t.guid == uefi::table::cfg::ACPI2_GUID)
            .map_or(0, |t| t.address as u64)
    });
    RSDP.store(endereco, Ordering::Relaxed);
}
```

(`guardar` = *save*, `endereco` = *address*, `tabelas` = *tables*.)

A GUID is a 128-bit identifier (like a Java UUID). The ACPI 2.0 one is
fixed, defined in the UEFI specification. We only save the physical
address; the tables themselves are read later, when the kernel's page
tables already exist.

### Step 2: reading the tables

The ACPI tables are in `ACPI`-type regions of the memory map (doc 05).
Since doc 06 mapped all of RAM (except MMIO and reserved) in the upper
half, the kernel reads them with `fisico_para_virtual`:

```rust
/// Reads a value of type T at the given PHYSICAL address, without requiring
/// alignment (the fields of the ACPI tables are not aligned).
fn ler<T: Copy>(fisico: u64) -> T {
    unsafe { (fisico_para_virtual(fisico) as *const T).read_unaligned() }
}
```

`read_unaligned` is needed because the ACPI tables are "packed": a 4-byte
field may start at an odd address
([§29](rust-basico.md#29-reprc-packed-and-read_unaligned)).

Every table except the RSDP starts with the same **36-byte header**:

```rust
#[repr(C, packed)]
struct Cabecalho {
    assinatura: [u8; 4],   // "XSDT", "APIC", "FACP"...
    tamanho: u32,          // of the whole table, header included
    revisao: u8,
    checksum: u8,          // adjusted so that the sum of all bytes is 0
    oem: [u8; 6],          // who made the table
    oem_tabela: [u8; 8],
    oem_revisao: u32,
    criador: u32,
    criador_revisao: u32,
}
```

(`cabecalho` = *header*, `assinatura` = *signature*, `tamanho` = *size*.)

The **checksum** protects against reading garbage: adding up all the
table's bytes in a `u8` (letting it overflow), the result must be zero.

```rust
fn checksum_ok(fisico: u64, tamanho: u64) -> bool {
    (0..tamanho).fold(0u8, |soma, i| soma.wrapping_add(ler::<u8>(fisico + i))) == 0
}
```

Finding the MADT means: validate the RSDP, go to the XSDT, and walk the
list of 8-byte addresses after its header until finding a table whose
signature is `"APIC"`:

```rust
let n = (cab.tamanho as u64 - TAMANHO_CABECALHO) / 8;
(0..n)
    .map(|i| ler::<u64>(xsdt + TAMANHO_CABECALHO + i * 8))
    .find(|&t| ler::<[u8; 4]>(t) == *assinatura)
```

### Step 3: what the MADT says

After the header, the MADT has the local APIC address, some *flags* and a
sequence of **variable-size records**, each starting with
`[type: u8][size: u8]`. SiliconOS uses four types:

| Type | Record | What we take from it |
|---|---|---|
| 0 | local APIC (one CPU) | we count the enabled CPUs |
| 1 | I/O APIC | id, physical address, first GSI |
| 2 | IRQ override (*interrupt source override*) | ISA IRQ → GSI, polarity, trigger |
| 5 | 64-bit local APIC address | replaces the 32-bit one |

```rust
while p + 2 <= fim {
    let tipo: u8 = ler(p);
    let tamanho: u8 = ler(p + 1);
    match tipo {
        0 => if ler::<u32>(p + 4) & 1 != 0 { madt.cpus += 1 },
        1 => if madt.ioapic.is_none() { madt.ioapic = Some(IoApic { ... }) },
        2 => if n_desvios < MAX_DESVIOS { madt.desvios[n_desvios] = Some(Desvio { ... }); ... },
        5 => madt.lapic = ler(p + 4),
        _ => {}
    }
    if tamanho < 2 { break; } // corrupted table: avoids an infinite loop
    p += tamanho as u64;
}
```

### GSIs and overrides: IRQ 0 is not on pin 0

The I/O APIC numbers its inputs as **GSIs** (*global system interrupts*):
GSI 0, 1, 2... It would be natural for IRQ *n* to arrive at GSI *n*, and it
almost always does. The exceptions come in the MADT as **overrides**
(`desvios`). On QEMU, the kernel prints:

```
ACPI (MADT): 1 CPU(s), APIC local em 0xfee00000, I/O APIC 0 em 0xfec00000 (GSI a partir de 0), 8259: sim
  desvio: IRQ 0 -> GSI 2 (ativo alto, borda)
  desvio: IRQ 5 -> GSI 5 (ativo alto, nivel)
  desvio: IRQ 9 -> GSI 9 (ativo alto, nivel)
  desvio: IRQ 10 -> GSI 10 (ativo alto, nivel)
  desvio: IRQ 11 -> GSI 11 (ativo alto, nivel)
```

(`a partir de` = *starting at*, `sim` = *yes*, `ativo alto` = *active
high*, `borda` = *edge*, `nivel` = *level*.)

The classic one is the first: on PC chipsets, pin 0 of the I/O APIC is
connected to the output of the 8259, and the old timer (IRQ 0) went to pin
**2**. IRQ 1 (the keyboard) is not on the list, so it arrives at GSI 1 with
the ISA bus's default behavior.

The overrides also say **how the signal behaves**:

- **polarity**: is the interrupt a "1" (active high) or a "0" (active low)?
- **trigger**: by **edge** (the interrupt is the *change* of the signal,
  once per event) or by **level** (the interrupt continues *while* the
  signal is active, until the device is serviced)?

The `rotear_irq` method puts this together:

```rust
pub fn rotear_irq(&self, irq: u8) -> Roteamento {
    let mut r = Roteamento { gsi: irq as u32, ativo_baixo: false, por_nivel: false };
    if let Some(d) = self.desvios.iter().flatten().find(|d| d.irq == irq) {
        r.gsi = d.gsi;
        r.ativo_baixo = d.flags & 0b11 == 0b11;       // 00 = bus default, 01 = high, 11 = low
        r.por_nivel = (d.flags >> 2) & 0b11 == 0b11;  // 00 = bus default, 01 = edge, 11 = level
    }
    r
}
```

ACPI is much bigger than this (it also describes power, sensors,
shutdown...). Here we read only what interrupts need.

---

## 4. Turning off the 8259

Even unused, the 8259 has to be **turned off properly**, for two reasons:

1. In the original PC configuration, the master 8259 delivered IRQs 0–7 at
   vectors **0x08–0x0F**. In protected mode, those vectors are CPU
   **exceptions**: the timer's IRQ 0 would arrive as if it were a *double
   fault* (vector 8). Today's firmware may have configured something else,
   but the kernel should not depend on that.
2. Even masked, the 8259 can raise a **spurious** interrupt (a "ghost"
   interrupt, when a signal disappears halfway). It arrives as the chip's
   IRQ 7 (or 15).

The solution has two steps: **remap** the 16 vectors to 0x20–0x2F (which
collide with nothing) and **mask** all lines. Programming the 8259 is a
sequence of 4 bytes called ICWs (*initialization command words*), sent
through I/O ports 0x20/0x21 (master) and 0xA0/0xA1 (slave):

```rust
// src/interrupcoes/pic.rs
outb(PIC1_COMANDO, 0x11); esperar();          // ICW1: initialize, an ICW4 will follow
outb(PIC2_COMANDO, 0x11); esperar();
outb(PIC1_DADOS, VETOR_BASE); esperar();      // ICW2: vector of IRQ 0  -> 0x20
outb(PIC2_DADOS, VETOR_BASE + 8); esperar();  //       vector of IRQ 8  -> 0x28
outb(PIC1_DADOS, 1 << 2); esperar();          // ICW3: the slave is on the master's pin 2
outb(PIC2_DADOS, 2); esperar();               //       the slave is number 2
outb(PIC1_DADOS, 0x01); esperar();            // ICW4: 8086 mode
outb(PIC2_DADOS, 0x01); esperar();

// Mask 0xFF = all 8 lines of each chip turned off.
outb(PIC1_DADOS, 0xFF);
outb(PIC2_DADOS, 0xFF);
```

`esperar()` ("wait") writes to port 0x80, which nobody uses, just to spend
about 1 µs: the original 8259 needed a pause between commands. The IDT
gets a handler for vectors 0x27 and 0x2F that only counts the spurious
interrupts and does nothing else.

The output shows that the firmware (OVMF) had already masked everything:

```
PIC 8259: mascaras do firmware 0xff 0xff -> remapeado para 0x20-0x2f e todo mascarado
```

(*the firmware's masks 0xff 0xff -> remapped to 0x20-0x2f and fully masked*)

---

## 5. The local APIC

### Where it is: an MSR

Besides the MADT, the CPU itself says where its local APIC is, in an
**MSR** (*model-specific register*). MSRs are hundreds of configuration
registers identified by **number**, read with the `rdmsr` instruction: the
number goes in ECX and the 64-bit value comes back split into EDX (high)
and EAX (low).

```rust
// src/cpu.rs
pub fn ler_msr(numero: u32) -> u64 {
    let (alto, baixo): (u32, u32);
    unsafe {
        asm!("rdmsr", in("ecx") numero, out("edx") alto, out("eax") baixo,
             options(nomem, nostack, preserves_flags));
    }
    (alto as u64) << 32 | baixo as u64
}
```

MSR `0x1B` (`IA32_APIC_BASE`) has the local APIC's physical address in
bits 12 and up, and bit 11 says whether it is turned on. `apic::init`
checks that the MSR and the MADT agree:

```rust
let msr = cpu::ler_msr(IA32_APIC_BASE);
let fisico = msr & APIC_BASE_ENDERECO;
assert!(msr & APIC_BASE_LIGADO != 0, "APIC local desligado no MSR");
assert_eq!(fisico, fisico_madt, "MSR e MADT discordam sobre o endereco do APIC local");
```

### Registers in memory: uncached MMIO

The local APIC's registers are not accessed with special instructions:
they are **MMIO** (doc 05, glossary), a 4 KiB page in which each 32-bit
register takes a position aligned to 16 bytes. Writing to `base + 0xB0` is
giving the APIC an order.

That address (`0xFEE0_0000`) is not RAM, so it was **not** mapped in doc 06.
SiliconOS gets a virtual range just for devices, next to the heap:

```
0xFFFF_8000_0000_0000   all physical RAM (doc 06)          PML4 256
0xFFFF_9000_0000_0000   heap (doc 07)                      PML4 288
0xFFFF_A000_0000_0000   MMIO: local APIC, I/O APIC...      PML4 320   ◄── new
```

```rust
// src/memoria/paginacao.rs
pub fn mapear_mmio(fisico: u64, tamanho: u64) -> Result<u64, ErroMapeamento> {
    let pagina = fisico & !(TAMANHO_QUADRO - 1);
    let deslocamento = fisico - pagina;
    let paginas = (deslocamento + tamanho).div_ceil(TAMANHO_QUADRO);
    let virt = PROXIMO_MMIO.fetch_add(paginas * TAMANHO_QUADRO, Ordering::Relaxed);
    for i in 0..paginas {
        mapear(virt + i * TAMANHO_QUADRO, pagina + i * TAMANHO_QUADRO, TamanhoPagina::K4,
               ESCRITA | SEM_CACHE | SEM_EXECUCAO)?;
    }
    Ok(virt + deslocamento)
}
```

The important bit is **`SEM_CACHE`** ("no cache"). With caching, a write
to the EOI register could stay parked in the CPU cache and never reach the
APIC, and a read of the timer counter could return an old value. For a
device, every read and every write must **really happen**. For the same
reason, every access uses `read_volatile`/`write_volatile` (doc 01), which
stops the compiler from merging, delaying or removing accesses:

```rust
pub fn escrever(registrador: u32, valor: u32) {
    let base = BASE.load(Ordering::Relaxed);
    unsafe { ((base + registrador as u64) as *mut u32).write_volatile(valor) };
}
```

### Turning it on

```rust
// Priority 0: accept all interrupts.
escrever(REG_PRIORIDADE, 0);
// LINT0 is the pin where the old 8259 would deliver IRQs: no longer used.
escrever(REG_LVT_LINT0, LVT_MASCARADA);
// Turns the APIC on (bit 8) and sets the vector for spurious interrupts.
escrever(REG_ESPURIO, SVR_APIC_LIGADO | vetor_espurio as u32);
```

Three registers:

- **TPR** (`0x80`, *task priority*): the APIC only delivers interrupts with
  priority **above** this value. Zero = all of them.
- **LVT LINT0** (`0x350`): the *local vector table* has one entry for each
  **local** interrupt source (the timer, pins LINT0 and LINT1, errors...).
  LINT0 is where the 8259 would come in; we mask it.
- **SVR** (`0xF0`, *spurious vector register*): bit 8 **turns the APIC on**,
  and bits 0–7 say which vector to deliver a spurious interrupt on (we use
  0xFF).

```
APIC local: id 0, versao 0x14, 6 entradas LVT; fisico 0xfee00000 -> virtual 0xffffa00000000000
```

(`versao` = *version*, `entradas` = *entries*, `fisico` = *physical*.)

### EOI: "I'm done"

When the APIC delivers an interrupt, it sets a bit for it in an internal
register (the ISR, *in-service register*). While that bit is set, the APIC
**does not deliver** another interrupt of the same **priority class** or a
lower one. The class is the high hexadecimal digit of the vector: 0x30 and
0x31 are both class 3.

The handler clears the bit by writing any value to the EOI register:

```rust
pub fn eoi() {
    escrever(REG_EOI, 0);
}
```

Forgetting the EOI is one of the most common mistakes in OSDev, and the
`teste-sem-eoi` test (section 11.3) shows the result: **everything stops**.

---

## 6. The I/O APIC

The I/O APIC has a curious interface: only **two** MMIO registers. One is
the **selector** (`IOREGSEL`, at offset 0x00); the other is the **window**
(`IOWIN`, at 0x10). To read internal register N, you write N to the
selector and read the window:

```rust
// src/interrupcoes/ioapic.rs
fn ler(registrador: u32) -> u32 {
    let base = BASE.load(Ordering::Relaxed);
    unsafe {
        ((base + IOREGSEL) as *mut u32).write_volatile(registrador);
        ((base + IOWIN) as *const u32).read_volatile()
    }
}
```

Register 1 (version) says, in bits 16–23, how many pins the chip has. Each
pin has a 64-bit **redirection entry**, split into two 32-bit registers
(`0x10 + 2n` and `0x10 + 2n + 1`):

```
 63      56 55                17 16  15  14  13  12  11  10  8  7        0
┌──────────┬─────────────────────┬───┬───┬───┬───┬───┬───┬─────┬──────────┐
│destination│     reserved       │ M │ L │   │ A │   │ D │mode │  vector  │
│ (APIC id)│                     │   │   │   │   │   │   │     │          │
└──────────┴─────────────────────┴───┴───┴───┴───┴───┴───┴─────┴──────────┘
 M = masked   L = level-triggered   A = active low
 D = logical destination (0 = physical: "destination" is an APIC id)
 mode 000 = fixed (normal delivery)
```

`init` masks all 24 pins. Then each device is enabled with `rotear`
("route"):

```rust
pub fn rotear(gsi: u32, vetor: u8, destino: u8, ativo_baixo: bool, por_nivel: bool) {
    let pino = gsi - GSI_BASE.load(Ordering::Relaxed) as u32;
    let mut baixo = vetor as u32; // fixed mode, physical destination, not masked
    if ativo_baixo { baixo |= ATIVO_BAIXO; }
    if por_nivel { baixo |= POR_NIVEL; }
    // First the high half (destination), then the low one (which unmasks).
    escrever(REG_REDIRECIONAMENTO + 2 * pino + 1, (destino as u32) << 24);
    escrever(REG_REDIRECIONAMENTO + 2 * pino, baixo);
}
```

The order of the two writes matters: the low half has the mask bit. If it
were written first, the pin would be enabled for an instant with the old
destination.

For the keyboard, the entry becomes `0x0000000000000031`: vector 0x31,
destination APIC 0, active high, edge, not masked.

```
I/O APIC: id 0, versao 0x20, 24 pinos; virtual 0xffffa00000001000
```

(`pinos` = *pins*.)

---

## 7. The timer

### Which timer?

A PC has several clocks. The three that appear in this chapter:

| Clock | Since | What we use it for |
|---|---|---|
| **PIT** 8253/8254 | 1981 | reference stopwatch: fixed, known frequency |
| **local APIC timer** | 1990s | the kernel's tick: 100 interrupts per second |
| **RTC** (CMOS clock) | 1984 | checking the timer in the test (it counts real seconds, on a battery) |

The APIC timer is the one chosen for the tick: it is inside the CPU (each
CPU has its own, which will matter with several CPUs) and does not depend
on the I/O APIC. Its problem: it counts at a frequency that **varies from
machine to machine** (the bus clock), and no register says what it is.

### Calibrating: timing it with the PIT

The solution is to measure. The PIT counts at a fixed frequency of
**1,193,182 Hz** (the original PC's 14.318 MHz crystal divided by 12). We
program the PIT's **channel 2** to count 10 ms (11,931 pulses), start the
APIC timer at the same time counting down from the maximum, and see how
far it went:

```rust
// src/interrupcoes/timer.rs
// Channel 2 gate on, speaker off.
outb(PORTA_61, (inb(PORTA_61) & !0b10) | 0b01);
// Channel 2, low byte then high byte, mode 0 ("interrupt on terminal
// count": the output stays at 0 and goes to 1 when the count reaches 0).
outb(PIT_COMANDO, 0b10_11_000_0);
outb(PIT_CANAL2, contagem_pit as u8);
// From this write on the PIT starts counting. We start the APIC along with it.
apic::escrever(apic::REG_TIMER_DIVISOR, 0b0011); // divides the clock by 16
apic::escrever(apic::REG_LVT_TIMER, apic::LVT_MASCARADA); // count without interrupting
outb(PIT_CANAL2, (contagem_pit >> 8) as u8);
apic::escrever(apic::REG_TIMER_INICIAL, u32::MAX);

// Waits for channel 2's output to go up (10 ms later).
while inb(PORTA_61) & 0x20 == 0 {
    core::hint::spin_loop();
}
```

Why channel 2? The PIT's channels 0 and 1 are wired, respectively, to IRQ
0 and to the old memory refresh. Channel 2 is the one for the PC
**speaker** (the "beep"), and it has an advantage: its output can be
**read** in bit 5 of port 0x61. It can be used as a stopwatch without any
interrupt, just by watching that bit. (Bit 1 of the same port would turn
the speaker on; the code leaves it at 0 so the PC does not beep.)

The command byte `0b10_11_000_0` is read in parts: `10` = channel 2, `11` =
we will send the low byte and then the high one, `000` = mode 0, `0` =
count in binary.

On QEMU:

```
Timer do APIC: 624954 contagens em 10 ms = 62 MHz (barramento de ~999 MHz / 16)
  modo periodico: 100 Hz no vetor 0x30
```

(*APIC timer: 624954 counts in 10 ms = 62 MHz (bus of ~999 MHz / 16);
periodic mode: 100 Hz on vector 0x30*)

The APIC timer went ~625,000 in 10 ms: 62.5 million counts per second, with
divider 16. The input clock is therefore ~1 GHz, which is the value QEMU
emulates. On a real machine the number would be different, and that is
exactly why it is calibrated.

### Periodic mode

With the measurement, the timer is programmed to reload by itself and
interrupt 100 times per second:

```rust
pub fn iniciar(vetor: u8, contagens_10ms: u32) {
    const PERIODICO: u32 = 1 << 17; // bits 17-18 of the timer LVT: 01 = periodic
    let por_tique = contagens_10ms as u64 * 100 / FREQUENCIA_HZ;
    apic::escrever(apic::REG_TIMER_DIVISOR, 0b0011);
    apic::escrever(apic::REG_LVT_TIMER, PERIODICO | vetor as u32);
    apic::escrever(apic::REG_TIMER_INICIAL, por_tique as u32);
}
```

And the handler is as small as possible:

```rust
pub extern "x86-interrupt" fn tratador(_quadro: QuadroInterrupcao) {
    let t = TIQUES.fetch_add(1, Ordering::Relaxed) + 1;
    // ... (pieces used only by test features)
    apic::eoi();
}
```

`TIQUES` is an `AtomicU64` ([§25](rust-basico.md#25-atomics-atomicu64-atomicbool)).
At 100 Hz, it takes 5.8 billion years to overflow. With it, the kernel now
has `timer::tiques()` and `timer::milissegundos()`.

> Why 100 Hz and not 1000? Each interrupt costs a few hundred instructions
> (push, save registers, EOI, return). 100 Hz is the classic value of old
> Linux and more than enough for now. When there is process switching, this
> number becomes a design decision: higher = faster response, lower = less
> time spent on interrupts.

---

## 8. The keyboard

### From the finger to port 0x60

The PS/2 keyboard (the round purple connector, which QEMU emulates) talks to
a **controller** on the motherboard, the "8042". Every time a key is
pressed **or released**, the controller:

1. places a byte, the **scancode**, on I/O port **0x60**;
2. sets bit 0 of status port 0x64 ("data waiting");
3. raises **IRQ 1**.

The scancode says **which key** (the physical position), not which letter.
The same key is "A" on an American keyboard and "Q" on a French one.
Translating is the operating system's job.

### Configuring the controller

The controller has a **configuration byte** that says, among other things,
whether it should raise IRQ 1. `teclado::init` reads that byte (command
0x20), sets the bits that matter and writes it back (command 0x60):

```rust
const CONFIG_IRQ1: u8 = 1 << 0;      // raise IRQ 1 when a byte arrives from the keyboard
const CONFIG_TRADUCAO: u8 = 1 << 6;  // translate to "scancode set 1"

enviar_comando(0x20); // "read configuration byte"
let antes = ler_dado();
let depois = antes | CONFIG_IRQ1 | CONFIG_TRADUCAO;
enviar_comando(0x60); // "write configuration byte"
enviar_dado(depois);
```

Before that, it throws away any bytes left over from the firmware (a key
pressed during boot would sit in the queue and confuse the start).

```
Teclado PS/2: config 0x67 -> 0x67, 0 bytes velhos descartados
  IRQ 1 -> GSI 1 -> vetor 0x31; entrada do I/O APIC: 0x0000000000000031
```

(*PS/2 keyboard: config 0x67 -> 0x67, 0 old bytes discarded; IRQ 1 -> GSI 1
-> vector 0x31; I/O APIC entry: ...*)

OVMF had already left `0x67` = `0110 0111`: bit 0 (IRQ 1) and bit 6
(translation) already set, plus bit 1 (IRQ 12, for the mouse), bit 2 (the
system test passed) and bit 5 (mouse port clock disabled). `init` sets the
bits anyway: on another machine they could be off.

### Scancodes: press, release and the 0xE0 prefix

With translation on, scancodes follow **set 1** (the original PC XT's; the
keyboard sends set 2, and the controller translates):

- pressing a key produces its code (*make code*): A = `0x1E`;
- releasing produces the same code with bit 7 set (*break code*): `0x9E`;
- keys that came later (arrows, right Ctrl, *Home*...) first send a
  **`0xE0`** byte.

With `./run.sh teste-teclado`, the kernel shows every byte. Shift + A and
then the right arrow:

```
  scancode 0x2a       left Shift pressed
  scancode 0x1e       A pressed             -> 'A'
A  scancode 0x9e      A released (0x1E | 0x80)
  scancode 0xaa       Shift released (0x2A | 0x80)
  scancode 0xe0       extended prefix
  scancode 0x4d       right arrow pressed
  scancode 0xe0
  scancode 0xcd       right arrow released
```

(The explanations on the right were added here; the "A" at the start of a
line is the character's echo.)

### The handler: just store it

An interrupt handler must be **short**: while it runs, interrupts are off
(it is an *interrupt gate*, doc 04) and the timer waits. That is why the
keyboard handler neither translates nor prints anything. It reads the byte
and puts it in a **queue**:

```rust
pub extern "x86-interrupt" fn tratador(_quadro: QuadroInterrupcao) {
    let scancode = unsafe { inb(PORTA_DADOS) };
    RECEBIDOS.fetch_add(1, Ordering::Relaxed);

    let e = ESCRITA.load(Ordering::Relaxed);
    let proxima = (e + 1) % TAMANHO_FILA;
    if proxima == LEITURA.load(Ordering::Acquire) {
        PERDIDOS.fetch_add(1, Ordering::Relaxed); // full: discard
    } else {
        FILA[e].store(scancode, Ordering::Relaxed);
        // Release: whoever reads the new ESCRITA also sees the byte written above.
        ESCRITA.store(proxima, Ordering::Release);
    }
    apic::eoi();
}
```

(`recebidos` = *received*, `perdidos` = *lost*, `proxima` = *next*.)

Reading port 0x60 is mandatory: until the byte is read, the controller
does not send the next one.

The queue is a 64-byte **circular buffer** with two positions: `ESCRITA`
(only the handler changes it) and `LEITURA` (only the main loop changes
it). Empty when the two are equal; full when the next write would catch up
with the read position:

```
 FILA:  [ . . . 1E 9E 2A 1E . . . . ]
                 ▲           ▲
              LEITURA     ESCRITA
             (next read)  (next write)
```

It **uses no lock**. That is possible because there is exactly one producer
and one consumer, and each position is changed by only one side. A `Mutex`
here would be an invitation to the deadlock in section 10. The
`Release`/`Acquire` pair guarantees that whoever sees the new position also
sees the byte written before it
([§28](rust-basico.md#28-orderingacquire-and-release)).

### The main loop: take, translate and show

```rust
// src/main.rs
fn ecoar_teclado() -> ! {
    use interrupcoes::teclado;
    let mut decodificador = teclado::Decodificador::new();
    loop {
        while let Some(scancode) = teclado::proximo_scancode() {
            if let Some(c) = decodificador.processar(scancode) {
                kprint!("{}", c);
            }
        }
        teclado::esperar();
    }
}
```

The `Decodificador` ("decoder") remembers the state of the Shift keys and
Caps Lock and translates with two tables indexed by the scancode, one
without and one with Shift:

```rust
const NORMAL: &[u8; 58] =
    b"\0\x1b1234567890-=\x08\tqwertyuiop[]\n\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
const COM_SHIFT: &[u8; 58] =
    b"\0\x1b!@#$%^&*()_+\x08\tQWERTYUIOP{}\n\0ASDFGHJKL:\"~\0|ZXCVBNM<>?\0*\0 ";
```

Position 0x1E of the first table is `a`; 0x02 is `1`, and in the second
one, `!`. The type `&[u8; 58]` makes the compiler check that both have
exactly 58 bytes (from 0x00 to 0x39, the space bar).

The layout is **US**, which is what QEMU emulates by default. On other
physical layouts some keys would come out swapped. Besides, the console
font only has ASCII. Other layouts are for later.

The console got the *backspace* character (`0x08`): it goes back one column
and draws a space over it. On the serial port, the kernel sends "back,
space, back", which is how terminals erase.

### Sleeping without losing a key: `sti; hlt`

`teclado::esperar` puts the CPU to sleep (`hlt`) until the next interrupt,
but it has a subtle detail:

```rust
pub fn esperar() {
    unsafe {
        core::arch::asm!("cli", options(nostack));
        if fila_vazia() {
            core::arch::asm!("sti; hlt", options(nostack));
        } else {
            core::arch::asm!("sti", options(nostack));
        }
    }
}
```

If it were just `if fila_vazia() { hlt }` ("queue empty"), a key could
arrive **between** the check and the `hlt`: the handler puts the byte in the
queue, returns, and the CPU goes to sleep with a key waiting. It would only
wake up at the next timer tick, and if there were no timer, never.

The solution uses a CPU rule: after an `sti`, interrupts are only accepted
**after the following instruction**. So `sti; hlt` is atomic: with
interrupts off, it checks the queue; if it is empty, it turns interrupts on
and goes to sleep **in one go**. A key that arrives in the meantime wakes
the `hlt`.

---

## 9. Putting it all together

The IDT gets the new entries (doc 04):

```rust
// src/idt.rs
use crate::interrupcoes::{self as int, teclado, timer};
t[int::VETOR_TIMER as usize] = Entrada::new(timer::tratador as *const () as u64, 0);
t[int::VETOR_TECLADO as usize] = Entrada::new(teclado::tratador as *const () as u64, 0);
for v in [int::VETOR_PIC_ESPURIO_1, int::VETOR_PIC_ESPURIO_2, int::VETOR_APIC_ESPURIO] {
    t[v as usize] = Entrada::new(int::espuria as *const () as u64, 0);
}
```

And `interrupcoes::init`, called in `kernel_main` after the heap, does
everything in order:

```
interrupcoes::init()
  1. acpi::madt()           where the controllers are, IRQ overrides
  2. pic::desligar()        8259 remapped to 0x20 and masked
  3. apic::init()           maps 0xFEE0_0000 uncached, turns the local APIC on
  4. ioapic::init()         maps 0xFEC0_0000, masks the 24 pins
  5. timer::calibrar()      10 ms of the PIT  →  how many APIC counts
     timer::iniciar()       periodic mode, 100 Hz, vector 0x30
  6. teclado::init()        IRQ 1 enabled in the PS/2 controller
     ioapic::rotear()       GSI 1 → vector 0x31 → APIC 0
  7. sti                    from here on, interrupts arrive
```

The order is not arbitrary: `sti` comes **last**, when every vector that can
arrive already has a handler and every controller is in a known state.

---

## 10. The new danger: deadlock

Until now, the kernel's code ran from start to finish without being
interrupted. Now, **between any two instructions**, a handler may run. That
creates a problem that did not exist.

`kprintln!` uses `CONSOLE`, protected by a spinlock (`spin::Mutex`,
[§11](rust-basico.md#11-mutex-and-once-from-the-spin-crate)). Imagine a
handler that also prints:

```
 main loop                               timer handler
 ─────────                               ─────────────
 kprintln!("line 14")
   CONSOLE.lock()      ← takes the lock
   drawing a letter...
        ┊  ◄────── timer interrupt ──────
        ┊                                 kprint!("[tique 10]")
        ┊                                   CONSOLE.lock()  ← taken!
        ┊                                   waits... waits... waits...
        ┊
   (never runs again: the CPU is stuck in the handler,
    and the handler waits for a lock only the main loop releases)
```

In an application with threads, the lock's "owner" would keep running on
another thread and would eventually release it. Not here: the owner was
**interrupted**, and only comes back when the handler finishes. The
handler never finishes. **Deadlock.**

### The protection: interrupts off while the lock is held

The solution is simple: while `_print` holds the lock, **no interrupt gets
in**. If a handler wants to print, it waits for the current `kprintln!` to
finish, not the other way round.

```rust
// src/cpu.rs
pub fn sem_interrupcoes<R>(f: impl FnOnce() -> R) -> R {
    let estavam_ligadas = interrupcoes_ligadas();
    if estavam_ligadas {
        desligar_interrupcoes();
    }
    let resultado = f();
    if estavam_ligadas {
        ligar_interrupcoes();
    }
    resultado
}
```

```rust
// src/main.rs
pub fn _print(args: core::fmt::Arguments) {
    #[cfg(not(feature = "teste-deadlock"))]
    cpu::sem_interrupcoes(|| escrever(args));
    #[cfg(feature = "teste-deadlock")]
    escrever(args);
}
```

Two details:

- `sem_interrupcoes` **restores** the previous state instead of always
  turning interrupts on. Inside a handler interrupts are already off, and
  an `sti` in the middle of it would let an interrupt interrupt the handler
  itself.
- The heap has the same risk (a handler allocating while the main loop is
  in the middle of a `Vec::push`), so doc 07's `alloc` and `dealloc` now
  also use `sem_interrupcoes`.

The keyboard handler does not even need this, because it uses no lock at
all (the queue in section 8). That is the general rule: **handlers should
do the minimum** and leave the rest to normal code.

### `cli` and `sti` are also barriers for the compiler

The compiler may reorder memory accesses when it thinks that does not
change the result. If it thinks `sti` does not touch memory, it may move
the lock release (a memory write) to **after** the `sti`, and the problem
comes back. That is why the functions in `cpu.rs` do **not** use
`options(nomem)` in `asm!` ([§15](rust-basico.md#15-asm-assembly-inside-rust)):

```rust
// No `options(nomem)` on purpose: this way the compiler treats `cli` and
// `sti` as if they touched any memory and does NOT move reads and writes
// (such as releasing a lock) to the other side of them.
pub fn ligar_interrupcoes() {
    unsafe { asm!("sti", options(nostack)) };
}
```

---

## 11. How it was tested

### 11.1 The normal boot

```
ACPI (MADT): 1 CPU(s), APIC local em 0xfee00000, I/O APIC 0 em 0xfec00000 (GSI a partir de 0), 8259: sim
  desvio: IRQ 0 -> GSI 2 (ativo alto, borda)
  ...
PIC 8259: mascaras do firmware 0xff 0xff -> remapeado para 0x20-0x2f e todo mascarado
APIC local: id 0, versao 0x14, 6 entradas LVT; fisico 0xfee00000 -> virtual 0xffffa00000000000
I/O APIC: id 0, versao 0x20, 24 pinos; virtual 0xffffa00000001000
Timer do APIC: 624954 contagens em 10 ms = 62 MHz (barramento de ~999 MHz / 16)
  modo periodico: 100 Hz no vetor 0x30
Teclado PS/2: config 0x67 -> 0x67, 0 bytes velhos descartados
  IRQ 1 -> GSI 1 -> vetor 0x31; entrada do I/O APIC: 0x0000000000000031
Interrupcoes LIGADAS (sti).

Proxima etapa: shell e testes (capitulo 10).
Digite algo (teclado PS/2, layout americano):
```

(The last lines: *interrupts ON (sti). Next step: shell and tests (chapter
10). Type something (PS/2 keyboard, US layout):*)

To test the keyboard with nobody typing, QEMU was started with its
**monitor** on a socket (`-monitor unix:...`) and keys were sent with the
monitor's `sendkey` command: `o l a spc shift-m u n d o shift-1 ret a b c x
backspace backspace d caps_lock t e s t e caps_lock spc shift-2 ret`. On
the serial port and on screen:

```
ola Mundo!
abdTESTE @
```

Shift, Caps Lock (which only affects letters: the `@` still depended on
Shift) and the two *backspaces* erasing `x` and `c` worked. In QEMU's
normal window, just click on it and type.

### 11.2 `./run.sh teste-timer`: the timer against the CMOS clock

The RTC is an independent clock, with its own crystal and battery, that
counts real seconds. The test waits for the RTC's second to change and
counts how many ticks happen during each of the next 5 seconds:

```
Teste timer: contando tiques a cada segundo do RTC (esperado: 100)
  segundo 39 do RTC: 100 tiques
  segundo 40 do RTC: 100 tiques
  segundo 41 do RTC: 100 tiques
  segundo 42 do RTC: 100 tiques
  segundo 43 do RTC: 100 tiques
Teste timer OK: media de 100 tiques/s; 5720 ms desde que o timer ligou; espurias: 0
```

(*counting ticks every RTC second (expected: 100) ... average of 100
ticks/s; 5720 ms since the timer was turned on; spurious: 0*)

The PIT calibration matched the RTC. The test accepts up to a 10%
difference, because QEMU without KVM sometimes falls behind: in another
run, one of the seconds had 89 ticks (average 98), and with `RAM=4G` the
average was 97. No spurious interrupts.

### 11.3 `./run.sh teste-sem-eoi`: forgetting the EOI

The timer handler is built without the `apic::eoi()` line. The test waits
2 seconds by the RTC (without `hlt`: if the timer stops, nothing else would
wake the CPU):

```
Teste: tratador do timer SEM EOI. Esperando 2 s pelo RTC...
  tiques depois de 2 s: 1 (com EOI seriam ~200)
  teclas recebidas: 0 (o teclado tambem ficou bloqueado?)
```

(*timer handler WITHOUT EOI. Waiting 2 s by the RTC... ticks after 2 s: 1
(with EOI they would be ~200); keys received: 0 (was the keyboard blocked
too?)*)

**One** tick, and nothing more. And the answer to the question is **yes**:
with `sendkey` sending `o l a ret` after that, nothing showed up on screen.
The keyboard vector (0x31) is in the same priority class as the timer
(0x30), and the APIC, thinking interrupt 0x30 is still being handled, holds
back both.

### 11.4 `./run.sh teste-print-interrupcao`: printing inside the handler

The main loop prints lines non-stop, and the timer handler prints
`[tique N]` every 10 ticks. With `_print`'s protection:

```
[tique 10]laco principal: linha 16 (tiques: 10)
[tique 20]laco principal: linha 35 (tiques: 20)
[tique 30]laco principal: linha 54 (tiques: 30)
...
[tique 300]Teste OK: 300 tiques e 561 linhas sem travar.
```

(`laco principal: linha` = *main loop: line*; *300 ticks and 561 lines
without freezing*.)

The `[tique N]` always appears **between** two lines, never in the middle
of one: that is the protection working (the interrupt waits for the
`kprintln!` to finish).

A finding during the test: in the first version, the main loop had no
pause at all between one `kprintln!` and the next, and the timer **stopped
at the third tick**. Interrupts were on for only a few instructions between
one `kprintln!` and the next. The same binary running with KVM
(`qemu-system-x86_64 -enable-kvm`, the real CPU executing the code) reached
300 ticks with no pause at all. What made the difference was QEMU without
KVM (TCG mode, which translates the instructions): it checks for pending
interrupts at specific points (between blocks of translated instructions),
and such a short window was always missed. A pause of 1000 `spin_loop`
iterations with interrupts on solved it. The lesson: **the emulator is not
the hardware**, and a timing-dependent test may behave differently on it.

### 11.5 `./run.sh teste-deadlock`: without the protection

The same test, but built without `sem_interrupcoes` in `_print`. The serial
output ends like this, and QEMU stays on, but frozen:

```
laco principal: linha 13 (tiques: 9)
laco principal: linha 14 (tiques: 9)
[tique 10]
```

`[tique 10]` appears because the serial port is written **before** taking
the CONSOLE lock. Right after that, the handler tries to take the lock,
which the main loop (interrupted in the middle of line 15) is holding. The
QEMU monitor confirms it:

```
(qemu) info registers
RIP=000000000605ce12 RFL=00000002 ...
(qemu) x /24xb 0x605ce00
...
000000000605ce10: 0xf3 0x90 0xc3 0xcc ...
```

- `RFL=00000002`: the IF bit (bit 9) is **off**, so the CPU is inside a
  handler (the *interrupt gate* turned interrupts off);
- checked several times, RIP is always the same, and the bytes there are
  `f3 90` (`pause`) and `c3` (`ret`): the `core::hint::spin_loop` function,
  which the spinlock calls on every turn while it waits. The CPU is
  spinning forever on a lock that will never be released.

### 11.6 The earlier tests

All 21 builds (the 17 features from the earlier chapters, the normal boot
and the new `teste-timer`, `teste-sem-eoi` and `teste-print-interrupcao`)
were run again, and all gave the expected result. `teste-deadlock` freezes,
as it should.

---

## Changes to the files

What this chapter adds to the code of doc 07 (tag `doc-07`). Each file's link
leads to its state at the end of this chapter (tag `doc-08`); every changed line
is in the [comparison between the two tags](https://github.com/moises-dev-git/siliconOS/compare/doc-07...doc-08).

| File | What changes | Where |
|---|---|---|
| [`src/cpu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | **new**: `ligar_interrupcoes`, `desligar_interrupcoes`, `interrupcoes_ligadas`, `sem_interrupcoes`, `ler_msr` | — |
| [`src/acpi.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | **new**: `guardar_rsdp`, `Cabecalho`, `Madt` (with `rotear_irq`), `madt`, `procurar_tabela`, `ler_madt` | — |
| [`src/interrupcoes/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/mod.rs) | **new**: the vectors, `espuria`, `init` (the ACPI → 8259 → APIC → I/O APIC → timer → keyboard → `sti` sequence) | — |
| [`src/interrupcoes/pic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/pic.rs) | **new**: `desligar` | — |
| [`src/interrupcoes/apic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | **new**: registers, `ler`/`escrever`, `eoi`, `id`, `init` | — |
| [`src/interrupcoes/ioapic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | **new**: `init`, `rotear`, `entrada` | — |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | **new**: `TIQUES`, `tratador`, `calibrar` (PIT), `iniciar`, `segundos_rtc` | — |
| [`src/interrupcoes/teclado.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | **new**: the lock-free queue, `tratador`, `proximo_scancode`, `esperar`, `init` (8042), `Decodificador` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | `mod acpi; mod cpu;` and `#[allow(dead_code)] mod interrupcoes;` | with the other `mod`s |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | `_print` becomes `cpu::sem_interrupcoes(\|\| escrever(args))`; the old body becomes `fn escrever` (with `#[cfg]` for the `teste-deadlock` feature) | `fn _print` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | `acpi::guardar_rsdp();` | in `main()`, before `exit_boot_services` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | in `kernel_main`: `interrupcoes::init(); testes_de_interrupcoes();`; the final `loop { hlt }` is replaced by `ecoar_teclado()`; new functions `ecoar_teclado` and `testes_de_interrupcoes` | after `testes_de_heap()` |
| [`src/idt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/idt.rs) | entries for vectors 0x30 (timer), 0x31 (keyboard) and the spurious ones (0x27, 0x2F, 0xFF) | in `init`, after the `PAGE_FAULT` line |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/memoria/paginacao.rs) | `MMIO_INICIO`, `PROXIMO_MMIO` and `mapear_mmio` | before `alocar_e_mapear` |
| [`src/memoria/heap.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/memoria/heap.rs) | the bodies of `alloc` and `dealloc` inside `crate::cpu::sem_interrupcoes(...)` | `impl GlobalAlloc for AlocadorLista` |
| [`src/console.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/console.rs) | `0x08` (backspace) arm in the `match` | `write_byte` |
| [`src/serial.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/serial.rs) | `match` on `\n` (sends `\r\n`) and `0x08` (sends back, space, back) | `write_str` |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/Cargo.toml) | `teste-timer`, `teste-teclado`, `teste-sem-eoi`, `teste-print-interrupcao`, `teste-deadlock = ["teste-print-interrupcao"]` | `[features]` |

---

## Functions in this chapter

Every function this chapter adds, in one sentence each. The most important ones were explained in the text above; this table is a map and a quick reference. (Functions that already existed and only changed are listed in [Changes to the files](#changes-to-the-files).)

| Function | File | What it does |
|---|---|---|
| `guardar_rsdp` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Takes the RSDP (ACPI 2.0) address from the UEFI configuration table and stores it; must run before `exit_boot_services`. |
| `ler` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Reads a value of any type at a physical address without requiring alignment (`read_unaligned`). |
| `checksum_ok` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Checks the ACPI checksum: all bytes added up (with wrap-around) give zero. |
| `Madt::rotear_irq` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Which GSI an ISA IRQ goes to, and with which polarity and trigger mode, applying the MADT's overrides. |
| `madt` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Reads the MADT on the first call and always returns the same copy. |
| `procurar_tabela` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Validates the RSDP and the XSDT (signature and checksum) and walks the XSDT looking for the table with the given signature ("APIC", "FACP"...). |
| `ler_madt` | [acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | Parses the MADT entries: local APICs, I/O APICs and IRQ overrides. |
| `ligar_interrupcoes` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | `sti`: the CPU starts accepting hardware interrupts. |
| `desligar_interrupcoes` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | `cli`: hardware interrupts are held pending. |
| `interrupcoes_ligadas` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | Reads the IF bit of RFLAGS. |
| `sem_interrupcoes` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | Runs a closure with interrupts off and restores IF afterwards; protects anything a handler also uses. |
| `ler_msr` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | `rdmsr`: reads a model-specific register (MSR) by number. |
| `ler` | [interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | Reads a 32-bit local APIC register (MMIO at the given offset). |
| `escrever` | [interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | Writes a local APIC register. |
| `eoi` | [interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | Writes 0 to the EOI register: "done". Without it, no interrupt of equal or lower priority arrives. |
| `id` | [interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | This APIC's number (this CPU's), used as the destination in the I/O APIC. |
| `init` | [interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | Reads MSR 0x1B, maps the registers, sets the priority to zero, masks LINT0 (where the 8259 would come in) and enables the APIC with the spurious vector. |
| `ler` | [interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | Reads an I/O APIC register: writes the number to IOREGSEL and reads IOWIN. |
| `escrever` | [interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | Writes an I/O APIC register through the same IOREGSEL/IOWIN pair. |
| `init` | [interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | Maps the I/O APIC, reads how many pins it has and masks them all. |
| `rotear` | [interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | Programs a pin's redirection entry: vector, destination CPU, polarity and trigger mode, unmasked. |
| `entrada` | [interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | A pin's current redirection entry (diagnostics). |
| `espurias` | [interrupcoes/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/mod.rs) | How many spurious interrupts have arrived. |
| `espuria` | [interrupcoes/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/mod.rs) | Spurious interrupt handler (8259 and APIC): only counts, no EOI. |
| `init` | [interrupcoes/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/mod.rs) | The whole sequence: disables the 8259, enables the local APIC and the I/O APIC, calibrates and starts the timer, routes the keyboard and runs `sti`. |
| `esperar` | [interrupcoes/pic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/pic.rs) | Writes to port 0x80 just to spend ~1 µs between 8259 commands. |
| `desligar` | [interrupcoes/pic.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/pic.rs) | Remaps both 8259s to 0x20–0x2F (so they do not collide with exceptions) and masks every IRQ. |
| `recebidos` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | How many scancodes the handler has received. |
| `perdidos` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | How many scancodes were dropped because the queue was full. |
| `tratador` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Vector 0x31 handler: reads the scancode from port 0x60, queues it (or counts it as lost) and sends EOI. |
| `proximo_scancode` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Takes the next scancode from the queue (the consumer side of the SPSC queue). |
| `fila_vazia` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Is the queue empty? (compares the read and write indices). |
| `esperar` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Sleeps with `hlt` if the queue is empty, using `cli; test; sti; hlt` so a key arriving in between is not lost. |
| `enviar_comando` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Waits for the 8042 input buffer to empty and writes a command to port 0x64. |
| `enviar_dado` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Waits for the input buffer to empty and writes a data byte to port 0x60. |
| `ler_dado` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Waits for the output buffer to fill and reads a byte from port 0x60. |
| `init` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Discards stale bytes, reads the 8042 configuration, sets the IRQ 1 bit and writes it back. |
| `Decodificador::new` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | New decoder: Shift and Caps Lock off. |
| `Decodificador::processar` | [interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | Translates a set-1 scancode into a character (US layout), tracking Shift, Caps Lock and the 0xE0 prefix. |
| `tiques` | [interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | How many ticks (of 10 ms) since the timer was started. |
| `milissegundos` | [interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | Ticks × 10. |
| `tratador` | [interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | Vector 0x30 handler: adds a tick, sends EOI (and, since doc 10, kills a ring-3 program that went over the limit). |
| `calibrar` | [interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | Measures how much the APIC timer counts in 10 ms, using PIT channel 2 as a stopwatch. |
| `iniciar` | [interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | Starts the APIC timer in periodic mode on the given vector with the calibrated count. |
| `segundos_rtc` | [interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | Reads the seconds of the CMOS clock (RTC) by polling, to check the timer. |
| `escrever` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | The part of `_print` that actually writes (serial and screen), running inside `sem_interrupcoes`. |
| `ecoar_teclado` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | The doc 08 main loop: sleeps, and each key that arrives is translated and shown (doc 09 replaces it with the shell). |
| `testes_de_interrupcoes` | [main.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | Tests chosen by the feature (`teste-timer`, `teste-sem-eoi`, `teste-print-interrupcao`, `teste-deadlock`). |
| `mapear_mmio` | [memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/memoria/paginacao.rs) | Maps device registers in the MMIO_INICIO range, with WRITE, NO_CACHE and NO_EXECUTE, and returns the virtual address. |


---

## Difference from the book

Chapter 9 of the book covers the same topic: having the hardware notify
the kernel through interrupts, with the timer and the keyboard as examples.
OSDev tutorials usually start with the 8259, which is simpler to program
(the timer on IRQ 0 and the keyboard on IRQ 1 arrive directly, without
ACPI). It is worth comparing as you read.

SiliconOS's particularities:

- it goes straight to the **local APIC + I/O APIC**, which is what a modern
  system uses (and what will allow using several CPUs), and turns off the
  8259;
- it finds the addresses and the IRQ overrides by reading the **MADT** by
  hand, without the `acpi` crate;
- it uses no ready-made crates for the PIC or for translating scancodes:
  the registers and tables are in the code, with named bits;
- it calibrates the APIC timer with the **PIT** and checks the result
  against the **RTC**.

---

## Summary

- A **hardware interrupt** (IRQ) is the hardware's *callback*: the device
  signals, the CPU interrupts what it is doing, runs the IDT handler and
  comes back.
- The PC has the **8259** (1981, one CPU, 15 IRQs) and the **APIC** (one
  local APIC per CPU, with a timer, and an I/O APIC for the devices).
  SiliconOS remaps the 8259 to 0x20–0x2F, masks everything and uses the
  APIC.
- The addresses come from the **ACPI** tables: UEFI → RSDP → XSDT →
  **MADT**. The MADT also lists the IRQ **overrides** (on QEMU, IRQ 0 goes to
  GSI 2).
- The APIC's registers are **MMIO**, mapped in a range of their own
  (`0xFFFF_A000_...`) with **SEM_CACHE**.
- Every handler ends with an **EOI**; without it, the APIC blocks that
  priority class (the timer stopped at the 1st tick, and the keyboard with
  it).
- The **APIC timer** was calibrated with the **PIT** (10 ms) and ticks 100
  times per second; the RTC confirmed it: 100 ticks per second.
- The **keyboard** sends scancodes through port 0x60. The handler only puts
  them in a **lock-free queue**; the main loop translates them (set 1, US
  layout) and shows them. `sti; hlt` sleeps without losing a key.
- A handler that asks for a lock held by the code it interrupted causes a
  **deadlock**. `_print` and the heap turn interrupts off while they hold
  their locks; `teste-deadlock` shows the CPU stuck in the spinlock's
  `pause` without that protection.

<details>
<summary><b>Review questions (click to see the answers)</b></summary>

1. **What is the advantage of an interrupt over *polling*?**
   With polling, the CPU spends all its time asking the device whether
   there is anything new. With interrupts, it does something else (or
   sleeps with `hlt`) and is only called when something happens.

2. **Why does the 8259 need to be remapped, even though it will stay
   masked?**
   Because in the original configuration it uses vectors 0x08–0x0F, which
   are CPU exceptions, and even masked it can raise spurious interrupts
   (IRQ 7 or 15). Remapped to 0x20–0x2F, a spurious one lands on a harmless
   vector, with its own handler.

3. **How does the kernel know that the I/O APIC is at `0xFEC0_0000` and that
   IRQ 0 arrives at GSI 2?**
   Through the MADT, an ACPI table. The path is: UEFI gives the RSDP (read
   before `exit_boot_services`), the RSDP points to the XSDT, and the XSDT
   lists the tables; the one with signature `"APIC"` is the MADT. The
   type 1 record gives the I/O APIC; the type 2 one gives the IRQ 0 → GSI 2
   override.

4. **Why are the APIC's registers mapped with `SEM_CACHE`?**
   Because they belong to a device: a write (such as the EOI) must really
   reach it, and a read (such as the timer counter) must bring the current
   value. With caching, they could stay in the CPU cache.

5. **Why does the APIC timer need calibrating and the PIT does not?**
   The APIC timer counts at the bus clock, which changes from machine to
   machine and is not reported in any register. The PIT has had a fixed
   frequency (1,193,182 Hz) since the original PC, so it serves as a
   stopwatch to measure the other one.

6. **In `teste-sem-eoi`, why did the keyboard stop too, if its handler sends
   an EOI?**
   Because the keyboard interrupt was never even delivered. Without the
   timer's EOI, the APIC considers vector 0x30 still "in service" and blocks
   everything in the same priority class or lower. 0x31 is in the same
   class (3).

7. **Why doesn't the keyboard handler translate or print the key?**
   To be short (while it runs, other interrupts wait) and to take no lock:
   printing would need the CONSOLE lock, which the interrupted code could
   be holding. It only puts the byte in a lock-free queue; the main loop
   does the rest.

8. **Why `sti; hlt` and not a plain `hlt` after seeing that the queue is
   empty?**
   Because a key could arrive between the check and the `hlt`, and the CPU
   would sleep with it in the queue. With interrupts off during the check
   and the `sti; hlt` pair (the CPU only accepts interrupts after the
   instruction following `sti`), a key that arrives wakes the `hlt`.

9. **What does `sem_interrupcoes` do, and why doesn't it always turn
   interrupts on at the end?**
   It turns interrupts off, runs the function and returns them to their
   previous state. If it always turned them on, when called from inside a
   handler (where interrupts are off) it would turn them on in the middle
   of the handler.

</details>
