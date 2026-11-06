//! GDT (Global Descriptor Table) e TSS (Task State Segment).
//!
//! Explicação completa em docs/03-gdt-e-tss.md.
//!
//! Resumo: o firmware UEFI nos deixou rodando com a GDT DELE, que fica em
//! memória que não controlamos. Aqui criamos a nossa própria GDT com:
//!   - segmentos de código/dados do kernel (anel 0)
//!   - segmentos de código/dados de usuário (anel 3, usados no futuro)
//!   - um descritor de TSS, que aponta para pilhas de emergência (IST)
//! e a carregamos na CPU com `lgdt`, recarregando os registradores de
//! segmento e o registrador de tarefa (`ltr`).

use core::arch::asm;
use core::mem::size_of;
use spin::Once;

// ---------------------------------------------------------------------------
// Seletores: "índice na GDT * 8" (+ nível de privilégio nos 2 bits baixos)
// ---------------------------------------------------------------------------
pub const KERNEL_CODE: u16 = 1 << 3; // 0x08
pub const KERNEL_DATA: u16 = 2 << 3; // 0x10
pub const USER_DATA: u16 = (3 << 3) | 3; // 0x1B (RPL 3)
pub const USER_CODE: u16 = (4 << 3) | 3; // 0x23 (RPL 3)
pub const TSS_SEL: u16 = 5 << 3; // 0x28 (ocupa as entradas 5 e 6)

/// Índice da pilha IST usada pelo tratador de double fault (capítulo 5).
/// A CPU numera as IST de 1 a 7; no array `ist` elas vão de 0 a 6.
pub const IST_DOUBLE_FAULT: u16 = 1;

// ---------------------------------------------------------------------------
// Descritores de segmento (64 bits cada)
//
// Em modo 64 bits a CPU ignora base e limite dos segmentos de código/dados;
// o que importa são os bits de acesso e flags. Bits relevantes:
//   bit 47     P   presente
//   bits 45-46 DPL nível de privilégio (0 = kernel, 3 = usuário)
//   bit 44     S   1 = código/dados (0 = sistema, ex.: TSS)
//   bit 43     E   1 = executável (código)
//   bit 41     RW  leitura (código) / escrita (dados)
//   bit 53     L   1 = segmento de código de 64 bits
//   bit 55     G   limite em páginas de 4 KiB
// ---------------------------------------------------------------------------
const P: u64 = 1 << 47;
const S: u64 = 1 << 44;
const E: u64 = 1 << 43;
const RW: u64 = 1 << 41;
const L: u64 = 1 << 53;
const DB: u64 = 1 << 54;
const G: u64 = 1 << 55;
const LIMITE_MAX: u64 = 0xFFFF | (0xF << 48); // limite = 0xFFFFF
const fn dpl(n: u64) -> u64 { n << 45 }

const DESC_KERNEL_CODE: u64 = P | S | E | RW | L | G | LIMITE_MAX | dpl(0);
const DESC_KERNEL_DATA: u64 = P | S | RW | DB | G | LIMITE_MAX | dpl(0);
const DESC_USER_DATA: u64 = P | S | RW | DB | G | LIMITE_MAX | dpl(3);
const DESC_USER_CODE: u64 = P | S | E | RW | L | G | LIMITE_MAX | dpl(3);

// ---------------------------------------------------------------------------
// TSS de 64 bits (formato fixo definido pela Intel/AMD: 104 bytes)
// ---------------------------------------------------------------------------
#[repr(C, packed(4))]
pub struct Tss {
    _reservado0: u32,
    /// Pilhas usadas ao entrar no anel 0/1/2 vindo de um anel menos
    /// privilegiado. rsp[0]: a pilha do kernel quando uma interrupção ou
    /// chamada de sistema chega com a CPU no anel 3 (docs/10-modo-usuario.md).
    pub rsp: [u64; 3],
    _reservado1: u64,
    /// Interrupt Stack Table: 7 pilhas de emergência que a IDT pode pedir.
    pub ist: [u64; 7],
    _reservado2: u64,
    _reservado3: u16,
    /// Deslocamento do mapa de permissões de E/S; = tamanho da TSS significa
    /// "não há mapa" (código de usuário não pode usar in/out).
    pub iomap_base: u16,
}

const _: () = assert!(size_of::<Tss>() == 104);

// Pilha de emergência para o double fault. Fica em memória estática
// (seção .bss do executável): não depende de nenhum alocador.
const TAM_PILHA_IST: usize = 5 * 4096;

#[repr(C, align(16))]
struct Pilha([u8; TAM_PILHA_IST]);

static mut PILHA_DOUBLE_FAULT: Pilha = Pilha([0; TAM_PILHA_IST]);

/// Pilha do anel 0 para interrupções vindas do anel 3 (TSS.rsp[0]).
/// Programas de usuário nunca a veem: a CPU troca de pilha sozinha.
static mut PILHA_ANEL0: Pilha = Pilha([0; TAM_PILHA_IST]);

static TSS: Once<Tss> = Once::new();
static GDT: Once<[u64; 7]> = Once::new();

/// Formato que `lgdt`/`sgdt` usam: limite (tamanho - 1) seguido da base.
#[repr(C, packed(2))]
pub(crate) struct PonteiroTabela {
    pub limite: u16,
    pub base: u64,
}

/// O descritor de TSS ocupa DUAS entradas (128 bits), porque o endereço
/// tem 64 bits e não cabe no formato antigo.
fn descritor_tss(base: u64, limite: u64) -> (u64, u64) {
    let tipo_tss_disponivel: u64 = 0x9; // "TSS de 64 bits, disponível"
    let baixo = (limite & 0xFFFF)
        | ((base & 0xFF_FFFF) << 16)
        | (tipo_tss_disponivel << 40)
        | P
        | (((limite >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    let alto = base >> 32;
    (baixo, alto)
}

pub fn init() {
    let tss = TSS.call_once(|| {
        // A pilha cresce para baixo: a CPU precisa do endereço do TOPO.
        let inicio = &raw const PILHA_DOUBLE_FAULT as u64;
        let mut ist = [0u64; 7];
        ist[(IST_DOUBLE_FAULT - 1) as usize] = inicio + TAM_PILHA_IST as u64;
        let topo_anel0 = &raw const PILHA_ANEL0 as u64 + TAM_PILHA_IST as u64;
        Tss {
            _reservado0: 0,
            rsp: [topo_anel0, 0, 0],
            _reservado1: 0,
            ist,
            _reservado2: 0,
            _reservado3: 0,
            iomap_base: size_of::<Tss>() as u16,
        }
    });

    let gdt = GDT.call_once(|| {
        let (tss_baixo, tss_alto) =
            descritor_tss(tss as *const Tss as u64, (size_of::<Tss>() - 1) as u64);
        [
            0, // entrada 0: obrigatoriamente nula
            DESC_KERNEL_CODE,
            DESC_KERNEL_DATA,
            DESC_USER_DATA,
            DESC_USER_CODE,
            tss_baixo,
            tss_alto,
        ]
    });

    let ponteiro = PonteiroTabela {
        limite: (size_of::<[u64; 7]>() - 1) as u16,
        base: gdt.as_ptr() as u64,
    };

    unsafe {
        // 1) Diz à CPU onde está a nova GDT.
        asm!("lgdt [{}]", in(reg) &ponteiro, options(readonly, nostack, preserves_flags));

        // 2) Recarrega CS. Não existe "mov cs, ax": o único jeito é um salto
        //    "far". Empilhamos (seletor, endereço de retorno) e usamos retfq,
        //    que desempilha os dois e salta para o rótulo 2 com o novo CS.
        asm!(
            "push {sel}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            sel = in(reg) KERNEL_CODE as u64,
            tmp = lateout(reg) _,
            options(preserves_flags),
        );

        // 3) Recarrega os registradores de segmento de dados.
        asm!(
            "mov ss, {0:x}",
            "mov ds, {0:x}",
            "mov es, {0:x}",
            in(reg) KERNEL_DATA,
            options(nostack, preserves_flags),
        );
        asm!(
            "mov fs, {0:x}",
            "mov gs, {0:x}",
            in(reg) 0u16,
            options(nostack, preserves_flags),
        );

        // 4) Carrega o registrador de tarefa: a CPU passa a usar a nossa TSS.
        asm!("ltr {0:x}", in(reg) TSS_SEL, options(nostack, preserves_flags));
    }
}

/// Estado atual dos registradores ligados à GDT (para diagnóstico).
pub struct Estado {
    pub base: u64,
    pub limite: u16,
    pub cs: u16,
    pub ss: u16,
    pub ds: u16,
    pub tr: u16,
}

pub fn estado() -> Estado {
    let mut p = PonteiroTabela { limite: 0, base: 0 };
    let (cs, ss, ds, tr): (u16, u16, u16, u16);
    unsafe {
        asm!("sgdt [{}]", in(reg) &mut p, options(nostack, preserves_flags));
        asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags));
        asm!("mov {0:x}, ss", out(reg) ss, options(nomem, nostack, preserves_flags));
        asm!("mov {0:x}, ds", out(reg) ds, options(nomem, nostack, preserves_flags));
        asm!("str {0:x}", out(reg) tr, options(nomem, nostack, preserves_flags));
    }
    Estado { base: p.base, limite: p.limite, cs, ss, ds, tr }
}

/// Topo da pilha usada ao vir do anel 3 (TSS.rsp[0]).
pub fn topo_pilha_anel0() -> u64 {
    TSS.get().map(|t| { let rsp = t.rsp; rsp[0] }).unwrap_or(0)
}

/// Endereço do topo da pilha IST de double fault (para exibir no boot).
pub fn topo_pilha_double_fault() -> u64 {
    TSS.get().map(|t| { let ist = t.ist; ist[(IST_DOUBLE_FAULT - 1) as usize] }).unwrap_or(0)
}
