//! Leitura mínima das tabelas ACPI: só o necessário para achar os
//! controladores de interrupção (a tabela MADT).
//!
//! Explicação em docs/08-interrupcoes-de-hardware.md.
//!
//! O caminho até a MADT:
//!
//!   UEFI (tabela de configuração) ──► RSDP ──► XSDT ──► várias tabelas ──► "APIC" (MADT)
//!
//! Todas as tabelas (menos a RSDP) começam com o mesmo cabeçalho de 36 bytes:
//! assinatura de 4 letras, tamanho total e um checksum (a soma de todos os
//! bytes da tabela, em u8, tem que dar 0).

use core::sync::atomic::{AtomicU64, Ordering};
use spin::Once;

use crate::memoria::paginacao::fisico_para_virtual;

/// Endereço físico da RSDP, guardado enquanto o UEFI ainda existia.
static RSDP: AtomicU64 = AtomicU64::new(0);

/// Precisa ser chamada ANTES do exit_boot_services: é o UEFI que diz onde
/// está a RSDP (na sua "tabela de configuração", procurada pelo GUID do ACPI 2.0).
pub fn guardar_rsdp() {
    let endereco = uefi::system::with_config_table(|tabelas| {
        tabelas.iter()
            .find(|t| t.guid == uefi::table::cfg::ACPI2_GUID)
            .map_or(0, |t| t.address as u64)
    });
    RSDP.store(endereco, Ordering::Relaxed);
}

/// Lê um valor de tipo T no endereço FÍSICO dado, sem exigir alinhamento
/// (os campos das tabelas ACPI não são alinhados).
fn ler<T: Copy>(fisico: u64) -> T {
    unsafe { (fisico_para_virtual(fisico) as *const T).read_unaligned() }
}

/// A soma de todos os bytes (em u8, com estouro) tem que dar zero.
fn checksum_ok(fisico: u64, tamanho: u64) -> bool {
    (0..tamanho).fold(0u8, |soma, i| soma.wrapping_add(ler::<u8>(fisico + i))) == 0
}

/// Cabeçalho comum de toda tabela ACPI (36 bytes).
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct Cabecalho {
    assinatura: [u8; 4],
    tamanho: u32,
    revisao: u8,
    checksum: u8,
    oem: [u8; 6],
    oem_tabela: [u8; 8],
    oem_revisao: u32,
    criador: u32,
    criador_revisao: u32,
}
const TAMANHO_CABECALHO: u64 = 36;
const _: () = assert!(core::mem::size_of::<Cabecalho>() == TAMANHO_CABECALHO as usize);

// ---------------------------------------------------------------------------
// O que a MADT nos conta
// ---------------------------------------------------------------------------

/// Um I/O APIC: o controlador que recebe as IRQs dos dispositivos.
#[derive(Clone, Copy)]
pub struct IoApic {
    pub id: u8,
    pub endereco: u64,
    /// Primeira "GSI" (global system interrupt) que ele atende.
    pub gsi_base: u32,
}

/// "Interrupt source override": a IRQ `irq` do barramento ISA (o PC antigo)
/// não chega no pino de mesmo número do I/O APIC, e sim no pino `gsi`.
#[derive(Clone, Copy)]
pub struct Desvio {
    pub irq: u8,
    pub gsi: u32,
    /// Bits 0-1: polaridade; bits 2-3: modo de disparo (ver `Roteamento`).
    pub flags: u16,
}

pub const MAX_DESVIOS: usize = 16;

pub struct Madt {
    /// Endereço físico do APIC local (o de cada CPU).
    pub lapic: u64,
    /// Bit 0 das flags: a máquina também tem os dois PICs 8259 antigos.
    pub tem_pic_8259: bool,
    /// CPUs (APICs locais) habilitadas.
    pub cpus: usize,
    pub ioapic: Option<IoApic>,
    pub desvios: [Option<Desvio>; MAX_DESVIOS],
}

/// Como uma IRQ ISA chega ao I/O APIC.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roteamento {
    pub gsi: u32,
    /// O sinal é "ativo em nível baixo" (0 volt = interrupção)?
    pub ativo_baixo: bool,
    /// Disparo por nível (enquanto o sinal estiver ativo) em vez de por borda?
    pub por_nivel: bool,
}

impl Madt {
    /// Para onde vai a IRQ ISA `irq`. Sem desvio na MADT: GSI = IRQ,
    /// ativo em nível alto, disparo por borda (o padrão do barramento ISA).
    pub fn rotear_irq(&self, irq: u8) -> Roteamento {
        let mut r = Roteamento { gsi: irq as u32, ativo_baixo: false, por_nivel: false };
        if let Some(d) = self.desvios.iter().flatten().find(|d| d.irq == irq) {
            r.gsi = d.gsi;
            r.ativo_baixo = d.flags & 0b11 == 0b11;       // 00 = padrão do barramento, 01 = alto, 11 = baixo
            r.por_nivel = (d.flags >> 2) & 0b11 == 0b11;  // 00 = padrão do barramento, 01 = borda, 11 = nível
        }
        r
    }
}

static MADT: Once<Madt> = Once::new();

/// Lê (uma vez) e devolve a MADT. Precisa das tabelas de página do kernel
/// (lê a memória física pela metade alta).
pub fn madt() -> &'static Madt {
    MADT.call_once(|| ler_madt().expect("ACPI: MADT nao encontrada"))
}

/// Endereço físico da tabela com a assinatura dada, procurando na XSDT.
fn procurar_tabela(assinatura: &[u8; 4]) -> Option<u64> {
    let rsdp = RSDP.load(Ordering::Relaxed);
    assert!(rsdp != 0, "ACPI: o UEFI nao informou a RSDP");
    // RSDP (ACPI 2.0+): "RSD PTR " (8 bytes), ..., revisão no byte 15,
    // endereço da XSDT (64 bits) no byte 24. Checksum estendido sobre 36 bytes.
    assert!(ler::<[u8; 8]>(rsdp) == *b"RSD PTR ", "ACPI: RSDP invalida");
    assert!(ler::<u8>(rsdp + 15) >= 2 && checksum_ok(rsdp, 36), "ACPI: RSDP sem XSDT");
    let xsdt: u64 = ler(rsdp + 24);

    let cab: Cabecalho = ler(xsdt);
    assert!(cab.assinatura == *b"XSDT" && checksum_ok(xsdt, cab.tamanho as u64), "ACPI: XSDT invalida");
    // Depois do cabeçalho, a XSDT é só uma lista de endereços de 8 bytes.
    let n = (cab.tamanho as u64 - TAMANHO_CABECALHO) / 8;
    (0..n)
        .map(|i| ler::<u64>(xsdt + TAMANHO_CABECALHO + i * 8))
        .find(|&t| ler::<[u8; 4]>(t) == *assinatura)
}

fn ler_madt() -> Option<Madt> {
    // A MADT tem a assinatura "APIC" (nome histórico).
    let endereco = procurar_tabela(b"APIC")?;
    let cab: Cabecalho = ler(endereco);
    assert!(checksum_ok(endereco, cab.tamanho as u64), "ACPI: checksum da MADT errado");

    // Depois do cabeçalho: endereço do APIC local (32 bits) e flags (32 bits).
    let mut madt = Madt {
        lapic: ler::<u32>(endereco + 36) as u64,
        tem_pic_8259: ler::<u32>(endereco + 40) & 1 != 0,
        cpus: 0,
        ioapic: None,
        desvios: [None; MAX_DESVIOS],
    };

    // Em seguida, registros de tamanho variável: [tipo: u8][tamanho: u8][dados...]
    let mut p = endereco + 44;
    let fim = endereco + cab.tamanho as u64;
    let mut n_desvios = 0;
    while p + 2 <= fim {
        let tipo: u8 = ler(p);
        let tamanho: u8 = ler(p + 1);
        match tipo {
            // 0: um APIC local (uma CPU). Flags bit 0 = habilitada.
            0 => if ler::<u32>(p + 4) & 1 != 0 { madt.cpus += 1 },
            // 1: um I/O APIC. Usamos o primeiro (o QEMU tem um só).
            1 => if madt.ioapic.is_none() {
                madt.ioapic = Some(IoApic {
                    id: ler(p + 2),
                    endereco: ler::<u32>(p + 4) as u64,
                    gsi_base: ler(p + 8),
                });
            },
            // 2: desvio de IRQ ISA (barramento 0).
            2 => if n_desvios < MAX_DESVIOS {
                madt.desvios[n_desvios] = Some(Desvio { irq: ler(p + 3), gsi: ler(p + 4), flags: ler(p + 8) });
                n_desvios += 1;
            },
            // 5: endereço de 64 bits do APIC local, substitui o de 32.
            5 => madt.lapic = ler(p + 4),
            _ => {}
        }
        if tamanho < 2 { break; } // tabela corrompida: evita laço infinito
        p += tamanho as u64;
    }
    Some(madt)
}

// ---------------------------------------------------------------------------
// Desligar (estado S5 do ACPI)
// ---------------------------------------------------------------------------

/// Tenta desligar a máquina pelo ACPI. Só volta se não conseguir.
///
/// Desligar é escrever, no registrador "PM1 control" (porta indicada na
/// tabela FADT), o código do estado S5 ("soft off") junto com o bit SLP_EN.
/// O código do S5 não está numa tabela simples: fica no DSDT, que é
/// *bytecode* (AML). Aqui só procuramos o objeto `_S5_` e lemos o primeiro
/// número do pacote dele, o truque clássico que funciona no QEMU e na
/// maioria das máquinas; um interpretador de AML de verdade fica para depois.
pub fn desligar() {
    use crate::serial::{inw, outb, outw};

    let Some(fadt) = procurar_tabela(b"FACP") else { return };
    let cab: Cabecalho = ler(fadt);
    let smi_cmd: u32 = ler(fadt + 48);
    let acpi_enable: u8 = ler(fadt + 52);
    let pm1a = ler::<u32>(fadt + 64) as u16;
    let pm1b = ler::<u32>(fadt + 68) as u16;
    // DSDT: endereço de 64 bits (X_DSDT, byte 140) se a tabela for longa o
    // bastante e ele não for zero; senão o de 32 bits (byte 40).
    let mut dsdt = ler::<u32>(fadt + 40) as u64;
    if cab.tamanho >= 148 && ler::<u64>(fadt + 140) != 0 {
        dsdt = ler(fadt + 140);
    }
    let Some(slp_typ) = tipo_s5(dsdt) else { return };

    const SCI_EN: u16 = 1 << 0;   // o hardware está em "modo ACPI"
    const SLP_EN: u16 = 1 << 13;  // "entrar no estado agora"
    unsafe {
        // Se o firmware não ligou o modo ACPI, pedir para ligar e esperar.
        if inw(pm1a) & SCI_EN == 0 && smi_cmd != 0 {
            outb(smi_cmd as u16, acpi_enable);
            while inw(pm1a) & SCI_EN == 0 {}
        }
        outw(pm1a, (slp_typ as u16) << 10 | SLP_EN);
        if pm1b != 0 {
            outw(pm1b, (slp_typ as u16) << 10 | SLP_EN);
        }
    }
    // O desligamento não é instantâneo (no QEMU, o pedido é atendido um
    // pouco depois). Esperar ~200 ms antes de dizer que não funcionou.
    let fim = crate::interrupcoes::timer::tiques() + 20;
    while crate::interrupcoes::timer::tiques() < fim {
        core::hint::spin_loop();
    }
}

/// Procura `_S5_` no DSDT e devolve o primeiro elemento do pacote:
///   08 '_S5_' 12 <tamanho> <n. elementos> [0A] <SLP_TYPa> ...
///   NameOp     PackageOp                   BytePrefix
fn tipo_s5(dsdt: u64) -> Option<u8> {
    let cab: Cabecalho = ler(dsdt);
    let fim = dsdt + cab.tamanho as u64;
    let mut p = dsdt + TAMANHO_CABECALHO;
    while p + 4 < fim {
        if ler::<[u8; 4]>(p) == *b"_S5_" && ler::<u8>(p - 1) == 0x08 && ler::<u8>(p + 4) == 0x12 {
            let mut q = p + 5;
            let tamanho_pacote: u8 = ler(q);
            q += ((tamanho_pacote >> 6) & 0b11) as u64 + 1; // bits 6-7: bytes extras do tamanho
            q += 1; // número de elementos
            if ler::<u8>(q) == 0x0A { q += 1; } // BytePrefix: o número vem no byte seguinte
            return Some(ler(q));
        }
        p += 1;
    }
    None
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn madt_tem_cpu_e_ioapic() {
        let m = madt();
        assert!(m.cpus >= 1);
        assert_eq!(m.lapic, 0xFEE0_0000);
        assert!(m.ioapic.is_some());
    }

    #[test_case]
    fn irq_sem_desvio_vai_para_a_mesma_gsi() {
        // O teclado (IRQ 1) não tem desvio no QEMU.
        let r = madt().rotear_irq(1);
        assert_eq!(r, Roteamento { gsi: 1, ativo_baixo: false, por_nivel: false });
    }

    #[test_case]
    fn irq0_desviada_para_gsi2_no_qemu() {
        assert_eq!(madt().rotear_irq(0).gsi, 2);
    }

    #[test_case]
    fn dsdt_tem_s5() {
        let fadt = procurar_tabela(b"FACP").expect("FADT");
        let dsdt = ler::<u32>(fadt + 40) as u64;
        assert_eq!(ler::<[u8; 4]>(dsdt), *b"DSDT");
        assert!(tipo_s5(dsdt).is_some());
    }
}
