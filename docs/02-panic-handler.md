# 02: Panic handler: quando o Rust encontra um erro no kernel

> **Você vai entender:** o que é um *panic* em Rust; por que, num kernel,
> somos nós que decidimos o que acontece quando ele ocorre; e as
> armadilhas de escrever um tratador de erro que não pode falhar.
>
> **Arquivo:** `src/panic.rs`
> **Livro:** capítulo 3.2
> **Testar:** `./run.sh teste-panic`
> **Rust novo aqui:** `-> !` ([§3](rust-basico.md#3-funções-e-o-tipo--nunca-retorna)),
> atributos ([§13](rust-basico.md#13-atributos--e-)),
> features e `#[cfg]` ([§12](rust-basico.md#12-módulos-crates-cargo-e-features))

---

## 1. O que é um panic

Rust não tem exceções. Erros **esperados** (arquivo não encontrado,
número inválido) são valores do tipo `Result`, que você trata
explicitamente ([rust-basico §6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)).

Já os erros que indicam **um bug no programa**, situações que "nunca
deveriam acontecer", geram um **panic**. Exemplos:

| Código | Por que dá panic |
|---|---|
| `panic!("estado impossível")` | chamada explícita |
| `v[10]` num array de 3 elementos | índice fora dos limites |
| `None.unwrap()` | "eu garanti que existia", e não existia |
| `255u8 + 1` (em modo debug) | estouro aritmético |
| `x / 0` com inteiros | divisão por zero |

Para comparar com linguagens que você conhece: um panic é como uma
`IndexOutOfBoundsException` ou uma `NullPointerException` **que ninguém vai
capturar**. O comportamento padrão de um programa Rust comum é:

1. imprimir a mensagem e o local (`arquivo:linha`);
2. desfazer a pilha, chamando os destrutores no caminho (*unwinding*);
3. encerrar o processo. O sistema operacional limpa o resto.

## 2. O problema no kernel

No kernel, **nenhum desses três passos existe**:

1. Imprimir onde? O `println!` é da `std`, que não temos.
2. O *unwinding* depende de bibliotecas de suporte da `std`.
3. Encerrar o processo e voltar para quem? O kernel **é** o processo, e
   não há ninguém abaixo dele.

Por isso, num projeto `#![no_std]`, o compilador **exige** que você escreva
a função que decide o que acontece num panic. Sem ela, o projeto não
compila:

```rust
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    ...
}
```

- `#[panic_handler]` é o atributo que diz "esta é a função de panic". Só
  pode haver uma no programa inteiro.
- `info: &PanicInfo` traz a mensagem e o local do erro.
- `-> !` diz que **ela nunca retorna** ([rust-basico §3](rust-basico.md#3-funções-e-o-tipo--nunca-retorna)).
  Faz sentido: depois de um bug, não há como continuar com segurança. O
  compilador verifica que a função termina num loop infinito.

## 3. Antes e depois

Até a etapa anterior, o SiliconOS usava o panic handler que vem pronto na
crate `uefi` (ligado pela opção `features = ["panic_handler"]` no
`Cargo.toml`). Ele escreve a mensagem usando **o console do firmware**.
Só que, depois de `exit_boot_services`, esse console não existe mais.
Resultado: um panic no kernel **travava em silêncio**, o pior tipo de bug
para investigar.

Agora a opção foi removida do `Cargo.toml` e o SiliconOS tem o seu próprio
tratador. Um panic mostra isto, na serial e na tela (com fundo vermelho):

```
!!! KERNEL PANIC !!!
Local: src/main.rs:111:25
Mensagem: index out of bounds: the len is 3 but the index is 20790
```

## 4. O tratador, passo a passo

Um tratador de erro tem uma exigência especial: **ele mesmo não pode
falhar**, porque não há mais ninguém para tratar o erro dele. Cada passo
abaixo existe para evitar uma forma de falha.

### Passo 1: desligar as interrupções

```rust
unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
```

`cli` (*clear interrupt flag*) impede que interrupções de hardware
interrompam o tratador. Hoje elas já estão desligadas, mas no capítulo 9
vamos ligar o timer. Sem o `cli`, um tique do timer poderia rodar código no
meio do relatório de erro, e esse código pode estar justamente na parte
quebrada do sistema. Num panic, a regra é: **ninguém mais roda**.

### Passo 2: proteção contra panic dentro do panic

```rust
static EM_PANIC: AtomicBool = AtomicBool::new(false);

if EM_PANIC.swap(true, Ordering::SeqCst) {
    let _ = Serial.write_str("\n!!! PANIC DUPLO - parando\n");
    parar();
}
```

E se o próprio tratador tiver um bug, por exemplo no código que desenha na
tela? Ele dispararia outro panic, que chamaria o tratador de novo, que
dispararia outro panic... até a pilha estourar.

Para evitar isso, uma variável global marca "já estou tratando um panic":

- `AtomicBool` é um booleano que pode ser lido e alterado com segurança
  mesmo por várias CPUs ao mesmo tempo, como o `AtomicBoolean` do Java.
- `swap(true, ...)` grava `true` **e devolve o valor anterior**, numa única
  operação que nada consegue interromper ao meio.
- Na primeira vez, o valor anterior é `false`: seguimos normalmente. Se o
  tratador for chamado de novo, o valor anterior é `true`: escrevemos uma
  linha curta e paramos, sem tentar mais nada.

No futuro, com várias CPUs, isso também garante que **só uma** delas relate
o panic.

### Passo 3: escrever primeiro na serial

```rust
let _ = writeln!(Serial, "\n!!! KERNEL PANIC !!!");
escrever_detalhes(&mut Serial, info);
```

A serial é o canal com mais chance de funcionar: não depende de vídeo,
memória ou lock nenhum. Se o resto falhar, pelo menos a serial já recebeu a
mensagem.

### Passo 4: escrever na tela, com cuidado com o lock

```rust
if CONSOLE.is_locked() {
    unsafe { CONSOLE.force_unlock() };
}
if let Some(con) = CONSOLE.lock().as_mut() {
    con.cor = 0x00FF_FFFF;
    con.fundo = 0x00B0_0000;      // vermelho
    let _ = writeln!(con, "\n!!! KERNEL PANIC !!!");
    escrever_detalhes(con, info);
    let _ = writeln!(con, "Sistema parado.");
}
```

Considere este cenário:

1. Um `kprintln!` pega o lock do console.
2. No meio do desenho, um bug dispara um panic.
3. O tratador chama `CONSOLE.lock()` para escrever a mensagem...
4. ...e fica **esperando para sempre** um lock que nunca será liberado,
   porque quem o segurava nunca mais vai rodar.

Isso é um **deadlock**: a máquina trava e a tela não mostra nada. Para
evitá-lo, se o lock estiver preso, forçamos a liberação com
`force_unlock()`.

Normalmente forçar a liberação de um lock é perigoso, porque o dado
protegido pode ter ficado pela metade. Aqui é aceitável: quem segurava o
lock **nunca mais vai rodar**, e o pior que pode acontecer é uma letra
mal desenhada. Por isso a chamada é `unsafe`.

### Passo 5: parar a CPU para sempre

```rust
fn parar() -> ! {
    loop {
        unsafe { core::arch::asm!("cli; hlt", options(nomem, nostack)) };
    }
}
```

- `hlt` faz a CPU **dormir** até a próxima interrupção. Gasta menos energia
  que um loop vazio, que deixaria a CPU a 100%.
- O `cli` antes garante que nenhuma interrupção comum a acorde.
- O `loop` existe para um caso raro: há uma interrupção especial (NMI) que
  não pode ser desligada. Se ela acordar a CPU, o loop a põe para dormir de
  novo.

## 5. As informações do `PanicInfo`

```rust
fn escrever_detalhes(w: &mut impl Write, info: &PanicInfo) {
    if let Some(loc) = info.location() {
        let _ = writeln!(w, "Local: {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    let _ = writeln!(w, "Mensagem: {}", info.message());
}
```

- `info.location()` traz arquivo, linha e coluna. O compilador grava essas
  informações dentro do binário no momento em que compila cada `panic!`.
- `info.message()` é a mensagem já formatada.
- O parâmetro `w: &mut impl Write` aceita qualquer destino de texto. Por
  isso a mesma função escreve tanto na `Serial` quanto no `Console`
  ([rust-basico §5](rust-basico.md#5-traits-interfaces)).

## 6. Como foi testado

### Uma "feature" só para teste

Para ver o tratador funcionando, precisamos provocar um panic de propósito,
mas **sem** deixar esse código na versão normal. Para isso, o `Cargo.toml`
declara uma opção de compilação:

```toml
[features]
teste-panic = []
```

E o código de teste fica dentro de um bloco que só é compilado quando essa
opção está ligada ([rust-basico §12](rust-basico.md#12-módulos-crates-cargo-e-features)):

```rust
#[cfg(feature = "teste-panic")]
{
    let v: [u32; 3] = [1, 2, 3];
    let i = paginas_livres as usize;   // um número grande: índice inválido de propósito
    kprintln!("Testando panic: acessando v[{}]...", i);
    kprintln!("{}", v[i]);             // ← panic aqui
}
```

Por que o índice vem de `paginas_livres`, e não de um número fixo como 10?
Porque, se escrevêssemos `v[10]`, o compilador perceberia o erro **durante
a compilação** e recusaria compilar. Usando um valor que só é conhecido
durante a execução, o erro acontece de verdade em tempo de execução.

`./run.sh teste-panic` compila com a feature ligada. `./run.sh` compila sem
ela, e o bloco simplesmente não existe no binário.

### `-no-reboot`

O `run.sh` passa `-no-reboot` ao QEMU. Se a CPU reiniciar (o que acontece
num *triple fault*, doc 04), o QEMU **fecha** em vez de reiniciar em
loop. Assim a última mensagem na serial fica visível, e fica fácil
perceber que algo deu muito errado.

## 7. O que este tratador ainda **não** pega

O panic handler só é chamado por **erros que o Rust detecta**: verificações
que o compilador inseriu no código (limite de array, `unwrap`...).

Existe outra categoria: erros que **a própria CPU** detecta ao executar uma
instrução, como acessar um endereço que não existe, executar bytes que não
são instrução ou dividir por zero em assembly. Esses geram **exceções da
CPU**, que **não passam pelo Rust** e não chamam o panic handler. Para
tratá-las precisamos da **IDT** (doc 04), que por sua vez depende da **GDT**
(doc 03).

---

## Resumo

- Um **panic** é um erro de lógica do programa (índice inválido, `unwrap`
  em `None`...). Num programa comum, a `std` imprime e encerra o processo.
- Num kernel `no_std`, **nós** escrevemos o `#[panic_handler]`, que nunca
  retorna (`-> !`).
- O tratador é escrito para **não poder falhar**: desliga interrupções,
  evita recursão (`AtomicBool`), escreve primeiro na serial, força a
  liberação do lock da tela e para a CPU com `cli; hlt`.
- Erros detectados pela **CPU** (não pelo Rust) precisam de outro
  mecanismo: a IDT.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que o tratador escreve na serial antes da tela?**
   Porque a serial não depende de vídeo nem de lock; é o canal com mais
   chance de funcionar, mesmo que o console esteja quebrado.

2. **O que aconteceria sem o `force_unlock`, se o panic ocorresse dentro
   de um `kprintln!`?**
   Deadlock: o tratador esperaria para sempre por um lock que nunca seria
   liberado, e a mensagem de erro nunca apareceria na tela.

3. **Por que o teste usa `paginas_livres` como índice em vez de `10`?**
   Com um número fixo, o compilador detectaria o erro durante a compilação
   e recusaria compilar. Precisamos de um valor conhecido só em tempo de
   execução.

4. **Um acesso a um endereço de memória inexistente chama o panic
   handler?**
   Não. Quem detecta é a CPU, que gera uma exceção (page fault), tratada
   pela IDT (doc 04).

</details>
