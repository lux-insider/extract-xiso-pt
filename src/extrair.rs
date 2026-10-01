//! Extração da árvore para uma pasta.
//!
//! Garantias: nada é gravado fora do destino (os nomes já foram validados na
//! leitura da árvore); um arquivo pela metade nunca fica com o nome de
//! pronto (é gravado com um sufixo temporário e renomeado no fim); e, se a
//! extração falhar ou for cancelada, tudo o que ela criou é apagado.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::arvore::{self, Entrada};
use crate::erro::{self, Contexto, Erro, Operacao, Resultado};
use crate::imagem::Imagem;
use crate::progresso::Progresso;
use crate::sistema;
use crate::temporario;
use crate::terminal::fmt_bytes;

const BLOCO: usize = 1024 * 1024;

pub struct Opcoes {
    /// Não extrai a pasta `$SystemUpdate` da raiz (atualização do console).
    pub sem_atualizacao: bool,
    /// Aceita um destino que já existe e tem arquivos.
    pub sobrescrever: bool,
}

/// Pasta padrão: ao lado da imagem, com o nome dela sem a extensão.
pub fn destino_padrao(iso: &Path) -> PathBuf {
    let pasta = iso
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    pasta.join(iso.file_stem().unwrap_or_default())
}

fn e_atualizacao(e: &Entrada) -> bool {
    e.eh_diretorio() && e.nome.eq_ignore_ascii_case("$SystemUpdate")
}

/// O que vai ser extraído (tira a `$SystemUpdate` da raiz, se pedido).
pub fn selecionar(raiz: Vec<Entrada>, opcoes: &Opcoes) -> Vec<Entrada> {
    raiz.into_iter()
        .filter(|e| !(opcoes.sem_atualizacao && e_atualizacao(e)))
        .collect()
}

/// Extrai `entradas` em `destino`. Devolve os bytes gravados.
pub fn extrair(
    img: &mut Imagem,
    entradas: &[Entrada],
    destino: &Path,
    opcoes: &Opcoes,
    progresso: &Progresso,
) -> Resultado<u64> {
    let totais = arvore::totais(entradas);

    // Destino: se já existe com conteúdo, só com --sobrescrever.
    let existia = destino.exists();
    if existia {
        if !destino.is_dir() {
            return Err(Erro::Destino(format!(
                "{} existe e não é uma pasta",
                destino.display()
            )));
        }
        let vazio = fs::read_dir(destino)
            .ctx(Operacao::ListarPasta, destino)?
            .next()
            .is_none();
        if !vazio && !opcoes.sobrescrever {
            return Err(Erro::Destino(format!(
                "a pasta {} já existe e não está vazia; escolha outra ou use --sobrescrever",
                destino.display()
            )));
        }
    }
    if let Some(livre) = sistema::espaco_livre(destino)
        && livre < totais.bytes
    {
        return Err(Erro::Destino(format!(
            "não cabe: a extração precisa de {} e {} tem {} livres",
            fmt_bytes(totais.bytes),
            destino.display(),
            fmt_bytes(livre)
        )));
    }

    let mut criados = Criados {
        caminhos: Vec::new(),
    };
    if !existia {
        fs::create_dir_all(destino).ctx(Operacao::CriarPasta, destino)?;
        criados.caminhos.push(destino.to_path_buf());
    }

    let resultado = extrair_nivel(img, entradas, destino, &mut criados, progresso);
    if resultado.is_err() {
        criados.desfazer();
    }
    resultado.map(|_| totais.bytes)
}

/// O que esta extração criou, para desfazer em caso de falha.
struct Criados {
    caminhos: Vec<PathBuf>,
}

impl Criados {
    fn desfazer(&mut self) {
        // do mais novo para o mais antigo: arquivos antes das pastas deles
        for p in self.caminhos.iter().rev() {
            if p.is_dir() {
                fs::remove_dir(p).ok();
            } else {
                fs::remove_file(p).ok();
            }
        }
    }
}

fn extrair_nivel(
    img: &mut Imagem,
    entradas: &[Entrada],
    pasta: &Path,
    criados: &mut Criados,
    progresso: &Progresso,
) -> Resultado<()> {
    for e in entradas {
        if sistema::cancelado() {
            return Err(Erro::Cancelado);
        }
        let alvo = pasta.join(&e.nome);
        if e.eh_diretorio() {
            // `symlink_metadata` não segue links: com --sobrescrever, uma
            // pasta do destino que é link (ou junção, no Windows) levaria a
            // extração para fora dele
            let existente = fs::symlink_metadata(&alvo).ok();
            if existente
                .as_ref()
                .is_some_and(|m| m.file_type().is_symlink())
            {
                return Err(Erro::Destino(format!(
                    "{} é um link simbólico; a extração não grava através de links, \
                     para não sair da pasta de destino",
                    alvo.display()
                )));
            }
            if !existente.is_some_and(|m| m.is_dir()) {
                fs::create_dir(&alvo).ctx(Operacao::CriarPasta, &alvo)?;
                criados.caminhos.push(alvo.clone());
            }
            extrair_nivel(img, &e.filhos, &alvo, criados, progresso)?;
        } else {
            extrair_arquivo(img, e, &alvo, criados, progresso)?;
        }
    }
    Ok(())
}

fn extrair_arquivo(
    img: &mut Imagem,
    e: &Entrada,
    alvo: &Path,
    criados: &mut Criados,
    progresso: &Progresso,
) -> Resultado<()> {
    let parcial = temporario::caminho_de(alvo);
    let mut saida = temporario::criar(&parcial).ctx(Operacao::Criar, &parcial)?;
    criados.caminhos.push(parcial.clone());

    if e.tamanho > 0 {
        let origem = img.caminho().to_path_buf();
        let leitor = img.leitor_em(e.setor)?;
        let mut restante = e.tamanho as usize;
        let mut buf = vec![0u8; BLOCO.min(restante)];
        while restante > 0 {
            if sistema::cancelado() {
                return Err(Erro::Cancelado);
            }
            let n = restante.min(buf.len());
            leitor
                .read_exact(&mut buf[..n])
                .ctx(Operacao::Ler, &origem)?;
            saida.write_all(&buf[..n]).ctx(Operacao::Gravar, &parcial)?;
            restante -= n;
            progresso.avancar(n as u64, &e.nome);
        }
    }
    drop(saida);

    erro::renomear(&parcial, alvo)?;
    // o que existe agora é o arquivo pronto, não o parcial
    if let Some(p) = criados.caminhos.last_mut() {
        *p = alvo.to_path_buf();
    }
    Ok(())
}
