//! Instruções da CPU usadas por várias partes do kernel: ligar e desligar
//! interrupções, ler e escrever MSRs.
//!
//! Explicação em docs/08-interrupcoes-de-hardware.md.

use core::arch::asm;

/// Bit 9 do RFLAGS: IF ("interrupt flag"). 1 = interrupções de hardware ligadas.
const RFLAGS_IF: u64 = 1 << 9;

// Sem `options(nomem)` de propósito: assim o compilador trata `cli` e `sti`
// como se mexessem em qualquer memória e NÃO move leituras e escritas
// (como a liberação de um lock) para o outro lado deles.

/// `sti`: a CPU passa a aceitar interrupções de hardware.
pub fn ligar_interrupcoes() {
    unsafe { asm!("sti", options(nostack)) };
}

/// `cli`: a CPU ignora interrupções de hardware (elas ficam esperando).
pub fn desligar_interrupcoes() {
    unsafe { asm!("cli", options(nostack)) };
}

/// As interrupções estão ligadas agora? (lê o bit IF do RFLAGS)
pub fn interrupcoes_ligadas() -> bool {
    let rflags: u64;
    unsafe { asm!("pushfq", "pop {}", out(reg) rflags, options(nomem, preserves_flags)) };
    rflags & RFLAGS_IF != 0
}

/// Roda `f` com as interrupções desligadas e depois as devolve ao estado
/// em que estavam (se já estavam desligadas, continuam desligadas).
///
/// É o que impede um tratador de interrupção de tentar pegar um lock que o
/// código interrompido já está segurando (deadlock).
pub fn sem_interrupcoes<R>(f: impl FnOnce() -> R) -> R {
    let estavam_ligadas = interrupcoes_ligadas();
    if estavam_ligadas {
        desligar_interrupcoes();
    }
    let resultado = f();
    if estavam_ligadas {
        ligar_interrupcoes();
    }
    resultado
}

/// Lê um MSR ("model-specific register"): registradores de configuração
/// identificados por número, acessados com `rdmsr` (o número vai em ECX,
/// o valor volta dividido em EDX:EAX).
pub fn ler_msr(numero: u32) -> u64 {
    let (alto, baixo): (u32, u32);
    unsafe {
        asm!("rdmsr", in("ecx") numero, out("edx") alto, out("eax") baixo,
             options(nomem, nostack, preserves_flags));
    }
    (alto as u64) << 32 | baixo as u64
}

/// Reinicia a máquina. Primeiro pelo jeito do PC AT: o controlador do
/// teclado (8042) também controla a linha de RESET da CPU, e o comando 0xFE
/// dá um pulso nela. Se não funcionar, provoca um triple fault de propósito:
/// uma IDT vazia, e qualquer interrupção vira double fault e depois triple
/// fault, que reinicia a CPU.
pub fn reiniciar() -> ! {
    use crate::serial::{inb, outb};
    desligar_interrupcoes();
    unsafe {
        while inb(0x64) & 0b10 != 0 {} // esperar o controlador aceitar comando
        outb(0x64, 0xFE);
    }
    // Ainda aqui? Plano B: triple fault.
    let idt_vazia = crate::gdt::PonteiroTabela { limite: 0, base: 0 };
    unsafe {
        asm!("lidt [{}]", "int3", in(reg) &idt_vazia, options(noreturn));
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn sem_interrupcoes_restaura_o_estado() {
        assert!(interrupcoes_ligadas(), "os testes rodam com as interrupcoes ligadas");
        let dentro = sem_interrupcoes(|| interrupcoes_ligadas());
        assert!(!dentro);
        assert!(interrupcoes_ligadas());
        // Aninhado: o de dentro não pode religar antes da hora.
        sem_interrupcoes(|| {
            sem_interrupcoes(|| {});
            assert!(!interrupcoes_ligadas());
        });
        assert!(interrupcoes_ligadas());
    }

    #[test_case]
    fn msr_do_apic() {
        let base = ler_msr(0x1B);
        assert_eq!(base & 0xFFFF_F000, 0xFEE0_0000);
        assert!(base & (1 << 11) != 0);
    }
}
