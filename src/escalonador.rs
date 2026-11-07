//! Escalonador: vários processos dividindo a CPU, trocados pelo timer.
//!
//! Explicação completa em docs/12-escalonador.md.
//!
//! Cada processo tem a sua PILHA DO KERNEL. Trocar de processo é trocar de
//! pilha do kernel (`trocar`), sempre dentro do kernel:
//!
//!   processo A (anel 3) ──timer──► tratador na pilha de A ──► ceder()
//!                                                               │ trocar(&A.rsp, B.rsp)
//!   processo B (anel 3) ◄──iretq── tratador na pilha de B ◄─────┘ (B tinha parado ali)
//!
//! O processo 0 é o próprio kernel (o shell), rodando na pilha do boot.
//! Os outros são programas de usuário, cada um no seu espaço de endereços.
//!
//! Regra que torna isto seguro: todo lock do kernel é pego com as
//! interrupções desligadas (`cpu::sem_interrupcoes`). Assim o timer nunca
//! troca de processo no meio de uma seção crítica.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::arch::naked_asm;
use spin::Mutex;

use crate::cpu;
use crate::gdt;
use crate::idt::QuadroInterrupcao;
use crate::kprintln;
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao;
use crate::usuario::{self, Resultado};

pub type Pid = u32;

/// Tamanho da pilha do kernel de cada processo.
const TAM_PILHA_KERNEL: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// Pode rodar; espera a vez.
    Pronto,
    /// É o processo que está na CPU agora.
    Rodando,
    /// Terminou; espera alguém recolher o resultado e liberar a memória.
    Terminado(Resultado),
}

pub struct Processo {
    pub pid: Pid,
    pub nome: &'static str,
    pub estado: Estado,
    /// RSP salvo por `trocar` quando o processo sai da CPU.
    rsp: u64,
    /// Pilha do kernel (None no processo 0, que usa a pilha do boot).
    pilha: Option<Box<[u64]>>,
    /// Espaço de endereços (None no processo 0: usa as tabelas do kernel).
    espaco: Option<EspacoEnderecos>,
    /// Tiques do timer que pegaram este processo no anel 3.
    pub tiques_cpu: u64,
    /// Ninguém espera por ele: o resultado é impresso ao terminar.
    pub segundo_plano: bool,
}

impl Processo {
    /// Endereço do topo da pilha do kernel (onde a CPU começa a empilhar).
    fn topo_pilha(&self) -> u64 {
        self.pilha.as_ref().map_or(0, |p| p.as_ptr() as u64 + (p.len() * 8) as u64)
    }
}

struct Tabela {
    /// Box: o endereço de cada `Processo` não muda quando o Vec cresce
    /// (`trocar` guarda um ponteiro para o campo `rsp`).
    processos: Vec<Box<Processo>>,
    /// Índice (em `processos`) de quem está rodando.
    atual: usize,
    proximo_pid: Pid,
}

static TABELA: Mutex<Tabela> = Mutex::new(Tabela { processos: Vec::new(), atual: 0, proximo_pid: 1 });

/// Registra o código que está rodando agora (o kernel) como processo 0.
pub fn init() {
    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        assert!(t.processos.is_empty(), "escalonador::init chamado duas vezes");
        t.processos.push(Box::new(Processo {
            pid: 0, nome: "kernel", estado: Estado::Rodando, rsp: 0,
            pilha: None, espaco: None, tiques_cpu: 0, segundo_plano: false,
        }));
    });
}

// ---------------------------------------------------------------------------
// Criar
// ---------------------------------------------------------------------------

/// Cria um processo pronto para rodar `rip` no anel 3, com a pilha em
/// `rsp`, dentro de `espaco` (que já tem o código e a pilha mapeados).
pub fn criar(nome: &'static str, espaco: EspacoEnderecos, rip: u64, rsp: u64, segundo_plano: bool) -> Pid {
    // A pilha do kernel do processo, preparada como se ele já tivesse
    // rodado e sido trocado: o primeiro `trocar` para ele desempilha os 6
    // registradores, e o `ret` cai em `primeira_entrada`, que faz o iretq.
    //
    //   topo ─► [ SS          ]  ┐
    //           [ RSP usuário ]  │ quadro que o iretq desempilha
    //           [ RFLAGS      ]  │
    //           [ CS          ]  │
    //           [ RIP         ]  ┘
    //           [ primeira_entrada ]   endereço de retorno de `trocar`
    //           [ rbx rbp r12 r13 r14 r15 = 0 ]   ◄─ rsp salvo
    let mut pilha = alloc::vec![0u64; TAM_PILHA_KERNEL / 8].into_boxed_slice();
    let n = pilha.len();
    pilha[n - 1] = gdt::USER_DATA as u64;
    pilha[n - 2] = rsp;
    pilha[n - 3] = 0x202; // IF ligado
    pilha[n - 4] = gdt::USER_CODE as u64;
    pilha[n - 5] = rip;
    pilha[n - 6] = primeira_entrada as *const () as u64;
    // pilha[n - 12 .. n - 6]: os 6 registradores, já zerados.
    let rsp_kernel = &pilha[n - 12] as *const u64 as u64;

    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let pid = t.proximo_pid;
        t.proximo_pid += 1;
        t.processos.push(Box::new(Processo {
            pid, nome, estado: Estado::Pronto, rsp: rsp_kernel,
            pilha: Some(pilha), espaco: Some(espaco), tiques_cpu: 0, segundo_plano,
        }));
        pid
    })
}

/// Para onde o primeiro `trocar` de um processo novo "retorna": zera os
/// registradores e desempilha o quadro montado em `criar`.
#[unsafe(naked)]
unsafe extern "sysv64" fn primeira_entrada() -> ! {
    naked_asm!(
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor edx, edx",
        "xor esi, esi", "xor edi, edi", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
    )
}

// ---------------------------------------------------------------------------
// Trocar
// ---------------------------------------------------------------------------

/// A troca de contexto. Salva os registradores que a convenção sysv64
/// manda preservar, guarda o RSP em `*salvar`, passa a usar a pilha
/// `novo_rsp` e desempilha os registradores de lá. O `ret` volta para onde
/// o OUTRO processo tinha chamado `trocar`.
#[unsafe(naked)]
unsafe extern "sysv64" fn trocar(salvar: *mut u64, novo_rsp: u64) {
    naked_asm!(
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "ret",
    )
}

/// Passa a CPU para o próximo processo pronto (rodízio). Volta quando
/// chegar de novo a vez deste. Se não houver outro pronto, volta na hora.
pub fn ceder() {
    cpu::sem_interrupcoes(|| {
        if let Some((salvar, novo_rsp)) = escolher_proximo() {
            unsafe { trocar(salvar, novo_rsp) };
        }
    });
}

/// Escolhe o próximo, prepara a CPU para ele (CR3, TSS.RSP0) e devolve o
/// que `trocar` precisa. O lock é solto ANTES da troca: o próximo processo
/// vai precisar dele.
fn escolher_proximo() -> Option<(*mut u64, u64)> {
    let mut t = TABELA.lock();
    let n = t.processos.len();
    let atual = t.atual;
    let proximo = (1..n).map(|d| (atual + d) % n).find(|&i| t.processos[i].estado == Estado::Pronto)?;

    if t.processos[atual].estado == Estado::Rodando {
        t.processos[atual].estado = Estado::Pronto;
    }
    t.processos[proximo].estado = Estado::Rodando;
    t.atual = proximo;

    let p = &t.processos[proximo];
    if p.pilha.is_some() {
        gdt::definir_rsp0(p.topo_pilha());
    }
    paginacao::ativar(p.espaco.as_ref().map_or(paginacao::raiz_kernel(), |e| e.raiz()));

    let novo_rsp = p.rsp;
    let salvar = &mut t.processos[atual].rsp as *mut u64;
    Some((salvar, novo_rsp))
}

/// Chamada pelo tratador do timer (depois do EOI), a cada tique.
pub fn tique(quadro: &QuadroInterrupcao) {
    if quadro.cs & 3 == 3 {
        // O tique pegou um programa no anel 3: conta o tempo de CPU dele.
        let excedeu = cpu::sem_interrupcoes(|| {
            let mut t = TABELA.lock();
            let a = t.atual;
            let p = &mut t.processos[a];
            p.tiques_cpu += 1;
            p.tiques_cpu >= usuario::LIMITE_TIQUES
        });
        if excedeu {
            encerrar(Resultado::TempoEsgotado);
        }
    }
    ceder();
}

// ---------------------------------------------------------------------------
// Terminar e recolher
// ---------------------------------------------------------------------------

/// Termina o processo atual. Chamada de um tratador (syscall `sair`,
/// exceção, timer). Nunca volta: a pilha deste processo é abandonada e
/// liberada depois, por quem recolher o resultado.
pub fn encerrar(resultado: Resultado) -> ! {
    cpu::desligar_interrupcoes();
    let (pid, nome, segundo_plano) = {
        let mut t = TABELA.lock();
        let a = t.atual;
        assert!(a != 0, "o kernel (processo 0) nao pode ser encerrado");
        let p = &mut t.processos[a];
        p.estado = Estado::Terminado(resultado);
        (p.pid, p.nome, p.segundo_plano)
    };
    if segundo_plano {
        kprintln!("\n[{}] '{}' {}", pid, nome, usuario::descrever(resultado));
    }
    ceder();
    unreachable!("processo encerrado voltou a rodar");
}

/// Retira da tabela um processo terminado e devolve o resultado. A pilha e
/// o espaço de endereços são liberados aqui (pelo `Drop` de cada um).
fn retirar(t: &mut Tabela, i: usize) -> (Box<Processo>, Resultado) {
    let Estado::Terminado(r) = t.processos[i].estado else { unreachable!() };
    if i < t.atual {
        t.atual -= 1; // os índices depois de `i` andam uma posição
    }
    (t.processos.remove(i), r)
}

/// O resultado do processo `pid`, se ele já terminou (e libera o processo).
pub fn recolher(pid: Pid) -> Option<Resultado> {
    let (processo, r) = cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let i = t.processos.iter().position(|p| p.pid == pid && matches!(p.estado, Estado::Terminado(_)))?;
        Some(retirar(&mut t, i))
    })?;
    // Liberar fora do lock: o Drop do espaço usa o alocador de quadros.
    drop(processo);
    Some(r)
}

/// Libera os processos de segundo plano que já terminaram.
pub fn recolher_segundo_plano() {
    loop {
        let retirado = cpu::sem_interrupcoes(|| {
            let mut t = TABELA.lock();
            let i = t.processos.iter().position(|p| p.segundo_plano && matches!(p.estado, Estado::Terminado(_)))?;
            Some(retirar(&mut t, i))
        });
        match retirado {
            Some((processo, _)) => drop(processo),
            None => return,
        }
    }
}

/// Espera o processo `pid` terminar e devolve o resultado. Enquanto isso,
/// cede a CPU para os outros processos.
pub fn esperar(pid: Pid) -> Resultado {
    loop {
        if let Some(r) = recolher(pid) {
            return r;
        }
        ceder();
    }
}

// ---------------------------------------------------------------------------
// Consultas
// ---------------------------------------------------------------------------

/// O pid do processo que está rodando.
pub fn pid_atual() -> Pid {
    cpu::sem_interrupcoes(|| {
        let t = TABELA.lock();
        t.processos.get(t.atual).map_or(0, |p| p.pid)
    })
}

/// Há algum processo, além do atual, pronto para rodar?
pub fn ha_outros_prontos() -> bool {
    cpu::sem_interrupcoes(|| {
        let t = TABELA.lock();
        t.processos.iter().enumerate().any(|(i, p)| i != t.atual && p.estado == Estado::Pronto)
    })
}

/// (pid, nome, estado, tiques de CPU) de cada processo, para o comando `ps`.
pub fn listar() -> Vec<(Pid, &'static str, Estado, u64)> {
    cpu::sem_interrupcoes(|| {
        TABELA.lock().processos.iter().map(|p| (p.pid, p.nome, p.estado, p.tiques_cpu)).collect()
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::interrupcoes::timer;
    use crate::memoria::{heap, quadros};
    use crate::usuario::programas;

    fn iniciar(nome: &str) -> Pid {
        usuario::iniciar(programas::procurar(nome).unwrap(), false)
    }

    #[test_case]
    fn o_kernel_e_o_processo_zero() {
        assert_eq!(pid_atual(), 0);
        let lista = listar();
        assert_eq!((lista[0].0, lista[0].1, lista[0].2), (0, "kernel", Estado::Rodando));
    }

    #[test_case]
    fn dois_contadores_ao_mesmo_tempo() {
        // Cada contador leva 5 x 50 = 250 tiques. Um depois do outro: 500.
        // Ao mesmo tempo, o total fica perto de 250.
        let inicio = timer::tiques();
        let (a, b) = (iniciar("contador"), iniciar("contador"));
        assert_eq!(esperar(a), Resultado::Saiu(0));
        assert_eq!(esperar(b), Resultado::Saiu(0));
        let total = timer::tiques() - inicio;
        crate::kprint!("({} tiques) ", total);
        assert!(total < 300, "levou {} tiques", total);
    }

    #[test_case]
    fn dois_lacos_dividem_a_cpu() {
        // Cada laço só é encerrado depois de 200 tiques DE CPU. Dividindo a
        // CPU, os dois juntos precisam de uns 400 tiques de relógio.
        let inicio = timer::tiques();
        let (a, b) = (iniciar("laco"), iniciar("laco"));
        assert_eq!(esperar(a), Resultado::TempoEsgotado);
        assert_eq!(esperar(b), Resultado::TempoEsgotado);
        let total = timer::tiques() - inicio;
        crate::kprint!("({} tiques) ", total);
        assert!(total >= 2 * usuario::LIMITE_TIQUES - 10, "levou so {} tiques", total);
    }

    #[test_case]
    fn processo_terminado_devolve_pilha_e_quadros() {
        let (livres, heap_antes) = (quadros::livres(), heap::estatisticas().em_uso);
        let pid = iniciar("ola");
        assert_eq!(esperar(pid), Resultado::Saiu(42));
        assert_eq!(quadros::livres(), livres);
        assert_eq!(heap::estatisticas().em_uso, heap_antes);
        assert!(listar().iter().all(|p| p.0 != pid));
    }
}
