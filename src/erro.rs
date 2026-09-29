//! Erros do programa, com mensagens em português que dizem o que houve e,
//! quando dá, o que fazer.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Erro {
    #[error("erro de E/S: {0}")]
    Io(#[from] std::io::Error),

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
