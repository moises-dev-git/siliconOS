//! I/O APIC: recebe as interrupções dos DISPOSITIVOS (teclado, disco,
//! rede...) e as encaminha para o APIC local de alguma CPU.
//!
//! Cada pino de entrada tem uma "entrada de redirecionamento" de 64 bits que
//! diz: qual vetor usar, para qual CPU mandar, como o sinal se comporta e se
//! o pino está mascarado.
//!
//! O I/O APIC só expõe DOIS registradores MMIO: um seletor (IOREGSEL) e uma
//! janela (IOWIN). Para ler o registrador N, escreve-se N no seletor e lê-se
//! a janela.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::memoria::paginacao;

const IOREGSEL: u64 = 0x00;
const IOWIN: u64 = 0x10;

const REG_ID: u32 = 0x00;
const REG_VERSAO: u32 = 0x01;
/// Entrada de redirecionamento do pino n: registradores 0x10 + 2n (bits 0-31)
/// e 0x10 + 2n + 1 (bits 32-63).
const REG_REDIRECIONAMENTO: u32 = 0x10;

// Bits da metade baixa de uma entrada de redirecionamento.
const ATIVO_BAIXO: u32 = 1 << 13;
const POR_NIVEL: u32 = 1 << 15;
const MASCARADA: u32 = 1 << 16;
// Bits 0-7: vetor. Bits 8-10: modo de entrega (000 = fixo). Bit 11: destino
// físico (0). Na metade alta, bits 24-31 (= 56-63): ID do APIC de destino.

static BASE: AtomicU64 = AtomicU64::new(0);
static GSI_BASE: AtomicU64 = AtomicU64::new(0);

fn ler(registrador: u32) -> u32 {
    let base = BASE.load(Ordering::Relaxed);
    unsafe {
        ((base + IOREGSEL) as *mut u32).write_volatile(registrador);
        ((base + IOWIN) as *const u32).read_volatile()
    }
}

fn escrever(registrador: u32, valor: u32) {
    let base = BASE.load(Ordering::Relaxed);
    unsafe {
        ((base + IOREGSEL) as *mut u32).write_volatile(registrador);
        ((base + IOWIN) as *mut u32).write_volatile(valor);
    }
}

pub struct Info {
    pub virtual_: u64,
    pub id: u8,
    pub versao: u8,
    pub pinos: u32,
}

/// Mapeia o I/O APIC e mascara todos os pinos (cada dispositivo será
/// liberado um a um, com `rotear`).
pub fn init(fisico: u64, gsi_base: u32) -> Info {
    let virtual_ = paginacao::mapear_mmio(fisico, 0x20).expect("mapear o I/O APIC");
    BASE.store(virtual_, Ordering::Relaxed);
    GSI_BASE.store(gsi_base as u64, Ordering::Relaxed);

    let versao = ler(REG_VERSAO);
    let pinos = ((versao >> 16) & 0xFF) + 1; // bits 16-23: índice da última entrada
    for pino in 0..pinos {
        escrever(REG_REDIRECIONAMENTO + 2 * pino, MASCARADA);
    }
    Info { virtual_, id: (ler(REG_ID) >> 24) as u8 & 0x0F, versao: versao as u8, pinos }
}

/// Faz a interrupção global `gsi` chegar como `vetor` no APIC `destino`.
pub fn rotear(gsi: u32, vetor: u8, destino: u8, ativo_baixo: bool, por_nivel: bool) {
    let pino = gsi - GSI_BASE.load(Ordering::Relaxed) as u32;
    let mut baixo = vetor as u32; // modo fixo, destino físico, não mascarada
    if ativo_baixo { baixo |= ATIVO_BAIXO; }
    if por_nivel { baixo |= POR_NIVEL; }
    // Primeiro a metade alta (destino), depois a baixa (que desmascara).
    escrever(REG_REDIRECIONAMENTO + 2 * pino + 1, (destino as u32) << 24);
    escrever(REG_REDIRECIONAMENTO + 2 * pino, baixo);
}

/// A entrada de redirecionamento atual do pino (para diagnóstico).
pub fn entrada(gsi: u32) -> u64 {
    let pino = gsi - GSI_BASE.load(Ordering::Relaxed) as u32;
    (ler(REG_REDIRECIONAMENTO + 2 * pino + 1) as u64) << 32 | ler(REG_REDIRECIONAMENTO + 2 * pino) as u64
}
