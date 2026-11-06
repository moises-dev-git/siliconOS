//! Modo usuário: rodar um programa no anel 3 e atender as chamadas de
//! sistema dele.
//!
//! Explicação completa em docs/10-modo-usuario.md.
//!
//! O ciclo de vida de um programa:
//!
//!   shell: rodar ola
//!     └─ rodar()                       mapeia código e pilha com o bit USUARIO
//!          └─ entrar_no_anel3()        salva o RSP do kernel, monta um quadro e faz iretq
//!               ═══════ anel 3 ═══════ o programa roda; int 0x80 = chamada de sistema
//!               ◄─ voltar_ao_kernel()  restaura o RSP salvo: entrar_no_anel3 "retorna"
//!     └─ rodar()                       desmapeia, devolve os quadros, informa o resultado
//!
//! O programa termina por um de três caminhos, e todos acabam em `encerrar`:
//! a chamada `sair`, uma exceção da CPU no anel 3, ou o timer (tempo esgotado).

pub mod programas;

use core::arch::naked_asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::gdt;
use crate::idt::QuadroInterrupcao;
use crate::interrupcoes::timer;
use crate::memoria::paginacao::{self, flags};
use crate::memoria::{quadros, TAMANHO_QUADRO};
use crate::{cpu, kprint, kprintln};

/// Onde o código do programa fica (entrada 64 da PML4, metade baixa: nada
/// do kernel mora ali).
pub const CODIGO_VIRT: u64 = 0x0000_2000_0000_0000;
/// A página da pilha do programa; a pilha começa no fim dela.
pub const PILHA_VIRT: u64 = 0x0000_2000_0010_0000;
pub const VETOR_SYSCALL: u8 = 0x80;
/// Um programa que passar disto sem devolver a CPU é encerrado pelo timer.
pub const LIMITE_TIQUES: u64 = 2 * timer::FREQUENCIA_HZ;

// Números das chamadas de sistema (RAX).
pub const SYS_SAIR: u64 = 0;
pub const SYS_ESCREVER: u64 = 1;
pub const SYS_TIQUES: u64 = 2;

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

/// Há um programa rodando agora?
static ATIVO: AtomicBool = AtomicBool::new(false);
static INICIO: AtomicU64 = AtomicU64::new(0);
static RESULTADO: Mutex<Option<Resultado>> = Mutex::new(None);

/// RSP do kernel no momento em que entrou no anel 3 (lido por voltar_ao_kernel).
static mut RSP_KERNEL: u64 = 0;

// ---------------------------------------------------------------------------
// Rodar um programa
// ---------------------------------------------------------------------------

pub fn rodar(programa: &programas::Programa) -> Resultado {
    let codigo = programa.codigo();
    assert!(codigo.len() <= TAMANHO_QUADRO as usize, "programa maior que uma pagina");

    // 1. Uma página de código (só leitura, executável) e uma de pilha
    //    (escrita, sem execução), as duas acessíveis pelo anel 3.
    let q_codigo = nova_pagina_zerada();
    let q_pilha = nova_pagina_zerada();
    unsafe {
        let destino = paginacao::fisico_para_virtual(q_codigo.endereco()) as *mut u8;
        core::ptr::copy_nonoverlapping(codigo.as_ptr(), destino, codigo.len());
    }
    paginacao::mapear(CODIGO_VIRT, q_codigo.endereco(), paginacao::TamanhoPagina::K4,
                      flags::USUARIO).expect("mapear o codigo do programa");
    paginacao::mapear(PILHA_VIRT, q_pilha.endereco(), paginacao::TamanhoPagina::K4,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO).expect("mapear a pilha");

    // 2. Entrar no anel 3. Só volta quando o programa terminar.
    *RESULTADO.lock() = None;
    INICIO.store(timer::tiques(), Ordering::Relaxed);
    ATIVO.store(true, Ordering::Relaxed);
    unsafe { entrar_no_anel3(CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO) };
    ATIVO.store(false, Ordering::Relaxed);
    // Voltamos de dentro de um tratador (interrupções desligadas).
    cpu::ligar_interrupcoes();

    // 3. Limpar: nada do programa fica mapeado.
    for (virt, q) in [(CODIGO_VIRT, q_codigo), (PILHA_VIRT, q_pilha)] {
        paginacao::desmapear(virt).expect("desmapear");
        quadros::liberar(q);
    }
    RESULTADO.lock().take().expect("programa terminou sem resultado")
}

fn nova_pagina_zerada() -> quadros::Quadro {
    let q = quadros::alocar().expect("sem quadros para o programa");
    // Um quadro reaproveitado tem os dados de quem o usou antes: zerar, para
    // o programa não ler restos do kernel.
    unsafe { core::ptr::write_bytes(paginacao::fisico_para_virtual(q.endereco()) as *mut u8, 0, 4096) };
    q
}

/// Termina o programa atual e volta para dentro de `rodar`. Chamada de um
/// tratador (de syscall, de exceção ou do timer), com o anel 3 interrompido.
pub fn encerrar(resultado: Resultado) -> ! {
    *RESULTADO.lock() = Some(resultado);
    unsafe { voltar_ao_kernel() }
}

/// Chamada pelos tratadores de exceção (idt.rs): se a exceção aconteceu no
/// anel 3, a culpa é do programa, não do kernel. Encerra só o programa.
pub fn falha_do_programa(quadro: &QuadroInterrupcao, excecao: &'static str, detalhes: core::fmt::Arguments) {
    if quadro.cs & 3 == 3 {
        kprintln!("[kernel] {} no programa (RIP={:#x}, CS={:#x}): {}", excecao, quadro.rip, quadro.cs, detalhes);
        encerrar(Resultado::Falha { excecao, rip: quadro.rip });
    }
}

/// Chamada pelo tratador do timer: o programa já passou do limite?
pub fn tempo_esgotado(quadro: &QuadroInterrupcao) -> bool {
    quadro.cs & 3 == 3
        && ATIVO.load(Ordering::Relaxed)
        && timer::tiques() - INICIO.load(Ordering::Relaxed) >= LIMITE_TIQUES
}

// ---------------------------------------------------------------------------
// A troca de anel (assembly)
// ---------------------------------------------------------------------------

/// Salva o estado do kernel e "volta" de uma interrupção que nunca
/// aconteceu: o `iretq` desempilha um quadro montado à mão, com CS e SS do
/// anel 3, e a CPU passa a executar `rip` com privilégio de usuário.
///
/// `extern "sysv64"`: convenção de chamada do Linux (argumentos em RDI e
/// RSI), escolhida explicitamente porque o alvo UEFI usa a do Windows.
#[unsafe(naked)]
unsafe extern "sysv64" fn entrar_no_anel3(rip: u64, rsp: u64) -> u64 {
    naked_asm!(
        // Registradores que a convenção manda preservar: voltar_ao_kernel os restaura.
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov [rip + {rsp_kernel}], rsp",
        // O quadro que o iretq espera, de baixo para cima: RIP, CS, RFLAGS, RSP, SS.
        "push {ss}",
        "push rsi",
        "push {rflags}",
        "push {cs}",
        "push rdi",
        // Não deixar valores do kernel nos registradores para o programa ver.
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor edx, edx",
        "xor esi, esi", "xor edi, edi", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
        rsp_kernel = sym RSP_KERNEL,
        ss = const gdt::USER_DATA as u64,
        cs = const gdt::USER_CODE as u64,
        rflags = const 0x202u64, // bit 9 (IF): o programa roda com interrupções ligadas
    )
}

/// O caminho de volta: descarta a pilha do tratador atual, restaura o RSP
/// salvo por entrar_no_anel3 e retorna como se ela tivesse terminado.
#[unsafe(naked)]
unsafe extern "sysv64" fn voltar_ao_kernel() -> ! {
    naked_asm!(
        "mov rsp, [rip + {rsp_kernel}]",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "xor eax, eax",
        "ret",
        rsp_kernel = sym RSP_KERNEL,
    )
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
        SYS_SAIR => encerrar(Resultado::Saiu(r.rdi)),
        SYS_ESCREVER => escrever(r.rdi, r.rsi),
        SYS_TIQUES => timer::tiques(),
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
        let livres = quadros::livres();
        rodar_por_nome("ola");
        assert!(paginacao::traduzir(CODIGO_VIRT).is_none());
        assert!(paginacao::traduzir(PILHA_VIRT).is_none());
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
