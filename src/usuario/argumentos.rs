//! Argumentos de linha de comando (argv) para os programas ELF.
//!
//! Explicação completa em docs/16-argumentos.md.
//!
//! O kernel entrega os argumentos do jeito do System V (o mesmo do Linux):
//! escritos no topo da pilha do programa antes da primeira instrução. Quando
//! o programa começa, RSP aponta para `argc`, e logo acima estão os
//! ponteiros para os textos:
//!
//!   PILHA_ELF_TOPO ─►  ┌──────────────────────────┐
//!                      │ "primos\0" "5000\0"      │ os textos, colados
//!                      ├──────────────────────────┤
//!                      │ (0 a 15 bytes de folga)  │ para RSP ficar alinhado em 16
//!                      ├──────────────────────────┤
//!                      │ 0                        │ fim de envp (ambiente vazio)
//!                      │ 0                        │ fim de argv
//!                      │ argv[1] ─► "5000"        │
//!                      │ argv[0] ─► "primos"      │
//!   RSP inicial ─────► │ argc = 2                 │
//!                      └──────────────────────────┘
//!
//! Tudo isso cabe na última página da pilha (4 KiB): é o limite que
//! `montar` impõe.

use alloc::vec;
use alloc::vec::Vec;

use crate::memoria::TAMANHO_QUADRO;

/// Mais que isto, `montar` recusa (o Linux aceita muito mais; aqui o shell
/// nem consegue digitar tantos).
pub const MAX_ARGUMENTOS: usize = 32;

/// Por que os argumentos não puderam ser entregues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroArgumentos {
    /// Nenhum argumento: falta pelo menos argv[0], o nome do programa.
    Vazio,
    /// Mais de MAX_ARGUMENTOS.
    Demais,
    /// Textos e ponteiros juntos não cabem na última página da pilha.
    GrandesDemais,
    /// Um argumento contém o byte 0, que marca o fim de cada texto.
    TemZero,
}

/// O topo da pilha pronto para ser copiado: `bytes` vai morar em
/// [rsp, topo) no espaço do programa.
#[derive(Debug)]
pub struct PilhaInicial {
    pub bytes: Vec<u8>,
    pub rsp: u64,
}

/// Monta, num vetor do kernel, o topo da pilha de um programa que vai
/// começar com estes argumentos. `topo` é o endereço (no espaço do
/// programa) logo depois do último byte da pilha: os ponteiros de argv são
/// calculados a partir dele.
pub fn montar(argumentos: &[&str], topo: u64) -> Result<PilhaInicial, ErroArgumentos> {
    if argumentos.is_empty() {
        return Err(ErroArgumentos::Vazio);
    }
    if argumentos.len() > MAX_ARGUMENTOS {
        return Err(ErroArgumentos::Demais);
    }
    if argumentos.iter().any(|a| a.as_bytes().contains(&0)) {
        return Err(ErroArgumentos::TemZero);
    }

    // 1. Onde começa a área dos textos (cada um com o 0 do fim).
    let tam_textos: u64 = argumentos.iter().map(|a| a.len() as u64 + 1).sum();
    let inicio_textos = topo - tam_textos;

    // 2. As palavras de 8 bytes: argc, um ponteiro por argumento, o 0 que
    //    fecha argv e o 0 que fecha envp. RSP no começo delas, alinhado em 16.
    let palavras = 1 + argumentos.len() as u64 + 1 + 1;
    let rsp = (inicio_textos - palavras * 8) & !0xF;

    let tamanho = topo - rsp;
    if tamanho > TAMANHO_QUADRO {
        return Err(ErroArgumentos::GrandesDemais);
    }

    // 3. Preencher. `pos(endereco)` = índice no vetor do byte que vai morar
    //    naquele endereço do programa.
    let mut bytes = vec![0u8; tamanho as usize];
    let pos = |endereco: u64| (endereco - rsp) as usize;
    let escrever_u64 = |bytes: &mut Vec<u8>, endereco: u64, valor: u64| {
        bytes[pos(endereco)..pos(endereco) + 8].copy_from_slice(&valor.to_le_bytes());
    };

    escrever_u64(&mut bytes, rsp, argumentos.len() as u64); // argc
    let mut texto = inicio_textos;
    for (i, a) in argumentos.iter().enumerate() {
        escrever_u64(&mut bytes, rsp + 8 + 8 * i as u64, texto); // argv[i]
        bytes[pos(texto)..pos(texto) + a.len()].copy_from_slice(a.as_bytes());
        texto += a.len() as u64 + 1; // o 0 do fim já está lá (vetor zerado)
    }
    // argv[argc] = 0 e envp[0] = 0: também já zerados.
    Ok(PilhaInicial { bytes, rsp })
}

#[cfg(test)]
mod testes {
    use super::*;
    use alloc::string::String;

    fn ler_u64(p: &PilhaInicial, endereco: u64) -> u64 {
        let i = (endereco - p.rsp) as usize;
        u64::from_le_bytes(p.bytes[i..i + 8].try_into().unwrap())
    }

    #[test_case]
    fn pilha_inicial_no_formato_system_v() {
        let topo = 0x0000_8000_0000_0000;
        let p = montar(&["primos", "5000"], topo).unwrap();
        assert_eq!(p.rsp % 16, 0);
        assert_eq!(p.rsp + p.bytes.len() as u64, topo);
        assert_eq!(ler_u64(&p, p.rsp), 2);                 // argc
        assert_eq!(ler_u64(&p, p.rsp + 24), 0);            // argv[2] = 0
        assert_eq!(ler_u64(&p, p.rsp + 32), 0);            // envp[0] = 0
        // Os textos ficam colados no topo: "primos\0" (7) + "5000\0" (5).
        assert_eq!(ler_u64(&p, p.rsp + 8), topo - 12);
        assert_eq!(ler_u64(&p, p.rsp + 16), topo - 5);
        assert_eq!(&p.bytes[p.bytes.len() - 12..], b"primos\x005000\x00");
    }

    #[test_case]
    fn argumentos_ruins_recusados() {
        let topo = 0x0000_8000_0000_0000;
        assert_eq!(montar(&[], topo).unwrap_err(), ErroArgumentos::Vazio);
        assert_eq!(montar(&["a"; MAX_ARGUMENTOS + 1], topo).unwrap_err(), ErroArgumentos::Demais);
        assert_eq!(montar(&["a\0b"], topo).unwrap_err(), ErroArgumentos::TemZero);
        let grande: String = core::iter::repeat_n('x', 4096).collect();
        assert_eq!(montar(&["p", &grande], topo).unwrap_err(), ErroArgumentos::GrandesDemais);
        // O maior que cabe: 4096 bytes no total. 5 palavras (argc, 2
        // ponteiros, os 2 zeros: 40 bytes) + "p\0" (2) + o texto grande com
        // o 0 (4054) = 4096, já alinhado. Um byte a mais e não cabe.
        let cabe: String = core::iter::repeat_n('x', 4053).collect();
        assert_eq!(montar(&["p", &cabe], topo).unwrap().bytes.len(), 4096);
        let nao_cabe: String = core::iter::repeat_n('x', 4054).collect();
        assert_eq!(montar(&["p", &nao_cabe], topo).unwrap_err(), ErroArgumentos::GrandesDemais);
    }
}
