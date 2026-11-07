//! Disco em RAM: um arquivo tar embutido no kernel.
//!
//! Explicação completa em docs/13-disco-em-ram-e-elf.md.
//!
//! O build.rs compila os programas de `programas/` e os empacota no formato
//! tar (USTAR): para cada arquivo, um cabeçalho de 512 bytes seguido dos
//! dados, completados até um múltiplo de 512; no fim, blocos zerados.
//!
//!   [cabeçalho "oi"][dados de oi...][cabeçalho "primos"][dados...]...[000][000]
//!
//! Não há sistema de arquivos de verdade (diretórios, escrita): só leitura,
//! por nome, de arquivos que já estão na memória.

/// O arquivo inteiro, embutido no executável do kernel na compilação.
static RAMDISK: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ramdisk.tar"));

const BLOCO: usize = 512;

/// Um arquivo do disco: nome e conteúdo (fatias do próprio tar: nada é
/// copiado). `'a` é o tempo de vida do tar; para o RAMDISK, `'static`.
#[derive(Clone, Copy)]
pub struct Arquivo<'a> {
    pub nome: &'a str,
    pub dados: &'a [u8],
}

/// Lê um número em octal, como o tar guarda tamanhos (texto ASCII, terminado
/// por 0 ou espaço).
fn ler_octal(campo: &[u8]) -> Option<u64> {
    let mut valor: u64 = 0;
    for &c in campo.iter().take_while(|&&c| c != 0 && c != b' ') {
        if !(b'0'..=b'7').contains(&c) {
            return None;
        }
        valor = valor.checked_mul(8)?.checked_add((c - b'0') as u64)?;
    }
    Some(valor)
}

/// O cabeçalho está íntegro? A soma de todos os bytes, contando o campo da
/// soma (148..156) como espaços, tem que dar o número gravado nele.
fn cabecalho_valido(cab: &[u8]) -> bool {
    let soma: u64 = cab.iter().enumerate()
        .map(|(i, &b)| if (148..156).contains(&i) { b' ' as u64 } else { b as u64 })
        .sum();
    &cab[257..262] == b"ustar" && ler_octal(&cab[148..156]) == Some(soma)
}

/// Percorre os arquivos de um tar (em geral, o RAMDISK).
pub struct Arquivos<'a> {
    tar: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for Arquivos<'a> {
    type Item = Arquivo<'a>;

    fn next(&mut self) -> Option<Arquivo<'a>> {
        let cab = self.tar.get(self.pos..self.pos + BLOCO)?;
        if cab.iter().all(|&b| b == 0) || !cabecalho_valido(cab) {
            return None; // fim do arquivo (ou lixo: paramos por segurança)
        }
        let tamanho_nome = cab[..100].iter().position(|&b| b == 0).unwrap_or(100);
        let nome = core::str::from_utf8(&cab[..tamanho_nome]).ok()?;
        let tamanho = ler_octal(&cab[124..136])? as usize;
        let inicio = self.pos + BLOCO;
        let dados = self.tar.get(inicio..inicio.checked_add(tamanho)?)?;
        // O próximo cabeçalho começa no primeiro múltiplo de 512 depois dos dados.
        self.pos = inicio + tamanho.div_ceil(BLOCO) * BLOCO;
        Some(Arquivo { nome, dados })
    }
}

/// Todos os arquivos do disco em RAM.
pub fn arquivos() -> Arquivos<'static> {
    Arquivos { tar: RAMDISK, pos: 0 }
}

/// O arquivo com esse nome, se existir.
pub fn abrir(nome: &str) -> Option<Arquivo<'static>> {
    arquivos().find(|a| a.nome == nome)
}

/// Tamanho do tar inteiro (para o comando `ls`).
pub fn tamanho_total() -> usize {
    RAMDISK.len()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn os_programas_estao_no_disco() {
        let nomes: alloc::vec::Vec<&str> = arquivos().map(|a| a.nome).collect();
        assert_eq!(nomes, ["dados", "insone", "medir", "oi", "primos", "soneca"]);
        assert!(arquivos().all(|a| a.dados.starts_with(b"\x7fELF")));
        assert!(abrir("nao-existe").is_none());
    }

    #[test_case]
    fn octal_do_tar() {
        assert_eq!(ler_octal(b"00000017331\0"), Some(0o17331));
        assert_eq!(ler_octal(b"0000644 "), Some(0o644));
        assert_eq!(ler_octal(b"0009"), None);
    }

    #[test_case]
    fn cabecalho_estragado_para_a_leitura() {
        // Uma cópia do tar com um byte do primeiro nome trocado: a soma de
        // verificação não bate mais, e a leitura para logo no começo.
        let mut copia = alloc::vec::Vec::from(RAMDISK);
        assert_eq!(Arquivos { tar: &copia, pos: 0 }.count(), arquivos().count());
        copia[0] ^= 0x20;
        assert_eq!(Arquivos { tar: &copia, pos: 0 }.count(), 0);
    }
}
