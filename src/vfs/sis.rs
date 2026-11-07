//! /sis: arquivos que não existem em lugar nenhum. O conteúdo é escrito
//! pelo kernel no momento em que alguém os abre, a partir do estado atual
//! (a mesma ideia do /proc e do /sys do Linux).
//!
//!   /sis/tempo      há quanto tempo a máquina está ligada
//!   /sis/mem        quadros livres e uso do heap
//!   /sis/processos  a tabela de processos (como o comando `ps`)

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::{Conteudo, SistemaDeArquivos};
use crate::escalonador;
use crate::interrupcoes::timer;
use crate::memoria::{heap, quadros};

pub struct Sis;

/// Os arquivos de /sis e a função que gera cada um.
const ARQUIVOS: &[(&str, fn() -> String)] = &[
    ("tempo", tempo),
    ("mem", mem),
    ("processos", processos),
];

impl SistemaDeArquivos for Sis {
    fn abrir(&self, nome: &str) -> Option<Conteudo> {
        let (_, gerar) = ARQUIVOS.iter().find(|(n, _)| *n == nome)?;
        Some(Conteudo::Gerado(gerar().into_bytes()))
    }

    fn listar(&self) -> Vec<(String, usize)> {
        // O tamanho é o de agora: gerar e medir.
        ARQUIVOS.iter().map(|(n, gerar)| (String::from(*n), gerar().len())).collect()
    }
}

fn tempo() -> String {
    format!("{} ms ({} tiques a {} Hz)\n", timer::milissegundos(), timer::tiques(), timer::FREQUENCIA_HZ)
}

fn mem() -> String {
    let e = heap::estatisticas();
    format!("quadros livres: {}\nheap em uso: {} bytes\n", quadros::livres(), e.em_uso)
}

fn processos() -> String {
    let mut texto = String::from("PID NOME\n");
    for (pid, nome, _, _) in escalonador::listar() {
        texto += &format!("{:>3} {}\n", pid, nome);
    }
    texto
}
