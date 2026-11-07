//! Manda ao kernel pedidos de arquivo errados de propósito e confere a
//! resposta de cada um (docs/17-vfs-e-arquivos.md). Sai com o número de
//! provas que falharam: 0 = o kernel se defendeu de todas.
#![no_std]
#![no_main]

use sos::{cru, println, principal, Arquivo};

principal!(main);

/// Uma prova: o que foi tentado, a resposta do kernel e a esperada.
fn prova(falhas: &mut u64, descricao: &str, obtido: i64, esperado: i64) {
    let ok = obtido == esperado;
    println!("  {:<44} {:>4} {}", descricao, obtido, if ok { "ok" } else { "FALHOU" });
    if !ok {
        *falhas += 1;
    }
}

fn main() -> u64 {
    let mut falhas = 0;
    let f = &mut falhas;

    // Caminhos ruins.
    let erro = |r: Result<Arquivo, i64>| r.err().unwrap_or(0);
    prova(f, "abrir um arquivo que nao existe", erro(Arquivo::abrir("/nao-existe")), -2);
    prova(f, "abrir um diretorio (/sis)", erro(Arquivo::abrir("/sis")), -2);
    prova(f, "abrir com o caminho na memoria do kernel", cru::abrir(0xFFFF_8000_0000_0000, 8), -14);
    let longo = [b'a'; 300];
    prova(f, "abrir com um caminho de 300 bytes", cru::abrir(longo.as_ptr() as u64, 300), -36);

    // Descritores que não estão abertos.
    prova(f, "ler do descritor 99", cru::ler(99, longo.as_ptr() as u64, 1), -9);
    prova(f, "ler do descritor 1 (reservado)", cru::ler(1, longo.as_ptr() as u64, 1), -9);
    prova(f, "fechar o descritor 0 (reservado)", cru::fechar(0), -9);

    // Destinos ruins para `ler`: o kernel teria de ESCREVER ali.
    let mut a = Arquivo::abrir("/leiame.txt").unwrap();
    let d = a.descritor();
    let codigo = main as *const () as u64; // uma página de código: só leitura
    prova(f, "ler para dentro do proprio codigo", cru::ler(d, codigo, 16), -14);
    prova(f, "ler para dentro do kernel", cru::ler(d, 0xFFFF_8000_0000_0000, 16), -14);
    let mut buf = [0u8; 16];
    prova(f, "ler 16 bytes para a pilha (certo)", a.ler(&mut buf).unwrap_or(0) as i64, 16);
    prova(f, "...e as leituras recusadas nao avancaram", buf.starts_with(b"Este arquivo mor") as i64, 1);

    // Fechar duas vezes.
    prova(f, "fechar o descritor", cru::fechar(d), 0);
    prova(f, "fechar o mesmo descritor de novo", cru::fechar(d), -9);
    core::mem::forget(a); // já fechado à mão: o Drop não deve fechar de novo

    // Encher a tabela (16 arquivos por processo).
    let mut abertos = 0;
    let mut ultimo = 0;
    for _ in 0..20 {
        ultimo = cru::abrir("/sis/tempo".as_ptr() as u64, 10);
        if ultimo < 0 { break; }
        abertos += 1;
    }
    prova(f, "quantos arquivos abrem ao mesmo tempo", abertos, 16);
    prova(f, "o 17o e recusado", ultimo, -24);

    // Os 16 ficam abertos de propósito: o kernel tem de liberá-los quando o
    // processo for recolhido (o teste do kernel confere o heap).
    println!("conferir: {} falhas (16 arquivos ficam abertos de proposito)", falhas);
    falhas
}
