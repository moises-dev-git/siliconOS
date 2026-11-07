//! Heap do kernel: a memória por trás de `Box`, `Vec` e `String`.
//!
//! Explicação completa em docs/07-heap.md.
//!
//! - Faixa virtual própria a partir de `HEAP_INICIO`, mapeada com quadros do
//!   alocador de quadros (doc 05) e páginas de 4 KiB (doc 06).
//! - Começa com `TAMANHO_INICIAL` mapeado e cresce sob demanda, de
//!   `CRESCIMENTO_MINIMO` em `CRESCIMENTO_MINIMO`, até `TAMANHO_MAXIMO`.
//! - Alocador: lista encadeada de blocos livres, ordenada por endereço,
//!   juntando vizinhos ao liberar. Os nós da lista moram DENTRO dos próprios
//!   blocos livres, então a lista não ocupa memória extra.
//! - Com a feature `teste-bump`, o alocador global passa a ser o de
//!   `bump.rs`, para demonstrar por que ele não serve sozinho.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use spin::Mutex;

use super::paginacao::{self, flags, ErroMapeamento};
use super::TAMANHO_QUADRO;

/// Início da faixa virtual do heap. Fica na metade alta, longe do
/// mapeamento direto da RAM (que começa em 0xFFFF_8000_0000_0000): entrada
/// 288 da PML4, enquanto o mapeamento direto usa as entradas 256 e 257.
pub const HEAP_INICIO: u64 = 0xFFFF_9000_0000_0000;
/// Quanto é mapeado no `init`.
pub const TAMANHO_INICIAL: u64 = 1024 * 1024; // 1 MiB
/// Cada vez que falta espaço, o heap cresce pelo menos isto.
pub const CRESCIMENTO_MINIMO: u64 = 64 * 1024; // 64 KiB
/// Limite do heap. Sem limite, um bug que aloca sem parar consumiria toda
/// a RAM antes de aparecer.
pub const TAMANHO_MAXIMO: u64 = 64 * 1024 * 1024; // 64 MiB

/// Páginas do heap: graváveis e proibidas de executar (NX). Dados nunca
/// devem virar código.
const BITS_HEAP: u64 = flags::ESCRITA | flags::SEM_EXECUCAO;

/// Arredonda `x` para cima, para o próximo múltiplo de `alinhamento`
/// (que precisa ser potência de 2).
pub const fn alinhar_acima(x: usize, alinhamento: usize) -> usize {
    (x + alinhamento - 1) & !(alinhamento - 1)
}

/// Mapeia `quantidade` bytes (múltiplo de 4 KiB) a partir de `inicio`, uma
/// página de cada vez, cada uma num quadro novo. Devolve quantos bytes
/// conseguiu mapear (menos que o pedido se os quadros acabarem).
pub(super) fn mapear_faixa(inicio: u64, quantidade: u64) -> u64 {
    let mut feito = 0;
    while feito < quantidade {
        match paginacao::alocar_e_mapear(inicio + feito, BITS_HEAP) {
            Ok(_) => feito += TAMANHO_QUADRO,
            Err(ErroMapeamento::SemMemoria) => break,
            Err(e) => panic!("heap: erro ao mapear {:#x}: {:?}", inicio + feito, e),
        }
    }
    feito
}

// ---------------------------------------------------------------------------
// A lista de blocos livres
// ---------------------------------------------------------------------------

/// Cabeçalho gravado no início de cada bloco LIVRE.
struct NoLivre {
    tamanho: usize,        // tamanho do bloco livre, incluindo este cabeçalho
    proximo: *mut NoLivre, // próximo bloco livre (endereço maior), ou nulo
}

/// Todo bloco tem pelo menos este tamanho e começa num múltiplo de 16:
/// assim sempre cabe um `NoLivre` quando ele for liberado.
const BLOCO_MINIMO: usize = core::mem::size_of::<NoLivre>(); // 16
const ALINHAMENTO_MINIMO: usize = 16;
const _: () = assert!(BLOCO_MINIMO == ALINHAMENTO_MINIMO);

/// Tamanho que o heap realmente reserva para um pedido: arredondado para
/// múltiplo de 16 e nunca menor que um `NoLivre`. `alocar` e `liberar`
/// usam a mesma conta, então os dois sempre concordam.
fn tamanho_real(layout: Layout) -> usize {
    alinhar_acima(layout.size(), ALINHAMENTO_MINIMO).max(BLOCO_MINIMO)
}

/// Números do heap, para os relatórios e os testes.
#[derive(Clone, Copy, Debug)]
pub struct Estatisticas {
    pub mapeado: u64,          // bytes de memória virtual já mapeados
    pub em_uso: usize,         // bytes entregues e ainda não liberados
    pub pico: usize,           // maior valor que `em_uso` já teve
    pub alocacoes: u64,
    pub liberacoes: u64,
    pub blocos_livres: usize,  // quantos nós há na lista
    pub maior_livre: usize,    // tamanho do maior bloco livre
}

pub struct Heap {
    livres: *mut NoLivre, // lista ordenada por endereço
    fim: u64,             // primeiro endereço virtual ainda NÃO mapeado
    em_uso: usize,
    pico: usize,
    alocacoes: u64,
    liberacoes: u64,
}

// O `Heap` guarda ponteiros crus, que o Rust não considera seguros para
// compartilhar. Todo acesso passa pelo `Mutex` HEAP, então é seguro.
unsafe impl Send for Heap {}

impl Heap {
    /// Procura o primeiro bloco livre em que cabe `tamanho` bytes alinhados
    /// em `alinhamento` ("first fit"). Se achar, recorta o pedaço e devolve
    /// o endereço; as sobras (antes e depois) continuam na lista.
    unsafe fn procurar(&mut self, tamanho: usize, alinhamento: usize) -> Option<usize> {
        // `elo` aponta para o campo que aponta para o nó atual: começa em
        // `self.livres` e depois vira o `proximo` do nó anterior. Assim dá
        // para tirar o nó atual da lista sem tratar a cabeça como caso especial.
        let mut elo: *mut *mut NoLivre = &raw mut self.livres;
        unsafe {
            while !(*elo).is_null() {
                let no = *elo;
                let inicio = no as usize;
                let fim = inicio + (*no).tamanho;

                // Onde o bloco pedido começaria dentro deste bloco livre.
                let mut bloco = alinhar_acima(inicio, alinhamento);
                // Se sobrar um pedacinho na frente, ele precisa caber um NoLivre.
                if bloco > inicio && bloco - inicio < BLOCO_MINIMO {
                    bloco = alinhar_acima(inicio + BLOCO_MINIMO, alinhamento);
                }

                let cabe = match bloco.checked_add(tamanho) {
                    Some(fim_bloco) => fim_bloco <= fim,
                    None => false,
                };
                if cabe {
                    let fim_bloco = bloco + tamanho;
                    let proximo = (*no).proximo;

                    // Sobra DEPOIS do bloco: vira um nó livre novo. Como tudo
                    // é múltiplo de 16, a sobra é 0 ou pelo menos 16 bytes.
                    let mut seguinte = proximo;
                    if fim > fim_bloco {
                        let sobra = fim_bloco as *mut NoLivre;
                        (*sobra).tamanho = fim - fim_bloco;
                        (*sobra).proximo = proximo;
                        seguinte = sobra;
                    }

                    // Sobra ANTES do bloco: o próprio nó encolhe e continua
                    // na lista. Sem sobra, o nó sai da lista.
                    if bloco > inicio {
                        (*no).tamanho = bloco - inicio;
                        (*no).proximo = seguinte;
                    } else {
                        *elo = seguinte;
                    }
                    return Some(bloco);
                }
                elo = &raw mut (*no).proximo;
            }
        }
        None
    }

    /// Devolve um bloco à lista, na posição certa (ordem de endereço), e
    /// junta com os vizinhos se eles encostarem nele.
    unsafe fn inserir(&mut self, inicio: usize, tamanho: usize) {
        unsafe {
            let mut anterior: *mut NoLivre = ptr::null_mut();
            let mut atual = self.livres;
            while !atual.is_null() && (atual as usize) < inicio {
                anterior = atual;
                atual = (*atual).proximo;
            }

            // Um bloco livre não pode se sobrepor a outro. Se isso acontecer,
            // alguém liberou memória que já estava livre.
            if !atual.is_null() {
                assert!(inicio + tamanho <= atual as usize,
                        "heap: liberar {:#x}: bloco ja estava livre (liberacao dupla)", inicio);
            }
            if !anterior.is_null() {
                assert!(anterior as usize + (*anterior).tamanho <= inicio,
                        "heap: liberar {:#x}: bloco ja estava livre (liberacao dupla)", inicio);
            }

            let no = inicio as *mut NoLivre;
            (*no).tamanho = tamanho;
            (*no).proximo = atual;

            // Junta com o vizinho de cima: [no][atual] -> [no.........]
            if !atual.is_null() && inicio + tamanho == atual as usize {
                (*no).tamanho += (*atual).tamanho;
                (*no).proximo = (*atual).proximo;
            }

            // Liga ao vizinho de baixo, juntando se encostar: [anterior][no] -> [anterior....]
            if anterior.is_null() {
                self.livres = no;
            } else if anterior as usize + (*anterior).tamanho == inicio {
                (*anterior).tamanho += (*no).tamanho;
                (*anterior).proximo = (*no).proximo;
            } else {
                (*anterior).proximo = no;
            }
        }
    }

    /// Mapeia mais memória no fim do heap. Devolve `false` se o limite foi
    /// atingido ou os quadros acabaram.
    unsafe fn crescer(&mut self, minimo: usize) -> bool {
        let limite = HEAP_INICIO + TAMANHO_MAXIMO;
        let disponivel = limite - self.fim;
        let pedido = (alinhar_acima(minimo, TAMANHO_QUADRO as usize) as u64)
            .max(CRESCIMENTO_MINIMO)
            .min(disponivel);
        if pedido < minimo as u64 {
            return false;
        }
        let feito = mapear_faixa(self.fim, pedido);
        if feito == 0 {
            return false;
        }
        unsafe { self.inserir(self.fim as usize, feito as usize) };
        self.fim += feito;
        true
    }

    /// Tamanho do último bloco livre, se ele terminar exatamente no fim do
    /// heap (senão, 0).
    fn livre_no_fim(&self) -> usize {
        let mut no = self.livres;
        let mut ultimo: *mut NoLivre = ptr::null_mut();
        while !no.is_null() {
            ultimo = no;
            no = unsafe { (*no).proximo };
        }
        if ultimo.is_null() {
            return 0;
        }
        let tamanho = unsafe { (*ultimo).tamanho };
        if ultimo as u64 + tamanho as u64 == self.fim { tamanho } else { 0 }
    }

    unsafe fn alocar(&mut self, layout: Layout) -> *mut u8 {
        let tamanho = tamanho_real(layout);
        let alinhamento = layout.align().max(ALINHAMENTO_MINIMO);
        loop {
            if let Some(bloco) = unsafe { self.procurar(tamanho, alinhamento) } {
                self.em_uso += tamanho;
                self.pico = self.pico.max(self.em_uso);
                self.alocacoes += 1;
                return bloco as *mut u8;
            }
            // Não coube: cresce o bastante para o pedido mais o pior caso
            // de alinhamento, e tenta de novo. Um bloco livre que já encosta
            // no fim do heap vai se juntar ao pedaço novo, então ele conta.
            let falta = (tamanho + alinhamento).saturating_sub(self.livre_no_fim());
            if !unsafe { self.crescer(falta) } {
                return ptr::null_mut(); // o Rust transforma isso em "memory allocation failed"
            }
        }
    }

    unsafe fn liberar(&mut self, ponteiro: *mut u8, layout: Layout) {
        let inicio = ponteiro as u64;
        assert!(inicio >= HEAP_INICIO && inicio < self.fim,
                "heap: liberar {:#x}: endereco fora do heap", inicio);
        let tamanho = tamanho_real(layout);
        unsafe { self.inserir(ponteiro as usize, tamanho) };
        self.em_uso -= tamanho;
        self.liberacoes += 1;
    }

    fn estatisticas(&self) -> Estatisticas {
        let mut blocos_livres = 0;
        let mut maior_livre = 0;
        let mut no = self.livres;
        while !no.is_null() {
            unsafe {
                blocos_livres += 1;
                maior_livre = maior_livre.max((*no).tamanho);
                no = (*no).proximo;
            }
        }
        Estatisticas {
            mapeado: self.fim - HEAP_INICIO,
            em_uso: self.em_uso,
            pico: self.pico,
            alocacoes: self.alocacoes,
            liberacoes: self.liberacoes,
            blocos_livres,
            maior_livre,
        }
    }
}

// ---------------------------------------------------------------------------
// O alocador global
// ---------------------------------------------------------------------------

static HEAP: Mutex<Option<Heap>> = Mutex::new(None);

/// O tipo que o Rust chama em todo `Box::new`, `Vec::push`, `String::from`...
pub struct AlocadorLista;

unsafe impl GlobalAlloc for AlocadorLista {
    // `sem_interrupcoes`: se um tratador de interrupção alocasse enquanto o
    // código interrompido segura o lock do heap, seria deadlock (doc 08).
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        crate::cpu::sem_interrupcoes(|| match HEAP.lock().as_mut() {
            Some(h) => unsafe { h.alocar(layout) },
            None => ptr::null_mut(), // heap ainda não existe
        })
    }

    unsafe fn dealloc(&self, ponteiro: *mut u8, layout: Layout) {
        crate::cpu::sem_interrupcoes(|| {
            let mut guarda = HEAP.lock();
            let h = guarda.as_mut().expect("heap nao inicializado");
            unsafe { h.liberar(ponteiro, layout) };
        })
    }
}

#[cfg(not(feature = "teste-bump"))]
#[global_allocator]
static ALOCADOR: AlocadorLista = AlocadorLista;

/// Mapeia o heap inicial e liga o alocador. Precisa vir depois de
/// `paginacao::init` (usa as tabelas do kernel e o alocador de quadros).
pub fn init() {
    #[cfg(feature = "teste-bump")]
    {
        super::bump::init();
    }
    #[cfg(not(feature = "teste-bump"))]
    {
        let mapeado = mapear_faixa(HEAP_INICIO, TAMANHO_INICIAL);
        assert!(mapeado == TAMANHO_INICIAL, "heap: sem quadros para o heap inicial");
        let mut h = Heap {
            livres: ptr::null_mut(),
            fim: HEAP_INICIO + mapeado,
            em_uso: 0,
            pico: 0,
            alocacoes: 0,
            liberacoes: 0,
        };
        // No começo, o heap inteiro é um único bloco livre.
        unsafe { h.inserir(HEAP_INICIO as usize, mapeado as usize) };
        *HEAP.lock() = Some(h);
    }
}

pub fn estatisticas() -> Estatisticas {
    // Interrupções desligadas, como em alloc/dealloc (docs 08 e 12).
    crate::cpu::sem_interrupcoes(|| HEAP.lock().as_ref().expect("heap nao inicializado").estatisticas())
}

/// O endereço pertence à faixa já mapeada do heap?
pub fn contem(endereco: u64) -> bool {
    crate::cpu::sem_interrupcoes(|| match HEAP.lock().as_ref() {
        Some(h) => endereco >= HEAP_INICIO && endereco < h.fim,
        None => false,
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    #[test_case]
    fn alinhar_acima_arredonda() {
        assert_eq!(alinhar_acima(0, 16), 0);
        assert_eq!(alinhar_acima(1, 16), 16);
        assert_eq!(alinhar_acima(16, 16), 16);
        assert_eq!(alinhar_acima(17, 4096), 4096);
    }

    #[test_case]
    fn box_mora_no_heap() {
        let b = Box::new(7u64);
        assert!(contem(&*b as *const u64 as u64));
    }

    #[test_case]
    fn sem_vazamento_e_blocos_juntados() {
        let antes = estatisticas();
        let mut v: Vec<Box<u64>> = (0..1000).map(Box::new).collect();
        v.retain(|b| **b % 2 == 0); // libera metade, deixando buracos
        assert_eq!(v.iter().map(|b| **b).sum::<u64>(), (0..1000).filter(|x| x % 2 == 0).sum());
        drop(v);
        let depois = estatisticas();
        assert_eq!(depois.em_uso, antes.em_uso, "vazamento");
        assert_eq!(depois.blocos_livres, antes.blocos_livres, "buracos nao se juntaram");
    }
}
