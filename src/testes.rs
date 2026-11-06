//! Framework de testes do kernel: `cargo test` compila uma versão do kernel
//! que, depois de inicializar tudo, roda cada função marcada com
//! `#[test_case]` e encerra o QEMU com sucesso ou falha.
//!
//! Explicação em docs/09-shell-e-testes.md.

use crate::qemu::{self, Saida};
use crate::{kprint, kprintln};

/// Algo que pode ser rodado como teste: qualquer função sem parâmetros.
/// O nome é tirado do tipo da função (`siliconos::memoria::heap::testes::box_simples`).
pub trait Testavel {
    fn rodar(&self);
}

impl<T: Fn()> Testavel for T {
    fn rodar(&self) {
        kprint!("{} ... ", core::any::type_name::<T>());
        self();
        kprintln!("[ok]");
    }
}

/// Chamada pelo código que o compilador gera em modo de teste, com a lista
/// de todas as funções `#[test_case]` do crate.
pub fn executar(testes: &[&dyn Testavel]) {
    kprintln!("Rodando {} testes", testes.len());
    for t in testes {
        t.rodar();
    }
    kprintln!("Todos os {} testes passaram.", testes.len());
    qemu::sair(Saida::Sucesso);
}

/// `cargo test --features teste-que-falha`: mostra o que acontece quando um
/// teste falha (o QEMU sai com o código de falha e o `cargo test` também).
#[cfg(feature = "teste-que-falha")]
#[test_case]
fn este_teste_falha() {
    assert_eq!(1 + 1, 3, "de proposito");
}
