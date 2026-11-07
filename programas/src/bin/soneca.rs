//! Acorda 5 vezes, uma a cada meio segundo, e DORME no intervalo: a
//! syscall `dormir` tira o programa da CPU (docs/15-dormir-e-bloqueio.md).
//! Compare com `insone`, que espera o mesmo tempo sem dormir.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    let pid = sos::pid();
    for vez in 1..=5 {
        println!("soneca (pid {}): acordei, vez {} (tique {})", pid, vez, sos::tiques());
        sos::dormir(500);
    }
    0
}
