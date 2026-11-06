//! Console de texto desenhado pixel a pixel no framebuffer (GOP).
//!
//! Depois do ExitBootServices não existe mais `println!` do UEFI: para
//! mostrar texto na tela, o kernel precisa desenhar cada letra sozinho.
//! Usamos uma fonte bitmap 8x8 (cada caractere = 8 bytes, 1 bit por pixel).

use core::fmt;
use font8x8::legacy::BASIC_LEGACY;

/// Informações do vídeo capturadas enquanto o UEFI ainda estava ativo.
#[derive(Clone, Copy)]
pub struct FrameBuffer {
    pub base: *mut u32,
    pub width: usize,
    pub height: usize,
    pub stride: usize, // pixels por linha na memória (pode ser > width)
}

// O framebuffer é memória física única; só acessamos via o Mutex do CONSOLE.
unsafe impl Send for FrameBuffer {}

const ESCALA: usize = 2; // cada pixel da fonte vira um bloco 2x2
const CHAR_W: usize = 8 * ESCALA;
const CHAR_H: usize = 8 * ESCALA + 4; // +4px de espaço entre linhas

pub struct Console {
    fb: FrameBuffer,
    col: usize,
    row: usize,
    pub cor: u32,
    pub fundo: u32,
}

impl Console {
    pub fn new(fb: FrameBuffer) -> Self {
        let mut c = Console { fb, col: 0, row: 0, cor: 0x00FF_FFFF, fundo: 0x0010_1830 };
        c.limpar();
        c
    }

    fn cols(&self) -> usize { self.fb.width / CHAR_W }
    fn rows(&self) -> usize { self.fb.height / CHAR_H }

    fn pixel(&mut self, x: usize, y: usize, cor: u32) {
        if x < self.fb.width && y < self.fb.height {
            unsafe { self.fb.base.add(y * self.fb.stride + x).write_volatile(cor) };
        }
    }

    pub fn limpar(&mut self) {
        for y in 0..self.fb.height {
            for x in 0..self.fb.width {
                self.pixel(x, y, self.fundo);
            }
        }
        self.col = 0;
        self.row = 0;
    }

    fn desenhar_char(&mut self, c: u8, col: usize, row: usize) {
        let glifo = BASIC_LEGACY[(c & 0x7F) as usize];
        for (gy, linha) in glifo.iter().enumerate() {
            for gx in 0..8 {
                // bit 0 = pixel mais à esquerda
                let cor = if linha & (1 << gx) != 0 { self.cor } else { self.fundo };
                for dy in 0..ESCALA {
                    for dx in 0..ESCALA {
                        self.pixel(col * CHAR_W + gx * ESCALA + dx,
                                   row * CHAR_H + gy * ESCALA + dy, cor);
                    }
                }
            }
        }
    }

    /// Sobe todo o texto uma linha quando a tela enche.
    fn rolar(&mut self) {
        let linhas_px = CHAR_H * (self.rows() - 1);
        for y in 0..linhas_px {
            unsafe {
                let dst = self.fb.base.add(y * self.fb.stride);
                let src = self.fb.base.add((y + CHAR_H) * self.fb.stride);
                core::ptr::copy(src, dst, self.fb.width);
            }
        }
        for y in linhas_px..self.fb.height {
            for x in 0..self.fb.width {
                self.pixel(x, y, self.fundo);
            }
        }
        self.row -= 1;
    }

    fn nova_linha(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= self.rows() {
            self.rolar();
        }
    }

    pub fn write_byte(&mut self, b: u8) {
        match b {
            b'\n' => self.nova_linha(),
            b'\r' => self.col = 0,
            _ => {
                if self.col >= self.cols() {
                    self.nova_linha();
                }
                self.desenhar_char(b, self.col, self.row);
                self.col += 1;
            }
        }
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            // A fonte só tem ASCII; acentos viram '?'
            self.write_byte(if c.is_ascii() { c as u8 } else { b'?' });
        }
        Ok(())
    }
}
