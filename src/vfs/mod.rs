//! VFS (sistema de arquivos virtual): um jeito só de abrir e ler arquivos,
//! não importa quem os guarda.
//!
//! Explicação completa em docs/17-vfs-e-arquivos.md.
//!
//! Quem usa (o shell, as syscalls) só conhece caminhos, como "/leiame.txt"
//! ou "/sis/tempo". O VFS procura na tabela de MONTAGENS qual sistema de
//! arquivos cuida daquele caminho e repassa o resto do caminho para ele:
//!
//!   "/sis/tempo"   ──►  montagem "/sis" ──► Sis.abrir("tempo")    (gerado na hora)
//!   "/leiame.txt"  ──►  montagem "/"    ──► Disco.abrir("leiame.txt") (do tar)
//!
//! Cada processo tem a sua tabela de arquivos abertos (`Descritores`): o
//! número que `abrir` devolve (o DESCRITOR) é um índice nessa tabela.

pub mod disco;
pub mod sis;

use alloc::string::String;
use alloc::vec::Vec;

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

/// Qual montagem cuida de `caminho`, e o resto do caminho depois dela.
/// "/sis/tempo" → (/sis, "tempo"); "/leiame.txt" → (/, "leiame.txt").
/// Um caminho sem barra no começo é tratado como se tivesse ("leiame.txt"
/// = "/leiame.txt"): ainda não há diretório atual.
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

/// Abre um arquivo pelo caminho completo.
pub fn abrir(caminho: &str) -> Option<ArquivoAberto> {
    let (montagem, nome) = procurar(caminho)?;
    if nome.is_empty() {
        return None; // é um diretório ("/sis"), não um arquivo
    }
    montagem.sistema.abrir(nome).map(|conteudo| ArquivoAberto { conteudo, posicao: 0 })
}

/// O conteúdo de um diretório: (nome, tamanho), com as montagens que moram
/// dentro dele aparecendo como subdiretórios (nome terminado em '/').
/// `None` se o diretório não existe.
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

// ---------------------------------------------------------------------------
// Arquivos abertos e descritores
// ---------------------------------------------------------------------------

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

    /// Quantos arquivos estão abertos.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn quantos(&self) -> usize {
        self.abertos.iter().filter(|a| a.is_some()).count()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::escalonador;
    use crate::memoria::{heap, quadros};
    use crate::usuario::{self, Resultado};

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

    #[test_case]
    fn kernel_se_defende_e_libera_o_que_ficou_aberto() {
        // `conferir` termina com 16 arquivos abertos (gerados, no heap).
        let (livres, heap_antes) = (quadros::livres(), heap::estatisticas().em_uso);
        assert_eq!(rodar_com(&["conferir"]), Resultado::Saiu(0));
        assert_eq!(quadros::livres(), livres);
        assert_eq!(heap::estatisticas().em_uso, heap_antes);
    }

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
}
