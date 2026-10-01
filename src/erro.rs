//! Erros do programa, com mensagens em português que dizem o que houve e,
//! quando dá, o que fazer.
//!
//! Uma falha de E/S sempre diz **qual arquivo** e **qual operação**: não há
//! conversão automática de `io::Error` para `Erro`, então cada chamada tem
//! que dar o contexto (`.ctx(Operacao::Ler, caminho)`).

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

/// O que se tentava fazer com o arquivo quando a E/S falhou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operacao {
    Abrir,
    Ler,
    Gravar,
    Criar,
    CriarPasta,
    ListarPasta,
    Consultar,
    /// Esvaziar os buffers no disco (`sync_all`).
    Sincronizar,
}

impl fmt::Display for Operacao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Operacao::Abrir => "abrir",
            Operacao::Ler => "ler",
            Operacao::Gravar => "gravar",
            Operacao::Criar => "criar",
            Operacao::CriarPasta => "criar a pasta",
            Operacao::ListarPasta => "listar a pasta",
            Operacao::Consultar => "consultar",
            Operacao::Sincronizar => "terminar de gravar no disco",
        })
    }
}

#[derive(Debug, Error)]
pub enum Erro {
    /// Uma operação de E/S que falhou num arquivo ou pasta conhecida.
    #[error("não foi possível {operacao} {}: {}", caminho.display(), Causa(fonte))]
    Arquivo {
        operacao: Operacao,
        caminho: PathBuf,
        #[source]
        fonte: io::Error,
    },

    /// Um `rename` que falhou (dar o nome final a um arquivo pronto).
    #[error(
        "não foi possível renomear {} para {}: {}",
        de.display(),
        para.display(),
        Causa(fonte)
    )]
    Renomear {
        de: PathBuf,
        para: PathBuf,
        #[source]
        fonte: io::Error,
    },

    /// A imagem não é de Xbox, está truncada ou tem a estrutura corrompida.
    #[error("{0}")]
    Imagem(String),

    /// Um nome dentro da imagem que não pode virar arquivo com segurança
    /// (sairia da pasta de destino ou não existe no Windows).
    #[error("nome inseguro na imagem: {0}")]
    NomeInseguro(String),

    /// Problema com a pasta de destino (existe e não está vazia, sem
    /// permissão, sem espaço...).
    #[error("{0}")]
    Destino(String),

    /// O usuário apertou Ctrl+C (ou outro programa mandou SIGTERM).
    #[error("operação cancelada")]
    Cancelado,
}

pub type Resultado<T> = Result<T, Erro>;

pub fn imagem(msg: impl Into<String>) -> Erro {
    Erro::Imagem(msg.into())
}

/// Dá a uma falha de E/S o arquivo e a operação.
pub trait Contexto<T> {
    fn ctx(self, operacao: Operacao, caminho: &Path) -> Resultado<T>;
}

impl<T> Contexto<T> for io::Result<T> {
    #[inline]
    fn ctx(self, operacao: Operacao, caminho: &Path) -> Resultado<T> {
        match self {
            Ok(v) => Ok(v),
            Err(fonte) => Err(erro_arquivo(operacao, caminho, fonte)),
        }
    }
}

/// Fora da parte genérica: uma cópia só, em vez de uma por tipo de `T`.
#[inline(never)]
fn erro_arquivo(operacao: Operacao, caminho: &Path, fonte: io::Error) -> Erro {
    Erro::Arquivo {
        operacao,
        caminho: caminho.to_path_buf(),
        fonte,
    }
}

/// `rename` com o erro dizendo os dois caminhos.
pub fn renomear(de: &Path, para: &Path) -> Resultado<()> {
    std::fs::rename(de, para).map_err(|fonte| Erro::Renomear {
        de: de.to_path_buf(),
        para: para.to_path_buf(),
        fonte,
    })
}

/// A causa de uma falha de E/S em português, para os casos comuns, com o
/// código do sistema (útil para quem for ajudar); nos outros, a mensagem
/// do próprio sistema.
struct Causa<'a>(&'a io::Error);

impl fmt::Display for Causa<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use io::ErrorKind as K;
        let e = self.0;
        let texto = match e.kind() {
            K::NotFound => "não existe",
            K::PermissionDenied => "sem permissão",
            K::AlreadyExists => "já existe",
            K::StorageFull => "não há espaço no disco",
            K::QuotaExceeded => "a cota de disco do usuário acabou",
            K::FileTooLarge => {
                "arquivo grande demais para este sistema de arquivos (FAT32 aceita até 4 GiB)"
            }
            K::ReadOnlyFilesystem => "o disco é só de leitura",
            K::IsADirectory => "é uma pasta",
            K::NotADirectory => "parte do caminho não é uma pasta",
            K::DirectoryNotEmpty => "a pasta não está vazia",
            K::CrossesDevices => "a origem e o destino estão em discos diferentes",
            K::ResourceBusy | K::ExecutableFileBusy => "o arquivo está em uso por outro programa",
            K::InvalidFilename => "nome de arquivo inválido para este sistema",
            K::UnexpectedEof => "o arquivo terminou antes do esperado (truncado?)",
            K::OutOfMemory => "falta de memória",
            K::Interrupted => "a operação foi interrompida",
            _ => return write!(f, "{e}"),
        };
        match e.raw_os_error() {
            Some(c) => write!(f, "{texto} (erro {c} do sistema)"),
            None => f.write_str(texto),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mensagem_diz_arquivo_operacao_e_causa() {
        let e: Resultado<()> = Err(io::Error::from_raw_os_error(2))
            .ctx(Operacao::Abrir, Path::new("/jogos/Halo 3.iso"));
        let m = e.unwrap_err().to_string();
        assert!(
            m.starts_with("não foi possível abrir /jogos/Halo 3.iso: não existe"),
            "{m}"
        );

        let e = renomear(Path::new("/nao/existe/a"), Path::new("/nao/existe/b")).unwrap_err();
        let m = e.to_string();
        assert!(
            m.contains("renomear /nao/existe/a para /nao/existe/b"),
            "{m}"
        );

        // causa sem tradução: a mensagem do sistema
        let e: Resultado<()> =
            Err(io::Error::other("algo estranho")).ctx(Operacao::Ler, Path::new("x"));
        assert_eq!(
            e.unwrap_err().to_string(),
            "não foi possível ler x: algo estranho"
        );
    }
}
