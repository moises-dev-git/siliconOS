//! Espaço de endereços: uma PML4 para cada programa.
//!
//! Explicação completa em docs/11-espacos-de-enderecos.md.
//!
//! Até o doc 10, o programa era mapeado nas tabelas do kernel: só cabia um
//! por vez, e o que um programa mapeasse ficava visível para o próximo.
//! Agora cada programa ganha a sua PML4:
//!
//!   PML4 do kernel             PML4 do programa A        PML4 do programa B
//!   [0]   identidade ───────┬─ [0]   (a mesma) ──────┬── [0]   (a mesma)
//!   [64]  (vazia)           │  [64]  código/pilha A  │   [64]  código/pilha B
//!   [256] metade alta ──────┼─ [256] (a mesma) ──────┼── [256] (a mesma)
//!   [288] heap ─────────────┼─ [288] (a mesma) ──────┼── [288] (a mesma)
//!   [320] MMIO ─────────────┘  [320] (a mesma) ──────┘   [320] (a mesma)
//!
//! As entradas do kernel são COPIADAS (8 bytes cada) na criação: as três
//! PML4 apontam para as MESMAS tabelas de nível 2 em diante, então o kernel
//! aparece igual em todas. Só as entradas que o kernel não usa pertencem
//! ao programa; elas, as tabelas abaixo delas e as páginas no fim de tudo
//! são liberadas quando o espaço é destruído (`Drop`).

use super::paginacao::{self, tabela_em, nova_tabela, ErroMapeamento, TamanhoPagina, Traducao};
use super::quadros::{self, Quadro};

pub struct EspacoEnderecos {
    /// Endereço físico da PML4 deste espaço.
    raiz: u64,
}

impl EspacoEnderecos {
    /// Cria um espaço novo: uma PML4 com as entradas do kernel copiadas e o
    /// resto vazio. `None` se não houver um quadro livre.
    pub fn novo() -> Option<EspacoEnderecos> {
        let raiz = nova_tabela()?;
        let kernel = tabela_em(paginacao::raiz_kernel());
        let nova = tabela_em(raiz);
        for i in 0..512 {
            if kernel.entradas[i].presente() {
                nova.entradas[i] = kernel.entradas[i];
            }
        }
        Some(EspacoEnderecos { raiz })
    }

    pub fn raiz(&self) -> u64 {
        self.raiz
    }

    /// Mapeia uma página de 4 KiB do programa. O espaço passa a ser DONO do
    /// quadro: ele será liberado junto com o espaço (ou agora, se der erro).
    /// Só aceita endereços em entradas da PML4 que o kernel não usa: um
    /// mapeamento ali dentro mexeria em tabelas compartilhadas com todos.
    pub fn mapear(&mut self, virt: u64, quadro: Quadro, bits: u64) -> Result<(), ErroMapeamento> {
        let resultado = if !Self::do_programa(virt) {
            Err(ErroMapeamento::AreaDoKernel)
        } else {
            paginacao::mapear_em(self.raiz, virt, quadro.endereco(), TamanhoPagina::K4, bits)
        };
        if resultado.is_err() {
            quadros::liberar(quadro);
        }
        resultado
    }

    pub fn traduzir(&self, virt: u64) -> Option<Traducao> {
        paginacao::traduzir_em(self.raiz, virt)
    }

    /// Passa a usar este espaço (CR3 = raiz). O kernel continua visível,
    /// porque as entradas dele estão aqui também.
    pub fn ativar(&self) {
        paginacao::ativar(self.raiz);
    }

    /// O endereço cai numa entrada da PML4 livre para programas? (metade
    /// baixa e não usada pelo kernel)
    pub fn do_programa(virt: u64) -> bool {
        let i = paginacao::indices(virt)[0];
        i < 256 && Self::entrada_livre(i)
    }

    /// A entrada `i` da PML4 do kernel está vazia?
    pub fn entrada_livre(i: usize) -> bool {
        !tabela_em(paginacao::raiz_kernel()).entradas[i].presente()
    }
}

impl Drop for EspacoEnderecos {
    /// Devolve ao alocador tudo o que é do programa: páginas, tabelas e a
    /// própria PML4. As tabelas do kernel (compartilhadas) não são tocadas.
    fn drop(&mut self) {
        // Liberar as tabelas que a CPU está usando seria serrar o galho em
        // que se está sentado: o próximo acesso à memória leria lixo.
        assert!(paginacao::raiz_atual() != self.raiz, "destruindo o espaco de enderecos ativo");
        let kernel = tabela_em(paginacao::raiz_kernel());
        let pml4 = tabela_em(self.raiz);
        for i in 0..256 {
            let e = pml4.entradas[i];
            if e.presente() && !kernel.entradas[i].presente() {
                liberar_arvore(e.endereco(), 1);
            }
        }
        quadros::liberar(Quadro(self.raiz));
    }
}

/// Libera a tabela do `nivel` dado (1 = PDPT, 2 = PD, 3 = PT), tudo o que
/// está abaixo dela e as páginas para as quais a PT aponta.
fn liberar_arvore(tabela: u64, nivel: usize) {
    for e in tabela_em(tabela).entradas.iter().filter(|e| e.presente()) {
        // Programas só recebem páginas de 4 KiB (veja `mapear`).
        assert!(!e.grande(), "pagina grande num espaco de programa");
        if nivel == 3 {
            quadros::liberar(Quadro(e.endereco())); // a página do programa
        } else {
            liberar_arvore(e.endereco(), nivel + 1);
        }
    }
    quadros::liberar(Quadro(tabela));
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::memoria::heap;
    use crate::memoria::paginacao::flags;

    const V: u64 = 0x0000_2000_0000_0000;

    fn pagina_com(valor: u64) -> Quadro {
        let q = quadros::alocar().unwrap();
        unsafe { (paginacao::fisico_para_virtual(q.endereco()) as *mut u64).write_volatile(valor) };
        q
    }

    #[test_case]
    fn mesmo_endereco_paginas_diferentes() {
        let (qa, qb) = (pagina_com(0xAAAA), pagina_com(0xBBBB));
        let (mut a, mut b) = (EspacoEnderecos::novo().unwrap(), EspacoEnderecos::novo().unwrap());
        a.mapear(V, qa, flags::USUARIO).unwrap();
        b.mapear(V, qb, flags::USUARIO).unwrap();
        assert_eq!(a.traduzir(V).unwrap().fisico, qa.endereco());
        assert_eq!(b.traduzir(V).unwrap().fisico, qb.endereco());
        assert!(paginacao::traduzir_em(paginacao::raiz_kernel(), V).is_none());
        // Lendo de verdade, com cada espaço ativo:
        let ler = || unsafe { (V as *const u64).read_volatile() };
        a.ativar();
        let va = ler();
        b.ativar();
        let vb = ler();
        paginacao::ativar(paginacao::raiz_kernel());
        assert_eq!((va, vb), (0xAAAA, 0xBBBB));
    }

    #[test_case]
    fn kernel_aparece_igual_em_todos() {
        let a = EspacoEnderecos::novo().unwrap();
        let caixa = alloc::boxed::Box::new(7u64);
        let endereco = &*caixa as *const u64 as u64;
        assert!(heap::contem(endereco));
        assert_eq!(a.traduzir(endereco).map(|t| t.fisico), paginacao::traduzir(endereco).map(|t| t.fisico));
    }

    #[test_case]
    fn destruir_devolve_todos_os_quadros() {
        let livres = quadros::livres();
        let mut a = EspacoEnderecos::novo().unwrap();
        a.mapear(V, pagina_com(1), flags::USUARIO).unwrap();
        a.mapear(V + 0x20_0000, pagina_com(2), flags::USUARIO).unwrap();
        // PML4 + PDPT + PD + 2 PTs (cada PT cobre 2 MiB; as páginas estão a 2 MiB uma da outra) + 2 páginas
        assert_eq!(livres - quadros::livres(), 7);
        drop(a);
        assert_eq!(quadros::livres(), livres);
    }

    #[test_case]
    fn area_do_kernel_recusada() {
        let livres = quadros::livres();
        let mut a = EspacoEnderecos::novo().unwrap();
        let heap = 0xFFFF_9000_0000_0000;
        assert_eq!(a.mapear(heap, pagina_com(0), flags::USUARIO), Err(ErroMapeamento::AreaDoKernel));
        assert_eq!(a.mapear(0x1000, pagina_com(0), flags::USUARIO), Err(ErroMapeamento::AreaDoKernel));
        drop(a);
        assert_eq!(quadros::livres(), livres);
    }
}
