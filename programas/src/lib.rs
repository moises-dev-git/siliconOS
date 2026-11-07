//! `sos`: a biblioteca mínima dos programas de usuário do SiliconOS.
//!
//! Explicação completa em docs/13-disco-em-ram-e-elf.md.
//!
//! Um programa em Rust sem sistema operacional (`no_std`) não tem `println!`
//! nem `std::process::exit`: quem fornece isso, num sistema de verdade, é a
//! biblioteca do sistema (a libc no Linux). Aqui ela é este arquivo: cada
//! função é uma chamada de sistema. Até o doc 13, pela instrução `int 0x80`
//! (doc 10); desde o doc 14, pela instrução `syscall`, feita para isso.
//! Desde o doc 16, também é quem lê os argumentos (argv) que o kernel deixa
//! na pilha (docs/16-argumentos.md), e desde o doc 17 abre e lê arquivos
//! (docs/17-vfs-e-arquivos.md).

#![no_std]

use core::arch::asm;
use core::fmt;
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

// Números das chamadas de sistema (os mesmos de src/usuario/mod.rs do kernel).
const SYS_SAIR: u64 = 0;
const SYS_ESCREVER: u64 = 1;
const SYS_TIQUES: u64 = 2;
const SYS_PID: u64 = 3;
const SYS_DORMIR: u64 = 4;
const SYS_ABRIR: u64 = 5;
const SYS_LER: u64 = 6;
const SYS_FECHAR: u64 = 7;

/// Uma chamada de sistema com até dois argumentos, pela instrução
/// `syscall` (doc 14). Número em RAX, argumentos em RDI e RSI, resultado em
/// RAX. A própria CPU usa RCX (endereço de volta) e R11 (RFLAGS): o
/// compilador precisa saber que os dois mudam (`lateout(...) _`).
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    chamada3(numero, a, b, 0)
}

/// O mesmo, com um terceiro argumento em RDX (doc 17: `ler` precisa de três).
#[inline(always)]
fn chamada3(numero: u64, a: u64, b: u64, c: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b, in("rdx") c,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}

/// Termina o programa com o código dado. Não volta.
pub fn sair(codigo: u64) -> ! {
    chamada(SYS_SAIR, codigo, 0);
    unreachable!("o kernel voltou de sair()")
}

/// Escreve os bytes no console. Devolve quantos foram escritos, ou um erro
/// negativo (-14 se o kernel recusar o ponteiro).
pub fn escrever(bytes: &[u8]) -> i64 {
    chamada(SYS_ESCREVER, bytes.as_ptr() as u64, bytes.len() as u64) as i64
}

/// Tiques do timer desde o boot (100 por segundo).
pub fn tiques() -> u64 {
    chamada(SYS_TIQUES, 0, 0)
}

/// O número deste processo.
pub fn pid() -> u64 {
    chamada(SYS_PID, 0, 0)
}

/// Dorme por `ms` milissegundos sem gastar CPU (doc 15): o kernel tira o
/// programa do rodízio e o devolve quando o prazo vence. A precisão é de um
/// tique (10 ms).
pub fn dormir(ms: u64) {
    chamada(SYS_DORMIR, ms, 0);
}

// ---------------------------------------------------------------------------
// Arquivos (doc 17)
// ---------------------------------------------------------------------------

/// As syscalls de arquivo sem nenhuma ajuda: números crus, ponteiros crus.
/// Os programas normais usam `Arquivo`; o `conferir` usa estas para
/// mandar ao kernel o que um programa com defeito (ou malicioso) mandaria.
pub mod cru {
    use super::*;

    /// Abre o caminho [ponteiro, ponteiro + tamanho). Descritor ou erro negativo.
    pub fn abrir(ponteiro: u64, tamanho: u64) -> i64 {
        chamada(SYS_ABRIR, ponteiro, tamanho) as i64
    }

    /// Lê até `tamanho` bytes para `destino`. Quantos leu (0 = fim) ou erro.
    pub fn ler(descritor: u64, destino: u64, tamanho: u64) -> i64 {
        chamada3(SYS_LER, descritor, destino, tamanho) as i64
    }

    /// Fecha o descritor. 0 ou erro.
    pub fn fechar(descritor: u64) -> i64 {
        chamada(SYS_FECHAR, descritor, 0) as i64
    }
}

/// O texto de um código de erro do kernel (os mesmos números do Linux).
pub fn nome_do_erro(codigo: i64) -> &'static str {
    match codigo {
        -2 => "nao existe",
        -9 => "descritor nao esta aberto",
        -14 => "endereco invalido",
        -24 => "arquivos abertos demais",
        -36 => "caminho grande demais",
        -38 => "chamada desconhecida",
        _ => "erro desconhecido",
    }
}

/// Um arquivo aberto. Fecha sozinho quando sai de escopo (`Drop`), como um
/// `File` do Rust ou um try-with-resources do Java.
pub struct Arquivo {
    descritor: u64,
}

impl Arquivo {
    /// Abre o arquivo `caminho` ("/leiame.txt", "/sis/tempo"). O erro é o
    /// código negativo do kernel (veja `nome_do_erro`).
    pub fn abrir(caminho: &str) -> Result<Arquivo, i64> {
        let r = cru::abrir(caminho.as_ptr() as u64, caminho.len() as u64);
        if r < 0 { Err(r) } else { Ok(Arquivo { descritor: r as u64 }) }
    }

    /// Lê os próximos bytes para `destino`. `Ok(0)` = fim do arquivo.
    pub fn ler(&mut self, destino: &mut [u8]) -> Result<usize, i64> {
        let r = cru::ler(self.descritor, destino.as_mut_ptr() as u64, destino.len() as u64);
        if r < 0 { Err(r) } else { Ok(r as usize) }
    }

    /// O número que o kernel deu a este arquivo.
    pub fn descritor(&self) -> u64 {
        self.descritor
    }
}

impl Drop for Arquivo {
    fn drop(&mut self) {
        cru::fechar(self.descritor);
    }
}

/// Destino de `print!`: cada pedaço de texto formatado vira um `escrever`.
pub struct Console;

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        escrever(s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;
        let _ = write!($crate::Console, $($arg)*);
    }};
}

#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {{ $crate::print!($($arg)*); $crate::print!("\n"); }};
}

// ---------------------------------------------------------------------------
// Início do programa e argumentos (doc 16)
// ---------------------------------------------------------------------------

/// argc e argv, guardados por `__iniciar` antes de `main`. Atômicos só para
/// não precisar de `static mut`: o programa tem uma thread só.
static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());

/// Define o ponto de entrada `_start` (o que o ELF diz para executar
/// primeiro), que chama `main` e passa o valor devolvido para `sair`.
///
/// Desde o doc 16, `_start` é uma função *naked* (só assembly): na entrada,
/// RSP aponta para `argc`, que o kernel escreveu na pilha (formato System
/// V), e uma função Rust comum não tem como saber onde estava o RSP antes
/// do prólogo dela. O assembly passa esse RSP como 1º argumento e `main`
/// como 2º para `__iniciar`, que guarda argc/argv e chama `main`.
#[macro_export]
macro_rules! principal {
    ($main:path) => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn _start() -> ! {
            core::arch::naked_asm!(
                "mov rdi, rsp",           // 1º argumento: onde está argc
                "lea rsi, [rip + {main}]", // 2º argumento: a função main
                "and rsp, -16",           // alinhar em 16 (o kernel já alinha; garantia)
                "call {iniciar}",         // o call empilha 8 bytes: o Rust espera isso
                "ud2",                    // __iniciar não volta
                main = sym $main,
                iniciar = sym $crate::__iniciar,
            )
        }
    };
}

/// Chamada pelo `_start` de `principal!`. Não é para os programas usarem.
#[doc(hidden)]
pub unsafe extern "C" fn __iniciar(pilha: *const u64, main: fn() -> u64) -> ! {
    // pilha[0] = argc; pilha[1..] = argv[0], argv[1], ..., 0.
    unsafe {
        ARGC.store(*pilha as usize, Ordering::Relaxed);
        ARGV.store(pilha.add(1) as *mut *const u8, Ordering::Relaxed);
    }
    sair(main())
}

/// Quantos argumentos o programa recebeu, contando o nome (argv[0]).
pub fn quantos_argumentos() -> usize {
    ARGC.load(Ordering::Relaxed)
}

/// O argumento `i` (0 = nome do programa), ou `None` se não existe.
/// Como o `sys.argv[i]` do Python, mas sem estourar: devolve `None`.
pub fn argumento(i: usize) -> Option<&'static str> {
    if i >= quantos_argumentos() {
        return None;
    }
    // O kernel garante: argv[i] aponta para um texto terminado em 0, que
    // fica na pilha até o programa acabar (por isso 'static).
    unsafe {
        let inicio = *ARGV.load(Ordering::Relaxed).add(i);
        let mut tamanho = 0;
        while *inicio.add(tamanho) != 0 {
            tamanho += 1;
        }
        let bytes = core::slice::from_raw_parts(inicio, tamanho);
        Some(core::str::from_utf8(bytes).unwrap_or("?"))
    }
}

/// Todos os argumentos, em ordem, como o `std::env::args()` do Rust.
pub fn argumentos() -> impl Iterator<Item = &'static str> {
    (0..quantos_argumentos()).filter_map(argumento)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic no programa: {}", info);
    sair(101)
}
