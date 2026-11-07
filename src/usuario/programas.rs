//! Os primeiros programas de usuário do SiliconOS: alguns bytes de código de
//! máquina, escritos em assembly, que o kernel copia para uma página do anel 3.
//!
//! Ainda não há executáveis nem sistema de arquivos, então cada programa é
//! um trecho entre dois rótulos (`*_inicio` e `*_fim`) dentro do próprio
//! kernel. Eles ficam na seção `.rdata` (dados somente leitura): o kernel
//! nunca os executa ali; só copia os bytes.
//!
//! Regras para um programa daqui:
//! - código independente de posição (endereços relativos ao RIP, `lea [rip + ...]`),
//!   porque roda num endereço diferente de onde foi montado;
//! - nada de chamar funções do kernel: a única porta é `int 0x80`.
//!
//! Chamadas de sistema (iguais às do Linux na escolha dos registradores):
//!   RAX = número; RDI, RSI = argumentos; resultado volta em RAX.
//!   0 sair(codigo)        1 escrever(ponteiro, tamanho)        2 tiques()

use core::arch::global_asm;

global_asm!(
    ".section .rdata",

    // ---- ola: escreve uma mensagem e sai com código 42 ----
    ".global programa_ola_inicio",
    ".global programa_ola_fim",
    "programa_ola_inicio:",
    "    lea rdi, [rip + ola_msg]", // ponteiro para a mensagem
    "    mov esi, offset OLA_TAM",  // tamanho (offset = o número em si, não "ler a memória ali")
    "    mov eax, 1",               // escrever
    "    int 0x80",
    "    mov edi, 42",              // código de saída
    "    xor eax, eax",             // sair
    "    int 0x80",
    "ola_msg: .ascii \"Ola do anel 3! (escrito por uma chamada de sistema)\\n\"",
    ".set OLA_TAM, . - ola_msg",   // "." = posição atual: fim - início da mensagem
    "programa_ola_fim:",

    // ---- invasor: tenta ler a memória do kernel ----
    ".global programa_invasor_inicio",
    ".global programa_invasor_fim",
    "programa_invasor_inicio:",
    "    movabs rax, 0xffff800000100000", // RAM física vista pelo kernel (doc 06)
    "    mov rax, [rax]",
    "    xor edi, edi",
    "    xor eax, eax",
    "    int 0x80",
    "programa_invasor_fim:",

    // ---- privilegiado: tenta desligar as interrupções ----
    ".global programa_privilegiado_inicio",
    ".global programa_privilegiado_fim",
    "programa_privilegiado_inicio:",
    "    cli",
    "    xor edi, edi",
    "    xor eax, eax",
    "    int 0x80",
    "programa_privilegiado_fim:",

    // ---- ponteiro: pede ao kernel para escrever a partir de um endereço do kernel ----
    ".global programa_ponteiro_inicio",
    ".global programa_ponteiro_fim",
    "programa_ponteiro_inicio:",
    "    movabs rdi, 0xffff900000000000", // o heap do kernel (doc 07)
    "    mov esi, 16",
    "    mov eax, 1",
    "    int 0x80",
    "    test rax, rax",             // negativo = o kernel recusou
    "    jns 4f",
    "    lea rdi, [rip + ponteiro_msg]",
    "    mov esi, offset PONTEIRO_TAM",
    "    mov eax, 1",
    "    int 0x80",
    "    mov edi, 1",
    "    xor eax, eax",
    "    int 0x80",
    "4:  xor edi, edi",
    "    xor eax, eax",
    "    int 0x80",
    "ponteiro_msg: .ascii \"o kernel recusou o ponteiro\\n\"",
    ".set PONTEIRO_TAM, . - ponteiro_msg",
    "programa_ponteiro_fim:",

    // ---- laco: nunca devolve a CPU ----
    ".global programa_laco_inicio",
    ".global programa_laco_fim",
    "programa_laco_inicio:",
    "2:  jmp 2b",
    "programa_laco_fim:",

    // ---- relogio: espera 1 s (100 tiques) perguntando ao kernel, e sai ----
    ".global programa_relogio_inicio",
    ".global programa_relogio_fim",
    "programa_relogio_inicio:",
    "    mov eax, 2",               // tiques()
    "    int 0x80",
    "    lea rbx, [rax + 100]",     // tique de chegada
    "2:  mov eax, 2",
    "    int 0x80",
    "    cmp rax, rbx",
    "    jb 2b",
    "    mov edi, 100",
    "    xor eax, eax",
    "    int 0x80",
    "programa_relogio_fim:",

    // ---- contador: escreve o próprio pid 5 vezes, esperando 50 tiques entre elas (doc 12) ----
    ".global programa_contador_inicio",
    ".global programa_contador_fim",
    "programa_contador_inicio:",
    "    mov eax, 3",               // pid()
    "    int 0x80",
    "    xor edx, edx",
    "    mov ecx, 10",
    "    div rcx",                  // rdx = pid % 10 (o último dígito)
    "    add dl, 0x30",             // vira o caractere ASCII ('0' = 0x30)
    "    sub rsp, 16",              // 2 bytes na pilha do programa: dígito e espaço
    "    mov byte ptr [rsp], dl",
    "    mov byte ptr [rsp + 1], 0x20",
    "    mov r12d, 5",              // 5 vezes (int 0x80 preserva tudo menos rax)
    "3:  mov rdi, rsp",
    "    mov esi, 2",
    "    mov eax, 1",               // escrever(rsp, 2)
    "    int 0x80",
    "    mov eax, 2",               // tiques()
    "    int 0x80",
    "    lea rbx, [rax + 50]",      // espera 50 tiques (500 ms)
    "4:  mov eax, 2",
    "    int 0x80",
    "    cmp rax, rbx",
    "    jb 4b",
    "    dec r12d",
    "    jnz 3b",
    "    xor edi, edi",             // sair(0)
    "    xor eax, eax",
    "    int 0x80",
    "programa_contador_fim:",

    ".text",
);

unsafe extern "C" {
    static programa_ola_inicio: u8;
    static programa_ola_fim: u8;
    static programa_invasor_inicio: u8;
    static programa_invasor_fim: u8;
    static programa_privilegiado_inicio: u8;
    static programa_privilegiado_fim: u8;
    static programa_ponteiro_inicio: u8;
    static programa_ponteiro_fim: u8;
    static programa_laco_inicio: u8;
    static programa_laco_fim: u8;
    static programa_relogio_inicio: u8;
    static programa_relogio_fim: u8;
    static programa_contador_inicio: u8;
    static programa_contador_fim: u8;
}

pub struct Programa {
    pub nome: &'static str,
    pub descricao: &'static str,
    inicio: *const u8,
    fim: *const u8,
}

// Os ponteiros apontam para dados fixos do executável: seguros entre CPUs.
unsafe impl Sync for Programa {}

impl Programa {
    /// Os bytes do código de máquina do programa.
    pub fn codigo(&self) -> &'static [u8] {
        unsafe { core::slice::from_raw_parts(self.inicio, self.fim as usize - self.inicio as usize) }
    }
}

pub static PROGRAMAS: [Programa; 7] = [
        Programa { nome: "ola", descricao: "escreve uma mensagem e sai com codigo 42",
                   inicio: &raw const programa_ola_inicio, fim: &raw const programa_ola_fim },
        Programa { nome: "relogio", descricao: "espera 1 s perguntando os tiques ao kernel",
                   inicio: &raw const programa_relogio_inicio, fim: &raw const programa_relogio_fim },
        Programa { nome: "ponteiro", descricao: "passa um endereco do kernel para escrever()",
                   inicio: &raw const programa_ponteiro_inicio, fim: &raw const programa_ponteiro_fim },
        Programa { nome: "invasor", descricao: "tenta ler a memoria do kernel",
                   inicio: &raw const programa_invasor_inicio, fim: &raw const programa_invasor_fim },
        Programa { nome: "privilegiado", descricao: "tenta executar cli",
                   inicio: &raw const programa_privilegiado_inicio, fim: &raw const programa_privilegiado_fim },
        Programa { nome: "laco", descricao: "laco infinito (o timer o encerra depois de 2 s de CPU)",
                   inicio: &raw const programa_laco_inicio, fim: &raw const programa_laco_fim },
        Programa { nome: "contador", descricao: "escreve o proprio pid 5 vezes, a cada 500 ms",
                   inicio: &raw const programa_contador_inicio, fim: &raw const programa_contador_fim },
];

pub fn procurar(nome: &str) -> Option<&'static Programa> {
    PROGRAMAS.iter().find(|p| p.nome == nome)
}
