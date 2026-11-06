//! Gerenciamento de memória física.
//!
//! Explicação completa em docs/05-memoria-fisica.md.
//!
//! - `mapa`: cópia própria (ordenada e simplificada) do mapa de memória que
//!   o UEFI entregou no `exit_boot_services`.
//! - `quadros`: alocador de quadros físicos de 4 KiB, baseado em bitmap.
//! - `paginacao`: tabelas de página do kernel (tradução virtual -> físico).

// `allow(dead_code)`: algumas funções (alocar, liberar...) só são chamadas
// pelos testes e pelos próximos capítulos (paginação, heap). Sem isso, o
// compilador avisaria "nunca usado" na versão normal.
#[allow(dead_code)]
pub mod mapa;
#[allow(dead_code)]
pub mod paginacao;
#[allow(dead_code)]
pub mod quadros;

/// Tamanho de um quadro (e de uma página): 4 KiB.
pub const TAMANHO_QUADRO: u64 = 4096;

/// Abaixo de 1 MiB fica a "memória baixa" herdada do PC original. Nós a
/// deixamos de fora do alocador: no futuro ela será útil para acordar as
/// outras CPUs (capítulo 11), que começam em modo de 16 bits e só enxergam
/// o primeiro megabyte.
pub const INICIO_UTILIZAVEL: u64 = 0x10_0000; // 1 MiB
