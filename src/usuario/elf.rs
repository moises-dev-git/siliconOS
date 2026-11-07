//! Executáveis ELF de 64 bits: ler o cabeçalho e os segmentos, e carregar
//! o programa num espaço de endereços.
//!
//! Explicação completa em docs/13-disco-em-ram-e-elf.md.
//!
//! Um ELF executável tem, no começo, um cabeçalho que diz onde está a
//! tabela de segmentos ("program headers"). Cada segmento PT_LOAD diz:
//! "copie `tam_arquivo` bytes a partir de `deslocamento` no arquivo para o
//! endereço virtual `endereco`, complete com zeros até `tam_memoria`, com
//! estas permissões (R, W, X)". O resto do arquivo (símbolos, nomes de
//! seções) não interessa para rodar.
//!
//!   arquivo:  [cabeçalho][program headers][   .text   ][.rodata][.data]
//!                                           │            │        │
//!   memória:                0x2000_0000_0000 R-X   ...1000 R--  ...2000 RW- [.bss: zeros]

use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao::{self, flags, ErroMapeamento};
use crate::memoria::{quadros, TAMANHO_QUADRO};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroElf {
    /// Não começa com 0x7F 'E' 'L' 'F'.
    NaoEElf,
    /// É ELF, mas não um que sabemos rodar (32 bits, outra CPU, não executável...).
    NaoSuportado(&'static str),
    /// O arquivo acaba antes do que o cabeçalho promete.
    Truncado,
    /// Um segmento sem sentido (tamanho no arquivo maior que na memória, soma que estoura).
    SegmentoInvalido,
    /// Um segmento quer morar na área do kernel ou fora da metade baixa.
    ForaDaAreaDoPrograma,
    /// Dois segmentos querem a mesma página.
    Sobreposto,
    /// O ponto de entrada não está num segmento executável.
    EntradaInvalida,
    /// Faltaram quadros.
    SemMemoria,
}

/// O que nos interessa do cabeçalho ELF (os primeiros 64 bytes).
#[derive(Debug, Clone, Copy)]
pub struct Cabecalho {
    /// Endereço da primeira instrução (`e_entry`).
    pub entrada: u64,
    /// Onde começa a tabela de segmentos no arquivo (`e_phoff`).
    pub ph_inicio: u64,
    /// Quantos segmentos (`e_phnum`).
    pub ph_quantidade: u16,
}

/// Uma entrada da tabela de segmentos ("program header", 56 bytes).
#[derive(Debug, Clone, Copy)]
pub struct Segmento {
    pub tipo: u32,
    pub flags: u32,
    pub deslocamento: u64,
    pub endereco: u64,
    pub tam_arquivo: u64,
    pub tam_memoria: u64,
}

pub const PT_LOAD: u32 = 1;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;
const TAM_PROGRAM_HEADER: u16 = 56;

// Leitura de números little-endian em qualquer posição do arquivo.
fn ler<const N: usize>(dados: &[u8], pos: u64) -> Result<[u8; N], ErroElf> {
    let pos = usize::try_from(pos).map_err(|_| ErroElf::Truncado)?;
    let fim = pos.checked_add(N).ok_or(ErroElf::Truncado)?;
    dados.get(pos..fim).map(|b| b.try_into().unwrap()).ok_or(ErroElf::Truncado)
}
fn u16_em(d: &[u8], p: u64) -> Result<u16, ErroElf> { Ok(u16::from_le_bytes(ler(d, p)?)) }
fn u32_em(d: &[u8], p: u64) -> Result<u32, ErroElf> { Ok(u32::from_le_bytes(ler(d, p)?)) }
fn u64_em(d: &[u8], p: u64) -> Result<u64, ErroElf> { Ok(u64::from_le_bytes(ler(d, p)?)) }

/// Confere a identificação e lê o cabeçalho.
pub fn ler_cabecalho(dados: &[u8]) -> Result<Cabecalho, ErroElf> {
    if dados.get(0..4) != Some(b"\x7fELF") {
        return Err(ErroElf::NaoEElf);
    }
    if ler::<1>(dados, 4)?[0] != 2 {
        return Err(ErroElf::NaoSuportado("nao e de 64 bits"));
    }
    if ler::<1>(dados, 5)?[0] != 1 {
        return Err(ErroElf::NaoSuportado("nao e little-endian"));
    }
    if u16_em(dados, 16)? != 2 {
        return Err(ErroElf::NaoSuportado("nao e executavel de endereco fixo (ET_EXEC)"));
    }
    if u16_em(dados, 18)? != 0x3E {
        return Err(ErroElf::NaoSuportado("nao e para x86-64"));
    }
    if u16_em(dados, 54)? != TAM_PROGRAM_HEADER {
        return Err(ErroElf::NaoSuportado("program header de tamanho estranho"));
    }
    Ok(Cabecalho {
        entrada: u64_em(dados, 24)?,
        ph_inicio: u64_em(dados, 32)?,
        ph_quantidade: u16_em(dados, 56)?,
    })
}

/// O segmento número `i` da tabela.
pub fn segmento(dados: &[u8], cab: &Cabecalho, i: u16) -> Result<Segmento, ErroElf> {
    let base = cab.ph_inicio
        .checked_add(i as u64 * TAM_PROGRAM_HEADER as u64)
        .ok_or(ErroElf::Truncado)?;
    Ok(Segmento {
        tipo: u32_em(dados, base)?,
        flags: u32_em(dados, base + 4)?,
        deslocamento: u64_em(dados, base + 8)?,
        endereco: u64_em(dados, base + 16)?,
        tam_arquivo: u64_em(dados, base + 32)?,
        tam_memoria: u64_em(dados, base + 40)?,
    })
}

/// Carrega todos os segmentos PT_LOAD em `espaco` e devolve o ponto de
/// entrada. Em caso de erro, o que já foi mapeado fica no espaço, e
/// destruir o espaço (quem chamou) devolve tudo.
pub fn carregar(dados: &[u8], espaco: &mut EspacoEnderecos) -> Result<u64, ErroElf> {
    let cab = ler_cabecalho(dados)?;
    for i in 0..cab.ph_quantidade {
        let s = segmento(dados, &cab, i)?;
        if s.tipo == PT_LOAD {
            carregar_segmento(dados, &s, espaco)?;
        }
    }
    // A primeira instrução precisa estar numa página carregada e executável.
    match espaco.traduzir(cab.entrada) {
        Some(t) if t.flags & flags::SEM_EXECUCAO == 0 => Ok(cab.entrada),
        _ => Err(ErroElf::EntradaInvalida),
    }
}

fn carregar_segmento(dados: &[u8], s: &Segmento, espaco: &mut EspacoEnderecos) -> Result<(), ErroElf> {
    // Nunca confiar no arquivo: tamanhos coerentes e nenhuma soma estourando.
    let fim_no_arquivo = s.deslocamento.checked_add(s.tam_arquivo).ok_or(ErroElf::SegmentoInvalido)?;
    let fim_na_memoria = s.endereco.checked_add(s.tam_memoria).ok_or(ErroElf::SegmentoInvalido)?;
    if s.tam_arquivo > s.tam_memoria {
        return Err(ErroElf::SegmentoInvalido);
    }
    if fim_no_arquivo > dados.len() as u64 {
        return Err(ErroElf::Truncado);
    }

    // As permissões da página vêm das flags do segmento.
    let mut bits = flags::USUARIO;
    if s.flags & PF_W != 0 {
        bits |= flags::ESCRITA;
    }
    if s.flags & PF_X == 0 {
        bits |= flags::SEM_EXECUCAO;
    }

    let mut pagina = s.endereco & !(TAMANHO_QUADRO - 1);
    while pagina < fim_na_memoria {
        if !EspacoEnderecos::do_programa(pagina) {
            return Err(ErroElf::ForaDaAreaDoPrograma);
        }
        let q = quadros::alocar().ok_or(ErroElf::SemMemoria)?;
        let destino = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
        unsafe { core::ptr::write_bytes(destino, 0, TAMANHO_QUADRO as usize) };

        // A parte de [endereco, endereco + tam_arquivo) que cai nesta página
        // vem do arquivo; o resto da página fica zerado (é aí que mora a .bss).
        let ini = pagina.max(s.endereco);
        let fim = (pagina + TAMANHO_QUADRO).min(s.endereco + s.tam_arquivo);
        if ini < fim {
            let origem = (s.deslocamento + (ini - s.endereco)) as usize;
            let n = (fim - ini) as usize;
            unsafe {
                core::ptr::copy_nonoverlapping(dados[origem..origem + n].as_ptr(),
                                               destino.add((ini - pagina) as usize), n);
            }
        }

        espaco.mapear(pagina, q, bits).map_err(|e| match e {
            ErroMapeamento::JaMapeado => ErroElf::Sobreposto,
            ErroMapeamento::SemMemoria => ErroElf::SemMemoria,
            _ => ErroElf::ForaDaAreaDoPrograma,
        })?;
        pagina += TAMANHO_QUADRO;
    }
    Ok(())
}

/// Texto curto com as permissões de um segmento: "R-X", "RW-"...
pub fn permissoes(flags: u32) -> [u8; 3] {
    [
        if flags & PF_R != 0 { b'R' } else { b'-' },
        if flags & PF_W != 0 { b'W' } else { b'-' },
        if flags & PF_X != 0 { b'X' } else { b'-' },
    ]
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::escalonador;
    use crate::ramdisk;
    use crate::usuario::Resultado;
    use alloc::vec::Vec;

    fn rodar(nome: &'static str) -> Resultado {
        let pid = crate::usuario::iniciar_elf(nome, ramdisk::abrir(nome).unwrap().dados, false).unwrap();
        escalonador::esperar(pid)
    }

    /// Tenta carregar uma cópia de `oi` alterada por `mexer`, sem rodar.
    fn carregar_alterado(mexer: impl FnOnce(&mut Vec<u8>)) -> Result<u64, ErroElf> {
        let mut copia = Vec::from(ramdisk::abrir("oi").unwrap().dados);
        mexer(&mut copia);
        let mut espaco = EspacoEnderecos::novo().unwrap();
        carregar(&copia, &mut espaco)
    }

    #[test_case]
    fn cabecalho_do_oi() {
        let dados = ramdisk::abrir("oi").unwrap().dados;
        let cab = ler_cabecalho(dados).unwrap();
        let s = segmento(dados, &cab, 0).unwrap();
        assert_eq!((s.tipo, s.endereco), (PT_LOAD, 0x0000_2000_0000_0000));
        assert_eq!(&permissoes(s.flags), b"R-X");
        assert!(cab.entrada >= s.endereco && cab.entrada < s.endereco + s.tam_memoria);
    }

    #[test_case]
    fn programas_em_rust_rodam() {
        assert_eq!(rodar("oi"), Resultado::Saiu(0));
        assert_eq!(rodar("primos"), Resultado::Saiu(1229));
        assert_eq!(rodar("dados"), Resultado::Saiu(42));
    }

    #[test_case]
    fn arquivos_ruins_recusados_sem_vazar() {
        let livres = quadros::livres();
        let ph = |d: &Vec<u8>| u64::from_le_bytes(d[32..40].try_into().unwrap()) as usize;
        assert_eq!(carregar_alterado(|d| d[0] = 0), Err(ErroElf::NaoEElf));
        assert_eq!(carregar_alterado(|d| d[4] = 1), Err(ErroElf::NaoSuportado("nao e de 64 bits")));
        assert_eq!(carregar_alterado(|d| d[18] = 0x28), Err(ErroElf::NaoSuportado("nao e para x86-64")));
        assert_eq!(carregar_alterado(|d| d.truncate(0x1800)), Err(ErroElf::Truncado));
        // O primeiro segmento pedindo para morar no heap do kernel:
        assert_eq!(carregar_alterado(|d| { let p = ph(d) + 16; d[p..p + 8].copy_from_slice(&0xFFFF_9000_0000_0000u64.to_le_bytes()) }),
                   Err(ErroElf::ForaDaAreaDoPrograma));
        // tam_arquivo maior que tam_memoria:
        assert_eq!(carregar_alterado(|d| { let p = ph(d) + 40; d[p..p + 8].copy_from_slice(&1u64.to_le_bytes()) }),
                   Err(ErroElf::SegmentoInvalido));
        // O segundo segmento no mesmo endereço do primeiro:
        assert_eq!(carregar_alterado(|d| { let p = ph(d) + 56 + 16; d[p..p + 8].copy_from_slice(&0x0000_2000_0000_0000u64.to_le_bytes()) }),
                   Err(ErroElf::Sobreposto));
        // Ponto de entrada fora do código: o endereço do segundo segmento (as
        // constantes, R--), lido do próprio arquivo.
        assert_eq!(carregar_alterado(|d| { let p = ph(d) + 56 + 16; let v: [u8; 8] = d[p..p + 8].try_into().unwrap(); d[24..32].copy_from_slice(&v) }),
                   Err(ErroElf::EntradaInvalida));
        assert_eq!(quadros::livres(), livres);
    }
}
