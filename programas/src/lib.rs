//! `sos`: a biblioteca mínima dos programas de usuário do SiliconOS.
//!
//! Explicação completa em docs/13-disco-em-ram-e-elf.md.
//!
//! Um programa em Rust sem sistema operacional (`no_std`) não tem `println!`
//! nem `std::process::exit`: quem fornece isso, num sistema de verdade, é a
//! biblioteca do sistema (a libc no Linux). Aqui ela é este arquivo: cada
//! função é uma chamada de sistema. Até o doc 13, pela instrução `int 0x80`
//! (doc 10); desde o doc 14, pela instrução `syscall`, feita para isso.

#![no_std]

use core::arch::asm;
use core::fmt;

// Números das chamadas de sistema (os mesmos de src/usuario/mod.rs do kernel).
const SYS_SAIR: u64 = 0;
const SYS_ESCREVER: u64 = 1;
const SYS_TIQUES: u64 = 2;
const SYS_PID: u64 = 3;
const SYS_DORMIR: u64 = 4;

/// Uma chamada de sistema com até dois argumentos, pela instrução
/// `syscall` (doc 14). Número em RAX, argumentos em RDI e RSI, resultado em
/// RAX. A própria CPU usa RCX (endereço de volta) e R11 (RFLAGS): o
/// compilador precisa saber que os dois mudam (`lateout(...) _`).
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}

/// Termina o programa com o código dado. Não volta.
pub fn sair(codigo: u64) -> ! {
    chamada(SYS_SAIR, codigo, 0);
    unreachable!("o kernel voltou de sair()")
}

/// Escreve os bytes no console. Devolve quantos foram escritos, ou um erro
/// negativo (-14 se o kernel recusar o ponteiro).
pub fn escrever(bytes: &[u8]) -> i64 {
    chamada(SYS_ESCREVER, bytes.as_ptr() as u64, bytes.len() as u64) as i64
}

/// Tiques do timer desde o boot (100 por segundo).
pub fn tiques() -> u64 {
    chamada(SYS_TIQUES, 0, 0)
}

/// O número deste processo.
pub fn pid() -> u64 {
    chamada(SYS_PID, 0, 0)
}

/// Dorme por `ms` milissegundos sem gastar CPU (doc 15): o kernel tira o
/// programa do rodízio e o devolve quando o prazo vence. A precisão é de um
/// tique (10 ms).
pub fn dormir(ms: u64) {
    chamada(SYS_DORMIR, ms, 0);
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
