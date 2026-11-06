//! Driver mínimo da porta serial COM1 (UART 16550).
//!
//! A serial é a ferramenta de depuração mais importante em OSDev: funciona
//! antes de qualquer outra coisa (vídeo, memória, interrupções) e o QEMU
//! pode gravá-la em arquivo com `-serial file:serial.log`.

use core::fmt;

const COM1: u16 = 0x3F8;

pub struct Serial;

impl Serial {
    /// Configura a UART: 38400 baud, 8 bits, sem paridade, 1 stop bit.
    pub fn init() {
        unsafe {
            outb(COM1 + 1, 0x00); // desliga interrupções da UART
            outb(COM1 + 3, 0x80); // DLAB=1: próximos 2 registradores são o divisor
            outb(COM1 + 0, 0x03); // divisor = 3 -> 115200 / 3 = 38400 baud
            outb(COM1 + 1, 0x00);
            outb(COM1 + 3, 0x03); // DLAB=0, formato 8N1
            outb(COM1 + 2, 0xC7); // liga e limpa as FIFOs
            outb(COM1 + 4, 0x03); // DTR + RTS
        }
    }

    pub fn write_byte(b: u8) {
        unsafe {
            // Espera o registrador de transmissão esvaziar (bit 5 do LSR)
            while inb(COM1 + 5) & 0x20 == 0 {
                core::hint::spin_loop();
            }
            outb(COM1, b);
        }
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                Serial::write_byte(b'\r');
            }
            Serial::write_byte(b);
        }
        Ok(())
    }
}

pub unsafe fn outb(port: u16, value: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") value,
            options(nomem, nostack, preserves_flags));
    }
}

pub unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", out("al") value, in("dx") port,
            options(nomem, nostack, preserves_flags));
    }
    value
}
