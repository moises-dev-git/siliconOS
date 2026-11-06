//! IDT (Interrupt Descriptor Table) e tratadores de exceções da CPU.
//!
//! Explicação completa em docs/04-idt-e-excecoes.md.
//!
//! A IDT tem 256 entradas (vetores 0–255). Os vetores 0–31 são reservados
//! para EXCEÇÕES da CPU (divisão por zero, page fault...). Os demais ficam
//! para interrupções de hardware e software: o timer (0x30) e o teclado
//! (0x31) estão em src/interrupcoes/ (docs/08-interrupcoes-de-hardware.md).
//!
//! Os tratadores usam a convenção de chamada `extern "x86-interrupt"`
//! (recurso do Rust nightly): o compilador gera o código que salva todos os
//! registradores e termina com `iretq` em vez de `ret`.

use core::arch::asm;
use core::fmt;
use core::mem::size_of;
use spin::Once;

use crate::gdt;
use crate::kprintln;

// ---------------------------------------------------------------------------
// Formato de uma entrada da IDT (16 bytes em modo 64 bits)
// ---------------------------------------------------------------------------
#[derive(Clone, Copy)]
#[repr(C)]
struct Entrada {
    endereco_0_15: u16,
    seletor: u16,      // segmento de código em que o tratador roda (0x08)
    ist: u8,           // bits 0–2: qual pilha IST usar (0 = nenhuma)
    tipo_atributos: u8, // P, DPL e tipo do "portão" (gate)
    endereco_16_31: u16,
    endereco_32_63: u32,
    _reservado: u32,
}

const _: () = assert!(size_of::<Entrada>() == 16);

/// P=1, DPL=0, tipo 0xE = "interrupt gate" de 64 bits
/// (desliga as interrupções automaticamente ao entrar no tratador).
const INTERRUPT_GATE: u8 = 0x8E;

impl Entrada {
    const VAZIA: Entrada = Entrada {
        endereco_0_15: 0, seletor: 0, ist: 0, tipo_atributos: 0,
        endereco_16_31: 0, endereco_32_63: 0, _reservado: 0,
    };

    fn new(tratador: u64, ist: u16) -> Entrada {
        Entrada {
            endereco_0_15: tratador as u16,
            seletor: gdt::KERNEL_CODE,
            ist: ist as u8,
            tipo_atributos: INTERRUPT_GATE,
            endereco_16_31: (tratador >> 16) as u16,
            endereco_32_63: (tratador >> 32) as u32,
            _reservado: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// O que a CPU empilha ao entrar num tratador
// ---------------------------------------------------------------------------
#[repr(C)]
pub struct QuadroInterrupcao {
    pub rip: u64,    // instrução que causou a exceção (ou a próxima, nas "traps")
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,    // pilha de quem foi interrompido
    pub ss: u64,
}

impl fmt::Display for QuadroInterrupcao {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RIP={:#x} CS={:#x} RFLAGS={:#x} RSP={:#x} SS={:#x}",
               self.rip, self.cs, self.rflags, self.rsp, self.ss)
    }
}

// Vetores das exceções que tratamos
const DIVISAO: usize = 0;
const BREAKPOINT: usize = 3;
const OPCODE_INVALIDO: usize = 6;
const DOUBLE_FAULT: usize = 8;
const PROTECAO_GERAL: usize = 13;
const PAGE_FAULT: usize = 14;

static IDT: Once<[Entrada; 256]> = Once::new();

pub fn init() {
    let idt = IDT.call_once(|| {
        let mut t = [Entrada::VAZIA; 256];
        t[DIVISAO] = Entrada::new(divisao as *const () as u64, 0);
        t[BREAKPOINT] = Entrada::new(breakpoint as *const () as u64, 0);
        t[OPCODE_INVALIDO] = Entrada::new(opcode_invalido as *const () as u64, 0);
        // O double fault roda na pilha de emergência da TSS (IST1).
        t[DOUBLE_FAULT] = Entrada::new(double_fault as *const () as u64, gdt::IST_DOUBLE_FAULT);
        t[PROTECAO_GERAL] = Entrada::new(protecao_geral as *const () as u64, 0);
        t[PAGE_FAULT] = Entrada::new(page_fault as *const () as u64, 0);

        // Interrupções de hardware (docs/08-interrupcoes-de-hardware.md).
        use crate::interrupcoes::{self as int, teclado, timer};
        t[int::VETOR_TIMER as usize] = Entrada::new(timer::tratador as *const () as u64, 0);
        t[int::VETOR_TECLADO as usize] = Entrada::new(teclado::tratador as *const () as u64, 0);
        for v in [int::VETOR_PIC_ESPURIO_1, int::VETOR_PIC_ESPURIO_2, int::VETOR_APIC_ESPURIO] {
            t[v as usize] = Entrada::new(int::espuria as *const () as u64, 0);
        }
        t
    });

    let ponteiro = gdt::PonteiroTabela {
        limite: (size_of::<[Entrada; 256]>() - 1) as u16,
        base: idt.as_ptr() as u64,
    };
    unsafe {
        asm!("lidt [{}]", in(reg) &ponteiro, options(readonly, nostack, preserves_flags));
    }
}

/// Base e limite da IDT ativa (lidos com `sidt`, para diagnóstico).
pub fn estado() -> (u64, u16) {
    let mut p = gdt::PonteiroTabela { limite: 0, base: 0 };
    unsafe { asm!("sidt [{}]", in(reg) &mut p, options(nostack, preserves_flags)) };
    (p.base, p.limite)
}

// ---------------------------------------------------------------------------
// Tratadores
// ---------------------------------------------------------------------------

/// #BP — instrução `int3`. É uma "trap": depois do tratador, a execução
/// CONTINUA na instrução seguinte. Depuradores usam isso para breakpoints.
extern "x86-interrupt" fn breakpoint(quadro: QuadroInterrupcao) {
    kprintln!("[excecao] BREAKPOINT (#3) em {:#x} - continuando", quadro.rip);
}

/// #DE — divisão (inteira) por zero, ou resultado grande demais para o registrador.
extern "x86-interrupt" fn divisao(quadro: QuadroInterrupcao) {
    panic!("EXCECAO #0 DIVISAO POR ZERO\n{}", quadro);
}

/// #UD — a CPU encontrou bytes que não formam uma instrução válida.
extern "x86-interrupt" fn opcode_invalido(quadro: QuadroInterrupcao) {
    panic!("EXCECAO #6 OPCODE INVALIDO\n{}", quadro);
}

/// #GP — "proteção geral": a categoria genérica de "você não pode fazer isso"
/// (seletor inválido, endereço não-canônico, instrução privilegiada no anel 3,
/// vetor da IDT sem tratador...). O código de erro, quando não é zero,
/// indica o seletor ou vetor envolvido.
extern "x86-interrupt" fn protecao_geral(quadro: QuadroInterrupcao, codigo: u64) {
    if codigo == 0 {
        // Nenhum seletor envolvido: endereço não-canônico, instrução
        // privilegiada, bit reservado num registrador de controle...
        panic!("EXCECAO #13 PROTECAO GERAL (codigo=0: nenhum seletor envolvido)\n{}", quadro);
    }
    let origem = match (codigo >> 1) & 0b11 {
        0b00 => "GDT",
        0b10 => "LDT",
        _ => "IDT",
    };
    panic!("EXCECAO #13 PROTECAO GERAL (codigo={:#x}: indice {} na {})\n{}",
           codigo, codigo >> 3, origem, quadro);
}

/// #PF — acesso a um endereço cuja página não está mapeada (ou sem permissão).
/// O endereço que causou a falha fica no registrador CR2.
extern "x86-interrupt" fn page_fault(quadro: QuadroInterrupcao, codigo: u64) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)) };
    panic!("EXCECAO #14 PAGE FAULT ao {} {:#x} ({}, {}{})\n{}",
           if codigo & 0x10 != 0 { "executar" } else if codigo & 0x2 != 0 { "escrever em" } else { "ler" },
           cr2,
           if codigo & 0x1 != 0 { "pagina presente, sem permissao" } else { "pagina nao mapeada" },
           if codigo & 0x4 != 0 { "modo usuario" } else { "modo kernel" },
           if codigo & 0x8 != 0 { ", bit reservado na tabela" } else { "" },
           quadro);
}

/// #DF — uma exceção aconteceu ENQUANTO a CPU tentava entregar outra.
/// Roda na pilha IST1, então funciona mesmo se a pilha normal estiver
/// destruída. Não há como continuar: o tipo de retorno é `!`.
extern "x86-interrupt" fn double_fault(quadro: QuadroInterrupcao, _codigo: u64) -> ! {
    let rsp_atual: u64;
    unsafe { asm!("mov {}, rsp", out(reg) rsp_atual, options(nomem, nostack, preserves_flags)) };
    panic!("EXCECAO #8 DOUBLE FAULT\n{}\nTratador rodando na pilha IST1 (RSP={:#x})",
           quadro, rsp_atual);
}
