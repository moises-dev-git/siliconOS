# 08: Interrupções de hardware: timer e teclado

> **Você vai entender:** o que é uma interrupção de hardware (IRQ) e por que
> ela é melhor do que ficar perguntando "já chegou?"; por que um PC moderno
> tem **dois** sistemas de controladores de interrupção (o 8259 de 1981 e o
> APIC); como o kernel descobre onde eles estão lendo as tabelas **ACPI**;
> como o timer passa a "bater" 100 vezes por segundo; como uma tecla vira um
> caractere na tela; e o perigo novo que tudo isso traz: o **deadlock**.
>
> **Arquivos:** `src/interrupcoes/` (`mod.rs`, `pic.rs`, `apic.rs`,
> `ioapic.rs`, `timer.rs`, `teclado.rs`), `src/acpi.rs`, `src/cpu.rs`,
> `mapear_mmio` em `src/memoria/paginacao.rs`, `ecoar_teclado` e
> `testes_de_interrupcoes` em `src/main.rs`
> **Livro:** capítulo 9
> **Testar:** `./run.sh` (digite na janela do QEMU), `./run.sh teste-timer`,
> `./run.sh teste-sem-eoi`, `./run.sh teste-print-interrupcao`,
> `./run.sh teste-deadlock`, `./run.sh teste-teclado`
> **Leia antes:** doc 04 (IDT e exceções) e doc 06 (paginação)
> **Rust novo aqui:** `Acquire` e `Release` ([§28](rust-basico.md#28-orderingacquire-e-release)),
> `#[repr(C, packed)]` e `read_unaligned` ([§29](rust-basico.md#29-reprc-packed-e-read_unaligned)),
> receber uma closure com `impl FnOnce` ([§30](rust-basico.md#30-receber-uma-closure-impl-fnonce))
>
> **Código deste capítulo:** [estado no fim do doc 08](https://github.com/moises-dev-git/siliconOS/tree/doc-08) · [o que mudou desde o doc 07](https://github.com/moises-dev-git/siliconOS/compare/doc-07...doc-08) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

Desde a primeira linha do `kernel_main`, o SiliconOS roda com as
interrupções de hardware **desligadas** (`cli`, doc 04). Fazia sentido: a
IDT do firmware apontava para código que deixou de existir, e não havia
nada para tratar. Mas isso tem um preço: o kernel é **surdo**. Ele não sabe
que o tempo passa e não sabe que alguém apertou uma tecla.

Existem duas formas de um programa saber que algo aconteceu num dispositivo:

1. **Perguntar o tempo todo** (*polling*): "chegou tecla? chegou tecla?
   chegou tecla?". É o que a serial faz hoje ao escrever: ela fica num laço
   lendo um bit de estado até a UART estar livre (doc 01). Funciona, mas a
   CPU passa 100% do tempo perguntando e não faz mais nada.
2. **Ser avisado** (*interrupção*): o dispositivo manda um sinal elétrico, a
   CPU **interrompe** o que estiver fazendo, executa uma função nossa (o
   **tratador**) e volta exatamente de onde parou.

É a diferença entre ficar atualizando a caixa de entrada a cada segundo e
receber uma notificação. Num programa Java ou JS, é a diferença entre um
laço `while (!pronto)` e um *callback* (`addEventListener("keydown", ...)`).
A interrupção é o *callback* do hardware.

Neste capítulo o SiliconOS passa a ter duas fontes de interrupção:

- o **timer**, que interrompe a CPU 100 vezes por segundo. É assim que um
  sistema operacional sabe que o tempo passa (e, no futuro, troca de
  programa: é o timer que tira a CPU de quem a está usando há tempo demais);
- o **teclado**, que interrompe a cada tecla apertada ou solta.

---

## 2. A ideia: do dispositivo até a IDT

A CPU tem poucos "pinos" para receber interrupções, e o PC tem dezenas de
dispositivos. Por isso existe, no meio do caminho, um **controlador de
interrupções**: ele recebe os sinais de todos os dispositivos, decide qual
é o mais importante, e avisa a CPU dizendo **qual vetor** usar (o número
da entrada da IDT, doc 04).

Cada linha de interrupção de um dispositivo se chama **IRQ** (*interrupt
request*). No PC, as IRQs têm números tradicionais, herdados de 1981:

| IRQ | Dispositivo |
|---|---|
| 0 | timer (PIT) |
| 1 | teclado |
| 4 | porta serial COM1 |
| 8 | relógio CMOS (RTC) |
| 12 | mouse PS/2 |
| 14, 15 | discos IDE |

### Uma história em duas camadas

**1981: o 8259.** O IBM PC original tinha um chip **8259** (o PIC,
*programmable interrupt controller*) com 8 entradas. O PC AT, de 1984,
ligou um segundo 8259 numa das entradas do primeiro (em **cascata**): 15
IRQs ao todo. Simples, mas feito para **uma CPU só**.

**Anos 1990: o APIC.** Com o Pentium e as máquinas com várias CPUs, a Intel
criou o **APIC** (*Advanced PIC*), dividido em duas peças:

- o **APIC local**: um por CPU, **dentro** dela. Recebe interrupções e as
  entrega ao núcleo. Tem também um **timer** próprio;
- o **I/O APIC**: fica no chipset, recebe as IRQs dos dispositivos e as
  encaminha para o APIC local de alguma CPU.

Os computadores de hoje têm **os dois sistemas**. O 8259 continua lá
(emulado dentro do chipset) só porque um sistema antigo, como o DOS, ainda
pode precisar dele. Um sistema moderno faz o que o SiliconOS faz neste
capítulo: **desliga o 8259 e usa o APIC**.

```
     DISPOSITIVOS                 CHIPSET                       CPU
  ┌──────────────┐  IRQ 1   ┌──────────────┐  mensagem  ┌──────────────────┐
  │ teclado PS/2 │─────────►│              │ "vetor 0x31"│   APIC local     │
  └──────────────┘          │   I/O APIC   │────────────►│  ┌────────────┐  │
  ┌──────────────┐  IRQ 4   │  (24 pinos)  │            │  │   timer    │  │
  │ serial COM1  │─────────►│              │            │  │ (vetor 0x30)│ │
  └──────────────┘          └──────────────┘            │  └────────────┘  │
                                                         │        │         │
  ┌──────────────┐                                       │        ▼         │
  │ 8259 (PIC)   │  desligado: todas as linhas           │   núcleo ──► IDT[vetor]
  │  x2, cascata │  mascaradas                           └──────────────────┘
  └──────────────┘
```

Quando uma interrupção chega, a CPU faz o mesmo que já faz numa exceção
(doc 04): empilha RIP, CS, RFLAGS, RSP e SS, olha a IDT na posição do vetor
e pula para o tratador. A diferença é o **fim**: o tratador de uma
interrupção de hardware precisa avisar o controlador de que terminou
(**EOI**, *end of interrupt*, seção 5). Sem isso, o controlador não manda
mais nada.

### Os vetores do SiliconOS

| Vetores | Uso |
|---|---|
| 0x00–0x1F | exceções da CPU (doc 04). Reservados pela Intel. |
| 0x20–0x2F | o 8259 desligado (só podem aparecer interrupções "espúrias", seção 4) |
| **0x30** | **timer** do APIC local |
| **0x31** | **teclado** (IRQ 1, via I/O APIC) |
| 0xFF | interrupção espúria do APIC |

---

## 3. Onde estão os controladores? As tabelas ACPI

O APIC local fica, por padrão, no endereço físico `0xFEE0_0000`, e o I/O APIC
costuma ficar em `0xFEC0_0000`. "Costuma" não basta: numa máquina com dois
I/O APICs, ou num chipset diferente, os endereços mudam. Quem diz a verdade
é o **firmware**, por meio das tabelas **ACPI** (*Advanced Configuration and
Power Interface*): estruturas que ele deixa na memória descrevendo o
hardware da máquina (CPUs, controladores, botão de desligar, sensores...).

A tabela que interessa aqui é a **MADT** (*Multiple APIC Description
Table*). Para chegar nela há um caminho de ponteiros:

```
  UEFI: tabela de configuração
    └─ GUID "ACPI 2.0" ──► RSDP  ("RSD PTR ", endereço da XSDT)
                              └──► XSDT  (lista de endereços de tabelas)
                                     ├──► FACP
                                     ├──► APIC  ◄── a MADT (nome histórico)
                                     ├──► HPET
                                     └──► ...
```

### Passo 1: perguntar ao UEFI, antes de dispensá-lo

Só o UEFI sabe onde está a **RSDP** (*Root System Description Pointer*), o
começo do caminho. Ele guarda isso na sua "tabela de configuração", uma
lista de pares (GUID, endereço). Por isso o `main` agora chama, **antes**
do `exit_boot_services`:

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

Um GUID é um identificador de 128 bits (como um UUID do Java). O do ACPI
2.0 é fixo, definido na especificação do UEFI. Guardamos só o endereço
físico; as tabelas em si são lidas depois, quando as tabelas de página do
kernel já existem.

### Passo 2: ler as tabelas

As tabelas ACPI estão em regiões do tipo `ACPI` do mapa de memória (doc 05).
Como o doc 06 mapeou toda a RAM (menos MMIO e reservada) na metade alta, o
kernel as lê com `fisico_para_virtual`:

```rust
/// Lê um valor de tipo T no endereço FÍSICO dado, sem exigir alinhamento
/// (os campos das tabelas ACPI não são alinhados).
fn ler<T: Copy>(fisico: u64) -> T {
    unsafe { (fisico_para_virtual(fisico) as *const T).read_unaligned() }
}
```

O `read_unaligned` é necessário porque as tabelas ACPI são "compactadas":
um campo de 4 bytes pode começar num endereço ímpar
([§29](rust-basico.md#29-reprc-packed-e-read_unaligned)).

Toda tabela, menos a RSDP, começa com o mesmo **cabeçalho de 36 bytes**:

```rust
#[repr(C, packed)]
struct Cabecalho {
    assinatura: [u8; 4],   // "XSDT", "APIC", "FACP"...
    tamanho: u32,          // da tabela inteira, cabeçalho incluído
    revisao: u8,
    checksum: u8,          // ajustado para que a soma de todos os bytes dê 0
    oem: [u8; 6],          // quem fez a tabela
    oem_tabela: [u8; 8],
    oem_revisao: u32,
    criador: u32,
    criador_revisao: u32,
}
```

O **checksum** protege contra ler lixo: somando todos os bytes da tabela em
um `u8` (deixando estourar), o resultado tem que ser zero.

```rust
fn checksum_ok(fisico: u64, tamanho: u64) -> bool {
    (0..tamanho).fold(0u8, |soma, i| soma.wrapping_add(ler::<u8>(fisico + i))) == 0
}
```

A busca pela MADT é: validar a RSDP, ir para a XSDT, e percorrer a lista
de endereços de 8 bytes que vem depois do cabeçalho até achar uma tabela
cuja assinatura seja `"APIC"`:

```rust
let n = (cab.tamanho as u64 - TAMANHO_CABECALHO) / 8;
(0..n)
    .map(|i| ler::<u64>(xsdt + TAMANHO_CABECALHO + i * 8))
    .find(|&t| ler::<[u8; 4]>(t) == *assinatura)
```

### Passo 3: o que a MADT diz

Depois do cabeçalho, a MADT tem o endereço do APIC local, umas *flags* e
uma sequência de **registros de tamanho variável**, cada um começando com
`[tipo: u8][tamanho: u8]`. O SiliconOS usa quatro tipos:

| Tipo | Registro | O que tiramos dele |
|---|---|---|
| 0 | APIC local (uma CPU) | contamos as CPUs habilitadas |
| 1 | I/O APIC | id, endereço físico, primeira GSI |
| 2 | desvio de IRQ (*interrupt source override*) | IRQ ISA → GSI, polaridade, disparo |
| 5 | endereço de 64 bits do APIC local | substitui o de 32 bits |

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
    if tamanho < 2 { break; } // tabela corrompida: evita laço infinito
    p += tamanho as u64;
}
```

### GSI e desvios: a IRQ 0 não está no pino 0

O I/O APIC numera suas entradas como **GSI** (*global system interrupt*):
GSI 0, 1, 2... Seria natural que a IRQ *n* chegasse na GSI *n*, e quase
sempre é assim. As exceções vêm na MADT como **desvios**. No QEMU, o kernel
imprime:

```
ACPI (MADT): 1 CPU(s), APIC local em 0xfee00000, I/O APIC 0 em 0xfec00000 (GSI a partir de 0), 8259: sim
  desvio: IRQ 0 -> GSI 2 (ativo alto, borda)
  desvio: IRQ 5 -> GSI 5 (ativo alto, nivel)
  desvio: IRQ 9 -> GSI 9 (ativo alto, nivel)
  desvio: IRQ 10 -> GSI 10 (ativo alto, nivel)
  desvio: IRQ 11 -> GSI 11 (ativo alto, nivel)
```

O clássico é o primeiro: nos chipsets de PC, o pino 0 do I/O APIC é ligado à
saída do 8259, e o timer antigo (IRQ 0) foi para o pino **2**. A IRQ 1 (o
teclado) não aparece na lista, então chega na GSI 1 com o comportamento
padrão do barramento ISA.

Os desvios também dizem **como o sinal se comporta**:

- **polaridade**: a interrupção é "1" (ativo alto) ou "0" (ativo baixo)?
- **disparo**: por **borda** (a interrupção é a *mudança* do sinal, uma vez
  por evento) ou por **nível** (a interrupção continua *enquanto* o sinal
  estiver ativo, até o dispositivo ser atendido)?

O método `rotear_irq` junta isso:

```rust
pub fn rotear_irq(&self, irq: u8) -> Roteamento {
    let mut r = Roteamento { gsi: irq as u32, ativo_baixo: false, por_nivel: false };
    if let Some(d) = self.desvios.iter().flatten().find(|d| d.irq == irq) {
        r.gsi = d.gsi;
        r.ativo_baixo = d.flags & 0b11 == 0b11;       // 00 = padrão do barramento, 01 = alto, 11 = baixo
        r.por_nivel = (d.flags >> 2) & 0b11 == 0b11;  // 00 = padrão do barramento, 01 = borda, 11 = nível
    }
    r
}
```

O ACPI é muito maior do que isso (descreve também energia, sensores,
desligamento...). Aqui lemos só o necessário para as interrupções.

---

## 4. Desligando o 8259

Mesmo sem uso, o 8259 precisa ser **desligado direito**, por dois motivos:

1. Na configuração original do PC, o 8259 mestre entregava as IRQs 0–7 nos
   vetores **0x08–0x0F**. No modo protegido, esses vetores são **exceções**
   da CPU: a IRQ 0 do timer chegaria como se fosse um *double fault* (vetor
   8). O firmware de hoje pode ter configurado outra coisa, mas o kernel não
   deve depender disso.
2. Mesmo mascarado, o 8259 pode gerar uma interrupção **espúria** (uma
   interrupção "fantasma", quando um sinal some no meio do caminho). Ela
   chega como a IRQ 7 (ou 15) do chip.

A solução tem dois passos: **remapear** os 16 vetores para 0x20–0x2F (que
não colidem com nada) e **mascarar** todas as linhas. A programação do
8259 é uma sequência de 4 bytes chamados ICW (*initialization command
words*), enviados pelas portas de I/O 0x20/0x21 (mestre) e 0xA0/0xA1
(escravo):

```rust
// src/interrupcoes/pic.rs
outb(PIC1_COMANDO, 0x11); esperar();          // ICW1: inicializar, haverá ICW4
outb(PIC2_COMANDO, 0x11); esperar();
outb(PIC1_DADOS, VETOR_BASE); esperar();      // ICW2: vetor da IRQ 0  -> 0x20
outb(PIC2_DADOS, VETOR_BASE + 8); esperar();  //       vetor da IRQ 8  -> 0x28
outb(PIC1_DADOS, 1 << 2); esperar();          // ICW3: o escravo está no pino 2 do mestre
outb(PIC2_DADOS, 2); esperar();               //       o escravo é o número 2
outb(PIC1_DADOS, 0x01); esperar();            // ICW4: modo 8086
outb(PIC2_DADOS, 0x01); esperar();

// Máscara 0xFF = todas as 8 linhas de cada chip desligadas.
outb(PIC1_DADOS, 0xFF);
outb(PIC2_DADOS, 0xFF);
```

O `esperar()` escreve na porta 0x80, que ninguém usa, só para gastar cerca
de 1 µs: o 8259 original precisava de um respiro entre comandos. A IDT
ganha um tratador para os vetores 0x27 e 0x2F que só conta as espúrias e
não faz mais nada.

A saída mostra que o firmware (OVMF) já tinha mascarado tudo:

```
PIC 8259: mascaras do firmware 0xff 0xff -> remapeado para 0x20-0x2f e todo mascarado
```

---

## 5. O APIC local

### Onde ele está: um MSR

Além da MADT, a própria CPU diz onde está o seu APIC local, num **MSR**
(*model-specific register*). MSRs são centenas de registradores de
configuração identificados por **número**, lidos com a instrução `rdmsr`: o
número vai em ECX e o valor de 64 bits volta dividido em EDX (alto) e EAX
(baixo).

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

O MSR `0x1B` (`IA32_APIC_BASE`) tem o endereço físico do APIC local nos bits
12 em diante, e no bit 11 diz se ele está ligado. O `apic::init` confere
que o MSR e a MADT concordam:

```rust
let msr = cpu::ler_msr(IA32_APIC_BASE);
let fisico = msr & APIC_BASE_ENDERECO;
assert!(msr & APIC_BASE_LIGADO != 0, "APIC local desligado no MSR");
assert_eq!(fisico, fisico_madt, "MSR e MADT discordam sobre o endereco do APIC local");
```

### Registradores na memória: MMIO sem cache

Os registradores do APIC local não são acessados com instruções especiais:
são **MMIO** (doc 05, glossário), uma página de 4 KiB em que cada
registrador de 32 bits ocupa uma posição alinhada em 16 bytes. Escrever em
`base + 0xB0` é dar uma ordem ao APIC.

Esse endereço (`0xFEE0_0000`) não é RAM, então **não** foi mapeado no doc
06. O SiliconOS ganha uma faixa virtual só para dispositivos, ao lado do
heap:

```
0xFFFF_8000_0000_0000   toda a RAM física (doc 06)        PML4 256
0xFFFF_9000_0000_0000   heap (doc 07)                     PML4 288
0xFFFF_A000_0000_0000   MMIO: APIC local, I/O APIC...     PML4 320   ◄── novo
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

O bit importante é **`SEM_CACHE`**. Com cache, uma escrita no registrador de
EOI poderia ficar parada no cache da CPU e nunca chegar ao APIC, e uma
leitura do contador do timer poderia devolver um valor velho. Para um
dispositivo, cada leitura e cada escrita precisam **acontecer de verdade**.
Pelo mesmo motivo, todo acesso usa `read_volatile`/`write_volatile` (doc
01), que impede o compilador de juntar, adiar ou eliminar acessos:

```rust
pub fn escrever(registrador: u32, valor: u32) {
    let base = BASE.load(Ordering::Relaxed);
    unsafe { ((base + registrador as u64) as *mut u32).write_volatile(valor) };
}
```

### Ligando

```rust
// Prioridade 0: aceitar todas as interrupções.
escrever(REG_PRIORIDADE, 0);
// LINT0 é o pino onde o 8259 antigo entregaria as IRQs: não usamos mais.
escrever(REG_LVT_LINT0, LVT_MASCARADA);
// Liga o APIC (bit 8) e define o vetor das interrupções espúrias.
escrever(REG_ESPURIO, SVR_APIC_LIGADO | vetor_espurio as u32);
```

Três registradores:

- **TPR** (`0x80`, *task priority*): o APIC só entrega interrupções de
  prioridade **acima** deste valor. Zero = todas.
- **LVT LINT0** (`0x350`): a *local vector table* tem uma entrada para cada
  fonte de interrupção **local** (o timer, os pinos LINT0 e LINT1, erros...).
  LINT0 é por onde o 8259 chegaria; mascaramos.
- **SVR** (`0xF0`, *spurious vector register*): o bit 8 **liga** o APIC, e os
  bits 0–7 dizem em que vetor entregar uma interrupção espúria (usamos 0xFF).

```
APIC local: id 0, versao 0x14, 6 entradas LVT; fisico 0xfee00000 -> virtual 0xffffa00000000000
```

### EOI: "terminei"

Quando o APIC entrega uma interrupção, ele liga um bit dela num registrador
interno (o ISR, *in-service register*). Enquanto esse bit estiver ligado, o
APIC **não entrega** outra interrupção da mesma **classe de prioridade** ou
de classe menor. A classe é o dígito hexadecimal alto do vetor: 0x30 e 0x31
são ambos da classe 3.

O tratador apaga o bit escrevendo qualquer valor no registrador de EOI:

```rust
pub fn eoi() {
    escrever(REG_EOI, 0);
}
```

Esquecer o EOI é um dos erros mais comuns em OSDev, e o teste
`teste-sem-eoi` (seção 11.3) mostra o resultado: **tudo para**.

---

## 6. O I/O APIC

O I/O APIC tem uma interface curiosa: só **dois** registradores MMIO. Um é
o **seletor** (`IOREGSEL`, no deslocamento 0x00); o outro é a **janela**
(`IOWIN`, em 0x10). Para ler o registrador interno N, escreve-se N no
seletor e lê-se a janela:

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

O registrador 1 (versão) diz, nos bits 16–23, quantos pinos o chip tem. Cada
pino tem uma **entrada de redirecionamento** de 64 bits, dividida em dois
registradores de 32 (`0x10 + 2n` e `0x10 + 2n + 1`):

```
 63      56 55                17 16  15  14  13  12  11  10  8  7        0
┌──────────┬─────────────────────┬───┬───┬───┬───┬───┬───┬─────┬──────────┐
│ destino  │      reservado      │ M │ N │   │ B │   │ F │modo │  vetor   │
│(id APIC) │                     │   │   │   │   │   │   │     │          │
└──────────┴─────────────────────┴───┴───┴───┴───┴───┴───┴─────┴──────────┘
 M = mascarado   N = disparo por nível   B = ativo baixo
 F = destino lógico (0 = físico: "destino" é o id de um APIC)
 modo 000 = fixo (entrega normal)
```

O `init` mascara todos os 24 pinos. Depois, cada dispositivo é liberado com
`rotear`:

```rust
pub fn rotear(gsi: u32, vetor: u8, destino: u8, ativo_baixo: bool, por_nivel: bool) {
    let pino = gsi - GSI_BASE.load(Ordering::Relaxed) as u32;
    let mut baixo = vetor as u32; // modo fixo, destino físico, não mascarada
    if ativo_baixo { baixo |= ATIVO_BAIXO; }
    if por_nivel { baixo |= POR_NIVEL; }
    // Primeiro a metade alta (destino), depois a baixa (que desmascara).
    escrever(REG_REDIRECIONAMENTO + 2 * pino + 1, (destino as u32) << 24);
    escrever(REG_REDIRECIONAMENTO + 2 * pino, baixo);
}
```

A ordem das duas escritas importa: a metade baixa tem o bit de máscara.
Se ela fosse escrita primeiro, o pino ficaria ligado por um instante com o
destino antigo.

Para o teclado, a entrada fica `0x0000000000000031`: vetor 0x31, destino o
APIC 0, ativo alto, borda, não mascarada.

```
I/O APIC: id 0, versao 0x20, 24 pinos; virtual 0xffffa00000001000
```

---

## 7. O timer

### Qual timer?

Um PC tem vários relógios. Os três que aparecem neste capítulo:

| Relógio | Desde | Para que o usamos |
|---|---|---|
| **PIT** 8253/8254 | 1981 | cronômetro de referência: frequência fixa e conhecida |
| **timer do APIC local** | anos 1990 | o tique do kernel: 100 interrupções por segundo |
| **RTC** (relógio CMOS) | 1984 | conferir o timer no teste (conta segundos de verdade, com bateria) |

O timer do APIC é o escolhido para o tique: fica dentro da CPU (cada CPU
tem o seu, o que vai importar com várias CPUs) e não depende do I/O APIC.
O problema dele: ele conta numa frequência que **varia de máquina para
máquina** (o clock do barramento), e nenhum registrador diz qual é.

### Calibrar: cronometrar com o PIT

A solução é medir. O PIT conta numa frequência fixa de **1 193 182 Hz** (o
cristal de 14,318 MHz do PC original dividido por 12). Programamos o
**canal 2** do PIT para contar 10 ms (11 931 pulsos), soltamos o timer do
APIC ao mesmo tempo contando para baixo a partir do máximo, e vemos quanto
ele andou:

```rust
// src/interrupcoes/timer.rs
// Gate do canal 2 ligado, alto-falante desligado.
outb(PORTA_61, (inb(PORTA_61) & !0b10) | 0b01);
// Canal 2, byte baixo e depois o alto, modo 0 ("interromper ao
// terminar": a saída fica em 0 e vai a 1 quando a contagem chega a 0).
outb(PIT_COMANDO, 0b10_11_000_0);
outb(PIT_CANAL2, contagem_pit as u8);
// A partir desta escrita o PIT começa a contar. Disparamos o APIC junto.
apic::escrever(apic::REG_TIMER_DIVISOR, 0b0011); // divide o clock por 16
apic::escrever(apic::REG_LVT_TIMER, apic::LVT_MASCARADA); // contar sem interromper
outb(PIT_CANAL2, (contagem_pit >> 8) as u8);
apic::escrever(apic::REG_TIMER_INICIAL, u32::MAX);

// Espera a saída do canal 2 subir (10 ms depois).
while inb(PORTA_61) & 0x20 == 0 {
    core::hint::spin_loop();
}
```

Por que o canal 2? Os canais 0 e 1 do PIT são, respectivamente, ligados à
IRQ 0 e ao antigo refresh de memória. O canal 2 é o do **alto-falante** do
PC (o "bip"), e tem uma vantagem: a sua saída pode ser **lida** no bit 5 da
porta 0x61. Dá para cronometrar sem nenhuma interrupção, só olhando esse
bit. (O bit 1 da mesma porta ligaria o alto-falante; o código o deixa em 0
para o PC não apitar.)

O byte de comando `0b10_11_000_0` é lido em partes: `10` = canal 2, `11` =
vamos mandar o byte baixo e depois o alto, `000` = modo 0, `0` = contar em
binário.

No QEMU:

```
Timer do APIC: 624954 contagens em 10 ms = 62 MHz (barramento de ~999 MHz / 16)
  modo periodico: 100 Hz no vetor 0x30
```

O timer do APIC andou ~625 000 em 10 ms: 62,5 milhões de contagens por
segundo, com o divisor 16. O clock de entrada é, portanto, ~1 GHz, que é
o valor que o QEMU emula. Numa máquina real o número seria outro, e é
exatamente por isso que se calibra.

### Modo periódico

Com a medida, o timer é programado para recarregar sozinho e interromper
100 vezes por segundo:

```rust
pub fn iniciar(vetor: u8, contagens_10ms: u32) {
    const PERIODICO: u32 = 1 << 17; // bits 17-18 da LVT do timer: 01 = periódico
    let por_tique = contagens_10ms as u64 * 100 / FREQUENCIA_HZ;
    apic::escrever(apic::REG_TIMER_DIVISOR, 0b0011);
    apic::escrever(apic::REG_LVT_TIMER, PERIODICO | vetor as u32);
    apic::escrever(apic::REG_TIMER_INICIAL, por_tique as u32);
}
```

E o tratador é o menor possível:

```rust
pub extern "x86-interrupt" fn tratador(_quadro: QuadroInterrupcao) {
    let t = TIQUES.fetch_add(1, Ordering::Relaxed) + 1;
    // ... (trechos usados só por features de teste)
    apic::eoi();
}
```

`TIQUES` é um `AtomicU64` ([§25](rust-basico.md#25-atômicos-atomicu64-atomicbool)).
A 100 Hz, ele leva 5,8 bilhões de anos para estourar. Com ele, o kernel
passa a ter `timer::tiques()` e `timer::milissegundos()`.

> Por que 100 Hz e não 1000? Cada interrupção custa algumas centenas de
> instruções (empilhar, salvar registradores, EOI, voltar). 100 Hz é o valor
> clássico do Linux antigo e mais do que suficiente por enquanto. Quando houver
> troca de processos, esse número vira uma decisão de projeto: maior =
> resposta mais rápida, menor = menos tempo gasto com interrupções.

---

## 8. O teclado

### Do dedo à porta 0x60

O teclado PS/2 (o conector redondo roxo, que o QEMU emula) conversa com um
**controlador** na placa-mãe, o "8042". Cada vez que uma tecla é apertada
**ou solta**, o controlador:

1. coloca um byte, o **scancode**, na porta de I/O **0x60**;
2. liga o bit 0 da porta de estado 0x64 ("há dado esperando");
3. dispara a **IRQ 1**.

O scancode diz **qual tecla** (a posição física), não qual letra. A mesma
tecla é "A" num teclado americano e "Q" num francês. Traduzir é trabalho do
sistema operacional.

### Configurando o controlador

O controlador tem um **byte de configuração** que diz, entre outras coisas,
se ele deve gerar a IRQ 1. O `teclado::init` lê esse byte (comando 0x20),
liga os bits que importam e o grava de volta (comando 0x60):

```rust
const CONFIG_IRQ1: u8 = 1 << 0;      // gerar IRQ 1 quando chega byte do teclado
const CONFIG_TRADUCAO: u8 = 1 << 6;  // traduzir para o "scancode set 1"

enviar_comando(0x20); // "ler byte de configuração"
let antes = ler_dado();
let depois = antes | CONFIG_IRQ1 | CONFIG_TRADUCAO;
enviar_comando(0x60); // "escrever byte de configuração"
enviar_dado(depois);
```

Antes disso, ele joga fora os bytes que tenham sobrado do firmware (uma
tecla apertada durante o boot ficaria na fila e confundiria o começo).

```
Teclado PS/2: config 0x67 -> 0x67, 0 bytes velhos descartados
  IRQ 1 -> GSI 1 -> vetor 0x31; entrada do I/O APIC: 0x0000000000000031
```

O OVMF já tinha deixado `0x67` = `0110 0111`: bit 0 (IRQ 1) e bit 6
(tradução) já ligados, além do bit 1 (IRQ 12, do mouse), bit 2 (o teste do
sistema passou) e bit 5 (relógio da porta do mouse desligado). O `init`
liga os bits mesmo assim: noutra máquina eles poderiam estar desligados.

### Scancodes: apertar, soltar e o prefixo 0xE0

Com a tradução ligada, os scancodes seguem o **set 1** (o do PC XT original;
o teclado envia o set 2, e o controlador traduz):

- apertar uma tecla gera o código dela (*make code*): A = `0x1E`;
- soltar gera o mesmo código com o bit 7 ligado (*break code*): `0x9E`;
- teclas que vieram depois (setas, Ctrl direito, *Home*...) mandam antes
  um byte **`0xE0`**.

Com `./run.sh teste-teclado`, o kernel mostra cada byte. Shift + A e depois
a seta para a direita:

```
  scancode 0x2a       Shift esquerdo apertado
  scancode 0x1e       A apertado            -> 'A'
A  scancode 0x9e      A solto (0x1E | 0x80)
  scancode 0xaa       Shift solto (0x2A | 0x80)
  scancode 0xe0       prefixo estendido
  scancode 0x4d       seta direita apertada
  scancode 0xe0
  scancode 0xcd       seta direita solta
```

(As explicações à direita foram acrescentadas aqui; o "A" no começo de
uma linha é o eco do caractere.)

### O tratador: só guardar

Um tratador de interrupção precisa ser **curto**: enquanto ele roda, as
interrupções estão desligadas (é um *interrupt gate*, doc 04) e o timer
espera. Por isso o tratador do teclado não traduz nem imprime nada. Ele lê
o byte e o põe numa **fila**:

```rust
pub extern "x86-interrupt" fn tratador(_quadro: QuadroInterrupcao) {
    let scancode = unsafe { inb(PORTA_DADOS) };
    RECEBIDOS.fetch_add(1, Ordering::Relaxed);

    let e = ESCRITA.load(Ordering::Relaxed);
    let proxima = (e + 1) % TAMANHO_FILA;
    if proxima == LEITURA.load(Ordering::Acquire) {
        PERDIDOS.fetch_add(1, Ordering::Relaxed); // cheia: descarta
    } else {
        FILA[e].store(scancode, Ordering::Relaxed);
        // Release: quem ler o novo ESCRITA também vê o byte gravado acima.
        ESCRITA.store(proxima, Ordering::Release);
    }
    apic::eoi();
}
```

Ler a porta 0x60 é obrigatório: enquanto o byte não for lido, o controlador
não manda o próximo.

A fila é um **buffer circular** de 64 bytes com duas posições: `ESCRITA`
(só o tratador mexe) e `LEITURA` (só o laço principal mexe). Vazia quando as
duas são iguais; cheia quando a próxima escrita alcançaria a leitura:

```
 FILA:  [ . . . 1E 9E 2A 1E . . . . ]
                 ▲           ▲
              LEITURA     ESCRITA
           (próximo a ler) (próximo a escrever)
```

Ela **não usa lock**. Isso é possível porque há exatamente um produtor e um
consumidor, e cada posição só é alterada por um lado. Um `Mutex` aqui seria
um convite ao deadlock da seção 10. O par `Release`/`Acquire` garante que
quem vê a posição nova também vê o byte gravado antes dela
([§28](rust-basico.md#28-orderingacquire-e-release)).

### O laço principal: tirar, traduzir e mostrar

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

O `Decodificador` lembra o estado dos Shifts e do Caps Lock e traduz com
duas tabelas indexadas pelo scancode, uma sem e outra com Shift:

```rust
const NORMAL: &[u8; 58] =
    b"\0\x1b1234567890-=\x08\tqwertyuiop[]\n\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
const COM_SHIFT: &[u8; 58] =
    b"\0\x1b!@#$%^&*()_+\x08\tQWERTYUIOP{}\n\0ASDFGHJKL:\"~\0|ZXCVBNM<>?\0*\0 ";
```

A posição 0x1E da primeira tabela é `a`; a 0x02 é `1`, e na segunda, `!`.
O tipo `&[u8; 58]` faz o compilador conferir que as duas têm exatamente 58
bytes (de 0x00 a 0x39, a barra de espaço).

O layout é o **americano**, que é o que o QEMU emula por padrão. Num teclado
ABNT2 de verdade, algumas teclas (ç, acentos, `/`) sairiam trocadas. Além
disso, a fonte do console só tem ASCII. Um layout brasileiro fica para
depois.

O console ganhou o caractere *backspace* (`0x08`): volta uma coluna e
desenha um espaço por cima. Na serial, o kernel manda "volta, espaço,
volta", que é como os terminais apagam.

### Dormir sem perder tecla: `sti; hlt`

O `teclado::esperar` põe a CPU para dormir (`hlt`) até a próxima
interrupção, mas tem um detalhe sutil:

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

Se fosse só `if fila_vazia() { hlt }`, uma tecla poderia chegar **entre** o
teste e o `hlt`: o tratador põe o byte na fila, volta, e a CPU vai dormir
com uma tecla esperando. Ela só acordaria no próximo tique do timer, e se
não houvesse timer, nunca.

A solução usa uma regra da CPU: depois de um `sti`, as interrupções só são
aceitas **depois da instrução seguinte**. Então `sti; hlt` é atômico: com as
interrupções desligadas, confere a fila; se estiver vazia, liga as
interrupções e dorme **na mesma tacada**. Uma tecla que chegar nesse meio
tempo acorda o `hlt`.

---

## 9. Juntando tudo

A IDT ganha as entradas novas (doc 04):

```rust
// src/idt.rs
use crate::interrupcoes::{self as int, teclado, timer};
t[int::VETOR_TIMER as usize] = Entrada::new(timer::tratador as *const () as u64, 0);
t[int::VETOR_TECLADO as usize] = Entrada::new(teclado::tratador as *const () as u64, 0);
for v in [int::VETOR_PIC_ESPURIO_1, int::VETOR_PIC_ESPURIO_2, int::VETOR_APIC_ESPURIO] {
    t[v as usize] = Entrada::new(int::espuria as *const () as u64, 0);
}
```

E `interrupcoes::init`, chamado no `kernel_main` depois do heap, faz tudo na
ordem:

```
interrupcoes::init()
  1. acpi::madt()           onde estão os controladores, desvios de IRQ
  2. pic::desligar()        8259 remapeado para 0x20 e mascarado
  3. apic::init()           mapeia 0xFEE0_0000 sem cache, liga o APIC local
  4. ioapic::init()         mapeia 0xFEC0_0000, mascara os 24 pinos
  5. timer::calibrar()      10 ms do PIT  →  quantas contagens do APIC
     timer::iniciar()       modo periódico, 100 Hz, vetor 0x30
  6. teclado::init()        IRQ 1 ligada no controlador PS/2
     ioapic::rotear()       GSI 1 → vetor 0x31 → APIC 0
  7. sti                    a partir daqui, as interrupções chegam
```

A ordem não é arbitrária: o `sti` vem **por último**, quando todo vetor que
pode chegar já tem tratador e todo controlador está num estado conhecido.

---

## 10. O perigo novo: deadlock

Até aqui, o código do kernel rodava do começo ao fim sem ser interrompido.
Agora, **entre quaisquer duas instruções**, um tratador pode rodar. Isso
cria um problema que não existia.

O `kprintln!` usa o `CONSOLE`, protegido por um spinlock (`spin::Mutex`,
[§11](rust-basico.md#11-mutex-e-once-da-crate-spin)). Imagine um tratador
que também imprime:

```
 laço principal                          tratador do timer
 ──────────────                          ─────────────────
 kprintln!("linha 14")
   CONSOLE.lock()      ← pega o lock
   desenhando letra...
        ┊  ◄────── interrupção do timer ──────
        ┊                                 kprint!("[tique 10]")
        ┊                                   CONSOLE.lock()  ← ocupado!
        ┊                                   espera... espera... espera...
        ┊
   (nunca volta a rodar: a CPU está presa no tratador,
    e o tratador espera um lock que só o laço principal solta)
```

Numa aplicação com threads, o "dono" do lock continuaria rodando noutra
thread e acabaria soltando. Aqui não: o dono foi **interrompido**, e só
volta quando o tratador terminar. O tratador nunca termina. **Deadlock.**

### A proteção: interrupções desligadas enquanto o lock está pego

A solução é simples: enquanto o `_print` segura o lock, **nenhuma
interrupção entra**. Se um tratador quiser imprimir, ele espera o
`kprintln!` atual terminar, e não o contrário.

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

Dois detalhes:

- `sem_interrupcoes` **restaura** o estado anterior em vez de sempre ligar.
  Dentro de um tratador as interrupções já estão desligadas, e um `sti` ali
  no meio permitiria que uma interrupção interrompesse o próprio tratador.
- O heap tem o mesmo risco (um tratador que alocasse enquanto o laço
  principal está no meio de um `Vec::push`), então `alloc` e `dealloc` do
  doc 07 também passaram a usar `sem_interrupcoes`.

O tratador do teclado nem precisa disso, porque não usa lock nenhum (a fila
da seção 8). É a regra geral: **tratadores devem fazer o mínimo** e deixar
o resto para o código normal.

### `cli` e `sti` também são barreiras para o compilador

O compilador pode reordenar acessos à memória quando acha que isso não muda
o resultado. Se ele achar que `sti` não mexe em memória, pode mover a
liberação do lock (uma escrita na memória) para **depois** do `sti`, e o
problema volta. Por isso as funções de `cpu.rs` **não** usam
`options(nomem)` no `asm!` ([§15](rust-basico.md#15-asm-assembly-dentro-do-rust)):

```rust
// Sem `options(nomem)` de propósito: assim o compilador trata `cli` e `sti`
// como se mexessem em qualquer memória e NÃO move leituras e escritas
// (como a liberação de um lock) para o outro lado deles.
pub fn ligar_interrupcoes() {
    unsafe { asm!("sti", options(nostack)) };
}
```

---

## 11. Como foi testado

### 11.1 O boot normal

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

Para testar o teclado sem ninguém digitando, o QEMU foi iniciado com o
**monitor** num socket (`-monitor unix:...`) e as teclas foram enviadas com
o comando `sendkey` do monitor: `o l a spc shift-m u n d o shift-1 ret a b c
x backspace backspace d caps_lock t e s t e caps_lock spc shift-2 ret`. Na
serial e na tela:

```
ola Mundo!
abdTESTE @
```

Shift, Caps Lock (que só afeta letras: o `@` continuou dependendo do Shift)
e os dois *backspaces* apagando `x` e `c` funcionaram. Na janela normal do
QEMU, basta clicar nela e digitar.

### 11.2 `./run.sh teste-timer`: o timer contra o relógio CMOS

O RTC é um relógio independente, com cristal e bateria próprios, que conta
segundos de verdade. O teste espera o segundo do RTC virar e conta quantos
tiques acontecem durante cada um dos 5 segundos seguintes:

```
Teste timer: contando tiques a cada segundo do RTC (esperado: 100)
  segundo 39 do RTC: 100 tiques
  segundo 40 do RTC: 100 tiques
  segundo 41 do RTC: 100 tiques
  segundo 42 do RTC: 100 tiques
  segundo 43 do RTC: 100 tiques
Teste timer OK: media de 100 tiques/s; 5720 ms desde que o timer ligou; espurias: 0
```

A calibração com o PIT bateu com o RTC. O teste aceita até 10% de diferença,
porque o QEMU sem KVM às vezes atrasa: noutra execução, um dos segundos
teve 89 tiques (média 98), e com `RAM=4G` a média foi 97. Nenhuma
interrupção espúria.

### 11.3 `./run.sh teste-sem-eoi`: esquecer o EOI

O tratador do timer é compilado sem a linha `apic::eoi()`. O teste espera 2
segundos pelo RTC (sem `hlt`: se o timer parar, nada mais acordaria a CPU):

```
Teste: tratador do timer SEM EOI. Esperando 2 s pelo RTC...
  tiques depois de 2 s: 1 (com EOI seriam ~200)
  teclas recebidas: 0 (o teclado tambem ficou bloqueado?)
```

**Um** tique, e mais nada. E a resposta da pergunta é **sim**: com o
`sendkey` mandando `o l a ret` depois disso, nada apareceu na tela. O vetor
do teclado (0x31) é da mesma classe de prioridade do timer (0x30), e o APIC,
achando que a interrupção 0x30 ainda está sendo tratada, segura as duas.

### 11.4 `./run.sh teste-print-interrupcao`: imprimir dentro do tratador

O laço principal imprime linhas sem parar, e o tratador do timer imprime
`[tique N]` a cada 10 tiques. Com a proteção do `_print`:

```
[tique 10]laco principal: linha 16 (tiques: 10)
[tique 20]laco principal: linha 35 (tiques: 20)
[tique 30]laco principal: linha 54 (tiques: 30)
...
[tique 300]Teste OK: 300 tiques e 561 linhas sem travar.
```

O `[tique N]` sempre aparece **entre** duas linhas, nunca no meio de uma:
é a proteção funcionando (a interrupção espera o `kprintln!` terminar).

Um achado durante o teste: na primeira versão, o laço principal não tinha
pausa nenhuma entre um `kprintln!` e o próximo, e o timer **parou no
terceiro tique**. As interrupções ficavam ligadas só por umas poucas
instruções entre um `kprintln!` e outro. O mesmo binário rodando com KVM
(`qemu-system-x86_64 -enable-kvm`, a CPU real executando o código) chegou
aos 300 tiques sem pausa nenhuma. O que mudava era o QEMU sem KVM (o modo
TCG, que traduz as instruções): ele confere se há interrupção pendente em
pontos específicos (entre blocos de instruções traduzidas), e uma janela tão
curta escapava sempre. Uma pausa de 1000 voltas de `spin_loop` com as
interrupções ligadas resolveu. Fica a lição: **o emulador não é o
hardware**, e um teste que depende de tempo pode se comportar diferente
nele.

### 11.5 `./run.sh teste-deadlock`: sem a proteção

O mesmo teste, mas compilado sem o `sem_interrupcoes` no `_print`. A serial
termina assim, e o QEMU continua ligado, mas parado:

```
laco principal: linha 13 (tiques: 9)
laco principal: linha 14 (tiques: 9)
[tique 10]
```

O `[tique 10]` aparece porque a serial é escrita **antes** de pegar o lock
do CONSOLE. Logo depois, o tratador tenta pegar o lock, que o laço principal
(interrompido no meio da linha 15) está segurando. O monitor do QEMU
confirma:

```
(qemu) info registers
RIP=000000000605ce12 RFL=00000002 ...
(qemu) x /24xb 0x605ce00
...
000000000605ce10: 0xf3 0x90 0xc3 0xcc ...
```

- `RFL=00000002`: o bit IF (bit 9) está **desligado**, ou seja, a CPU está
  dentro de um tratador (o *interrupt gate* desligou as interrupções);
- consultado várias vezes, o RIP é sempre o mesmo, e os bytes ali são
  `f3 90` (`pause`) e `c3` (`ret`): a função `core::hint::spin_loop`, que o
  spinlock chama a cada volta enquanto espera. A CPU está girando para
  sempre num lock que nunca vai ser solto.

### 11.6 Os testes anteriores

Todas as 21 features (as 17 dos capítulos anteriores, o boot normal e as
novas `teste-timer`, `teste-sem-eoi` e `teste-print-interrupcao`) foram
rodadas de novo, e todas deram o resultado esperado. A `teste-deadlock`
trava, como deve.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 07 (tag `doc-07`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-08`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-07...doc-08).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/cpu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/cpu.rs) | **novo**: `ligar_interrupcoes`, `desligar_interrupcoes`, `interrupcoes_ligadas`, `sem_interrupcoes`, `ler_msr` | — |
| [`src/acpi.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/acpi.rs) | **novo**: `guardar_rsdp`, `Cabecalho`, `Madt` (com `rotear_irq`), `madt`, `procurar_tabela`, `ler_madt` | — |
| [`src/interrupcoes/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/mod.rs) | **novo**: os vetores, `espuria`, `init` (a sequência ACPI → 8259 → APIC → I/O APIC → timer → teclado → `sti`) | — |
| [`src/interrupcoes/pic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/pic.rs) | **novo**: `desligar` | — |
| [`src/interrupcoes/apic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/apic.rs) | **novo**: registradores, `ler`/`escrever`, `eoi`, `id`, `init` | — |
| [`src/interrupcoes/ioapic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/ioapic.rs) | **novo**: `init`, `rotear`, `entrada` | — |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/timer.rs) | **novo**: `TIQUES`, `tratador`, `calibrar` (PIT), `iniciar`, `segundos_rtc` | — |
| [`src/interrupcoes/teclado.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/interrupcoes/teclado.rs) | **novo**: a fila sem lock, `tratador`, `proximo_scancode`, `esperar`, `init` (8042), `Decodificador` | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | `mod acpi; mod cpu;` e `#[allow(dead_code)] mod interrupcoes;` | junto dos outros `mod` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | `_print` passa a ser `cpu::sem_interrupcoes(\|\| escrever(args))`; o corpo antigo vira `fn escrever` (com `#[cfg]` para a feature `teste-deadlock`) | `fn _print` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | `acpi::guardar_rsdp();` | em `main()`, antes do `exit_boot_services` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/main.rs) | em `kernel_main`: `interrupcoes::init(); testes_de_interrupcoes();`; o `loop { hlt }` final dá lugar a `ecoar_teclado()`; funções novas `ecoar_teclado` e `testes_de_interrupcoes` | depois de `testes_de_heap()` |
| [`src/idt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/idt.rs) | entradas dos vetores 0x30 (timer), 0x31 (teclado) e das espúrias (0x27, 0x2F, 0xFF) | em `init`, depois da linha do `PAGE_FAULT` |
| [`src/memoria/paginacao.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/memoria/paginacao.rs) | `MMIO_INICIO`, `PROXIMO_MMIO` e `mapear_mmio` | antes de `alocar_e_mapear` |
| [`src/memoria/heap.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/memoria/heap.rs) | o corpo de `alloc` e de `dealloc` dentro de `crate::cpu::sem_interrupcoes(...)` | `impl GlobalAlloc for AlocadorLista` |
| [`src/console.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/console.rs) | braço `0x08` (backspace) no `match` | `write_byte` |
| [`src/serial.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/src/serial.rs) | `match` com `\n` (manda `\r\n`) e `0x08` (manda volta, espaço, volta) | `write_str` |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-08/Cargo.toml) | `teste-timer`, `teste-teclado`, `teste-sem-eoi`, `teste-print-interrupcao`, `teste-deadlock = ["teste-print-interrupcao"]` | `[features]` |

---

## Diferença em relação ao livro

O capítulo 9 do livro trata do mesmo tema: fazer o hardware avisar o kernel
por interrupções, com o timer e o teclado como exemplos. Tutoriais de OSDev
costumam começar pelo 8259, que é mais simples de programar (o timer na IRQ
0 e o teclado na IRQ 1 chegam direto, sem ACPI). Vale comparar ao ler.

As particularidades do SiliconOS:

- vai direto ao **APIC local + I/O APIC**, que é o que um sistema moderno
  usa (e o que vai permitir usar várias CPUs), e desliga o 8259;
- descobre os endereços e os desvios de IRQ lendo a **MADT** à mão, sem a
  crate `acpi`;
- não usa crates prontas para o PIC nem para traduzir scancodes: os
  registradores e as tabelas estão no código, com os bits nomeados;
- calibra o timer do APIC com o **PIT** e confere o resultado com o **RTC**.

---

## Resumo

- Uma **interrupção de hardware** (IRQ) é o *callback* do hardware: o
  dispositivo avisa, a CPU interrompe o que está fazendo, roda o tratador da
  IDT e volta.
- O PC tem o **8259** (1981, uma CPU, 15 IRQs) e o **APIC** (um APIC local
  por CPU, com timer, e um I/O APIC para os dispositivos). O SiliconOS
  remapeia o 8259 para 0x20–0x2F, mascara tudo e usa o APIC.
- Os endereços vêm das tabelas **ACPI**: UEFI → RSDP → XSDT → **MADT**. A
  MADT também traz os **desvios** de IRQ (no QEMU, a IRQ 0 vai para a GSI 2).
- Os registradores do APIC são **MMIO**, mapeados numa faixa própria
  (`0xFFFF_A000_...`) com **SEM_CACHE**.
- Todo tratador termina com **EOI**; sem ele, o APIC bloqueia aquela classe
  de prioridade (o timer parou no 1º tique, e o teclado junto).
- O **timer do APIC** foi calibrado com o **PIT** (10 ms) e bate 100 vezes
  por segundo; o RTC confirmou: 100 tiques por segundo.
- O **teclado** manda scancodes pela porta 0x60. O tratador só os põe numa
  **fila sem lock**; o laço principal traduz (set 1, layout americano) e
  mostra. `sti; hlt` dorme sem perder tecla.
- Um tratador que pede um lock que o código interrompido segura causa
  **deadlock**. O `_print` e o heap desligam as interrupções enquanto
  seguram seus locks; o `teste-deadlock` mostra a CPU presa no `pause` do
  spinlock sem essa proteção.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Qual a vantagem de uma interrupção sobre o *polling*?**
   Com *polling*, a CPU gasta o tempo todo perguntando ao dispositivo se há
   novidade. Com interrupção, ela faz outra coisa (ou dorme com `hlt`) e só
   é chamada quando algo acontece.

2. **Por que o 8259 precisa ser remapeado, mesmo que vá ficar mascarado?**
   Porque na configuração original ele usa os vetores 0x08–0x0F, que são
   exceções da CPU, e mesmo mascarado ele pode gerar interrupções espúrias
   (IRQ 7 ou 15). Remapeado para 0x20–0x2F, uma espúria cai num vetor
   inofensivo, com tratador próprio.

3. **Como o kernel sabe que o I/O APIC está em `0xFEC0_0000` e que a IRQ 0
   chega na GSI 2?**
   Pela MADT, uma tabela ACPI. O caminho é: o UEFI informa a RSDP (lida
   antes do `exit_boot_services`), a RSDP aponta para a XSDT, e a XSDT lista
   as tabelas; a de assinatura `"APIC"` é a MADT. O registro de tipo 1 dá o
   I/O APIC; o de tipo 2 dá o desvio IRQ 0 → GSI 2.

4. **Por que os registradores do APIC são mapeados com `SEM_CACHE`?**
   Porque são de um dispositivo: uma escrita (como o EOI) tem que chegar a
   ele de verdade, e uma leitura (como o contador do timer) tem que trazer o
   valor atual. Com cache, elas poderiam ficar no cache da CPU.

5. **Por que o timer do APIC precisa ser calibrado e o PIT não?**
   O timer do APIC conta no clock do barramento, que muda de máquina para
   máquina e não é informado em nenhum registrador. O PIT tem frequência
   fixa (1 193 182 Hz) desde o PC original, então serve de cronômetro para
   medir o outro.

6. **No `teste-sem-eoi`, por que o teclado também parou, se o tratador
   dele manda EOI?**
   Porque a interrupção do teclado nem chegou a ser entregue. Sem o EOI do
   timer, o APIC considera o vetor 0x30 ainda "em atendimento" e bloqueia
   tudo da mesma classe de prioridade ou menor. 0x31 é da mesma classe (3).

7. **Por que o tratador do teclado não traduz nem imprime a tecla?**
   Para ser curto (enquanto ele roda, as outras interrupções esperam) e para
   não pegar lock nenhum: imprimir exigiria o lock do CONSOLE, que o código
   interrompido poderia estar segurando. Ele só põe o byte numa fila sem
   lock; o laço principal faz o resto.

8. **Por que `sti; hlt` e não um `hlt` sozinho depois de ver que a fila está
   vazia?**
   Porque uma tecla poderia chegar entre o teste e o `hlt`, e a CPU dormiria
   com ela na fila. Com as interrupções desligadas durante o teste e o par
   `sti; hlt` (a CPU só aceita interrupções depois da instrução que segue o
   `sti`), a tecla que chegar acorda o `hlt`.

9. **O que `sem_interrupcoes` faz e por que ele não liga as interrupções
   sempre no fim?**
   Desliga as interrupções, roda a função e as devolve ao estado anterior.
   Se ligasse sempre, chamado de dentro de um tratador (onde as interrupções
   estão desligadas), ele as ligaria no meio do tratador.

</details>
