//! Conta os números primos abaixo de um limite e sai com a quantidade.
//! O limite vem do primeiro argumento (doc 16): `rodar primos 5000`. Sem
//! argumento, 10 000 (o que o programa fazia desde o doc 13).
#![no_std]
#![no_main]

use sos::{print, println, principal};

principal!(main);

fn primo(n: u64) -> bool {
    n >= 2 && (2..).take_while(|d| d * d <= n).all(|d| n % d != 0)
}

fn main() -> u64 {
    let limite = match sos::argumento(1) {
        None => 10_000,
        Some(texto) => match texto.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                println!("uso: primos [limite]   ('{}' nao e um numero)", texto);
                return u64::MAX; // o shell mostra -1
            }
        },
    };
    let inicio = sos::tiques();
    let mut quantos = 0;
    print!("primeiros primos:");
    for n in 0..limite {
        if primo(n) {
            quantos += 1;
            if quantos <= 10 {
                print!(" {}", n);
            }
        }
    }
    println!();
    println!("{} primos abaixo de {} ({} tiques)", quantos, limite, sos::tiques() - inicio);
    quantos
}
