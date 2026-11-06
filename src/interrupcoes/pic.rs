//! Os dois PICs 8259 do PC antigo: só para DESLIGÁ-LOS.
//!
//! Desde 1981 o PC tem um controlador de interrupções 8259 (desde o PC AT,
//! dois em cascata: 15 IRQs). Hoje quem faz esse trabalho é o APIC, mas os
//! 8259 continuam lá (emulados) por compatibilidade. Se ficassem ligados,
//! poderiam entregar IRQs nos vetores 0x08–0x0F, que são EXCEÇÕES da CPU
//! (a IRQ 0 do timer chegaria como se fosse um double fault!).
//!
//! Por isso: remapeamos para vetores que não incomodam (0x20–0x2F) e
//! mascaramos todas as linhas.

use crate::serial::{inb, outb};

const PIC1_COMANDO: u16 = 0x20;
const PIC1_DADOS: u16 = 0x21;
const PIC2_COMANDO: u16 = 0xA0;
const PIC2_DADOS: u16 = 0xA1;

/// Para onde as 16 IRQs do 8259 vão (se algum dia chegarem): 0x20–0x2F.
pub const VETOR_BASE: u8 = 0x20;

/// Uma escrita na porta 0x80 (não usada) só para gastar ~1 µs: o 8259
/// original precisava de um respiro entre comandos.
fn esperar() {
    unsafe { outb(0x80, 0) };
}

/// Remapeia os dois 8259 para VETOR_BASE e mascara todas as IRQs.
/// Devolve as máscaras que o firmware tinha deixado (1 bit por IRQ; 1 = mascarada).
pub fn desligar() -> (u8, u8) {
    unsafe {
        let mascaras = (inb(PIC1_DADOS), inb(PIC2_DADOS));

        // A sequência de inicialização tem 4 "palavras" (ICW1..ICW4).
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
        mascaras
    }
}
