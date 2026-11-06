# Rust para quem já programa (só o que o SiliconOS usa)

Este guia não ensina Rust inteiro. Ele explica **apenas os recursos que
aparecem no código do SiliconOS**, comparando com linguagens que você já
conhece (Java, Kotlin, JavaScript/TypeScript, Python). Os capítulos remetem
para cá sempre que um recurso aparece pela primeira vez.

Se algo daqui parecer abstrato, pule e volte quando o capítulo pedir.

---

## 1. Tipos numéricos: o tamanho é explícito

Em Java você tem `int` (32 bits) e `long` (64 bits). Em Python, inteiros não
têm tamanho. Em Rust, **todo inteiro diz quantos bits tem e se tem sinal**:

| Rust | Significado | Equivalente aproximado |
|---|---|---|
| `u8` | sem sinal, 8 bits (0 a 255) | `byte` sem sinal |
| `u16` | sem sinal, 16 bits | `char` do Java (numérico) |
| `u32` | sem sinal, 32 bits | `int` sem sinal |
| `u64` | sem sinal, 64 bits | `long` sem sinal |
| `i32`, `i64` | com sinal | `int`, `long` |
| `usize` | sem sinal, do tamanho de um ponteiro (64 bits aqui) | usado para índices e tamanhos |

Num kernel isso é essencial: o hardware exige campos de tamanho exato
("este campo tem 16 bits"), então os tipos mapeiam um para um com o
manual da Intel.

Números podem ter `_` para facilitar a leitura e prefixos de base:

```rust
0x00FF_FFFF        // hexadecimal (o _ é só visual)
0b0000_1100        // binário
5 * 4096           // expressões normais
```

**Conversões são sempre explícitas**, com `as`:

```rust
let x: u64 = 300;
let y = x as u16;   // como um cast em Java: (short) x
let z = x as u8;    // 300 não cabe em 8 bits: vira 44 (descarta os bits altos)
```

Não existe conversão automática entre `u32` e `u64`, como existe de `int`
para `long` em Java. Isso parece chato no começo, mas evita bugs sutis de
truncamento.

---

## 2. Variáveis: imutáveis por padrão

```rust
let a = 5;          // imutável: como `val` do Kotlin ou `final` do Java
let mut b = 5;      // mutável: como `var` do Kotlin
b += 1;             // ok
// a += 1;          // ERRO de compilação
```

O tipo geralmente é inferido, como em Kotlin e TypeScript. Quando
precisa, ele vai depois do nome: `let x: u64 = 0;`.

---

## 3. Funções e o tipo `!` ("nunca retorna")

```rust
fn soma(a: u32, b: u32) -> u32 {
    a + b           // a última expressão SEM ponto e vírgula é o retorno
}
```

Como em Kotlin, quase tudo é expressão: um bloco `{ ... }` "vale" o valor
da sua última linha.

O SiliconOS usa muito um tipo especial: **`!`**, chamado *never*.

```rust
fn kernel_main(...) -> ! {
    ...
    loop { ... }    // loop infinito: nunca sai
}
```

`-> !` significa **"esta função nunca retorna"**. Não é `void` (que
retorna sem valor); é uma promessa verificada pelo compilador de que a
função termina num loop infinito, para a máquina ou chama outra função
`-> !`. Num kernel isso aparece o tempo todo: não existe "para onde
voltar" depois que o kernel começa.

---

## 4. Structs e `impl`: classes sem herança

Rust não tem classes. Tem **structs** (dados) e blocos **`impl`** (métodos
para esses dados):

```rust
pub struct Console {        // como uma classe Java só com campos
    col: usize,
    row: usize,
    pub cor: u32,           // pub = público; sem pub = privado ao módulo
}

impl Console {
    pub fn new(...) -> Self { ... }        // "construtor" é só convenção de nome
    fn cols(&self) -> usize { ... }        // método que só lê o objeto
    pub fn write_byte(&mut self, b: u8) { ... }  // método que altera o objeto
}
```

- **`Self`** = o próprio tipo (aqui, `Console`).
- **`&self`** = como o `this` do Java, mas somente leitura.
- **`&mut self`** = `this` com permissão de alterar campos.
- Uma função dentro de `impl` sem `self` é como um método `static`:
  chamada com `Console::new(...)`.

---

## 5. Traits: interfaces

Uma **trait** é como uma `interface` do Java/Kotlin. O SiliconOS implementa a
trait da biblioteca padrão `core::fmt::Write`, que exige um único método:

```rust
impl fmt::Write for Serial {           // "Serial implements Write"
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() { ... }
        Ok(())
    }
}
```

Assim como interfaces Java podem ter métodos `default`, a trait `Write`
traz de graça o método `write_fmt`, que formata texto (`"{} + {}"`) e
chama o nosso `write_str`. **Implementamos 1 método e ganhamos a
formatação inteira.** É por isso que `writeln!(Serial, "x = {}", x)`
funciona.

`impl Trait` em parâmetro significa "qualquer tipo que implemente esta
trait", como um genérico com restrição (`<T extends MemoryMap>` em Java):

```rust
fn kernel_main(fb: FrameBuffer, memory_map: &impl MemoryMap) -> ! { ... }
```

---

## 6. `Option` e `Result`: sem `null` e sem exceções

Rust **não tem `null`** nem exceções (`try/catch`). No lugar:

- **`Option<T>`** é `Some(valor)` ou `None`. É o `Optional<T>` do Java ou
  o `T?` do Kotlin.
- **`Result<T, E>`** é `Ok(valor)` ou `Err(erro)`. É um "retorno que pode
  ser erro", que você é obrigado a examinar.

```rust
static CONSOLE: Mutex<Option<Console>> = ...;   // pode não existir ainda

if let Some(con) = CONSOLE.lock().as_mut() {    // "se existir, chame de con"
    con.write_fmt(args);
}
```

`if let Some(x) = valor` é como `valor?.let { x -> ... }` do Kotlin.

**`.unwrap()`** significa "eu garanto que é `Some`/`Ok`; se não for,
aborte". Se falhar, gera um **panic** (capítulo 02). Usamos `unwrap()` na
fase de boot, em que não há o que fazer se o firmware falhar.

`let _ = algo();` significa "ignoro o resultado de propósito". O
compilador avisa se você ignorar um `Result` sem dizer nada.

---

## 7. `match`: um `switch` que obriga a cobrir tudo

```rust
let origem = match (codigo >> 1) & 0b11 {
    0b00 => "GDT",
    0b10 => "LDT",
    _    => "IDT",       // _ = "qualquer outro valor" (o default)
};
```

Parecido com o `when` do Kotlin. O `match` é uma expressão (devolve um
valor), e o compilador **exige** que todos os casos possíveis sejam
cobertos.

---

## 8. Referências e o "borrow checker" (versão curtíssima)

Em Java, objetos são sempre acessados por referência e o coletor de lixo
cuida da memória. Rust **não tem coletor de lixo**. Em vez dele, o
compilador acompanha quem é "dono" de cada valor e quem o pegou
emprestado:

- `&x`: empréstimo **somente leitura** (pode haver vários ao mesmo tempo);
- `&mut x`: empréstimo **com escrita** (só um por vez, e nenhum de
  leitura junto).

Essa regra ("ou muitos leitores, ou um escritor") é verificada **em tempo
de compilação** e elimina uma classe inteira de bugs (dois trechos
alterando o mesmo dado ao mesmo tempo). É por isso que o SiliconOS usa um
`Mutex` para a variável global do console (seção 11).

Quando um valor sai de escopo, ele é destruído automaticamente (o `drop`).
É como o `try-with-resources` do Java, só que para tudo. O SiliconOS depende
disso, por exemplo, para liberar o lock do console sozinho.

---

## 9. Ponteiros crus e `unsafe`

Referências (`&T`) são sempre válidas: o compilador garante. Mas um
kernel precisa fazer coisas que o compilador **não tem como verificar**,
como "escreva no endereço `0x80000000`, que eu sei que é a tela". Para
isso existem os **ponteiros crus**:

```rust
*mut u32     // ponteiro para u32, com escrita (como `uint32_t*` em C)
*const u64   // ponteiro somente leitura
```

Ponteiros crus podem ser criados livremente, mas **só podem ser usados
dentro de um bloco `unsafe`**:

```rust
unsafe {
    self.fb.base.add(y * self.fb.stride + x).write_volatile(cor);
}
```

`unsafe` **não desliga** as verificações do Rust. Ele libera apenas
algumas operações extras (usar ponteiro cru, chamar função `unsafe`,
executar assembly, mexer em `static mut`) e significa: **"compilador, aqui
quem garante a correção sou eu"**. A boa prática, seguida no SiliconOS, é
manter os blocos `unsafe` pequenos e comentados, e expor funções seguras
por cima. Exemplo: `outb` é `unsafe`, mas `Serial::write_byte` não é, então
quem usa a serial não precisa se preocupar.

Uma **`unsafe fn`** é uma função que só pode ser chamada dentro de
`unsafe { }`, porque quem chama precisa garantir alguma condição.

---

## 10. `static` e `const`

```rust
const ESCALA: usize = 2;              // constante: substituída no código, como #define
static CONSOLE: Mutex<...> = ...;     // variável global: um único endereço na memória
static mut PILHA: [u8; 20480] = ...;  // global MUTÁVEL: todo uso exige unsafe
```

Em Java, `static` é um campo da classe. Em Rust, `static` é uma
**variável global com endereço fixo** que vive o programa inteiro. Isso
importa para a CPU, que guarda **endereços** de tabelas (GDT, IDT): elas
precisam ficar no mesmo lugar para sempre.

`static mut` é perigoso, porque qualquer parte do código poderia alterá-la
ao mesmo tempo. Por isso o Rust exige `unsafe` em todo acesso e
recomenda envolver globais num `Mutex` ou `Once`.

**`'static`** (com apóstrofo) é um *tempo de vida* que significa "válido
até o fim do programa". `&'static T` é uma referência que nunca fica
inválida.

---

## 11. `Mutex` e `Once` (da crate `spin`)

- **`Mutex<T>`** protege um dado compartilhado, como o `synchronized` do
  Java. `CONSOLE.lock()` devolve um "guarda"; enquanto ele existir, ninguém
  mais pega o lock. Quando o guarda sai de escopo, o lock é liberado
  sozinho (seção 8).
  - O `Mutex` da biblioteca padrão põe a thread para *dormir* pedindo ao
    sistema operacional, mas **nós somos o sistema operacional**. A crate
    `spin` oferece um *spinlock*: quem quer o lock fica em loop tentando
    até conseguir.
- **`Once<T>`** guarda um valor que é calculado **uma única vez**, na
  primeira chamada de `call_once`, como um `lazy` do Kotlin ou um
  singleton. Devolve `&'static T`.

---

## 12. Módulos, crates, Cargo e features

| Rust | Analogia |
|---|---|
| **crate** | um pacote/biblioteca (como um `.jar` ou um pacote npm) |
| **Cargo** | o gerenciador de build e dependências (Maven/Gradle/npm juntos) |
| `Cargo.toml` | o `pom.xml`/`package.json` |
| `mod serial;` | "existe um módulo no arquivo `serial.rs`" (o Rust não descobre arquivos sozinho) |
| `use spin::Mutex;` | `import` |
| `crate::gdt` | caminho a partir da raiz do nosso projeto |
| `pub` | `public`; sem `pub` é privado ao módulo |

**Features** são opções de compilação declaradas no `Cargo.toml`:

```toml
[features]
teste-pagefault = []
```

No código, `#[cfg(feature = "teste-pagefault")]` faz o bloco seguinte
**só existir** quando a feature é ligada (`cargo build --features
teste-pagefault`). É o `#ifdef` do C. O código de teste nem entra no
binário normal.

---

## 13. Atributos: `#[...]` e `#![...]`

São como as *annotations* do Java (`@Override`):

| Atributo | Efeito |
|---|---|
| `#![no_std]` | (vale para o crate todo, por causa do `!`) não use a biblioteca padrão |
| `#![no_main]` | não use o `main` padrão |
| `#![feature(...)]` | ligue um recurso experimental (só no Rust *nightly*) |
| `#[entry]` | (da crate `uefi`) esta função é o ponto de entrada UEFI |
| `#[panic_handler]` | esta função trata os panics |
| `#[repr(C)]` | organize os campos na memória exatamente na ordem escrita, como em C |
| `#[repr(C, packed(2))]` | idem, e sem espaços de alinhamento (*padding*) entre campos |
| `#[repr(align(16))]` | o início deste tipo fica num endereço múltiplo de 16 |
| `#[repr(transparent)]` | uma struct com um único campo tem exatamente a mesma representação na memória que esse campo |
| `#[derive(Clone, Copy)]` | gere automaticamente a cópia do valor |
| `#[allow(dead_code)]` | não avise que isto não está sendo usado |

O `#[repr(...)]` é crucial num kernel. Sem ele, o compilador Rust pode
**reordenar os campos** de uma struct para economizar memória. Quando a
struct descreve algo que a **CPU** vai ler (uma entrada da IDT, a TSS), a
ordem e o tamanho precisam bater bit a bit com o manual.

---

## 14. Macros: o `!` no fim do nome

`println!`, `panic!`, `writeln!`, `format_args!`, `asm!`: o `!` indica
uma **macro**, não uma função. Macros são expandidas em código em tempo de
compilação. É assim que `println!("{} {}", a, b)` aceita qualquer número de
argumentos e verifica o texto de formatação durante a compilação.

O SiliconOS define suas próprias macros (`kprintln!`, `kprint!`) com
`macro_rules!`. Não é preciso entender a sintaxe interna delas: basta saber
que `kprintln!("x = {}", x)` funciona como o `println!` normal.

---

## 15. `asm!`: assembly dentro do Rust

```rust
core::arch::asm!("out dx, al", in("dx") port, in("al") value);
```

- O texto entre aspas é a instrução de assembly x86 (sintaxe Intel:
  destino primeiro).
- `in("dx") port` significa "antes de executar, coloque a variável `port` no
  registrador DX".
- `out("al") valor` significa "depois de executar, copie o registrador AL para
  `valor`".
- `in(reg) x` significa "coloque `x` em qualquer registrador livre", e o
  `{}` no texto vira o nome do registrador escolhido.
- `options(nomem, nostack, ...)` são promessas ao otimizador ("esta
  instrução não mexe na memória nem na pilha"), que permitem gerar código
  melhor.

Sempre exige `unsafe`.

---

## 16. Closures

```rust
TSS.call_once(|| { ... })
```

`|| { ... }` é uma função anônima sem parâmetros, como uma lambda do Java
(`() -> { ... }`) ou uma arrow function do JS (`() => { ... }`). Com
parâmetros: `|x, y| x + y`.

---

## 17. `no_std`, `core` e `alloc`

| Biblioteca | O que tem | Depende de SO? |
|---|---|---|
| `core` | tipos, `Option`, `Result`, iteradores, formatação, atômicos | **não** |
| `alloc` | `Box`, `Vec`, `String` (precisa de alguém que forneça memória) | não, mas exige um alocador |
| `std` | arquivos, rede, threads, `println!`... | **sim** |

O SiliconOS é `#![no_std]`: usa `core` e, desde o heap do doc 07, também
`alloc` ([§26](#26-alloc-box-vec-e-string)), com `Vec` e `String` dentro do
kernel. A `std` continua fora.

---

## 18. Rust nightly

O Rust tem três canais: *stable*, *beta* e *nightly*. O nightly permite
recursos experimentais ligados com `#![feature(...)]`. O SiliconOS usa um
deles: `abi_x86_interrupt` (capítulo 04), que permite escrever tratadores
de interrupção em Rust puro.

---

## 19. `enum` simples

```rust
pub enum Tipo { Livre, BootServices, Kernel, Runtime, Acpi, Mmio, Reservada }
```

Como um `enum` do Java ou do Kotlin: um tipo que só pode assumir um dos
valores listados. Usa-se com o nome do tipo na frente (`Tipo::Livre`) e
quase sempre com `match`, que obriga a tratar todos os valores
([§7](#7-match-um-switch-que-obriga-a-cobrir-tudo)).

Os enums de Rust também podem carregar dados em cada variante (é assim que
`Option` e `Result` são feitos: `Some(valor)`, `Ok(valor)`), mas o SiliconOS
ainda não precisou disso.

Para comparar dois valores com `==`, o enum precisa de
`#[derive(PartialEq, Eq)]`. Em Java, a comparação de enums funciona sem
declarar nada; em Rust, toda capacidade é pedida explicitamente.

`matches!(valor, A | B)` é um atalho para "o valor é A ou B?", que devolve
um `bool`.

---

## 20. Iteradores em cadeia: `filter`, `map`, `fold`...

```rust
self.regioes()
    .iter()                                  // percorre as regiões
    .filter(|r| r.tipo == tipo)              // só as do tipo pedido
    .fold((0, 0), |(n, q), r| (n + 1, q + r.quadros))   // acumula (contagem, soma)
```

É a mesma ideia dos `Stream`s do Java, das funções de coleção do Kotlin ou
de `filter`/`map`/`reduce` do JavaScript. Os nomes:

| Rust | Java Stream / JS |
|---|---|
| `.iter()` | `.stream()` |
| `.filter(\|x\| ...)` | `.filter(x -> ...)` |
| `.map(\|x\| ...)` | `.map(x -> ...)` |
| `.find(\|x\| ...)` | `.filter(...).findFirst()` / `.find()` |
| `.fold(inicial, \|acc, x\| ...)` | `.reduce(inicial, ...)` |
| `.max()` | `.max()` |
| `.count()` | `.count()` |
| `.enumerate()` | (pares índice + valor) |

Uma diferença importante: em Rust isso **não aloca memória** nem cria
objetos intermediários. O compilador transforma a cadeia num laço comum,
tão rápido quanto escrever o `for` à mão. Por isso pode ser usado dentro do
kernel antes mesmo de existir um alocador.

---

## 21. Slices: um pedaço de um array

```rust
&self.regioes[..self.qtd]     // do início até qtd (exclusivo)
&lista[..i]                   // os i primeiros
```

Um **slice** (`&[T]` ou `&mut [T]`) é uma "janela" sobre uma sequência de
valores na memória: um ponteiro mais um tamanho. Não copia nada. É como o
`subList` do Java ou uma `view` sobre um array. Funciona com a sintaxe de
intervalo: `[a..b]` (de `a` até antes de `b`), `[..b]`, `[a..]`.

Slices conhecem o próprio tamanho, então `slice[i]` verifica o limite (e dá
panic se `i` for grande demais).

`core::slice::from_raw_parts_mut(ponteiro, n)` cria um slice a partir de um
endereço cru e de uma quantidade. É `unsafe`, porque o compilador não tem
como saber se aquela memória existe. O SiliconOS usa isso para enxergar o
bitmap do alocador como um array comum.

Métodos úteis de slice usados no projeto: `.len()`, `.fill(valor)`,
`.contains(&x)`, `.sort_unstable_by_key(|x| chave)`, `.iter()`,
`.iter_mut()`.

---

## 22. *Newtype*: um tipo próprio para um número

```rust
pub struct Quadro(pub u64);       // uma "struct-tupla" com um único campo

let q = Quadro(0x101000);
q.0                               // acessa o campo (por posição, não por nome)
```

Embrulhar um número num tipo próprio faz o compilador **recusar misturas
erradas**: uma função que pede `Quadro` não aceita um `u64` qualquer, nem um
futuro `EnderecoVirtual`. Em Java, o equivalente seria criar uma classe só
para isso, com custo de objeto. Em Rust, o custo é **zero**: na memória, um
`Quadro` é só um `u64`.

---

## 23. `assert!` e `expect`: verificações que viram panic

```rust
assert!(condicao, "mensagem {}", valor);    // se a condição for falsa: panic com a mensagem
assert_eq!(a, b);                           // se a != b: panic mostrando os dois valores
opcao.expect("mensagem")                    // como unwrap(), mas com mensagem explicativa
```

São usados para **invariantes**: coisas que, se forem falsas, significam
um bug. Diferente do Java, os `assert!` do Rust **ficam ligados também na
versão final** (há `debug_assert!` para os que só valem em modo de
depuração). Num kernel isso é uma vantagem: um estado impossível vira uma
mensagem clara na hora, em vez de corrupção silenciosa mais tarde.

`expect("...")` é preferível a `unwrap()` quando a falha é possível em
teoria: a mensagem aparece no panic e diz **o que** estava faltando.

---

## 24. O operador `?`: propagar erros

```rust
let nova = nova_tabela().ok_or(ErroMapeamento::SemMemoria)?;
```

Lendo da esquerda para a direita:

1. `nova_tabela()` devolve `Option<u64>` (`None` se a memória acabou);
2. `.ok_or(erro)` converte para `Result`: `Some(x)` vira `Ok(x)`, `None` vira
   `Err(erro)`;
3. o **`?`** no fim: se for `Ok(x)`, a expressão vale `x` e o código segue; se
   for `Err(e)`, **a função atual retorna `Err(e)` imediatamente**.

É o equivalente a deixar uma exceção "subir" em Java, mas visível no código
(o `?` marca exatamente onde a função pode sair mais cedo) e checado pelo
compilador: só se pode usar `?` numa função que também devolve `Result` (ou
`Option`).

Sem o `?`, seria preciso escrever:

```rust
let nova = match nova_tabela() {
    Some(x) => x,
    None => return Err(ErroMapeamento::SemMemoria),
};
```

---

## 25. Atômicos: `AtomicU64`, `AtomicBool`

```rust
static DESLOCAMENTO_ATUAL: AtomicU64 = AtomicU64::new(0);

DESLOCAMENTO_ATUAL.store(valor, Ordering::Relaxed);   // escrever
let d = DESLOCAMENTO_ATUAL.load(Ordering::Relaxed);   // ler
```

Os tipos `Atomic*` são como os `AtomicLong`/`AtomicBoolean` do Java: valores
que podem ser lidos e alterados com segurança mesmo por várias CPUs ao
mesmo tempo, **sem lock**. São a forma mais simples de ter uma global
mutável sem `unsafe` e sem `Mutex`, quando o dado é um único número.

O parâmetro `Ordering` controla as garantias de ordem entre CPUs
diferentes. `Relaxed` (a mais fraca) basta quando só importa o valor da
própria variável; `SeqCst` (a mais forte, usada no panic handler) garante uma
ordem global. Enquanto o SiliconOS rodar numa CPU só, a diferença não aparece.

---

## 26. `alloc`: `Box`, `Vec` e `String`

Com um alocador global (doc 07), a crate `alloc` passa a funcionar. Ela é
trazida com uma linha no topo do `main.rs`:

```rust
extern crate alloc;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
```

| Rust | O que é | Equivalente aproximado |
|---|---|---|
| `Box<T>` | um único valor `T` guardado no heap; o `Box` é o dono | um objeto Java (sempre no heap), mas com dono único |
| `Vec<T>` | lista que cresce, com os elementos contíguos no heap | `ArrayList<T>`, `list` do Python, array do JS |
| `String` | texto UTF-8 que cresce, no heap | `StringBuilder` do Java |
| `format!(...)` | como `kprintln!`, mas devolve uma `String` | `String.format(...)`, f-strings |

```rust
let b = Box::new(42u64);                      // aloca 8 bytes e guarda 42
let mut v: Vec<u64> = Vec::new();
v.push(1);                                    // pode realocar se encher
let q: Vec<u64> = (1..=8).map(|x| x * x).collect();   // monta um Vec a partir de um iterador
let mut s = String::from("Silicon");
s.push_str("OS");
let t = format!("{} = {}", "x", 10);
```

A grande diferença para Java: **não há coletor de lixo**. Quando um `Box`,
`Vec` ou `String` sai de escopo, o `drop` (seção 8) chama o alocador e
devolve a memória **naquele momento**. `drop(v)` faz isso antes do fim do
escopo.

Um `Vec` guarda três números: ponteiro para os elementos, quantidade
(`len`) e capacidade. Quando `len` chega à capacidade, o próximo `push`
pede um bloco maior (normalmente o dobro), copia tudo e libera o antigo.
`Vec::with_capacity(n)` reserva espaço de uma vez e evita essas cópias.

`vec![valor; n]` cria um `Vec` com `n` cópias de `valor` direto no heap.
Útil para buffers grandes: `Box::new([0u8; 1 << 20])` montaria o array de
1 MiB **na pilha** antes de copiá-lo, e poderia estourá-la.

---

## 27. `GlobalAlloc`, `Layout` e `#[global_allocator]`

A crate `alloc` não sabe de onde vem a memória. Ela chama o `static`
marcado com `#[global_allocator]`, que precisa implementar a trait
`GlobalAlloc`:

```rust
unsafe impl GlobalAlloc for AlocadorLista {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 { ... }      // nulo = sem memória
    unsafe fn dealloc(&self, ponteiro: *mut u8, layout: Layout) { ... }
}

#[global_allocator]
static ALOCADOR: AlocadorLista = AlocadorLista;
```

- **`Layout`** descreve um pedido: `layout.size()` (bytes) e
  `layout.align()` (o endereço precisa ser múltiplo disto; sempre uma
  potência de 2). `Layout::new::<u64>()` é 8 e 8;
  `Layout::from_size_align(100, 4096)` cria um à mão.
- `dealloc` recebe o **mesmo** `Layout` usado no `alloc`, então o alocador
  não precisa anotar o tamanho de cada bloco.
- **`unsafe impl`**: implementar uma trait `unsafe` é uma promessa que o
  compilador não verifica. Aqui: "os blocos são válidos, alinhados e não se
  sobrepõem". `unsafe impl Send for X {}` (doc 01) é o mesmo tipo de
  promessa.
- Devolver um ponteiro nulo em `alloc` significa "sem memória". O Rust
  então chama o *alloc error handler*, que em `no_std` dispara um panic
  "memory allocation of N bytes failed".
- Também dá para chamar o alocador diretamente:
  `alloc::alloc::alloc(layout)` e `alloc::alloc::dealloc(p, layout)`
  (sempre `unsafe`). O doc 07 usa isso nos testes de alinhamento e de
  liberação dupla.

`core::mem::transmute::<A, B>(x)` reinterpreta os bits de `x` como outro
tipo, sem conversão nenhuma. O doc 07 o usa para transformar um endereço
do heap num ponteiro de função, no teste do NX. É uma das operações mais
perigosas do Rust: use só quando não houver outra saída.

---

## 28. `Ordering::Acquire` e `Release`

```rust
// produtor (o tratador do teclado)
FILA[e].store(scancode, Ordering::Relaxed);
ESCRITA.store(proxima, Ordering::Release);

// consumidor (o laço principal)
if l == ESCRITA.load(Ordering::Acquire) { return None; }
let scancode = FILA[l].load(Ordering::Relaxed);
```

A §25 disse que `Relaxed` basta "quando só importa o valor da própria
variável". Na fila do teclado (doc 08) importa a **ordem entre duas
variáveis**: o byte em `FILA[e]` precisa estar gravado **antes** de
`ESCRITA` avançar. Senão o consumidor veria a posição nova e leria um byte
velho.

Tanto o compilador quanto a CPU podem reordenar acessos à memória que não
dependem um do outro. O par `Release`/`Acquire` proíbe isso no ponto que
importa:

- uma escrita **`Release`** garante: tudo que veio antes dela no código fica
  visível antes dela;
- uma leitura **`Acquire`** que vê esse valor garante: tudo que vem depois
  dela no código enxerga o que o outro lado gravou antes do `Release`.

É o mesmo mecanismo que um `Mutex` usa por dentro (pegar o lock = `Acquire`,
soltar = `Release`), e equivale, em Java, ao que `volatile` garante sobre as
escritas anteriores. Com uma CPU só, o risco vem do compilador; com várias,
também da CPU.

Para criar um array de atômicos numa `static`, onde cada elemento precisa
ser construído (atômicos não são `Copy`), usa-se um bloco `const`:

```rust
static FILA: [AtomicU8; 64] = [const { AtomicU8::new(0) }; 64];
```

---

## 29. `#[repr(C, packed)]` e `read_unaligned`

```rust
#[repr(C, packed)]
struct Cabecalho {
    assinatura: [u8; 4],
    tamanho: u32,
    revisao: u8,
    checksum: u8,
    // ...
}

fn ler<T: Copy>(fisico: u64) -> T {
    unsafe { (fisico_para_virtual(fisico) as *const T).read_unaligned() }
}
```

Normalmente o compilador põe cada campo num endereço múltiplo do seu
tamanho (um `u32` num múltiplo de 4), inserindo bytes vazios (*padding*)
entre os campos se precisar. `packed` proíbe isso: os campos ficam colados,
exatamente como no formato definido por outra pessoa (aqui, a especificação
ACPI). `repr(C)` garante a ordem dos campos; `packed`, a ausência de buracos.

O preço: um campo pode ficar **desalinhado**, e em Rust ler um valor
desalinhado com `*ponteiro` ou `ponteiro.read()` é comportamento indefinido.
`read_unaligned()` é a leitura que aceita qualquer endereço. Pelo mesmo
motivo, o Rust não deixa criar uma referência (`&cab.tamanho`) para um
campo de struct `packed`; copiar o valor (`cab.tamanho`) é permitido.

`<T: Copy>` torna a função **genérica**: `ler::<u32>(p)` lê 4 bytes,
`ler::<[u8; 4]>(p)` lê uma assinatura, `ler::<Cabecalho>(p)` lê 36 bytes. É
como `<T> T ler(long p)` em Java, com a restrição de que `T` possa ser
copiado bit a bit.

---

## 30. Receber uma closure: `impl FnOnce`

```rust
pub fn sem_interrupcoes<R>(f: impl FnOnce() -> R) -> R {
    // ... desliga as interrupções
    let resultado = f();
    // ... religa
    resultado
}

cpu::sem_interrupcoes(|| escrever(args));
```

`impl FnOnce() -> R` quer dizer "qualquer closure ([§16](#16-closures)) sem
parâmetros que devolva um `R`". É como receber um `Supplier<R>` em Java ou
uma função em JS, e serve para o padrão "faça algo antes, rode o código do
chamador, faça algo depois", como o `try`/`finally` ou o `with` do Python.

Existem três variantes: `FnOnce` (pode ser chamada uma vez, a mais
permissiva), `FnMut` (várias vezes, pode alterar o que captura) e `Fn`
(várias vezes, só lê). O compilador gera uma versão de `sem_interrupcoes`
para cada closure usada, sem custo de chamada indireta.

`core::mem::take(&mut x)` devolve o valor de `x` e deixa no lugar o valor
padrão (`false`, `0`, `None`...). O decodificador do teclado usa
`if core::mem::take(&mut self.estendido) { ... }` para ler e zerar a
marca numa operação só.

---

## 31. Ponteiros de função: `fn(...)`

```rust
pub struct Comando {
    pub nome: &'static str,
    pub funcao: fn(&[&str]),
}

const COMANDOS: &[Comando] = &[
    Comando { nome: "ajuda", funcao: ajuda },
];

(comando.funcao)(argumentos);   // chamar
```

`fn(&[&str])` é o **tipo** "função que recebe uma fatia de `&str` e não
devolve nada". Um valor desse tipo é só o endereço da função, e o nome de
qualquer função com essa assinatura serve (`ajuda`, `eco`...). É parecido
com uma referência de método em Java (`this::ajuda` guardado num
`Consumer<List<String>>`), mas sem objeto nenhum por trás: não captura
variáveis e cabe numa `const`.

Os parênteses em `(comando.funcao)(argumentos)` são necessários: sem eles,
`comando.funcao(argumentos)` procuraria um **método** chamado `funcao`.

A diferença para uma closure ([§16](#16-closures)): a closure pode capturar
variáveis do lugar onde foi criada; o ponteiro de função, não. Uma closure
que não captura nada pode ser convertida em `fn`.

---

## 32. `let ... else` e `split_first`

```rust
let palavras: Vec<&str> = linha.split_whitespace().collect();
let Some((&nome, argumentos)) = palavras.split_first() else {
    return; // linha vazia
};
// daqui em diante, `nome` e `argumentos` existem
```

`let PADRÃO = valor else { ... };` tenta encaixar o valor no padrão (como
um `match` de um braço só). Se encaixar, as variáveis do padrão ficam
disponíveis **depois** do `let`; se não, roda o bloco `else`, que é
obrigado a sair dali (`return`, `break`, `panic!`...). É o "retorno
antecipado" de `if (x == null) return;` em Java, mas o compilador garante
que, depois dele, o valor existe.

`split_first()` divide uma fatia em `Some((primeiro, resto))`, ou `None` se
ela estiver vazia. O `&nome` no padrão tira o `&` de fora: `split_first`
devolve uma referência para o elemento (`&&str`), e o padrão a desfaz para
termos o `&str` direto.

---

## 33. Testes: `#[cfg(test)]` e `#[test_case]`

```rust
// no fim de um módulo
#[cfg(test)]
mod testes {
    use super::*;

    #[test_case]
    fn ler_numero_decimal_e_hexadecimal() {
        assert_eq!(ler_numero("0x2a"), Some(42));
    }
}
```

- **`#[cfg(test)]`**: o item só existe quando o código é compilado pelo
  `cargo test`. No `cargo build`, o módulo `testes` some do binário. É o
  mesmo mecanismo das features ([§12](#12-módulos-crates-cargo-e-features)),
  com a condição `test` ligada automaticamente pelo Cargo.
- **`use super::*`**: traz tudo do módulo de cima, inclusive o que é
  privado. Testes no mesmo arquivo podem testar funções internas.
- Num projeto comum, usa-se `#[test]`. Aqui é **`#[test_case]`**, do
  framework próprio (doc 09), ligado no `main.rs` por:

```rust
#![feature(custom_test_frameworks)]                // recurso do nightly
#![test_runner(crate::testes::executar)]           // quem roda a lista de testes
#![reexport_test_harness_main = "rodar_testes"]    // nome da função gerada
```

O compilador junta todas as funções `#[test_case]` do crate numa lista e
gera `rodar_testes()`, que chama `executar(&[&teste1, &teste2, ...])`.

---

## 34. `dyn Trait` e implementar para todo `T`

```rust
pub trait Testavel {
    fn rodar(&self);
}

impl<T: Fn()> Testavel for T {
    fn rodar(&self) { /* ... */ self(); /* ... */ }
}

pub fn executar(testes: &[&dyn Testavel]) { ... }
```

- **`impl<T: Fn()> Testavel for T`** implementa a trait para **todo tipo**
  `T` que seja uma função sem parâmetros. Em Java seria como se toda lambda
  `Runnable` ganhasse automaticamente uma interface nova. Em Rust isso se
  chama *blanket implementation*.
- **`&dyn Testavel`** é uma referência para "algum tipo que implementa
  `Testavel`", decidido em tempo de execução. Cada função de teste tem um
  tipo diferente, então uma lista delas só pode ser de `&dyn Testavel`. É o
  equivalente a uma `List<Testavel>` de interface em Java: a chamada
  `t.rodar()` passa por uma tabela de métodos (*vtable*).
- Contraste com `impl FnOnce` ([§30](#30-receber-uma-closure-impl-fnonce)):
  `impl` gera uma cópia da função para cada tipo, decidida na compilação;
  `dyn` usa uma única função e decide na execução.

`core::any::type_name::<T>()` devolve o nome do tipo `T` como texto. Para
uma função, o tipo é o caminho completo dela, e é assim que cada teste
imprime o próprio nome.

---

## 35. Funções *naked* e `naked_asm!`

```rust
#[unsafe(naked)]
unsafe extern "sysv64" fn voltar_ao_kernel() -> ! {
    naked_asm!(
        "mov rsp, [rip + {rsp_kernel}]",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "xor eax, eax",
        "ret",
        rsp_kernel = sym RSP_KERNEL,
    )
}
```

Uma função comum ganha do compilador um **prólogo** (reservar espaço na
pilha, salvar registradores) e um **epílogo** (desfazer tudo e `ret`). Uma
função **naked** ("nua") não ganha nada: o corpo é **só** o assembly do
`naked_asm!`, e é ele que precisa terminar com `ret`, `iretq` ou um salto.
Serve para o código que mexe na própria pilha ou nos registradores de um
jeito que o compilador não entende, como entrar e sair do anel 3 (doc 10).

- `#[unsafe(naked)]`: o atributo é marcado `unsafe` porque o compilador não
  confere nada lá dentro.
- Diferença para o `asm!` da [§15](#15-asm-assembly-dentro-do-rust): o `asm!`
  fica **dentro** de uma função normal, e o compilador cuida do resto; o
  `naked_asm!` **é** a função inteira.
- Operandos especiais, que viram texto no assembly:
  - `sym NOME`: o endereço de um `static` ou de uma função
    (`sym RSP_KERNEL`, `sym tratar_syscall`);
  - `const VALOR`: um número conhecido na compilação
    (`const gdt::USER_CODE as u64` vira `35`).

---

## 36. `global_asm!` e símbolos externos

```rust
global_asm!(
    ".section .rdata",
    ".global programa_ola_inicio",
    "programa_ola_inicio:",
    "    lea rdi, [rip + ola_msg]",
    ...
);

unsafe extern "C" {
    static programa_ola_inicio: u8;
}

let p: *const u8 = &raw const programa_ola_inicio;
```

- **`global_asm!`** escreve assembly fora de qualquer função, direto no
  arquivo objeto: rótulos, dados (`.ascii`), seções. O doc 10 o usa para os
  programas de usuário, que não são funções do kernel.
- **`.global NOME`** torna o rótulo visível para o linker, como um símbolo
  público.
- **`unsafe extern "C" { static NOME: u8; }`** declara, no Rust, que esse
  símbolo existe em algum lugar. O tipo (`u8`) é só um ponto de partida:
  interessa o **endereço**, obtido com `&raw const NOME` (um ponteiro cru,
  sem criar referência).
- **`unsafe impl Sync for Programa {}`**: uma `static` só pode guardar tipos
  `Sync` ("seguros para várias threads ao mesmo tempo"), e ponteiros crus
  não são. A promessa aqui é que eles apontam para bytes fixos do
  executável, que ninguém altera.

---

## 37. Convenções de chamada: `extern "sysv64"`

```rust
unsafe extern "sysv64" fn entrar_no_anel3(rip: u64, rsp: u64) -> u64 { ... }
extern "sysv64" fn tratar_syscall(r: &mut Registros) { ... }
extern "x86-interrupt" fn page_fault(quadro: QuadroInterrupcao, codigo: u64) { ... }
```

A **convenção de chamada** é o acordo, no nível do assembly, sobre como uma
função recebe argumentos e devolve o resultado: em quais registradores,
quem salva o quê, como fica a pilha. Em Rust, `extern "NOME"` escolhe a
convenção:

| Convenção | Argumentos | Onde aparece |
|---|---|---|
| `"Rust"` (padrão) | não especificada: o compilador decide | funções comuns |
| `"C"` | a do sistema-alvo. No alvo UEFI: a do **Windows** (RCX, RDX, R8, R9) | interface com C |
| `"sysv64"` | a do **Linux/macOS** (RDI, RSI, RDX, RCX, R8, R9) | o assembly do doc 10 |
| `"x86-interrupt"` | o quadro da CPU; termina com `iretq` | tratadores (doc 04) |

Quando o assembly chama uma função Rust (`call {tratar}`) ou é chamado por
ela, os dois lados precisam concordar. Escolher `"sysv64"` explicitamente
garante que o primeiro argumento vem em RDI, como o assembly espera,
independentemente do alvo. A convenção também diz quais registradores a
função chamada precisa **preservar**: na `sysv64`, RBX, RBP e R12–R15. São
exatamente os que `entrar_no_anel3` empilha.
