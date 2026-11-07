# 17: VFS e arquivos (abrir, ler, fechar)

> **Você vai entender:** o que é um **sistema de arquivos virtual** (VFS),
> a camada que deixa o mesmo `abrir` funcionar para arquivos que vêm de
> lugares completamente diferentes; o que é uma **montagem**; o que é um
> **descritor de arquivo** (o número que o `open` devolve) e por que cada
> processo tem a sua tabela deles; as syscalls **`abrir`**, **`ler`** e
> **`fechar`**; arquivos que o kernel **inventa na hora** (como o `/proc` do
> Linux); e por que o kernel precisa conferir que pode **escrever** num
> endereço antes de copiar dados para um programa.
>
> **Arquivos:** `src/vfs/mod.rs`, `src/vfs/disco.rs` e `src/vfs/sis.rs`
> (novos), `src/usuario/mod.rs` (syscalls 5, 6 e 7), `src/escalonador.rs`
> (tabela de arquivos por processo), `src/shell.rs` (`ls [diretorio]`),
> `build.rs` (põe arquivos de texto no disco), `programas/src/lib.rs`
> (`Arquivo`, `sos::cru`), `programas/src/bin/cat.rs` e `conferir.rs` e
> `programas/arquivos/leiame.txt` (novos)
> **Livro:** capítulo 11, seção 11.4 (o roteiro do livro cita o VFS entre o
> ramdisk e o carregador; a implementação é nossa)
> **Testar:** `./run.sh`, depois `ls`, `ls /sis`, `rodar cat /leiame.txt`,
> `rodar cat /sis/tempo /sis/processos` e `rodar conferir`;
> `cargo test --target x86_64-unknown-uefi`
> **Leia antes:** doc 13 (o disco em RAM), doc 10 (validar ponteiros do
> programa), doc 16 (argumentos, que o `cat` usa)
> **Rust novo aqui:** `&'static dyn Trait` num `static`, arrays com
> `[const { None }; N]`, `Option::take`, `find_map`, `Result` que devolve o
> valor de volta no erro e `mem::forget`
> ([§47](rust-basico.md#47-dyn-trait-em-static-arrays-de-option-take-e-forget))
>
> **Código deste capítulo:** [estado no fim do doc 17](https://github.com/moises-dev-git/siliconOS/tree/doc-17) · [o que mudou desde o doc 16](https://github.com/moises-dev-git/siliconOS/compare/doc-16...doc-17) · mapa em [Mudanças nos arquivos](#mudanças-nos-arquivos)

---

## 1. O problema: os programas não leem nada

Desde o doc 13, há um disco no SiliconOS: um arquivo tar dentro do kernel,
com os programas. Mas só o **kernel** o lê: o shell procura o executável no
tar e o carrega. Um programa, lá no anel 3, não tem como ler um arquivo. Não
há syscall para isso, e ele não enxerga a memória do kernel, onde o tar mora
(doc 11).

Um `cat` (o programa que mostra o conteúdo de um arquivo) é impossível. E
programas que leem dados (configuração, texto, imagens) são quase todos.

A solução óbvia seria uma syscall `ler_do_tar(nome, destino)`. Funciona, mas
amarra os programas ao tar. No dia em que houver um disco de verdade, ou um
pendrive, ou arquivos que o kernel gera, cada um precisaria das suas
syscalls, e cada programa teria de saber de onde vem cada arquivo.

## 2. A ideia: uma camada só, várias fontes

Todo sistema Unix resolve isso do mesmo jeito, e a ideia é antiga (o Unix
dos anos 70 já fazia assim; o nome **VFS**, *virtual file system*, vem da Sun,
em 1985):

1. **Os programas só conhecem caminhos** (`/leiame.txt`, `/sis/tempo`) e três
   operações: abrir, ler, fechar.
2. **Cada fonte de arquivos é um "sistema de arquivos"** que sabe fazer essas
   operações do seu jeito: o tar procura no tar; um disco leria setores; o
   `/proc` do Linux inventa o conteúdo na hora.
3. **O VFS fica no meio.** Uma tabela, a das **montagens**, diz qual sistema
   de arquivos cuida de qual parte da árvore de caminhos. O VFS olha o
   caminho, acha a montagem e repassa o resto do caminho para ela.

```
   programa: abrir("/sis/tempo")          abrir("/leiame.txt")
                    │                               │
   ════════ syscall ╪═══════════════════════════════╪═════════════
                    ▼                               ▼
   VFS:        montagem "/sis"                 montagem "/"
               resto: "tempo"                  resto: "leiame.txt"
                    │                               │
                    ▼                               ▼
   sistemas:   Sis (gera o texto agora)        Disco (aponta para o tar)
```

No Linux, `mount` é o comando que acrescenta uma linha a essa tabela: `mount
/dev/sdb1 /mnt/pendrive` diz "o que estiver abaixo de `/mnt/pendrive` é
com o sistema de arquivos deste pendrive". Aqui a tabela é fixa, com duas
montagens: o disco em RAM na raiz `/` e um sistema de arquivos novo em `/sis`.

### 2.1 Arquivos que não existem

`/sis` é a parte divertida. Os arquivos dele não estão guardados em lugar
nenhum: quando alguém abre `/sis/tempo`, o kernel **escreve o texto naquele
momento**, com o tempo de agora. É a ideia do `/proc` do Linux, onde
`/proc/uptime` (há quanto tempo a máquina está ligada) e `/proc/meminfo`
(memória) são gerados a cada leitura. Num Linux, `ls -l` mostra esses
arquivos com tamanho 0, porque o tamanho só existe depois de gerar:

```
$ ls -l /proc/uptime /proc/meminfo      (no computador de desenvolvimento)
... 0 /proc/meminfo
... 0 /proc/uptime
$ cat /proc/uptime
16154.35 253265.61
```

Para o programa, não há diferença nenhuma: ele abre, lê e fecha como
qualquer arquivo. É isso que o VFS compra: o `cat` deste capítulo mostra o
`leiame.txt` (do tar) e o `/sis/processos` (gerado) sem saber que um é
diferente do outro.

### 2.2 Descritores

Abrir um arquivo cria um estado: **qual** arquivo, e **até onde** ele já foi
lido. O kernel guarda esse estado e devolve ao programa só um número, o
**descritor de arquivo** (*file descriptor*, o `fd` do C). Nas chamadas
seguintes, o programa diz o número: "leia 64 bytes do 3".

Por que um número, e não um ponteiro para o estado? Pelo mesmo motivo de
sempre: o estado mora na memória do kernel, que o programa não pode tocar.
Um número é só um índice numa tabela que o kernel guarda **por processo**: o
descritor 3 de um programa não tem nada a ver com o 3 de outro. Um programa
não consegue ler os arquivos de outro chutando números.

Dois detalhes do Unix que o SiliconOS segue:

- **0, 1 e 2 são reservados**: a entrada, a saída e a saída de erros padrão
  (o teclado e a tela, num terminal). O SiliconOS ainda não os tem (a
  escrita na tela usa a syscall `escrever` do doc 10), mas deixa os números
  livres. O primeiro arquivo aberto ganha o 3.
- **O menor número livre é reaproveitado.** Fechar o 3 e abrir outro dá o 3
  de novo.

Os dois dá para ver no Linux, com três linhas de Python que abrem, fecham e
reabrem (no computador de desenvolvimento):

```python
a = os.open('/etc/hostname', os.O_RDONLY); b = os.open(...); os.close(a); c = os.open(...)
```
```
fds: 3 4 3
```

## 3. O VFS: `src/vfs/mod.rs`

### 3.1 O contrato de um sistema de arquivos

```rust
/// O que um sistema de arquivos precisa saber fazer para ser montado.
/// `Sync`: as montagens são `static` e podem ser usadas de qualquer processo.
pub trait SistemaDeArquivos: Sync {
    /// Abre um arquivo. `nome` é o caminho DENTRO da montagem, sem a barra
    /// do começo ("leiame.txt", "tempo"). Nossos sistemas não têm
    /// subdiretórios.
    fn abrir(&self, nome: &str) -> Option<Conteudo>;
    /// Os arquivos desta montagem: (nome, tamanho em bytes).
    fn listar(&self) -> Vec<(String, usize)>;
}
```

Uma *trait* é uma interface ([§5](rust-basico.md#5-traits-interfaces)): qualquer tipo que implemente `abrir` e
`listar` pode ser montado. O `: Sync` exige que o tipo possa ser usado por
vários processos ao mesmo tempo; é preciso porque as montagens ficam numa
variável `static`, visível de qualquer lugar
([§47](rust-basico.md#47-dyn-trait-em-static-arrays-de-option-take-e-forget)).

O que `abrir` devolve é o conteúdo:

```rust
/// Os bytes de um arquivo aberto. Os do disco já estão na memória (o tar
/// está dentro do kernel): basta apontar para eles. Os de /sis são gerados
/// no momento do `abrir`, uma "fotografia" do estado do kernel.
pub enum Conteudo {
    Fixo(&'static [u8]),
    Gerado(Vec<u8>),
}

impl Conteudo {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Conteudo::Fixo(b) => b,
            Conteudo::Gerado(v) => v,
        }
    }
}
```

Essa é a grande simplificação deste VFS: **todo arquivo aberto já está
inteiro na memória**. Com o tar é verdade de graça. Com `/sis`, o texto
inteiro é gerado no `abrir`, e as leituras seguintes leem dessa fotografia
(abrir `/sis/tempo` e ler 10 minutos depois mostra a hora do `abrir`). Num
disco de verdade, isso seria impossível (um arquivo pode ter gigabytes), e o
VFS do Linux pede a cada sistema de arquivos uma função `read` que busca só
o pedaço pedido. A seção "O que ainda falta" volta a isso.

### 3.2 As montagens

```rust
/// Um caminho montado e o sistema de arquivos que cuida dele.
struct Montagem {
    caminho: &'static str,
    sistema: &'static dyn SistemaDeArquivos,
}

/// A tabela de montagens. A ordem importa: a mais específica primeiro
/// ("/sis" antes de "/"), porque `procurar` fica com a primeira que serve.
static MONTAGENS: &[Montagem] = &[
    Montagem { caminho: "/sis", sistema: &sis::Sis },
    Montagem { caminho: "/", sistema: &disco::Disco },
];
```

`&'static dyn SistemaDeArquivos` é "uma referência para alguma coisa que
implementa `SistemaDeArquivos`, que vive para sempre" ([§34](rust-basico.md#34-dyn-trait-e-implementar-para-todo-t)): `Sis` e `Disco`
são tipos diferentes, mas cabem na mesma tabela porque o que fica guardado é
a referência mais a tabela de funções da trait. É o equivalente a um array
de `SistemaDeArquivos` em Java, onde cada elemento é de uma classe que
implementa a interface.

A busca:

```rust
fn procurar(caminho: &str) -> Option<(&'static Montagem, &str)> {
    let caminho = caminho.strip_prefix('/').unwrap_or(caminho);
    MONTAGENS.iter().find_map(|m| {
        let base = m.caminho.trim_start_matches('/');
        if base.is_empty() {
            return Some((m, caminho)); // a raiz serve para tudo
        }
        let resto = caminho.strip_prefix(base)?;
        // "/sis/tempo" e "/sis" são de /sis; "/sistema" não é.
        if resto.is_empty() {
            Some((m, resto))
        } else {
            resto.strip_prefix('/').map(|r| (m, r))
        }
    })
}
```

`find_map` percorre as montagens e devolve o primeiro `Some` que a closure
produzir. Dentro dela, `strip_prefix(base)?` tira o começo do caminho ou,
se o caminho não começa com aquilo, faz a closure devolver `None` (o `?`
funciona com `Option` também, [§24](rust-basico.md#24-o-operador--propagar-erros)), e a busca passa para a próxima montagem.

O cuidado com `/sistema` é o detalhe que mais se erra nesse tipo de código:
`"sistema".strip_prefix("sis")` dá `"tema"`, e sem conferir que o resto
começa com `/` (ou é vazio), `/sistema` cairia na montagem `/sis`, como
arquivo `tema`. Um teste cobre exatamente isso (seção 8.3).

Caminhos sem a barra do começo (`leiame.txt`) são tratados como se tivessem.
Num Unix, eles seriam relativos ao **diretório atual** do processo; o
SiliconOS ainda não tem diretório atual, então "relativo" e "a partir da
raiz" são a mesma coisa.

Com a montagem em mãos, abrir é repassar:

```rust
/// Abre um arquivo pelo caminho completo.
pub fn abrir(caminho: &str) -> Option<ArquivoAberto> {
    let (montagem, nome) = procurar(caminho)?;
    if nome.is_empty() {
        return None; // é um diretório ("/sis"), não um arquivo
    }
    montagem.sistema.abrir(nome).map(|conteudo| ArquivoAberto { conteudo, posicao: 0 })
}
```

### 3.3 Listar, com as montagens no meio

O `ls` do shell precisa mostrar, em `/`, os arquivos do tar **e** o
diretório `sis/`. Mas o tar não sabe que `/sis` existe: quem sabe é a tabela
de montagens. Então o VFS junta as duas coisas:

```rust
pub fn listar(diretorio: &str) -> Option<Vec<(String, usize)>> {
    let (montagem, nome) = procurar(diretorio)?;
    if !nome.is_empty() {
        return None; // nossos sistemas não têm subdiretórios
    }
    let mut itens = montagem.sistema.listar();
    for m in MONTAGENS {
        // Uma montagem "/a" aparece em "/" como "a/".
        if m.caminho != montagem.caminho && pai(m.caminho) == montagem.caminho {
            itens.push((String::from(m.caminho.rsplit('/').next().unwrap()) + "/", 0));
        }
    }
    Some(itens)
}

/// O diretório que contém `caminho`: pai("/sis") = "/".
fn pai(caminho: &str) -> &str {
    match caminho.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &caminho[..i],
    }
}
```

No Linux é igual: o diretório `/proc` aparece no `ls /` porque é um **ponto
de montagem**, não porque o sistema de arquivos da raiz tenha uma pasta
`proc` com algo dentro. (Ele tem uma pasta vazia, que o `/proc` cobre ao ser
montado; aqui, nem isso.)

### 3.4 O arquivo aberto

```rust
/// Um arquivo aberto: o conteúdo e até onde ele já foi lido.
pub struct ArquivoAberto {
    conteudo: Conteudo,
    posicao: usize,
}

impl ArquivoAberto {
    /// Copia para `destino` os próximos bytes e avança a posição. Devolve
    /// quantos copiou: menos que `destino.len()` perto do fim, 0 no fim.
    pub fn ler(&mut self, destino: &mut [u8]) -> usize {
        let restante = &self.conteudo.bytes()[self.posicao..];
        let n = restante.len().min(destino.len());
        destino[..n].copy_from_slice(&restante[..n]);
        self.posicao += n;
        n
    }

    /// Quantos bytes ainda faltam ler.
    pub fn restante(&self) -> usize {
        self.conteudo.bytes().len() - self.posicao
    }
}
```

`ler` segue o contrato do `read` do Unix, que tem uma pegadinha: ele pode
devolver **menos** bytes do que foi pedido sem que isso seja erro. Perto do
fim do arquivo, devolve o que sobrou; no fim, devolve 0. É o 0 (e não um
erro) que diz "acabou". Por isso todo laço de leitura tem a forma "leia até
vir 0", como o do `cat` (seção 6.2).

### 3.5 A tabela de descritores

```rust
/// Quantos arquivos um processo pode ter abertos ao mesmo tempo.
pub const MAX_ABERTOS: usize = 16;
/// O primeiro descritor. 0, 1 e 2 são, por convenção do Unix, a entrada, a
/// saída e a saída de erros padrão; ficam reservados para quando o
/// SiliconOS tiver um terminal.
pub const PRIMEIRO_DESCRITOR: u64 = 3;

/// A tabela de arquivos abertos de um processo. O descritor `d` é a posição
/// `d - PRIMEIRO_DESCRITOR`. Quando o processo é recolhido, a tabela morre
/// com ele e todo arquivo que ficou aberto é liberado (o `Drop` dos `Vec`).
pub struct Descritores {
    abertos: [Option<ArquivoAberto>; MAX_ABERTOS],
}

impl Descritores {
    pub const fn novo() -> Self {
        Descritores { abertos: [const { None }; MAX_ABERTOS] }
    }
```

Um array de 16 posições, cada uma vazia (`None`) ou com um arquivo. O
`[const { None }; MAX_ABERTOS]` é a forma de criar um array repetindo um valor
que não pode ser copiado (`ArquivoAberto` tem um `Vec` dentro; [§47](rust-basico.md#47-dyn-trait-em-static-arrays-de-option-take-e-forget)).

16 é bem pouco: no computador de desenvolvimento, `ulimit -n` (o limite de
arquivos abertos por processo do Linux) responde 1048576. Mas um limite
precisa existir: sem ele, um programa poderia abrir arquivos em laço até
esgotar o heap do kernel, e quem pagaria seria a máquina inteira.

```rust
    /// Guarda o arquivo na primeira posição livre e devolve o descritor, ou
    /// devolve o arquivo de volta se a tabela está cheia.
    pub fn guardar(&mut self, arquivo: ArquivoAberto) -> Result<u64, ArquivoAberto> {
        match self.abertos.iter().position(|a| a.is_none()) {
            Some(i) => {
                self.abertos[i] = Some(arquivo);
                Ok(i as u64 + PRIMEIRO_DESCRITOR)
            }
            None => Err(arquivo),
        }
    }

    /// O arquivo do descritor `d`, se ele estiver aberto.
    pub fn arquivo(&mut self, d: u64) -> Option<&mut ArquivoAberto> {
        let i = d.checked_sub(PRIMEIRO_DESCRITOR)? as usize;
        self.abertos.get_mut(i)?.as_mut()
    }

    /// Fecha o descritor `d` (e libera o arquivo). Falso se não estava aberto.
    pub fn fechar(&mut self, d: u64) -> bool {
        let Some(i) = d.checked_sub(PRIMEIRO_DESCRITOR) else { return false };
        self.abertos.get_mut(i as usize).and_then(|a| a.take()).is_some()
    }
```

Três funções curtas, e cada uma tem um cuidado:

- **`guardar`** procura a **primeira** posição vazia: é isso que faz o menor
  descritor livre ser o reaproveitado. Se a tabela está cheia, devolve o
  arquivo **de volta** dentro do `Err`. Em Rust, `arquivo` foi movido para
  dentro da função; se ela não o devolvesse, quem chamou o perderia de vez
  (aqui tanto faz, ele seria liberado, mas o tipo deixa a escolha com quem
  chamou; [§47](rust-basico.md#47-dyn-trait-em-static-arrays-de-option-take-e-forget)).
- **`arquivo`** recebe um número que veio do programa, e portanto pode ser
  qualquer coisa: 0, 2, 99, `u64::MAX`. `checked_sub` devolve `None` em vez
  de dar a volta (um `0 - 3` em `u64` viraria um número enorme, [§45](rust-basico.md#45-impl-fn-variantes-com-campos-e-aritmética-sem-estouro)), e
  `get_mut` devolve `None` para um índice fora do array, em vez de dar
  panic como `abertos[i]` daria. Nenhum número faz o kernel cair.
- **`fechar`** usa `take()`, que tira o valor de dentro do `Option` e deixa
  `None` no lugar. O arquivo tirado morre ali mesmo, e o `Vec` de um
  conteúdo gerado volta para o heap.

## 4. Os dois sistemas de arquivos

### 4.1 O disco: `src/vfs/disco.rs`

```rust
pub struct Disco;

impl SistemaDeArquivos for Disco {
    fn abrir(&self, nome: &str) -> Option<Conteudo> {
        // Os dados já estão no tar, dentro do kernel: nada é copiado.
        ramdisk::abrir(nome).map(|a| Conteudo::Fixo(a.dados))
    }

    fn listar(&self) -> Vec<(String, usize)> {
        ramdisk::arquivos().map(|a| (String::from(a.nome), a.dados.len())).collect()
    }
}
```

Um **adaptador**: todo o trabalho de ler o tar já existia (doc 13); `Disco`
só o veste com a interface do VFS. `pub struct Disco;` é uma *struct* sem
campos (não há estado: o tar é um `static`), que existe só para ter onde
pendurar a implementação da trait.

### 4.2 O kernel por dentro: `src/vfs/sis.rs`

```rust
pub struct Sis;

/// Os arquivos de /sis e a função que gera cada um.
const ARQUIVOS: &[(&str, fn() -> String)] = &[
    ("tempo", tempo),
    ("mem", mem),
    ("processos", processos),
];

impl SistemaDeArquivos for Sis {
    fn abrir(&self, nome: &str) -> Option<Conteudo> {
        let (_, gerar) = ARQUIVOS.iter().find(|(n, _)| *n == nome)?;
        Some(Conteudo::Gerado(gerar().into_bytes()))
    }

    fn listar(&self) -> Vec<(String, usize)> {
        // O tamanho é o de agora: gerar e medir.
        ARQUIVOS.iter().map(|(n, gerar)| (String::from(*n), gerar().len())).collect()
    }
}
```

A mesma ideia da tabela de comandos do shell (doc 09): um nome e um
ponteiro de função ([§31](rust-basico.md#31-ponteiros-de-função-fn)). Acrescentar um arquivo em `/sis` é uma linha
aqui e uma função que monta o texto. As três de hoje:

```rust
fn tempo() -> String {
    format!("{} ms ({} tiques a {} Hz)\n", timer::milissegundos(), timer::tiques(), timer::FREQUENCIA_HZ)
}

fn mem() -> String {
    let e = heap::estatisticas();
    format!("quadros livres: {}\nheap em uso: {} bytes\n", quadros::livres(), e.em_uso)
}

fn processos() -> String {
    let mut texto = String::from("PID NOME\n");
    for (pid, nome, _, _) in escalonador::listar() {
        texto += &format!("{:>3} {}\n", pid, nome);
    }
    texto
}
```

Diferente do Linux, `listar` mede o tamanho de verdade, gerando o texto (e
jogando fora). Fica caro se o arquivo for grande; para três textos de
poucas dezenas de bytes, não importa.

`processos` chama `escalonador::listar`, que pega o lock da tabela de
processos. Isso vai importar na seção 5.

### 4.3 Arquivos de texto no disco

Até aqui, o tar só tinha executáveis. Para o `cat` ter o que mostrar, o
`build.rs` (doc 13) passa a empacotar também os arquivos da pasta
`programas/arquivos/`, como estão:

```rust
    let mut conteudo: Vec<(String, Vec<u8>)> = nomes.iter()
        .map(|nome| (nome.clone(), fs::read(binarios.join(nome)).unwrap()))
        .collect();
    // Desde o doc 17: os arquivos comuns (texto) de programas/arquivos,
    // copiados como estão.
    for entrada in fs::read_dir(programas.join("arquivos")).unwrap() {
        let caminho = entrada.unwrap().path();
        let nome = caminho.file_name().unwrap().to_string_lossy().into_owned();
        conteudo.push((nome, fs::read(&caminho).unwrap()));
    }
    conteudo.sort();
```

Hoje há um só, `leiame.txt`, de 420 bytes, em ASCII. (Só ASCII porque a
syscall `escrever` do doc 10 troca qualquer byte não ASCII por `?`, e o
console só desenha ASCII.)

## 5. As syscalls

Três números novos em `src/usuario/mod.rs`, e cinco códigos de erro, todos
com os números do Linux (que é de onde vêm os nomes entre parênteses; o
`errno` do C):

```rust
pub const SYS_ABRIR: u64 = 5;   // doc 17
pub const SYS_LER: u64 = 6;
pub const SYS_FECHAR: u64 = 7;
```
```rust
pub const ERRO_NAO_EXISTE: u64 = -2i64 as u64;      // ENOENT: arquivo não existe (doc 17)
pub const ERRO_DESCRITOR: u64 = -9i64 as u64;       // EBADF: descritor não está aberto
pub const ERRO_ENDERECO: u64 = -14i64 as u64;       // EFAULT: ponteiro inválido
pub const ERRO_ABERTOS_DEMAIS: u64 = -24i64 as u64; // EMFILE: tabela de arquivos cheia
pub const ERRO_NOME_LONGO: u64 = -36i64 as u64;     // ENAMETOOLONG: caminho grande demais
pub const ERRO_SYSCALL: u64 = -38i64 as u64;        // ENOSYS: chamada desconhecida

/// O maior caminho que `abrir` aceita, em bytes.
pub const MAX_CAMINHO: u64 = 255;
```

E três linhas em `tratar_syscall`. `ler` é a primeira syscall com **três**
argumentos: o terceiro chega em RDX, que as duas entradas (a do `int 0x80` e
a do `syscall`) já salvavam em `Registros` desde o doc 10.

```rust
        SYS_ABRIR => abrir(r.rdi, r.rsi),
        SYS_LER => ler(r.rdi, r.rsi, r.rdx),
        SYS_FECHAR => fechar(r.rdi),
```

### 5.1 De quem é a tabela de descritores

Cada processo tem a sua, e ela mora no `Processo` do escalonador:

```rust
    /// Os arquivos abertos (doc 17). Morre com o processo: o que ficou
    /// aberto é liberado quando ele é recolhido.
    arquivos: Descritores,
```

O campo não é `pub`: fora do escalonador, a única porta é esta função, que
dá acesso à tabela **do processo atual** (o que fez a syscall):

```rust
/// Roda `f` com a tabela de arquivos abertos do processo atual (doc 17),
/// com as interrupções desligadas e o lock da tabela pego: `f` deve ser
/// curta e não pode chamar nada que pegue o lock de novo (como `listar`).
pub fn com_arquivos<R>(f: impl FnOnce(&mut Descritores) -> R) -> R {
    cpu::sem_interrupcoes(|| {
        let mut t = TABELA.lock();
        let a = t.atual;
        f(&mut t.processos[a].arquivos)
    })
}
```

O mesmo formato de `cpu::sem_interrupcoes` ([§30](rust-basico.md#30-receber-uma-closure-impl-fnonce)): em vez de devolver uma
referência para a tabela (que continuaria viva depois de o lock ser
solto), a função **recebe o código** que quer usar a tabela e o roda com o
lock pego. Não há como esquecer de soltar o lock, nem como guardar a
referência para depois.

O aviso no comentário é sério. O lock da tabela de processos (um
`spin::Mutex`, [§11](rust-basico.md#11-mutex-e-once-da-crate-spin)) não é reentrante: se `f` chamasse algo que pega o
mesmo lock, o processo ficaria girando para sempre esperando a si mesmo, com
as interrupções desligadas. A máquina travaria (é o `teste-deadlock` do doc
08, por outro caminho).

### 5.2 `abrir`

```rust
fn abrir(ponteiro: u64, tamanho: u64) -> u64 {
    if tamanho > MAX_CAMINHO {
        return ERRO_NOME_LONGO;
    }
    if !faixa_do_programa(ponteiro, tamanho) {
        return ERRO_ENDERECO;
    }
    // Copiar o caminho para o kernel antes de usar: a memória é do
    // programa, e o kernel não deve trabalhar em cima dela.
    let bytes = unsafe { core::slice::from_raw_parts(ponteiro as *const u8, tamanho as usize) };
    let Ok(caminho) = core::str::from_utf8(bytes).map(String::from) else {
        return ERRO_NAO_EXISTE; // nenhum arquivo tem um nome que não é UTF-8
    };
    // Abrir FORA do lock da tabela de processos: gerar /sis/processos
    // consulta a tabela (escalonador::listar).
    let Some(arquivo) = crate::vfs::abrir(&caminho) else {
        return ERRO_NAO_EXISTE;
    };
    escalonador::com_arquivos(|d| d.guardar(arquivo)).unwrap_or(ERRO_ABERTOS_DEMAIS)
    // (se a tabela estava cheia, `guardar` devolve o arquivo, que morre aqui)
}
```

Na ordem:

1. **Tamanho antes de tudo.** Antes de olhar a memória, recusar caminhos
   absurdos. 255 bytes é o limite de um **nome** (um pedaço do caminho) no
   Linux, e 4096 o do caminho inteiro (`getconf NAME_MAX /` e `getconf
   PATH_MAX /`, no computador de desenvolvimento). Aqui, sem subdiretórios,
   o caminho é praticamente só o nome.
2. **O ponteiro é do programa?** `faixa_do_programa` (doc 10, refeita no doc
   13): cada página precisa estar mapeada com o bit USUARIO numa entrada da
   PML4 do programa. Ler o caminho de um ponteiro para o kernel seria um
   jeito de o programa espiar a memória do kernel.
3. **Copiar.** O caminho é copiado para uma `String` do kernel antes de ser
   usado. Hoje, com uma CPU só, isso é prudência: nada pode mudar a memória do
   programa durante a syscall. Com várias CPUs, outra *thread* do mesmo
   programa poderia trocar o texto no meio do caminho, entre a conferência e
   o uso (o ataque se chama TOCTOU, *time of check to time of use*). Copiar
   uma vez e trabalhar só na cópia fecha essa porta.
4. **Abrir fora do lock.** `vfs::abrir("/sis/processos")` chama
   `escalonador::listar`, que pega o lock da tabela de processos. Se o
   `abrir` rodasse dentro de `com_arquivos`, o lock já estaria pego: o
   travamento descrito na seção anterior. Por isso o arquivo é aberto
   primeiro e só depois, com o lock, guardado na tabela.

O caminho chega como ponteiro **e tamanho**, o formato de um `&str` do Rust.
O `open` do Linux recebe um texto terminado em 0 (o formato do C, doc 16), e
o kernel precisa procurar o 0 byte a byte, conferindo cada página no
caminho. Com o tamanho, a conferência é uma só.

### 5.3 `ler`, e a página que só pode ser lida

```rust
fn ler(descritor: u64, destino: u64, tamanho: u64) -> u64 {
    escalonador::com_arquivos(|d| {
        let Some(arquivo) = d.arquivo(descritor) else { return ERRO_DESCRITOR };
        // Só o que vai ser copiado de fato precisa ser conferido (e assim um
        // `tamanho` enorme não faz a conferência andar por milhões de páginas).
        let n = (tamanho as usize).min(arquivo.restante());
        if !faixa_gravavel_do_programa(destino, n as u64) {
            return ERRO_ENDERECO;
        }
        let destino = unsafe { core::slice::from_raw_parts_mut(destino as *mut u8, n) };
        arquivo.ler(destino) as u64
    })
}
```

Aqui o kernel faz uma coisa nova: **escreve** na memória do programa. Até o
doc 16, as syscalls só liam dela (`escrever` lê o texto a mostrar). E
escrever pede uma conferência a mais, que `faixa_do_programa` não fazia: a
página precisa ser **gravável**.

Por quê, se o kernel pode tudo? Porque não pode. O doc 06 deixou ligado o bit
**WP** (*write protect*) do CR0: com ele, nem o anel 0 escreve numa página
marcada só para leitura. Isso protege o próprio kernel dos seus erros, mas
também quer dizer que um programa que passe como destino o endereço do seu
próprio código (páginas R-X, doc 13) faria o **kernel** levar um page fault.
Um page fault no anel 0 não é culpa do programa (`falha_do_programa` do doc
10 só encerra o programa se a exceção veio do anel 3): é um kernel panic.

Foi testado. Com a checagem trocada temporariamente pela antiga (sem exigir
escrita), o programa `conferir` (seção 6.3) derrubou a máquina:

```
silicon> rodar conferir
  abrir um arquivo que nao existe                -2 ok
  ...
  fechar o descritor 0 (reservado)               -9 ok

!!! KERNEL PANIC !!!
Local: src/idt.rs:197:5
Mensagem: EXCECAO #14 PAGE FAULT ao escrever em 0x200000000015 (pagina presente, sem permissao, modo kernel)
RIP=0x5fcaca5 CS=0x8 RFLAGS=0x6 RSP=0xffff900000004fc8 SS=0x10
```

`0x2000_0000_0015` é um endereço dentro do código do `conferir` (que começa
em `0x2000_0000_0000`, doc 13). Um ponteiro ruim de **um programa** e o
**sistema inteiro** caiu. Com a checagem certa, o kernel devolve -14 e o
programa segue.

A checagem nova reaproveita a antiga:

```rust
pub fn faixa_do_programa(inicio: u64, tamanho: u64) -> bool {
    faixa_com_flags(inicio, tamanho, flags::USUARIO)
}

/// Como `faixa_do_programa`, mas exige também que o programa possa
/// ESCREVER em cada página (doc 17): é para onde o kernel vai copiar dados.
/// Com CR0.WP ligado (doc 06), o kernel que escreve numa página só de
/// leitura leva um page fault, mesmo no anel 0.
pub fn faixa_gravavel_do_programa(inicio: u64, tamanho: u64) -> bool {
    faixa_com_flags(inicio, tamanho, flags::USUARIO | flags::ESCRITA)
}

/// Toda página de [inicio, inicio + tamanho) é do programa e tem todos os
/// bits de `exigidas` ligados?
fn faixa_com_flags(inicio: u64, tamanho: u64, exigidas: u64) -> bool {
    let Some(fim) = inicio.checked_add(tamanho) else { return false };
    let mut pagina = inicio & !(TAMANHO_QUADRO - 1);
    while pagina < fim {
        let do_programa = EspacoEnderecos::do_programa(pagina)
            && paginacao::traduzir(pagina).is_some_and(|t| t.flags & exigidas == exigidas);
```

O teste mudou de "`flags & USUARIO != 0`" para "`flags & exigidas ==
exigidas`": todos os bits pedidos ligados, e não só algum. (`traduzir`
devolve as flags da entrada final; as tabelas intermediárias do programa são
sempre criadas com ESCRITA e com USUARIO (`paginacao.rs`, doc 10), então a
entrada final é quem decide.)

Outro detalhe de `ler`: o tamanho conferido é `n`, o que **vai ser
copiado**, e não o que o programa pediu. Pedir 1 GiB de um arquivo de 420
bytes confere uma página, não 262 144. E no fim do arquivo (`n = 0`), não há
nada a conferir: devolve 0, como o Linux.

Por fim, a cópia acontece dentro de `com_arquivos`, com o lock pego e as
interrupções desligadas. É seguro porque a cópia é curta (no máximo o
arquivo inteiro, 420 bytes hoje) e porque, depois da conferência, ela não
pode falhar.

### 5.4 `fechar`

```rust
fn fechar(descritor: u64) -> u64 {
    if escalonador::com_arquivos(|d| d.fechar(descritor)) { 0 } else { ERRO_DESCRITOR }
}
```

E quem não fecha? Quando o processo termina e é recolhido, o `Processo`
inteiro é destruído, e com ele a tabela de descritores e todo arquivo que
ainda estava nela (o `Drop`, [§38](rust-basico.md#38-drop-código-que-roda-quando-o-valor-morre)). Um programa com defeito que esquece de
fechar não vaza memória do kernel para sempre: só enquanto ele existe. A
seção 8.3 confere isso.

## 6. O lado dos programas

### 6.1 `sos::Arquivo`

Na biblioteca `sos`, as syscalls ganham duas camadas. A crua, que só passa
números:

```rust
pub mod cru {
    use super::*;

    /// Abre o caminho [ponteiro, ponteiro + tamanho). Descritor ou erro negativo.
    pub fn abrir(ponteiro: u64, tamanho: u64) -> i64 {
        chamada(SYS_ABRIR, ponteiro, tamanho) as i64
    }

    /// Lê até `tamanho` bytes para `destino`. Quantos leu (0 = fim) ou erro.
    pub fn ler(descritor: u64, destino: u64, tamanho: u64) -> i64 {
        chamada3(SYS_LER, descritor, destino, tamanho) as i64
    }

    /// Fecha o descritor. 0 ou erro.
    pub fn fechar(descritor: u64) -> i64 {
        chamada(SYS_FECHAR, descritor, 0) as i64
    }
}
```

(`chamada3` é a `chamada` do doc 14 com um `in("rdx") c` a mais; a de dois
argumentos agora só a chama com `c = 0`.)

E a que um programa normal usa:

```rust
/// Um arquivo aberto. Fecha sozinho quando sai de escopo (`Drop`), como um
/// `File` do Rust ou um try-with-resources do Java.
pub struct Arquivo {
    descritor: u64,
}

impl Arquivo {
    /// Abre o arquivo `caminho` ("/leiame.txt", "/sis/tempo"). O erro é o
    /// código negativo do kernel (veja `nome_do_erro`).
    pub fn abrir(caminho: &str) -> Result<Arquivo, i64> {
        let r = cru::abrir(caminho.as_ptr() as u64, caminho.len() as u64);
        if r < 0 { Err(r) } else { Ok(Arquivo { descritor: r as u64 }) }
    }

    /// Lê os próximos bytes para `destino`. `Ok(0)` = fim do arquivo.
    pub fn ler(&mut self, destino: &mut [u8]) -> Result<usize, i64> {
        let r = cru::ler(self.descritor, destino.as_mut_ptr() as u64, destino.len() as u64);
        if r < 0 { Err(r) } else { Ok(r as usize) }
    }
```
```rust
impl Drop for Arquivo {
    fn drop(&mut self) {
        cru::fechar(self.descritor);
    }
}
```

Duas melhorias sobre os números crus. O erro vira `Result` ([§6](rust-basico.md#6-option-e-result-sem-null-e-sem-exceções)): quem
chama é obrigado a tratar, e o `?` funciona (seção 6.2). E o arquivo **fecha
sozinho** quando a variável sai de escopo, pelo `Drop` ([§38](rust-basico.md#38-drop-código-que-roda-quando-o-valor-morre)): é o padrão que
o Rust chama de RAII e o Java imita com try-with-resources e o Python com
`with open(...)`. Esquecer de fechar fica impossível, mesmo saindo da função
no meio por um `?`.

Os códigos de erro ganham nomes legíveis:

```rust
pub fn nome_do_erro(codigo: i64) -> &'static str {
    match codigo {
        -2 => "nao existe",
        -9 => "descritor nao esta aberto",
        -14 => "endereco invalido",
        -24 => "arquivos abertos demais",
        -36 => "caminho grande demais",
        -38 => "chamada desconhecida",
        _ => "erro desconhecido",
    }
}
```

### 6.2 `cat`

```rust
fn main() -> u64 {
    if sos::quantos_argumentos() < 2 {
        println!("uso: cat <arquivo>...");
        return 2;
    }
    let mut falhou = false;
    for caminho in sos::argumentos().skip(1) {
        if let Err(e) = mostrar(caminho) {
            println!("cat: {}: {}", caminho, sos::nome_do_erro(e));
            falhou = true;
        }
    }
    falhou as u64
}

/// Lê o arquivo aos pedaços de 64 bytes e escreve cada pedaço na tela. O
/// arquivo é fechado no fim da função, pelo `Drop` de `Arquivo`.
fn mostrar(caminho: &str) -> Result<(), i64> {
    let mut arquivo = Arquivo::abrir(caminho)?;
    let mut pedaco = [0u8; 64];
    loop {
        let n = arquivo.ler(&mut pedaco)?;
        if n == 0 {
            return Ok(()); // fim do arquivo
        }
        sos::escrever(&pedaco[..n]);
    }
}
```

O `cat` de verdade tem essa mesma forma: para cada argumento (doc 16),
abrir, ler até vir 0, escrever, fechar. Ler aos pedaços de 64 bytes, e não
o arquivo de uma vez, é de propósito: o programa não sabe o tamanho do
arquivo, e um pedaço fixo na pilha funciona para qualquer tamanho. O
`leiame.txt` vem em 7 leituras (6 de 64 bytes e uma de 36) e mais uma que
devolve 0.

`falhou as u64` converte o `bool` em 0 ou 1: o código de saída é 1 se algum
arquivo falhou, como o `cat` do Linux.

### 6.3 `conferir`: um programa mal-intencionado

Um kernel não pode confiar em nada que vem do anel 3. Para conferir que o
nosso não confia, o programa `conferir` faz de propósito tudo o que um
programa com defeito (ou malicioso) faria, pelas funções cruas, e compara a
resposta com a esperada:

```rust
/// Uma prova: o que foi tentado, a resposta do kernel e a esperada.
fn prova(falhas: &mut u64, descricao: &str, obtido: i64, esperado: i64) {
    let ok = obtido == esperado;
    println!("  {:<44} {:>4} {}", descricao, obtido, if ok { "ok" } else { "FALHOU" });
    if !ok {
        *falhas += 1;
    }
}
```

As provas mais interessantes:

```rust
    prova(f, "abrir com o caminho na memoria do kernel", cru::abrir(0xFFFF_8000_0000_0000, 8), -14);
    let longo = [b'a'; 300];
    prova(f, "abrir com um caminho de 300 bytes", cru::abrir(longo.as_ptr() as u64, 300), -36);
```
```rust
    // Destinos ruins para `ler`: o kernel teria de ESCREVER ali.
    let mut a = Arquivo::abrir("/leiame.txt").unwrap();
    let d = a.descritor();
    let codigo = main as *const () as u64; // uma página de código: só leitura
    prova(f, "ler para dentro do proprio codigo", cru::ler(d, codigo, 16), -14);
    prova(f, "ler para dentro do kernel", cru::ler(d, 0xFFFF_8000_0000_0000, 16), -14);
    let mut buf = [0u8; 16];
    prova(f, "ler 16 bytes para a pilha (certo)", a.ler(&mut buf).unwrap_or(0) as i64, 16);
    prova(f, "...e as leituras recusadas nao avancaram", buf.starts_with(b"Este arquivo mor") as i64, 1);
```

`main as *const () as u64` é o endereço da própria função `main`: um lugar
que o programa pode ler, mas não escrever. A última prova confere um detalhe
fino: as duas leituras recusadas não "gastaram" bytes do arquivo. A leitura
certa, a seguir, ainda começa no primeiro byte ("Este arquivo mor...").

```rust
    // Fechar duas vezes.
    prova(f, "fechar o descritor", cru::fechar(d), 0);
    prova(f, "fechar o mesmo descritor de novo", cru::fechar(d), -9);
    core::mem::forget(a); // já fechado à mão: o Drop não deve fechar de novo
```

`mem::forget(a)` "esquece" o `Arquivo` sem rodar o `Drop` ([§47](rust-basico.md#47-dyn-trait-em-static-arrays-de-option-take-e-forget)). Sem ele, o
`Drop` fecharia o descritor `d` uma terceira vez. Aqui seria inofensivo (o
kernel diria -9), mas num programa de verdade seria um defeito sério: se
outro arquivo tivesse sido aberto nesse meio-tempo, ele teria ganho o mesmo
número (o menor livre), e o `Drop` atrasado fecharia o arquivo **errado**.

Por fim, encher a tabela, e sair sem fechar:

```rust
    // Encher a tabela (16 arquivos por processo).
    let mut abertos = 0;
    let mut ultimo = 0;
    for _ in 0..20 {
        ultimo = cru::abrir("/sis/tempo".as_ptr() as u64, 10);
        if ultimo < 0 { break; }
        abertos += 1;
    }
    prova(f, "quantos arquivos abrem ao mesmo tempo", abertos, 16);
    prova(f, "o 17o e recusado", ultimo, -24);

    // Os 16 ficam abertos de propósito: o kernel tem de liberá-los quando o
    // processo for recolhido (o teste do kernel confere o heap).
```

O código de saída é o número de provas que falharam: 0 quer dizer que o
kernel se defendeu de todas.

## 7. O shell: `ls` pelo VFS

O `ls` do doc 13 lia o tar direto. Agora pergunta ao VFS e aceita um
diretório:

```rust
fn ls(argumentos: &[&str]) {
    let diretorio = argumentos.first().copied().unwrap_or("/");
    let Some(itens) = crate::vfs::listar(diretorio) else {
        kprintln!("'{}' nao e um diretorio (veja 'ls /')", diretorio);
        return;
    };
    for (nome, tamanho) in &itens {
        if nome.ends_with('/') {
            kprintln!("  {:<12}{:>8}        diretorio", nome, "");
            continue;
        }
        // Os primeiros bytes dizem o tipo: só o do disco pode ser ELF.
        let caminho = alloc::format!("{}/{}", diretorio.trim_end_matches('/'), nome);
        let mut inicio = [0u8; 4];
        let elf = crate::vfs::abrir(&caminho).is_some_and(|mut a| a.ler(&mut inicio) == 4 && &inicio == b"\x7fELF");
        kprintln!("  {:<12}{:>8} bytes  {}", nome, tamanho, if elf { "executavel ELF" } else { "" });
    }
    kprintln!("{} itens em {}", itens.len(), diretorio);
}
```

O shell roda no kernel e usa o VFS direto, sem descritores e sem syscall:
`vfs::abrir` devolve o `ArquivoAberto`, que morre no fim da expressão. Para
saber se um arquivo é executável, o `ls` lê os 4 primeiros bytes e procura a
assinatura do ELF (doc 13), em vez de confiar no nome.

`rodar` continua como estava: procura o programa direto no tar, pelo nome
(`rodar primos`). Ele precisa dos bytes do ELF como `&'static [u8]`, que é
exatamente o que o tar dá.

## 8. Como foi testado

### 8.1 Pelo shell

Saída real da serial (teclas enviadas pelo monitor do QEMU):

```
silicon> ls
  args           25816 bytes  executavel ELF
  cat            26240 bytes  executavel ELF
  conferir       26624 bytes  executavel ELF
  dados          25480 bytes  executavel ELF
  insone         25392 bytes  executavel ELF
  leiame.txt       420 bytes  
  medir          25248 bytes  executavel ELF
  oi             25240 bytes  executavel ELF
  primos         25688 bytes  executavel ELF
  soneca         25440 bytes  executavel ELF
  sis/                        diretorio
11 itens em /
silicon> ls /sis
  tempo             30 bytes  
  mem               46 bytes  
  processos         31 bytes  
3 itens em /sis
silicon> rodar cat /leiame.txt
Este arquivo mora no disco em RAM do SiliconOS.

Ele nao e um programa: e texto puro, empacotado no tar pelo build.rs junto
com os executaveis (docs/17-vfs-e-arquivos.md). Um programa o le com as
chamadas de sistema abrir, ler e fechar, por exemplo:

    rodar cat /leiame.txt

Experimente tambem os arquivos de /sis, que o kernel escreve na hora em
que alguem os abre:

    rodar cat /sis/tempo /sis/mem /sis/processos
'cat' terminou: codigo de saida 0
```

O `/sis`, lido por um programa:

```
silicon> rodar cat /sis/tempo /sis/mem /sis/processos
10670 ms (1067 tiques a 100 Hz)
quadros livres: 20201
heap em uso: 22592 bytes
PID NOME
  0 kernel
  1 ocioso
  3 cat
'cat' terminou: codigo de saida 0
```

O `cat` aparece na lista de processos que ele mesmo está lendo: o texto foi
gerado no `abrir`, enquanto ele rodava. (O pid 2 foi o `cat` anterior, que já
terminou.)

Os erros, e um caminho sem a barra:

```
silicon> rodar cat /nada /sis
cat: /nada: nao existe
cat: /sis: nao existe
'cat' terminou: codigo de saida 1
silicon> rodar cat sis/tempo
17740 ms (1774 tiques a 100 Hz)
'cat' terminou: codigo de saida 0
silicon> ls /oi
'/oi' nao e um diretorio (veja 'ls /')
```

### 8.2 O kernel se defende

```
silicon> rodar conferir
  abrir um arquivo que nao existe                -2 ok
  abrir um diretorio (/sis)                      -2 ok
  abrir com o caminho na memoria do kernel      -14 ok
  abrir com um caminho de 300 bytes             -36 ok
  ler do descritor 99                            -9 ok
  ler do descritor 1 (reservado)                 -9 ok
  fechar o descritor 0 (reservado)               -9 ok
  ler para dentro do proprio codigo             -14 ok
  ler para dentro do kernel                     -14 ok
  ler 16 bytes para a pilha (certo)              16 ok
  ...e as leituras recusadas nao avancaram        1 ok
  fechar o descritor                              0 ok
  fechar o mesmo descritor de novo               -9 ok
  quantos arquivos abrem ao mesmo tempo          16 ok
  o 17o e recusado                              -24 ok
conferir: 0 falhas (16 arquivos ficam abertos de proposito)
'conferir' terminou: codigo de saida 0
```

E a seção 5.3 mostrou o que acontece com a conferência de escrita
desligada: kernel panic na oitava prova.

### 8.3 `cargo test`

Seis testes novos (63 no total, todos passando):

```
siliconos::vfs::testes::abrir_e_ler_aos_pedacos ... [ok]
siliconos::vfs::testes::cat_le_do_disco_e_de_sis ... [ok]
siliconos::vfs::testes::descritores_reaproveitam_o_menor_livre ... [ok]
siliconos::vfs::testes::kernel_se_defende_e_libera_o_que_ficou_aberto ... [ok]
siliconos::vfs::testes::montagens_escolhidas_pelo_caminho ... [ok]
siliconos::vfs::testes::raiz_mostra_a_montagem_sis ... [ok]
Todos os 63 testes passaram.
```

A escolha da montagem, inclusive o caso `/sistema`:

```rust
    #[test_case]
    fn montagens_escolhidas_pelo_caminho() {
        let nome = |c| procurar(c).map(|(m, r)| (m.caminho, r));
        assert_eq!(nome("/sis/tempo"), Some(("/sis", "tempo")));
        assert_eq!(nome("/sis"), Some(("/sis", "")));
        assert_eq!(nome("/sistema"), Some(("/", "sistema")));
        assert_eq!(nome("/leiame.txt"), Some(("/", "leiame.txt")));
        assert_eq!(nome("leiame.txt"), Some(("/", "leiame.txt")));
        assert_eq!(nome("/"), Some(("/", "")));
    }
```

A tabela de descritores, sem processo nenhum:

```rust
    #[test_case]
    fn descritores_reaproveitam_o_menor_livre() {
        let mut d = Descritores::novo();
        let a = d.guardar(abrir("/leiame.txt").unwrap()).ok().unwrap();
        let b = d.guardar(abrir("/leiame.txt").unwrap()).ok().unwrap();
        assert_eq!((a, b), (3, 4));
        assert!(d.fechar(3));
        assert!(!d.fechar(3));                     // já fechado
        assert!(!d.fechar(0) && !d.fechar(99));    // nunca existiram
        assert_eq!(d.guardar(abrir("/leiame.txt").unwrap()).ok(), Some(3));
        for _ in 2..MAX_ABERTOS {
            assert!(d.guardar(abrir("/leiame.txt").unwrap()).is_ok());
        }
        assert!(d.guardar(abrir("/leiame.txt").unwrap()).is_err()); // cheia
        assert_eq!(d.quantos(), MAX_ABERTOS);
    }
```

E o que um programa deixa aberto volta para o heap:

```rust
    #[test_case]
    fn kernel_se_defende_e_libera_o_que_ficou_aberto() {
        // `conferir` termina com 16 arquivos abertos (gerados, no heap).
        let (livres, heap_antes) = (quadros::livres(), heap::estatisticas().em_uso);
        assert_eq!(rodar_com(&["conferir"]), Resultado::Saiu(0));
        assert_eq!(quadros::livres(), livres);
        assert_eq!(heap::estatisticas().em_uso, heap_antes);
    }
```

Os 16 arquivos que o `conferir` deixa abertos são de `/sis/tempo`, cada um
com um `Vec` de texto gerado no heap do kernel. Se a tabela não morresse
com o processo, `em_uso` terminaria maior que no começo.

### 8.4 Regressão

As 25 builds (a normal e as 24 features de teste e diagnóstico) se
comportam como antes.

---

## O que ainda falta

- **Ler aos pedaços, de verdade.** Todo arquivo aberto está inteiro na
  memória (seção 3.1). Um disco de verdade pediria que `SistemaDeArquivos`
  tivesse um `ler(arquivo, posicao, destino)` que buscasse só o pedaço pedido,
  e o `ArquivoAberto` guardaria uma referência ao arquivo, não os bytes.
- **Escrever.** Não há `escrever_em(descritor)` nem criação de arquivos: o
  tar é só de leitura (está dentro do executável do kernel). Um sistema de
  arquivos gravável em RAM (o `tmpfs` do Linux) seria o próximo passo natural.
- **Diretórios de verdade.** Cada sistema de arquivos aqui é uma lista plana;
  não há `/a/b/c`, nem diretório atual, nem `..`.
- **0, 1 e 2.** A saída padrão como descritor: `escrever(1, ...)` no lugar da
  syscall `escrever` do doc 10, e um teclado que se lê com `ler(0, ...)`. É o
  que liga este capítulo à ideia de terminal do doc 15.
- **SMAP.** Com a proteção SMAP ligada (desligada até aqui, doc 10), o kernel
  não poderia nem ler nem escrever páginas do programa sem pedir licença com
  as instruções `stac`/`clac`. É uma segunda trava, da CPU, para o erro que
  `faixa_gravavel_do_programa` evita pelo software.

---

## Mudanças nos arquivos

O que este capítulo acrescenta ao código do doc 16 (tag `doc-16`). O link de
cada arquivo leva ao estado dele no fim deste capítulo (tag `doc-17`); todas as
linhas que mudaram estão na [comparação entre as duas tags](https://github.com/moises-dev-git/siliconOS/compare/doc-16...doc-17).

| Arquivo | O que muda | Onde |
|---|---|---|
| [`src/vfs/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | **novo** (seção 3): `SistemaDeArquivos`, `Conteudo`, `MONTAGENS`, `procurar`, `abrir`, `listar`, `pai`, `ArquivoAberto`, `Descritores`; 6 testes | — |
| [`src/vfs/disco.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/disco.rs), [`src/vfs/sis.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/sis.rs) | **novos** (seção 4) | — |
| [`src/main.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/main.rs) | `mod vfs;` | lista de módulos |
| [`src/escalonador.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/escalonador.rs) | campo `arquivos` no `Processo` (nos 3 lugares que criam um); `com_arquivos` (seção 5.1) | `use`; `Processo`; `init`; `criar`; antes de `listar` |
| [`src/usuario/mod.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | `SYS_ABRIR`, `SYS_LER`, `SYS_FECHAR`, 4 erros novos, `MAX_CAMINHO`; `tratar_syscall`; `faixa_gravavel_do_programa`, `faixa_com_flags`; `abrir`, `ler`, `fechar` (seção 5) | constantes; `tratar_syscall`; `faixa_do_programa`; antes de `escrever` |
| [`src/shell.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/shell.rs) | `ls [diretorio]` pelo VFS (seção 7) | `COMANDOS`; `ls` |
| [`src/ramdisk.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/ramdisk.rs) | sai `tamanho_total` (o `ls` não usa mais); teste: 10 nomes, o `.txt` não é ELF | fim; testes |
| [`build.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/build.rs) | empacota também `programas/arquivos/` (seção 4.3) | `rerun-if-changed`; montagem do tar |
| [`programas/src/lib.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | `chamada3`; `SYS_ABRIR`/`LER`/`FECHAR`; `cru`, `nome_do_erro`, `Arquivo` e o `Drop` (seção 6.1) | constantes; `chamada`; antes de `Console` |
| [`programas/src/bin/cat.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/cat.rs), [`conferir.rs`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/conferir.rs), [`programas/arquivos/leiame.txt`](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/arquivos/leiame.txt) | **novos** (seções 6.2, 6.3, 4.3) | — |

### Os trechos que o texto não mostrou

O começo de `src/vfs/mod.rs`:

```rust
pub mod disco;
pub mod sis;

use alloc::string::String;
use alloc::vec::Vec;
```

O resto de `Descritores`:

```rust
    /// Quantos arquivos estão abertos.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn quantos(&self) -> usize {
        self.abertos.iter().filter(|a| a.is_some()).count()
    }
}
```

Os `use` de `disco.rs` e de `sis.rs`:

```rust
use alloc::string::String;
use alloc::vec::Vec;

use super::{Conteudo, SistemaDeArquivos};
use crate::ramdisk;
```
```rust
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::{Conteudo, SistemaDeArquivos};
use crate::escalonador;
use crate::interrupcoes::timer;
use crate::memoria::{heap, quadros};
```

No escalonador, o `use` e o campo novo nos três `Processo` criados (`init`
cria o 0 e o 1, `criar` os outros):

```rust
use crate::vfs::Descritores;
```
```rust
            arquivos: Descritores::novo(),
```

A `chamada3` da `sos` e a `chamada` de dois argumentos, que agora a usa:

```rust
#[inline(always)]
fn chamada(numero: u64, a: u64, b: u64) -> u64 {
    chamada3(numero, a, b, 0)
}

/// O mesmo, com um terceiro argumento em RDX (doc 17: `ler` precisa de três).
#[inline(always)]
fn chamada3(numero: u64, a: u64, b: u64, c: u64) -> u64 {
    let r: u64;
    unsafe {
        asm!("syscall", inlateout("rax") numero => r, in("rdi") a, in("rsi") b, in("rdx") c,
             lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    r
}
```

O resto de `Arquivo` e as constantes da `sos`:

```rust
    /// O número que o kernel deu a este arquivo.
    pub fn descritor(&self) -> u64 {
        self.descritor
    }
}
```
```rust
const SYS_ABRIR: u64 = 5;
const SYS_LER: u64 = 6;
const SYS_FECHAR: u64 = 7;
```

O começo do `conferir` (as provas de caminho e de descritor que o texto
não mostrou) e o fim:

```rust
fn main() -> u64 {
    let mut falhas = 0;
    let f = &mut falhas;

    // Caminhos ruins.
    let erro = |r: Result<Arquivo, i64>| r.err().unwrap_or(0);
    prova(f, "abrir um arquivo que nao existe", erro(Arquivo::abrir("/nao-existe")), -2);
    prova(f, "abrir um diretorio (/sis)", erro(Arquivo::abrir("/sis")), -2);
```
```rust
    // Descritores que não estão abertos.
    prova(f, "ler do descritor 99", cru::ler(99, longo.as_ptr() as u64, 1), -9);
    prova(f, "ler do descritor 1 (reservado)", cru::ler(1, longo.as_ptr() as u64, 1), -9);
    prova(f, "fechar o descritor 0 (reservado)", cru::fechar(0), -9);
```
```rust
    println!("conferir: {} falhas (16 arquivos ficam abertos de proposito)", falhas);
    falhas
}
```

Os testes que o texto não mostrou, e o ajudante deles:

```rust
    /// Roda um programa do disco com estes argumentos.
    fn rodar_com(argv: &[&str]) -> Resultado {
        let a = crate::ramdisk::abrir(argv[0]).unwrap();
        escalonador::esperar(usuario::iniciar_elf(a.nome, a.dados, argv, false).unwrap())
    }

    #[test_case]
    fn cat_le_do_disco_e_de_sis() {
        assert_eq!(rodar_com(&["cat", "/leiame.txt"]), Resultado::Saiu(0));
        assert_eq!(rodar_com(&["cat", "/sis/tempo", "/sis/processos"]), Resultado::Saiu(0));
        assert_eq!(rodar_com(&["cat", "/sis/mem", "/nao-existe"]), Resultado::Saiu(1));
        assert_eq!(rodar_com(&["cat"]), Resultado::Saiu(2));
    }
```
```rust
    #[test_case]
    fn abrir_e_ler_aos_pedacos() {
        let mut a = abrir("/leiame.txt").unwrap();
        let total = a.restante();
        assert!(total > 100);
        let mut pedaco = [0u8; 7];
        let mut lidos = 0;
        loop {
            let n = a.ler(&mut pedaco);
            if n == 0 { break; }
            lidos += n;
        }
        assert_eq!(lidos, total);
        assert_eq!(a.ler(&mut pedaco), 0); // continua no fim
        assert!(abrir("/nao-existe").is_none());
        assert!(abrir("/sis").is_none());           // diretório
        assert!(abrir("/sis/tempo").unwrap().restante() > 0);
    }

    #[test_case]
    fn raiz_mostra_a_montagem_sis() {
        let raiz = listar("/").unwrap();
        assert!(raiz.iter().any(|(n, _)| n == "sis/"));
        assert!(raiz.iter().any(|(n, _)| n == "leiame.txt"));
        let sis = listar("/sis").unwrap();
        assert!(sis.iter().any(|(n, _)| n == "tempo"));
        assert!(listar("/leiame.txt").is_none());
    }
```

Os `use` dos testes de `vfs/mod.rs`:

```rust
    use super::*;
    use crate::escalonador;
    use crate::memoria::{heap, quadros};
    use crate::usuario::{self, Resultado};
```

O teste do disco em `ramdisk.rs`:

```rust
        assert_eq!(nomes, ["args", "cat", "conferir", "dados", "insone", "leiame.txt", "medir", "oi", "primos", "soneca"]);
        // Todos são executáveis, menos o texto (doc 17).
        assert!(arquivos().all(|a| a.dados.starts_with(b"\x7fELF") == !a.nome.ends_with(".txt")));
```

No `build.rs`, a linha nova de `rerun-if-changed` e o laço que monta o tar
(que agora percorre `conteudo` em vez de ler cada binário):

```rust
    println!("cargo:rerun-if-changed=programas/arquivos");
```
```rust
    let mut tar = Vec::new();
    for (nome, dados) in &conteudo {
        acrescentar_ao_tar(&mut tar, nome, dados);
    }
```

E a linha do `ls` na tabela de comandos:

```rust
    Comando { nome: "ls", uso: "ls [diretorio]", descricao: "lista os arquivos (/ = disco em RAM, /sis = do kernel)", funcao: ls },
```

---

## Funções deste capítulo

Todas as funções que este capítulo acrescenta, em uma frase cada. As mais importantes foram explicadas no texto acima; esta tabela serve de mapa e de referência rápida. (Funções que já existiam e só mudaram aparecem em [Mudanças nos arquivos](#mudanças-nos-arquivos).)

| Função | Arquivo | O que faz |
|---|---|---|
| `SistemaDeArquivos::abrir` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | (trait) Abre um arquivo pelo nome dentro da montagem e devolve o conteúdo. |
| `SistemaDeArquivos::listar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | (trait) Os arquivos da montagem, com o tamanho. |
| `Conteudo::bytes` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Os bytes do conteúdo, fixo (do tar) ou gerado. |
| `procurar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Acha a montagem que cuida de um caminho e o resto do caminho depois dela. |
| `abrir` (VFS) | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Abre um arquivo pelo caminho completo, repassando ao sistema de arquivos da montagem. |
| `listar` (VFS) | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | O conteúdo de um diretório, com as montagens de dentro dele como subdiretórios. |
| `pai` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | O diretório que contém um caminho. |
| `ArquivoAberto::ler` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Copia os próximos bytes e avança a posição; 0 no fim. |
| `ArquivoAberto::restante` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Quantos bytes faltam ler. |
| `Descritores::novo` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Uma tabela de arquivos abertos vazia. |
| `Descritores::guardar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Guarda o arquivo no menor descritor livre (a partir de 3), ou o devolve se a tabela está cheia. |
| `Descritores::arquivo` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | O arquivo de um descritor, se aberto; aceita qualquer número sem cair. |
| `Descritores::fechar` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Fecha um descritor e libera o arquivo. |
| `Descritores::quantos` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Quantos arquivos estão abertos (usada nos testes). |
| `Disco::abrir`, `Disco::listar` | [vfs/disco.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/disco.rs) | O disco em RAM (tar) visto pelo VFS. |
| `Sis::abrir`, `Sis::listar` | [vfs/sis.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/sis.rs) | Os arquivos de /sis, gerados na hora pela função de cada um. |
| `tempo`, `mem`, `processos` | [vfs/sis.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/sis.rs) | Geram o texto de /sis/tempo, /sis/mem e /sis/processos. |
| `com_arquivos` | [escalonador.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/escalonador.rs) | Roda uma closure com a tabela de arquivos do processo atual, com o lock pego e as interrupções desligadas. |
| `faixa_gravavel_do_programa` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Como `faixa_do_programa`, mas exige páginas graváveis (destino de uma cópia do kernel). |
| `faixa_com_flags` | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Toda página da faixa é do programa e tem os bits exigidos? |
| `abrir` (syscall) | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Syscall 5: valida e copia o caminho, abre pelo VFS e devolve o descritor. |
| `ler` (syscall) | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Syscall 6: copia os próximos bytes do arquivo para um destino gravável do programa. |
| `fechar` (syscall) | [usuario/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/usuario/mod.rs) | Syscall 7: fecha o descritor. |
| `chamada3` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | Uma syscall com três argumentos (RDI, RSI, RDX). |
| `cru::abrir`, `cru::ler`, `cru::fechar` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | As syscalls de arquivo com números e ponteiros crus. |
| `nome_do_erro` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | O texto de um código de erro do kernel. |
| `Arquivo::abrir`, `Arquivo::ler`, `Arquivo::descritor` | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | Um arquivo aberto, com erros em `Result`. |
| `drop` (Arquivo) | [programas/src/lib.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/lib.rs) | Fecha o arquivo quando ele sai de escopo. |
| `main`, `mostrar` (cat) | [programas/src/bin/cat.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/cat.rs) | Mostra cada arquivo pedido, lendo aos pedaços de 64 bytes. |
| `main`, `prova` (conferir) | [programas/src/bin/conferir.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/programas/src/bin/conferir.rs) | Manda pedidos de arquivo errados e confere cada resposta do kernel. |

Testes automáticos (`#[test_case]`, rodam com `cargo test`, ver doc 09):

| Função | Arquivo | O que faz |
|---|---|---|
| `rodar_com` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Ajudante: roda um programa do disco com um argv e espera o resultado. |
| `montagens_escolhidas_pelo_caminho` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Teste: cada caminho cai na montagem certa, inclusive `/sistema` (que não é de `/sis`). |
| `abrir_e_ler_aos_pedacos` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Teste: ler de 7 em 7 bytes dá o arquivo inteiro e depois 0; diretório e inexistente não abrem. |
| `raiz_mostra_a_montagem_sis` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Teste: `listar("/")` inclui `sis/` e `leiame.txt`; arquivo não é diretório. |
| `descritores_reaproveitam_o_menor_livre` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Teste: 3 e 4, o 3 volta depois de fechado, números inválidos recusados, tabela cheia em 16. |
| `cat_le_do_disco_e_de_sis` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Teste: o `cat` sai com 0, 1 (arquivo que falta) ou 2 (sem argumentos). |
| `kernel_se_defende_e_libera_o_que_ficou_aberto` | [vfs/mod.rs](https://github.com/moises-dev-git/siliconOS/blob/doc-17/src/vfs/mod.rs) | Teste: o `conferir` passa em todas as provas, e os 16 arquivos que ele deixa abertos voltam para o heap. |

---

## Diferença em relação ao livro

O roteiro da seção 11.4 do livro cita um VFS entre o disco em RAM e o
carregador de programas, sem implementá-lo. O desenho aqui é nosso, com
os nomes e os números de erro do Unix: montagens numa tabela fixa, um
pseudo-sistema de arquivos (`/sis`) no lugar de um segundo disco, descritores
por processo a partir de 3, e o conteúdo inteiro de cada arquivo na memória.

## Resumo

- O **VFS** separa "o que o programa pede" (caminhos; abrir, ler, fechar) de
  "quem guarda o arquivo". Uma tabela de **montagens** diz qual sistema de
  arquivos cuida de cada parte da árvore.
- Um sistema de arquivos é qualquer tipo que implemente a trait
  `SistemaDeArquivos`. Há dois: o disco em RAM em `/` e o `/sis`, cujos
  arquivos o kernel gera na hora, como o `/proc` do Linux.
- Abrir um arquivo devolve um **descritor**, um índice na tabela de arquivos
  abertos **do processo**. Os números começam em 3 (0, 1 e 2 são reservados)
  e o menor livre é reaproveitado. A tabela morre com o processo.
- `ler` devolve **quantos** bytes copiou; 0 quer dizer fim do arquivo.
- Copiar dados **para** o programa exige páginas **graváveis**: com CR0.WP,
  um destino só de leitura faria o kernel levar um page fault. Testado: sem
  essa conferência, um ponteiro de um programa derrubou o kernel.
- A `sos` embrulha os descritores num `Arquivo` que fecha sozinho (`Drop`),
  e o `conferir` mostra que o kernel resiste a 15 pedidos ruins.

<details>
<summary><b>Para fixar (clique para ver as respostas)</b></summary>

1. **Por que o `cat` consegue mostrar `/leiame.txt` e `/sis/processos` sem
   nenhuma linha de código diferente para cada um?**
   Porque ele só fala com o VFS, por caminhos e pelas mesmas três syscalls.
   Quem sabe que um vem do tar e o outro é gerado na hora são os sistemas de
   arquivos atrás de cada montagem, e o VFS escolhe pelo caminho.

2. **Por que o kernel devolve um número (o descritor), e não um ponteiro
   para o `ArquivoAberto`?**
   O `ArquivoAberto` mora no heap do kernel, que o programa não pode ler nem
   escrever. Um número é só um índice que o kernel confere a cada uso numa
   tabela que é do próprio processo: um programa não alcança os arquivos de
   outro nem a memória do kernel chutando números.

3. **Na tabela de montagens, por que `/sis` vem antes de `/`? E por que
   `/sistema` não pode cair em `/sis`?**
   `procurar` fica com a primeira montagem que serve, e `/` serve para tudo:
   se viesse primeiro, `/sis/tempo` iria para o tar. Quanto a `/sistema`:
   tirar "sis" do começo deixa "tema", que não começa com `/`; só um resto
   vazio ou começando com `/` quer dizer que o caminho está dentro de `/sis`.

4. **Por que `abrir` chama `vfs::abrir` antes de `com_arquivos`, e não dentro
   dele?**
   Porque abrir `/sis/processos` chama `escalonador::listar`, que pega o lock
   da tabela de processos, e `com_arquivos` roda com esse mesmo lock pego. O
   `spin::Mutex` não é reentrante: o processo ficaria esperando a si mesmo,
   com as interrupções desligadas, e a máquina travaria.

5. **O programa passa como destino de `ler` o endereço da sua própria função
   `main`. Por que isso derrubaria o kernel, se o endereço é do programa?**
   A página do código é do programa, mas só de leitura (R-X). Com CR0.WP
   ligado, nem o kernel escreve numa página assim: a cópia causaria um page
   fault no anel 0, que é tratado como erro do kernel (panic), não do
   programa. Por isso `ler` exige `USUARIO | ESCRITA` em cada página.

6. **Um programa abre 5 arquivos e termina sem fechar nenhum. O que acontece
   com eles?**
   Nada vaza. A tabela de descritores é um campo do `Processo`; quando o
   processo é recolhido, ele é destruído, a tabela junto, e cada
   `ArquivoAberto` (com o `Vec` dos gerados) volta ao heap. O teste
   `kernel_se_defende_e_libera_o_que_ficou_aberto` confere o heap antes e
   depois.

7. **No `conferir`, para que serve o `mem::forget(a)` depois de fechar o
   descritor à mão?**
   Para o `Drop` do `Arquivo` não fechar o mesmo número de novo. Se outro
   arquivo tivesse sido aberto entre o fechamento manual e o fim do escopo,
   ele teria recebido o mesmo descritor (o menor livre), e o `Drop` atrasado
   fecharia o arquivo errado.

</details>
