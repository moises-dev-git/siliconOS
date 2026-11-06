//! APIC local: o controlador de interrupções que fica DENTRO de cada CPU.
//!
//! Ele recebe as interrupções (do I/O APIC, do próprio timer, de outras CPUs)
//! e as entrega ao núcleo. Seus registradores são MMIO: uma página de 4 KiB
//! no endereço físico 0xFEE0_0000 (o padrão; a MADT confirma), cada
//! registrador de 32 bits alinhado em 16 bytes.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::cpu;
use crate::memoria::paginacao;

/// MSR com o endereço físico do APIC local e o bit que o liga.
const IA32_APIC_BASE: u32 = 0x1B;
const APIC_BASE_LIGADO: u64 = 1 << 11;
const APIC_BASE_ENDERECO: u64 = 0x000F_FFFF_FFFF_F000;

// Deslocamentos dos registradores dentro da página.
pub const REG_ID: u32 = 0x020;
pub const REG_VERSAO: u32 = 0x030;
pub const REG_PRIORIDADE: u32 = 0x080;  // TPR: só aceita vetores com prioridade acima desta
pub const REG_EOI: u32 = 0x0B0;         // "end of interrupt": escrever aqui = "terminei"
pub const REG_ESPURIO: u32 = 0x0F0;     // SVR: vetor espúrio + bit 8 que liga o APIC
pub const REG_LVT_TIMER: u32 = 0x320;
pub const REG_LVT_LINT0: u32 = 0x350;
pub const REG_TIMER_INICIAL: u32 = 0x380;
pub const REG_TIMER_ATUAL: u32 = 0x390;
pub const REG_TIMER_DIVISOR: u32 = 0x3E0;

/// Bit 16 de qualquer entrada LVT ("local vector table"): mascarada.
pub const LVT_MASCARADA: u32 = 1 << 16;
const SVR_APIC_LIGADO: u32 = 1 << 8;

/// Endereço VIRTUAL da página de registradores (0 = ainda não mapeada).
static BASE: AtomicU64 = AtomicU64::new(0);

pub fn ler(registrador: u32) -> u32 {
    let base = BASE.load(Ordering::Relaxed);
    unsafe { ((base + registrador as u64) as *const u32).read_volatile() }
}

pub fn escrever(registrador: u32, valor: u32) {
    let base = BASE.load(Ordering::Relaxed);
    unsafe { ((base + registrador as u64) as *mut u32).write_volatile(valor) };
}

/// Avisa o APIC que o tratador terminou. Sem isso, ele não entrega mais
/// nenhuma interrupção de prioridade igual ou menor.
pub fn eoi() {
    escrever(REG_EOI, 0);
}

/// Número deste APIC (= desta CPU), usado como destino no I/O APIC.
pub fn id() -> u8 {
    (ler(REG_ID) >> 24) as u8
}

pub struct Info {
    pub fisico: u64,
    pub virtual_: u64,
    pub id: u8,
    pub versao: u8,
    pub entradas_lvt: u8,
}

/// Mapeia os registradores e liga o APIC local.
pub fn init(fisico_madt: u64, vetor_espurio: u8) -> Info {
    let msr = cpu::ler_msr(IA32_APIC_BASE);
    let fisico = msr & APIC_BASE_ENDERECO;
    assert!(msr & APIC_BASE_LIGADO != 0, "APIC local desligado no MSR");
    assert_eq!(fisico, fisico_madt, "MSR e MADT discordam sobre o endereco do APIC local");

    let virtual_ = paginacao::mapear_mmio(fisico, 4096).expect("mapear o APIC local");
    BASE.store(virtual_, Ordering::Relaxed);

    // Prioridade 0: aceitar todas as interrupções.
    escrever(REG_PRIORIDADE, 0);
    // LINT0 é o pino onde o 8259 antigo entregaria as IRQs: não usamos mais.
    escrever(REG_LVT_LINT0, LVT_MASCARADA);
    // Liga o APIC (bit 8) e define o vetor das interrupções espúrias.
    escrever(REG_ESPURIO, SVR_APIC_LIGADO | vetor_espurio as u32);

    let versao = ler(REG_VERSAO);
    Info {
        fisico,
        virtual_,
        id: id(),
        versao: versao as u8,
        entradas_lvt: ((versao >> 16) & 0xFF) as u8 + 1,
    }
}
