//! Roda no computador de desenvolvimento ANTES de compilar o kernel
//! (docs/13-disco-em-ram-e-elf.md):
//!
//!   1. compila os programas de usuário (pasta `programas/`) para o alvo
//!      x86_64-unknown-none, gerando um executável ELF para cada um;
//!   2. empacota os ELF num arquivo tar (o "disco em RAM");
//!   3. deixa o tar em OUT_DIR, de onde o kernel o embute com include_bytes!.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let raiz = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let programas = raiz.join("programas");
    let saida = PathBuf::from(env::var("OUT_DIR").unwrap());
    // Um diretório de compilação separado do do kernel: o cargo de fora
    // está com o dele travado enquanto este script roda.
    let alvo_dir = raiz.join("target").join("programas");

    println!("cargo:rerun-if-changed=programas/src");
    println!("cargo:rerun-if-changed=programas/linker.ld");
    println!("cargo:rerun-if-changed=programas/Cargo.toml");
    println!("cargo:rerun-if-changed=programas/.cargo/config.toml");

    // 1. Compilar. As variáveis de ambiente que o cargo de fora define para
    //    este script (flags do kernel UEFI) não podem vazar para o de dentro.
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .current_dir(&programas)
        .args(["build", "--release", "--target-dir"])
        .arg(&alvo_dir)
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_BUILD_TARGET")
        .status()
        .expect("nao consegui rodar o cargo para os programas");
    assert!(status.success(), "a compilacao dos programas de usuario falhou");

    // 2. Os programas são os arquivos de programas/src/bin, em ordem alfabética.
    let mut nomes: Vec<String> = fs::read_dir(programas.join("src/bin"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    nomes.sort();

    let binarios = alvo_dir.join("x86_64-unknown-none").join("release");
    let mut tar = Vec::new();
    for nome in &nomes {
        let dados = fs::read(binarios.join(nome)).unwrap();
        acrescentar_ao_tar(&mut tar, nome, &dados);
    }
    // O fim de um tar: dois blocos de 512 bytes zerados.
    tar.extend_from_slice(&[0u8; 1024]);

    fs::write(saida.join("ramdisk.tar"), &tar).unwrap();
}

/// Um arquivo no formato tar (USTAR): um cabeçalho de 512 bytes e os dados,
/// completados com zeros até um múltiplo de 512.
fn acrescentar_ao_tar(tar: &mut Vec<u8>, nome: &str, dados: &[u8]) {
    assert!(nome.len() < 100, "nome grande demais para o tar: {}", nome);
    let mut cab = [0u8; 512];
    cab[0..nome.len()].copy_from_slice(nome.as_bytes());        // nome
    octal(&mut cab[100..108], 0o755);                            // permissões
    octal(&mut cab[108..116], 0);                                // dono
    octal(&mut cab[116..124], 0);                                // grupo
    octal(&mut cab[124..136], dados.len() as u64);               // tamanho
    octal(&mut cab[136..148], 0);                                // data de modificação
    cab[156] = b'0';                                             // tipo: arquivo comum
    cab[257..263].copy_from_slice(b"ustar\0");                   // assinatura
    cab[263..265].copy_from_slice(b"00");                        // versão
    // Soma de verificação: a soma de todos os bytes do cabeçalho, contando
    // o próprio campo como 8 espaços.
    cab[148..156].copy_from_slice(b"        ");
    let soma: u32 = cab.iter().map(|&b| b as u32).sum();
    octal(&mut cab[148..155], soma as u64);
    cab[155] = b' ';

    tar.extend_from_slice(&cab);
    tar.extend_from_slice(dados);
    let resto = dados.len() % 512;
    if resto != 0 {
        tar.extend(std::iter::repeat_n(0u8, 512 - resto));
    }
}

/// Escreve `valor` em octal, com zeros à esquerda e um byte 0 no fim.
fn octal(campo: &mut [u8], valor: u64) {
    let texto = format!("{:0largura$o}\0", valor, largura = campo.len() - 1);
    campo.copy_from_slice(texto.as_bytes());
}
