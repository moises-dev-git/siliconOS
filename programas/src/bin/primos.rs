//! Conta os números primos abaixo de 10 000 e sai com a quantidade.
#![no_std]
#![no_main]

use sos::{print, println, principal};

principal!(main);

fn primo(n: u64) -> bool {
    n >= 2 && (2..).take_while(|d| d * d <= n).all(|d| n % d != 0)
}

fn main() -> u64 {
    let inicio = sos::tiques();
    let mut quantos = 0;
    print!("primeiros primos:");
    for n in 0..10_000 {
        if primo(n) {
            quantos += 1;
            if quantos <= 10 {
                print!(" {}", n);
            }
        }
    }
    println!();
    println!("{} primos abaixo de 10000 ({} tiques)", quantos, sos::tiques() - inicio);
    quantos
}
