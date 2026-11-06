# 09: Shell, testes automáticos e depuração com GDB

> **Você vai entender:** como transformar o eco do teclado numa **linha de
> comando** de verdade; como o kernel descobre o modelo da CPU (`cpuid`) e a
> hora (RTC); como se reinicia e se **desliga** um PC por software; como
> fazer o `cargo test` funcionar **dentro do QEMU**, com um framework de
> testes próprio; e como parar o kernel no meio da execução com o **GDB**.
>
> **Arquivos:** `src/shell.rs` (o shell), `src/testes.rs` e `src/qemu.rs`
> (o framework de testes), `testar.sh` e `.cargo/config.toml` (o `cargo test`
> roda no QEMU), `depurar.sh` (QEMU + GDB), `reiniciar` em `src/cpu.rs`,
> `desligar` em `src/acpi.rs`, `hora_rtc` em `src/interrupcoes/timer.rs`
> **Livro:** capítulo 10
> **Testar:** `./run.sh` e digitar `ajuda`; `cargo test --target x86_64-unknown-uefi`;
> `cargo test --target x86_64-unknown-uefi --features teste-que-falha`; `./depurar.sh`
> **Leia antes:** doc 08 (teclado e timer)
> **Rust novo aqui:** ponteiros de função ([§31](rust-basico.md#31-ponteiros-de-função-fn)),
> `let ... else` e `split_first` ([§32](rust-basico.md#32-let--else-e-split_first)),
> `#[cfg(test)]` e o framework de testes próprio ([§33](rust-basico.md#33-testes-cfgtest-e-test_case)),
> `dyn Trait` e implementação para todo `T` ([§34](rust-basico.md#34-dyn-trait-e-implementar-para-todo-t))
>
> **Código deste capítulo:** [estado no fim do doc 09](https://github.com/moises-dev-git/siliconOS/tree/doc-09) · [o que mudou desde o doc 08](https://github.com/moises-dev-git/siliconOS/compare/doc-08...doc-09) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

No fim do doc 08, o SiliconOS sabia ler o teclado, mas só repetia na tela o
que era digitado. Faltam três coisas para ele ser usável no dia a dia (o
nosso dia a dia de desenvolvedores do kernel):

1. **Conversar com ele.** Até aqui, para ver o estado do heap ou testar uma
   tradução de endereço, era preciso escrever código, compilar e rodar. Um
   **shell** permite perguntar na hora: `mem`, `traduzir 0x...`.
2. **Testes que dão "passou" ou "falhou" sozinhos.** As 21 features de teste
   funcionam, mas cada uma exige ler a saída e decidir se está certa. O
   `cargo test` que você conhece (JUnit, pytest) faz isso automaticamente,
   e queremos o mesmo para o kernel, mesmo ele rodando dentro de um
   emulador.
3. **Um depurador.** Quando algo trava, imprimir mensagens nem sempre ajuda
   (no doc 08, o deadlock travou justamente o `kprintln!`). Precisamos poder
   parar a CPU e olhar onde ela está.

---

## 2. O shell

### 2.1 De tecla a linha

O doc 08 deixou um laço que traduzia scancodes e imprimia cada caractere. O
shell entra no meio: em vez de imprimir direto, cada caractere vai para
`Shell::tecla`, que monta a linha num `String` (doc 07):

```rust
// src/shell.rs
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
```

Três detalhes:

- **O eco é do shell, não do teclado.** Quem decide se o caractere aparece
  na tela é o shell: uma letra além do limite de 70 não aparece, e um
  Backspace com a linha vazia não apaga o `silicon> `. É assim também nos
  terminais de verdade: o programa (ou o terminal, em "modo cozido") decide
  o que ecoar.
- **`String::pop`** devolve `Option<char>`: `None` quando a linha está vazia.
  O `if ... is_some()` só manda o Backspace para a tela se havia algo para
  apagar.
- **`core::mem::take`** ([§30](rust-basico.md#30-receber-uma-closure-impl-fnonce))
  entrega a linha pronta ao `executar` e já deixa o shell com uma linha
  vazia para a próxima, sem copiar nada.

O laço principal, agora em `shell::rodar`, é o mesmo do doc 08 com o shell
no lugar do `kprint!`:

```rust
pub fn rodar() -> ! {
    let mut decodificador = teclado::Decodificador::new();
    let mut shell = Shell::new();
    kprintln!("Shell do SiliconOS. Digite 'ajuda' para ver os comandos.");
    kprint!("{}", PROMPT);
    loop {
        while let Some(scancode) = teclado::proximo_scancode() {
            if let Some(c) = decodificador.processar(scancode) {
                shell.tecla(c);
            }
        }
        teclado::esperar();
    }
}
```

Note **onde** o comando roda: no laço principal, fora de qualquer tratador
de interrupção. Um comando pode demorar o quanto quiser, imprimir, alocar;
o timer e o teclado continuam chegando normalmente (e as teclas digitadas
durante um comando lento esperam na fila do doc 08).

### 2.2 A tabela de comandos

Cada comando é uma linha numa tabela:

```rust
pub struct Comando {
    pub nome: &'static str,
    pub uso: &'static str,
    pub descricao: &'static str,
    pub funcao: fn(&[&str]),
}

pub const COMANDOS: &[Comando] = &[
    Comando { nome: "ajuda", uso: "ajuda", descricao: "lista os comandos", funcao: ajuda },
    Comando { nome: "eco", uso: "eco <texto>", descricao: "repete o texto", funcao: eco },
    // ...
];
```

O campo `funcao` é um **ponteiro de função** ([§31](rust-basico.md#31-ponteiros-de-função-fn)):
o endereço de uma função que recebe os argumentos já separados (`&[&str]`,
uma fatia de pedaços de texto). É a versão Rust de um
`Map<String, Consumer<List<String>>>` do Java, só que fixa na compilação e
sem alocar nada.

Executar uma linha é separar as palavras, achar o comando pelo nome e
chamar a função dele:

```rust
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
```

`split_first` divide a fatia em "primeiro elemento" e "o resto"
([§32](rust-basico.md#32-let--else-e-split_first)): `"traduzir 0x1000"` vira
`nome = "traduzir"` e `argumentos = ["0x1000"]`. Os `&str` de `palavras`
não são cópias: são pedaços da própria linha (o `split_whitespace` só
aponta para dentro dela).

A vantagem da tabela aparece no `ajuda`, que não precisa ser atualizado à
mão quando um comando novo entra:

```rust
fn ajuda(_: &[&str]) {
    for c in COMANDOS {
        kprintln!("  {:<22}{}", c.uso, c.descricao);
    }
}
```

### 2.3 Os comandos

```
silicon> ajuda
  ajuda                 lista os comandos
  eco <texto>           repete o texto
  limpar                limpa a tela
  tempo                 tempo ligado e hora do relogio CMOS
  mem                   quadros livres e estado do heap
  cpu                   fabricante e modelo (instrucao cpuid)
  int                   contadores de interrupcoes
  traduzir <endereco>   mostra a traducao de um endereco virtual
  ler <endereco>        le 8 bytes da memoria (pode dar page fault!)
  reiniciar             reinicia a maquina
  desligar              desliga a maquina (ACPI; testado so no QEMU)
```

A maioria só junta o que os capítulos anteriores já construíram:

| Comando | De onde vem |
|---|---|
| `mem` | `quadros::livres()` (doc 05) e `heap::estatisticas()` (doc 07) |
| `traduzir` | `explicar_traducao` (doc 06), a mesma do boot |
| `ler` | um `read_volatile` direto: `ler 0` mostra o page fault do doc 06 |
| `int` | os contadores do timer, do teclado e das espúrias (doc 08) |
| `tempo` | `timer::milissegundos()` (doc 08) e a hora do RTC |
| `limpar` | `Console::limpar` (doc 01), com as interrupções desligadas |

Os números digitados podem ser decimais ou hexadecimais, e aceitam `_` como
separador, igual ao Rust (`0xffff_9000_0000_0000`):

```rust
pub fn ler_numero(texto: &str) -> Option<u64> {
    let limpo: String = texto.chars().filter(|&c| c != '_').collect();
    match limpo.strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => limpo.parse().ok(),
    }
}
```

Três comandos trazem coisas novas: `cpu`, `tempo` (a hora) e o par
`reiniciar`/`desligar`.

### 2.4 `cpu`: a instrução `cpuid`

`cpuid` é a forma de um programa perguntar à CPU quem ela é e o que ela
sabe fazer. Coloca-se um número de "folha" (*leaf*) em EAX, executa-se
`cpuid`, e a resposta vem em EAX, EBX, ECX e EDX. O Rust oferece a
instrução pronta em `core::arch::x86_64::__cpuid`:

```rust
// Folha 0: o fabricante, 12 letras em EBX, EDX, ECX (nessa ordem).
let r = __cpuid(0);
let mut fabricante = [0u8; 12];
fabricante[0..4].copy_from_slice(&r.ebx.to_le_bytes());
fabricante[4..8].copy_from_slice(&r.edx.to_le_bytes());
fabricante[8..12].copy_from_slice(&r.ecx.to_le_bytes());
```

O fabricante é um texto de 12 letras espalhado em três registradores, na
ordem estranha EBX, EDX, ECX: `"GenuineIntel"`, `"AuthenticAMD"`... As
folhas `0x8000_0002` a `0x8000_0004` trazem o nome comercial do modelo, 48
letras em 12 registradores. A folha 1 traz família/modelo e, no EDX, um bit
por recurso (o bit 9 diz se há APIC local).

```
silicon> cpu
fabricante: AuthenticAMD
modelo:     QEMU Virtual CPU version 2.5+
familia 15, modelo 11, revisao 1; APIC local: sim
```

É a CPU que o QEMU emula por padrão (`qemu64`), que se apresenta como AMD.
Com KVM e `-cpu host`, apareceria a CPU real do computador.

> Por que não usar `asm!("cpuid")` direto? Porque `cpuid` escreve em EBX, e
> o compilador reserva o RBX para uso próprio: o `asm!` não aceita RBX como
> saída. A função `__cpuid` cuida de salvar e restaurar RBX.

### 2.5 `tempo`: a hora do relógio CMOS

O teste do doc 08 só lia os segundos do RTC. Para a hora completa, lêem-se
os registradores 4 (hora), 2 (minuto) e 0 (segundo), com dois cuidados
herdados de 1984: os valores podem estar em **BCD** (cada dígito decimal em
4 bits: 19 h é `0x19`) e a hora pode estar no formato de **12 horas**, com o
bit 7 indicando PM. O registrador 0x0B diz qual é o caso:

```rust
let b = registrador(0x0B);
let pm = h & 0x80 != 0; // no formato de 12 horas, o bit 7 da hora = PM
h &= 0x7F;
let bcd = |v: u8| (v >> 4) * 10 + (v & 0x0F);
let (mut h, m, s) = if b & 0x04 != 0 { (h, m, s) } else { (bcd(h), bcd(m), bcd(s)) };
if b & 0x02 == 0 {
    // Formato de 12 horas: 12 AM = 0h, 1-11 PM = 13-23h.
    h = (h % 12) + if pm { 12 } else { 0 };
}
```

```
silicon> tempo
ligado ha 0h 00m 03.970s (397 tiques a 100 Hz)
relogio CMOS (RTC): 19:04:55
```

O QEMU entrega ao RTC a hora UTC do computador hospedeiro: 19:04 UTC era
16:04 em Brasília no momento do teste. O RTC não sabe nada de fuso horário.

### 2.6 `reiniciar` e `desligar`

**Reiniciar** usa um truque do PC AT: o controlador do teclado (o 8042 do
doc 08) também controla a linha de **RESET** da CPU, porque em 1984 era o
chip que tinha um pino livre. O comando `0xFE` na porta 0x64 dá um pulso
nessa linha:

```rust
// src/cpu.rs
pub fn reiniciar() -> ! {
    desligar_interrupcoes();
    unsafe {
        while inb(0x64) & 0b10 != 0 {} // esperar o controlador aceitar comando
        outb(0x64, 0xFE);
    }
    // Ainda aqui? Plano B: triple fault.
    let idt_vazia = crate::gdt::PonteiroTabela { limite: 0, base: 0 };
    unsafe {
        asm!("lidt [{}]", "int3", in(reg) &idt_vazia, options(noreturn));
    }
}
```

O plano B usa o que vimos no doc 04: com uma IDT de tamanho zero, o `int3`
não encontra tratador, o que vira double fault, que também não tem
tratador, e isso é um **triple fault**: a CPU desiste e reinicia. É o
"reinício de emergência" que todo kernel em desenvolvimento conhece bem.

**Desligar** é mais complicado, porque não existe um pino de "desligar"
padronizado desde sempre. Desde os anos 1990 é o **ACPI** que cuida disso
(doc 08): desligar é pôr a máquina no estado **S5** (*soft off*),
escrevendo no registrador **PM1 control** o código do S5 junto com o bit
`SLP_EN` ("entrar no estado agora"). As duas informações vêm de lugares
diferentes:

- **a porta do PM1 control** está na tabela **FADT** (assinatura `"FACP"`),
  no byte 64. No QEMU: `0xB004`;
- **o código do S5** está no **DSDT**, e é aqui que complica: o DSDT não é
  uma tabela de campos fixos, é um programa em **AML** (*ACPI Machine
  Language*), um *bytecode* que o sistema operacional deveria interpretar.

Interpretar AML é um projeto em si. O SiliconOS usa o atalho clássico:
procurar no DSDT os bytes do objeto `_S5_` e ler o primeiro número do
pacote que vem logo depois:

```
  08      '_S5_'    12         0A ou nada       código do S5 (SLP_TYPa)
  NameOp  o nome    PackageOp  [tamanho, n. de elementos, BytePrefix]  ...
```

```rust
// src/acpi.rs
if ler::<[u8; 4]>(p) == *b"_S5_" && ler::<u8>(p - 1) == 0x08 && ler::<u8>(p + 4) == 0x12 {
    let mut q = p + 5;
    let tamanho_pacote: u8 = ler(q);
    q += ((tamanho_pacote >> 6) & 0b11) as u64 + 1; // bits 6-7: bytes extras do tamanho
    q += 1; // número de elementos
    if ler::<u8>(q) == 0x0A { q += 1; } // BytePrefix: o número vem no byte seguinte
    return Some(ler(q));
}
```

Com isso, desligar é:

```rust
outw(pm1a, (slp_typ as u16) << 10 | SLP_EN);
```

No QEMU, o código do S5 é 0, então o valor escrito é `0x2000`. O QEMU
fecha sozinho, com código de saída 0. Um detalhe que apareceu no teste: o
desligamento não é instantâneo. Na primeira versão, o kernel ainda chegava
a imprimir "nao consegui desligar" antes de o QEMU fechar. Agora ele espera
20 tiques (200 ms) antes de concluir que falhou.

A descrição do comando diz "testado só no QEMU" de propósito: o atalho do
`_S5_` funciona na maioria das máquinas, mas não foi testado em hardware
real.

---

## 3. Testes automáticos: `cargo test` dentro do QEMU

### 3.1 O que falta para o `cargo test` funcionar

Num projeto Rust comum, `cargo test` compila uma versão especial do programa
em que o `main` é trocado por um executor de testes (o *test harness*, da
crate `test`), roda esse executável e lê o resultado. No SiliconOS, três
coisas impedem isso:

1. a crate `test` depende da `std` (threads para rodar testes em paralelo,
   captura da saída, processos), e o kernel é `no_std`;
2. o "executável" é um `.efi`, que não roda no Linux: precisa do QEMU;
3. o QEMU, quando o kernel termina, não sabe dizer "passou" ou "falhou":
   ele simplesmente fica lá, rodando um `hlt` para sempre.

Cada problema tem sua solução: um **framework de testes próprio**, um
**runner** que chama o QEMU, e o dispositivo **`isa-debug-exit`**.

### 3.2 O framework próprio (`custom_test_frameworks`)

O Rust nightly permite trocar o executor de testes:

```rust
// src/main.rs
#![feature(custom_test_frameworks)]
#![test_runner(crate::testes::executar)]
#![reexport_test_harness_main = "rodar_testes"]
```

As três linhas dizem: "em modo de teste, colete todas as funções marcadas
com `#[test_case]`, passe a lista para `crate::testes::executar`, e gere uma
função chamada `rodar_testes` que faz isso". Quem chama `rodar_testes` é o
próprio kernel, depois de inicializar tudo:

```rust
interrupcoes::init();
testes_de_interrupcoes();

// `cargo test`: roda os #[test_case] e encerra o QEMU (não volta).
#[cfg(test)]
rodar_testes();
```

A escolha de **onde** chamar importa: os testes rodam com GDT, IDT,
paginação, heap e interrupções prontos, então podem testar tudo isso. O
`#[cfg(test)]` ([§33](rust-basico.md#33-testes-cfgtest-e-test_case)) faz essa
linha existir só na compilação de teste.

O executor é curto:

```rust
// src/testes.rs
pub trait Testavel {
    fn rodar(&self);
}

impl<T: Fn()> Testavel for T {
    fn rodar(&self) {
        kprint!("{} ... ", core::any::type_name::<T>());
        self();
        kprintln!("[ok]");
    }
}

pub fn executar(testes: &[&dyn Testavel]) {
    kprintln!("Rodando {} testes", testes.len());
    for t in testes {
        t.rodar();
    }
    kprintln!("Todos os {} testes passaram.", testes.len());
    qemu::sair(Saida::Sucesso);
}
```

- `impl<T: Fn()> Testavel for T` implementa a trait para **qualquer** função
  sem parâmetros ([§34](rust-basico.md#34-dyn-trait-e-implementar-para-todo-t)).
  Toda função `#[test_case]` vira automaticamente um `Testavel`.
- `core::any::type_name::<T>()` dá o nome do tipo da função, que em Rust é
  o caminho completo dela: `siliconos::memoria::heap::testes::box_mora_no_heap`.
  Assim cada teste se apresenta sozinho.
- Se um teste falha, o `assert!` dispara um **panic**. Não há como "pegar" o
  panic e seguir para o próximo teste (não existe `catch` em `no_std` com
  `panic = abort`). Então o panic handler, em modo de teste, encerra o QEMU
  com falha:

```rust
// src/panic.rs (no fim, depois de mostrar a mensagem)
#[cfg(test)]
{
    let _ = writeln!(Serial, "[FALHOU]");
    crate::qemu::sair(crate::qemu::Saida::Falha);
}
#[cfg(not(test))]
parar();
```

### 3.3 Sair do QEMU com um código: `isa-debug-exit`

O QEMU tem um dispositivo feito para isso. Iniciado com
`-device isa-debug-exit,iobase=0xf4,iosize=0x04`, ele cria uma porta de
I/O em `0xF4`: escrever um valor V nela **encerra o QEMU**, que devolve ao
Linux o código de saída `(V << 1) | 1`.

```rust
// src/qemu.rs
pub enum Saida {
    /// O QEMU sai com (0x10 << 1) | 1 = 33.
    Sucesso = 0x10,
    /// O QEMU sai com (0x11 << 1) | 1 = 35.
    Falha = 0x11,
}

pub fn sair(codigo: Saida) -> ! {
    unsafe { outb(PORTA_SAIDA, codigo as u8) };
    // Sem o dispositivo (máquina real, ou QEMU sem o -device): parar aqui.
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
```

Por que não 0 para sucesso? Porque a fórmula sempre produz um número
**ímpar**: 0 é impossível. Escolhemos 33 e 35, e o script traduz.

### 3.4 O runner: `.cargo/config.toml` e `testar.sh`

Falta dizer ao Cargo que, para o alvo UEFI, "rodar um executável" significa
chamar um script:

```toml
# .cargo/config.toml
[target.x86_64-unknown-uefi]
runner = "./testar.sh"
```

O Cargo chama `./testar.sh <caminho-do-.efi-de-teste>`. O script põe o
`.efi` numa partição EFI separada (`target/esp-teste/`, para não estragar a
do `run.sh`), roda o QEMU sem janela com o `isa-debug-exit` e traduz o
código:

```bash
timeout 60 qemu-system-x86_64 -bios /usr/share/ovmf/OVMF.fd \
    -drive format=raw,file=fat:rw:target/esp-teste -net none -no-reboot \
    -display none -serial stdio \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04
codigo=$?
case $codigo in
    33)  exit 0 ;;                                  # Saida::Sucesso
    35)  echo "testar.sh: o kernel relatou FALHA"; exit 1 ;;
    124) echo "testar.sh: tempo esgotado (60 s)"; exit 2 ;;
    *)   echo "testar.sh: QEMU saiu com codigo $codigo"; exit 3 ;;
esac
```

O `timeout 60` cobre o terceiro tipo de resultado, que um kernel tem e um
programa comum quase nunca tem: **travar**. Um teste que entra em deadlock
(doc 08) não passa nem falha, só para; depois de 60 s, o script desiste e
devolve 2 (o `timeout` sai com 124).

```
 cargo test --target x86_64-unknown-uefi
   │
   ├─ compila o kernel com cfg(test): rodar_testes() no fim da inicialização
   ├─ chama ./testar.sh target/.../siliconos-<hash>.efi
   │     └─ QEMU ─► boot ─► inicializa tudo ─► roda os #[test_case]
   │                 ├─ todos ok  ─► porta 0xF4 ← 0x10 ─► QEMU sai com 33 ─► script: 0
   │                 ├─ panic     ─► porta 0xF4 ← 0x11 ─► QEMU sai com 35 ─► script: 1
   │                 └─ travou    ─► timeout 60 s      ─► 124             ─► script: 2
   └─ código 0 = "test result: ok"
```

### 3.5 Os testes

Cada módulo ganhou, no fim, um bloco de testes:

```rust
// src/interrupcoes/teclado.rs
#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn shift_e_caps_lock() {
        let mut d = Decodificador::new();
        // Shift esquerdo + a + 1, solta o Shift, a
        assert_eq!(digitar(&mut d, &[0x2A, 0x1E, 0x02, 0xAA, 0x1E]), "A!a");
        // Caps Lock liga: letra maiúscula, número continua número
        assert_eq!(digitar(&mut d, &[0x3A, 0xBA, 0x1E, 0x02]), "A1");
        // ...
    }
}
```

São 24 testes, de dois tipos:

- **lógica pura**, que poderia rodar em qualquer lugar: o decodificador de
  scancodes, `ler_numero`, a edição da linha do shell, `alinhar_acima`;
- **o hardware de verdade**, que só faz sentido dentro do kernel: mapear e
  desmapear uma página, a página 0 sem mapeamento, o heap sem vazamento, o
  MSR do APIC, a MADT com o desvio IRQ 0 → GSI 2, o DSDT com `_S5_`, os
  tiques do timer avançando, `sem_interrupcoes` restaurando o estado
  (inclusive aninhado).

```
$ cargo test --target x86_64-unknown-uefi
     Running unittests src/main.rs (target/x86_64-unknown-uefi/debug/build/siliconos/.../siliconos-....efi)
... (o boot de sempre) ...
Rodando 24 testes
siliconos::acpi::testes::dsdt_tem_s5 ... [ok]
siliconos::acpi::testes::irq0_desviada_para_gsi2_no_qemu ... [ok]
siliconos::acpi::testes::irq_sem_desvio_vai_para_a_mesma_gsi ... [ok]
siliconos::acpi::testes::madt_tem_cpu_e_ioapic ... [ok]
siliconos::cpu::testes::msr_do_apic ... [ok]
siliconos::cpu::testes::sem_interrupcoes_restaura_o_estado ... [ok]
siliconos::interrupcoes::teclado::testes::enter_e_backspace ... [ok]
siliconos::interrupcoes::teclado::testes::letras_e_numeros ... [ok]
siliconos::interrupcoes::teclado::testes::shift_e_caps_lock ... [ok]
siliconos::interrupcoes::teclado::testes::teclas_estendidas_ignoradas ... [ok]
siliconos::interrupcoes::timer::testes::hora_rtc_valida ... [ok]
siliconos::interrupcoes::timer::testes::tiques_avancam ... [ok]
siliconos::memoria::heap::testes::alinhar_acima_arredonda ... [ok]
siliconos::memoria::heap::testes::box_mora_no_heap ... [ok]
siliconos::memoria::heap::testes::sem_vazamento_e_blocos_juntados ... [ok]
siliconos::memoria::paginacao::testes::erros_de_mapeamento ... [ok]
siliconos::memoria::paginacao::testes::mapear_e_desmapear ... [ok]
siliconos::memoria::paginacao::testes::metade_alta_mostra_a_ram ... [ok]
siliconos::memoria::paginacao::testes::pagina_zero_nao_mapeada ... [ok]
siliconos::memoria::quadros::testes::alocar_e_liberar_devolve_a_contagem ... [ok]
siliconos::shell::testes::editar_linha ... ecx o   [ok]
siliconos::shell::testes::ler_numero_decimal_e_hexadecimal ... [ok]
siliconos::shell::testes::linha_tem_limite ... aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[ok]
siliconos::shell::testes::nomes_de_comandos_unicos ... [ok]
Todos os 24 testes passaram.
$ echo $?
0
```

(O `ecx o` e a fileira de `a` são o eco do shell durante os testes de
edição: o teste digita de verdade, e o shell ecoa como faria com você.)

Para ver uma falha, a feature `teste-que-falha` acrescenta um teste errado
de propósito:

```
$ cargo test --target x86_64-unknown-uefi --features teste-que-falha
siliconos::testes::este_teste_falha ...
!!! KERNEL PANIC !!!
Local: src/testes.rs:40:5
Mensagem: assertion `left == right` failed: de proposito
  left: 2
 right: 3
[FALHOU]
testar.sh: o kernel relatou FALHA
error: test failed, to rerun pass `--bin siliconos`
$ echo $?
1
```

### 3.6 E as features de teste?

Continuam, e por um bom motivo: elas testam o que **termina a execução**
(page fault, double fault, triple fault, deadlock, heap esgotado) e o que
precisa de olho humano (o vermelho do panic na tela, o travamento do
`teste-deadlock`). Um `#[test_case]` que provoca um page fault derrubaria o
kernel e encerraria todos os testes com "falha", sem dizer se o page fault
era o esperado. Um framework mais completo poderia rodar cada teste desses
num QEMU separado e esperar o panic certo (o equivalente do
`#[should_panic]`); por enquanto, o `cargo test` cobre o que passa e as
features cobrem o que quebra.

---

## 4. Depurar com o GDB

### 4.1 O QEMU como alvo do GDB

O GDB, o depurador do GNU, sabe conversar com um programa por um protocolo
de rede (o *GDB remote protocol*). O QEMU fala esse protocolo: com a opção
**`-s`**, ele abre a porta TCP 1234 e se comporta como se a CPU emulada
fosse um programa sendo depurado. O GDB pode então parar a CPU, ler
registradores e memória, colocar breakpoints e continuar. Tudo isso sem
nenhuma ajuda do kernel: é o emulador que obedece.

### 4.2 O problema dos símbolos

Para o GDB dizer `siliconos::shell::executar` em vez de `0x5ffdb00`, ele
precisa da **tabela de símbolos** (nome → endereço). Aí aparecem duas
dificuldades, ambas por o `.efi` ser um executável no formato do Windows
(PE/COFF):

1. **O Rust grava as informações de depuração no formato da Microsoft**
   (um arquivo `.pdb` separado, em `target/`), que o GDB não lê. Nem os
   nomes das funções ficam dentro do `.efi`. A solução encontrada é pedir ao
   linker que grave a tabela de símbolos dentro do `.efi`: a opção
   `/debug:symtab`. Com ela, o GDB conhece os nomes e endereços das funções.
   Não há números de linha nem nomes de variáveis (isso estaria no `.pdb`).
2. **O endereço muda a cada boot.** No arquivo, o código começa em
   `0x1_4000_0000` (o *ImageBase* padrão do formato PE). O UEFI carrega o
   `.efi` onde houver memória livre e corrige os endereços (as *relocations*,
   seção `.reloc` do arquivo). Em duas execuções seguidas do mesmo `.efi`,
   a base foi `0x5fe8000` e `0x5ff8000`. Por isso o kernel agora imprime a
   base logo no começo, pegando-a do protocolo `LoadedImage` do UEFI antes de
   dispensá-lo:

```
Imagem do kernel: base 0x6036000, 332 KiB (no arquivo, a base e 0x140000000)
```

O GDB então carrega os símbolos com um deslocamento:
`add-symbol-file siliconos.efi -o <base>-0x140000000`.

### 4.3 `depurar.sh`

O script faz tudo isso:

```bash
RUSTFLAGS="-C link-arg=/debug:symtab" \
    cargo build --target x86_64-unknown-uefi --target-dir target/simbolos $FEATURES
...
qemu-system-x86_64 ... -serial file:"$SERIAL" -s &     # -s: servidor do GDB na porta 1234
...                                                    # espera "Imagem do kernel: base 0x..."
cat > target/simbolos/gdb.txt <<FIM
target remote :1234
add-symbol-file $EFI -o $BASE-0x140000000
FIM
gdb -q -x target/simbolos/gdb.txt
```

A compilação com símbolos vai para uma pasta separada (`target/simbolos`),
porque mudar o `RUSTFLAGS` recompila tudo e não queremos misturar com a
compilação normal do `run.sh`.

### 4.4 Uma sessão

`./depurar.sh` abre a janela do QEMU e, no terminal, o GDB já conectado. A
CPU fica parada no ponto em que o GDB se conectou (o `hlt` do shell). Um
breakpoint no executor de comandos, e depois `continue`:

```
(gdb) break siliconos::shell::executar
Breakpoint 1 at 0x5ffdb00
(gdb) continue
Continuing.
```

Na janela do QEMU, digita-se `mem` e Enter. A CPU para:

```
Breakpoint 1, 0x0000000005ffdb00 in siliconos::shell::executar ()
(gdb) bt 2
#0  0x0000000005ffdb00 in siliconos::shell::executar ()
#1  0x0000000005ffc254 in <siliconos::shell::Shell>::tecla ()
(gdb) info registers rip rsp
rip            0x5ffdb00           0x5ffdb00 <siliconos::shell::executar>
rsp            0x7e95f38           0x7e95f38
(gdb) x/4i $pc
=> 0x5ffdb00 <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar>:	sub    $0x128,%rsp
   0x5ffdb07 <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar+7>:	mov    %rdx,%r8
   0x5ffdb0a <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar+10>:	mov    %rcx,%rdx
   0x5ffdb0d <_RNvNtCsfGbHo577FqM_9siliconos5shell8executar+13>:	mov    %rdx,0xf8(%rsp)
(gdb) continue
```

E o `mem` termina normalmente na tela. Algumas observações:

- `_RNvNtCsfGbHo577FqM_9siliconos5shell8executar` é o **nome "mutilado"**
  (*mangled*) da função: o Rust codifica o caminho completo e um hash num
  nome que o linker aceita. O GDB traduz de volta (`siliconos::shell::executar`)
  nos lugares que importam.
- A primeira instrução, `sub $0x128,%rsp`, reserva 296 bytes de pilha para
  as variáveis locais de `executar`.
- O `bt` (*backtrace*, a pilha de chamadas) só é confiável nos primeiros
  níveis. Sem as informações de desenrolar a pilha (que estariam no `.pdb`),
  o GDB adivinha os níveis seguintes lendo a pilha, e mostra lixo. O
  `bt 2` acima pede só os dois níveis que ele acerta. Tentamos também
  compilar com *frame pointers* (`-C force-frame-pointers=yes`), sem
  melhora.

Um breakpoint no tratador do timer mostra o doc 08 por dentro:

```
(gdb) break siliconos::interrupcoes::timer::tratador
(gdb) continue
Breakpoint 1, 0x0000000005fef6f0 in siliconos::interrupcoes::timer::tratador ()
(gdb) bt 2
#0  0x0000000005fef6f0 in siliconos::interrupcoes::timer::tratador ()
#1  0x000000000600aaf3 in siliconos::interrupcoes::teclado::esperar ()
(gdb) info registers eflags
eflags         0x2                 [ IOPL=0 ]
```

O timer interrompeu a CPU dentro de `teclado::esperar` (o `sti; hlt`, onde
ela passa quase todo o tempo), e dentro do tratador o `eflags` é `0x2`: o
bit IF está **desligado**, como o *interrupt gate* promete (doc 04). Um
cuidado: um breakpoint no tratador do timer dispara 100 vezes por segundo.
Apague-o (`delete`) antes de continuar.

Com `SEM_TELA=1`, o script roda sem janela, e os comandos do GDB podem vir
de um arquivo ou de um *pipe*, como foi feito para gerar as saídas acima.

---

## 5. Como foi testado

- **Shell:** com o monitor do QEMU e `sendkey` (doc 08), foram digitados
  `ajuda`, `eco ola Mundo`, `tempo`, `mem`, `cpu`, `int`,
  `traduzir 0xffff_9000_0000_0000`, `xyz` e uma linha vazia. Todas as saídas
  deste doc são dessas execuções. A tela (por `screendump`) mostrou o mesmo
  que a serial, incluindo o `limpar` e um `tempox` corrigido com Backspace.
- **`ler 0`:** page fault "ao ler 0x0 (pagina nao mapeada, modo kernel)", o
  tratador do doc 04 em ação.
- **`reiniciar`:** o QEMU, iniciado com `-no-reboot`, fecha em vez de
  reiniciar, o que prova que o reset aconteceu.
- **`desligar`:** o QEMU fechou sozinho, com código de saída 0, sem a
  mensagem de falha (a FADT deu a porta `0xB004`; o DSDT, S5 = 0).
- **`cargo test`:** 24 testes, código de saída 0. Com `teste-que-falha`,
  código 1 e a mensagem do `assert_eq!`.
- **GDB:** as sessões da seção 4.4.
- **Regressão:** as 21 builds de antes (boot normal e 20 features) deram o
  resultado esperado.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 08 (tag `doc-08`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-09`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-08...doc-09).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/shell.rs) | **novo**: `Shell` (`tecla`), `rodar`, `Comando`, `COMANDOS`, `executar`, `ler_numero` e uma função por comando | — |
| [`src/testes.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/testes.rs) | **novo**: `Testavel`, `executar` (o *runner*) e o teste da feature `teste-que-falha` | — |
| [`src/qemu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/qemu.rs) | **novo**: `Saida`, `sair` (porta 0xF4) | — |
| [`.cargo/config.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/.cargo/config.toml) | **novo**: `[target.x86_64-unknown-uefi]` com `runner = "./testar.sh"` | — |
| [`testar.sh`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/testar.sh) | **novo** (executável): roda o `.efi` de teste no QEMU com `isa-debug-exit` e traduz 33/35/124 | raiz do projeto |
| [`depurar.sh`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/depurar.sh) | **novo** (executável): compila com `/debug:symtab`, sobe o QEMU com `-s`, lê a base e abre o GDB | raiz do projeto |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `#![feature(custom_test_frameworks)]`, `#![test_runner(crate::testes::executar)]`, `#![reexport_test_harness_main = "rodar_testes"]` | topo, depois de `#![feature(abi_x86_interrupt)]` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `#[cfg_attr(not(test), allow(dead_code))] mod qemu;`, `mod shell;`, `#[cfg(test)] mod testes;` | junto dos outros `mod` |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `static IMAGEM_BASE` e `IMAGEM_TAMANHO` (`AtomicU64`); em `main()`, bloco que abre o `LoadedImage` e as preenche; em `kernel_main`, o `kprintln!` "Imagem do kernel" | depois de `CONSOLE`; depois de `guardar_rsdp()`; depois da linha do vídeo |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/main.rs) | `#[cfg(test)] rodar_testes();`; `ecoar_teclado` sai e o fim de `kernel_main` vira `shell::rodar()` | depois de `testes_de_interrupcoes()` |
| [`src/panic.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/panic.rs) | `#[cfg(test)]` escreve `[FALHOU]` e chama `qemu::sair(Falha)`; `#[cfg(not(test))] parar();` | fim da função `panic` |
| [`src/cpu.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/cpu.rs) | `reiniciar` (8042 + triple fault) | fim do arquivo |
| [`src/acpi.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/acpi.rs) | `desligar` (FADT + S5) e `tipo_s5` | fim do arquivo |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/interrupcoes/timer.rs) | `hora_rtc` | fim do arquivo |
| [`src/serial.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/src/serial.rs) | `outw` e `inw` | antes de `inb` |
| vários | um bloco `#[cfg(test)] mod testes { ... }` com `#[test_case]` em `acpi.rs`, `cpu.rs`, `interrupcoes/teclado.rs`, `interrupcoes/timer.rs`, `memoria/heap.rs`, `memoria/paginacao.rs`, `memoria/quadros.rs` e `shell.rs` | fim de cada arquivo |
| [`Cargo.toml`](https://github.com/moises-dev-git/siliconOS/blob/doc-09/Cargo.toml) | `teste-que-falha = []` | `[features]` |

---

## Diferença em relação ao livro

O capítulo 10 do livro trata de um shell interativo, de testes no QEMU e de
depuração com GDB, os mesmos três temas. Vale comparar ao ler. É comum, em
projetos com bootloader, que o executável do kernel seja ELF e traga as
informações de depuração em DWARF, que o GDB entende diretamente; aí o GDB
mostra até o número da linha.

As particularidades do SiliconOS:

- como o kernel é um programa UEFI (formato PE), as informações de depuração
  saem no formato da Microsoft. A saída foi gravar só a tabela de símbolos
  (`/debug:symtab`) e calcular o deslocamento pela base que o UEFI informa;
- o `cargo test` usa um runner próprio (`testar.sh`) e uma partição EFI
  separada, e o kernel roda os testes **depois** de inicializar tudo,
  inclusive as interrupções;
- `desligar` lê a FADT e procura o `_S5_` no DSDT, em vez de usar uma porta
  fixa do QEMU.

---

## Resumo

- O **shell** recebe caracteres do decodificador do teclado, monta a linha
  num `String` e, no Enter, procura o comando numa **tabela de ponteiros de
  função**. Ele decide o eco (Backspace nunca apaga o prompt) e roda os
  comandos no laço principal, fora dos tratadores.
- `cpuid` diz o fabricante e o modelo da CPU; o RTC dá a hora (BCD e 12/24 h
  conforme o registrador 0x0B).
- **Reiniciar:** comando `0xFE` no controlador do teclado, com triple fault
  como plano B. **Desligar:** estado S5 do ACPI, com a porta da FADT e o
  código do `_S5_` no DSDT.
- `cargo test` funciona com um **framework próprio** (`custom_test_frameworks`):
  os `#[test_case]` rodam no fim da inicialização, um panic encerra com
  falha, e o **`isa-debug-exit`** transforma o resultado em código de saída
  do QEMU (33 ou 35), que o `testar.sh` traduz para 0 ou 1. Um teste que
  trava cai no `timeout`.
- As features de teste continuam para o que derruba o kernel de propósito.
- **GDB:** `-s` no QEMU, tabela de símbolos com `/debug:symtab` e o
  deslocamento pela base impressa no boot (que muda a cada execução).
  Breakpoints por nome de função funcionam; o `bt` só é confiável nos
  primeiros níveis.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que o Backspace do shell não apaga o `silicon> `?**
   Porque é o shell que decide o eco: ele só manda o Backspace para a tela
   se `self.linha.pop()` tirou algum caractere, ou seja, se havia algo
   digitado para apagar.

2. **O que o campo `funcao: fn(&[&str])` guarda?**
   O endereço de uma função que recebe os argumentos do comando. `executar`
   acha o comando pelo nome e chama `(comando.funcao)(argumentos)`.

3. **Por que o kernel não usa `asm!("cpuid")` direto?**
   Porque `cpuid` escreve em EBX, e o RBX é reservado pelo compilador: o
   `asm!` não o aceita como saída. `__cpuid` salva e restaura o RBX.

4. **De onde vêm as duas informações necessárias para desligar?**
   A porta do PM1 control vem da tabela FADT (byte 64). O código do estado
   S5 vem do DSDT, que é bytecode AML; o kernel procura o objeto `_S5_` e lê
   o primeiro número do pacote.

5. **Por que o `cargo test` padrão não funciona num kernel `no_std`?**
   Porque o executor de testes padrão (crate `test`) depende da `std`, o
   `.efi` não roda no Linux, e o QEMU não tem como informar o resultado. O
   framework próprio, o runner `testar.sh` e o `isa-debug-exit` resolvem
   cada um desses pontos.

6. **Por que os códigos de saída são 33 e 35, e não 0 e 1?**
   Porque o `isa-debug-exit` sai com `(V << 1) | 1`, sempre ímpar. 0x10 dá
   33 e 0x11 dá 35. O `testar.sh` traduz para 0 e 1.

7. **Como o `cargo test` lida com um teste que trava?**
   O `testar.sh` roda o QEMU com `timeout 60`. Se o kernel não sair em 60
   s, o `timeout` o encerra e o script devolve 2.

8. **Por que o GDB precisa de `-o <base>-0x140000000` ao carregar os
   símbolos?**
   Porque os endereços no arquivo partem do ImageBase `0x140000000`, mas o
   UEFI carrega o `.efi` noutro endereço (que muda a cada boot) e corrige o
   código com as *relocations*. O deslocamento leva cada símbolo do endereço
   do arquivo ao endereço real na memória.

</details>
