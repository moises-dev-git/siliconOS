# 14: `syscall` e `sysret`

> **Você vai entender:** por que `int 0x80` é um jeito caro de pedir algo
> ao kernel; o que as instruções **`syscall`** e **`sysret`** fazem (e o que
> elas **não** fazem, como trocar de pilha); como configurá-las com quatro
> **MSRs**; por que a GDT do doc 03 tem os dados de usuário antes do código;
> e quanto se ganha, medido em ciclos na CPU de verdade.
>
> **Arquivos:** `src/usuario/syscall.rs` (novo: MSRs e a entrada), `src/cpu.rs`
> (`escrever_msr`), `src/gdt.rs` (`RSP0_ATUAL`), `src/main.rs` (chama o
> `init`), `programas/src/lib.rs` (a biblioteca `sos` passa a usar
> `syscall`), `programas/src/bin/medir.rs` (novo: o programa que mede)
> **Livro:** capítulo 11, seção 11.4 (o livro recomenda começar por uma
> interrupção de software e trocar depois; a implementação é nossa)
> **Testar:** `./run.sh`, depois `rodar medir` e `rodar oi`;
> `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 03 (GDT), doc 10 (chamadas de sistema por `int 0x80`)
> e doc 12 (pilha do kernel por processo)
> **Rust novo aqui:** registradores descartados no `asm!` e
> `#[inline(always)]` ([§44](rust-basico.md#44-asm-registradores-descartados-e-inlinealways))
>
> **Código deste capítulo:** [estado no fim do doc 14](https://github.com/moises-dev-git/siliconOS/tree/doc-14) · [o que mudou desde o doc 13](https://github.com/moises-dev-git/siliconOS/compare/doc-13...doc-14) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

Desde o doc 10, um programa pede algo ao kernel com `int 0x80`: uma
**interrupção de software**. Funciona, e foi a escolha certa para começar
(o livro recomenda isso: o mecanismo de interrupções já existia e era
conhecido). Mas uma interrupção é um caminho **geral**, feito para tudo
(exceções, hardware, chamadas), e a CPU faz muito trabalho a cada uma:

1. lê a entrada 0x80 da IDT na memória (16 bytes) e confere se é válida;
2. confere o DPL da entrada contra o anel atual (é o que permite o `int
   0x80` do anel 3, doc 10);
3. lê o novo CS da entrada e o descritor dele na GDT;
4. como o anel muda, lê o RSP0 da TSS e troca de pilha;
5. empilha 5 valores (SS, RSP, RFLAGS, CS, RIP);
6. na volta, `iretq` desempilha os 5 e refaz várias dessas conferências.

Um programa que imprime faz uma chamada de sistema por pedaço de texto
formatado (doc 13). Num sistema de verdade, são milhões de chamadas por
segundo: o custo de cada uma importa.

## 2. A ideia: uma porta feita só para isso

As CPUs x86-64 têm duas instruções feitas **só** para chamadas de sistema:
`syscall` (entrar) e `sysret` (sair). Elas vêm da AMD, que as definiu como
o jeito padrão de chamar o kernel no modo de 64 bits; a Intel tinha a sua
dupla, `sysenter`/`sysexit`, e também implementa `syscall`/`sysret` no
modo de 64 bits.

A diferença está no que elas **deixam de fazer**. `syscall`:

- **não lê a IDT, a GDT nem a TSS na memória:** o endereço de entrada e os
  seletores vêm de registradores de configuração (MSRs), fixados uma vez
  no boot;
- **não confere privilégios:** só existe um destino possível, escolhido
  pelo kernel;
- **não empilha nada:** guarda o RIP de volta em **RCX** e o RFLAGS em
  **R11**;
- **não troca de pilha.** A entrada começa rodando com o RSP do programa.

Em detalhe, `syscall` faz:

```
  RCX    ← RIP           (a instrução depois do syscall: para onde voltar)
  R11    ← RFLAGS
  RFLAGS ← RFLAGS sem os bits do MSR FMASK
  CS     ← STAR[47:32]          SS ← STAR[47:32] + 8      (anel 0)
  RIP    ← LSTAR
```

e `sysret` (a versão de 64 bits, `sysretq`), o inverso:

```
  RIP    ← RCX
  RFLAGS ← R11
  CS     ← STAR[63:48] + 16 (RPL 3)    SS ← STAR[63:48] + 8 (RPL 3)
```

Repare: RCX e R11 do programa **se perdem** (a CPU os usa). Isso vira parte
do contrato: depois de `syscall`, o programa não pode contar com RCX e R11.

## 3. Configurar: quatro MSRs

Um **MSR** (*model-specific register*, doc 08) é um registrador de
configuração identificado por número, lido com `rdmsr` e escrito com
`wrmsr`. Até aqui o kernel só lia (o APIC, no doc 08); agora precisa
escrever:

```rust
/// Escreve um MSR (`wrmsr`: número em ECX, valor em EDX:EAX). Usado para
/// configurar `syscall`/`sysret` (doc 14).
pub fn escrever_msr(numero: u32, valor: u64) {
    unsafe {
        asm!("wrmsr", in("ecx") numero, in("edx") (valor >> 32) as u32, in("eax") valor as u32,
             options(nostack, preserves_flags));
    }
}
```

Os quatro que importam:

```rust
pub const IA32_EFER: u32 = 0xC000_0080;
pub const IA32_STAR: u32 = 0xC000_0081;
pub const IA32_LSTAR: u32 = 0xC000_0082;
pub const IA32_FMASK: u32 = 0xC000_0084;

/// EFER.SCE (System Call Enable): sem ele, `syscall` é instrução inválida (#UD).
pub const EFER_SCE: u64 = 1 << 0;

/// Bits do RFLAGS que `syscall` desliga ao entrar: IF (interrupções), TF
/// (passo a passo do depurador) e DF (direção das instruções de string).
pub const FMASK: u64 = (1 << 9) | (1 << 8) | (1 << 10);

/// Liga `syscall`/`sysret` e aponta a CPU para a nossa entrada.
pub fn init() {
    // STAR: bits 32–47 = seletor de código do kernel (SS do kernel = esse + 8);
    // bits 48–63 = base para o sysret de 64 bits: CS = base + 16, SS = base + 8,
    // ambos com RPL 3. Com a GDT do doc 03 (0x08 código, 0x10 dados do kernel,
    // 0x18 dados e 0x20 código do usuário), a base é 0x10:
    //   syscall → CS = 0x08, SS = 0x10;   sysret → CS = 0x20|3 = 0x23, SS = 0x18|3 = 0x1B.
    let base_sysret = (gdt::KERNEL_DATA as u64) | 3; // 0x13
    let star = (base_sysret << 48) | ((gdt::KERNEL_CODE as u64) << 32);
    cpu::escrever_msr(IA32_STAR, star);
    cpu::escrever_msr(IA32_LSTAR, entrada_syscall_rapida as *const () as u64);
    cpu::escrever_msr(IA32_FMASK, FMASK);
    cpu::escrever_msr(IA32_EFER, cpu::ler_msr(IA32_EFER) | EFER_SCE);
}
```

- **EFER.SCE** liga as instruções. (O EFER é o mesmo MSR em que o UEFI já
  tinha ligado o modo de 64 bits e o NXE, doc 06: por isso o `| EFER_SCE`
  sobre o valor lido, sem apagar o resto.)
- **LSTAR** é o endereço da entrada (seção 4).
- **FMASK**: quais bits do RFLAGS desligar ao entrar. O mais importante é
  o **IF**: a entrada começa na pilha do programa e precisa trocar de pilha
  antes que qualquer interrupção chegue (seção 4). TF e DF: para o kernel
  não entrar em modo passo a passo nem com as instruções de string ao
  contrário só porque o programa os ligou.
- **STAR** guarda dois seletores numa conta que exige uma ordem certa na
  GDT.

### 3.1 A promessa do doc 03

O `sysret` não lê CS e SS da memória: calcula os dois a partir de **um**
número, `STAR[63:48]`, somando 8 para o SS e 16 para o CS. Isso só funciona
se, na GDT, os **dados** de usuário estiverem logo antes do **código** de
usuário. O doc 03 montou a GDT nessa ordem e avisou que era por causa do
`syscall`/`sysret` "no futuro":

```
  GDT do SiliconOS (doc 03)          a conta do STAR

  0x00  nula
  0x08  código do kernel   ◄── STAR[47:32] = 0x08      syscall: CS = 0x08
  0x10  dados do kernel    ◄── STAR[63:48] = 0x10|3    syscall: SS = 0x08 + 8 = 0x10
  0x18  dados do usuário   ◄── base + 8                sysret:  SS = 0x18 | 3 = 0x1B
  0x20  código do usuário  ◄── base + 16               sysret:  CS = 0x20 | 3 = 0x23
  0x28  TSS
```

Os valores de CS e SS são exatamente os que o `iretq` usava (0x23 e 0x1B,
doc 10): o programa não percebe diferença. (Por que "+16" para o código? Na
época, o `sysret` também sabia voltar para programas de 32 bits, com CS =
base, e a entrada de 64 bits ficou depois dela.)

## 4. A entrada: trocar de pilha sozinha

Esse é o ponto delicado. Quando `syscall` pula para `entrada_syscall_rapida`,
o RSP ainda é **o do programa**. Usar a pilha do programa no kernel seria
um erro grave: o programa escolhe o RSP, e poderia apontá-lo para memória
do kernel, para um endereço sem mapeamento ou para outra coisa sua (o
`int 0x80` não tem esse problema porque a CPU troca de pilha pela TSS, doc
10).

A entrada precisa então trocar de pilha **antes de usar a pilha para
qualquer coisa**. Para onde? Para a pilha do kernel do processo atual, a
mesma que o escalonador põe no TSS.RSP0 (doc 12). Como o assembly não
alcança a TSS facilmente (ela mora numa `UnsafeCell` dentro de um `Once`),
`definir_rsp0` passou a guardar uma cópia num `static` com nome:

```rust
pub fn definir_rsp0(topo: u64) {
    let tss = TSS.get().expect("gdt::init antes de definir_rsp0");
    unsafe {
        (&raw mut (*tss.0.get()).rsp as *mut u64).write_unaligned(topo);
        RSP0_ATUAL = topo;
    }
}

/// Cópia de TSS.rsp[0] num lugar que o assembly alcança pelo nome (doc 14):
/// a instrução `syscall`, ao contrário de `int`, NÃO troca de pilha, e a
/// entrada de `syscall` lê daqui a pilha do kernel do processo atual.
pub static mut RSP0_ATUAL: u64 = 0;
```

(e `gdt::init` põe ali o valor inicial, o topo da `PILHA_ANEL0`.)

A entrada inteira:

```rust
static mut RSP_DO_PROGRAMA: u64 = 0;

#[unsafe(naked)]
pub unsafe extern "sysv64" fn entrada_syscall_rapida() {
    naked_asm!(
        // 1. Trocar para a pilha do kernel do processo atual (a mesma do TSS.RSP0).
        "mov [rip + {rsp_programa}], rsp",
        "mov rsp, [rip + {rsp0}]",
        // 2. O quadro que o `int` empilharia: SS, RSP, RFLAGS (em R11), CS, RIP (em RCX).
        "push {ss}",
        "push qword ptr [rip + {rsp_programa}]",
        "push r11",
        "push {cs}",
        "push rcx",
        // 3. Os registradores, na mesma ordem da entrada_syscall (struct Registros).
        "push rax", "push rcx", "push rdx", "push rbx", "push rbp", "push rsi", "push rdi",
        "push r8", "push r9", "push r10", "push r11", "push r12", "push r13", "push r14", "push r15",
        "mov rdi, rsp",          // 1º argumento: &mut Registros
        "call {tratar}",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop r11", "pop r10", "pop r9", "pop r8",
        "pop rdi", "pop rsi", "pop rbp", "pop rbx", "pop rdx", "pop rcx", "pop rax",
        // 4. Voltar: RIP vai em RCX e RFLAGS em R11 (é de lá que o sysret os
        //    lê); o RSP do programa por último, porque depois dele a pilha do
        //    kernel deixa de estar à mão.
        "mov rcx, [rsp]",        // RIP
        "mov r11, [rsp + 16]",   // RFLAGS
        "mov rsp, [rsp + 24]",   // RSP do programa
        "sysretq",
        rsp_programa = sym RSP_DO_PROGRAMA,
        rsp0 = sym gdt::RSP0_ATUAL,
        ss = const gdt::USER_DATA as u64,
        cs = const gdt::USER_CODE as u64,
        tratar = sym super::tratar_syscall,
    )
}
```

Passo a passo:

1. **Trocar de pilha.** O RSP do programa vai para `RSP_DO_PROGRAMA` (um
   `static`, porque ainda não há pilha confiável onde guardá-lo), e o RSP
   passa a ser o topo da pilha do kernel do processo. Um `static` só basta
   porque há uma CPU e as interrupções estão desligadas (o FMASK desligou o
   IF): ninguém mais passa por aqui entre essas duas instruções.
2. **Fingir o quadro do `int`.** Em vez de inventar um formato novo, a
   entrada empilha **o mesmo** que a CPU empilha num `int 0x80`: SS e CS de
   usuário (constantes), o RSP do programa, o RFLAGS (que `syscall` deixou
   em R11) e o RIP de volta (em RCX).
3. **Os mesmos registradores, na mesma ordem.** Com isso, a pilha fica
   idêntica à da `entrada_syscall` do doc 10, e a struct `Registros` e a
   função `tratar_syscall` servem para as duas portas **sem mudar uma
   linha**. As chamadas `sair`, `escrever`, `tiques` e `pid`, a validação
   de ponteiros, o `encerrar` do escalonador: tudo igual.
4. **Voltar com `sysretq`.** Depois de restaurar os registradores, RCX
   recebe o RIP de volta e R11 o RFLAGS (é de lá que `sysret` os lê), e o
   RSP volta a ser o do programa. A ordem importa: enquanto o RSP aponta
   para a pilha do kernel, dá para ler `[rsp]`, `[rsp + 16]` e
   `[rsp + 24]`; depois do último `mov`, não dá mais.

```
  pilha do kernel do processo (topo = RSP0_ATUAL)
  ┌────────────────────┐
  │ SS = 0x1B          │  ┐
  │ RSP do programa    │  │ o "quadro do int",
  │ RFLAGS (era R11)   │  │ montado à mão
  │ CS = 0x23          │  │
  │ RIP (era RCX)      │  ┘ ◄── [rsp] na volta
  │ rax rcx rdx ...    │  ┐ struct Registros:
  │ ... r14 r15        │  ┘ o mesmo do int 0x80
  └────────────────────┘ ◄── rsp ao chamar tratar_syscall
```

E se a chamada for `sair`? `tratar_syscall` chama `escalonador::encerrar`,
que nunca volta (doc 12): o processo fica `Terminado`, e a pilha do kernel
dele, com este quadro, é liberada depois. Igual ao `int 0x80`.

> **O RIP de volta é sempre canônico?** `sysret` em CPUs Intel tem uma
> armadilha famosa: se o RCX tiver um endereço não canônico (doc 04), a
> exceção #GP acontece **já no anel 0, com o RSP do programa**, e o
> programa ganha o controle da pilha do kernel. Em 2012, isso afetou
> vários sistemas ao mesmo tempo. O SiliconOS escapa porque o RCX na volta
> é exatamente o que `syscall` guardou (a instrução seguinte à do
> programa), e nenhuma chamada de sistema o altera. Um kernel que permitisse
> mudar o RIP de volta (para sinais, por exemplo) teria que conferir antes.

## 5. Os programas passam a usar `syscall`

Os programas embutidos em assembly (docs 10 e 12) continuam com `int 0x80`:
as duas portas levam ao mesmo `tratar_syscall`. A biblioteca `sos` dos
programas em Rust (doc 13) trocou de porta, e com isso todos os ELF trocam
juntos, sem mexer neles:

```rust
/// Uma chamada de sistema com até dois argumentos, pela instrução
/// `syscall` (doc 14). Número em RAX, argumentos em RDI e RSI, resultado em
/// RAX. A própria CPU usa RCX (endereço de volta) e R11 (RFLAGS): o
/// compilador precisa saber que os dois mudam (`lateout(...) _`).
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}

/// Termina o programa com o código dado. Não volta.
pub fn sair(codigo: u64) -> ! {
    chamada(SYS_SAIR, codigo, 0);
    unreachable!("o kernel voltou de sair()")
}

/// Escreve os bytes no console. Devolve quantos foram escritos, ou um erro
/// negativo (-14 se o kernel recusar o ponteiro).
pub fn escrever(bytes: &[u8]) -> i64 {
    chamada(SYS_ESCREVER, bytes.as_ptr() as u64, bytes.len() as u64) as i64
}

/// Tiques do timer desde o boot (100 por segundo).
pub fn tiques() -> u64 {
    chamada(SYS_TIQUES, 0, 0)
}

/// O número deste processo.
pub fn pid() -> u64 {
    chamada(SYS_PID, 0, 0)
}
```

- **`lateout("rcx") _, lateout("r11") _`**: "estes dois registradores saem
  daqui com lixo" ([§44](rust-basico.md#44-asm-registradores-descartados-e-inlinealways)).
  Sem isso, o compilador poderia deixar um valor importante em RCX antes da
  chamada e contar com ele depois. Com `int 0x80` não era preciso, porque
  o kernel restaura todos os registradores.
- **`sair`** não pode mais usar `options(noreturn)` diretamente (a chamada
  passa por `chamada`, que devolve um valor); o `unreachable!` diz ao
  compilador que dali não se passa, e, se um dia o kernel voltar, o
  programa dá panic em vez de seguir rodando lixo.
- Tudo o mais na `sos` (`print!`, `println!`, `principal!`, panic) continua
  igual.

## 6. Quanto se ganha: o programa `medir`

Um programa novo no disco em RAM mede o custo das duas portas, chamando a
mesma chamada de sistema (`tiques`, a mais barata) 100 000 vezes por cada
uma e contando **ciclos da CPU** com a instrução `rdtsc` (*read time-stamp
counter*), que o anel 3 também pode usar:

```rust
//! Mede quanto custa uma chamada de sistema: `int 0x80` (doc 10) contra
//! `syscall` (doc 14), contando ciclos da CPU com `rdtsc`.
#![no_std]
#![no_main]

use core::arch::asm;
use sos::{println, principal};

principal!(main);

const VEZES: u64 = 100_000;

/// O contador de ciclos da CPU (Time Stamp Counter).
fn ciclos() -> u64 {
    let (alto, baixo): (u32, u32);
    unsafe { asm!("rdtsc", out("edx") alto, out("eax") baixo, options(nomem, nostack)) };
    (alto as u64) << 32 | baixo as u64
}

/// A mesma chamada `tiques` (número 2), pelo caminho antigo.
fn tiques_int80() -> u64 {
    let r: u64;
    unsafe { asm!("int 0x80", inlateout("rax") 2u64 => r, options(nostack)) };
    r
}

/// Ciclos médios por chamada de `f`.
fn medir(f: impl Fn() -> u64) -> u64 {
    f(); // a primeira vez carrega caches e TLB
    let inicio = ciclos();
    for _ in 0..VEZES {
        f();
    }
    (ciclos() - inicio) / VEZES
}

fn main() -> u64 {
    let int80 = medir(tiques_int80);
    let syscall = medir(sos::tiques);
    println!("{} chamadas de cada: int 0x80 = {} ciclos, syscall = {} ciclos por chamada",
             VEZES, int80, syscall);
    0
}
```

O trabalho dentro do kernel é o mesmo nas duas medições (ler o contador de
tiques); a diferença é só a porta. Os tiques do timer que caem no meio
(a cada 10 ms) entram nas duas contas igualmente.

Resultados reais, três vezes cada, em dois modos do QEMU:

```
== TCG (o QEMU emula a CPU instrução por instrução)
100000 chamadas de cada: int 0x80 = 1751 ciclos, syscall = 748 ciclos por chamada
100000 chamadas de cada: int 0x80 = 1691 ciclos, syscall = 784 ciclos por chamada
100000 chamadas de cada: int 0x80 = 1712 ciclos, syscall = 803 ciclos por chamada

== KVM (a CPU de verdade roda o código; QEMU com -enable-kvm -cpu host)
100000 chamadas de cada: int 0x80 = 843 ciclos, syscall = 219 ciclos por chamada
100000 chamadas de cada: int 0x80 = 887 ciclos, syscall = 229 ciclos por chamada
100000 chamadas de cada: int 0x80 = 858 ciclos, syscall = 265 ciclos por chamada
```

(A CPU do teste com KVM, segundo o comando `cpu` do shell: Intel Core
i5-14450HX.)

- **Na CPU de verdade, `syscall` custa uns 230 ciclos e `int 0x80` uns
  860:** quase 4 vezes menos. Esses números incluem tudo: as duas
  instruções de ida e volta, as nossas 15 instruções `push` e 15 `pop`, a
  chamada a `tratar_syscall` e o próprio laço.
- **No TCG, a diferença cai para 2,2 vezes**, e os números não dizem nada
  sobre hardware: ali, cada instrução é emulada por software, e o custo
  depende de como o QEMU implementa cada uma.

## 7. Como foi testado

### 7.1 Pelo shell

Além do `medir` (seção 6), os programas de antes:

```
silicon> rodar oi
Oi! Sou um programa em Rust, carregado de um arquivo ELF. Meu pid e 4.
'oi' terminou: codigo de saida 0
```

`oi`, `primos` e `dados` agora entram no kernel só por `syscall`; `ola`,
`contador` e os outros embutidos, só por `int 0x80`; `medir`, pelas duas.

### 7.2 `cargo test`

48 testes, todos `[ok]`: os 46 de antes e 2 novos em `syscall.rs`:

```rust
#[test_case]
fn msrs_configurados() {
    assert!(cpu::ler_msr(IA32_EFER) & EFER_SCE != 0);
    assert_eq!(cpu::ler_msr(IA32_LSTAR), entrada_syscall_rapida as *const () as u64);
    assert_eq!(cpu::ler_msr(IA32_STAR) >> 32, 0x0013_0008);
    assert_eq!(cpu::ler_msr(IA32_FMASK), 0x700);
}

#[test_case]
fn as_duas_portas_funcionam_no_mesmo_programa() {
    // `medir` chama `tiques` 100 000 vezes por `int 0x80` e 100 000 por
    // `syscall`, e imprime pela `syscall`.
    let arquivo = crate::ramdisk::abrir("medir").unwrap();
    let pid = super::super::iniciar_elf(arquivo.nome, arquivo.dados, false).unwrap();
    assert_eq!(crate::escalonador::esperar(pid), super::super::Resultado::Saiu(0));
}
```

Dois testes do doc 13 precisaram de ajuste, e os dois ensinam algo:

- **`os_tres_programas_estao_no_disco`** virou `os_programas_estao_no_disco`
  e espera `["dados", "medir", "oi", "primos"]`: o disco ganhou um arquivo.
- **O caso "ponto de entrada fora do código"** de
  `arquivos_ruins_recusados_sem_vazar` usava o endereço fixo
  `0x2000_0000_1000`, que no doc 13 era a página das constantes. Com a
  troca para `syscall` (e o `unreachable!` em `sair`, que traz junto o
  código de panic), o código de `oi` passou de 4 KiB, e esse endereço caiu
  **dentro do código**, que é executável: o teste recebeu `Ok` em vez de
  `EntradaInvalida`. O teste agora lê do próprio arquivo o endereço do
  segundo segmento (as constantes, R--), e não depende mais do leiaute:

  ```rust
        // Ponto de entrada fora do código: o endereço do segundo segmento (as
        // constantes, R--), lido do próprio arquivo.
        assert_eq!(carregar_alterado(|d| { let p = ph(d) + 56 + 16; let v: [u8; 8] = d[p..p + 8].try_into().unwrap(); d[24..32].copy_from_slice(&v) }),
                   Err(ErroElf::EntradaInvalida));
  ```

### 7.3 E sem o EFER.SCE?

Para ver o papel de cada MSR, a linha que liga o EFER.SCE foi comentada (e
depois restaurada):

```
silicon> rodar oi
[kernel] OPCODE INVALIDO no programa (RIP=0x200000000091, CS=0x23): #UD
'oi' encerrado pelo kernel: OPCODE INVALIDO em 0x200000000091
silicon> rodar ola
Ola do anel 3! (escrito por uma chamada de sistema)
'ola' terminou: codigo de saida 42
```

Desmontando `oi` (`objdump -d`), em `0x2000_0000_0091` estão os bytes
`0f 05`: a instrução `syscall`, dentro da função `pid()` da `sos`. Com o
SCE desligado, a CPU trata `syscall` como uma instrução que não existe
(#UD, doc 04), e o kernel encerra só o programa (doc 10). O `ola`, que usa
`int 0x80`, nem percebe.

### 7.4 Regressão

As 21 builds com features de teste continuam passando.

## O que ainda falta

- **Uma CPU só.** Com várias, `RSP_DO_PROGRAMA` e `RSP0_ATUAL` teriam que
  ser um por CPU. Os kernels de verdade usam a instrução `swapgs` para
  trocar o registrador GS por um ponteiro para os dados da CPU atual logo
  na entrada, e guardam lá o RSP do programa e a pilha do kernel.
- **Interrupções não mascaráveis (NMI).** O FMASK desliga o IF, mas uma NMI
  ignora o IF. Se chegasse entre `syscall` e a troca de pilha, rodaria com
  o RSP do programa. O SiliconOS não usa NMI; quem usa dá a ela uma pilha
  própria pela IST (doc 03).
- **Mais argumentos.** A convenção do Linux usa RDI, RSI, RDX, R10, R8 e
  R9 (R10 no lugar do RCX, que o `syscall` usa); as nossas chamadas usam
  dois.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 13 (tag `doc-13`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-14`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-13...doc-14).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/usuario/syscall.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | **novo**: constantes dos MSRs, `init`, `RSP_DO_PROGRAMA`, `entrada_syscall_rapida`, 2 testes | — |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/mod.rs) | `pub mod syscall;` | depois de `pub mod programas;` |
| [`src/cpu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/cpu.rs) | `escrever_msr` | logo depois de `ler_msr` |
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/gdt.rs) | `RSP0_ATUAL`; `definir_rsp0` o atualiza; `init` põe o valor inicial | `RSP0_ATUAL` depois de `definir_rsp0`; em `init`, logo depois de `let tss = tss.0.get() ...` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/main.rs) | `usuario::syscall::init();` | logo depois de `escalonador::init();` |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/lib.rs) | `chamada` (com `syscall`); `sair`, `escrever`, `tiques` e `pid` passam a usá-la; comentário do topo | as quatro funções |
| [`programas/src/bin/medir.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | **novo** (seção 6) | — |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/ramdisk.rs) | testes: `os_programas_estao_no_disco` com 4 nomes; contagem 4 em `cabecalho_estragado_para_a_leitura` | testes |
| [`src/usuario/elf.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/elf.rs) | teste: entrada inválida lida do arquivo (seção 7.2) | `arquivos_ruins_recusados_sem_vazar` |

### Os trechos que o texto não mostrou

O começo de `syscall.rs`:

```rust
use core::arch::naked_asm;

use crate::cpu;
use crate::gdt;
```

e o começo dos testes:

```rust
#[cfg(test)]
mod testes {
    use super::*;

    // os 2 testes da seção 7.2
}
```

Em `gdt::init`, a linha que dá o valor inicial a `RSP0_ATUAL`:

```rust
    let tss = tss.0.get() as *const Tss;
    unsafe { RSP0_ATUAL = (&raw const (*tss).rsp as *const u64).read_unaligned() };
```

Em `main.rs`:

```rust
    escalonador::init();
    // `syscall`/`sysret` (docs/14-syscall-sysret.md): MSRs STAR, LSTAR, FMASK e EFER.SCE.
    usuario::syscall::init();
```

O novo comentário do topo de `programas/src/lib.rs`:

```rust
//! biblioteca do sistema (a libc no Linux). Aqui ela é este arquivo: cada
//! função é uma chamada de sistema. Até o doc 13, pela instrução `int 0x80`
//! (doc 10); desde o doc 14, pela instrução `syscall`, feita para isso.
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `init` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | Escreve STAR (seletores), LSTAR (entrada), FMASK (IF, TF, DF) e liga o EFER.SCE. |
| `entrada_syscall_rapida` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | (naked) Troca para a pilha do kernel do processo, monta o mesmo quadro e os mesmos registradores do `int 0x80`, chama `tratar_syscall` e volta com `sysretq`. |
| `escrever_msr` | [cpu.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/cpu.rs) | `wrmsr`: escreve um MSR (número em ECX, valor em EDX:EAX). |
| `chamada` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/lib.rs) | Faz uma chamada de sistema por `syscall`, avisando o compilador que RCX e R11 mudam. |
| `ciclos` | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | `rdtsc`: o contador de ciclos da CPU. |
| `tiques_int80` | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | A chamada `tiques` pela porta antiga, `int 0x80`. |
| `medir` | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | Ciclos médios por chamada de uma função, em 100 000 repetições. |
| `main` (medir) | [programas/src/bin/medir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/programas/src/bin/medir.rs) | Mede as duas portas e imprime os ciclos por chamada de cada uma. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `msrs_configurados` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | Teste: EFER.SCE ligado e STAR, LSTAR e FMASK com os valores esperados. |
| `as_duas_portas_funcionam_no_mesmo_programa` | [usuario/syscall.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-14/src/usuario/syscall.rs) | Teste: `medir` (que usa `int 0x80` e `syscall`) sai com 0. |

---

## Diferença em relação ao livro

O livro recomenda começar as chamadas de sistema por uma interrupção de
software, mais fácil de entender, e trocar por `syscall`/`sysret` depois que
o comportamento estiver certo: foi o que fizemos (doc 10 e este). Escolhas
nossas: manter as duas portas levando ao mesmo tratador (montando à mão o
mesmo quadro), a cópia do RSP0 num `static` em vez de `swapgs` (há uma CPU
só) e a medição com `rdtsc`.

## Resumo

- `int 0x80` passa pelo mecanismo geral de interrupções: IDT, conferência
  de privilégios, TSS, quadro de 5 valores. `syscall`/`sysret` pulam quase
  tudo: os destinos vêm de **MSRs**, o RIP de volta vai em **RCX** e o
  RFLAGS em **R11**.
- Configuração: **EFER.SCE** liga; **LSTAR** é a entrada; **FMASK**
  desliga IF, TF e DF; **STAR** dá os seletores, e a conta dele exige os
  dados de usuário antes do código de usuário na GDT (doc 03).
- `syscall` **não troca de pilha**: a entrada guarda o RSP do programa e
  carrega a pilha do kernel do processo (`RSP0_ATUAL`, a cópia do
  TSS.RSP0), com as interrupções desligadas.
- A entrada monta **o mesmo quadro e os mesmos registradores** do `int
  0x80`, e `tratar_syscall` serve às duas portas.
- O programa perde RCX e R11 a cada `syscall`: a `sos` avisa o compilador
  com `lateout(...) _`.
- Medido na CPU de verdade (KVM): **~230 ciclos contra ~860**, quase 4
  vezes menos.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **O que `int 0x80` faz que `syscall` não faz?**
   Lê a IDT e a GDT na memória, confere privilégios, troca de pilha pela
   TSS e empilha um quadro de 5 valores. `syscall` tira tudo de MSRs, guarda
   RIP e RFLAGS em RCX e R11 e não troca de pilha.

2. **Por que a entrada de `syscall` precisa trocar de pilha antes de tudo,
   e por que pode usar um `static` para guardar o RSP do programa?**
   Porque o RSP ainda é o do programa, que pode apontar para qualquer
   lugar. O `static` basta porque há uma CPU só e as interrupções estão
   desligadas (FMASK desligou o IF): ninguém mais passa por ali.

3. **Por que os dados de usuário vêm antes do código de usuário na GDT?**
   Porque `sysret` calcula os dois seletores a partir de um número só do
   STAR: SS = base + 8, CS = base + 16. Com 0x18 (dados) e 0x20 (código), a
   base é 0x10.

4. **Por que `tratar_syscall` não precisou mudar?**
   Porque a entrada nova monta na pilha exatamente o que a do `int 0x80`
   monta: o quadro (SS, RSP, RFLAGS, CS, RIP) e os 15 registradores na
   mesma ordem, que é a struct `Registros`.

5. **O que daria errado se a `sos` não declarasse `lateout("rcx") _` e
   `lateout("r11") _`?**
   O compilador poderia guardar um valor em RCX ou R11 antes da chamada e
   usá-lo depois, sem saber que `syscall` os sobrescreve (com o RIP de
   volta e o RFLAGS). O erro apareceria longe dali, como um valor errado.

6. **Sem o EFER.SCE, o que acontece com um programa que usa `syscall`? E
   com um que usa `int 0x80`?**
   O primeiro morre com #UD (instrução inválida) na própria instrução
   `syscall`; o kernel encerra só ele. O segundo funciona normalmente: a
   porta do `int 0x80` não depende do SCE.

</details>
