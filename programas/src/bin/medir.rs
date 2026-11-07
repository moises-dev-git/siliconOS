//! Mede quanto custa uma chamada de sistema: `int 0x80` (doc 10) contra
//! `syscall` (doc 14), contando ciclos da CPU com `rdtsc`.
#![no_std]
#![no_main]

use core::arch::asm;
use sos::{println, principal};

principal!(main);

const VEZES: u64 = 100_000;

/// O contador de ciclos da CPU (Time Stamp Counter).
fn ciclos() -> u64 {
    let (alto, baixo): (u32, u32);
    unsafe { asm!("rdtsc", out("edx") alto, out("eax") baixo, options(nomem, nostack)) };
    (alto as u64) << 32 | baixo as u64
}

/// A mesma chamada `tiques` (número 2), pelo caminho antigo.
fn tiques_int80() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") 2u64 => r, options(nostack)) };
    r
}

/// Ciclos médios por chamada de `f`.
fn medir(f: impl Fn() -> u64) -> u64 {
    f(); // a primeira vez carrega caches e TLB
    let inicio = ciclos();
    for _ in 0..VEZES {
        f();
    }
    (ciclos() - inicio) / VEZES
}

fn main() -> u64 {
    let int80 = medir(tiques_int80);
    let syscall = medir(sos::tiques);
    println!("{} chamadas de cada: int 0x80 = {} ciclos, syscall = {} ciclos por chamada",
             VEZES, int80, syscall);
    0
}
