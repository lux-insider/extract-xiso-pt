//! A imagem aberta: onde fica a partição XDVDFS, o descritor de volume e a
//! leitura de bytes sempre dentro dos limites do volume.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::erro::{Resultado, imagem};

pub const SETOR: u64 = 2048;
/// O descritor de volume fica no setor 32 da partição.
const SETOR_DESCRITOR: u64 = 32;
pub const ASSINATURA: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";

/// Onde começa a partição XDVDFS no arquivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Layout {
    /// Imagem só com o sistema de arquivos (XISO), de qualquer console.
    Xiso,
    /// Disco de Xbox com a partição de vídeo antes.
    Xgd1,
    /// Disco de Xbox 360 (primeira geração).
    Xgd2,
    /// Disco de Xbox 360 (segunda geração).
    Xgd3,
}

impl Layout {
    /// Na ordem em que são procurados.
    pub const TODOS: [Layout; 4] = [Layout::Xiso, Layout::Xgd1, Layout::Xgd2, Layout::Xgd3];

    pub fn deslocamento(self) -> u64 {
        match self {
            Layout::Xiso => 0,
            Layout::Xgd1 => 0x1830_0000,
            Layout::Xgd2 => 0x0FD9_0000,
            Layout::Xgd3 => 0x0208_0000,
        }
    }

    pub fn rotulo(self) -> &'static str {
        match self {
            Layout::Xiso => "XISO",
            Layout::Xgd1 => "XGD1",
            Layout::Xgd2 => "XGD2",
            Layout::Xgd3 => "XGD3",
        }
    }
}

/// Uma imagem aberta, com o descritor de volume já conferido.
pub struct Imagem {
    arquivo: File,
    pub layout: Layout,
    /// Tamanho da partição: do início dela ao fim do arquivo.
    pub tamanho_volume: u64,
    pub setor_raiz: u32,
    pub tamanho_raiz: u32,
    /// Data de criação da imagem (FILETIME: centenas de ns desde 1601).
    pub criacao: u64,
}

impl Imagem {
    pub fn abrir(caminho: &Path) -> Resultado<Self> {
        let mut arquivo = File::open(caminho)?;
        let tamanho_arquivo = arquivo.metadata()?.len();
        if !arquivo.metadata()?.is_file() {
            return Err(imagem(format!("{} não é um arquivo", caminho.display())));
        }

        for layout in Layout::TODOS {
            let pos = layout.deslocamento() + SETOR_DESCRITOR * SETOR;
            if pos + SETOR > tamanho_arquivo {
                continue;
            }
            let mut descritor = vec![0u8; SETOR as usize];
            arquivo.seek(SeekFrom::Start(pos))?;
            arquivo.read_exact(&mut descritor)?;
            if &descritor[0..20] != ASSINATURA {
                continue;
            }
            let setor_raiz = u32::from_le_bytes(descritor[20..24].try_into().unwrap());
            let tamanho_raiz = u32::from_le_bytes(descritor[24..28].try_into().unwrap());
            let criacao = u64::from_le_bytes(descritor[28..36].try_into().unwrap());
            let img = Self {
                arquivo,
                layout,
                tamanho_volume: tamanho_arquivo - layout.deslocamento(),
                setor_raiz,
                tamanho_raiz,
                criacao,
            };
            img.conferir_trecho(
                setor_raiz,
                tamanho_raiz as u64,
                "a tabela do diretório raiz",
            )?;
            return Ok(img);
        }

        Err(imagem(
            "não parece uma imagem de Xbox ou Xbox 360: a assinatura MICROSOFT*XBOX*MEDIA não \
             está em nenhum dos lugares conhecidos (XISO, XGD1, XGD2, XGD3)",
        ))
    }

    /// Erra se `tamanho` bytes a partir de `setor` não cabem no volume —
    /// **antes** de alocar ou ler qualquer coisa.
    pub fn conferir_trecho(&self, setor: u32, tamanho: u64, o_que: &str) -> Resultado<()> {
        let fim = setor as u64 * SETOR + tamanho;
        if fim > self.tamanho_volume {
            return Err(imagem(format!(
                "{o_que} vai do setor {setor} até o byte {fim}, além do fim da imagem \
                 ({} bytes): ela está truncada ou corrompida",
                self.tamanho_volume
            )));
        }
        Ok(())
    }

    /// Lê `tamanho` bytes a partir de `setor` (já conferidos contra o volume).
    pub fn ler(&mut self, setor: u32, tamanho: usize, o_que: &str) -> Resultado<Vec<u8>> {
        self.conferir_trecho(setor, tamanho as u64, o_que)?;
        let mut buf = vec![0u8; tamanho];
        self.arquivo.seek(SeekFrom::Start(
            self.layout.deslocamento() + setor as u64 * SETOR,
        ))?;
        self.arquivo.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Posiciona o arquivo no início do conteúdo de `setor`, para leitura em
    /// fluxo (extração de arquivos grandes sem carregar tudo na memória).
    pub fn leitor_em(&mut self, setor: u32) -> Resultado<&mut File> {
        self.arquivo.seek(SeekFrom::Start(
            self.layout.deslocamento() + setor as u64 * SETOR,
        ))?;
        Ok(&mut self.arquivo)
    }
}
