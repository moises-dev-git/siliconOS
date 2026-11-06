//! Paginação x86-64 de 4 níveis: tradução de endereços virtuais em físicos.
//!
//! Explicação completa em docs/06-paginacao.md.
//!
//! Resumo:
//! - Um endereço virtual de 48 bits é dividido em 4 índices de 9 bits
//!   (um por nível de tabela) + um deslocamento de 12 bits dentro da página.
//! - Cada tabela tem 512 entradas de 8 bytes = exatamente 4 KiB (um quadro).
//! - O registrador CR3 guarda o endereço FÍSICO da tabela do nível mais alto.
//!
//! O UEFI deixa suas próprias tabelas ativas, marcadas como somente
//! leitura. O `init` monta tabelas novas, do kernel, com:
//!   1. mapeamento identidade (virtual = físico) de toda a RAM, para o
//!      código, a pilha e as tabelas atuais continuarem funcionando;
//!   2. a mesma RAM vista também a partir de DESLOCAMENTO_FISICO, na metade
//!      alta do espaço de endereços (é por ali que o kernel passa a acessar
//!      memória física, como as próprias tabelas de página);
//!   3. o framebuffer;
//! e deixa a página 0 SEM mapeamento, para que ponteiros nulos deem page fault.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

use super::mapa::{Mapa, Tipo};
use super::quadros::{self, Quadro};
use super::TAMANHO_QUADRO;

/// Onde a RAM física inteira aparece no espaço virtual depois do `init`:
/// o físico X fica visível no virtual DESLOCAMENTO_FISICO + X.
/// É o primeiro endereço da metade alta (canônica) do espaço de 48 bits.
pub const DESLOCAMENTO_FISICO: u64 = 0xFFFF_8000_0000_0000;

/// Deslocamento em uso AGORA para converter físico -> virtual.
/// 0 enquanto as tabelas do UEFI (identidade) estão ativas;
/// DESLOCAMENTO_FISICO depois que as nossas assumem.
static DESLOCAMENTO_ATUAL: AtomicU64 = AtomicU64::new(0);

/// Converte um endereço físico num endereço virtual pelo qual o kernel
/// consegue acessá-lo neste momento.
pub fn fisico_para_virtual(fisico: u64) -> u64 {
    fisico + DESLOCAMENTO_ATUAL.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Entradas de tabela
// ---------------------------------------------------------------------------

/// Bits de uma entrada de tabela de páginas (iguais em todos os níveis,
/// exceto GRANDE, que só existe nos níveis 2 e 3).
pub mod flags {
    pub const PRESENTE: u64 = 1 << 0;      // a entrada é válida
    pub const ESCRITA: u64 = 1 << 1;       // permite escrever (senão, só leitura)
    pub const USUARIO: u64 = 1 << 2;       // acessível pelo anel 3
    pub const ESCRITA_DIRETA: u64 = 1 << 3; // cache write-through
    pub const SEM_CACHE: u64 = 1 << 4;     // desliga o cache para esta página
    pub const ACESSADA: u64 = 1 << 5;      // a CPU liga quando a página é lida
    pub const SUJA: u64 = 1 << 6;          // a CPU liga quando a página é escrita
    pub const GRANDE: u64 = 1 << 7;        // página de 2 MiB (nível PD) ou 1 GiB (nível PDPT)
    pub const GLOBAL: u64 = 1 << 8;        // não sai da TLB ao trocar o CR3
    pub const SEM_EXECUCAO: u64 = 1 << 63; // proibido executar código daqui (precisa de EFER.NXE)
}
use flags::*;

/// Bits 12–51 de uma entrada: o endereço físico (alinhado em 4 KiB) do
/// próximo nível ou da página.
const MASCARA_ENDERECO: u64 = 0x000F_FFFF_FFFF_F000;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Entrada(u64);

impl Entrada {
    pub fn presente(self) -> bool { self.0 & PRESENTE != 0 }
    pub fn grande(self) -> bool { self.0 & GRANDE != 0 }
    pub fn endereco(self) -> u64 { self.0 & MASCARA_ENDERECO }
    pub fn flags(self) -> u64 { self.0 & !MASCARA_ENDERECO }
}

/// Uma tabela: 512 entradas de 8 bytes = 4096 bytes, alinhada em 4 KiB.
#[repr(C, align(4096))]
pub struct Tabela {
    pub entradas: [Entrada; 512],
}

const _: () = assert!(core::mem::size_of::<Tabela>() == 4096);

/// Acessa a tabela que está no endereço FÍSICO `fisico`.
fn tabela_em(fisico: u64) -> &'static mut Tabela {
    unsafe { &mut *(fisico_para_virtual(fisico) as *mut Tabela) }
}

/// Pega um quadro livre e zera (uma tabela nova começa sem nenhuma entrada).
fn nova_tabela() -> Option<u64> {
    let q = quadros::alocar()?;
    unsafe { core::ptr::write_bytes(fisico_para_virtual(q.endereco()) as *mut u8, 0, 4096) };
    Some(q.endereco())
}

// ---------------------------------------------------------------------------
// Endereços virtuais
// ---------------------------------------------------------------------------

/// Nomes dos 4 níveis, do mais alto para o mais baixo.
pub const NOMES_NIVEIS: [&str; 4] = ["PML4", "PDPT", "PD", "PT"];

/// Os 4 índices (9 bits cada) que um endereço virtual escolhe, um por nível.
pub fn indices(virt: u64) -> [usize; 4] {
    [
        ((virt >> 39) & 0x1FF) as usize, // PML4: bits 39–47
        ((virt >> 30) & 0x1FF) as usize, // PDPT: bits 30–38
        ((virt >> 21) & 0x1FF) as usize, // PD:   bits 21–29
        ((virt >> 12) & 0x1FF) as usize, // PT:   bits 12–20
    ]
}

/// Os bits 48–63 precisam ser cópia do bit 47 (ver docs/04, #GP).
pub fn canonico(virt: u64) -> bool {
    let alto = virt >> 47;
    alto == 0 || alto == 0x1FFFF
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TamanhoPagina {
    K4,
    M2,
    G1,
}

impl TamanhoPagina {
    pub fn bytes(self) -> u64 {
        match self {
            TamanhoPagina::K4 => 4 * 1024,
            TamanhoPagina::M2 => 2 * 1024 * 1024,
            TamanhoPagina::G1 => 1024 * 1024 * 1024,
        }
    }
    pub fn nome(self) -> &'static str {
        match self {
            TamanhoPagina::K4 => "4 KiB",
            TamanhoPagina::M2 => "2 MiB",
            TamanhoPagina::G1 => "1 GiB",
        }
    }
    /// Em que nível (0 = PML4 ... 3 = PT) fica a entrada final deste tamanho.
    fn nivel_folha(self) -> usize {
        match self {
            TamanhoPagina::G1 => 1,
            TamanhoPagina::M2 => 2,
            TamanhoPagina::K4 => 3,
        }
    }
}

// ---------------------------------------------------------------------------
// Tradução (o que a CPU faz em hardware a cada acesso, aqui em software)
// ---------------------------------------------------------------------------

pub struct Traducao {
    pub fisico: u64,
    pub tamanho: TamanhoPagina,
    pub flags: u64,
}

/// Percorre as tabelas a partir de `raiz` (endereço físico da PML4).
pub fn traduzir_em(raiz: u64, virt: u64) -> Option<Traducao> {
    if !canonico(virt) {
        return None;
    }
    let idx = indices(virt);
    let mut tabela = raiz;
    for nivel in 0..4 {
        let e = tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() {
            return None;
        }
        let tamanho = match nivel {
            1 if e.grande() => Some(TamanhoPagina::G1),
            2 if e.grande() => Some(TamanhoPagina::M2),
            3 => Some(TamanhoPagina::K4),
            _ => None, // ainda não chegou na página: desce mais um nível
        };
        if let Some(t) = tamanho {
            let mascara_pagina = t.bytes() - 1;
            // Em páginas grandes o bit 12 é outro campo (PAT); por isso
            // também zeramos os bits abaixo do tamanho da página.
            let base = e.endereco() & !mascara_pagina;
            return Some(Traducao { fisico: base + (virt & mascara_pagina), tamanho: t, flags: e.flags() });
        }
        tabela = e.endereco();
    }
    None
}

/// Endereço físico da PML4 ativa (o CR3, sem os bits de controle).
pub fn raiz_atual() -> u64 {
    let cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)) };
    cr3 & MASCARA_ENDERECO
}

pub fn traduzir(virt: u64) -> Option<Traducao> {
    traduzir_em(raiz_atual(), virt)
}

// ---------------------------------------------------------------------------
// Mapear e desmapear
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
pub enum ErroMapeamento {
    /// Endereço virtual ou físico não alinhado ao tamanho da página.
    Desalinhado,
    /// Endereço virtual fora das duas metades válidas.
    NaoCanonico,
    /// Já existe algo mapeado ali (ou uma página grande cobre o endereço).
    JaMapeado,
    /// Não está mapeado (ao desmapear).
    NaoMapeado,
    /// O alocador de quadros não tem quadro para uma tabela nova.
    SemMemoria,
}

/// Faz o endereço virtual `virt` apontar para o físico `fisico`, criando as
/// tabelas intermediárias que faltarem (cada uma é um quadro do alocador).
pub fn mapear_em(raiz: u64, virt: u64, fisico: u64, tamanho: TamanhoPagina, bits: u64)
    -> Result<(), ErroMapeamento>
{
    let mascara = tamanho.bytes() - 1;
    if virt & mascara != 0 || fisico & mascara != 0 {
        return Err(ErroMapeamento::Desalinhado);
    }
    if !canonico(virt) {
        return Err(ErroMapeamento::NaoCanonico);
    }
    let idx = indices(virt);
    let folha = tamanho.nivel_folha();
    let mut tabela = raiz;

    // Desce até o nível da folha, criando tabelas no caminho.
    for nivel in 0..folha {
        let e = &mut tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() {
            let nova = nova_tabela().ok_or(ErroMapeamento::SemMemoria)?;
            // Nos níveis intermediários as permissões são as mais abertas:
            // quem restringe é a entrada final (a CPU combina todas com "E").
            *e = Entrada(nova | PRESENTE | ESCRITA | (bits & USUARIO));
        } else if e.grande() {
            return Err(ErroMapeamento::JaMapeado);
        }
        tabela = e.endereco();
    }

    let e = &mut tabela_em(tabela).entradas[idx[folha]];
    if e.presente() {
        return Err(ErroMapeamento::JaMapeado);
    }
    let grande = if tamanho == TamanhoPagina::K4 { 0 } else { GRANDE };
    *e = Entrada(fisico | bits | grande | PRESENTE);
    invalidar(virt);
    Ok(())
}

/// Remove o mapeamento de `virt` e devolve o endereço físico que ele usava.
/// (As tabelas intermediárias que ficarem vazias não são liberadas: é um
/// desperdício pequeno, que podemos resolver no futuro.)
pub fn desmapear_em(raiz: u64, virt: u64) -> Result<u64, ErroMapeamento> {
    if !canonico(virt) {
        return Err(ErroMapeamento::NaoCanonico);
    }
    let idx = indices(virt);
    let mut tabela = raiz;
    for nivel in 0..4 {
        let e = &mut tabela_em(tabela).entradas[idx[nivel]];
        if !e.presente() {
            return Err(ErroMapeamento::NaoMapeado);
        }
        if nivel == 3 || e.grande() {
            let fisico = e.endereco();
            *e = Entrada(0);
            invalidar(virt);
            return Ok(fisico);
        }
        tabela = e.endereco();
    }
    Err(ErroMapeamento::NaoMapeado)
}

pub fn mapear(virt: u64, fisico: u64, tamanho: TamanhoPagina, bits: u64) -> Result<(), ErroMapeamento> {
    mapear_em(raiz_atual(), virt, fisico, tamanho, bits)
}

pub fn desmapear(virt: u64) -> Result<u64, ErroMapeamento> {
    desmapear_em(raiz_atual(), virt)
}

/// Tira o endereço da TLB (o cache de traduções da CPU). Sem isso, a CPU
/// poderia continuar usando a tradução antiga depois de mudarmos a tabela.
fn invalidar(virt: u64) {
    unsafe { asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags)) };
}

// ---------------------------------------------------------------------------
// Montagem das tabelas do kernel
// ---------------------------------------------------------------------------

const DOIS_MIB: u64 = 2 * 1024 * 1024;

pub struct Resumo {
    pub raiz_uefi: u64,
    pub raiz_kernel: u64,
    pub quadros_usados: usize,
}

pub fn init(mapa: &Mapa, framebuffer: u64, tamanho_framebuffer: u64) -> Resumo {
    // Não suportamos paginação de 5 níveis (CR4.LA57); o UEFI do QEMU usa 4.
    let cr4: u64;
    unsafe { asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags)) };
    assert!(cr4 & (1 << 12) == 0, "paginacao de 5 niveis ativa: nao suportado");

    let raiz_uefi = raiz_atual();
    let livres_antes = quadros::livres();
    let pml4 = nova_tabela().expect("sem memoria para a PML4");
    let rw = ESCRITA;

    // 1. Primeiros 2 MiB em páginas de 4 KiB, MENOS a página 0.
    //    Assim, ler ou escrever num ponteiro nulo gera page fault.
    for p in 1..512u64 {
        mapear_em(pml4, p * TAMANHO_QUADRO, p * TAMANHO_QUADRO, TamanhoPagina::K4, rw).unwrap();
    }

    // 2. Toda a RAM em páginas de 2 MiB, duas vezes:
    //    identidade (virt = fis) e na metade alta (virt = DESLOCAMENTO + fis).
    for r in mapa.regioes().iter().filter(|r| !matches!(r.tipo, Tipo::Mmio | Tipo::Reservada)) {
        let mut a = r.inicio & !(DOIS_MIB - 1);          // arredonda para baixo
        let fim = (r.fim() + DOIS_MIB - 1) & !(DOIS_MIB - 1); // arredonda para cima
        while a < fim {
            if a != 0 && traduzir_em(pml4, a).is_none() {
                mapear_em(pml4, a, a, TamanhoPagina::M2, rw).unwrap();
            }
            if traduzir_em(pml4, DESLOCAMENTO_FISICO + a).is_none() {
                mapear_em(pml4, DESLOCAMENTO_FISICO + a, a, TamanhoPagina::M2, rw).unwrap();
            }
            a += DOIS_MIB;
        }
    }

    // 3. O framebuffer (não é RAM, então não estava no passo 2), em identidade.
    let mut a = framebuffer & !(DOIS_MIB - 1);
    while a < framebuffer + tamanho_framebuffer {
        if traduzir_em(pml4, a).is_none() {
            mapear_em(pml4, a, a, TamanhoPagina::M2, rw).unwrap();
        }
        a += DOIS_MIB;
    }

    let quadros_usados = livres_antes - quadros::livres();

    // 4. A troca: a partir da próxima instrução, a CPU traduz com as nossas
    //    tabelas. Tudo que o kernel usa (código, pilha, GDT, IDT, framebuffer,
    //    bitmap) precisa estar mapeado nelas, senão: page fault imediato.
    unsafe { asm!("mov cr3, {}", in(reg) pml4, options(nostack, preserves_flags)) };

    // 5. Daqui em diante, memória física é acessada pela metade alta.
    DESLOCAMENTO_ATUAL.store(DESLOCAMENTO_FISICO, Ordering::Relaxed);
    quadros::reposicionar_bitmap(DESLOCAMENTO_FISICO);

    Resumo { raiz_uefi, raiz_kernel: pml4, quadros_usados }
}

/// Faixa virtual reservada para registradores de dispositivos (MMIO):
/// APIC local, I/O APIC... Entrada 320 da PML4.
pub const MMIO_INICIO: u64 = 0xFFFF_A000_0000_0000;
static PROXIMO_MMIO: AtomicU64 = AtomicU64::new(MMIO_INICIO);

/// Mapeia `tamanho` bytes de registradores de um dispositivo a partir do
/// físico `fisico` e devolve o endereço virtual correspondente.
/// SEM_CACHE: cada leitura e escrita tem que chegar ao dispositivo, nunca
/// ficar no cache da CPU. SEM_EXECUCAO: ali não há código.
pub fn mapear_mmio(fisico: u64, tamanho: u64) -> Result<u64, ErroMapeamento> {
    let pagina = fisico & !(TAMANHO_QUADRO - 1);
    let deslocamento = fisico - pagina;
    let paginas = (deslocamento + tamanho).div_ceil(TAMANHO_QUADRO);
    let virt = PROXIMO_MMIO.fetch_add(paginas * TAMANHO_QUADRO, Ordering::Relaxed);
    for i in 0..paginas {
        mapear(virt + i * TAMANHO_QUADRO, pagina + i * TAMANHO_QUADRO, TamanhoPagina::K4,
               ESCRITA | SEM_CACHE | SEM_EXECUCAO)?;
    }
    Ok(virt + deslocamento)
}

/// Uma página fica livre por quadro do alocador: atalho para os testes.
pub fn alocar_e_mapear(virt: u64, bits: u64) -> Result<Quadro, ErroMapeamento> {
    let q = quadros::alocar().ok_or(ErroMapeamento::SemMemoria)?;
    mapear(virt, q.endereco(), TamanhoPagina::K4, bits)?;
    Ok(q)
}
