//! Modo usuário: rodar um programa no anel 3 e atender as chamadas de
//! sistema dele.
//!
//! Explicação completa em docs/10-modo-usuario.md.
//!
//! O ciclo de vida de um programa (desde o doc 12, com o escalonador):
//!
//!   shell: rodar ola
//!     └─ iniciar()                     cria um espaço de endereços (doc 11), mapeia código e
//!                                      pilha com o bit USUARIO, entrega ao escalonador
//!     └─ escalonador::esperar(pid)     cede a CPU até o programa terminar
//!               ═══════ anel 3 ═══════ o programa roda quando é a vez dele; int 0x80 = chamada
//!                                      de sistema; o timer o troca pelos outros a cada tique
//!     └─ recolher                      destrói a pilha e o espaço, devolve o resultado
//!
//! O programa termina por um de três caminhos, e todos acabam em
//! `escalonador::encerrar`: a chamada `sair`, uma exceção da CPU no anel 3,
//! ou o timer (tempo de CPU esgotado).

pub mod programas;

use alloc::format;
use alloc::string::String;
use core::arch::naked_asm;

use crate::escalonador::{self, Pid};
use crate::idt::QuadroInterrupcao;
use crate::interrupcoes::timer;
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao::{self, flags};
use crate::memoria::{quadros, TAMANHO_QUADRO};
use crate::{kprint, kprintln};

/// Onde o código do programa fica (entrada 64 da PML4, metade baixa: nada
/// do kernel mora ali).
pub const CODIGO_VIRT: u64 = 0x0000_2000_0000_0000;
/// A página da pilha do programa; a pilha começa no fim dela.
pub const PILHA_VIRT: u64 = 0x0000_2000_0010_0000;
pub const VETOR_SYSCALL: u8 = 0x80;
/// Tempo de CPU (tiques no anel 3) além do qual o programa é encerrado.
pub const LIMITE_TIQUES: u64 = 2 * timer::FREQUENCIA_HZ;

// Números das chamadas de sistema (RAX).
pub const SYS_SAIR: u64 = 0;
pub const SYS_ESCREVER: u64 = 1;
pub const SYS_TIQUES: u64 = 2;
pub const SYS_PID: u64 = 3;

// Erros devolvidos em RAX, negativos como no Linux.
pub const ERRO_ENDERECO: u64 = -14i64 as u64;  // EFAULT: ponteiro inválido
pub const ERRO_SYSCALL: u64 = -38i64 as u64;   // ENOSYS: chamada desconhecida

/// Como um programa terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resultado {
    /// Chamou `sair(codigo)`.
    Saiu(u64),
    /// Causou uma exceção da CPU e o kernel o encerrou.
    Falha { excecao: &'static str, rip: u64 },
    /// Ficou com a CPU mais de LIMITE_TIQUES sem terminar.
    TempoEsgotado,
}

/// Texto curto que descreve como um programa terminou (shell e escalonador).
pub fn descrever(r: Resultado) -> String {
    match r {
        Resultado::Saiu(codigo) => format!("terminou: codigo de saida {}", codigo as i64),
        Resultado::Falha { excecao, rip } => format!("encerrado pelo kernel: {} em {:#x}", excecao, rip),
        Resultado::TempoEsgotado => format!("encerrado pelo kernel: usou {} s de CPU sem terminar",
                                            LIMITE_TIQUES / timer::FREQUENCIA_HZ),
    }
}

// ---------------------------------------------------------------------------
// Rodar um programa
// ---------------------------------------------------------------------------

/// Prepara o programa num espaço de endereços próprio e o entrega ao
/// escalonador. Volta na hora, com o pid; o programa roda quando chegar a
/// vez dele (doc 12).
pub fn iniciar(programa: &programas::Programa, segundo_plano: bool) -> Pid {
    let codigo = programa.codigo();
    assert!(codigo.len() <= TAMANHO_QUADRO as usize, "programa maior que uma pagina");

    // Um espaço de endereços só deste programa (doc 11): uma PML4 nova,
    // com o kernel copiado e a metade do programa vazia.
    let mut espaco = EspacoEnderecos::novo().expect("sem quadros para a PML4 do programa");

    // Uma página de código (só leitura, executável) e uma de pilha
    // (escrita, sem execução), as duas acessíveis pelo anel 3. O espaço
    // fica dono dos quadros.
    let q_codigo = nova_pagina_zerada();
    unsafe {
        let destino = paginacao::fisico_para_virtual(q_codigo.endereco()) as *mut u8;
        core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
    }
    espaco.mapear(CODIGO_VIRT, q_codigo, flags::USUARIO).expect("mapear o codigo do programa");
    espaco.mapear(PILHA_VIRT, nova_pagina_zerada(),
                  flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO).expect("mapear a pilha");

    escalonador::criar(programa.nome, espaco, CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO, segundo_plano)
}

/// Roda o programa e espera ele terminar (o que o shell faz sem `&`).
pub fn rodar(programa: &programas::Programa) -> Resultado {
    let pid = iniciar(programa, false);
    escalonador::esperar(pid)
}

fn nova_pagina_zerada() -> quadros::Quadro {
    let q = quadros::alocar().expect("sem quadros para o programa");
    // Um quadro reaproveitado tem os dados de quem o usou antes: zerar, para
    // o programa não ler restos do kernel.
    unsafe { core::ptr::write_bytes(paginacao::fisico_para_virtual(q.endereco()) as *mut u8, 0, 4096) };
    q
}

/// Chamada pelos tratadores de exceção (idt.rs): se a exceção aconteceu no
/// anel 3, a culpa é do programa, não do kernel. Encerra só o programa.
pub fn falha_do_programa(quadro: &QuadroInterrupcao, excecao: &'static str, detalhes: core::fmt::Arguments) {
    if quadro.cs & 3 == 3 {
        kprintln!("[kernel] {} no programa (RIP={:#x}, CS={:#x}): {}", excecao, quadro.rip, quadro.cs, detalhes);
        escalonador::encerrar(Resultado::Falha { excecao, rip: quadro.rip });
    }
}

// ---------------------------------------------------------------------------
// Chamadas de sistema: int 0x80
// ---------------------------------------------------------------------------

/// Os registradores do programa, como a entrada de syscall os empilha
/// (do endereço mais baixo para o mais alto), seguidos do quadro da CPU.
#[repr(C)]
pub struct Registros {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9: u64, pub r8: u64,
    pub rdi: u64, pub rsi: u64, pub rbp: u64, pub rbx: u64,
    pub rdx: u64, pub rcx: u64, pub rax: u64,
    // empilhado pela CPU:
    pub rip: u64, pub cs: u64, pub rflags: u64, pub rsp: u64, pub ss: u64,
}

/// O tratador do vetor 0x80 (registrado na IDT com DPL 3, senão o `int 0x80`
/// do anel 3 daria #GP). Salva todos os registradores, chama o Rust com um
/// ponteiro para eles e, na volta, os restaura (RAX com o resultado).
#[unsafe(naked)]
pub extern "sysv64" fn entrada_syscall() {
    naked_asm!(
        "push rax", "push rcx", "push rdx", "push rbx", "push rbp", "push rsi", "push rdi",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",          // 1º argumento: &mut Registros
        "call {tratar}",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rdi", "pop rsi", "pop rbp", "pop rbx", "pop rdx", "pop rcx", "pop rax",
        "iretq",
        tratar = sym tratar_syscall,
    )
}

extern "sysv64" fn tratar_syscall(r: &mut Registros) {
    r.rax = match r.rax {
        SYS_SAIR => escalonador::encerrar(Resultado::Saiu(r.rdi)),
        SYS_ESCREVER => escrever(r.rdi, r.rsi),
        SYS_TIQUES => timer::tiques(),
        SYS_PID => escalonador::pid_atual() as u64,
        _ => ERRO_SYSCALL,
    };
}

/// O ponteiro [inicio, inicio + tamanho) está inteiro dentro de uma página
/// do programa? Nunca confiar num ponteiro vindo do anel 3.
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    [CODIGO_VIRT, PILHA_VIRT].iter().any(|&p| inicio >= p && fim <= p + TAMANHO_QUADRO)
}

fn escrever(ponteiro: u64, tamanho: u64) -> u64 {
    if !faixa_do_programa(ponteiro, tamanho) {
        kprintln!("[kernel] escrever(): ponteiro {:#x} fora da memoria do programa, recusado", ponteiro);
        return ERRO_ENDERECO;
    }
    let bytes = unsafe { core::slice::from_raw_parts(ponteiro as *const u8, tamanho as usize) };
    for &b in bytes {
        kprint!("{}", if b.is_ascii() { b as char } else { '?' });
    }
    tamanho
}

#[cfg(test)]
mod testes {
    use super::*;

    fn rodar_por_nome(nome: &str) -> Resultado {
        rodar(programas::procurar(nome).unwrap())
    }

    #[test_case]
    fn ola_sai_com_42() {
        assert_eq!(rodar_por_nome("ola"), Resultado::Saiu(42));
    }

    #[test_case]
    fn relogio_usa_syscall_de_tiques() {
        let antes = timer::tiques();
        assert_eq!(rodar_por_nome("relogio"), Resultado::Saiu(100));
        assert!(timer::tiques() - antes >= 100);
    }

    #[test_case]
    fn ponteiro_do_kernel_recusado() {
        assert_eq!(rodar_por_nome("ponteiro"), Resultado::Saiu(1));
    }

    #[test_case]
    fn invasor_encerrado_por_page_fault() {
        assert!(matches!(rodar_por_nome("invasor"), Resultado::Falha { excecao: "PAGE FAULT", .. }));
    }

    #[test_case]
    fn privilegiado_encerrado_por_gp() {
        assert!(matches!(rodar_por_nome("privilegiado"), Resultado::Falha { excecao: "PROTECAO GERAL", .. }));
    }

    #[test_case]
    fn laco_encerrado_pelo_timer() {
        assert_eq!(rodar_por_nome("laco"), Resultado::TempoEsgotado);
    }

    #[test_case]
    fn nada_fica_mapeado_nem_vaza() {
        // Desde o doc 11 não precisa de aquecimento: as tabelas do programa
        // morrem com o espaço dele, então a primeira execução também não
        // deixa nada para trás.
        let livres = quadros::livres();
        rodar_por_nome("ola");
        assert!(paginacao::traduzir(CODIGO_VIRT).is_none());
        assert!(paginacao::traduzir(PILHA_VIRT).is_none());
        assert_eq!(paginacao::raiz_atual(), paginacao::raiz_kernel());
        assert_eq!(quadros::livres(), livres);
    }

    #[test_case]
    fn validacao_de_ponteiros() {
        assert!(faixa_do_programa(CODIGO_VIRT, 10));
        assert!(faixa_do_programa(PILHA_VIRT + 4000, 96));
        assert!(!faixa_do_programa(PILHA_VIRT + 4000, 97));          // passa do fim
        assert!(!faixa_do_programa(0xFFFF_9000_0000_0000, 16));     // kernel
        assert!(!faixa_do_programa(u64::MAX - 4, 10));               // estoura
    }
}
