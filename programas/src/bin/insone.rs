//! Faz o mesmo que `soneca` (5 vezes, uma a cada meio segundo), mas espera
//! perguntando a hora sem parar: a "espera ocupada". O tempo de relógio é
//! o mesmo; o tempo de CPU, não (docs/15-dormir-e-bloqueio.md).
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    let pid = sos::pid();
    for vez in 1..=5 {
        println!("insone (pid {}): acordei, vez {} (tique {})", pid, vez, sos::tiques());
        let fim = sos::tiques() + 50; // 50 tiques = 500 ms
        while sos::tiques() < fim {}
    }
    0
}
