//! Alocador de quadros físicos (blocos de 4 KiB de RAM), baseado em bitmap.
//!
//! Um bit para cada quadro de RAM: 1 = ocupado, 0 = livre.
//! Com 128 MiB de RAM são 32 768 quadros -> 32 768 bits -> 4 KiB de bitmap.
//! Com 4 GiB, 1 Mi de quadros -> 128 KiB de bitmap.
//!
//! O próprio bitmap precisa morar em algum lugar: usamos o começo da
//! primeira região Livre grande o bastante, e marcamos esses quadros como
//! ocupados (o bitmap não pode se entregar a alguém!).
//!
//! IMPORTANTE: no início, escrevemos no bitmap usando o endereço FÍSICO como
//! se fosse um ponteiro. Isso funciona porque o UEFI deixou a paginação em
//! "mapeamento identidade" (endereço virtual = endereço físico). Quando as
//! tabelas de página do kernel assumem (paginacao::init), o bitmap passa a
//! ser acessado pela metade alta: ver `reposicionar_bitmap`.

use spin::Mutex;

use super::mapa::{Mapa, Tipo};
use super::{INICIO_UTILIZAVEL, TAMANHO_QUADRO};

/// Endereço físico de um quadro. Um tipo próprio (em vez de u64 solto) para
/// o compilador não deixar misturar "endereço físico" com "um número qualquer".
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Quadro(pub u64);

impl Quadro {
    pub fn endereco(self) -> u64 {
        self.0
    }
    fn indice(self) -> usize {
        (self.0 / TAMANHO_QUADRO) as usize
    }
}

pub struct AlocadorQuadros {
    /// bitmap[i] guarda os bits dos quadros 64*i .. 64*i+63
    bitmap: &'static mut [u64],
    /// Quantos quadros o bitmap cobre (do endereço 0 até o fim da RAM).
    total: usize,
    livres: usize,
    /// Onde começar a próxima busca (evita varrer do zero toda vez).
    proxima_palavra: usize,
    /// Endereço e tamanho do próprio bitmap (para o relatório).
    pub bitmap_endereco: u64,
    pub bitmap_quadros: u64,
}

impl AlocadorQuadros {
    fn ocupado(&self, quadro: usize) -> bool {
        self.bitmap[quadro / 64] & (1 << (quadro % 64)) != 0
    }

    fn marcar(&mut self, quadro: usize, ocupado: bool) {
        let (palavra, bit) = (quadro / 64, quadro % 64);
        if ocupado {
            self.bitmap[palavra] |= 1 << bit;
        } else {
            self.bitmap[palavra] &= !(1 << bit);
        }
    }

    /// Marca um intervalo de quadros [primeiro, primeiro + n).
    fn marcar_intervalo(&mut self, primeiro: usize, n: usize, ocupado: bool) {
        for q in primeiro..(primeiro + n).min(self.total) {
            self.marcar(q, ocupado);
        }
    }

    /// Entrega um quadro livre, ou None se a memória acabou.
    pub fn alocar(&mut self) -> Option<Quadro> {
        let palavras = self.bitmap.len();
        // Procura a partir de `proxima_palavra` e dá no máximo uma volta.
        for i in 0..palavras {
            let p = (self.proxima_palavra + i) % palavras;
            let valor = self.bitmap[p];
            if valor != u64::MAX {
                // Há pelo menos um bit 0 nesta palavra. trailing_ones() conta
                // quantos 1 seguidos há a partir do bit 0: esse é o índice
                // do primeiro bit livre.
                let bit = valor.trailing_ones() as usize;
                let quadro = p * 64 + bit;
                if quadro >= self.total {
                    continue; // bits além do fim da RAM (sempre ficam em 1)
                }
                self.marcar(quadro, true);
                self.livres -= 1;
                self.proxima_palavra = p;
                return Some(Quadro(quadro as u64 * TAMANHO_QUADRO));
            }
        }
        None
    }

    /// Devolve um quadro. Liberar duas vezes o mesmo quadro é um bug grave
    /// (dois donos para a mesma memória no futuro): vira panic na hora.
    pub fn liberar(&mut self, quadro: Quadro) {
        assert!(quadro.0 % TAMANHO_QUADRO == 0, "liberar: endereco {:#x} nao alinhado em 4 KiB", quadro.0);
        let i = quadro.indice();
        assert!(i < self.total, "liberar: {:#x} esta fora da RAM", quadro.0);
        assert!(self.ocupado(i), "liberar: quadro {:#x} ja estava livre (liberacao dupla)", quadro.0);
        self.marcar(i, false);
        self.livres += 1;
        if i / 64 < self.proxima_palavra {
            self.proxima_palavra = i / 64;
        }
    }

    pub fn livres(&self) -> usize {
        self.livres
    }

    pub fn total(&self) -> usize {
        self.total
    }
}

/// O alocador global. Começa vazio (None) até `init`.
pub static QUADROS: Mutex<Option<AlocadorQuadros>> = Mutex::new(None);

pub fn init(mapa: &Mapa) {
    let fim_ram = mapa.fim_da_ram();
    let total = (fim_ram / TAMANHO_QUADRO) as usize;
    let palavras = total.div_ceil(64);
    let bytes_bitmap = (palavras * 8) as u64;
    let quadros_bitmap = bytes_bitmap.div_ceil(TAMANHO_QUADRO);

    // 1. Onde guardar o bitmap: início da primeira região Livre (acima de
    //    1 MiB) que caiba nele inteiro.
    let regiao = mapa
        .regioes()
        .iter()
        .filter(|r| r.tipo == Tipo::Livre)
        .find(|r| {
            let inicio = r.inicio.max(INICIO_UTILIZAVEL);
            inicio + quadros_bitmap * TAMANHO_QUADRO <= r.fim()
        })
        .expect("nenhuma regiao livre comporta o bitmap de quadros");
    let endereco_bitmap = regiao.inicio.max(INICIO_UTILIZAVEL);

    // 2. Transformar esse endereço físico num slice Rust (`&mut [u64]`).
    //    Seguro porque: a região é RAM livre (ninguém mais a usa), está
    //    mapeada em identidade pelo UEFI, e será marcada como ocupada abaixo.
    let bitmap: &'static mut [u64] = unsafe {
        core::slice::from_raw_parts_mut(endereco_bitmap as *mut u64, palavras)
    };

    // 3. Começar com TUDO ocupado. Só o que provarmos ser livre vira 0.
    //    (Errar para o lado "ocupado" desperdiça memória; errar para o lado
    //    "livre" entrega memória de alguém -> corrupção. Melhor o primeiro.)
    bitmap.fill(u64::MAX);

    let mut a = AlocadorQuadros {
        bitmap,
        total,
        livres: 0,
        proxima_palavra: 0,
        bitmap_endereco: endereco_bitmap,
        bitmap_quadros: quadros_bitmap,
    };

    // 4. Liberar as regiões Livres (só a parte acima de 1 MiB).
    for r in mapa.regioes().iter().filter(|r| r.tipo == Tipo::Livre) {
        let inicio = r.inicio.max(INICIO_UTILIZAVEL);
        if inicio >= r.fim() {
            continue;
        }
        let primeiro = (inicio / TAMANHO_QUADRO) as usize;
        let n = ((r.fim() - inicio) / TAMANHO_QUADRO) as usize;
        a.marcar_intervalo(primeiro, n, false);
    }

    // 5. Proteger o próprio bitmap.
    a.marcar_intervalo((endereco_bitmap / TAMANHO_QUADRO) as usize, quadros_bitmap as usize, true);

    // 6. Contar quantos ficaram livres.
    a.livres = (0..total).filter(|&q| !a.ocupado(q)).count();
    a.proxima_palavra = (INICIO_UTILIZAVEL / TAMANHO_QUADRO / 64) as usize;

    *QUADROS.lock() = Some(a);
}

/// Passa a acessar o bitmap por outro endereço virtual (físico + deslocamento).
/// Chamado pela paginação quando o kernel troca de tabelas.
pub fn reposicionar_bitmap(deslocamento: u64) {
    let mut guarda = QUADROS.lock();
    let a = guarda.as_mut().expect("alocador de quadros nao inicializado");
    let palavras = a.bitmap.len();
    a.bitmap = unsafe {
        core::slice::from_raw_parts_mut((a.bitmap_endereco + deslocamento) as *mut u64, palavras)
    };
}

/// Atalhos para usar o alocador global sem lidar com o Mutex/Option.
pub fn alocar() -> Option<Quadro> {
    QUADROS.lock().as_mut().expect("alocador de quadros nao inicializado").alocar()
}

pub fn liberar(quadro: Quadro) {
    QUADROS.lock().as_mut().expect("alocador de quadros nao inicializado").liberar(quadro)
}

pub fn livres() -> usize {
    QUADROS.lock().as_ref().map_or(0, |a| a.livres())
}
