//! O mapa de memória do kernel.
//!
//! O UEFI descreve a memória com dezenas de tipos e regiões fragmentadas,
//! que nem sempre vêm em ordem. Aqui copiamos tudo para um formato nosso:
//!   1. tipos simplificados (7 em vez de ~16);
//!   2. regiões ordenadas por endereço;
//!   3. regiões vizinhas do mesmo tipo unidas numa só.
//! A cópia fica num `static`, então não dependemos mais da crate `uefi`
//! nem do buffer em que o firmware entregou o mapa.

use spin::Once;
use uefi::boot::MemoryType;
use uefi::mem::memory_map::MemoryMap;

use super::TAMANHO_QUADRO;

/// O que existe numa região de endereços físicos, do ponto de vista do kernel.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// RAM livre: pode ser entregue pelo alocador agora.
    Livre,
    /// RAM que o firmware usou durante o boot. Depois do exit_boot_services
    /// ela poderia ser reaproveitada, MAS a pilha em que o kernel está
    /// rodando agora mora aqui. Só será liberada quando tivermos pilha própria.
    BootServices,
    /// O próprio SiliconOS: código, dados, statics (GDT, IDT, TSS...) e o buffer
    /// do mapa de memória ("LOADER_*" no UEFI). Nunca liberar.
    Kernel,
    /// Código e dados que o firmware mantém mesmo depois do boot (serviços de
    /// runtime, como o relógio). Nunca tocar.
    Runtime,
    /// Tabelas ACPI: descrevem o hardware (capítulo 11). Ler antes de liberar.
    Acpi,
    /// Endereços de dispositivos (memory-mapped I/O). Não é RAM.
    Mmio,
    /// Reservado, defeituoso ou desconhecido. Nunca tocar.
    Reservada,
}

impl Tipo {
    fn de_uefi(t: MemoryType) -> Tipo {
        match t {
            MemoryType::CONVENTIONAL => Tipo::Livre,
            MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA => Tipo::BootServices,
            MemoryType::LOADER_CODE | MemoryType::LOADER_DATA => Tipo::Kernel,
            MemoryType::RUNTIME_SERVICES_CODE | MemoryType::RUNTIME_SERVICES_DATA => Tipo::Runtime,
            MemoryType::ACPI_RECLAIM | MemoryType::ACPI_NON_VOLATILE => Tipo::Acpi,
            MemoryType::MMIO | MemoryType::MMIO_PORT_SPACE => Tipo::Mmio,
            _ => Tipo::Reservada,
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Tipo::Livre => "Livre",
            Tipo::BootServices => "BootServices",
            Tipo::Kernel => "Kernel",
            Tipo::Runtime => "Runtime",
            Tipo::Acpi => "ACPI",
            Tipo::Mmio => "MMIO",
            Tipo::Reservada => "Reservada",
        }
    }

    /// Todos os tipos, na ordem em que aparecem no resumo.
    pub const TODOS: [Tipo; 7] = [
        Tipo::Livre, Tipo::BootServices, Tipo::Kernel, Tipo::Runtime,
        Tipo::Acpi, Tipo::Mmio, Tipo::Reservada,
    ];
}

/// Um intervalo contínuo de endereços físicos: [inicio, fim).
#[derive(Clone, Copy)]
pub struct Regiao {
    pub inicio: u64,
    pub quadros: u64,
    pub tipo: Tipo,
}

impl Regiao {
    const VAZIA: Regiao = Regiao { inicio: 0, quadros: 0, tipo: Tipo::Reservada };

    /// Primeiro endereço DEPOIS da região.
    pub fn fim(&self) -> u64 {
        self.inicio + self.quadros * TAMANHO_QUADRO
    }

    pub fn contem(&self, endereco: u64) -> bool {
        endereco >= self.inicio && endereco < self.fim()
    }
}

/// Mais que suficiente: o OVMF entrega ~130 entradas no QEMU.
const MAX_REGIOES: usize = 512;

pub struct Mapa {
    regioes: [Regiao; MAX_REGIOES],
    qtd: usize,
    /// Quantas entradas o UEFI entregou (antes de juntar vizinhas).
    pub entradas_uefi: usize,
}

impl Mapa {
    /// As regiões válidas (um "pedaço" do array, como um subList do Java).
    pub fn regioes(&self) -> &[Regiao] {
        &self.regioes[..self.qtd]
    }

    /// Em que região está um endereço físico (se estiver em alguma).
    pub fn encontrar(&self, endereco: u64) -> Option<&Regiao> {
        self.regioes().iter().find(|r| r.contem(endereco))
    }

    /// Soma de quadros e número de regiões de um tipo.
    pub fn total(&self, tipo: Tipo) -> (usize, u64) {
        self.regioes()
            .iter()
            .filter(|r| r.tipo == tipo)
            .fold((0, 0), |(n, q), r| (n + 1, q + r.quadros))
    }

    /// Maior endereço de RAM (tudo que não é MMIO nem Reservada).
    /// Define até onde o alocador precisa "enxergar".
    pub fn fim_da_ram(&self) -> u64 {
        self.regioes()
            .iter()
            .filter(|r| !matches!(r.tipo, Tipo::Mmio | Tipo::Reservada))
            .map(|r| r.fim())
            .max()
            .unwrap_or(0)
    }
}

static MAPA: Once<Mapa> = Once::new();

/// Copia o mapa do UEFI para o formato do kernel. Só faz o trabalho na
/// primeira chamada; depois devolve sempre a mesma cópia.
pub fn init(mapa_uefi: &impl MemoryMap) -> &'static Mapa {
    MAPA.call_once(|| {
        let mut m = Mapa { regioes: [Regiao::VAZIA; MAX_REGIOES], qtd: 0, entradas_uefi: 0 };

        // 1. Copiar, convertendo o tipo.
        for d in mapa_uefi.entries() {
            assert!(m.qtd < MAX_REGIOES, "mapa de memoria com mais de {} regioes", MAX_REGIOES);
            m.regioes[m.qtd] = Regiao {
                inicio: d.phys_start,
                quadros: d.page_count,
                tipo: Tipo::de_uefi(d.ty),
            };
            m.qtd += 1;
        }
        m.entradas_uefi = m.qtd;

        // 2. Ordenar por endereço (a especificação UEFI não garante a ordem).
        m.regioes[..m.qtd].sort_unstable_by_key(|r| r.inicio);

        // 3. Juntar vizinhas do mesmo tipo: [A][A][B][A] -> [AA][B][A]
        let mut escrita = 0;
        for leitura in 0..m.qtd {
            let r = m.regioes[leitura];
            if escrita > 0 {
                let anterior = &mut m.regioes[escrita - 1];
                if anterior.tipo == r.tipo && anterior.fim() == r.inicio {
                    anterior.quadros += r.quadros;
                    continue;
                }
            }
            m.regioes[escrita] = r;
            escrita += 1;
        }
        m.qtd = escrita;
        m
    })
}

/// O mapa já inicializado (para quem vier depois, como o alocador).
pub fn get() -> &'static Mapa {
    MAPA.get().expect("memoria::mapa::init() ainda nao foi chamado")
}
