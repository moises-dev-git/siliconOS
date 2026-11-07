//! `sos`: a biblioteca mínima dos programas de usuário do SiliconOS.
//!
//! Explicação completa em docs/13-disco-em-ram-e-elf.md.
//!
//! Um programa em Rust sem sistema operacional (`no_std`) não tem `println!`
//! nem `std::process::exit`: quem fornece isso, num sistema de verdade, é a
//! biblioteca do sistema (a libc no Linux). Aqui ela é este arquivo: cada
//! função é uma chamada de sistema `int 0x80` (doc 10).

#![no_std]

use core::arch::asm;
use core::fmt;

// Números das chamadas de sistema (os mesmos de src/usuario/mod.rs do kernel).
const SYS_SAIR: u64 = 0;
const SYS_ESCREVER: u64 = 1;
const SYS_TIQUES: u64 = 2;
const SYS_PID: u64 = 3;

/// Termina o programa com o código dado. Não volta.
pub fn sair(codigo: u64) -> ! {
    unsafe { asm!("int 0x80", in("rax") SYS_SAIR, in("rdi") codigo, options(noreturn)) }
}

/// Escreve os bytes no console. Devolve quantos foram escritos, ou um erro
/// negativo (-14 se o kernel recusar o ponteiro).
pub fn escrever(bytes: &[u8]) -> i64 {
    let r: u64;
    unsafe {
        // O kernel preserva todos os registradores, menos RAX (o resultado).
        asm!("int 0x80", inlateout("rax") SYS_ESCREVER => r,
             in("rdi") bytes.as_ptr(), in("rsi") bytes.len(), options(nostack));
    }
    r as i64
}

/// Tiques do timer desde o boot (100 por segundo).
pub fn tiques() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") SYS_TIQUES => r, options(nostack)) };
    r
}

/// O número deste processo.
pub fn pid() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") SYS_PID => r, options(nostack)) };
    r
}

/// Destino de `print!`: cada pedaço de texto formatado vira um `escrever`.
pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        escrever(s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::Console, $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{ $crate::print!($($arg)*); $crate::print!("\n"); }};
}

/// Define o ponto de entrada `_start` (o que o ELF diz para executar
/// primeiro), que chama `main` e passa o valor devolvido para `sair`.
#[macro_export]
macro_rules! principal {
    ($main:path) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn _start() -> ! {
            $crate::sair($main())
        }
    };
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic no programa: {}", info);
    sair(101)
}
