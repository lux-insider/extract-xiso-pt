//! Extração da árvore para uma pasta.
//!
//! Garantias: nada é gravado fora do destino (os nomes já foram validados na
//! leitura da árvore); um arquivo pela metade nunca fica com o nome de
//! pronto (é gravado com um sufixo temporário e renomeado no fim); e, se a
//! extração falhar ou for cancelada, tudo o que ela criou é apagado.
//!
//! As pastas são criadas primeiro, e os arquivos extraídos na ordem em que
//! estão no disco (ver `extrair_tudo`).

use std::collections::HashSet;
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

    let mut criados = Criados::default();
    if !existia {
        fs::create_dir_all(destino).ctx(Operacao::CriarPasta, destino)?;
        criados.caminhos.push(destino.to_path_buf());
    }

    let resultado = extrair_tudo(img, entradas, destino, &mut criados, progresso);
    if resultado.is_err() {
        criados.desfazer();
    }
    resultado.map(|_| totais.bytes)
}

/// O que esta extração criou, para desfazer em caso de falha.
#[derive(Default)]
struct Criados {
    caminhos: Vec<PathBuf>,
    /// Arquivos que esta extração já deixou prontos (sem diferenciar
    /// maiúsculas, como no Windows): o temporário de outro arquivo nunca
    /// pode cair em cima de um deles.
    prontos: HashSet<String>,
}

fn chave(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

impl Criados {
    fn desfazer(&mut self) {
        // do mais novo para o mais antigo: arquivos antes das pastas deles;
        // `symlink_metadata` para nunca seguir um link trocado no meio
        for p in self.caminhos.iter().rev() {
            if fs::symlink_metadata(p).is_ok_and(|m| m.is_dir()) {
                fs::remove_dir(p).ok();
            } else {
                fs::remove_file(p).ok();
            }
        }
    }

    /// O temporário de `alvo`: `alvo.extract-xiso-pt.parcial`, a não ser que
    /// a imagem tenha um arquivo com esse nome e ele já tenha sido
    /// extraído; aí `alvo.1.extract-xiso-pt.parcial`, e assim por diante.
    fn temporario_para(&self, alvo: &Path) -> PathBuf {
        let mut p = temporario::caminho_de(alvo);
        let mut n = 1u32;
        while self.prontos.contains(&chave(&p)) {
            p = temporario::caminho_numerado(alvo, n);
            n += 1;
        }
        p
    }
}

/// Primeiro todas as pastas, na ordem da árvore; depois os arquivos, na
/// ordem em que o conteúdo deles está na imagem. Num disco rígido (ou num
/// DVD), a ordem alfabética faz a leitura saltar pela imagem a cada
/// arquivo; na ordem dos setores ela é sequencial. Os bytes e os nomes
/// gravados são os mesmos.
fn extrair_tudo(
    img: &mut Imagem,
    entradas: &[Entrada],
    destino: &Path,
    criados: &mut Criados,
    progresso: &Progresso,
) -> Resultado<()> {
    let mut arquivos = Vec::new();
    criar_pastas(entradas, destino, criados, &mut arquivos)?;
    // estável: arquivos no mesmo setor (vazios, ou que dividem o conteúdo)
    // ficam na ordem da árvore
    arquivos.sort_by_key(|(e, _)| e.setor);
    let mut buf = Vec::new();
    for (e, alvo) in &arquivos {
        if sistema::cancelado() {
            return Err(Erro::Cancelado);
        }
        extrair_arquivo(img, e, alvo, criados, progresso, &mut buf)?;
    }
    Ok(())
}

fn criar_pastas<'a>(
    entradas: &'a [Entrada],
    pasta: &Path,
    criados: &mut Criados,
    arquivos: &mut Vec<(&'a Entrada, PathBuf)>,
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
            criar_pastas(&e.filhos, &alvo, criados, arquivos)?;
        } else {
            arquivos.push((e, alvo));
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
    buf: &mut Vec<u8>,
) -> Resultado<()> {
    let parcial = criados.temporario_para(alvo);
    let mut saida = temporario::criar(&parcial).ctx(Operacao::Criar, &parcial)?;
    criados.caminhos.push(parcial.clone());

    if e.tamanho > 0 {
        let origem = img.caminho().to_path_buf();
        let leitor = img.leitor_em(e.setor)?;
        let mut restante = e.tamanho as usize;
        if buf.len() < BLOCO.min(restante) {
            buf.resize(BLOCO.min(restante), 0);
        }
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

    // Com --sobrescrever, o rename pode trocar um arquivo que já existia.
    // O arquivo novo está completo; se ele entrasse na lista do desfazer,
    // uma falha mais adiante deixaria o usuário sem a versão antiga (já
    // trocada) e sem a nova.
    let substitui = fs::symlink_metadata(alvo).is_ok();
    erro::renomear(&parcial, alvo)?;
    criados.prontos.insert(chave(alvo));
    if substitui {
        criados.caminhos.pop();
    } else if let Some(p) = criados.caminhos.last_mut() {
        // o que existe agora é o arquivo pronto, não o parcial
        *p = alvo.to_path_buf();
    }
    Ok(())
}
