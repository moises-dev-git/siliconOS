//! O disco em RAM (doc 13) visto pelo VFS: cada arquivo do tar é um arquivo
//! da raiz ("/primos", "/leiame.txt").

use alloc::string::String;
use alloc::vec::Vec;

use super::{Conteudo, SistemaDeArquivos};
use crate::ramdisk;

pub struct Disco;

impl SistemaDeArquivos for Disco {
    fn abrir(&self, nome: &str) -> Option<Conteudo> {
        // Os dados já estão no tar, dentro do kernel: nada é copiado.
        ramdisk::abrir(nome).map(|a| Conteudo::Fixo(a.dados))
    }

    fn listar(&self) -> Vec<(String, usize)> {
        ramdisk::arquivos().map(|a| (String::from(a.nome), a.dados.len())).collect()
    }
}
