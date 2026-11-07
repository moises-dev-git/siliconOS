# 16: Argumentos para os programas (argv)

> **Você vai entender:** como um programa recebe os **argumentos da linha de
> comando** (`argc` e `argv`, o `String[] args` do Java ou o `sys.argv` do
> Python); por que, no Linux e aqui, eles chegam **escritos na pilha** do
> programa antes da primeira instrução; o formato exato dessa pilha inicial
> (o do **System V**); e por que isso obriga o ponto de entrada `_start` a
> ser escrito em assembly.
>
> **Arquivos:** `src/usuario/argumentos.rs` (novo: monta a pilha inicial),
> `src/usuario/mod.rs` (`iniciar_elf` recebe o argv), `src/usuario/elf.rs`
> (erro novo e testes), `src/shell.rs` (`rodar` separa os argumentos),
> `programas/src/lib.rs` (`_start` naked, `sos::argumento`,
> `sos::argumentos`), `programas/src/bin/args.rs` (novo) e `primos.rs` (o
> limite vem do argumento)
> **Livro:** capítulo 11, seção 11.4 (o livro não passa argumentos aos
> programas; o formato é o do System V, usado pelo Linux)
> **Testar:** `./run.sh`, depois `rodar args um dois tres`, `rodar primos 5000`
> e `rodar primos 300 &`; `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 13 (programas ELF, a lib `sos`, o `principal!`), doc 14
> (a convenção de chamada das syscalls)
> **Rust novo aqui:** `impl Iterator` como retorno, padrões que comparam
> texto dentro de um slice, `str::from_utf8`, `$crate` e `sym` numa macro
> ([§46](rust-basico.md#46-impl-iterator-padrões-em-slices-e-textos-vindos-de-bytes))
>
> **Código deste capítulo:** [estado no fim do doc 16](https://github.com/moises-dev-git/siliconOS/tree/doc-16) · [o que mudou desde o doc 15](https://github.com/moises-dev-git/siliconOS/compare/doc-15...doc-16) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema: todo programa faz sempre a mesma coisa

Até o doc 15, `primos` conta os primos abaixo de 10 000. Sempre. Para contar
abaixo de 5 000, era preciso mudar o código-fonte, recompilar o kernel (o
programa viaja dentro dele, doc 13) e reiniciar a máquina.

Em qualquer sistema de verdade, você escreveria:

```
$ primos 5000
```

e o programa leria o `5000`. Em Java, ele chega no `main(String[] args)`; em
Python, em `sys.argv`; em C, em `main(int argc, char **argv)`; em Rust com
`std`, em `std::env::args()`. Mas nenhuma dessas linguagens **inventa** os
argumentos: quem os entrega é o sistema operacional, quando cria o processo.
A biblioteca da linguagem só os encontra e os embrulha num tipo bonito.

Então duas perguntas, uma para cada lado:

1. **Kernel:** onde deixar os textos, para o programa achar?
2. **Programa:** como achá-los, se o `main` é chamado por um `_start` que
   nem sabe que eles existem?

## 2. A ideia: escrever os argumentos na pilha do programa

O kernel tem um problema: os textos estão na memória **dele** (a linha que
o shell montou, no heap do kernel). O programa não enxerga essa memória (doc
11: cada programa tem o seu espaço, e o kernel aparece sem o bit USUARIO).
Os textos precisam ser **copiados** para dentro do espaço do programa.

Para onde? O kernel já cria uma região do programa que é gravável e que o
programa usa desde a primeira instrução: a **pilha**. A solução dos sistemas
Unix desde os anos 70, padronizada pela **ABI System V** (o contrato que o
Linux, o FreeBSD e outros seguem em x86-64), é esta:

> Antes da primeira instrução, o kernel escreve no topo da pilha os textos
> dos argumentos e, logo abaixo, uma tabela com ponteiros para eles. RSP
> aponta para o começo da tabela.

O formato, do endereço mais baixo (onde RSP aponta) para o mais alto:

| Onde | O que | Tamanho |
|---|---|---|
| RSP | `argc`: quantos argumentos, contando o nome | 8 bytes |
| RSP + 8 | `argv[0]`, `argv[1]`, ..., `argv[argc-1]`: ponteiros para os textos | 8 bytes cada |
| | `0`: fim de `argv` | 8 bytes |
| | `envp[0]`, `envp[1]`, ...: ponteiros para `NOME=valor` | 8 bytes cada |
| | `0`: fim de `envp` | 8 bytes |
| | (no Linux) o *auxiliary vector*, pares (tipo, valor) | 16 bytes cada |
| | (folga para alinhar) | |
| topo | os textos, cada um terminado pelo byte 0 | |

Três detalhes que valem a pena:

- **`argv[0]` é o nome do programa.** Quem chama decide o que vai ali; por
  convenção, é o nome com que o programa foi chamado. Por isso um programa
  sem argumentos tem `argc = 1`, não 0.
- **Os textos terminam com o byte 0** (o "texto de C", *NUL-terminated*). Não
  há um tamanho guardado: quem lê anda até achar o 0. Consequência: um
  argumento não pode conter o byte 0.
- **RSP chega alinhado em 16.** A ABI exige isso no início do processo. Isso
  é diferente do que uma função comum espera na entrada (lá, RSP é "múltiplo
  de 16 menos 8", porque o `call` acabou de empilhar o endereço de volta). A
  seção 5 mostra o que isso muda no `_start`.

O Linux faz exatamente isso. Dá para conferir no próprio computador de
desenvolvimento, com um programa em C de 4 linhas que imprime onde ficam
`argv` e `envp` (rodado assim, no host Linux, não no SiliconOS):

```
$ ./pilha um dois
argc=3 ... argv=0x7ffd194dc608 argv[0]=0x7ffd194dcd5b "./pilha" envp=0x7ffd194dc628 (argv+argc+1=0x7ffd194dc628)
```

`envp` fica exatamente em `argv + argc + 1`: depois dos 3 ponteiros e do 0.
E `argv[0]` está mais de 1 KiB acima, no topo, junto com os outros textos. O
*auxiliary vector* também está lá (o Linux mostra com `LD_SHOW_AUXV=1
/bin/true`: `AT_PAGESZ: 4096`, `AT_PHDR`, ...); é por ele que o carregador
dinâmico descobre, por exemplo, onde estão os cabeçalhos do programa.

O SiliconOS segue o mesmo formato, com um ambiente vazio (`envp` só com o 0
do fim) e sem *auxiliary vector*: nada aqui precisa dele.

### 2.1 Por que não em registradores?

Seria mais simples o kernel pôr `argc` em RDI e um ponteiro para `argv` em
RSI, como numa chamada de função. Mas os textos ainda precisariam ser
copiados para **algum lugar** do espaço do programa, e a pilha é o único
lugar gravável que todo programa tem. Uma vez lá, deixar também a tabela de
ponteiros na pilha não custa nada, e o formato fica independente de
registradores (o mesmo layout serve em qualquer CPU; só o registrador da
pilha muda de nome). Seguir o System V também tem uma vantagem prática: é o
que o compilador e as bibliotecas de verdade esperam. Um programa feito para
ele precisaria só das syscalls certas, não de outro formato de entrada.

## 3. O kernel: montar a pilha inicial

O trabalho do kernel fica num módulo novo, `src/usuario/argumentos.rs`. A
função principal recebe os argumentos e o endereço do topo da pilha **no
espaço do programa** e devolve os bytes prontos para copiar:

```rust
/// O topo da pilha pronto para ser copiado: `bytes` vai morar em
/// [rsp, topo) no espaço do programa.
#[derive(Debug)]
pub struct PilhaInicial {
    pub bytes: Vec<u8>,
    pub rsp: u64,
}
```

Por que montar num `Vec` do kernel, e não escrever direto na pilha? Porque
o kernel escreve na memória do programa **pelo endereço físico** (o
deslocamento do doc 06: `fisico_para_virtual`), já que o espaço do programa
não está ativo enquanto ele é criado. Os ponteiros de `argv`, porém, têm de
ser endereços **do programa** (perto de `0x7FFF_FFFF_FFFF`). Montar tudo
num vetor, calculando os ponteiros a partir de `topo`, separa as duas
coisas: a conta é feita uma vez, com endereços do programa, e a cópia é só
um `copy_nonoverlapping`. E fica testável sem criar processo nenhum
(seção 7.3).

### 3.1 As verificações

```rust
/// Mais que isto, `montar` recusa (o Linux aceita muito mais; aqui o shell
/// nem consegue digitar tantos).
pub const MAX_ARGUMENTOS: usize = 32;

/// Por que os argumentos não puderam ser entregues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroArgumentos {
    /// Nenhum argumento: falta pelo menos argv[0], o nome do programa.
    Vazio,
    /// Mais de MAX_ARGUMENTOS.
    Demais,
    /// Textos e ponteiros juntos não cabem na última página da pilha.
    GrandesDemais,
    /// Um argumento contém o byte 0, que marca o fim de cada texto.
    TemZero,
}
```

`GrandesDemais` existe porque `montar` decide que tudo cabe numa página só
(a mais alta da pilha). É uma simplificação: o Linux reserva bem mais
(um quarto do limite da pilha: com os 8 MiB padrão, `getconf ARG_MAX`
responde 2097152, 2 MiB).
Com uma linha do shell limitada a 70 caracteres (doc 09), uma página sobra.
`TemZero` existe pelo formato: um 0 no meio do texto o cortaria em dois.

### 3.2 A conta

```rust
pub fn montar(argumentos: &[&str], topo: u64) -> Result<PilhaInicial, ErroArgumentos> {
    if argumentos.is_empty() {
        return Err(ErroArgumentos::Vazio);
    }
    if argumentos.len() > MAX_ARGUMENTOS {
        return Err(ErroArgumentos::Demais);
    }
    if argumentos.iter().any(|a| a.as_bytes().contains(&0)) {
        return Err(ErroArgumentos::TemZero);
    }

    // 1. Onde começa a área dos textos (cada um com o 0 do fim).
    let tam_textos: u64 = argumentos.iter().map(|a| a.len() as u64 + 1).sum();
    let inicio_textos = topo - tam_textos;

    // 2. As palavras de 8 bytes: argc, um ponteiro por argumento, o 0 que
    //    fecha argv e o 0 que fecha envp. RSP no começo delas, alinhado em 16.
    let palavras = 1 + argumentos.len() as u64 + 1 + 1;
    let rsp = (inicio_textos - palavras * 8) & !0xF;

    let tamanho = topo - rsp;
    if tamanho > TAMANHO_QUADRO {
        return Err(ErroArgumentos::GrandesDemais);
    }
```

A conta vai de cima para baixo, como a pilha cresce:

1. Os textos ficam colados no topo. Com `["args", "um", "dois", "tres"]`:
   5 + 3 + 5 + 5 = 18 bytes, então `inicio_textos = topo - 18`.
2. Abaixo deles, as palavras: argc (1), um ponteiro por argumento (4), o 0
   de `argv` (1) e o 0 de `envp` (1) = 7 palavras, 56 bytes.
3. `& !0xF` apaga os 4 bits de baixo: arredonda **para baixo** até um
   múltiplo de 16. Para baixo, e não para cima, porque para cima invadiria
   os textos. A folga (de 0 a 15 bytes) fica entre as palavras e os textos.

Com `topo = 0x8000_0000_0000` (o `PILHA_ELF_TOPO` do doc 13), o exemplo dá:
`inicio_textos = 0x7FFF_FFFF_FFEE`, e `0x7FFF_FFFF_FFEE - 56 =
0x7FFF_FFFF_FFB6`, que arredonda para `0x7FFF_FFFF_FFB0`. São os endereços
que o programa `args` imprime na seção 7.1.

### 3.3 Preencher

```rust
    // 3. Preencher. `pos(endereco)` = índice no vetor do byte que vai morar
    //    naquele endereço do programa.
    let mut bytes = vec![0u8; tamanho as usize];
    let pos = |endereco: u64| (endereco - rsp) as usize;
    let escrever_u64 = |bytes: &mut Vec<u8>, endereco: u64, valor: u64| {
        bytes[pos(endereco)..pos(endereco) + 8].copy_from_slice(&valor.to_le_bytes());
    };

    escrever_u64(&mut bytes, rsp, argumentos.len() as u64); // argc
    let mut texto = inicio_textos;
    for (i, a) in argumentos.iter().enumerate() {
        escrever_u64(&mut bytes, rsp + 8 + 8 * i as u64, texto); // argv[i]
        bytes[pos(texto)..pos(texto) + a.len()].copy_from_slice(a.as_bytes());
        texto += a.len() as u64 + 1; // o 0 do fim já está lá (vetor zerado)
    }
    // argv[argc] = 0 e envp[0] = 0: também já zerados.
    Ok(PilhaInicial { bytes, rsp })
}
```

O vetor começa zerado (`vec![0u8; ...]`), então quatro coisas já estão
certas sem nenhuma linha de código: o 0 no fim de cada texto, o 0 que fecha
`argv`, o 0 que fecha `envp` e a folga. O código só escreve o que não é
zero: `argc`, os ponteiros e os bytes dos textos.

As duas closures são só para a conta ficar legível: `pos` traduz "endereço
no programa" para "índice no vetor", e `escrever_u64` grava um número de 8
bytes em *little-endian* (o byte menos significativo primeiro, como a CPU
x86 lê; ver o glossário). `escrever_u64` recebe o vetor como argumento, em
vez de capturá-lo, porque o laço também escreve nele direto
(`bytes[...].copy_from_slice`): uma closure que guardasse um `&mut bytes`
impediria qualquer outro uso do vetor enquanto ela existisse (o *borrow
checker*, [§8](rust-basico.md#8-referências-e-o-borrow-checker-versão-curtíssima)).

O resultado, para `args um dois tres`:

```
  0x7FFF_FFFF_FFFF ┐
  0x7FFF_FFFF_FFFB │ "tres\0"
  0x7FFF_FFFF_FFF6 │ "dois\0"
  0x7FFF_FFFF_FFF3 │ "um\0"
  0x7FFF_FFFF_FFEE ┘ "args\0"          ◄─ inicio_textos
  0x7FFF_FFFF_FFE8   (6 bytes de folga)
  0x7FFF_FFFF_FFE0   0                 envp[0]
  0x7FFF_FFFF_FFD8   0                 argv[4]
  0x7FFF_FFFF_FFD0   0x7FFF_FFFF_FFFB  argv[3] ─► "tres"
  0x7FFF_FFFF_FFC8   0x7FFF_FFFF_FFF6  argv[2] ─► "dois"
  0x7FFF_FFFF_FFC0   0x7FFF_FFFF_FFF3  argv[1] ─► "um"
  0x7FFF_FFFF_FFB8   0x7FFF_FFFF_FFEE  argv[0] ─► "args"
  0x7FFF_FFFF_FFB0   4                 argc     ◄─ RSP inicial
```

### 3.4 Copiar para a pilha do programa

`iniciar_elf` (doc 13) ganha um parâmetro, o argv:

```rust
pub fn iniciar_elf(nome: &'static str, dados: &[u8], argumentos: &[&str],
                   segundo_plano: bool) -> Result<Pid, elf::ErroElf> {
    // Primeiro os argumentos: se não couberem, nem vale a pena carregar.
    let pilha = argumentos::montar(argumentos, PILHA_ELF_TOPO).map_err(elf::ErroElf::Argumentos)?;
    let mut espaco = EspacoEnderecos::novo().ok_or(elf::ErroElf::SemMemoria)?;
```

O argv inclui o nome em `argv[0]`, como na syscall `execve` do Linux:
`["primos", "5000"]`. Quem chama decide o que vai em `argv[0]`; o shell põe
o nome que você digitou.

`montar` roda **antes** de qualquer alocação: se os argumentos forem
recusados, não há espaço nem quadros para devolver. O erro vira uma
variante nova de `ErroElf`:

```rust
    /// Os argumentos não puderam ser entregues ao programa (doc 16). Não é
    /// defeito do arquivo, mas é na hora de carregar que se descobre.
    Argumentos(super::argumentos::ErroArgumentos),
```

`map_err(elf::ErroElf::Argumentos)` usa a própria variante como função: uma
variante de enum com um campo é também uma função que recebe o campo e
devolve o enum (`ErroElf::Argumentos(e)`).

Depois, no laço que cria as 4 páginas da pilha, a primeira (`i == 1`, a mais
alta) recebe os bytes, no fim dela:

```rust
    for i in 1..=PILHA_ELF_PAGINAS {
        let q = quadros::alocar().ok_or(elf::ErroElf::SemMemoria)?;
        let pagina = paginacao::fisico_para_virtual(q.endereco()) as *mut u8;
        unsafe { core::ptr::write_bytes(pagina, 0, 4096) };
        if i == 1 {
            // A página mais alta recebe argc, argv e os textos, no fim dela
            // (`montar` garante que cabem). O kernel escreve pelo endereço
            // físico (deslocamento, doc 06): o espaço do programa não está ativo.
            let inicio = TAMANHO_QUADRO as usize - pilha.bytes.len();
            unsafe {
                core::ptr::copy_nonoverlapping(pilha.bytes.as_ptr(), pagina.add(inicio), pilha.bytes.len());
            }
        }
        espaco.mapear(PILHA_ELF_TOPO - i * TAMANHO_QUADRO, q,
                      flags::USUARIO | flags::ESCRITA | flags::SEM_EXECUCAO)
            .map_err(|_| elf::ErroElf::SemMemoria)?;
    }
```

E o RSP inicial do processo passa a ser o que `montar` calculou:

```rust
    // RSP = o começo de argc, alinhado em 16, como o System V manda na
    // entrada do programa. Até o doc 15 era topo - 8, para imitar um `call`;
    // desde o doc 16 o `_start` da lib sos é escrito à mão e acerta o
    // alinhamento ele mesmo antes de chamar o Rust.
    Ok(escalonador::criar(nome, espaco, entrada, pilha.rsp, segundo_plano))
```

Esse `topo - 8` do doc 13 merece a próxima seção.

## 4. O shell: separar o argv do `&`

`rodar` já tinha um argumento opcional, o `&` do segundo plano (doc 12).
Agora a linha é `rodar <programa> [argumentos...] [&]`, e o `&` só conta se
for a **última** palavra:

```rust
fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario;
    // Um `&` no fim pede segundo plano (doc 12); o resto é o argv do
    // programa (doc 16), com o nome em argv[0].
    let (argv, segundo_plano) = match argumentos.split_last() {
        Some((&"&", resto)) => (resto, true),
        _ => (argumentos, false),
    };
    let Some(&nome) = argv.first() else {
        kprintln!("uso: rodar <programa> [argumentos...] [&]   (veja 'programas' e 'ls')");
        return;
    };
```

`split_last` é o irmão de `split_first` (doc 09): devolve o último elemento
e o resto. O padrão `Some((&"&", resto))` só casa se o último for o texto
`"&"` (o `&` antes das aspas é porque o elemento vem por referência; ver
[§46](rust-basico.md#46-impl-iterator-padrões-em-slices-e-textos-vindos-de-bytes)).
Para `rodar primos 300 &`, `argumentos` é `["primos", "300", "&"]` e o
argv fica `["primos", "300"]`: o shell entrega ao programa exatamente o que
ele precisa, já com o nome na frente. Um shell de verdade faz o mesmo: o
`&` (e o `>`, o `|`...) são instruções **para o shell**, e o programa nunca
os vê.

Os programas embutidos do doc 10 (em assembly) têm uma página de pilha só e
não sabem ler argumentos. Em vez de aceitar e ignorar em silêncio, o shell
avisa:

```rust
    let pid = if let Some(programa) = usuario::programas::procurar(nome) {
        if argv.len() > 1 {
            // Escritos em assembly, com uma página de pilha só: não sabem
            // ler argumentos.
            kprintln!("'{}' e um programa embutido e nao recebe argumentos (os do disco recebem)", nome);
            return;
        }
        usuario::iniciar(programa, segundo_plano)
    } else if let Some(arquivo) = crate::ramdisk::abrir(nome) {
        match usuario::iniciar_elf(arquivo.nome, arquivo.dados, argv, segundo_plano) {
```

O texto da `ajuda` também mudou para caber numa linha de 80 colunas:
`rodar <prog> [args]   roda no anel 3; args: so os do disco; & no fim = 2o plano`.

## 5. O programa: um `_start` que sabe onde está o RSP

### 5.1 Por que uma função Rust comum não serve

Até o doc 15, a macro `principal!` gerava isto:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    $crate::sair($main())
}
```

Uma função Rust comum. O problema: os argumentos estão em `[RSP]` **no
instante em que `_start` começa**, e uma função comum não tem como saber
esse valor. O compilador gera um **prólogo** antes do corpo (empilha
registradores, reserva espaço com `sub rsp, N`), e o tamanho do prólogo
muda conforme o código e as otimizações. Quando o corpo roda, RSP já se
mexeu, e não há como perguntar "quanto valia antes".

É a mesma situação dos tratadores de interrupção e da troca de contexto
(docs 10, 12 e 14), com a mesma solução: uma função **naked**, só assembly,
sem prólogo ([§35](rust-basico.md#35-funções-naked-e-naked_asm)).

### 5.2 O `_start` novo

```rust
#[macro_export]
macro_rules! principal {
    ($main:path) => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn _start() -> ! {
            core::arch::naked_asm!(
                "mov rdi, rsp",           // 1º argumento: onde está argc
                "lea rsi, [rip + {main}]", // 2º argumento: a função main
                "and rsp, -16",           // alinhar em 16 (o kernel já alinha; garantia)
                "call {iniciar}",         // o call empilha 8 bytes: o Rust espera isso
                "ud2",                    // __iniciar não volta
                main = sym $main,
                iniciar = sym $crate::__iniciar,
            )
        }
    };
}
```

Linha a linha:

- **`mov rdi, rsp`**: o RSP de agora, intacto, aponta para `argc`. Pela
  convenção do System V (doc 14, [§37](rust-basico.md#37-convenções-de-chamada-extern-sysv64)), RDI é o 1º argumento de uma função.
- **`lea rsi, [rip + {main}]`**: o endereço da função `main` do programa, no
  2º argumento (RSI). `lea` ("carregar endereço efetivo") calcula o endereço
  sem ler a memória; `[rip + ...]` é relativo à instrução atual, então
  funciona com o `code-model=large` do doc 13 (que proíbe endereços
  absolutos de 32 bits, não os relativos). `{main}` vira o símbolo da função
  graças a `main = sym $main`.
- **`and rsp, -16`**: `-16` em binário é `...1111_0000`; o `and` apaga os 4
  bits de baixo, arredondando RSP para baixo até um múltiplo de 16. O kernel
  já entrega alinhado (seção 3.2), mas o `_start` não deveria depender disso.
- **`call {iniciar}`**: empilha 8 bytes (o endereço de volta) e pula. Assim,
  na entrada de `__iniciar`, RSP é "múltiplo de 16 menos 8", **exatamente** o
  que o compilador supõe em toda função. Esse é o motivo do `topo - 8` do doc
  13: lá, sem assembly, o kernel imitava o `call` que nunca aconteceu.
  Agora o `call` acontece de verdade, e o kernel pode seguir o System V à
  risca (RSP múltiplo de 16 na entrada do processo).
- **`ud2`**: uma instrução inválida de propósito. `__iniciar` nunca volta
  (`-> !`), mas, se voltasse, o programa morreria com #UD (doc 04) em vez de
  executar o que viesse depois na memória.

Os dois `sym` usam caminhos que a macro recebe ou conhece: `$main` é a
função que o programa passou para `principal!(main)`, e `$crate` é "a crate
onde a macro foi definida" (a `sos`), não importa em que programa ela é
usada ([§46](rust-basico.md#46-impl-iterator-padrões-em-slices-e-textos-vindos-de-bytes)).

### 5.3 Guardar argc e argv

```rust
/// argc e argv, guardados por `__iniciar` antes de `main`. Atômicos só para
/// não precisar de `static mut`: o programa tem uma thread só.
static ARGC: AtomicUsize = AtomicUsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());

/// Chamada pelo `_start` de `principal!`. Não é para os programas usarem.
#[doc(hidden)]
pub unsafe extern "C" fn __iniciar(pilha: *const u64, main: fn() -> u64) -> ! {
    // pilha[0] = argc; pilha[1..] = argv[0], argv[1], ..., 0.
    unsafe {
        ARGC.store(*pilha as usize, Ordering::Relaxed);
        ARGV.store(pilha.add(1) as *mut *const u8, Ordering::Relaxed);
    }
    sair(main())
}
```

`__iniciar` é Rust comum, e por isso pode receber o RSP antigo como
argumento: o assembly já o pôs em RDI. Ela guarda `argc` (o número em
`pilha[0]`) e o endereço de `argv[0]` (`pilha + 1`, isto é, 8 bytes acima:
`add` em ponteiro conta em elementos, não em bytes) e chama `main`, como o
`_start` antigo fazia.

Os dois `static` são atômicos ([§25](rust-basico.md#25-atômicos-atomicu64-atomicbool)) não por causa de concorrência (o
programa tem uma thread só), mas porque Rust não deixa mudar um `static`
comum sem `unsafe` em todo lugar que o lê (`static mut`). `Relaxed` basta:
não há outra thread para quem a ordem importe ([§28](rust-basico.md#28-orderingacquire-e-release)). O `AtomicPtr<*const u8>`
guarda um "ponteiro para ponteiros para bytes", o `char **argv` do C.

O tipo de `main` é `fn() -> u64` ([§31](rust-basico.md#31-ponteiros-de-função-fn)): um ponteiro de função que não
recebe nada e devolve o código de saída. Assim, os programas que já existiam
(`oi`, `dados`, `soneca`...) não mudam uma linha; os argumentos ficam
disponíveis para quem quiser perguntar.

### 5.4 Perguntar pelos argumentos

```rust
/// Quantos argumentos o programa recebeu, contando o nome (argv[0]).
pub fn quantos_argumentos() -> usize {
    ARGC.load(Ordering::Relaxed)
}

/// O argumento `i` (0 = nome do programa), ou `None` se não existe.
/// Como o `sys.argv[i]` do Python, mas sem estourar: devolve `None`.
pub fn argumento(i: usize) -> Option<&'static str> {
    if i >= quantos_argumentos() {
        return None;
    }
    // O kernel garante: argv[i] aponta para um texto terminado em 0, que
    // fica na pilha até o programa acabar (por isso 'static).
    unsafe {
        let inicio = *ARGV.load(Ordering::Relaxed).add(i);
        let mut tamanho = 0;
        while *inicio.add(tamanho) != 0 {
            tamanho += 1;
        }
        let bytes = core::slice::from_raw_parts(inicio, tamanho);
        Some(core::str::from_utf8(bytes).unwrap_or("?"))
    }
}

/// Todos os argumentos, em ordem, como o `std::env::args()` do Rust.
pub fn argumentos() -> impl Iterator<Item = &'static str> {
    (0..quantos_argumentos()).filter_map(argumento)
}
```

`argumento(i)` faz o que o C faz com `strlen`: anda byte a byte até achar o
0, e então transforma os bytes num `&str`. Dois pontos de Rust:

- **`'static`**: o texto vive na pilha inicial, que nunca é desempilhada (ela
  está **acima** de tudo o que o programa empilha depois). Ele existe até o
  programa acabar, e `'static` diz exatamente isso ao compilador.
- **`str::from_utf8`**: um `&str` em Rust tem de ser UTF-8 válido. Os bytes
  vieram de fora (do kernel); `from_utf8` confere e devolve `Err` se não
  forem. Aqui, o `?` substitui um texto inválido (o shell só manda ASCII, que
  é UTF-8 válido, então não acontece hoje).

`argumentos()` devolve `impl Iterator<Item = &'static str>`: "algum tipo que
é um iterador de textos", sem dizer qual ([§46](rust-basico.md#46-impl-iterator-padrões-em-slices-e-textos-vindos-de-bytes)). Com isso, o programa usa os
argumentos com `for`, `enumerate`, `skip(1)` etc., como faria com o
`std::env::args()`.

## 6. Dois programas

### 6.1 `args`: mostrar o que chegou

```rust
//! Mostra os argumentos que recebeu (docs/16-argumentos.md) e sai com
//! quantos foram, contando o nome: `rodar args a bb ccc` sai com 4.
#![no_std]
#![no_main]

use sos::{println, principal};

principal!(main);

fn main() -> u64 {
    let local = 0u64; // uma variável na pilha, para comparar os endereços
    println!("argc = {} (uma variavel local esta em {:#x})", sos::quantos_argumentos(), &local as *const u64 as u64);
    for (i, a) in sos::argumentos().enumerate() {
        // O endereço mostra onde o kernel deixou o texto: no topo da pilha.
        println!("argv[{}] = \"{}\" (em {:#x})", i, a, a.as_ptr() as u64);
    }
    sos::quantos_argumentos() as u64
}
```

O código de saída é `argc`, para os testes automáticos conferirem sem ler a
tela. Os endereços servem para ver, na tela, o desenho da seção 3.3.

### 6.2 `primos`: o limite vem do argumento

```rust
fn main() -> u64 {
    let limite = match sos::argumento(1) {
        None => 10_000,
        Some(texto) => match texto.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                println!("uso: primos [limite]   ('{}' nao e um numero)", texto);
                return u64::MAX; // o shell mostra -1
            }
        },
    };
    let inicio = sos::tiques();
    let mut quantos = 0;
    print!("primeiros primos:");
    for n in 0..limite {
```

Sem argumento, conta abaixo de 10 000, como antes (o teste do doc 13 que
espera 1229 continua valendo). `texto.parse::<u64>()` é o `Long.parseLong`
do Java, mas sem exceção: devolve `Result` ([§6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)). Um texto que não é número
termina com o código `u64::MAX`, que o shell mostra como `-1` (ele imprime o
código como `i64`, doc 10).

## 7. Como foi testado

### 7.1 Pelo shell

Saída real da serial, com o QEMU recebendo as teclas pelo monitor:

```
silicon> rodar args um dois tres
argc = 4 (uma variavel local esta em 0x7fffffffff60)
argv[0] = "args" (em 0x7fffffffffee)
argv[1] = "um" (em 0x7ffffffffff3)
argv[2] = "dois" (em 0x7ffffffffff6)
argv[3] = "tres" (em 0x7ffffffffffb)
'args' terminou: codigo de saida 4
silicon> rodar args
argc = 1 (uma variavel local esta em 0x7fffffffff80)
argv[0] = "args" (em 0x7ffffffffffb)
'args' terminou: codigo de saida 1
```

Os endereços dos textos são os da seção 3.3. A variável local do `main`
está **abaixo** deles (`...ff60` < `...ffb0`, o RSP inicial): o programa
empilha tudo o que faz depois por baixo da área dos argumentos, que fica
intocada. Com um argumento só, a área é menor e a variável local fica mais
alta (`...ff80`).

O `primos`, com e sem um número válido, e dois em segundo plano:

```
silicon> rodar primos 5000
primeiros primos: 2 3 5 7 11 13 17 19 23 29
669 primos abaixo de 5000 (1 tiques)
'primos' terminou: codigo de saida 669
silicon> rodar primos xyz
uso: primos [limite]   ('xyz' nao e um numero)
'primos' terminou: codigo de saida -1
silicon> rodar ola extra
'ola' e um programa embutido e nao recebe argumentos (os do disco recebem)
silicon> rodar primos 300 &
[7] 'primos' em segundo plano
silicon> primeiros primos: 2 3 5 7 11 13 17 19 23 29
62 primos abaixo de 300 (2 tiques)

[7] 'primos' terminou: codigo de saida 62
```

O `&` não chegou ao programa (ele recebeu `["primos", "300"]`) e continuou
valendo para o shell. Os programas que não leem argumentos também os recebem
e simplesmente os ignoram: `rodar oi a b` roda o `oi` normalmente.

### 7.2 Olhando a memória do programa

Os endereços impressos pelo `args` mostram onde os textos estão, mas não a
tabela de ponteiros. Para ver a pilha inteira, foi usado o monitor do QEMU
(doc 09), que lê a memória **pelo CR3 atual**. O truque é rodar um programa
que gaste CPU por um tempo, para que o CR3 ativo seja o dele: o `insone` (doc
15) faz espera ocupada por uns 2 s. E desde este capítulo **todo** ELF recebe
argv, inclusive o `insone`, que não o lê. Com `rodar insone a bb` rodando,
os comandos do monitor `x /6gx 0x7fffffffffc0` e `x /64xb 0x7fffffffffc0`
mostraram:

```
00007fffffffffc0: 0x0000000000000003 0x00007ffffffffff4
00007fffffffffd0: 0x00007ffffffffffb 0x00007ffffffffffd
00007fffffffffe0: 0x0000000000000000 0x0000000000000000
...
00007ffffffffff0: 0x00 0x00 0x00 0x00 0x69 0x6e 0x73 0x6f
00007ffffffffff8: 0x6e 0x65 0x00 0x61 0x00 0x62 0x62 0x00
```

Lendo: `argc = 3`; três ponteiros (`...fff4`, `...fffb`, `...fffd`); os
dois zeros (fim de `argv` e de `envp`); 4 bytes de folga (`...fff0` a
`...fff3`); e os textos, em ASCII: `69 6e 73 6f 6e 65` = "insone", `00`,
`61` = "a", `00`, `62 62` = "bb", `00`. A conta da seção 3.2 prevê
exatamente isso: textos de 7 + 2 + 3 = 12 bytes (`topo - 12 = ...fff4`), 6
palavras (48 bytes), `...fff4 - 48 = ...ffc4`, arredondado para `...ffc0`.

### 7.3 `cargo test`

Quatro testes novos (57 no total, todos passando):

```
siliconos::usuario::argumentos::testes::argumentos_ruins_recusados ... [ok]
siliconos::usuario::argumentos::testes::pilha_inicial_no_formato_system_v ... [ok]
siliconos::usuario::elf::testes::argumentos_chegam_ao_programa ... [ok]
siliconos::usuario::elf::testes::argumentos_demais_recusados_sem_vazar ... [ok]
Todos os 57 testes passaram.
```

Os dois primeiros testam `montar` sozinha, sem criar processo:

```rust
    #[test_case]
    fn pilha_inicial_no_formato_system_v() {
        let topo = 0x0000_8000_0000_0000;
        let p = montar(&["primos", "5000"], topo).unwrap();
        assert_eq!(p.rsp % 16, 0);
        assert_eq!(p.rsp + p.bytes.len() as u64, topo);
        assert_eq!(ler_u64(&p, p.rsp), 2);                 // argc
        assert_eq!(ler_u64(&p, p.rsp + 24), 0);            // argv[2] = 0
        assert_eq!(ler_u64(&p, p.rsp + 32), 0);            // envp[0] = 0
        // Os textos ficam colados no topo: "primos\0" (7) + "5000\0" (5).
        assert_eq!(ler_u64(&p, p.rsp + 8), topo - 12);
        assert_eq!(ler_u64(&p, p.rsp + 16), topo - 5);
        assert_eq!(&p.bytes[p.bytes.len() - 12..], b"primos\x005000\x00");
    }
```

O segundo confere cada erro e o **limite exato** da página:

```rust
        // O maior que cabe: 4096 bytes no total. 5 palavras (argc, 2
        // ponteiros, os 2 zeros: 40 bytes) + "p\0" (2) + o texto grande com
        // o 0 (4054) = 4096, já alinhado. Um byte a mais e não cabe.
        let cabe: String = core::iter::repeat_n('x', 4053).collect();
        assert_eq!(montar(&["p", &cabe], topo).unwrap().bytes.len(), 4096);
        let nao_cabe: String = core::iter::repeat_n('x', 4054).collect();
        assert_eq!(montar(&["p", &nao_cabe], topo).unwrap_err(), ErroArgumentos::GrandesDemais);
```

Uma história honesta: a primeira versão deste teste contava **4** palavras
(esquecendo um dos zeros) e falhou no QEMU com `GrandesDemais`. Foi o teste
que achou o erro de conta, não o código: é para isso que se testa a
fronteira (o maior que cabe e o menor que não cabe), e não só um caso
confortável no meio.

Os outros dois rodam programas de verdade:

```rust
    #[test_case]
    fn argumentos_chegam_ao_programa() {
        assert_eq!(rodar_com(&["args"]), Resultado::Saiu(1));
        assert_eq!(rodar_com(&["args", "a", "bb", "ccc"]), Resultado::Saiu(4));
        assert_eq!(rodar_com(&["primos", "100"]), Resultado::Saiu(25));
        assert_eq!(rodar_com(&["primos", "abc"]), Resultado::Saiu(u64::MAX));
    }

    #[test_case]
    fn argumentos_demais_recusados_sem_vazar() {
        use crate::usuario::argumentos::{ErroArgumentos, MAX_ARGUMENTOS};
        let livres = quadros::livres();
        let arquivo = ramdisk::abrir("args").unwrap();
        let r = crate::usuario::iniciar_elf(arquivo.nome, arquivo.dados, &["args"; MAX_ARGUMENTOS + 1], false);
        assert_eq!(r, Err(ErroElf::Argumentos(ErroArgumentos::Demais)));
        assert_eq!(quadros::livres(), livres);
    }
```

O último confere também que a recusa não vaza quadros (nenhum chega a ser
alocado, porque `montar` roda primeiro). `&["args"; MAX_ARGUMENTOS + 1]` é
um array com 33 cópias do mesmo texto.

### 7.4 Regressão

As 25 builds (a normal e as 24 features de teste e diagnóstico) se
comportam como antes: cada exceção provocada aparece com a mesma mensagem,
os testes de memória, heap e timer imprimem "OK", e `teste-deadlock` trava
de propósito.

---

## O que ainda falta

- **Variáveis de ambiente.** O `envp` existe no formato, mas vazio. Um
  ambiente (`CAMINHO=/bin`, `IDIOMA=pt`) seria a mesma conta com mais textos;
  falta quem os guarde (o shell) e um uso para eles.
- **Um programa iniciar outro.** Aqui, só o shell (dentro do kernel) cria
  processos, e os argumentos vêm de memória do kernel, em que se pode
  confiar. Uma syscall como o `execve` do Linux receberia o argv **do anel
  3**: cada ponteiro, e cada texto até o 0, teria de passar por
  `faixa_do_programa` (doc 10) antes de ser copiado, e o tamanho total teria
  de ser limitado antes da cópia, não depois.
- **Aspas.** O shell separa a linha por espaços (`split_whitespace`):
  `rodar args "um dois"` entrega dois argumentos, `"um` e `dois"`. Um shell
  de verdade trata aspas e `\` antes de montar o argv.
- **O *auxiliary vector*.** Só faria falta com um carregador dinâmico ou uma
  libc de verdade, que o procuram logo depois do `envp`.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 15 (tag `doc-15`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-16`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-15...doc-16).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/usuario/argumentos.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | **novo** (seção 3): `MAX_ARGUMENTOS`, `ErroArgumentos`, `PilhaInicial`, `montar`; 2 testes | — |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/mod.rs) | `pub mod argumentos`; `iniciar_elf` recebe o argv, monta e copia a pilha inicial, RSP = `pilha.rsp` (seção 3.4) | topo; `iniciar_elf` |
| [`src/usuario/elf.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | `ErroElf::Argumentos`; 2 testes e o ajudante `rodar_com`; `rodar` passa `&[nome]` | fim de `ErroElf`; testes |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/shell.rs) | `rodar_programa` separa o `&` e passa o argv; recusa argumentos para embutidos; texto da `ajuda` | `COMANDOS`; `rodar_programa` |
| [`src/escalonador.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/escalonador.rs), [`src/usuario/syscall.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/syscall.rs) | testes: as chamadas de `iniciar_elf` passam o argv | testes |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/ramdisk.rs) | teste: 7 nomes (`args` é o primeiro) | testes |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | `principal!` gera um `_start` naked; `ARGC`, `ARGV`, `__iniciar`, `quantos_argumentos`, `argumento`, `argumentos` (seção 5) | `use`; onde ficava `principal!` |
| [`programas/src/bin/args.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/bin/args.rs) | **novo** (seção 6.1) | — |
| [`programas/src/bin/primos.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/bin/primos.rs) | o limite vem de `argumento(1)` (seção 6.2) | `main` |

### Os trechos que o texto não mostrou

O `use` novo de `programas/src/lib.rs`:

```rust
use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
```

Os `use` de `src/usuario/argumentos.rs`:

```rust
use alloc::vec;
use alloc::vec::Vec;

use crate::memoria::TAMANHO_QUADRO;
```

O ajudante dos testes de `argumentos.rs`:

```rust
    fn ler_u64(p: &PilhaInicial, endereco: u64) -> u64 {
        let i = (endereco - p.rsp) as usize;
        u64::from_le_bytes(p.bytes[i..i + 8].try_into().unwrap())
    }
```

O começo de `argumentos_ruins_recusados`:

```rust
    #[test_case]
    fn argumentos_ruins_recusados() {
        let topo = 0x0000_8000_0000_0000;
        assert_eq!(montar(&[], topo).unwrap_err(), ErroArgumentos::Vazio);
        assert_eq!(montar(&["a"; MAX_ARGUMENTOS + 1], topo).unwrap_err(), ErroArgumentos::Demais);
        assert_eq!(montar(&["a\0b"], topo).unwrap_err(), ErroArgumentos::TemZero);
        let grande: String = core::iter::repeat_n('x', 4096).collect();
        assert_eq!(montar(&["p", &grande], topo).unwrap_err(), ErroArgumentos::GrandesDemais);
```

O ajudante `rodar_com`, em `elf.rs`:

```rust
    /// Roda um programa do disco com estes argumentos (doc 16).
    fn rodar_com(argv: &[&str]) -> Resultado {
        let arquivo = ramdisk::abrir(argv[0]).unwrap();
        escalonador::esperar(crate::usuario::iniciar_elf(arquivo.nome, arquivo.dados, argv, false).unwrap())
    }
```

As outras chamadas de `iniciar_elf` (nos testes) só ganharam o argv:
`&[nome]` em `elf.rs` e `escalonador.rs` (que agora recebe `nome:
&'static str`) e `&["medir"]` em `syscall.rs`. A linha da tabela de
comandos:

```rust
    Comando { nome: "rodar", uso: "rodar <prog> [args]", descricao: "roda no anel 3; args: so os do disco; & no fim = 2o plano", funcao: rodar_programa },
```

O resto de `rodar_programa` (o tratamento de erro e o segundo plano) é o do
doc 13. O teste do disco:

```rust
        assert_eq!(nomes, ["args", "dados", "insone", "medir", "oi", "primos", "soneca"]);
```

E o fim de `primos.rs`, que só trocou o número fixo pelo `limite`:

```rust
    for n in 0..limite {
        if primo(n) {
            quantos += 1;
            if quantos <= 10 {
                print!(" {}", n);
            }
        }
    }
    println!();
    println!("{} primos abaixo de {} ({} tiques)", quantos, limite, sos::tiques() - inicio);
    quantos
}
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `montar` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Monta num vetor o topo da pilha inicial de um programa (argc, argv, zeros, textos) e calcula o RSP, alinhado em 16; recusa argumentos vazios, demais, grandes ou com o byte 0. |
| `_start` (de `principal!`) | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | (naked) Ponto de entrada do programa: passa o RSP inicial e `main` para `__iniciar`, com a pilha alinhada. |
| `__iniciar` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | Guarda argc e argv lidos da pilha inicial, chama `main` e sai com o valor devolvido. |
| `quantos_argumentos` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | argc: quantos argumentos, contando o nome do programa. |
| `argumento` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | O argumento `i` como `&'static str` (anda até o byte 0), ou `None` se não existe. |
| `argumentos` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/lib.rs) | Iterador sobre todos os argumentos, como o `std::env::args()`. |
| `main` (args) | [programas/src/bin/args.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/programas/src/bin/args.rs) | Mostra argc e cada argumento com o endereço; sai com argc. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `ler_u64` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Ajudante: lê o número de 8 bytes que vai morar num endereço da pilha montada. |
| `pilha_inicial_no_formato_system_v` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Teste: argc, ponteiros, zeros e textos nos lugares certos, RSP alinhado em 16. |
| `argumentos_ruins_recusados` | [usuario/argumentos.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/argumentos.rs) | Teste: cada erro de `montar`, e o limite exato de uma página (4053 cabe, 4054 não). |
| `rodar_com` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | Ajudante: roda um programa do disco com um argv e espera o resultado. |
| `argumentos_chegam_ao_programa` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | Teste: `args` sai com argc; `primos 100` sai com 25; `primos abc` sai com -1. |
| `argumentos_demais_recusados_sem_vazar` | [usuario/elf.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-16/src/usuario/elf.rs) | Teste: 33 argumentos são recusados por `iniciar_elf` sem alocar nenhum quadro. |

---

## Diferença em relação ao livro

O livro não passa argumentos aos programas. O formato usado aqui é o da ABI
System V para x86-64, o mesmo do Linux, com duas simplificações: o ambiente
é vazio e não há *auxiliary vector*. A área dos argumentos se limita à
página mais alta da pilha (4 KiB).

## Resumo

- Os argumentos de um programa não são da linguagem: o **kernel** os entrega
  ao criar o processo, e a biblioteca (`std`, libc, aqui a `sos`) só os
  encontra.
- No System V (Linux e SiliconOS), o kernel os **copia para o topo da pilha**
  do programa: os textos terminados em 0 lá em cima e, abaixo, `argc`, os
  ponteiros de `argv`, um 0, os de `envp` e outro 0. RSP chega apontando para
  `argc`, alinhado em 16.
- O kernel monta tudo num vetor, com os ponteiros já em endereços do
  programa, e copia para a página mais alta da pilha pelo endereço físico.
- O `_start` agora é **naked**: só assembly sabe o RSP no instante da
  entrada. Ele o passa a `__iniciar` (Rust comum), que guarda argc/argv e
  chama `main`. O `call` dele empilha os 8 bytes que o compilador espera, e o
  `topo - 8` do doc 13 deixou de ser necessário.
- O `&` é do shell, não do programa: `rodar primos 300 &` entrega
  `["primos", "300"]`.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que o kernel precisa copiar os textos dos argumentos, em vez de
   passar ao programa um ponteiro para a linha que o shell montou?**
   Porque a linha está no heap do kernel, que o programa não pode ler: as
   páginas do kernel não têm o bit USUARIO no espaço do programa (doc 11). Um
   ponteiro para lá daria page fault. Os textos precisam estar em páginas do
   próprio programa.

2. **Um programa foi chamado como `rodar args x`. Quanto vale `argc`, e o
   que há em `argv[2]`?**
   `argc = 2` (`"args"` e `"x"`). `argv[2]` é o 0 que fecha a lista: o
   formato garante que `argv[argc]` é sempre 0, então dá para percorrer a
   lista sem saber `argc`, parando no 0.

3. **Por que `montar` arredonda o RSP para baixo, e não para cima, até um
   múltiplo de 16?**
   Para cima, o RSP subiria para dentro da área dos textos, e `argc` e os
   ponteiros seriam escritos por cima deles. Para baixo, sobra uma folga de
   0 a 15 bytes entre as palavras e os textos, que não atrapalha ninguém.

4. **Por que o `_start` não pode ser uma função Rust comum que lê o RSP com
   um `asm!("mov {}, rsp")` na primeira linha?**
   Porque, quando a primeira linha do corpo roda, o prólogo gerado pelo
   compilador já pode ter mudado o RSP (empilhado registradores, reservado
   espaço para variáveis), e o tamanho do prólogo não é garantido. Só uma
   função naked, sem prólogo, vê o RSP exatamente como o kernel o deixou.

5. **O kernel entrega RSP múltiplo de 16. Na entrada de `__iniciar`, quanto
   vale RSP módulo 16, e por quê?**
   8. O `call` empilha o endereço de volta (8 bytes) antes de pular, e o
   compilador supõe isso em toda função: na entrada, RSP + 8 é múltiplo de
   16. Com o RSP desalinhado de outro jeito, uma instrução como `movaps` com
   um endereço na pilha daria #GP.

6. **Por que `argumento` pode devolver `&'static str`, se o texto está na
   pilha, e variáveis na pilha morrem quando a função volta?**
   Porque esse texto não pertence a nenhuma função: o kernel o pôs acima do
   RSP inicial, antes da primeira instrução. Tudo o que o programa empilha
   fica abaixo, e nada desempilha a área dos argumentos. Ela vive até o
   programa acabar, que é o que `'static` significa.

7. **Se um programa pudesse iniciar outro com uma syscall, por que o kernel
   não poderia simplesmente chamar `montar` com o argv que o programa
   passou?**
   Porque esse argv viria do anel 3: os ponteiros e os textos poderiam
   apontar para o kernel, para páginas não mapeadas ou não terminar nunca em
   0. Antes de copiar, o kernel teria de validar cada ponteiro e cada texto
   com `faixa_do_programa` e limitar o tamanho total. O shell de hoje roda no
   kernel, e os textos dele são de confiança.

</details>
