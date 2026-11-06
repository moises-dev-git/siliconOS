//! Saída do QEMU com código de retorno, pelo dispositivo `isa-debug-exit`.
//!
//! Explicação em docs/09-shell-e-testes.md.
//!
//! Com `-device isa-debug-exit,iobase=0xf4,iosize=0x04`, o QEMU cria uma
//! "porta de saída": escrever o valor V na porta 0xF4 encerra o QEMU, que
//! devolve ao sistema hospedeiro o código (V << 1) | 1. Numa máquina real a
//! porta não existe e a escrita não faz nada.

use crate::serial::outb;

const PORTA_SAIDA: u16 = 0xF4;

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum Saida {
    /// O QEMU sai com (0x10 << 1) | 1 = 33.
    Sucesso = 0x10,
    /// O QEMU sai com (0x11 << 1) | 1 = 35.
    Falha = 0x11,
}

pub fn sair(codigo: Saida) -> ! {
    unsafe { outb(PORTA_SAIDA, codigo as u8) };
    // Sem o dispositivo (máquina real, ou QEMU sem o -device): parar aqui.
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
