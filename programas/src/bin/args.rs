//! Mostra os argumentos que recebeu (docs/16-argumentos.md) e sai com
//! quantos foram, contando o nome: `rodar args a bb ccc` sai com 4.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    let local = 0u64; // uma variável na pilha, para comparar os endereços
    println!("argc = {} (uma variavel local esta em {:#x})", sos::quantos_argumentos(), &local as *const u64 as u64);
    for (i, a) in sos::argumentos().enumerate() {
        // O endereço mostra onde o kernel deixou o texto: no topo da pilha.
        println!("argv[{}] = \"{}\" (em {:#x})", i, a, a.as_ptr() as u64);
    }
    sos::quantos_argumentos() as u64
}
