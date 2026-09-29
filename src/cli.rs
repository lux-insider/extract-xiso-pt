use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Lista, extrai e confere imagens de disco de Xbox e Xbox 360 (XDVDFS).
#[derive(Debug, Parser)]
#[command(name = "extract-xiso-pt", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub comando: Comando,
}

#[derive(Debug, Subcommand)]
pub enum Comando {
    /// Mostra o layout, o console e o tamanho do conteúdo de uma imagem.
    Info {
        /// Imagem de disco (.iso / .xiso).
        imagem: PathBuf,
        /// Imprime um objeto JSON em vez do texto.
        #[arg(long)]
        json: bool,
    },

    /// Lista os arquivos de dentro da imagem, sem extrair nada.
    Listar {
        imagem: PathBuf,
        /// Imprime a árvore inteira em JSON.
        #[arg(long)]
        json: bool,
    },

    /// Extrai o conteúdo da imagem para uma pasta.
    Extrair {
        imagem: PathBuf,
        /// Pasta de destino. Sem ela, uma pasta com o nome da imagem, ao lado
        /// da imagem.
        #[arg(short, long)]
        destino: Option<PathBuf>,
        /// Não extrai a pasta $SystemUpdate (atualização do console).
        #[arg(short = 's', long)]
        sem_atualizacao: bool,
        /// Aceita um destino que já existe e tem arquivos.
        #[arg(long)]
        sobrescrever: bool,
        /// Relata o progresso como uma linha JSON por evento em stdout.
        #[arg(long)]
        progresso_json: bool,
    },

    /// Cria uma imagem XISO a partir de uma pasta (jogo de Xbox ou Xbox 360).
    Criar {
        /// Pasta com o conteúdo do disco (default.xbe ou default.xex na raiz).
        pasta: PathBuf,
        /// Imagem a criar. Sem ela, <pasta>.iso ao lado da pasta.
        #[arg(short, long)]
        saida: Option<PathBuf>,
        /// Não inclui a pasta $SystemUpdate (atualização do console).
        #[arg(short = 'u', long)]
        sem_atualizacao: bool,
        /// Substitui a imagem de saída se ela já existir.
        #[arg(long)]
        sobrescrever: bool,
        /// Relata o progresso como uma linha JSON por evento em stdout.
        #[arg(long)]
        progresso_json: bool,
    },

    /// Regrava uma imagem como XISO enxuta: tira a partição de vídeo e o
    /// espaço vazio de discos XGD1/XGD2/XGD3 e reorganiza as tabelas.
    Reescrever {
        imagem: PathBuf,
        /// Imagem a criar. Sem ela, <nome>.xiso.iso ao lado da original.
        #[arg(short, long, conflicts_with = "substituir")]
        saida: Option<PathBuf>,
        /// Não inclui a pasta $SystemUpdate (atualização do console).
        #[arg(short = 'u', long)]
        sem_atualizacao: bool,
        /// Substitui a imagem de saída se ela já existir.
        #[arg(long)]
        sobrescrever: bool,
        /// Troca a imagem original pela reescrita (só depois de ela estar
        /// pronta e conferida).
        #[arg(long)]
        substituir: bool,
        /// Relata o progresso como uma linha JSON por evento em stdout.
        #[arg(long)]
        progresso_json: bool,
    },
}
