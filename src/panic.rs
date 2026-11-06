//! Tratador de panic do kernel.
//!
//! Em Rust, `panic!` (ou um `unwrap()` que falha, ou um índice fora do
//! vetor) chama a função marcada com `#[panic_handler]`. Num programa comum
//! quem fornece essa função é a biblioteca padrão (std); num kernel `no_std`
//! somos nós. Ela NUNCA retorna (tipo `-> !`): não há para onde voltar.
//!
//! Regras que seguimos aqui:
//! 1. Desligar interrupções — nada deve interromper o relato do erro.
//! 2. Escrever primeiro na serial — é o canal mais confiável.
//! 3. Não travar esperando um lock: se o panic aconteceu enquanto o
//!    console estava em uso, forçamos a liberação (o código que o segurava
//!    nunca mais vai rodar mesmo).
//! 4. Parar a CPU para sempre com `cli; hlt`.

use core::fmt::Write;
use core::panic::PanicInfo;

use crate::serial::Serial;
use crate::CONSOLE;

/// Evita recursão infinita se o próprio tratador causar outro panic.
static EM_PANIC: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    unsafe { core::arch::asm!("cli", options(nomem, nostack)) };

    if EM_PANIC.swap(true, core::sync::atomic::Ordering::SeqCst) {
        // Panic dentro do panic: só uma mensagem curta e parar.
        let _ = Serial.write_str("\n!!! PANIC DUPLO - parando\n");
        parar();
    }

    // 1) Serial: sempre funciona (não depende de vídeo nem de memória).
    let _ = writeln!(Serial, "\n!!! KERNEL PANIC !!!");
    escrever_detalhes(&mut Serial, info);

    // 2) Tela, se o console já foi inicializado.
    if CONSOLE.is_locked() {
        unsafe { CONSOLE.force_unlock() };
    }
    if let Some(con) = CONSOLE.lock().as_mut() {
        con.cor = 0x00FF_FFFF;
        con.fundo = 0x00B0_0000; // vermelho
        let _ = writeln!(con, "\n!!! KERNEL PANIC !!!");
        escrever_detalhes(con, info);
        let _ = writeln!(con, "Sistema parado.");
    }

    // `cargo test`: um panic é um teste que falhou. Encerrar o QEMU com o
    // código de falha, para o `cargo test` saber (docs/09-shell-e-testes.md).
    #[cfg(test)]
    {
        let _ = writeln!(Serial, "[FALHOU]");
        crate::qemu::sair(crate::qemu::Saida::Falha);
    }
    #[cfg(not(test))]
    parar();
}

fn escrever_detalhes(w: &mut impl Write, info: &PanicInfo) {
    if let Some(loc) = info.location() {
        let _ = writeln!(w, "Local: {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    let _ = writeln!(w, "Mensagem: {}", info.message());
}

fn parar() -> ! {
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
