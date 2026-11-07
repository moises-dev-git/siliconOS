//! O primeiro programa em Rust do SiliconOS: diz oi e sai com 0.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    println!("Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e {}.", sos::pid());
    0
}
