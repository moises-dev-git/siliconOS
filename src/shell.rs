//! Shell: uma linha de comando dentro do kernel.
//!
//! Explicação completa em docs/09-shell-e-testes.md.
//!
//! O shell recebe os caracteres já traduzidos pelo decodificador do teclado
//! (doc 08), monta a linha num `String` do heap (doc 07) e, no Enter,
//! procura o comando numa tabela e chama a função dele.

use alloc::string::String;
use alloc::vec::Vec;

use crate::interrupcoes::{self, teclado, timer};
use crate::memoria::{self, heap, quadros};
use crate::{cpu, kprint, kprintln};

pub const PROMPT: &str = "silicon> ";
/// Uma linha não passa disto (a tela tem 80 colunas).
const MAX_LINHA: usize = 70;

pub struct Shell {
    linha: String,
}

impl Shell {
    pub fn new() -> Self {
        Shell { linha: String::new() }
    }

    /// Trata um caractere digitado: edita a linha ou, no Enter, executa.
    pub fn tecla(&mut self, c: char) {
        match c {
            '\n' => {
                kprintln!();
                // `take` devolve a linha e deixa uma String vazia no lugar.
                let linha = core::mem::take(&mut self.linha);
                executar(&linha);
                kprint!("{}", PROMPT);
            }
            // Backspace: só apaga o que foi digitado, nunca o prompt.
            '\x08' => {
                if self.linha.pop().is_some() {
                    kprint!("\x08");
                }
            }
            c if (c.is_ascii_graphic() || c == ' ') && self.linha.len() < MAX_LINHA => {
                self.linha.push(c);
                kprint!("{}", c);
            }
            _ => {} // Tab, Esc, linha cheia...: ignorado
        }
    }
}

/// O laço principal do kernel daqui em diante: tecla -> shell, e dormir.
pub fn rodar() -> ! {
    let mut decodificador = teclado::Decodificador::new();
    let mut shell = Shell::new();
    kprintln!("Shell do SiliconOS. Digite 'ajuda' para ver os comandos.");
    kprint!("{}", PROMPT);
    loop {
        while let Some(scancode) = teclado::proximo_scancode() {
            #[cfg(feature = "teste-teclado")]
            kprintln!("  scancode {:#04x}", scancode);
            if let Some(c) = decodificador.processar(scancode) {
                shell.tecla(c);
            }
        }
        teclado::esperar();
    }
}

// ---------------------------------------------------------------------------
// A tabela de comandos
// ---------------------------------------------------------------------------

/// Um comando: nome, como usar, o que faz e a função que o executa.
/// `funcao` é um ponteiro de função: recebe os argumentos já separados.
pub struct Comando {
    pub nome: &'static str,
    pub uso: &'static str,
    pub descricao: &'static str,
    pub funcao: fn(&[&str]),
}

pub const COMANDOS: &[Comando] = &[
    Comando { nome: "ajuda", uso: "ajuda", descricao: "lista os comandos", funcao: ajuda },
    Comando { nome: "eco", uso: "eco <texto>", descricao: "repete o texto", funcao: eco },
    Comando { nome: "limpar", uso: "limpar", descricao: "limpa a tela", funcao: limpar },
    Comando { nome: "tempo", uso: "tempo", descricao: "tempo ligado e hora do relogio CMOS", funcao: tempo },
    Comando { nome: "mem", uso: "mem", descricao: "quadros livres e estado do heap", funcao: mem },
    Comando { nome: "cpu", uso: "cpu", descricao: "fabricante e modelo (instrucao cpuid)", funcao: cpu_info },
    Comando { nome: "int", uso: "int", descricao: "contadores de interrupcoes", funcao: int },
    Comando { nome: "traduzir", uso: "traduzir <endereco>", descricao: "mostra a traducao de um endereco virtual", funcao: traduzir },
    Comando { nome: "ler", uso: "ler <endereco>", descricao: "le 8 bytes da memoria (pode dar page fault!)", funcao: ler },
    Comando { nome: "reiniciar", uso: "reiniciar", descricao: "reinicia a maquina", funcao: reiniciar },
    Comando { nome: "desligar", uso: "desligar", descricao: "desliga a maquina (ACPI; testado so no QEMU)", funcao: desligar },
];

/// Separa a linha em palavras, acha o comando e o executa.
pub fn executar(linha: &str) {
    let palavras: Vec<&str> = linha.split_whitespace().collect();
    let Some((&nome, argumentos)) = palavras.split_first() else {
        return; // linha vazia
    };
    match COMANDOS.iter().find(|c| c.nome == nome) {
        Some(comando) => (comando.funcao)(argumentos),
        None => kprintln!("comando desconhecido: '{}' (digite 'ajuda')", nome),
    }
}

/// Lê um número em decimal ou, com prefixo 0x, em hexadecimal.
/// Aceita `_` como separador: 0xffff_9000_0000_0000.
pub fn ler_numero(texto: &str) -> Option<u64> {
    let limpo: String = texto.chars().filter(|&c| c != '_').collect();
    match limpo.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => limpo.parse().ok(),
    }
}

// ---------------------------------------------------------------------------
// Os comandos
// ---------------------------------------------------------------------------

fn ajuda(_: &[&str]) {
    for c in COMANDOS {
        kprintln!("  {:<22}{}", c.uso, c.descricao);
    }
}

fn eco(argumentos: &[&str]) {
    kprintln!("{}", argumentos.join(" "));
}

fn limpar(_: &[&str]) {
    cpu::sem_interrupcoes(|| {
        if let Some(con) = crate::CONSOLE.lock().as_mut() {
            con.limpar();
        }
    });
}

fn tempo(_: &[&str]) {
    let ms = timer::milissegundos();
    let s = ms / 1000;
    kprintln!("ligado ha {}h {:02}m {:02}.{:03}s ({} tiques a {} Hz)",
              s / 3600, s / 60 % 60, s % 60, ms % 1000, timer::tiques(), timer::FREQUENCIA_HZ);
    let (h, m, seg) = timer::hora_rtc();
    kprintln!("relogio CMOS (RTC): {:02}:{:02}:{:02}", h, m, seg);
}

fn mem(_: &[&str]) {
    let total = quadros::QUADROS.lock().as_ref().map_or(0, |a| a.total());
    let livres = quadros::livres();
    kprintln!("quadros: {} livres de {} ({} MiB livres)",
              livres, total, livres as u64 * memoria::TAMANHO_QUADRO / (1024 * 1024));
    let e = heap::estatisticas();
    kprintln!("heap: {} bytes em uso ({} alocacoes vivas), pico {} KiB",
              e.em_uso, e.alocacoes - e.liberacoes, e.pico / 1024);
    kprintln!("      mapeado {} KiB, {} blocos livres, maior {} KiB",
              e.mapeado / 1024, e.blocos_livres, e.maior_livre / 1024);
}

fn cpu_info(_: &[&str]) {
    use core::arch::x86_64::__cpuid;
    // Folha 0: o fabricante, 12 letras em EBX, EDX, ECX (nessa ordem).
    let r = __cpuid(0);
    let mut fabricante = [0u8; 12];
    fabricante[0..4].copy_from_slice(&r.ebx.to_le_bytes());
    fabricante[4..8].copy_from_slice(&r.edx.to_le_bytes());
    fabricante[8..12].copy_from_slice(&r.ecx.to_le_bytes());
    kprintln!("fabricante: {}", core::str::from_utf8(&fabricante).unwrap_or("?"));

    // Folhas 0x8000_0002 a 0x8000_0004: o nome do modelo, 48 letras.
    if __cpuid(0x8000_0000).eax >= 0x8000_0004 {
        let mut nome = [0u8; 48];
        for (i, folha) in (0x8000_0002u32..=0x8000_0004).enumerate() {
            let r = __cpuid(folha);
            for (j, registrador) in [r.eax, r.ebx, r.ecx, r.edx].iter().enumerate() {
                let k = i * 16 + j * 4;
                nome[k..k + 4].copy_from_slice(&registrador.to_le_bytes());
            }
        }
        let nome = core::str::from_utf8(&nome).unwrap_or("?");
        kprintln!("modelo:     {}", nome.trim_matches(|c: char| c == '\0' || c == ' '));
    }
    let r = __cpuid(1);
    kprintln!("familia {}, modelo {}, revisao {}; APIC local: {}",
              (r.eax >> 8) & 0xF, (r.eax >> 4) & 0xF, r.eax & 0xF,
              if r.edx & (1 << 9) != 0 { "sim" } else { "nao" });
}

fn int(_: &[&str]) {
    kprintln!("timer:    {} tiques", timer::tiques());
    kprintln!("teclado:  {} scancodes recebidos, {} perdidos (fila cheia)",
              teclado::recebidos(), teclado::perdidos());
    kprintln!("espurias: {}", interrupcoes::espurias());
}

fn traduzir(argumentos: &[&str]) {
    match argumentos.first().and_then(|a| ler_numero(a)) {
        Some(endereco) => crate::explicar_traducao("endereco", endereco),
        None => kprintln!("uso: traduzir <endereco>   (ex.: traduzir 0xffff900000000000)"),
    }
}

fn ler(argumentos: &[&str]) {
    let Some(endereco) = argumentos.first().and_then(|a| ler_numero(a)) else {
        kprintln!("uso: ler <endereco>");
        return;
    };
    // Volátil e sem conferir nada antes: o objetivo é poder ver um page
    // fault de propósito (`ler 0`). O kernel ainda não se recupera dele.
    let valor = unsafe { (endereco as *const u64).read_volatile() };
    kprintln!("[{:#x}] = {:#018x}", endereco, valor);
}

fn reiniciar(_: &[&str]) {
    kprintln!("reiniciando...");
    cpu::reiniciar();
}

fn desligar(_: &[&str]) {
    kprintln!("desligando...");
    crate::acpi::desligar();
    kprintln!("nao consegui desligar por ACPI");
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn ler_numero_decimal_e_hexadecimal() {
        assert_eq!(ler_numero("42"), Some(42));
        assert_eq!(ler_numero("0x2a"), Some(42));
        assert_eq!(ler_numero("0xffff_9000_0000_0000"), Some(0xFFFF_9000_0000_0000));
        assert_eq!(ler_numero("0xzz"), None);
        assert_eq!(ler_numero("abc"), None);
        assert_eq!(ler_numero(""), None);
    }

    #[test_case]
    fn editar_linha() {
        let mut s = Shell::new();
        for c in "ecx".chars() { s.tecla(c); }
        s.tecla('\x08');                 // apaga o x
        s.tecla('o');
        assert_eq!(s.linha, "eco");
        for _ in 0..10 { s.tecla('\x08'); } // mais backspaces que letras
        assert_eq!(s.linha, "");
        s.tecla('\t');                    // ignorado
        assert_eq!(s.linha, "");
    }

    #[test_case]
    fn linha_tem_limite() {
        let mut s = Shell::new();
        for _ in 0..(MAX_LINHA + 10) { s.tecla('a'); }
        assert_eq!(s.linha.len(), MAX_LINHA);
    }

    #[test_case]
    fn nomes_de_comandos_unicos() {
        for (i, a) in COMANDOS.iter().enumerate() {
            assert!(COMANDOS[i + 1..].iter().all(|b| b.nome != a.nome), "comando repetido: {}", a.nome);
            assert!(a.uso.starts_with(a.nome));
        }
    }
}
