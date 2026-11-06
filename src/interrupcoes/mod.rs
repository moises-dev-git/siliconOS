//! Interrupções de hardware: os controladores (PIC, APIC local, I/O APIC),
//! o timer e o teclado.
//!
//! Explicação completa em docs/08-interrupcoes-de-hardware.md.
//!
//! Caminho de uma tecla até o nosso código:
//!
//!   teclado ─► controlador PS/2 ─IRQ 1─► I/O APIC ─vetor 0x31─► APIC local ─► CPU ─► IDT[0x31]
//!
//! e do timer (que fica dentro do próprio APIC local):
//!
//!   timer do APIC local ─vetor 0x30─► CPU ─► IDT[0x30]

pub mod apic;
pub mod ioapic;
pub mod pic;
pub mod teclado;
pub mod timer;

use core::sync::atomic::{AtomicU64, Ordering};

use crate::acpi;
use crate::cpu;
use crate::idt::QuadroInterrupcao;
use crate::kprintln;

// Vetores (posições na IDT). 0x00-0x1F são das exceções da CPU; 0x20-0x2F
// ficam para o 8259 desligado (só espúrias podem aparecer ali).
pub const VETOR_PIC_ESPURIO_1: u8 = pic::VETOR_BASE + 7;  // IRQ 7 falsa do mestre
pub const VETOR_PIC_ESPURIO_2: u8 = pic::VETOR_BASE + 15; // IRQ 15 falsa do escravo
pub const VETOR_TIMER: u8 = 0x30;
pub const VETOR_TECLADO: u8 = 0x31;
/// Interrupção "espúria" do APIC: ele começou a entregar algo que sumiu no
/// caminho. Não é preciso (nem se deve) mandar EOI. Os 4 bits baixos precisam
/// ser 1111 em CPUs antigas, por isso 0xFF.
pub const VETOR_APIC_ESPURIO: u8 = 0xFF;

static ESPURIAS: AtomicU64 = AtomicU64::new(0);

pub fn espurias() -> u64 {
    ESPURIAS.load(Ordering::Relaxed)
}

/// Tratador das interrupções espúrias (do 8259 e do APIC): só conta.
pub extern "x86-interrupt" fn espuria(_quadro: QuadroInterrupcao) {
    ESPURIAS.fetch_add(1, Ordering::Relaxed);
}

/// Configura tudo e liga as interrupções (`sti`).
/// Precisa vir depois da IDT e das tabelas de página do kernel.
pub fn init() {
    // 1. O que a máquina tem (ACPI/MADT).
    let madt = acpi::madt();
    let io = madt.ioapic.expect("ACPI: nenhum I/O APIC na MADT");
    kprintln!("ACPI (MADT): {} CPU(s), APIC local em {:#x}, I/O APIC {} em {:#x} (GSI a partir de {}), 8259: {}",
              madt.cpus, madt.lapic, io.id, io.endereco, io.gsi_base,
              if madt.tem_pic_8259 { "sim" } else { "nao" });
    for d in madt.desvios.iter().flatten() {
        let r = madt.rotear_irq(d.irq);
        kprintln!("  desvio: IRQ {} -> GSI {} ({}, {})", d.irq, r.gsi,
                  if r.ativo_baixo { "ativo baixo" } else { "ativo alto" },
                  if r.por_nivel { "nivel" } else { "borda" });
    }

    // 2. Desligar o 8259 antigo.
    let (m1, m2) = pic::desligar();
    kprintln!("PIC 8259: mascaras do firmware {:#04x} {:#04x} -> remapeado para {:#x}-{:#x} e todo mascarado",
              m1, m2, pic::VETOR_BASE, pic::VETOR_BASE + 15);

    // 3. APIC local (desta CPU).
    let l = apic::init(madt.lapic, VETOR_APIC_ESPURIO);
    kprintln!("APIC local: id {}, versao {:#x}, {} entradas LVT; fisico {:#x} -> virtual {:#x}",
              l.id, l.versao, l.entradas_lvt, l.fisico, l.virtual_);

    // 4. I/O APIC: todos os pinos mascarados.
    let i = ioapic::init(io.endereco, io.gsi_base);
    kprintln!("I/O APIC: id {}, versao {:#x}, {} pinos; virtual {:#x}", i.id, i.versao, i.pinos, i.virtual_);

    // 5. Timer: calibrar com o PIT e ligar em modo periódico.
    let contagens = timer::calibrar();
    timer::iniciar(VETOR_TIMER, contagens);
    kprintln!("Timer do APIC: {} contagens em 10 ms = {} MHz (barramento de ~{} MHz / 16)",
              contagens, contagens / 10_000, contagens as u64 * 16 / 10_000);
    kprintln!("  modo periodico: {} Hz no vetor {:#x}", timer::FREQUENCIA_HZ, VETOR_TIMER);

    // 6. Teclado: ligar a IRQ 1 no controlador PS/2 e rotear no I/O APIC.
    let (antes, depois, descartados) = teclado::init();
    let r = madt.rotear_irq(teclado::IRQ);
    ioapic::rotear(r.gsi, VETOR_TECLADO, l.id, r.ativo_baixo, r.por_nivel);
    kprintln!("Teclado PS/2: config {:#04x} -> {:#04x}, {} bytes velhos descartados",
              antes, depois, descartados);
    kprintln!("  IRQ {} -> GSI {} -> vetor {:#x}; entrada do I/O APIC: {:#018x}",
              teclado::IRQ, r.gsi, VETOR_TECLADO, ioapic::entrada(r.gsi));

    // 7. Pronto: a CPU passa a aceitar interrupções.
    cpu::ligar_interrupcoes();
    kprintln!("Interrupcoes LIGADAS (sti).");
}
