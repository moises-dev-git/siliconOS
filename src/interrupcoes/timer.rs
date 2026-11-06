//! O relógio do kernel: o timer do APIC local, calibrado com o PIT.
//!
//! O timer do APIC conta para baixo numa frequência que depende da máquina
//! (o "clock do barramento"), e nenhum registrador diz qual é. Então medimos:
//! deixamos ele contar durante 10 ms cronometrados pelo PIT 8253/8254, um
//! chip de 1981 cuja frequência é conhecida e fixa: 1 193 182 Hz.
//!
//! Depois o timer fica em modo periódico: a cada 1/FREQUENCIA_HZ segundo ele
//! gera uma interrupção, e o tratador soma 1 em TIQUES.

use core::sync::atomic::{AtomicU64, Ordering};

use super::apic;
use crate::idt::QuadroInterrupcao;
use crate::serial::{inb, outb};

/// Interrupções do timer por segundo (100 Hz = uma a cada 10 ms).
pub const FREQUENCIA_HZ: u64 = 100;

/// Quantas interrupções do timer já aconteceram desde que ele foi ligado.
static TIQUES: AtomicU64 = AtomicU64::new(0);

pub fn tiques() -> u64 {
    TIQUES.load(Ordering::Relaxed)
}

/// Milissegundos desde que o timer foi ligado.
pub fn milissegundos() -> u64 {
    tiques() * 1000 / FREQUENCIA_HZ
}

/// O tratador: roda FREQUENCIA_HZ vezes por segundo, com as interrupções
/// desligadas (interrupt gate). Precisa ser curto.
pub extern "x86-interrupt" fn tratador(_quadro: QuadroInterrupcao) {
    let t = TIQUES.fetch_add(1, Ordering::Relaxed) + 1;

    // `./run.sh teste-print-interrupcao`: imprimir de DENTRO do tratador.
    #[cfg(feature = "teste-print-interrupcao")]
    if t % 10 == 0 {
        crate::kprint!("[tique {}]", t);
    }
    let _ = t;

    // `./run.sh teste-sem-eoi`: esquecer o EOI. O APIC acha que ainda
    // estamos tratando esta interrupção e não entrega mais nenhuma.
    #[cfg(not(feature = "teste-sem-eoi"))]
    apic::eoi();
}

// ---------------------------------------------------------------------------
// PIT: só o canal 2, usado como cronômetro de 10 ms
// ---------------------------------------------------------------------------

const PIT_FREQUENCIA: u64 = 1_193_182;
const PIT_CANAL2: u16 = 0x42;
const PIT_COMANDO: u16 = 0x43;
/// Porta 0x61 (controle do alto-falante): bit 0 = "gate" do canal 2
/// (liga a contagem), bit 1 = liga o alto-falante, bit 5 = saída do canal 2.
const PORTA_61: u16 = 0x61;

/// Quanto o timer do APIC conta (com divisor 16) em 10 ms medidos pelo PIT.
pub fn calibrar() -> u32 {
    const MS: u64 = 10;
    let contagem_pit = (PIT_FREQUENCIA * MS / 1000) as u16; // 11 931

    unsafe {
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
    }
    let restante = apic::ler(apic::REG_TIMER_ATUAL);
    apic::escrever(apic::REG_TIMER_INICIAL, 0); // para o timer
    u32::MAX - restante
}

/// Liga o timer do APIC em modo periódico no vetor dado.
/// `contagens_10ms` vem de `calibrar`.
pub fn iniciar(vetor: u8, contagens_10ms: u32) {
    const PERIODICO: u32 = 1 << 17; // bits 17-18 da LVT do timer: 01 = periódico
    let por_tique = contagens_10ms as u64 * 100 / FREQUENCIA_HZ;
    apic::escrever(apic::REG_TIMER_DIVISOR, 0b0011);
    apic::escrever(apic::REG_LVT_TIMER, PERIODICO | vetor as u32);
    apic::escrever(apic::REG_TIMER_INICIAL, por_tique as u32);
}

// ---------------------------------------------------------------------------
// RTC: o relógio de pulso do PC (CMOS), usado para conferir o timer
// ---------------------------------------------------------------------------

/// Segundos do relógio de tempo real (0-59), lidos por polling.
pub fn segundos_rtc() -> u8 {
    unsafe {
        // Registrador 0x0A, bit 7: o RTC está atualizando os valores agora;
        // esperar terminar para não ler um número pela metade.
        loop {
            outb(0x70, 0x0A);
            if inb(0x71) & 0x80 == 0 { break; }
        }
        outb(0x70, 0x00); // registrador 0: segundos
        let s = inb(0x71);
        outb(0x70, 0x0B); // registrador 0x0B, bit 2: 1 = binário, 0 = BCD
        if inb(0x71) & 0x04 != 0 { s } else { (s >> 4) * 10 + (s & 0x0F) }
    }
}
