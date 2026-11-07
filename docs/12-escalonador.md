# 12: O escalonador

> **Você vai entender:** como uma CPU só roda vários programas "ao mesmo
> tempo"; o que é o **contexto** de um processo e como trocá-lo com uma
> função de 9 instruções que "retorna" na história de outro processo; por
> que cada processo precisa da **sua própria pilha do kernel**; como um
> processo novo começa a partir de uma pilha **fabricada**; como o timer
> tira a CPU de quem não a devolve (**preempção**); e a regra que impede o
> kernel de travar quando é interrompido no meio de uma seção crítica.
>
> **Arquivos:** `src/escalonador.rs` (novo: processos, troca, rodízio),
> `src/usuario/mod.rs` (`iniciar`, `rodar`, syscall `pid`),
> `src/usuario/programas.rs` (programa `contador`), `src/gdt.rs`
> (`definir_rsp0`), o tratador em `src/interrupcoes/timer.rs`, os atalhos
> de `src/memoria/quadros.rs` e `heap.rs`, e o shell (`rodar ... &`, `ps`)
> **Livro:** capítulo 11, seção 11.4 (o roteiro termina em "troca
> cooperativa, depois preemptiva"; a implementação é nossa)
> **Testar:** `./run.sh`, depois `rodar contador &`, `rodar contador &`,
> `ps`, `rodar laco &`, `eco oi`; `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 08 (timer, `sem_interrupcoes`), doc 10 (anel 3, TSS.RSP0,
> `iretq`) e doc 11 (espaços de endereços)
> **Rust novo aqui:** `UnsafeCell` ([§39](rust-basico.md#39-unsafecell-mutar-através-de-uma-referência-compartilhada))
>
> **Código deste capítulo:** [estado no fim do doc 12](https://github.com/moises-dev-git/siliconOS/tree/doc-12) · [o que mudou desde o doc 11](https://github.com/moises-dev-git/siliconOS/compare/doc-11...doc-12) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema

Até o doc 11, `rodar ola` funcionava assim: o shell entrava no anel 3 e
**ficava parado** até o programa terminar. Com `rodar laco`, o shell sumia
por 2 segundos; nenhuma tecla era atendida. Um programa por vez, e o
kernel inteiro à espera dele.

O doc 11 já deu a cada programa o seu espaço de endereços, de modo que dois
programas **podem** existir ao mesmo tempo no mesmo endereço virtual. Falta
a outra metade: dividir **a CPU** entre eles.

## 2. A ideia: dividir o tempo

Uma CPU executa uma instrução de cada vez. "Vários programas ao mesmo
tempo" é uma ilusão: o sistema roda um pouco de um, **para**, guarda onde
ele estava, roda um pouco de outro, e assim por diante, rápido o bastante
para parecer simultâneo. A ideia tem nome e data: **tempo compartilhado**
(*time-sharing*), demonstrada no CTSS do MIT em 1961, para que vários
usuários usassem o mesmo computador por terminais.

Três palavras para fixar:

- **Processo:** um programa em execução, com tudo o que precisa para
  continuar de onde parou. No SiliconOS: o espaço de endereços (doc 11),
  uma pilha do kernel e os registradores salvos.
- **Contexto:** o estado da CPU que pertence ao processo: os registradores
  (incluindo RIP e RSP), as flags, e qual PML4 está no CR3. **Trocar de
  contexto** é guardar o de um e carregar o de outro.
- **Escalonador** (*scheduler*): quem decide quem roda agora. O nosso usa o
  **rodízio** (*round robin*): cada processo pronto roda um tique (10 ms),
  e a vez passa ao próximo da fila.

E duas formas de tirar a CPU de alguém:

- **Cooperativa:** o processo devolve a CPU quando quer (chamando algo
  como `ceder()`). Simples, mas um programa com laço infinito nunca devolve.
- **Preemptiva:** o **timer** interrompe o processo, queira ele ou não, e
  o tratador troca de processo. O doc 10 já tinha o embrião disso: o timer
  encerrava um programa que passasse de 2 s. Agora ele não encerra: ele
  **passa a vez**.

O SiliconOS faz as duas: o timer troca a cada tique (preemptiva), e o
kernel, quando não tem nada a fazer, cede a vez sem esperar o tique
(cooperativa).

## 3. Cada processo precisa da sua pilha do kernel

Quando o timer interrompe um programa no anel 3, a CPU troca para a pilha
que está em **TSS.RSP0** (doc 10) e empilha ali o quadro de interrupção
(RIP, CS, RFLAGS, RSP, SS do programa). O tratador roda em cima desse
quadro. Se o tratador decide trocar para outro processo, **esse quadro
precisa ficar guardado** até o processo voltar: é ele que diz para onde o
`iretq` vai devolver o programa.

No doc 10 havia uma pilha só (`PILHA_ANEL0`). Com dois processos, ela não
basta:

```
   uma pilha só (errado)                 uma por processo (certo)

   A é interrompido:                     A é interrompido:
     [quadro de A] ◄─ RSP0                 pilha de A: [quadro de A]
   troca para B; B é interrompido:       troca para B; B é interrompido:
     [quadro de B] ◄─ RSP0 (por cima!)     pilha de B: [quadro de B]
   volta para A: iretq usa o             volta para A: o quadro de A
   quadro de B → A continua no            está intacto na pilha de A
   código de B, com a pilha de B
```

Por isso cada processo ganha uma pilha do kernel de 16 KiB, e o
escalonador, a cada troca, escreve em TSS.RSP0 o topo da pilha de quem vai
rodar. A CPU lê a TSS da memória a cada interrupção vinda do anel 3, então
a mudança vale na hora.

A TSS estava num `Once<Tss>`, que só entrega referências compartilhadas
(`&Tss`): não dá para alterar um campo por ali. A solução de Rust para
"um valor que muda, acessado por referência compartilhada" é a `UnsafeCell`
([§39](rust-basico.md#39-unsafecell-mutar-através-de-uma-referência-compartilhada)):

```rust
struct CelulaTss(UnsafeCell<Tss>);
// Só o escalonador escreve, sempre com as interrupções desligadas.
unsafe impl Sync for CelulaTss {}

static TSS: Once<CelulaTss> = Once::new();

pub fn definir_rsp0(topo: u64) {
    let tss = TSS.get().expect("gdt::init antes de definir_rsp0");
    unsafe { (&raw mut (*tss.0.get()).rsp as *mut u64).write_unaligned(topo) };
}
```

`UnsafeCell::get()` devolve um ponteiro cru (`*mut Tss`), e a escrita é
`unsafe`: nós garantimos que ninguém lê a TSS ao mesmo tempo. A TSS é
`packed` (doc 03), então o campo `rsp` não é alinhado em 8 bytes; por isso
`&raw mut` (que não cria referência, só um ponteiro) e `write_unaligned`.
`rsp` é um array de 3; `rsp[0]` é o primeiro u64, no mesmo endereço do
array.

## 4. A troca: uma função que retorna em outro lugar

O coração do escalonador são 9 instruções:

```rust
#[unsafe(naked)]
unsafe extern "sysv64" fn trocar(salvar: *mut u64, novo_rsp: u64) {
    naked_asm!(
        "push rbx", "push rbp", "push r12", "push r13", "push r14", "push r15",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "pop r15", "pop r14", "pop r13", "pop r12", "pop rbp", "pop rbx",
        "ret",
    )
}
```

Lendo devagar, com o processo A chamando `trocar(&A.rsp, B.rsp)`:

1. `call trocar` empilhou, na pilha de A, o endereço de retorno (para onde
   A continua).
2. Os 6 `push` guardam os registradores que a convenção `sysv64` diz que a
   função chamada precisa preservar (doc 10,
   [§37](rust-basico.md#37-convenções-de-chamada-extern-sysv64)). Os outros
   (RAX, RCX, RDX...) quem chamou já considera perdidos depois de qualquer
   chamada, então não precisam ser salvos.
3. `mov [rdi], rsp`: o RSP de A (que agora aponta para esses 6 valores e o
   endereço de retorno) vai para `A.rsp`. **Esse número é todo o contexto
   de A**: o resto está na pilha para a qual ele aponta.
4. `mov rsp, rsi`: a CPU passa a usar a pilha de B. A partir daqui, estamos
   "em B".
5. Os 6 `pop` tiram da pilha de B os registradores que **B** guardou quando
   chamou `trocar`, lá atrás.
6. `ret` desempilha o endereço de retorno **de B**: a CPU continua na
   instrução seguinte ao `trocar` que B chamou, muito tempo atrás.

```
  pilha de A                         pilha de B
  ┌──────────────────┐               ┌──────────────────┐
  │ ... (tratador)   │               │ ... (tratador)   │
  │ retorno p/ A     │               │ retorno p/ B     │ ◄─ ret volta aqui
  │ rbx rbp r12..r15 │ ◄─ A.rsp      │ rbx rbp r12..r15 │ ◄─ B.rsp: os pop leem daqui
  └──────────────────┘               └──────────────────┘
```

Para B, a chamada a `trocar` simplesmente "demorou": entrou, e um dia
retornou. Para A, a mesma coisa vai acontecer quando alguém chamar
`trocar(&X.rsp, A.rsp)`. Cada processo enxerga `trocar` como uma função
comum que leva um tempo imprevisível. É o mesmo truque do `voltar_ao_kernel`
do doc 10 (restaurar um RSP salvo e dar `ret`), agora generalizado: em vez
de um RSP do kernel, um por processo.

## 5. Um processo novo: a pilha fabricada

E na **primeira** vez? Um processo que acabou de ser criado nunca chamou
`trocar`; não há registradores nem endereço de retorno na pilha dele. A
solução é fabricar a pilha como se ele tivesse chamado:

```rust
pub fn criar(nome: &'static str, espaco: EspacoEnderecos, rip: u64, rsp: u64, segundo_plano: bool) -> Pid {
    // A pilha do kernel do processo, preparada como se ele já tivesse
    // rodado e sido trocado: o primeiro `trocar` para ele desempilha os 6
    // registradores, e o `ret` cai em `primeira_entrada`, que faz o iretq.
    //
    //   topo ─► [ SS          ]  ┐
    //           [ RSP usuário ]  │ quadro que o iretq desempilha
    //           [ RFLAGS      ]  │
    //           [ CS          ]  │
    //           [ RIP         ]  ┘
    //           [ primeira_entrada ]   endereço de retorno de `trocar`
    //           [ rbx rbp r12 r13 r14 r15 = 0 ]   ◄─ rsp salvo
    let mut pilha = alloc::vec![0u64; TAM_PILHA_KERNEL / 8].into_boxed_slice();
    let n = pilha.len();
    pilha[n - 1] = gdt::USER_DATA as u64;
    pilha[n - 2] = rsp;
    pilha[n - 3] = 0x202; // IF ligado
    pilha[n - 4] = gdt::USER_CODE as u64;
    pilha[n - 5] = rip;
    pilha[n - 6] = primeira_entrada as *const () as u64;
    // pilha[n - 12 .. n - 6]: os 6 registradores, já zerados.
    let rsp_kernel = &pilha[n - 12] as *const u64 as u64;

    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let pid = t.proximo_pid;
        t.proximo_pid += 1;
        t.processos.push(Box::new(Processo {
            pid, nome, estado: Estado::Pronto, rsp: rsp_kernel,
            pilha: Some(pilha), espaco: Some(espaco), tiques_cpu: 0, segundo_plano,
        }));
        pid
    })
}

#[unsafe(naked)]
unsafe extern "sysv64" fn primeira_entrada() -> ! {
    naked_asm!(
        "xor eax, eax", "xor ebx, ebx", "xor ecx, ecx", "xor edx, edx",
        "xor esi, esi", "xor edi, edi", "xor ebp, ebp",
        "xor r8d, r8d", "xor r9d, r9d", "xor r10d, r10d", "xor r11d, r11d",
        "xor r12d, r12d", "xor r13d, r13d", "xor r14d, r14d", "xor r15d, r15d",
        "iretq",
    )
}
```

Quando o escalonador escolhe esse processo pela primeira vez, `trocar`
carrega `rsp_kernel`, os 6 `pop` leem zeros, e o `ret` pula para
`primeira_entrada`. Ela zera os registradores (o programa não deve ver
valores do kernel) e executa `iretq`, que desempilha o quadro montado à mão
e entra no anel 3 em `rip`. É exatamente o `iretq` falso do doc 10, só que
montado com antecedência, na pilha do próprio processo.

Detalhes:

- **A pilha é um `Box<[u64]>`** no heap: alinhada em 8 bytes e indexável
  como array. `vec![0u64; N].into_boxed_slice()` cria o vetor zerado e o
  transforma num bloco de tamanho fixo.
- **RFLAGS = 0x202:** bit 9 (IF) ligado, para o programa poder ser
  interrompido pelo timer (o bit 1 é sempre 1).
- **`primeira_entrada as *const () as u64`:** o endereço da função como
  número. O Rust pede o passo intermediário por ponteiro (`*const ()`)
  para deixar claro que é um endereço, não uma conversão de valor.
- **O espaço já vem pronto:** `usuario::iniciar` (seção 9) cria o espaço,
  copia o código e mapeia a pilha do programa, e só depois chama `criar`.
  O escalonador cuida da CPU; quem prepara a memória é o módulo `usuario`.

## 6. A tabela de processos e o rodízio

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Estado {
    /// Pode rodar; espera a vez.
    Pronto,
    /// É o processo que está na CPU agora.
    Rodando,
    /// Terminou; espera alguém recolher o resultado e liberar a memória.
    Terminado(Resultado),
}

pub struct Processo {
    pub pid: Pid,
    pub nome: &'static str,
    pub estado: Estado,
    /// RSP salvo por `trocar` quando o processo sai da CPU.
    rsp: u64,
    /// Pilha do kernel (None no processo 0, que usa a pilha do boot).
    pilha: Option<Box<[u64]>>,
    /// Espaço de endereços (None no processo 0: usa as tabelas do kernel).
    espaco: Option<EspacoEnderecos>,
    /// Tiques do timer que pegaram este processo no anel 3.
    pub tiques_cpu: u64,
    /// Ninguém espera por ele: o resultado é impresso ao terminar.
    pub segundo_plano: bool,
}

struct Tabela {
    /// Box: o endereço de cada `Processo` não muda quando o Vec cresce
    /// (`trocar` guarda um ponteiro para o campo `rsp`).
    processos: Vec<Box<Processo>>,
    /// Índice (em `processos`) de quem está rodando.
    atual: usize,
    proximo_pid: Pid,
}

static TABELA: Mutex<Tabela> = Mutex::new(Tabela { processos: Vec::new(), atual: 0, proximo_pid: 1 });
```

**O processo 0 é o kernel.** O código que está rodando quando o escalonador
começa (o boot, depois o shell) também precisa de um lugar na tabela: quando
o timer o interromper e trocar para um programa, `trocar` precisa de um
`rsp` onde guardar o contexto dele. `init` o registra:

```rust
pub fn init() {
    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        assert!(t.processos.is_empty(), "escalonador::init chamado duas vezes");
        t.processos.push(Box::new(Processo {
            pid: 0, nome: "kernel", estado: Estado::Rodando, rsp: 0,
            pilha: None, espaco: None, tiques_cpu: 0, segundo_plano: false,
        }));
    });
}
```

Ele não tem pilha própria (usa a do boot), nem espaço próprio (usa as
tabelas do kernel), e nunca vai ao anel 3. `main.rs` chama `init` logo
antes de `interrupcoes::init`, porque o primeiro tique já chama o
escalonador.

**Por que `Vec<Box<Processo>>` e não `Vec<Processo>`?** Quando um `Vec`
cresce, ele pode mudar os elementos de lugar na memória. `trocar` recebe um
ponteiro para o campo `rsp` de um processo e escreve nele **depois** que o
lock foi solto; se o processo tivesse mudado de lugar nesse meio-tempo, a
escrita iria para memória já liberada. Com `Box`, o `Vec` guarda só
ponteiros; o `Processo` em si fica parado no heap.

### 6.1 Escolher e trocar

```rust
pub fn ceder() {
    cpu::sem_interrupcoes(|| {
        if let Some((salvar, novo_rsp)) = escolher_proximo() {
            unsafe { trocar(salvar, novo_rsp) };
        }
    });
}

fn escolher_proximo() -> Option<(*mut u64, u64)> {
    let mut t = TABELA.lock();
    let n = t.processos.len();
    let atual = t.atual;
    let proximo = (1..n).map(|d| (atual + d) % n).find(|&i| t.processos[i].estado == Estado::Pronto)?;

    if t.processos[atual].estado == Estado::Rodando {
        t.processos[atual].estado = Estado::Pronto;
    }
    t.processos[proximo].estado = Estado::Rodando;
    t.atual = proximo;

    let p = &t.processos[proximo];
    if p.pilha.is_some() {
        gdt::definir_rsp0(p.topo_pilha());
    }
    paginacao::ativar(p.espaco.as_ref().map_or(paginacao::raiz_kernel(), |e| e.raiz()));

    let novo_rsp = p.rsp;
    let salvar = &mut t.processos[atual].rsp as *mut u64;
    Some((salvar, novo_rsp))
}
```

- **O rodízio** é a linha do `find`: a partir de quem está rodando, olha o
  próximo, o seguinte, ... dando a volta (`% n`), e pega o primeiro
  `Pronto`. Se não houver nenhum outro, o `?` devolve `None` e `ceder`
  volta sem trocar.
- **O CR3 muda antes da troca de pilha.** Parece perigoso (estamos rodando
  na pilha do processo antigo com as tabelas do novo), mas não é: as
  pilhas do kernel estão no heap, que é do kernel e aparece igual em todos
  os espaços (doc 11). O processo 0 usa as tabelas do kernel
  (`raiz_kernel()`).
- **O lock é solto antes de `trocar`.** O `MutexGuard` `t` morre no fim de
  `escolher_proximo`. Se `trocar` acontecesse com o lock preso, o
  processo seguinte (que um dia chama `ceder`) giraria para sempre
  esperando um lock que só o processo antigo, parado, pode soltar.
- **Tudo com as interrupções desligadas** (`sem_interrupcoes`): um tique
  no meio de `escolher_proximo` chamaria `ceder` de novo, com a tabela pela
  metade. E repare num detalhe bonito: `sem_interrupcoes` guarda o estado
  do IF numa variável local, **na pilha de quem chamou**. Quando o processo
  volta a rodar (o `trocar` dele "retorna"), a closure termina e o IF volta
  ao que **aquele processo** tinha. Cada processo tem o seu.

### 6.2 O timer passa a vez

O tratador do timer (doc 08) termina agora assim:

```rust
    #[cfg(not(feature = "teste-sem-eoi"))]
    apic::eoi();

    // A vez do próximo processo (docs/12-escalonador.md). Depois do EOI:
    // este tratador só termina quando este processo voltar a rodar, e até
    // lá o APIC precisa continuar entregando tiques.
    crate::escalonador::tique(&quadro);
}
```

```rust
pub fn tique(quadro: &QuadroInterrupcao) {
    if quadro.cs & 3 == 3 {
        // O tique pegou um programa no anel 3: conta o tempo de CPU dele.
        let excedeu = cpu::sem_interrupcoes(|| {
            let mut t = TABELA.lock();
            let a = t.atual;
            let p = &mut t.processos[a];
            p.tiques_cpu += 1;
            p.tiques_cpu >= usuario::LIMITE_TIQUES
        });
        if excedeu {
            encerrar(Resultado::TempoEsgotado);
        }
    }
    ceder();
}
```

- **O EOI vem antes.** Se o tratador trocasse de processo antes do EOI, o
  APIC continuaria achando que a interrupção do timer está em andamento e
  não entregaria o próximo tique (é o `teste-sem-eoi` do doc 08). O
  processo novo rodaria para sempre.
- **O limite de 2 s virou tempo de CPU.** No doc 10, "2 s" era tempo de
  relógio desde o início do programa. Com vários processos, isso seria
  injusto: um programa esperando a vez também envelheceria. Agora conta só
  os tiques que pegaram o processo **rodando** no anel 3.
- **O tratador "fica suspenso".** Quando `ceder` troca de processo, o
  tratador do timer de A fica parado no meio, na pilha de A. Ele só termina
  (e faz `iretq` de volta para o programa A) quando A for escolhido de novo.

O caminho completo de uma troca entre dois programas:

```
  A no anel 3 ──tique──► CPU empilha o quadro de A na pilha de A (TSS.RSP0)
                         tratador do timer: tiques++, EOI, tique()
                         ceder() → escolher_proximo(): B; RSP0 = pilha de B; CR3 = B
                         trocar(&A.rsp, B.rsp)
                                │
                                └──► ... continua dentro do ceder() de B (o tique anterior)
                                     tique() termina, tratador termina
  B no anel 3 ◄─────────────────────  iretq com o quadro de B
```

## 7. Terminar e recolher

Um programa termina por três caminhos (doc 10: `sair`, exceção, tempo
esgotado), e os três chamam `escalonador::encerrar`:

```rust
pub fn encerrar(resultado: Resultado) -> ! {
    cpu::desligar_interrupcoes();
    let (pid, nome, segundo_plano) = {
        let mut t = TABELA.lock();
        let a = t.atual;
        assert!(a != 0, "o kernel (processo 0) nao pode ser encerrado");
        let p = &mut t.processos[a];
        p.estado = Estado::Terminado(resultado);
        (p.pid, p.nome, p.segundo_plano)
    };
    if segundo_plano {
        kprintln!("\n[{}] '{}' {}", pid, nome, usuario::descrever(resultado));
    }
    ceder();
    unreachable!("processo encerrado voltou a rodar");
}
```

O processo vira `Terminado` e cede a vez. Como `escolher_proximo` só
escolhe processos `Pronto`, ele nunca mais roda.

Mas ninguém libera a memória dele aqui, e de propósito: `encerrar` está
rodando **na pilha do kernel do próprio processo**. Liberar essa pilha
agora seria serrar o galho de novo (doc 11). O processo fica na tabela como
`Terminado`, com o resultado guardado, até alguém **recolhê-lo** de outro
processo. (Sistemas Unix chamam um processo nesse estado de **zumbi**: já
morreu, mas ainda ocupa uma linha na tabela até o pai ler o resultado.)

```rust
fn retirar(t: &mut Tabela, i: usize) -> (Box<Processo>, Resultado) {
    let Estado::Terminado(r) = t.processos[i].estado else { unreachable!() };
    if i < t.atual {
        t.atual -= 1; // os índices depois de `i` andam uma posição
    }
    (t.processos.remove(i), r)
}

pub fn recolher(pid: Pid) -> Option<Resultado> {
    let (processo, r) = cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let i = t.processos.iter().position(|p| p.pid == pid && matches!(p.estado, Estado::Terminado(_)))?;
        Some(retirar(&mut t, i))
    })?;
    // Liberar fora do lock: o Drop do espaço usa o alocador de quadros.
    drop(processo);
    Some(r)
}

pub fn esperar(pid: Pid) -> Resultado {
    loop {
        if let Some(r) = recolher(pid) {
            return r;
        }
        ceder();
    }
}
```

- `drop(processo)` libera tudo de uma vez: o `Box<Processo>`, a pilha
  (`Box<[u64]>`, de volta ao heap) e o `EspacoEnderecos`, cujo `Drop` (doc
  11) devolve páginas, tabelas e a PML4. Quem recolhe roda em outro
  processo, então nem a pilha nem o CR3 do morto estão em uso.
- **`esperar` não fica girando à toa:** enquanto o processo esperado não
  termina, ela cede a vez. O kernel só volta a olhar quando o rodízio
  passar por ele de novo.
- `recolher_segundo_plano` faz o mesmo para todos os processos de segundo
  plano terminados, sem devolver resultado (eles já imprimiram o seu em
  `encerrar`).

## 8. A regra dos locks

Com preempção, o timer pode trocar de processo **entre quaisquer duas
instruções** em que as interrupções estejam ligadas. Imagine o shell
(processo 0) pedindo um quadro:

```
  shell: quadros::alocar() → QUADROS.lock() → pegou o lock
         ── tique! ── escalonador troca para o programa A
  A:     int 0x80 → sair → encerrar → ...
  shell (mais tarde)...
```

Até aí, nada de mau. Mas se, depois do tique, outro processo tentasse pegar
`QUADROS` **com as interrupções desligadas** (dentro de uma chamada de
sistema, por exemplo: o `int 0x80` desliga o IF), ele giraria esperando o
lock, e com o IF desligado nenhum tique viria para devolver a CPU ao shell,
o único que pode soltar o lock. Travamento para sempre.

A regra que evita isso é simples: **todo lock do kernel é pego com as
interrupções desligadas.** Assim, quem segura um lock nunca é trocado no
meio. O heap e o `_print` já seguiam a regra desde o doc 08 (por causa dos
tratadores); agora os atalhos do alocador de quadros também:

```rust
/// Atalhos para usar o alocador global sem lidar com o Mutex/Option.
/// Com as interrupções desligadas (desde o doc 12): com o escalonador, o
/// timer pode trocar de processo a qualquer momento; se trocasse com o lock
/// preso, o próximo que pedisse um quadro ficaria girando para sempre.
pub fn alocar() -> Option<Quadro> {
    cpu::sem_interrupcoes(|| QUADROS.lock().as_mut().expect("alocador de quadros nao inicializado").alocar())
}
```

(e o mesmo em `liberar`, `livres`, `heap::estatisticas`, `heap::contem` e
na leitura de `QUADROS` do comando `mem`). A tabela de processos também só
é acessada dentro de `sem_interrupcoes`.

## 9. Os programas e o shell

### 9.1 `iniciar` e `rodar`

O `rodar` do doc 11 fazia tudo: preparava a memória, entrava no anel 3 e
esperava. Agora ele se divide em dois:

```rust
pub fn iniciar(programa: &programas::Programa, segundo_plano: bool) -> Pid {
    // ... cria o espaço, copia o código, mapeia código e pilha (como no doc 11) ...
    escalonador::criar(programa.nome, espaco, CODIGO_VIRT, PILHA_VIRT + TAMANHO_QUADRO, segundo_plano)
}

pub fn rodar(programa: &programas::Programa) -> Resultado {
    let pid = iniciar(programa, false);
    escalonador::esperar(pid)
}
```

Saíram do módulo `usuario`: `entrar_no_anel3`, `voltar_ao_kernel`,
`encerrar`, `tempo_esgotado` e as variáveis `ATIVO`, `INICIO`, `RESULTADO`
e `RSP_KERNEL`. O papel deles passou para `criar`/`primeira_entrada`,
`trocar`, `escalonador::encerrar` e `tique`.

### 9.2 Uma chamada de sistema nova: `pid`

```rust
pub const SYS_PID: u64 = 3;
...
        SYS_PID => escalonador::pid_atual() as u64,
```

E um programa que a usa, `contador`: escreve o último dígito do próprio pid
5 vezes, esperando 50 tiques (500 ms) entre elas:

```rust
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
```

`div rcx` divide RDX:RAX (128 bits) por RCX e deixa o quociente em RAX e o
**resto** em RDX; por isso RDX é zerado antes. O texto que `escrever`
recebe fica na pilha do programa, que `faixa_do_programa` aceita (doc 10).

### 9.3 O shell: `&` e `ps`

`rodar <programa> &` inicia em segundo plano e volta ao prompt:

```rust
    if argumentos.get(1) == Some(&"&") {
        // Segundo plano (doc 12): o shell não espera. Quando o programa
        // terminar, o escalonador imprime o resultado.
        let pid = usuario::iniciar(programa, true);
        kprintln!("[{}] '{}' em segundo plano", pid, programa.nome);
    } else {
        let r = usuario::rodar(programa);
        kprintln!("'{}' {}", programa.nome, usuario::descrever(r));
    }
```

`ps` lista a tabela (`escalonador::listar`). E o laço principal do shell
(doc 09) ganhou três linhas: recolher os programas de segundo plano que
terminaram e, se houver outro processo pronto, **ceder a vez** em vez de
dormir:

```rust
        escalonador::recolher_segundo_plano();
        if escalonador::ha_outros_prontos() {
            escalonador::ceder();
        } else {
            teclado::esperar();
        }
```

Sem isso, o shell gastaria um tique inteiro dormindo em `hlt` a cada volta
do rodízio, tempo que os programas poderiam usar. Com `ceder`, o shell olha
o teclado e passa a vez na hora; uma tecla espera no máximo um tique
(10 ms) para ser atendida.

## 10. Como foi testado

### 10.1 Pelo shell

Saída real (serial). As teclas foram enviadas pelo monitor do QEMU, uma a
cada 80 ms, então os dígitos dos programas aparecem **no meio** do que
está sendo digitado: é a preempção acontecendo na tela.

```
silicon> rodar contador &
[1] 'contador' em segundo plano
1 silicon> rodar 1 contad1 or &
[2] 'contador' em segundo plano
2 silicon> p1 s
  PID  ESTADO      CPU (tiques)  NOME
    0  rodando                0  kernel
    1  pronto               153  contador
    2  pronto                13  contador
silicon> 2 1 2 
[1] 'contador' terminou: codigo de saida 0
2 2 
[2] 'contador' terminou: codigo de saida 0
rodar laco &
[3] 'laco' em segundo plano
silicon> eco ainda respondo
ainda respondo
silicon> ps
  PID  ESTADO      CPU (tiques)  NOME
    0  rodando                0  kernel
    3  pronto               181  laco
silicon> 
[3] 'laco' encerrado pelo kernel: usou 2 s de CPU sem terminar
ps
  PID  ESTADO      CPU (tiques)  NOME
    0  rodando                0  kernel
silicon> mem
quadros: 20298 livres de 32768 (79 MiB livres)
heap: 224 bytes em uso (4 alocacoes vivas), pico 32 KiB
      mapeado 1024 KiB, 1 blocos livres, maior 1023 KiB
```

Lendo:

- **Dois contadores ao mesmo tempo:** os dígitos `1` e `2` se alternam.
- **O shell continua vivo** com `laco` rodando: `eco ainda respondo` foi
  atendido na hora. No doc 11, o shell ficaria mudo por 2 s.
- **`ps` mostra quem gasta CPU.** O processo 0 tem 0 tiques porque só se
  conta tempo no anel 3. O `contador` 1 já tinha 153 tiques: ele **não
  dorme** enquanto espera, fica perguntando `tiques()` sem parar (espera
  ocupada). Quem espera assim gasta a CPU dos outros; o remédio seria uma
  chamada de sistema `dormir`, que deixa o processo fora do rodízio até a
  hora certa (seção "O que ainda falta").
- **Nada vaza:** antes desta sessão o `mem` também dava 20298 quadros
  livres e 224 bytes de heap em uso. (São 4 alocações vivas, 2 a mais que
  no doc 11: o `Vec` da tabela de processos e o `Box` do processo 0.)

### 10.2 `cargo test`

40 testes, todos `[ok]` (os 36 de antes e 4 novos em `escalonador.rs`).
Os de concorrência imprimem o tempo medido:

```
siliconos::escalonador::testes::dois_contadores_ao_mesmo_tempo ... 1 2 1 2 1 2 1 2 1 2 (251 tiques) [ok]
siliconos::escalonador::testes::dois_lacos_dividem_a_cpu ... (400 tiques) [ok]
siliconos::escalonador::testes::o_kernel_e_o_processo_zero ... [ok]
siliconos::escalonador::testes::processo_terminado_devolve_pilha_e_quadros ... Ola do anel 3! (escrito por uma chamada de sistema)
[ok]
```

- **Dois contadores, 251 tiques.** Cada um leva 5 × 50 = 250 tiques de
  relógio. Um depois do outro seriam 500; juntos, 251.
- **Dois laços, 400 tiques.** Cada laço só morre depois de 200 tiques **de
  CPU**. Como dividem a CPU meio a meio, os dois juntos levam 400 tiques
  de relógio: a prova de que o rodízio é justo.

```rust
#[test_case]
fn dois_contadores_ao_mesmo_tempo() {
    // Cada contador leva 5 x 50 = 250 tiques. Um depois do outro: 500.
    // Ao mesmo tempo, o total fica perto de 250.
    let inicio = timer::tiques();
    let (a, b) = (iniciar("contador"), iniciar("contador"));
    assert_eq!(esperar(a), Resultado::Saiu(0));
    assert_eq!(esperar(b), Resultado::Saiu(0));
    let total = timer::tiques() - inicio;
    crate::kprint!("({} tiques) ", total);
    assert!(total < 300, "levou {} tiques", total);
}

#[test_case]
fn dois_lacos_dividem_a_cpu() {
    // Cada laço só é encerrado depois de 200 tiques DE CPU. Dividindo a
    // CPU, os dois juntos precisam de uns 400 tiques de relógio.
    let inicio = timer::tiques();
    let (a, b) = (iniciar("laco"), iniciar("laco"));
    assert_eq!(esperar(a), Resultado::TempoEsgotado);
    assert_eq!(esperar(b), Resultado::TempoEsgotado);
    let total = timer::tiques() - inicio;
    crate::kprint!("({} tiques) ", total);
    assert!(total >= 2 * usuario::LIMITE_TIQUES - 10, "levou so {} tiques", total);
}

#[test_case]
fn processo_terminado_devolve_pilha_e_quadros() {
    let (livres, heap_antes) = (quadros::livres(), heap::estatisticas().em_uso);
    let pid = iniciar("ola");
    assert_eq!(esperar(pid), Resultado::Saiu(42));
    assert_eq!(quadros::livres(), livres);
    assert_eq!(heap::estatisticas().em_uso, heap_antes);
    assert!(listar().iter().all(|p| p.0 != pid));
}
```

Os 8 testes do doc 10 (`ola_sai_com_42`, `laco_encerrado_pelo_timer`...)
passaram sem mudar uma linha: `usuario::rodar` continua devolvendo o mesmo
`Resultado`, só que agora pelo escalonador.

### 10.3 E sem `definir_rsp0`?

Para ver a seção 3 acontecer, a linha `gdt::definir_rsp0(p.topo_pilha());`
foi comentada (e depois restaurada). Todos os programas passam a usar a
mesma pilha do anel 0 (a `PILHA_ANEL0` do doc 10, que continua no RSP0).
O `cargo test` cai no primeiro teste com dois programas:

```
siliconos::escalonador::testes::dois_contadores_ao_mesmo_tempo ... 2 2 2 2 2 [kernel] PAGE FAULT no programa (RIP=0x200000000057, CS=0x23): ao ler 0x0 (pagina nao mapeada, modo usuario)

!!! KERNEL PANIC !!!
Local: src/escalonador.rs:343:9
Mensagem: assertion `left == right` failed
  left: Falha { excecao: "PAGE FAULT", rip: 35184372088919 }
 right: Saiu(0)
```

O que aconteceu: só os `2` aparecem, e o processo 1 morre em
`0x2000_0000_0057`. Desmontando o `contador`, a última instrução é o
`int 0x80` do `sair`, nos bytes `0x55`–`0x56`; **`0x57` é o endereço de
retorno dessa chamada**, logo depois do fim do programa. Quando o processo 2
chamou `sair`, a CPU empilhou o quadro dele (RIP = `...0057`) na pilha
compartilhada, **por cima do quadro que o processo 1 tinha deixado ali**.
Ao voltar a vez do 1, o `iretq` usou o quadro do 2 e mandou o 1 para
`...0057`. Ali a página tem zeros, que a CPU lê como `add [rax], al`; com
RAX = 0, page fault ao ler o endereço 0. É o diagrama da seção 3,
acontecendo de verdade.

### 10.4 Regressão

As 21 builds com features de teste continuam passando.

## O que ainda falta

- **Espera ocupada.** Não há como um programa dormir: `contador` e
  `relogio` gastam CPU perguntando a hora. Falta um estado `Dormindo` (ou
  `Bloqueado`) e uma chamada `dormir(tiques)`; o mesmo valeria para o
  shell esperando o teclado.
- **Rodízio puro.** Sem prioridades: todos ganham a mesma fatia.
- **Uma CPU.** Com várias (SMP), cada uma teria o seu "atual", e os locks
  com interrupções desligadas não bastariam (outra CPU continua rodando).
- **Processos só a partir de programas embutidos.** O próximo passo
  carrega programas de "arquivos" no formato ELF. *(Feito no [doc 13](13-disco-em-ram-e-elf.md).)*

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 11 (tag `doc-11`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-12`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-11...doc-12).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/escalonador.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | **novo**: `Estado`, `Processo`, `Tabela`, `TABELA`, `init`, `criar`, `primeira_entrada`, `trocar`, `ceder`, `escolher_proximo`, `tique`, `encerrar`, `retirar`, `recolher`, `recolher_segundo_plano`, `esperar`, `pid_atual`, `ha_outros_prontos`, `listar`, 4 testes | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/main.rs) | `mod escalonador;`; `escalonador::init()` | `mod` depois de `mod usuario;`; `init` logo antes de `interrupcoes::init()` |
| [`src/gdt.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/gdt.rs) | `use core::cell::UnsafeCell;`; `CelulaTss`; `TSS` vira `Once<CelulaTss>`; `init` embrulha a TSS; `topo_pilha_anel0` e `topo_pilha_double_fault` leem pela célula; `definir_rsp0` novo | veja os trechos abaixo |
| [`src/interrupcoes/timer.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/interrupcoes/timer.rs) | sai o bloco de `tempo_esgotado`; entra `escalonador::tique(&quadro)` no fim | `tratador` |
| [`src/memoria/quadros.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/memoria/quadros.rs) | `use crate::cpu;`; `alocar`, `liberar`, `livres` dentro de `sem_interrupcoes` | os três atalhos |
| [`src/memoria/heap.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/memoria/heap.rs) | `estatisticas` e `contem` dentro de `sem_interrupcoes` | as duas funções |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/mod.rs) | saem `ATIVO`, `INICIO`, `RESULTADO`, `RSP_KERNEL`, `encerrar`, `tempo_esgotado`, `entrar_no_anel3`, `voltar_ao_kernel`; entram `SYS_PID`, `descrever`, `iniciar`; `rodar` = `iniciar` + `esperar`; `falha_do_programa` chama `escalonador::encerrar`; `tratar_syscall` com `SYS_PID` | veja os trechos abaixo |
| [`src/usuario/programas.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/programas.rs) | programa `contador` (assembly, símbolos, entrada em `PROGRAMAS`, que passa a ter 7); descrição de `laco` | fim do `global_asm!`; fim do bloco `extern`; fim de `PROGRAMAS` |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/shell.rs) | `use crate::escalonador;`; `rodar ... &`; comando `ps`; laço de `rodar()` recolhe e cede; `mem` lê `QUADROS` com interrupções desligadas | — |

### Os trechos que o texto não mostrou

Em `gdt.rs`, o começo e o fim do `init` com a célula:

```rust
pub fn init() {
    let tss = TSS.call_once(|| CelulaTss(UnsafeCell::new({
        // A pilha cresce para baixo: a CPU precisa do endereço do TOPO.
        // ... (o mesmo de antes) ...
            iomap_base: size_of::<Tss>() as u16,
        }
    })));
    let tss = tss.0.get() as *const Tss;

    let gdt = GDT.call_once(|| {
        let (tss_baixo, tss_alto) =
            descritor_tss(tss as u64, (size_of::<Tss>() - 1) as u64);
        // ...
```

e as leituras:

```rust
pub fn topo_pilha_anel0() -> u64 {
    TSS.get().map(|t| unsafe { (&raw const (*t.0.get()).rsp as *const u64).read_unaligned() }).unwrap_or(0)
}

pub fn topo_pilha_double_fault() -> u64 {
    TSS.get().map(|t| { let ist = unsafe { (*t.0.get()).ist }; ist[(IST_DOUBLE_FAULT - 1) as usize] }).unwrap_or(0)
}
```

Em `escalonador.rs`, os `use`, o tamanho da pilha e o topo:

```rust
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::arch::naked_asm;
use spin::Mutex;

use crate::cpu;
use crate::gdt;
use crate::idt::QuadroInterrupcao;
use crate::kprintln;
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao;
use crate::usuario::{self, Resultado};

pub type Pid = u32;

/// Tamanho da pilha do kernel de cada processo.
const TAM_PILHA_KERNEL: usize = 16 * 1024;

impl Processo {
    /// Endereço do topo da pilha do kernel (onde a CPU começa a empilhar).
    fn topo_pilha(&self) -> u64 {
        self.pilha.as_ref().map_or(0, |p| p.as_ptr() as u64 + (p.len() * 8) as u64)
    }
}
```

as consultas:

```rust
pub fn recolher_segundo_plano() {
    loop {
        let retirado = cpu::sem_interrupcoes(|| {
            let mut t = TABELA.lock();
            let i = t.processos.iter().position(|p| p.segundo_plano && matches!(p.estado, Estado::Terminado(_)))?;
            Some(retirar(&mut t, i))
        });
        match retirado {
            Some((processo, _)) => drop(processo),
            None => return,
        }
    }
}

pub fn pid_atual() -> Pid {
    cpu::sem_interrupcoes(|| {
        let t = TABELA.lock();
        t.processos.get(t.atual).map_or(0, |p| p.pid)
    })
}

pub fn ha_outros_prontos() -> bool {
    cpu::sem_interrupcoes(|| {
        let t = TABELA.lock();
        t.processos.iter().enumerate().any(|(i, p)| i != t.atual && p.estado == Estado::Pronto)
    })
}

pub fn listar() -> Vec<(Pid, &'static str, Estado, u64)> {
    cpu::sem_interrupcoes(|| {
        TABELA.lock().processos.iter().map(|p| (p.pid, p.nome, p.estado, p.tiques_cpu)).collect()
    })
}
```

e o começo dos testes:

```rust
#[cfg(test)]
mod testes {
    use super::*;
    use crate::interrupcoes::timer;
    use crate::memoria::{heap, quadros};
    use crate::usuario::programas;

    fn iniciar(nome: &str) -> Pid {
        usuario::iniciar(programas::procurar(nome).unwrap(), false)
    }

    #[test_case]
    fn o_kernel_e_o_processo_zero() {
        assert_eq!(pid_atual(), 0);
        let lista = listar();
        assert_eq!((lista[0].0, lista[0].1, lista[0].2), (0, "kernel", Estado::Rodando));
    }

    // os outros 3 estão na seção 10.2
}
```

Em `usuario/mod.rs`, os `use` (saem os atômicos, o `Mutex`, `gdt` e `cpu`):

```rust
use alloc::format;
use alloc::string::String;
use core::arch::naked_asm;

use crate::escalonador::{self, Pid};
use crate::idt::QuadroInterrupcao;
use crate::interrupcoes::timer;
use crate::memoria::espaco::EspacoEnderecos;
use crate::memoria::paginacao::{self, flags};
use crate::memoria::{quadros, TAMANHO_QUADRO};
use crate::{kprint, kprintln};
```

`descrever` (usada pelo shell e por `encerrar`) e a versão nova de
`falha_do_programa`:

```rust
pub fn descrever(r: Resultado) -> String {
    match r {
        Resultado::Saiu(codigo) => format!("terminou: codigo de saida {}", codigo as i64),
        Resultado::Falha { excecao, rip } => format!("encerrado pelo kernel: {} em {:#x}", excecao, rip),
        Resultado::TempoEsgotado => format!("encerrado pelo kernel: usou {} s de CPU sem terminar",
                                            LIMITE_TIQUES / timer::FREQUENCIA_HZ),
    }
}

pub fn falha_do_programa(quadro: &QuadroInterrupcao, excecao: &'static str, detalhes: core::fmt::Arguments) {
    if quadro.cs & 3 == 3 {
        kprintln!("[kernel] {} no programa (RIP={:#x}, CS={:#x}): {}", excecao, quadro.rip, quadro.cs, detalhes);
        escalonador::encerrar(Resultado::Falha { excecao, rip: quadro.rip });
    }
}
```

e o despacho das chamadas de sistema:

```rust
    r.rax = match r.rax {
        SYS_SAIR => escalonador::encerrar(Resultado::Saiu(r.rdi)),
        SYS_ESCREVER => escrever(r.rdi, r.rsi),
        SYS_TIQUES => timer::tiques(),
        SYS_PID => escalonador::pid_atual() as u64,
        _ => ERRO_SYSCALL,
    };
```

Em `programas.rs`, os símbolos e a entrada na tabela:

```rust
    static programa_contador_inicio: u8;
    static programa_contador_fim: u8;
...
pub static PROGRAMAS: [Programa; 7] = [
        // ...
        Programa { nome: "laco", descricao: "laco infinito (o timer o encerra depois de 2 s de CPU)",
                   inicio: &raw const programa_laco_inicio, fim: &raw const programa_laco_fim },
        Programa { nome: "contador", descricao: "escreve o proprio pid 5 vezes, a cada 500 ms",
                   inicio: &raw const programa_contador_inicio, fim: &raw const programa_contador_fim },
];
```

Em `shell.rs`, as linhas da tabela de comandos e o `ps`:

```rust
    Comando { nome: "rodar", uso: "rodar <programa> [&]", descricao: "roda um programa no anel 3 (& = segundo plano)", funcao: rodar_programa },
    Comando { nome: "ps", uso: "ps", descricao: "lista os processos", funcao: ps },
```

```rust
fn rodar_programa(argumentos: &[&str]) {
    use crate::usuario;
    let Some(programa) = argumentos.first().and_then(|n| usuario::programas::procurar(n)) else {
        kprintln!("uso: rodar <programa> [&]   (veja 'programas')");
        return;
    };
    // ... o if/else da seção 9.3 ...
}

/// Comando `ps` (doc 12): a tabela de processos.
fn ps(_: &[&str]) {
    use crate::escalonador::Estado;
    kprintln!("  PID  ESTADO      CPU (tiques)  NOME");
    for (pid, nome, estado, tiques) in escalonador::listar() {
        let estado = match estado {
            Estado::Pronto => "pronto",
            Estado::Rodando => "rodando",
            Estado::Terminado(_) => "terminado",
        };
        kprintln!("  {:>3}  {:<10}  {:>12}  {}", pid, estado, tiques, nome);
    }
}
```

e no laço de `shell::rodar()`, o comentário das três linhas da seção 9.3:

```rust
        // Liberar a memória dos programas de segundo plano que terminaram,
        // e dar a CPU a eles enquanto ninguém digita (doc 12). Sem nenhum
        // outro processo pronto, dormir até a próxima interrupção.
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `init` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Registra o código que está rodando (o kernel) como processo 0. |
| `criar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Cria um processo com pilha do kernel fabricada (6 registradores, retorno para `primeira_entrada`, quadro de `iretq`) e o põe na tabela como `Pronto`. |
| `primeira_entrada` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | (naked) Onde o primeiro `trocar` de um processo novo "retorna": zera os registradores e faz `iretq` para o anel 3. |
| `trocar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | (naked) A troca de contexto: empilha 6 registradores, salva o RSP, carrega o RSP do outro processo, desempilha e `ret`. |
| `ceder` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Com as interrupções desligadas, escolhe o próximo e troca para ele; volta quando chegar de novo a vez deste. |
| `escolher_proximo` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Rodízio: acha o próximo `Pronto`, atualiza os estados, troca TSS.RSP0 e CR3 e devolve os dois RSP para `trocar`. |
| `tique` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Chamada pelo timer: conta o tempo de CPU do programa interrompido no anel 3, encerra quem passou do limite e cede a vez. |
| `encerrar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Marca o processo atual como `Terminado`, imprime o resultado se for de segundo plano e cede para nunca mais voltar. |
| `retirar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Tira um processo terminado da tabela (corrigindo o índice do atual) e devolve o processo e o resultado. |
| `recolher` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Se o processo `pid` terminou, retira-o, libera pilha e espaço e devolve o resultado. |
| `recolher_segundo_plano` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Libera todos os processos de segundo plano que já terminaram. |
| `esperar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Cede a CPU até o processo `pid` terminar e devolve o resultado. |
| `pid_atual` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | O pid de quem está rodando (a syscall `pid`). |
| `ha_outros_prontos` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Há algum processo, além do atual, pronto para rodar? |
| `listar` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Pid, nome, estado e tiques de CPU de cada processo (comando `ps`). |
| `Processo::topo_pilha` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Endereço do topo da pilha do kernel do processo (o valor de TSS.RSP0). |
| `definir_rsp0` | [gdt.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/gdt.rs) | Escreve TSS.rsp[0] (pela `UnsafeCell`, com escrita não alinhada): a pilha usada na próxima interrupção vinda do anel 3. |
| `descrever` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/mod.rs) | Texto curto do resultado de um programa ("terminou: codigo de saida 42"...). |
| `iniciar` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/usuario/mod.rs) | Prepara o espaço de endereços do programa (código e pilha) e o entrega ao escalonador; devolve o pid. |
| `ps` | [shell.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/shell.rs) | Comando `ps`: a tabela de processos. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `iniciar` (dos testes) | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Ajudante: inicia um programa embutido pelo nome, sem segundo plano. |
| `o_kernel_e_o_processo_zero` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Teste: quem roda os testes é o processo 0, "kernel", `Rodando`. |
| `dois_contadores_ao_mesmo_tempo` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Teste: dois `contador` juntos levam menos de 300 tiques (um depois do outro seriam 500). |
| `dois_lacos_dividem_a_cpu` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Teste: dois `laco` juntos levam pelo menos 390 tiques de relógio, porque cada um precisa de 200 de CPU. |
| `processo_terminado_devolve_pilha_e_quadros` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-12/src/escalonador.rs) | Teste: depois de recolhido, o processo some da tabela e quadros e heap voltam ao que eram. |

---

## Diferença em relação ao livro

O roteiro da seção 11.4 do livro termina em "troca cooperativa, depois
preemptiva". Aqui as duas vieram juntas: `ceder` é a cooperativa (o shell a
usa quando não tem o que fazer), e o timer chamando `ceder` a cada tique é a
preemptiva. A pilha do kernel por processo, a pilha fabricada com
`primeira_entrada`, o processo 0 como o próprio kernel, o tempo de CPU por
processo e a regra dos locks são escolhas nossas.

## Resumo

- Um **processo** é um programa em execução: espaço de endereços, pilha do
  kernel e contexto salvo. O **escalonador** decide quem roda; o nosso faz
  **rodízio**, um tique (10 ms) para cada.
- **Cada processo tem a sua pilha do kernel**, porque o quadro de
  interrupção de quem foi trocado precisa ficar guardado. A cada troca,
  TSS.RSP0 recebe o topo da pilha de quem vai rodar.
- **`trocar`** salva 6 registradores na pilha atual, guarda o RSP, carrega
  o RSP do outro e desempilha os registradores dele: o `ret` volta na
  história do outro processo.
- Um processo novo começa de uma **pilha fabricada** que termina em
  `primeira_entrada`, cujo `iretq` entra no anel 3.
- O **timer** (depois do EOI) chama `tique`, que conta o tempo de CPU e
  cede a vez: preempção. O limite de 2 s virou tempo de CPU.
- Um processo que termina vira **`Terminado`** e é liberado por outro
  (`recolher`), nunca por ele mesmo, que está rodando na pilha a liberar.
- **Todo lock do kernel é pego com as interrupções desligadas**, para que
  ninguém seja trocado segurando um lock.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que uma pilha do anel 0 só não basta com dois processos?**
   Porque o quadro de interrupção do processo trocado fica nela até ele
   voltar. Quando o segundo processo é interrompido, a CPU empilharia o
   quadro dele no mesmo lugar, por cima; o primeiro voltaria com o RIP e o
   RSP do segundo.

2. **Por que `trocar` salva só 6 registradores?**
   Porque, para quem chama, `trocar` é uma função comum: pela convenção
   sysv64, RBX, RBP e R12–R15 têm que sobreviver à chamada, e os outros
   podem ser destruídos. O que importa do resto do contexto (o quadro do
   anel 3, os registradores do programa) já está na pilha do kernel do
   processo.

3. **Como um processo que nunca rodou "volta" de `trocar`?**
   `criar` fabrica a pilha dele: 6 zeros, o endereço de `primeira_entrada`
   no lugar do endereço de retorno e, acima, um quadro de `iretq`. O `ret`
   de `trocar` cai em `primeira_entrada`, que faz o `iretq` para o anel 3.

4. **Por que o tratador do timer manda o EOI antes de chamar `tique`?**
   Porque `tique` pode trocar de processo, e o tratador só termina quando
   este processo voltar. Sem o EOI, o APIC não entregaria mais tiques, e o
   processo seguinte rodaria para sempre.

5. **Por que `escolher_proximo` solta o lock antes de `trocar`?**
   Porque o processo que vai rodar também usa a tabela (no próximo tique,
   ou em `esperar`). Se o lock ficasse preso pelo processo que saiu da CPU,
   o seguinte giraria para sempre esperando.

6. **Por que um processo terminado não libera a própria memória em
   `encerrar`?**
   Porque `encerrar` roda na pilha do kernel desse processo (e com o CR3
   dele). Liberar os dois enquanto estão em uso faria a próxima instrução
   ler memória que o alocador pode ter entregado a outro.

7. **O que daria errado se `quadros::alocar` pegasse o lock com as
   interrupções ligadas?**
   O timer poderia trocar de processo com o lock preso. Se o próximo
   processo pedisse um quadro com as interrupções desligadas (numa chamada
   de sistema, por exemplo), giraria para sempre: nenhum tique viria
   devolver a CPU a quem segura o lock.

</details>
