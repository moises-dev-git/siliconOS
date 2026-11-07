//! Mostra as seções .data e .bss: variáveis globais com valor inicial (que
//! vêm do arquivo) e zeradas (que o arquivo NÃO guarda; o kernel zera).
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

/// Em .data: o valor 41 está gravado no arquivo ELF.
static mut CONTADOR: u64 = 41;
/// Em .bss: 64 KiB de zeros que não ocupam nada no arquivo.
static mut TABELA: [u64; 8192] = [0; 8192];

fn main() -> u64 {
    // `&raw` e leituras/escritas explícitas: `static mut` não pode virar
    // referência comum (dois pedaços do programa poderiam mudá-lo ao mesmo tempo).
    let tabela = unsafe { &mut *&raw mut TABELA };
    let zerada = tabela.iter().all(|&x| x == 0);
    for (i, x) in tabela.iter_mut().enumerate() {
        *x = i as u64;
    }
    let soma: u64 = tabela.iter().sum();
    let contador = unsafe {
        CONTADOR += 1;
        CONTADOR
    };
    println!(".bss zerada: {}; soma depois de escrever: {}; .data: 41 -> {}",
             if zerada { "sim" } else { "NAO" }, soma, contador);
    contador
}
