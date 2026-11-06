//! Alocador "bump": o mais simples possível, só para comparação.
//!
//! Explicação em docs/07-heap.md, seção 3. Só é usado como alocador global
//! quando o kernel é compilado com `--features teste-bump`.
//!
//! Ele guarda um único ponteiro, `proximo`. Alocar = arredondar `proximo`
//! para o alinhamento pedido, entregar esse endereço e empurrar `proximo`
//! para a frente. Liberar = não fazer nada. É rapidíssimo, mas a memória
//! liberada nunca volta a ser usada.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use spin::Mutex;

use super::heap::{alinhar_acima, mapear_faixa, HEAP_INICIO, TAMANHO_INICIAL};

struct Bump {
    fim: usize,     // primeiro endereço depois da faixa mapeada
    proximo: usize, // onde começa a próxima alocação
    alocacoes: u64,
    liberacoes: u64,
}

static BUMP: Mutex<Bump> = Mutex::new(Bump { fim: 0, proximo: 0, alocacoes: 0, liberacoes: 0 });

pub struct AlocadorBump;

unsafe impl GlobalAlloc for AlocadorBump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut b = BUMP.lock();
        let inicio = alinhar_acima(b.proximo, layout.align());
        let fim = match inicio.checked_add(layout.size()) {
            Some(f) => f,
            None => return ptr::null_mut(),
        };
        if b.proximo == 0 || fim > b.fim {
            return ptr::null_mut(); // não inicializado, ou acabou
        }
        b.proximo = fim;
        b.alocacoes += 1;
        inicio as *mut u8
    }

    unsafe fn dealloc(&self, _ponteiro: *mut u8, _layout: Layout) {
        // Não há como devolver: o bump só sabe andar para a frente.
        BUMP.lock().liberacoes += 1;
    }
}

#[cfg(feature = "teste-bump")]
#[global_allocator]
static ALOCADOR: AlocadorBump = AlocadorBump;

pub fn init() {
    let mapeado = mapear_faixa(HEAP_INICIO, TAMANHO_INICIAL);
    let mut b = BUMP.lock();
    b.proximo = HEAP_INICIO as usize;
    b.fim = (HEAP_INICIO + mapeado) as usize;
}

/// (bytes já consumidos, alocações, liberações)
pub fn estado() -> (usize, u64, u64) {
    let b = BUMP.lock();
    (b.proximo - HEAP_INICIO as usize, b.alocacoes, b.liberacoes)
}
