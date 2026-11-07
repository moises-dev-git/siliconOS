//! Mostra o conteúdo dos arquivos pedidos, um depois do outro
//! (docs/17-vfs-e-arquivos.md): `rodar cat /leiame.txt /sis/tempo`.
//! Sai com 0 se todos foram lidos, 1 se algum falhou e 2 sem argumentos.
#![no_std]
#![no_main]

use sos::{println, principal, Arquivo};

principal!(main);

fn main() -> u64 {
    if sos::quantos_argumentos() < 2 {
        println!("uso: cat <arquivo>...");
        return 2;
    }
    let mut falhou = false;
    for caminho in sos::argumentos().skip(1) {
        if let Err(e) = mostrar(caminho) {
            println!("cat: {}: {}", caminho, sos::nome_do_erro(e));
            falhou = true;
        }
    }
    falhou as u64
}

/// Lê o arquivo aos pedaços de 64 bytes e escreve cada pedaço na tela. O
/// arquivo é fechado no fim da função, pelo `Drop` de `Arquivo`.
fn mostrar(caminho: &str) -> Result<(), i64> {
    let mut arquivo = Arquivo::abrir(caminho)?;
    let mut pedaco = [0u8; 64];
    loop {
        let n = arquivo.ler(&mut pedaco)?;
        if n == 0 {
            return Ok(()); // fim do arquivo
        }
        sos::escrever(&pedaco[..n]);
    }
}
