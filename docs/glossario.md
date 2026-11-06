# Glossário: hardware e sistemas operacionais

Termos que aparecem nos capítulos, explicados para quem programa mas nunca
trabalhou "abaixo" do sistema operacional. Os termos de **Rust** estão em
[rust-basico.md](rust-basico.md).

---

## Bits, bytes e hexadecimal (revisão rápida)

Quase tudo em OSDev é descrito em **hexadecimal** (base 16, prefixo `0x`),
porque cada dígito hexa corresponde a exatamente 4 bits:

```
0x8E  =  1000 1110  (binário)  =  142 (decimal)
   8      E
```

Isso deixa fácil ler "quais bits estão ligados" num número. Os bits são
numerados **da direita para a esquerda, começando em 0**:

```
bit:   7 6 5 4 3 2 1 0
0x8E:  1 0 0 0 1 1 1 0     → bits 7, 3, 2 e 1 ligados
```

Operações de bits usadas no código:

| Expressão | Significado |
|---|---|
| `1 << 47` | um número com só o bit 47 ligado |
| `a \| b` | junta os bits ligados de `a` e `b` (OR) |
| `x & 0x20` | isola o bit 5 de `x` (dá 0 se ele estiver desligado) |
| `x >> 16` | desloca para a direita: descarta os 16 bits de baixo |

Tamanhos: **KiB** = 1024 bytes, **MiB** = 1024 KiB, **GiB**, **TiB**...
(o "i" indica potência de 2, para diferenciar de KB = 1000).

---

## A CPU por dentro

**Registrador**
: Uma "variável" dentro da própria CPU, muito mais rápida que a memória. Em
x86-64 há 16 registradores de uso geral de 64 bits (RAX, RBX, RCX, RDX, RSI,
RDI, RSP, RBP, R8–R15) e vários especiais. Partes menores têm outros nomes:
EAX = 32 bits de baixo do RAX, AX = 16 bits, AL = 8 bits.

**RIP** (*instruction pointer*)
: Registrador com o endereço da **próxima instrução** a executar. Quando um
erro mostra "RIP=0x608c827", é ali que o código estava.

**RSP** (*stack pointer*)
: Endereço do **topo da pilha** atual.

**RFLAGS**
: Registrador de "bandeiras": bits que indicam resultado da última conta
(zero? negativo? estouro?) e estados da CPU. O bit **IF** (*interrupt flag*)
liga/desliga interrupções de hardware.

**Registradores de controle (CR0, CR2, CR3, CR4)**
: Configuram o modo de operação da CPU. Exemplo: o **CR2** guarda o endereço
que causou o último page fault; o **CR3** aponta para as tabelas de página.

**Registradores de segmento (CS, DS, SS, ES, FS, GS)**
: Relíquias da arquitetura de 16/32 bits. Em 64 bits servem quase só para
indicar o nível de privilégio (CS) e alguns usos especiais (FS/GS). Ver
capítulo 03.

**Instrução privilegiada**
: Instrução que só o kernel (anel 0) pode executar: `cli`, `sti`, `hlt`,
`lgdt`, `lidt`, `in`, `out`... Se um programa comum tentar, a CPU gera uma
exceção.

---

## A pilha (*stack*)

Região de memória onde ficam variáveis locais, parâmetros e endereços de
retorno das funções. É a mesma pilha que aparece no *stack trace* de um
erro em Java/Python.

Em x86 a pilha **cresce para baixo**: cada `push` diminui o RSP e grava
no novo endereço; cada `pop` lê e aumenta o RSP. Por isso, ao reservar uma
pilha de 20 KiB entre os endereços `A` e `A + 20480`, informamos à CPU o
**topo** `A + 20480`, e ela vai descendo a partir dali.

Se uma recursão infinita consumir a pilha inteira, acontece o **stack
overflow**, que num kernel pode virar um *double fault* (capítulo 04).

---

## Memória

**Endereço físico**
: Posição real de um byte nos chips de RAM (ou num dispositivo).

**Endereço virtual**
: O endereço que o programa usa. A CPU traduz endereço virtual para físico
usando as **tabelas de página**, configuradas pelo kernel. Cada programa
pode ter sua própria tradução, por isso dois programas podem usar o mesmo
endereço virtual sem se enxergarem. (Capítulo 7 do livro.)

**Página / quadro (*page / frame*)**
: A memória é gerenciada em blocos de **4 KiB**. "Página" é o bloco
virtual; "quadro" é o bloco físico correspondente.

**Tabelas de página (PML4, PDPT, PD, PT)**
: A árvore de 4 níveis que a CPU percorre para traduzir um endereço virtual.
Cada tabela ocupa um quadro de 4 KiB e tem 512 entradas (doc 06).

**CR3**
: Registrador que guarda o endereço físico da tabela do nível mais alto
(PML4). Trocar o CR3 troca o espaço de endereços inteiro.

**Página grande** (*huge page*)
: Página de 2 MiB ou 1 GiB, que interrompe a caminhada pelas tabelas mais
cedo. Economiza tabelas e entradas de TLB.

**TLB** (*Translation Lookaside Buffer*)
: Cache da CPU com traduções recentes de endereços. Depois de alterar uma
entrada de tabela, o kernel precisa invalidá-la (`invlpg`), senão a CPU pode
continuar usando a tradução antiga.

**Mapeamento com deslocamento** (*offset mapping*, *direct map*)
: Toda a RAM física mapeada a partir de um endereço virtual fixo: o físico F
fica no virtual `deslocamento + F`. No SiliconOS, a partir de
`0xFFFF_8000_0000_0000`.

**Kernel na metade alta** (*higher-half kernel*)
: Kernel que roda nos endereços virtuais altos, deixando a metade baixa para
os programas comuns.

**NX** (*no-execute*)
: Bit da entrada de página que proíbe executar código naquela página.

**WP** (*write protect*, bit 16 de CR0)
: Quando ligado, até o kernel recebe page fault ao escrever numa página
somente leitura.

**Page fault**
: Exceção gerada quando a CPU tenta traduzir um endereço virtual e não
encontra a página (ou ela não tem permissão para o acesso).

**Mapa de memória**
: Lista, fornecida pelo firmware, de quais regiões da RAM estão livres,
quais são do firmware, quais são de dispositivos etc.

**Mapeamento identidade** (*identity mapping*)
: Configuração da paginação em que o endereço virtual X aponta para o
endereço físico X. O UEFI entrega a máquina assim, e por isso, hoje, um
endereço físico pode ser usado diretamente como ponteiro.

**Alocador de quadros**
: Parte do kernel que entrega quadros físicos livres de 4 KiB a quem pede
e os recebe de volta (doc 05).

**Bitmap**
: Estrutura em que cada bit representa um item. No alocador de quadros, um
bit por quadro: 1 = ocupado, 0 = livre.

**Janela PCI** (*PCI hole*)
: Faixa de endereços físicos logo abaixo de 4 GiB reservada para
dispositivos (placa de vídeo, controladores). A RAM que ficaria ali é
remapeada para cima de 4 GiB.

**Memória baixa**
: O primeiro megabyte de endereços, herança do PC original (até 640 KiB de
RAM, depois vídeo e ROMs). Tem usos especiais, como ligar as outras CPUs.

**MMIO** (*memory-mapped I/O*)
: Dispositivo que aparece como se fosse memória: escrever num certo
endereço envia dados ao dispositivo. O framebuffer de vídeo é um exemplo.

**Heap**
: Região de memória de onde saem as alocações de tamanho variável (`Box`,
`Vec`, `String`), entregues e devolvidas em qualquer ordem. Não tem relação
com a estrutura de dados "heap" (fila de prioridade). (Doc 07.)

**Alocador global**
: A parte do kernel que atende todos os pedidos de memória do Rust. No
SiliconOS, a lista de blocos livres do doc 07.

**Bump allocator**
: Alocador que só empurra um ponteiro para a frente e nunca reaproveita a
memória liberada.

**Lista de blocos livres** (*free list*)
: Lista encadeada dos buracos do heap, com os nós guardados dentro dos
próprios buracos.

**First fit**
: Estratégia de alocação que usa o primeiro bloco livre em que o pedido
cabe.

**Fragmentação**
: Memória livre picada em buracos pequenos demais para atender um pedido,
mesmo que a soma deles seja grande. Juntar blocos vizinhos ao liberar
(*coalescing*) combate a fragmentação.

**Vazamento de memória** (*memory leak*)
: Memória alocada que nunca é devolvida. Num kernel, que roda por dias,
vazamentos acabam esgotando a RAM.

**Porta de E/S** (*I/O port*)
: Forma mais antiga de falar com dispositivos: um espaço separado de
65 536 "endereços" acessados pelas instruções `in` e `out`. A serial e o
teclado PS/2 usam portas.

---

## Software de baixo nível

**Firmware**
: Programa gravado num chip da placa-mãe, que roda ao ligar o computador.
Inicializa o hardware e carrega o sistema operacional.

**BIOS**
: Firmware antigo (desde 1981). Entrega a CPU em modo de 16 bits e carrega
512 bytes do disco.

**UEFI**
: Firmware moderno. Entrega a CPU em 64 bits, lê arquivos de partições FAT
e oferece serviços (vídeo, teclado, memória) até o SO dispensá-los.

**OVMF**
: Implementação de UEFI de código aberto que o QEMU usa.

**ESP** (*EFI System Partition*)
: A partição FAT onde o UEFI procura `EFI/BOOT/BOOTX64.EFI`. No SiliconOS é a
pasta `esp/`.

**Bootloader**
: Programa que o firmware executa e que carrega o kernel. No SiliconOS, o
próprio kernel faz esse papel.

**Kernel**
: O núcleo do sistema operacional: o único código que roda com privilégio
total.

**Bare-metal**
: "Direto no metal": código rodando sem nenhum sistema operacional por
baixo.

**QEMU**
: Emulador de PC. Roda o SiliconOS numa janela e reinicia em segundos.

---

## Privilégio e proteção

**Anéis (*rings*)**
: Níveis de privilégio da CPU x86. **Anel 0** = kernel (pode tudo); **anel
3** = programas comuns (restritos). Os anéis 1 e 2 quase não são usados.

**Modo usuário / modo kernel**
: O mesmo que anel 3 / anel 0.

**Chamada de sistema (*syscall*)**
: Quando um programa pede algo ao kernel (abrir arquivo, alocar memória),
ele executa uma instrução especial que passa a CPU para o anel 0 de forma
controlada.

---

## Interrupções e exceções

**Interrupção**
: Sinal que faz a CPU parar o que está fazendo, executar uma rotina
(**tratador** ou *handler*) e depois voltar. Pode vir do hardware (tecla
pressionada, timer) ou do software (instrução `int`).

**Exceção**
: Interrupção gerada pela própria CPU porque a instrução atual tem um
problema (divisão por zero, acesso inválido...).

**Vetor**
: Número de 0 a 255 que identifica cada interrupção/exceção.

**IDT**
: Tabela que diz qual tratador atende cada vetor (capítulo 04).

**Double fault / triple fault**
: Double fault = falha **durante o tratamento** de outra falha. Triple fault
= falha durante o tratamento do double fault; a CPU desiste e **reinicia o
computador**.

**`cli` / `sti`**
: Desligam / ligam as interrupções de hardware (o bit IF).

**`hlt`**
: Faz a CPU dormir até a próxima interrupção.

**IRQ** (*interrupt request*)
: Uma linha de interrupção de um dispositivo. No PC, números herdados de
1981: 0 = timer, 1 = teclado, 4 = COM1, 8 = RTC, 12 = mouse (doc 08).

**Tratador de interrupção** (*interrupt handler*)
: A função que a CPU executa quando chega uma interrupção. Deve ser curta e
terminar com EOI.

**PIC 8259**
: O controlador de interrupções do PC original: dois chips em cascata, 15
IRQs, uma CPU. Hoje é desligado em favor do APIC.

**APIC local**
: Controlador de interrupções que fica dentro de cada CPU. Entrega as
interrupções ao núcleo e tem um timer próprio. Registradores MMIO em
`0xFEE0_0000`.

**I/O APIC**
: Controlador no chipset que recebe as IRQs dos dispositivos e as
encaminha, como vetores, para o APIC local de alguma CPU.

**GSI** (*global system interrupt*)
: O número de uma entrada (pino) do I/O APIC. Normalmente IRQ *n* = GSI
*n*, salvo os desvios listados na MADT.

**EOI** (*end of interrupt*)
: Aviso do tratador ao controlador de que terminou. Sem ele, o APIC não
entrega mais interrupções daquela classe de prioridade.

**Interrupção espúria**
: Uma interrupção "fantasma": o controlador começou a entregá-la, mas o
sinal sumiu. Tem vetor próprio e não leva EOI.

**Borda / nível** (*edge / level*)
: Disparo por borda: a interrupção é a mudança do sinal (uma por evento).
Por nível: ela continua enquanto o sinal estiver ativo.

**MSR** (*model-specific register*)
: Registradores de configuração da CPU identificados por número, lidos com
`rdmsr`. Ex.: `0x1B` = endereço do APIC local.

**ACPI**
: Tabelas que o firmware deixa na memória descrevendo o hardware. O caminho
é RSDP → XSDT → tabelas como a MADT.

**MADT**
: Tabela ACPI (assinatura `"APIC"`) que lista as CPUs, os I/O APICs e os
desvios de IRQ.

**PIT** (8253/8254)
: Timer de 1981, frequência fixa de 1 193 182 Hz. Usado para calibrar o
timer do APIC.

**RTC** (relógio CMOS)
: Relógio com bateria que guarda data e hora, acessado pelas portas 0x70 e
0x71.

**Tique** (*tick*)
: Cada interrupção do timer. No SiliconOS, 100 por segundo.

**Scancode**
: O byte que o teclado manda para cada tecla apertada (*make code*) ou
solta (*break code* = make + 0x80). Diz a posição da tecla, não a letra.

**Controlador PS/2** ("8042")
: O chip que recebe os bytes do teclado e do mouse PS/2 e os entrega pela
porta 0x60, disparando a IRQ 1 (teclado) ou 12 (mouse).

**Deadlock**
: Dois lados esperando um pelo outro para sempre. No kernel: um tratador
esperando um lock que o código que ele interrompeu está segurando.

**Spinlock**
: Um lock que espera "girando" num laço (com a instrução `pause`) em vez de
dormir. É o `spin::Mutex` do SiliconOS.

---

## Tabelas da CPU

**GDT** (*Global Descriptor Table*)
: Tabela de segmentos. Em 64 bits define principalmente os níveis de
privilégio (capítulo 03).

**Descritor**
: Uma entrada de 8 (ou 16) bytes numa dessas tabelas.

**Seletor**
: Número que aponta para um descritor da GDT (índice × 8 + bits de
privilégio). É o que fica guardado em CS, DS, SS.

**TSS** (*Task State Segment*)
: Estrutura que, em 64 bits, guarda principalmente endereços de pilhas a
usar em situações especiais (capítulo 03).

**IST** (*Interrupt Stack Table*)
: As 7 pilhas de emergência da TSS.
