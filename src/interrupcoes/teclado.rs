//! Teclado PS/2: IRQ 1, porta 0x60.
//!
//! Cada tecla apertada ou solta faz o controlador PS/2 (o "8042") colocar um
//! byte, o *scancode*, na porta 0x60 e disparar a IRQ 1. O scancode diz QUAL
//! TECLA (posição física), não qual letra: a tradução para caracteres é
//! trabalho nosso.
//!
//! Divisão do trabalho:
//! - o TRATADOR só lê o byte e o põe numa fila (rápido, sem lock, sem imprimir);
//! - o laço principal tira da fila, traduz e mostra na tela.

use core::sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering};

use super::apic;
use crate::escalonador::{self, Motivo};
use crate::idt::QuadroInterrupcao;
use crate::serial::{inb, outb};

const PORTA_DADOS: u16 = 0x60;
const PORTA_ESTADO: u16 = 0x64;   // leitura: estado
const PORTA_COMANDO: u16 = 0x64;  // escrita: comando para o controlador

const ESTADO_SAIDA_CHEIA: u8 = 1 << 0; // há um byte esperando em 0x60
const ESTADO_ENTRADA_CHEIA: u8 = 1 << 1; // o controlador ainda não leu o último comando

/// IRQ do teclado no barramento ISA.
pub const IRQ: u8 = 1;

// ---------------------------------------------------------------------------
// Fila sem lock: um produtor (o tratador) e um consumidor (o laço principal)
// ---------------------------------------------------------------------------

const TAMANHO_FILA: usize = 64;
static FILA: [AtomicU8; TAMANHO_FILA] = [const { AtomicU8::new(0) }; TAMANHO_FILA];
/// Próxima posição a ESCREVER (só o tratador muda).
static ESCRITA: AtomicUsize = AtomicUsize::new(0);
/// Próxima posição a LER (só o consumidor muda).
static LEITURA: AtomicUsize = AtomicUsize::new(0);
/// Scancodes jogados fora porque a fila estava cheia.
static PERDIDOS: AtomicU64 = AtomicU64::new(0);
static RECEBIDOS: AtomicU64 = AtomicU64::new(0);

pub fn recebidos() -> u64 { RECEBIDOS.load(Ordering::Relaxed) }
pub fn perdidos() -> u64 { PERDIDOS.load(Ordering::Relaxed) }

pub extern "x86-interrupt" fn tratador(_quadro: QuadroInterrupcao) {
    let scancode = unsafe { inb(PORTA_DADOS) };
    RECEBIDOS.fetch_add(1, Ordering::Relaxed);

    let e = ESCRITA.load(Ordering::Relaxed);
    let proxima = (e + 1) % TAMANHO_FILA;
    if proxima == LEITURA.load(Ordering::Acquire) {
        PERDIDOS.fetch_add(1, Ordering::Relaxed); // cheia: descarta
    } else {
        FILA[e].store(scancode, Ordering::Relaxed);
        // Release: quem ler o novo ESCRITA também vê o byte gravado acima.
        ESCRITA.store(proxima, Ordering::Release);
    }
    apic::eoi();

    // Acordar quem espera tecla (o shell, doc 15) e já dar a vez a ele:
    // sem isso, a tecla só apareceria no próximo tique (até 10 ms depois).
    if escalonador::acordar(Motivo::Teclado) > 0 {
        escalonador::ceder();
    }
}

/// Tira o próximo scancode da fila, se houver.
pub fn proximo_scancode() -> Option<u8> {
    let l = LEITURA.load(Ordering::Relaxed);
    if l == ESCRITA.load(Ordering::Acquire) {
        return None; // vazia
    }
    let scancode = FILA[l].load(Ordering::Relaxed);
    LEITURA.store((l + 1) % TAMANHO_FILA, Ordering::Release);
    Some(scancode)
}

/// Há scancode na fila? Desde o doc 15, quem espera tecla bloqueia com
/// `escalonador::esperar_ate(Motivo::Teclado, teclado::tem_scancode)`; o
/// tratador acorda. (Até o doc 14, `esperar()` fazia `sti; hlt` aqui.)
pub fn tem_scancode() -> bool {
    LEITURA.load(Ordering::Relaxed) != ESCRITA.load(Ordering::Acquire)
}

// ---------------------------------------------------------------------------
// Controlador PS/2
// ---------------------------------------------------------------------------

fn enviar_comando(comando: u8) {
    unsafe {
        while inb(PORTA_ESTADO) & ESTADO_ENTRADA_CHEIA != 0 {}
        outb(PORTA_COMANDO, comando);
    }
}

fn enviar_dado(dado: u8) {
    unsafe {
        while inb(PORTA_ESTADO) & ESTADO_ENTRADA_CHEIA != 0 {}
        outb(PORTA_DADOS, dado);
    }
}

fn ler_dado() -> u8 {
    unsafe {
        while inb(PORTA_ESTADO) & ESTADO_SAIDA_CHEIA == 0 {}
        inb(PORTA_DADOS)
    }
}

/// Byte de configuração do controlador PS/2.
const CONFIG_IRQ1: u8 = 1 << 0;      // gerar IRQ 1 quando chega byte do teclado
const CONFIG_TRADUCAO: u8 = 1 << 6;  // traduzir para o "scancode set 1"

/// Liga a IRQ 1 no controlador. Devolve (configuração antes, depois, bytes
/// velhos descartados).
pub fn init() -> (u8, u8, u32) {
    // Bytes que ficaram do firmware (teclas apertadas no boot) atrapalhariam.
    let mut descartados = 0;
    while unsafe { inb(PORTA_ESTADO) } & ESTADO_SAIDA_CHEIA != 0 {
        unsafe { inb(PORTA_DADOS) };
        descartados += 1;
    }
    enviar_comando(0x20); // "ler byte de configuração"
    let antes = ler_dado();
    let depois = antes | CONFIG_IRQ1 | CONFIG_TRADUCAO;
    enviar_comando(0x60); // "escrever byte de configuração"
    enviar_dado(depois);
    (antes, depois, descartados)
}

// ---------------------------------------------------------------------------
// Tradução: scancode set 1 -> caractere (layout americano)
// ---------------------------------------------------------------------------

/// Caractere de cada tecla, indexado pelo scancode (0x00-0x39).
/// 0 = tecla sem caractere (Ctrl, Shift, Alt...).
const NORMAL: &[u8; 58] =
    b"\0\x1b1234567890-=\x08\tqwertyuiop[]\n\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
const COM_SHIFT: &[u8; 58] =
    b"\0\x1b!@#$%^&*()_+\x08\tQWERTYUIOP{}\n\0ASDFGHJKL:\"~\0|ZXCVBNM<>?\0*\0 ";

const SHIFT_ESQUERDO: u8 = 0x2A;
const SHIFT_DIREITO: u8 = 0x36;
const CAPS_LOCK: u8 = 0x3A;
/// Bit 7 ligado = a tecla foi SOLTA ("break code").
const SOLTOU: u8 = 0x80;
/// Teclas "estendidas" (setas, Ctrl direito...) vêm com um byte 0xE0 antes.
const PREFIXO_ESTENDIDO: u8 = 0xE0;

pub struct Decodificador {
    shift_esquerdo: bool,
    shift_direito: bool,
    caps_lock: bool,
    estendido: bool,
}

impl Decodificador {
    pub const fn new() -> Self {
        Decodificador { shift_esquerdo: false, shift_direito: false, caps_lock: false, estendido: false }
    }

    /// Recebe um scancode; devolve o caractere, se a tecla produzir um.
    pub fn processar(&mut self, scancode: u8) -> Option<char> {
        if scancode == PREFIXO_ESTENDIDO {
            self.estendido = true;
            return None;
        }
        if core::mem::take(&mut self.estendido) {
            return None; // setas e afins: ainda não usamos
        }
        let soltou = scancode & SOLTOU != 0;
        let tecla = scancode & !SOLTOU;
        match tecla {
            SHIFT_ESQUERDO => { self.shift_esquerdo = !soltou; None }
            SHIFT_DIREITO => { self.shift_direito = !soltou; None }
            CAPS_LOCK => { if !soltou { self.caps_lock = !self.caps_lock; } None }
            _ if soltou => None,
            _ if (tecla as usize) < NORMAL.len() => {
                let shift = self.shift_esquerdo || self.shift_direito;
                let letra = NORMAL[tecla as usize].is_ascii_lowercase();
                // Caps Lock só inverte o Shift nas letras.
                let maiuscula = if letra { shift != self.caps_lock } else { shift };
                let c = if maiuscula { COM_SHIFT[tecla as usize] } else { NORMAL[tecla as usize] };
                if c == 0 { None } else { Some(c as char) }
            }
            _ => None, // F1-F12 e outras: ainda não usamos
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Passa uma sequência de scancodes e junta os caracteres produzidos.
    fn digitar(d: &mut Decodificador, scancodes: &[u8]) -> alloc::string::String {
        scancodes.iter().filter_map(|&s| d.processar(s)).collect()
    }

    #[test_case]
    fn letras_e_numeros() {
        let mut d = Decodificador::new();
        // a (aperta, solta), 1 (aperta, solta), espaço
        assert_eq!(digitar(&mut d, &[0x1E, 0x9E, 0x02, 0x82, 0x39, 0xB9]), "a1 ");
    }

    #[test_case]
    fn shift_e_caps_lock() {
        let mut d = Decodificador::new();
        // Shift esquerdo + a + 1, solta o Shift, a
        assert_eq!(digitar(&mut d, &[0x2A, 0x1E, 0x02, 0xAA, 0x1E]), "A!a");
        // Caps Lock liga: letra maiúscula, número continua número
        assert_eq!(digitar(&mut d, &[0x3A, 0xBA, 0x1E, 0x02]), "A1");
        // Caps Lock + Shift: letra volta a ser minúscula
        assert_eq!(digitar(&mut d, &[0x36, 0x1E, 0xB6]), "a");
        // Caps Lock desliga
        assert_eq!(digitar(&mut d, &[0x3A, 0xBA, 0x1E]), "a");
    }

    #[test_case]
    fn teclas_estendidas_ignoradas() {
        let mut d = Decodificador::new();
        // seta direita (E0 4D / E0 CD) não pode virar 'm'... nem nada
        assert_eq!(digitar(&mut d, &[0xE0, 0x4D, 0xE0, 0xCD, 0x1E]), "a");
    }

    #[test_case]
    fn enter_e_backspace() {
        let mut d = Decodificador::new();
        assert_eq!(digitar(&mut d, &[0x1C, 0x0E]), "\n\x08");
    }
}
