<!-- GERADO por site/indice_funcoes.py; não editar à mão. / GENERATED; do not edit. -->

# Function index

Every kernel function, grouped by the file it lives in and sorted alphabetically, each with one sentence about what it does and the chapter that created it. To see how the pieces fit together, start with the [SiliconOS map](mapa.md).

This page is **generated automatically** (`site/indice_funcoes.py`) from the "Functions in this chapter" table of every chapter, each time the site is published. When a function changed in a later chapter, the sentence says "(since doc NN ...)"; the current code is always in [`src/`](https://github.com/moises-dev-git/siliconOS/tree/main/src).

324 functions and 74 tests (or test helpers), from 18 chapters.

---

## Kernel (`src/`)

### [src/main.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/main.rs)

| Function | What it does | Doc |
|---|---|---|
| `_print` | What `kprint!`/`kprintln!` call: formats the text and sends it to the serial port and the screen. (Since doc 08 with interrupts off, via `escrever`.) | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `demonstrar_heap` | On a normal boot, uses `Box`, `Vec` and `String` and prints where they live: proof that the heap works. | [07](07-heap.md#functions-in-this-chapter) |
| `ecoar_teclado` | The doc 08 main loop: sleeps, and each key that arrives is translated and shown (doc 09 replaces it with the shell). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `escrever` | The part of `_print` that actually writes (serial and screen), running inside `sem_interrupcoes`. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `explicar_traducao` | Prints the 4 indices of a virtual address and the translation result (used at boot and by the `traduzir` command). | [06](06-paginacao.md#functions-in-this-chapter) |
| `imprimir_alocador` | Prints the bitmap's size and address and how many frames are free. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `imprimir_mapa` | Prints the memory map summary (per kind) and where the kernel, the stack and the framebuffer are. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `kernel_main` | The kernel itself: initializes each subsystem in order and finally loops. Each chapter adds a step (GDT, IDT, memory, heap, interrupts; since doc 09 it hands control to the shell). Never returns. | [00](00-visao-geral.md#functions-in-this-chapter) |
| `main` | UEFI entry point (`#[entry]`). Saves the framebuffer and the RSDP, exits boot services and calls `kernel_main`. | [00](00-visao-geral.md#functions-in-this-chapter) |
| `testes_de_excecao` | Deliberately triggers the exception chosen by the feature (`teste-breakpoint`, `teste-divisao`, `teste-gpf`...). | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `testes_de_heap` | Heap tests chosen by the feature (`teste-heap`, `teste-heap-liberacao-dupla`, `teste-heap-esgotar`...). | [07](07-heap.md#functions-in-this-chapter) |
| `testes_de_interrupcoes` | Tests chosen by the feature (`teste-timer`, `teste-sem-eoi`, `teste-print-interrupcao`, `teste-deadlock`). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `testes_de_memoria` | Frame allocator tests chosen by the feature (`teste-quadros`, `teste-liberacao-dupla`). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `testes_de_paginacao` | Paging tests chosen by the feature (`teste-paginacao`, `teste-desmapeado`, `teste-nulo`). | [06](06-paginacao.md#functions-in-this-chapter) |

### [src/acpi.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/acpi.rs)

| Function | What it does | Doc |
|---|---|---|
| `checksum_ok` | Checks the ACPI checksum: all bytes added up (with wrap-around) give zero. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `desligar` | Powers the machine off (S5 state): finds SLP_TYP in the DSDT and writes it with SLP_EN to the FADT's PM1a register. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `guardar_rsdp` | Takes the RSDP (ACPI 2.0) address from the UEFI configuration table and stores it; must run before `exit_boot_services`. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ler` | Reads a value of any type at a physical address without requiring alignment (`read_unaligned`). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ler_madt` | Parses the MADT entries: local APICs, I/O APICs and IRQ overrides. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `madt` | Reads the MADT on the first call and always returns the same copy. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `Madt::rotear_irq` | Which GSI an ISA IRQ goes to, and with which polarity and trigger mode, applying the MADT's overrides. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `procurar_tabela` | Validates the RSDP and the XSDT (signature and checksum) and walks the XSDT looking for the table with the given signature ("APIC", "FACP"...). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `tipo_s5` | Searches the DSDT's AML bytes for the `_S5_` object and returns the package's first element (SLP_TYPa). | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### [src/console.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/console.rs)

| Function | What it does | Doc |
|---|---|---|
| `Console::cols` | How many text columns fit on the screen (width in pixels ÷ character width). | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::desenhar_char` | Draws one character at cell (column, row): for each bit of the 8×16 font glyph, one pixel in the text or background color. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::limpar` | Paints the whole screen with the background color and moves the cursor back to the start. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::new` | Creates the console from the framebuffer info (address, width, height, pixels per line), with the cursor at the top left. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::nova_linha` | Moves the cursor to the start of the next line; if it was already on the last one, calls `rolar`. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::pixel` | Paints one pixel: computes the offset `y * stride + x` and writes the color (u32) straight into framebuffer memory. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::rolar` | When the screen is full, copies every pixel row one text line up and clears the last one. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::rows` | How many text rows fit on the screen (height in pixels ÷ character height). | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::write_byte` | Writes one byte to the screen: `\n` calls `nova_linha`, `\r` returns to column 0; anything else becomes `desenhar_char`, wrapping at the end of the line. (Since doc 08, backspace erases the previous character.) | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Console::write_str` | `fmt::Write` implementation: passes each byte of the string to `write_byte`. This is what lets `write!` target the console. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |

### [src/cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/cpu.rs)

| Function | What it does | Doc |
|---|---|---|
| `desligar_interrupcoes` | `cli`: hardware interrupts are held pending. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `escrever_msr` | `wrmsr`: writes an MSR (number in ECX, value in EDX:EAX). | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `interrupcoes_ligadas` | Reads the IF bit of RFLAGS. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ler_msr` | `rdmsr`: reads a model-specific register (MSR) by number. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ligar_interrupcoes` | `sti`: the CPU starts accepting hardware interrupts. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `reiniciar` | Reboots: RESET pulse through the 8042 (command 0xFE); if that fails, loads an empty IDT and forces a triple fault. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `sem_interrupcoes` | Runs a closure with interrupts off and restores IF afterwards; protects anything a handler also uses. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/escalonador.rs)

| Function | What it does | Doc |
|---|---|---|
| `acordar` | Sets every process blocked for the given reason to `Pronto` and returns how many; can be called from a handler. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `acordar_em` | The same as `acordar`, for code that already holds the table's lock (`encerrar`). | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `bloquear_e_trocar` | Marks the current process as `Bloqueado(motivo)` and switches processes; returns when it is woken up and its turn comes. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `ceder` | With interrupts off, picks the next process and switches to it; returns when this one's turn comes again. | [12](12-escalonador.md#functions-in-this-chapter) |
| `com_arquivos` | Runs a closure with the current process's file table, with the lock held and interrupts off. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `criar` | Creates a process with a fabricated kernel stack (6 registers, return to `primeira_entrada`, an `iretq` frame) and puts it in the table as `Pronto`. | [12](12-escalonador.md#functions-in-this-chapter) |
| `dormir` | Blocks the current process for at least that many ticks, without using the CPU. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `encerrar` | Marks the current process `Terminado`, prints the result if it is a background one and yields never to return. | [12](12-escalonador.md#functions-in-this-chapter) |
| `escolher_proximo` | Round robin: finds the next `Pronto`, updates the states, switches TSS.RSP0 and CR3 and returns the two RSPs for `trocar`. | [12](12-escalonador.md#functions-in-this-chapter) |
| `esperar` | Yields the CPU until process `pid` ends and returns the result. (Since doc 15, blocked: out of the round robin until the process ends.) | [12](12-escalonador.md#functions-in-this-chapter) |
| `esperar_ate` | Blocks the current process until the condition is true, checking it with interrupts off (no lost wakeup). | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `ha_outros_prontos` | Is any process besides the current one ready to run? (Removed in doc 15: the shell now blocks.) | [12](12-escalonador.md#functions-in-this-chapter) |
| `ha_terminados_em_segundo_plano` | Is there a finished background process waiting to be reaped? (part of the shell's condition) | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `init` | Registers the code that is running (the kernel) as process 0. | [12](12-escalonador.md#functions-in-this-chapter) |
| `janela_do_acordar_perdido` | (only with `teste-acordar-perdido`) Turns interrupts on for 3 s between checking and blocking, to show the bug. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `laco_ocioso` | (naked) The idle process: `sti; hlt` forever; runs when nobody else can. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `listar` | Pid, name, state and CPU ticks of each process (the `ps` command). | [12](12-escalonador.md#functions-in-this-chapter) |
| `pid_atual` | The pid of whoever is running (the `pid` syscall). | [12](12-escalonador.md#functions-in-this-chapter) |
| `primeira_entrada` | (naked) Where a new process's first `trocar` "returns": zeroes the registers and does `iretq` into ring 3. | [12](12-escalonador.md#functions-in-this-chapter) |
| `Processo::topo_pilha` | Address of the top of the process's kernel stack (the TSS.RSP0 value). | [12](12-escalonador.md#functions-in-this-chapter) |
| `recolher` | If process `pid` has ended, removes it, frees stack and space and returns the result. | [12](12-escalonador.md#functions-in-this-chapter) |
| `recolher_segundo_plano` | Frees every background process that has already ended. | [12](12-escalonador.md#functions-in-this-chapter) |
| `retirar` | Removes a finished process from the table (fixing the current index) and returns the process and its result. | [12](12-escalonador.md#functions-in-this-chapter) |
| `terminou` | Has process `pid` finished (or does it not even exist)? The condition of `esperar`. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `tique` | Called by the timer: counts the CPU time of the program interrupted in ring 3, ends whoever went over the limit and yields. | [12](12-escalonador.md#functions-in-this-chapter) |
| `trocar` | (naked) The context switch: pushes 6 registers, saves RSP, loads the other process's RSP, pops and `ret`s. | [12](12-escalonador.md#functions-in-this-chapter) |

### [src/gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/gdt.rs)

| Function | What it does | Doc |
|---|---|---|
| `definir_rsp0` | Writes TSS.rsp[0] (through the `UnsafeCell`, with an unaligned write): the stack used on the next interrupt from ring 3. | [12](12-escalonador.md#functions-in-this-chapter) |
| `descritor_tss` | Builds the TSS descriptor, which takes two GDT entries (128 bits) because the base is 64 bits wide. | [03](03-gdt-e-tss.md#functions-in-this-chapter) |
| `dpl` | Builds the privilege level bits (DPL, bits 45–46) of a segment descriptor. | [03](03-gdt-e-tss.md#functions-in-this-chapter) |
| `estado` | Reads back the active GDT (`sgdt`) and the CS, SS, DS and TR selectors, so the boot can show that the switch worked. | [03](03-gdt-e-tss.md#functions-in-this-chapter) |
| `init` | Fills the TSS (the double-fault IST1 stack and, since doc 10, RSP0), loads the GDT with `lgdt`, switches CS with `retfq`, reloads SS/DS/ES and loads the TSS with `ltr`. | [03](03-gdt-e-tss.md#functions-in-this-chapter) |
| `topo_pilha_anel0` | Top of the stack the CPU uses when coming from ring 3 (TSS.rsp[0]). | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `topo_pilha_double_fault` | Address of the top of the IST1 stack (the double-fault one), printed at boot. | [03](03-gdt-e-tss.md#functions-in-this-chapter) |

### [src/idt.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/idt.rs)

| Function | What it does | Doc |
|---|---|---|
| `breakpoint` | #BP (`int3`) handler: prints the frame and lets execution continue at the next instruction. | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `divisao` | #DE (divide by zero) handler: prints the frame and panics. (Since doc 10, if it came from ring 3, it only kills the program.) | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `double_fault` | #DF handler, running on the IST1 stack (works even if the normal stack is destroyed). There is no way to continue: prints and panics. | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `Entrada::new` | Creates an IDT entry (interrupt gate, DPL 0, present) pointing at the handler, with the chosen IST stack (0 = none). | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `Entrada::para_usuario` | Changes the entry's DPL to 3 (type 0xEE) so that `int 0x80` works from ring 3. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `estado` | Base and limit of the active IDT, read with `sidt` (boot diagnostics). | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `init` | Fills the table with the exception handlers and loads it with `lidt`. (Docs 08 and 10 add the timer, keyboard, spurious and `int 0x80` entries.) | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `opcode_invalido` | #UD (invalid opcode) handler: prints the frame and panics. (Since doc 10, it only kills the program if it came from ring 3.) | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `page_fault` | #PF handler: reads the faulting address from CR2, decodes the error code (present/write/user/fetch) and panics. (Since doc 10, it only kills the program if it came from ring 3.) | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `protecao_geral` | #GP handler: shows the error code (the selector involved, if any) and panics. (Since doc 10, it only kills the program if it came from ring 3.) | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |
| `QuadroInterrupcao::fmt` | `Display` implementation for `QuadroInterrupcao`: prints the RIP, CS, RFLAGS, RSP and SS the CPU pushed. | [04](04-idt-e-excecoes.md#functions-in-this-chapter) |

### [src/interrupcoes/apic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/apic.rs)

| Function | What it does | Doc |
|---|---|---|
| `eoi` | Writes 0 to the EOI register: "done". Without it, no interrupt of equal or lower priority arrives. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `escrever` | Writes a local APIC register. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `id` | This APIC's number (this CPU's), used as the destination in the I/O APIC. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `init` | Reads MSR 0x1B, maps the registers, sets the priority to zero, masks LINT0 (where the 8259 would come in) and enables the APIC with the spurious vector. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ler` | Reads a 32-bit local APIC register (MMIO at the given offset). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/interrupcoes/ioapic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/ioapic.rs)

| Function | What it does | Doc |
|---|---|---|
| `entrada` | A pin's current redirection entry (diagnostics). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `escrever` | Writes an I/O APIC register through the same IOREGSEL/IOWIN pair. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `init` | Maps the I/O APIC, reads how many pins it has and masks them all. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ler` | Reads an I/O APIC register: writes the number to IOREGSEL and reads IOWIN. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `rotear` | Programs a pin's redirection entry: vector, destination CPU, polarity and trigger mode, unmasked. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/interrupcoes/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/mod.rs)

| Function | What it does | Doc |
|---|---|---|
| `espuria` | Spurious interrupt handler (8259 and APIC): only counts, no EOI. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `espurias` | How many spurious interrupts have arrived. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `init` | The whole sequence: disables the 8259, enables the local APIC and the I/O APIC, calibrates and starts the timer, routes the keyboard and runs `sti`. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/interrupcoes/pic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/pic.rs)

| Function | What it does | Doc |
|---|---|---|
| `desligar` | Remaps both 8259s to 0x20–0x2F (so they do not collide with exceptions) and masks every IRQ. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `esperar` | Writes to port 0x80 just to spend ~1 µs between 8259 commands. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/interrupcoes/teclado.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/teclado.rs)

| Function | What it does | Doc |
|---|---|---|
| `Decodificador::new` | New decoder: Shift and Caps Lock off. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `Decodificador::processar` | Translates a set-1 scancode into a character (US layout), tracking Shift, Caps Lock and the 0xE0 prefix. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `digitar` | Test helper: runs a sequence of scancodes through the decoder and collects the characters. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `enviar_comando` | Waits for the 8042 input buffer to empty and writes a command to port 0x64. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `enviar_dado` | Waits for the input buffer to empty and writes a data byte to port 0x60. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `esperar` | Sleeps with `hlt` if the queue is empty, using `cli; test; sti; hlt` so a key arriving in between is not lost. (Replaced in doc 15: the shell blocks with `escalonador::esperar_ate` and the handler wakes it up.) | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `fila_vazia` | Is the queue empty? (compares the read and write indices). (Replaced in doc 15 by `tem_scancode`.) | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `init` | Discards stale bytes, reads the 8042 configuration, sets the IRQ 1 bit and writes it back. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `ler_dado` | Waits for the output buffer to fill and reads a byte from port 0x60. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `perdidos` | How many scancodes were dropped because the queue was full. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `proximo_scancode` | Takes the next scancode from the queue (the consumer side of the SPSC queue). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `recebidos` | How many scancodes the handler has received. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `tem_scancode` | Is there a scancode in the queue? Replaces `esperar` and `fila_vazia`. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `tratador` | Vector 0x31 handler: reads the scancode from port 0x60, queues it (or counts it as lost) and sends EOI. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/interrupcoes/timer.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/interrupcoes/timer.rs)

| Function | What it does | Doc |
|---|---|---|
| `calibrar` | Measures how much the APIC timer counts in 10 ms, using PIT channel 2 as a stopwatch. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `hora_rtc` | Hour, minute and second from the CMOS clock, converted from BCD and 12-hour format when needed. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `iniciar` | Starts the APIC timer in periodic mode on the given vector with the calibrated count. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `milissegundos` | Ticks × 10. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `registrador` | (Inner function of `hora_rtc`.) Reads a CMOS register: number to port 0x70, value from 0x71. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `segundos_rtc` | Reads the seconds of the CMOS clock (RTC) by polling, to check the timer. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `tiques` | How many ticks (of 10 ms) since the timer was started. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `tratador` | Vector 0x30 handler: adds a tick, sends EOI (and, since doc 10, kills a ring-3 program that went over the limit). | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |

### [src/memoria/bump.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/bump.rs)

| Function | What it does | Doc |
|---|---|---|
| `AlocadorBump::alloc` | Bump allocator: aligns the pointer, advances it by the requested size and returns the start. Never reuses anything. | [07](07-heap.md#functions-in-this-chapter) |
| `AlocadorBump::dealloc` | Only counts the free: a bump allocator cannot give memory back. | [07](07-heap.md#functions-in-this-chapter) |
| `estado` | Bytes consumed, allocations and frees of the bump allocator (to compare with the real heap). | [07](07-heap.md#functions-in-this-chapter) |
| `init` | Maps the heap range for the bump allocator (used only with the `teste-bump` feature). | [07](07-heap.md#functions-in-this-chapter) |

### [src/memoria/espaco.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/espaco.rs)

| Function | What it does | Doc |
|---|---|---|
| `EspacoEnderecos::ativar` | Starts using this space (writes the root to CR3). | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::do_programa` | Is the address in the lower half, in a PML4 entry the kernel does not use? | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::drop` | `Drop`: checks the space is not active and frees the trees of the program's entries and the PML4. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::entrada_livre` | Is entry `i` of the kernel's PML4 empty? | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::mapear` | Maps a 4 KiB page and takes ownership of the frame; rejects addresses in the kernel's area (`AreaDoKernel`) and frees the frame on error. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::novo` | Creates a zeroed PML4 and copies into it the present entries of the kernel's PML4 (sharing the tables below them). | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::raiz` | Physical address of the space's PML4. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `EspacoEnderecos::traduzir` | `traduzir_em` with this space's PML4. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `liberar_arvore` | Recursively frees a table, the tables below it and the pages the PT points to. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |

### [src/memoria/heap.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/heap.rs)

| Function | What it does | Doc |
|---|---|---|
| `alinhar_acima` | Rounds a number up to the next multiple of a power of 2. | [07](07-heap.md#functions-in-this-chapter) |
| `AlocadorLista::alloc` | `GlobalAlloc::alloc`: what `Box`/`Vec` call. Locks the heap with interrupts off and calls `alocar`. | [07](07-heap.md#functions-in-this-chapter) |
| `AlocadorLista::dealloc` | `GlobalAlloc::dealloc`: locks the heap with interrupts off and calls `liberar`. | [07](07-heap.md#functions-in-this-chapter) |
| `contem` | Is the address in the heap's already mapped range? | [07](07-heap.md#functions-in-this-chapter) |
| `estatisticas` | Mapped size, bytes in use, free blocks, allocations and frees (there is the method on `Heap` and this global shortcut). | [07](07-heap.md#functions-in-this-chapter) |
| `Heap::alocar` | `procurar`; if nothing is found, `crescer` and try again; if that still fails, returns null. | [07](07-heap.md#functions-in-this-chapter) |
| `Heap::crescer` | Maps more pages at the end of the heap (up to the limit) and inserts them as a free block. | [07](07-heap.md#functions-in-this-chapter) |
| `Heap::inserir` | Returns a block to the list in address order and merges it with touching neighbors; detects double frees. | [07](07-heap.md#functions-in-this-chapter) |
| `Heap::liberar` | Computes the real size and calls `inserir`. | [07](07-heap.md#functions-in-this-chapter) |
| `Heap::livre_no_fim` | Size of the last free block if it touches the end of the heap (so growing only adds what is missing). | [07](07-heap.md#functions-in-this-chapter) |
| `Heap::procurar` | First fit: walks the list and carves the first free block where the request fits aligned; the leftovers go back to the list. | [07](07-heap.md#functions-in-this-chapter) |
| `init` | Maps the initial heap and adds that space as a single free block. | [07](07-heap.md#functions-in-this-chapter) |
| `mapear_faixa` | Maps a range of pages, each on a new frame, with WRITE and NO_EXECUTE; returns how many bytes it managed. | [07](07-heap.md#functions-in-this-chapter) |
| `tamanho_real` | Size the heap actually reserves: a multiple of 16 and never smaller than a `NoLivre`. | [07](07-heap.md#functions-in-this-chapter) |

### [src/memoria/mapa.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/mapa.rs)

| Function | What it does | Doc |
|---|---|---|
| `get` | Returns the already initialized map (used by later code, such as the allocator). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `init` | Copies the UEFI map into the kernel's format (merging neighboring regions of the same kind). Only does work on the first call. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Mapa::encontrar` | Which region a physical address is in (if any). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Mapa::fim_da_ram` | Highest RAM address (ignores MMIO and Reserved): how far the allocator's bitmap must reach. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Mapa::regioes` | The valid regions, as a slice (`&[Regiao]`) of the fixed array. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Mapa::total` | Adds up the frames and counts the regions of one kind (for the boot summary). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Regiao::contem` | Is the physical address inside this region? | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Regiao::fim` | First physical address after the region (start + frames × 4096). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Tipo::de_uefi` | Converts the UEFI memory type (about 15 kinds) into the few kinds the kernel cares about (Free, Kernel, ACPI, MMIO, Reserved...). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Tipo::nome` | Readable name of the kind, for printing. | [05](05-memoria-fisica.md#functions-in-this-chapter) |

### [src/memoria/paginacao.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/paginacao.rs)

| Function | What it does | Doc |
|---|---|---|
| `alocar_e_mapear` | Shortcut for tests: allocates a frame and maps it at the given virtual address. | [06](06-paginacao.md#functions-in-this-chapter) |
| `ativar` | Writes a PML4 to CR3 (unless it is already active); the CPU empties the TLB. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `canonico` | Is the address canonical (bits 48–63 equal to bit 47)? | [06](06-paginacao.md#functions-in-this-chapter) |
| `desmapear` | `desmapear_em` on the active PML4, followed by `invalidar`. | [06](06-paginacao.md#functions-in-this-chapter) |
| `desmapear_em` | Removes the mapping and returns the physical address it used (empty intermediate tables stay). | [06](06-paginacao.md#functions-in-this-chapter) |
| `Entrada::endereco` | The physical address stored in the entry (bits 12–51). | [06](06-paginacao.md#functions-in-this-chapter) |
| `Entrada::flags` | Only the entry's control bits (everything but the address). | [06](06-paginacao.md#functions-in-this-chapter) |
| `Entrada::grande` | Does the entry have the PS bit, i.e. map a 2 MiB or 1 GiB page instead of pointing to another table? | [06](06-paginacao.md#functions-in-this-chapter) |
| `Entrada::presente` | Does the entry have the P (present) bit? | [06](06-paginacao.md#functions-in-this-chapter) |
| `fisico_para_virtual` | Converts a physical address into the virtual address the kernel can reach it through right now (identical before the table switch; physical + 0xFFFF_8000_0000_0000 after). | [06](06-paginacao.md#functions-in-this-chapter) |
| `indices` | The four 9-bit indices a virtual address selects (PML4, PDPT, PD, PT). | [06](06-paginacao.md#functions-in-this-chapter) |
| `init` | Builds the kernel's tables (first 2 MiB in 4 KiB pages without page 0; all RAM in 2 MiB pages, identity-mapped and in the upper half; the framebuffer), switches CR3 and starts using the upper half. | [06](06-paginacao.md#functions-in-this-chapter) |
| `invalidar` | `invlpg`: drops one address from the TLB so the CPU does not use the old translation. | [06](06-paginacao.md#functions-in-this-chapter) |
| `mapear` | `mapear_em` on the active PML4, followed by `invalidar`. | [06](06-paginacao.md#functions-in-this-chapter) |
| `mapear_em` | Maps virtual → physical in a hierarchy, creating any missing intermediate tables; rejects unaligned, non-canonical or already mapped addresses. | [06](06-paginacao.md#functions-in-this-chapter) |
| `mapear_mmio` | Maps device registers in the MMIO_INICIO range, with WRITE, NO_CACHE and NO_EXECUTE, and returns the virtual address. | [08](08-interrupcoes-de-hardware.md#functions-in-this-chapter) |
| `nova_tabela` | Takes a free frame and zeroes it: a new table with no entries. | [06](06-paginacao.md#functions-in-this-chapter) |
| `raiz_atual` | Physical address of the active PML4 (CR3 without the control bits). | [06](06-paginacao.md#functions-in-this-chapter) |
| `raiz_kernel` | Physical address of the kernel's PML4 (the active tables, before `init`). | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `tabela_em` | Gives access, as `&mut Tabela`, to the page table at the given physical address (via `fisico_para_virtual`). | [06](06-paginacao.md#functions-in-this-chapter) |
| `TamanhoPagina::bytes` | Page size in bytes (4 KiB, 2 MiB or 1 GiB). | [06](06-paginacao.md#functions-in-this-chapter) |
| `TamanhoPagina::nivel_folha` | Which level holds the final entry for this page size (PT for 4 KiB, PD for 2 MiB, PDPT for 1 GiB). | [06](06-paginacao.md#functions-in-this-chapter) |
| `TamanhoPagina::nome` | Readable name of the size, for printing. | [06](06-paginacao.md#functions-in-this-chapter) |
| `traduzir` | `traduzir_em` using the active PML4. | [06](06-paginacao.md#functions-in-this-chapter) |
| `traduzir_em` | Does by hand what the CPU does: walks the tables from a PML4 and returns the physical address, size and flags (or `None`). | [06](06-paginacao.md#functions-in-this-chapter) |

### [src/memoria/quadros.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/memoria/quadros.rs)

| Function | What it does | Doc |
|---|---|---|
| `AlocadorQuadros::alocar` | Searches the bitmap, from the last word used, for the first 64-bit word that is not full; `trailing_ones` gives the free bit, which is marked and returned. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `AlocadorQuadros::liberar` | Clears the frame's bit and moves the search position back if needed; an unaligned address, one outside RAM, or one already free (double free) panics. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `AlocadorQuadros::livres` | Counter of free frames. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `AlocadorQuadros::marcar` | Sets or clears a frame's bit in the bitmap. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `AlocadorQuadros::marcar_intervalo` | Marks `n` consecutive frames as free or in use. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `AlocadorQuadros::ocupado` | Reads the frame's bit in the bitmap (1 = in use). | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `AlocadorQuadros::total` | How many frames the bitmap covers. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `alocar` | Global shortcut: locks the allocator's `Mutex` and calls `AlocadorQuadros::alocar`. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `init` | Creates the allocator: puts the bitmap in the first free region above 1 MiB where it fits, marks everything as used, frees the free regions above 1 MiB and protects the bitmap itself. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `liberar` | Global shortcut for `AlocadorQuadros::liberar`. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `livres` | Global shortcut for `AlocadorQuadros::livres`. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Quadro::endereco` | Physical address of the frame. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `Quadro::indice` | Frame number (address ÷ 4096): its position in the bitmap. | [05](05-memoria-fisica.md#functions-in-this-chapter) |
| `reposicionar_bitmap` | Switches to accessing the bitmap through another virtual address; called when paging switches to the kernel's tables. | [05](05-memoria-fisica.md#functions-in-this-chapter) |

### [src/panic.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/panic.rs)

| Function | What it does | Doc |
|---|---|---|
| `escrever_detalhes` | Writes the panic's message, file, line and column to any destination that implements `Write` (called twice: serial and screen). | [02](02-panic-handler.md#functions-in-this-chapter) |
| `panic` | The `#[panic_handler]`: turns interrupts off, detects a panic inside a panic, writes the details to serial and screen (red background, force-unlocking the console) and halts. (Since doc 09, under `cargo test`, it exits QEMU with the failure code.) | [02](02-panic-handler.md#functions-in-this-chapter) |
| `parar` | Endless `cli; hlt` loop: the CPU sleeps and never wakes up again. | [02](02-panic-handler.md#functions-in-this-chapter) |

### [src/qemu.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/qemu.rs)

| Function | What it does | Doc |
|---|---|---|
| `sair` | Writes to port 0xF4 of the isa-debug-exit device: QEMU exits with code (value × 2 + 1), 33 = ok, 35 = failure. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### [src/ramdisk.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/ramdisk.rs)

| Function | What it does | Doc |
|---|---|---|
| `abrir` | The file with that name, if it exists. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `arquivos` | An iterator over every file on the RAM disk. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `Arquivos::next` | The iterator: reads a header, returns name and data (slices of the tar) and jumps to the next multiple of 512. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `cabecalho_valido` | Checks a header's "ustar" signature and checksum. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `ler_octal` | Reads an octal number written as text, rejecting invalid digits and overflows. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `tamanho_total` | Size of the whole tar, in bytes. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [src/serial.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/serial.rs)

| Function | What it does | Doc |
|---|---|---|
| `inb` | The `in` instruction: reads one byte from an I/O port. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `inw` | 16-bit `in` instruction. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `outb` | The `out` instruction: writes one byte to an I/O port. `unsafe` because a wrong port can do anything to the hardware. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `outw` | 16-bit `out` instruction (used for the ACPI PM1a register). | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `Serial::init` | Configures the COM1 UART (port 0x3F8): 38400 baud, 8 bits, no parity, 1 stop bit, FIFOs on, UART interrupts off. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Serial::write_byte` | Waits for the transmit register to be empty (bit 5 of the status port) and sends one byte. | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |
| `Serial::write_str` | `fmt::Write` implementation for the serial port: sends byte by byte, turning `\n` into `\r\n`. (Since doc 08, backspace becomes "back, space, back".) | [01](01-saida-serial-e-video.md#functions-in-this-chapter) |

### [src/shell.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/shell.rs)

| Function | What it does | Doc |
|---|---|---|
| `ajuda` | `ajuda` command: lists the commands and their descriptions, taken from the table itself. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `cpu_info` | `cpu` command: vendor and model read with `cpuid`. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `desligar` | `desligar` command: calls `acpi::desligar` and waits 20 ticks before saying it failed. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `eco` | `eco` command: prints the arguments. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `elf` | `elf <file>` command: header and segments, like `readelf -l`. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `espacos` | `espacos` command: two live spaces with `ola` and `relogio` at the same address, showing each one's PML4, physical address and first bytes. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `executar` | Splits the line into words, looks the first one up in the command table and calls the function with the rest. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `int` | `int` command: counters of ticks, keys received/lost and spurious interrupts. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `ler` | `ler <address>` command: reads 8 bytes at the address without checking anything (on purpose: `ler 0` shows a page fault). | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `ler_numero` | Parses a decimal or hexadecimal (`0x`) number, accepting `_` as a separator. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `limpar` | `limpar` command: clears the screen. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `ls` | `ls` command: the files on the RAM disk. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `mem` | `mem` command: free frames and heap statistics (in use, peak, mapped, free blocks). | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `programas` | `programas` command: lists the built-in programs and their sizes. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `ps` | `ps` command: the process table. | [12](12-escalonador.md#functions-in-this-chapter) |
| `reiniciar` | `reiniciar` command: calls `cpu::reiniciar`. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `rodar` | The kernel's main loop: takes scancodes from the queue, decodes them, passes them to the shell and sleeps when there is nothing to do. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `rodar_programa` | `rodar <program>` command: runs it in ring 3 and shows the result (exit code, fault or timeout). | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `Shell::new` | New shell with an empty line. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `Shell::tecla` | Handles one key: a character goes into the line (up to the limit), backspace deletes, Enter runs it and shows the prompt again. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `tempo` | `tempo` command: time since boot (from ticks) and the RTC time. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `traduzir` | `traduzir <address>` command: shows the virtual address's translation (via `explicar_traducao`). | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### [src/testes.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/testes.rs)

| Function | What it does | Doc |
|---|---|---|
| `Testavel::rodar` | `Testavel::rodar` for every `Fn()`: prints the test name, runs it and prints `[ok]` (if it panics, the handler takes over). | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### [src/usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/argumentos.rs)

| Function | What it does | Doc |
|---|---|---|
| `montar` | Builds in a vector the top of a program's initial stack (argc, argv, zeros, strings) and computes RSP, aligned to 16; refuses empty, too many, too big or 0-containing arguments. | [16](16-argumentos.md#functions-in-this-chapter) |

### [src/usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/elf.rs)

| Function | What it does | Doc |
|---|---|---|
| `carregar` | Loads every LOAD segment into a space and checks that the entry is executable. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `carregar_segmento` | Validates the segment and, page by page, allocates a zeroed frame, copies the file's part and maps it with the segment's permissions. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `ler` | `N` bytes of the file starting at a position, or `Truncado`. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `ler_cabecalho` | Checks signature, class, byte order, type, machine and program header size; returns entry and table. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `permissoes` | A segment's flags as "R-X", "RW-"... | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `segmento` | Reads program header number `i`. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `u16_em` / `u32_em` / `u64_em` | A little-endian number at a position in the file. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [src/usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/mod.rs)

| Function | What it does | Doc |
|---|---|---|
| `abrir` (syscall) | Syscall 5: validates and copies the path, opens through the VFS and returns the descriptor. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `descrever` | Short text describing a program's result ("terminou: codigo de saida 42"...). | [12](12-escalonador.md#functions-in-this-chapter) |
| `dormir` (syscall) | Syscall 4: sleeps `ms` milliseconds (rounded up to ticks); returns 0. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `encerrar` | Stores the result and jumps to `voltar_ao_kernel`; called from inside a handler, never returns. (Replaced in doc 12 by `escalonador::encerrar`.) | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `entrada_syscall` | (naked) Vector 0x80 handler: pushes the registers into a `Registros`, calls `tratar_syscall` and returns with `iretq`. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `entrar_no_anel3` | (naked) Saves the kernel's registers and RSP, builds an interrupt frame by hand (the program's SS, RSP, RFLAGS, CS, RIP) and runs `iretq`. (Replaced in doc 12 by the fabricated stack of `escalonador::criar` + `primeira_entrada`.) | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `escrever` | The `escrever` syscall: validates the pointer, prints the bytes and returns how many were written (or -14). | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `faixa_com_flags` | Is every page in the range the program's, with the required bits? | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `faixa_do_programa` | Checks that a pointer + length from the program lies entirely within its pages (with no overflow in the addition). | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `faixa_gravavel_do_programa` | Like `faixa_do_programa`, but requires writable pages (the destination of a kernel copy). | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `falha_do_programa` | Called by the exception handlers: if CS says ring 3, prints the fault and kills the program; otherwise returns and the handler panics. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `fechar` (syscall) | Syscall 7: closes the descriptor. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `iniciar` | Prepares the program's address space (code and stack) and hands it to the scheduler; returns the pid. | [12](12-escalonador.md#functions-in-this-chapter) |
| `iniciar_elf` | Creates the space, loads the ELF, maps 4 stack pages at the end of the lower half and hands it to the scheduler with RSP = top − 8. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `ler` (syscall) | Syscall 6: copies the file's next bytes to a writable destination in the program. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `ms_para_tiques` | Milliseconds to ticks, rounding up and without overflowing. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `nova_pagina_zerada` | Allocates a frame and zeroes it (so the program does not see leftovers from the frame's previous user). | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `rodar` | Maps the program's code and stack with the USER bit, enters ring 3, and when it ends unmaps everything, frees the frames and returns the `Resultado`. (Since doc 11, in its own address space; since doc 12, it is `iniciar` + `escalonador::esperar`.) | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `rodar_por_nome` | Test helper: looks up the program by name and runs it. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `tempo_esgotado` | Called by the timer: did the tick interrupt ring 3, and is the program past `LIMITE_TIQUES`? (Replaced in doc 12 by `escalonador::tique`, which counts CPU time.) | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `tratar_syscall` | Dispatches on the number in rax: `sair`, `escrever` or `tiques`; an unknown number returns -38. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `voltar_ao_kernel` | (naked) Restores the RSP saved by `entrar_no_anel3`, pops the registers and does `ret`: it looks as if `entrar_no_anel3` returned. (Replaced in doc 12 by `escalonador::trocar`.) | [10](10-modo-usuario.md#functions-in-this-chapter) |

### [src/usuario/programas.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/programas.rs)

| Function | What it does | Doc |
|---|---|---|
| `procurar` | Looks up a built-in program by name. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `Programa::codigo` | The program's machine code bytes (a slice of `.rdata` between two labels). | [10](10-modo-usuario.md#functions-in-this-chapter) |

### [src/usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/usuario/syscall.rs)

| Function | What it does | Doc |
|---|---|---|
| `entrada_syscall_rapida` | (naked) Switches to the process's kernel stack, builds the same frame and registers as `int 0x80`, calls `tratar_syscall` and returns with `sysretq`. | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `init` | Writes STAR (selectors), LSTAR (entry), FMASK (IF, TF, DF) and sets EFER.SCE. | [14](14-syscall-sysret.md#functions-in-this-chapter) |

### [src/vfs/disco.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/vfs/disco.rs)

| Function | What it does | Doc |
|---|---|---|
| `Disco::abrir`, `Disco::listar` | The RAM disk (tar) as seen by the VFS. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |

### [src/vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/vfs/mod.rs)

| Function | What it does | Doc |
|---|---|---|
| `abrir` (VFS) | Opens a file by its full path, passing it to the mount's file system. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `ArquivoAberto::ler` | Copies the next bytes and advances the position; 0 at the end. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `ArquivoAberto::restante` | How many bytes are left to read. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Conteudo::bytes` | The content's bytes, fixed (from the tar) or generated. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Descritores::arquivo` | The file of a descriptor, if open; accepts any number without crashing. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Descritores::fechar` | Closes a descriptor and frees the file. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Descritores::guardar` | Stores the file under the lowest free descriptor (from 3), or hands it back if the table is full. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Descritores::novo` | An empty table of open files. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Descritores::quantos` | How many files are open (used in the tests). | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `listar` (VFS) | A directory's contents, with the mounts inside it shown as subdirectories. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `pai` | The directory that contains a path. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `procurar` | Finds the mount responsible for a path and the rest of the path after it. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `SistemaDeArquivos::abrir` | (trait) Opens a file by its name inside the mount and returns the content. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `SistemaDeArquivos::listar` | (trait) The mount's files, with their sizes. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |

### [src/vfs/sis.rs](https://github.com/moises-dev-git/siliconOS/blob/main/src/vfs/sis.rs)

| Function | What it does | Doc |
|---|---|---|
| `Sis::abrir`, `Sis::listar` | The /sis files, generated on the spot by each one's function. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `tempo`, `mem`, `processos` | Generate the text of /sis/tempo, /sis/mem and /sis/processos. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |

---

## User programs (`programas/`) and build

### [build.rs](https://github.com/moises-dev-git/siliconOS/blob/main/build.rs)

| Function | What it does | Doc |
|---|---|---|
| `acrescentar_ao_tar` | Writes a USTAR header (with checksum) and the data, padded to a multiple of 512. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `main` (build.rs) | Builds the programs with another cargo and packs the ELFs into `OUT_DIR/ramdisk.tar`. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `octal` | Writes a number in octal, with leading zeros and a 0 at the end, into a header field. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [programas/build.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/build.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (programas/build.rs) | Tells cargo to relink the programs when `linker.ld` changes. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [programas/src/bin/args.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/args.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (args) | Shows argc and each argument with its address; exits with argc. | [16](16-argumentos.md#functions-in-this-chapter) |

### [programas/src/bin/cat.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/cat.rs)

| Function | What it does | Doc |
|---|---|---|
| `main`, `mostrar` (cat) | Shows each requested file, reading in 64-byte pieces. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |

### [programas/src/bin/conferir.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/conferir.rs)

| Function | What it does | Doc |
|---|---|---|
| `main`, `prova` (conferir) | Sends bad file requests and checks every answer from the kernel. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |

### [programas/src/bin/dados.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/dados.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (dados) | Checks that `.bss` came zeroed, writes to it, adds 1 to `.data`'s `CONTADOR` and returns the value (42). | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [programas/src/bin/insone.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/insone.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (insone) | The same as `soneca`, but with busy waiting. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |

### [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/medir.rs)

| Function | What it does | Doc |
|---|---|---|
| `ciclos` | `rdtsc`: the CPU's cycle counter. | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `main` (medir) | Measures both doors and prints the cycles per call of each. | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `medir` | Average cycles per call of a function, over 100,000 repetitions. | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `tiques_int80` | The `tiques` call through the old door, `int 0x80`. | [14](14-syscall-sysret.md#functions-in-this-chapter) |

### [programas/src/bin/oi.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/oi.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (oi) | Says hi with its own pid and returns 0. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [programas/src/bin/primos.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/primos.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (primos) | Counts the primes below 10,000, shows the first 10 and returns the count (1229). | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `primo` | Is `n` prime? (divisions up to the square root) | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### [programas/src/bin/soneca.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/bin/soneca.rs)

| Function | What it does | Doc |
|---|---|---|
| `main` (soneca) | Wakes up 5 times, every 500 ms, sleeping in between. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |

### [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/main/programas/src/lib.rs)

| Function | What it does | Doc |
|---|---|---|
| `__iniciar` | Stores argc and argv read from the initial stack, calls `main` and exits with the returned value. | [16](16-argumentos.md#functions-in-this-chapter) |
| `_start` (from `principal!`) | (naked) The program's entry point: passes the initial RSP and `main` to `__iniciar`, with the stack aligned. | [16](16-argumentos.md#functions-in-this-chapter) |
| `_start` (macro `principal!`) | Entry point generated by the macro: calls the program's `main` and passes the result to `sair`. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `argumento` | Argument `i` as a `&'static str` (walks to the byte 0), or `None` if it does not exist. | [16](16-argumentos.md#functions-in-this-chapter) |
| `argumentos` | Iterator over all the arguments, like `std::env::args()`. | [16](16-argumentos.md#functions-in-this-chapter) |
| `Arquivo::abrir`, `Arquivo::ler`, `Arquivo::descritor` | An open file, with errors in a `Result`. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `chamada` | Makes a system call through `syscall`, telling the compiler RCX and R11 change. | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `chamada3` | A syscall with three arguments (RDI, RSI, RDX). | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `Console::write_str` | `fmt::Write`: each piece of formatted text becomes an `escrever` (the base of `print!`/`println!`). | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `cru::abrir`, `cru::ler`, `cru::fechar` | The file syscalls with raw numbers and pointers. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `dormir` (sos) | Calls the `dormir` syscall. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `drop` (Arquivo) | Closes the file when it goes out of scope. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `escrever` | System call 1: writes bytes to the console and returns how many (or a negative error). | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `nome_do_erro` | The text of a kernel error code. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `panic` | The programs' panic handler: prints the message and exits with 101. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `pid` | System call 3: the process number. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `quantos_argumentos` | argc: how many arguments, counting the program's name. | [16](16-argumentos.md#functions-in-this-chapter) |
| `sair` | System call 0: ends the program with the given code. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `tiques` | System call 2: timer ticks since boot. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

---

## Automated tests

The `#[test_case]`s that run with `cargo test` (doc [09](09-shell-e-testes.md)), grouped the same way.

### src/acpi.rs

| Function | What it does | Doc |
|---|---|---|
| `dsdt_tem_s5` | Test: the FADT points to a valid DSDT, which has the `_S5_` object. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `irq0_desviada_para_gsi2_no_qemu` | Test: on QEMU, IRQ 0 (the PIT) is redirected to GSI 2. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `irq_sem_desvio_vai_para_a_mesma_gsi` | Test: IRQ 1 (no override) goes to GSI 1, active high, edge-triggered. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `madt_tem_cpu_e_ioapic` | Test: the MADT lists at least one CPU, the local APIC at 0xFEE0_0000 and an I/O APIC. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/cpu.rs

| Function | What it does | Doc |
|---|---|---|
| `msr_do_apic` | Test: MSR 0x1B has base 0xFEE0_0000 and bit 11 (APIC enabled). | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `sem_interrupcoes_restaura_o_estado` | Test: `sem_interrupcoes` clears IF inside and restores it as it was, even when nested. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/escalonador.rs

| Function | What it does | Doc |
|---|---|---|
| `dois_contadores_ao_mesmo_tempo` | Test: two `contador`s together take under 300 ticks (one after the other would be 500). | [12](12-escalonador.md#functions-in-this-chapter) |
| `dois_lacos_dividem_a_cpu` | Test: two `laco`s together take at least 390 wall-clock ticks, because each needs 200 CPU ticks. | [12](12-escalonador.md#functions-in-this-chapter) |
| `iniciar` (in the tests) | Helper: starts a built-in program by name, not in the background. | [12](12-escalonador.md#functions-in-this-chapter) |
| `insone_gasta_a_cpu_e_e_encerrado` | Test: `insone` never lets the CPU stop and is terminated for running out of time. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `o_kernel_dorme_e_a_cpu_fica_parada` | Test: `dormir(20)` takes 20 or 21 ticks, almost all with the idle process running. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `o_kernel_e_o_processo_zero` | Test: whoever runs the tests is process 0, "kernel", `Rodando`. | [12](12-escalonador.md#functions-in-this-chapter) |
| `o_ocioso_e_o_processo_um` | Test: process 1 is "ocioso" and is `Pronto`. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `processo_terminado_devolve_pilha_e_quadros` | Test: once reaped, the process is gone from the table and frames and heap are back to what they were. | [12](12-escalonador.md#functions-in-this-chapter) |
| `rodar_do_disco` | Helper: runs a program from the RAM disk and waits for the result. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `soneca_dorme_sem_gastar_cpu` | Test: while `soneca` exists, the CPU stays stopped at least 90% of the time. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `tiques_ociosos` | Helper: the idle process's ticks so far (time with the CPU stopped). | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |

### src/interrupcoes/teclado.rs

| Function | What it does | Doc |
|---|---|---|
| `enter_e_backspace` | Test: Enter becomes `\n` and Backspace becomes `\x08`. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `letras_e_numeros` | Test: pressing and releasing A, 1 and space gives "a1 " (releases produce nothing). | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `shift_e_caps_lock` | Test: Shift and Caps Lock change case and symbols. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `teclas_estendidas_ignoradas` | Test: keys with the 0xE0 prefix produce no character. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/interrupcoes/timer.rs

| Function | What it does | Doc |
|---|---|---|
| `hora_rtc_valida` | Test: the RTC time is within valid ranges (0–23, 0–59, 0–59). | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `tiques_avancam` | Test: waiting with `hlt`, 3 ticks arrive and `milissegundos()` goes past 30. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/memoria/espaco.rs

| Function | What it does | Doc |
|---|---|---|
| `area_do_kernel_recusada` | Test: mapping on the heap or in entry 0 gives `AreaDoKernel`, without leaking frames. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `destruir_devolve_todos_os_quadros` | Test: two pages 2 MiB apart cost 7 frames, and `drop` returns them all. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `kernel_aparece_igual_em_todos` | Test: a heap address translates the same in a new space and in the kernel's tables. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `mesmo_endereco_paginas_diferentes` | Test: two spaces with different frames at the same address; different translations and real reads, nothing in the kernel's tables. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |
| `pagina_com` | Test helper: allocates a frame and writes a value at its start. | [11](11-espacos-de-enderecos.md#functions-in-this-chapter) |

### src/memoria/heap.rs

| Function | What it does | Doc |
|---|---|---|
| `alinhar_acima_arredonda` | Test: `alinhar_acima` with several values. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `box_mora_no_heap` | Test: a `Box`'s address is inside the heap. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `sem_vazamento_e_blocos_juntados` | Test: allocates 1000 `Box`es, frees half (leaving holes), then the rest; bytes in use and the free block count go back to the start. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/memoria/paginacao.rs

| Function | What it does | Doc |
|---|---|---|
| `erros_de_mapeamento` | Test: non-canonical and unaligned addresses are rejected with the right error. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `mapear_e_desmapear` | Test: maps a page, writes through it, reads the same value through the physical frame, unmaps it and the translation is gone. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `metade_alta_mostra_a_ram` | Test: the upper-half address + 0x20_0000 translates to physical 0x20_0000. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `pagina_zero_nao_mapeada` | Test: page 0 is not mapped (a null pointer gives a page fault). | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/memoria/quadros.rs

| Function | What it does | Doc |
|---|---|---|
| `alocar_e_liberar_devolve_a_contagem` | Test: allocating and freeing brings the free count back to its starting value. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/ramdisk.rs

| Function | What it does | Doc |
|---|---|---|
| `cabecalho_estragado_para_a_leitura` | Test: changing one byte of the first header breaks the sum, and the walk returns no file. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `octal_do_tar` | Test: octal numbers ending in 0 and in a space, and a digit 9 rejected. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `os_tres_programas_estao_no_disco` | Test: the tar has dados, oi and primos, all ELF, and a missing name gives `None`. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |

### src/shell.rs

| Function | What it does | Doc |
|---|---|---|
| `editar_linha` | Test: typing, deleting with backspace (even more times than letters) and an ignored Tab leave the right line. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `ler_numero_decimal_e_hexadecimal` | Test: `ler_numero` with decimals, hexadecimals, separators and invalid input. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `linha_tem_limite` | Test: the line does not grow past the maximum size. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `nomes_de_comandos_unicos` | Test: no two commands in the table share a name. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/testes.rs

| Function | What it does | Doc |
|---|---|---|
| `este_teste_falha` | Test that fails on purpose (`teste-que-falha` feature), to show what a failure looks like. | [09](09-shell-e-testes.md#functions-in-this-chapter) |
| `executar` | The `test_runner`: receives the list generated by the compiler, runs each test and exits QEMU with success. | [09](09-shell-e-testes.md#functions-in-this-chapter) |

### src/usuario/argumentos.rs

| Function | What it does | Doc |
|---|---|---|
| `argumentos_ruins_recusados` | Test: each `montar` error, and the exact limit of one page (4053 fits, 4054 does not). | [16](16-argumentos.md#functions-in-this-chapter) |
| `ler_u64` | Helper: reads the 8-byte number that will live at an address of the built stack. | [16](16-argumentos.md#functions-in-this-chapter) |
| `pilha_inicial_no_formato_system_v` | Test: argc, pointers, zeros and strings in the right places, RSP aligned to 16. | [16](16-argumentos.md#functions-in-this-chapter) |

### src/usuario/elf.rs

| Function | What it does | Doc |
|---|---|---|
| `argumentos_chegam_ao_programa` | Test: `args` exits with argc; `primos 100` exits with 25; `primos abc` exits with -1. | [16](16-argumentos.md#functions-in-this-chapter) |
| `argumentos_demais_recusados_sem_vazar` | Test: 33 arguments are refused by `iniciar_elf` without allocating any frame. | [16](16-argumentos.md#functions-in-this-chapter) |
| `arquivos_ruins_recusados_sem_vazar` | Test: eight different kinds of damage, each rejected with the right error, without losing frames. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `cabecalho_do_oi` | Test: `oi`'s first segment is LOAD, R-X, at `0x2000_0000_0000`, and contains the entry. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `carregar_alterado` | Helper: loads a copy of `oi` modified by a closure. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `programas_em_rust_rodam` | Test: oi, primos and dados exit with 0, 1229 and 42. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `rodar` (in the tests) | Helper: loads an ELF from the disk, runs it and waits for the result. | [13](13-disco-em-ram-e-elf.md#functions-in-this-chapter) |
| `rodar_com` | Helper: runs a program from the disk with an argv and waits for the result. | [16](16-argumentos.md#functions-in-this-chapter) |

### src/usuario/mod.rs

| Function | What it does | Doc |
|---|---|---|
| `invasor_encerrado_por_page_fault` | Test: `invasor` (reads kernel memory) ends with #PF. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `laco_encerrado_pelo_timer` | Test: `laco` (infinite loop) is killed by the timer. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `milissegundos_viram_tiques_para_cima` | Test: 1 ms becomes 1 tick, 11 ms become 2, `u64::MAX` does not overflow. | [15](15-dormir-e-bloqueio.md#functions-in-this-chapter) |
| `nada_fica_mapeado_nem_vaza` | Test: after a warm-up, running `ola` leaves the program's addresses unmapped and the free frame count unchanged. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `ola_sai_com_42` | Test: `ola` writes and exits with 42. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `ponteiro_do_kernel_recusado` | Test: `ponteiro` passes a kernel address, gets -14 and exits with 1. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `privilegiado_encerrado_por_gp` | Test: `privilegiado` (`cli` in ring 3) ends with #GP. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `relogio_usa_syscall_de_tiques` | Test: `relogio` waits 100 ticks through the syscall and exits with 100. | [10](10-modo-usuario.md#functions-in-this-chapter) |
| `validacao_de_ponteiros` | Test: `faixa_do_programa` accepts and rejects the right edge cases. | [10](10-modo-usuario.md#functions-in-this-chapter) |

### src/usuario/syscall.rs

| Function | What it does | Doc |
|---|---|---|
| `as_duas_portas_funcionam_no_mesmo_programa` | Test: `medir` (which uses `int 0x80` and `syscall`) exits with 0. | [14](14-syscall-sysret.md#functions-in-this-chapter) |
| `msrs_configurados` | Test: EFER.SCE set and STAR, LSTAR and FMASK holding the expected values. | [14](14-syscall-sysret.md#functions-in-this-chapter) |

### src/vfs/mod.rs

| Function | What it does | Doc |
|---|---|---|
| `abrir_e_ler_aos_pedacos` | Test: reading 7 bytes at a time gives the whole file and then 0; a directory and a missing file do not open. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `cat_le_do_disco_e_de_sis` | Test: `cat` exits with 0, 1 (missing file) or 2 (no arguments). | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `descritores_reaproveitam_o_menor_livre` | Test: 3 and 4, 3 comes back after being closed, invalid numbers refused, table full at 16. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `kernel_se_defende_e_libera_o_que_ficou_aberto` | Test: `conferir` passes every test, and the 16 files it leaves open go back to the heap. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `montagens_escolhidas_pelo_caminho` | Test: each path lands in the right mount, including `/sistema` (which is not `/sis`'s). | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `raiz_mostra_a_montagem_sis` | Test: `listar("/")` includes `sis/` and `leiame.txt`; a file is not a directory. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
| `rodar_com` | Helper: runs a program from the disk with an argv and waits for the result. | [17](17-vfs-e-arquivos.md#functions-in-this-chapter) |
