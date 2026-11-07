//! A porta rápida para o kernel: as instruções `syscall` e `sysret`.
//!
//! Explicação completa em docs/14-syscall-sysret.md.
//!
//! `int 0x80` (doc 10) usa o mecanismo geral de interrupções: a CPU lê a
//! entrada da IDT, confere privilégios, troca de pilha pela TSS e empilha
//! um quadro de 5 valores. `syscall` foi feita só para chamadas de sistema
//! e pula quase tudo isso:
//!
//!   - RCX ← RIP do programa (para onde voltar), R11 ← RFLAGS;
//!   - CS e SS ← seletores do kernel tirados do MSR STAR;
//!   - RFLAGS ← RFLAGS sem os bits do MSR FMASK (desligamos IF);
//!   - RIP ← MSR LSTAR (a nossa `entrada_syscall_rapida`).
//!
//! Não empilha nada e NÃO TROCA DE PILHA: a entrada começa rodando na pilha
//! do programa e precisa trocar sozinha. `sysret` faz o caminho inverso
//! (RIP ← RCX, RFLAGS ← R11, CS/SS de usuário tirados do STAR).

use core::arch::naked_asm;

use crate::cpu;
use crate::gdt;

// Números dos MSRs (manuais da Intel e da AMD).
pub const IA32_EFER: u32 = 0xC000_0080;
pub const IA32_STAR: u32 = 0xC000_0081;
pub const IA32_LSTAR: u32 = 0xC000_0082;
pub const IA32_FMASK: u32 = 0xC000_0084;

/// EFER.SCE (System Call Enable): sem ele, `syscall` é instrução inválida (#UD).
pub const EFER_SCE: u64 = 1 << 0;

/// Bits do RFLAGS que `syscall` desliga ao entrar: IF (interrupções), TF
/// (passo a passo do depurador) e DF (direção das instruções de string).
pub const FMASK: u64 = (1 << 9) | (1 << 8) | (1 << 10);

/// Liga `syscall`/`sysret` e aponta a CPU para a nossa entrada.
pub fn init() {
    // STAR: bits 32–47 = seletor de código do kernel (SS do kernel = esse + 8);
    // bits 48–63 = base para o sysret de 64 bits: CS = base + 16, SS = base + 8,
    // ambos com RPL 3. Com a GDT do doc 03 (0x08 código, 0x10 dados do kernel,
    // 0x18 dados e 0x20 código do usuário), a base é 0x10:
    //   syscall → CS = 0x08, SS = 0x10;   sysret → CS = 0x20|3 = 0x23, SS = 0x18|3 = 0x1B.
    let base_sysret = (gdt::KERNEL_DATA as u64) | 3; // 0x13
    let star = (base_sysret << 48) | ((gdt::KERNEL_CODE as u64) << 32);
    cpu::escrever_msr(IA32_STAR, star);
    cpu::escrever_msr(IA32_LSTAR, entrada_syscall_rapida as *const () as u64);
    cpu::escrever_msr(IA32_FMASK, FMASK);
    cpu::escrever_msr(IA32_EFER, cpu::ler_msr(IA32_EFER) | EFER_SCE);
}

/// RSP do programa durante os primeiros instantes da entrada (antes de
/// haver uma pilha do kernel onde guardá-lo). Basta um, porque há uma CPU só
/// e a entrada roda com as interrupções desligadas.
static mut RSP_DO_PROGRAMA: u64 = 0;

/// A entrada de `syscall`. Monta na pilha do kernel do processo EXATAMENTE o
/// que a `entrada_syscall` do `int 0x80` monta (o quadro que a CPU empilharia
/// e os 15 registradores), para reaproveitar `tratar_syscall` e `Registros`
/// sem mudar nada, e volta com `sysret`.
#[unsafe(naked)]
pub unsafe extern "sysv64" fn entrada_syscall_rapida() {
    naked_asm!(
        // 1. Trocar para a pilha do kernel do processo atual (a mesma do TSS.RSP0).
        "mov [rip + {rsp_programa}], rsp",
        "mov rsp, [rip + {rsp0}]",
        // 2. O quadro que o `int` empilharia: SS, RSP, RFLAGS (em R11), CS, RIP (em RCX).
        "push {ss}",
        "push qword ptr [rip + {rsp_programa}]",
        "push r11",
        "push {cs}",
        "push rcx",
        // 3. Os registradores, na mesma ordem da entrada_syscall (struct Registros).
        "push rax", "push rcx", "push rdx", "push rbx", "push rbp", "push rsi", "push rdi",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",          // 1º argumento: &mut Registros
        "call {tratar}",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rdi", "pop rsi", "pop rbp", "pop rbx", "pop rdx", "pop rcx", "pop rax",
        // 4. Voltar: RIP vai em RCX e RFLAGS em R11 (é de lá que o sysret os
        //    lê); o RSP do programa por último, porque depois dele a pilha do
        //    kernel deixa de estar à mão.
        "mov rcx, [rsp]",        // RIP
        "mov r11, [rsp + 16]",   // RFLAGS
        "mov rsp, [rsp + 24]",   // RSP do programa
        "sysretq",
        rsp_programa = sym RSP_DO_PROGRAMA,
        rsp0 = sym gdt::RSP0_ATUAL,
        ss = const gdt::USER_DATA as u64,
        cs = const gdt::USER_CODE as u64,
        tratar = sym super::tratar_syscall,
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn msrs_configurados() {
        assert!(cpu::ler_msr(IA32_EFER) & EFER_SCE != 0);
        assert_eq!(cpu::ler_msr(IA32_LSTAR), entrada_syscall_rapida as *const () as u64);
        assert_eq!(cpu::ler_msr(IA32_STAR) >> 32, 0x0013_0008);
        assert_eq!(cpu::ler_msr(IA32_FMASK), 0x700);
    }

    #[test_case]
    fn as_duas_portas_funcionam_no_mesmo_programa() {
        // `medir` chama `tiques` 100 000 vezes por `int 0x80` e 100 000 por
        // `syscall`, e imprime pela `syscall`.
        let arquivo = crate::ramdisk::abrir("medir").unwrap();
        let pid = super::super::iniciar_elf(arquivo.nome, arquivo.dados, &["medir"], false).unwrap();
        assert_eq!(crate::escalonador::esperar(pid), super::super::Resultado::Saiu(0));
    }
}
