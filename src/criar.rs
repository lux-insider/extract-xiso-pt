//! Escrita de uma imagem XISO nova, a partir de uma pasta (`criar`) ou do
//! conteúdo de outra imagem (`reescrever`: tira a partição de vídeo e o
//! espaço vazio de um disco XGD1/XGD2/XGD3, ou reorganiza uma XISO).
//!
//! Disposição gravada:
//!   setores 0..32  zeros (reservado)
//!   setor 32       descritor de volume
//!   setor 33..     tabelas de diretório, da raiz para dentro
//!   depois         o conteúdo dos arquivos, cada um começando num setor
//!   fim            zeros até um múltiplo de 64 KiB
//!
//! Cada tabela é uma árvore binária balanceada, ordenada pelo nome sem
//! diferenciar maiúsculas (a mesma comparação que o console usa para achar
//! um arquivo), com nós que nunca atravessam o fim de um setor.
//!
//! A imagem é gravada com um sufixo temporário e só ganha o nome final depois
//! de pronta e relida; se algo falhar ou for cancelado, o temporário é
//! apagado e nada mais muda.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::arvore::{self, Entrada};
use crate::erro::{Erro, Resultado, imagem};
use crate::imagem::{ASSINATURA, Imagem, SETOR};
use crate::progresso::Progresso;
use crate::sistema;
use crate::terminal::fmt_bytes;

const S: usize = SETOR as usize;
const SUFIXO_PARCIAL: &str = ".extract-xiso-pt.parcial";
/// A imagem termina num múltiplo disto, como as XISO feitas por outras
/// ferramentas.
const ALINHAMENTO_FIM: u64 = 64 * 1024;
/// Ponteiros de nó têm 16 bits em palavras de 4 bytes.
const MAX_TABELA: usize = 0xFFFF * 4;
const BLOCO: usize = 1024 * 1024;
const PROFUNDIDADE_MAXIMA: usize = arvore::PROFUNDIDADE_MAXIMA;

/// De onde vêm os bytes de um arquivo.
enum Origem {
    Pasta(PathBuf),
    Imagem { setor: u32 },
}

/// Um item a gravar.
struct Item {
    nome: Vec<u8>,
    diretorio: bool,
    tamanho: u32,
    origem: Origem,
    filhos: Vec<Item>,
    /// Preenchido na disposição: primeiro setor (tabela ou conteúdo).
    setor: u32,
    /// Bytes a trocar na cópia gravada (posição no arquivo, bytes novos):
    /// só o remendo de mídia do default.xbe, quando pedido.
    remendo: Option<(u32, [u8; 4])>,
}

// ---------------------------------------------------------------------------
// Montagem da árvore a partir das fontes
// ---------------------------------------------------------------------------

/// Lê a pasta inteira, conferindo nomes, tamanhos e laços de links.
fn de_pasta(raiz: &Path) -> Resultado<Vec<Item>> {
    let mut ancestrais = HashSet::new();
    let mut contagem = 0usize;
    de_pasta_rec(raiz, "", 0, &mut ancestrais, &mut contagem)
}

fn de_pasta_rec(
    pasta: &Path,
    caminho: &str,
    profundidade: usize,
    ancestrais: &mut HashSet<PathBuf>,
    contagem: &mut usize,
) -> Resultado<Vec<Item>> {
    if profundidade > PROFUNDIDADE_MAXIMA {
        return Err(Erro::Destino(format!(
            "{caminho}: mais de {PROFUNDIDADE_MAXIMA} níveis de pastas"
        )));
    }
    // um link simbólico para um ancestral faria a leitura girar para sempre
    let real = fs::canonicalize(pasta)?;
    if !ancestrais.insert(real.clone()) {
        return Err(Erro::Destino(format!(
            "{caminho}: é um link para uma pasta que já a contém"
        )));
    }

    let mut itens = Vec::new();
    for e in fs::read_dir(pasta)? {
        let e = e?;
        let nome_os = e.file_name();
        let Some(nome) = nome_os.to_str() else {
            return Err(Erro::Destino(format!(
                "{caminho}/{}: o nome não é texto válido (UTF-8)",
                nome_os.to_string_lossy()
            )));
        };
        let cam = if caminho.is_empty() {
            nome.to_string()
        } else {
            format!("{caminho}/{nome}")
        };
        validar_nome_novo(nome, &cam)?;
        *contagem += 1;
        if *contagem > 1_000_000 {
            return Err(Erro::Destino(
                "mais de um milhão de arquivos na pasta".into(),
            ));
        }
        let p = e.path();
        let meta = fs::metadata(&p)?; // segue links
        if meta.is_dir() {
            let filhos = de_pasta_rec(&p, &cam, profundidade + 1, ancestrais, contagem)?;
            itens.push(Item {
                nome: nome.as_bytes().to_vec(),
                diretorio: true,
                tamanho: 0,
                origem: Origem::Pasta(p),
                filhos,
                setor: 0,
                remendo: None,
            });
        } else if meta.is_file() {
            let tamanho = u32::try_from(meta.len()).map_err(|_| {
                Erro::Destino(format!(
                    "{cam} tem {} — o formato só guarda arquivos de até 4 GiB",
                    fmt_bytes(meta.len())
                ))
            })?;
            itens.push(Item {
                nome: nome.as_bytes().to_vec(),
                diretorio: false,
                tamanho,
                origem: Origem::Pasta(p),
                filhos: Vec::new(),
                setor: 0,
                remendo: None,
            });
        } else {
            return Err(Erro::Destino(format!(
                "{cam} não é arquivo nem pasta (dispositivo, fifo ou socket)"
            )));
        }
    }
    ancestrais.remove(&real);
    ordenar(&mut itens, caminho)?;
    Ok(itens)
}

/// A árvore de outra imagem, já validada pela leitura.
fn de_imagem(entradas: &[Entrada]) -> Vec<Item> {
    entradas
        .iter()
        .map(|e| Item {
            // mesmos bytes do nome: Latin-1 de volta para byte, UTF-8 como UTF-8
            nome: bytes_do_nome(&e.nome),
            diretorio: e.eh_diretorio(),
            tamanho: if e.eh_diretorio() { 0 } else { e.tamanho },
            origem: Origem::Imagem { setor: e.setor },
            filhos: de_imagem(&e.filhos),
            setor: 0,
            remendo: None,
        })
        .collect()
}

/// Volta um nome lido da imagem aos bytes originais: ele veio de UTF-8 válido
/// ou de Latin-1 (cada byte um caractere até U+00FF).
fn bytes_do_nome(nome: &str) -> Vec<u8> {
    if nome.is_ascii() {
        return nome.as_bytes().to_vec();
    }
    let latin1: Option<Vec<u8>> = nome.chars().map(|c| u8::try_from(c as u32).ok()).collect();
    match latin1 {
        // se os bytes Latin-1 formam UTF-8 válido, a leitura teria dado o
        // texto UTF-8, não este — então o original era mesmo UTF-8
        Some(b) if std::str::from_utf8(&b).is_err() => b,
        _ => nome.as_bytes().to_vec(),
    }
}

/// Um nome que vai para a imagem nova: as mesmas regras da leitura (a
/// imagem gerada tem que poder ser extraída em qualquer sistema) e cabe no
/// campo de 1 byte de tamanho.
fn validar_nome_novo(nome: &str, caminho: &str) -> Resultado<()> {
    arvore::validar_nome(nome, caminho).map_err(|e| match e {
        Erro::NomeInseguro(m) => Erro::Destino(format!("nome que não pode ir para a imagem: {m}")),
        outro => outro,
    })?;
    if nome.len() > 255 {
        return Err(Erro::Destino(format!(
            "{caminho}: o nome tem {} bytes e o limite é 255",
            nome.len()
        )));
    }
    Ok(())
}

/// A comparação do console: byte a byte, com a-z como A-Z.
fn comparar(a: &[u8], b: &[u8]) -> Ordering {
    a.iter()
        .map(u8::to_ascii_uppercase)
        .cmp(b.iter().map(u8::to_ascii_uppercase))
}

fn ordenar(itens: &mut [Item], caminho: &str) -> Resultado<()> {
    itens.sort_by(|a, b| comparar(&a.nome, &b.nome));
    for par in itens.windows(2) {
        if comparar(&par[0].nome, &par[1].nome) == Ordering::Equal {
            let n = String::from_utf8_lossy(&par[1].nome);
            let cam = if caminho.is_empty() {
                n.to_string()
            } else {
                format!("{caminho}/{n}")
            };
            return Err(Erro::Destino(format!(
                "{cam} existe duas vezes com maiúsculas diferentes; no console seriam o mesmo arquivo"
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tabelas de diretório
// ---------------------------------------------------------------------------

fn tamanho_no(nome: &[u8]) -> usize {
    (14 + nome.len()).div_ceil(4) * 4
}

/// Onde fica cada nó na tabela e o tamanho dela, arredondado para setores.
///
/// A raiz da árvore tem que estar na posição 0, então os nós são gravados em
/// pré-ordem da árvore balanceada (o do meio primeiro); um nó que não cabe no
/// resto do setor pula para o próximo. `posicoes[i]` é a posição de `itens[i]`.
fn disposicao(itens: &[Item]) -> (Vec<usize>, usize) {
    let mut ordem = Vec::with_capacity(itens.len());
    preordem(0, itens.len(), &mut ordem);
    let mut posicoes = vec![0usize; itens.len()];
    let mut p = 0usize;
    for i in ordem {
        let n = tamanho_no(&itens[i].nome);
        if p % S + n > S {
            p = p.div_ceil(S) * S;
        }
        posicoes[i] = p;
        p += n;
    }
    (posicoes, p.div_ceil(S).max(1) * S)
}

/// Índices de itens[ini..fim] em pré-ordem da árvore balanceada.
fn preordem(ini: usize, fim: usize, saida: &mut Vec<usize>) {
    if ini >= fim {
        return;
    }
    let meio = ini + (fim - ini) / 2;
    saida.push(meio);
    preordem(ini, meio, saida);
    preordem(meio + 1, fim, saida);
}

/// Tamanho em bytes da tabela de um diretório.
fn tamanho_tabela(itens: &[Item]) -> usize {
    disposicao(itens).1
}

/// Monta a tabela de um diretório (itens já ordenados). Um diretório vazio
/// vira um setor de preenchimento.
fn montar_tabela(itens: &[Item], caminho: &str) -> Resultado<Vec<u8>> {
    let (pos, tamanho) = disposicao(itens);
    if tamanho > MAX_TABELA {
        return Err(Erro::Destino(format!(
            "a pasta {} tem entradas demais para uma tabela de diretório ({} de no máximo {})",
            if caminho.is_empty() { "raiz" } else { caminho },
            fmt_bytes(tamanho as u64),
            fmt_bytes(MAX_TABELA as u64)
        )));
    }
    let mut t = vec![0xFFu8; tamanho];
    gravar_nos(0, itens.len(), itens, &pos, &mut t);
    Ok(t)
}

/// Grava a subárvore itens[ini..fim]; devolve a posição da raiz dela.
fn gravar_nos(
    ini: usize,
    fim: usize,
    itens: &[Item],
    pos: &[usize],
    t: &mut [u8],
) -> Option<usize> {
    if ini >= fim {
        return None;
    }
    let meio = ini + (fim - ini) / 2;
    let esq = gravar_nos(ini, meio, itens, pos, t).map_or(0, |p| (p / 4) as u16);
    let dir = gravar_nos(meio + 1, fim, itens, pos, t).map_or(0, |p| (p / 4) as u16);
    let it = &itens[meio];
    let p = pos[meio];
    let atributos: u8 = if it.diretorio {
        arvore::ATTR_DIRETORIO
    } else {
        0x20
    };
    let tamanho = if it.diretorio {
        tamanho_tabela(&it.filhos) as u32
    } else {
        it.tamanho
    };
    t[p..p + 2].copy_from_slice(&esq.to_le_bytes());
    t[p + 2..p + 4].copy_from_slice(&dir.to_le_bytes());
    t[p + 4..p + 8].copy_from_slice(&it.setor.to_le_bytes());
    t[p + 8..p + 12].copy_from_slice(&tamanho.to_le_bytes());
    t[p + 12] = atributos;
    t[p + 13] = it.nome.len() as u8;
    t[p + 14..p + 14 + it.nome.len()].copy_from_slice(&it.nome);
    Some(p)
}

// ---------------------------------------------------------------------------
// Disposição e gravação
// ---------------------------------------------------------------------------

/// Distribui setores: tabelas primeiro (em largura), depois os arquivos na
/// ordem em que aparecem. Devolve (setor da raiz, tamanho da raiz, setores
/// totais usados).
fn dispor(raiz: &mut [Item]) -> Resultado<(u32, u32, u64)> {
    let tamanho_raiz = tamanho_tabela(raiz) as u32;
    let mut prox: u64 = 33;
    let setor_raiz = prox as u32;
    prox += (tamanho_raiz as u64).div_ceil(SETOR);
    dispor_tabelas(raiz, &mut prox)?;
    dispor_arquivos(raiz, &mut prox)?;
    Ok((setor_raiz, tamanho_raiz, prox))
}

fn setor_u32(s: u64) -> Resultado<u32> {
    u32::try_from(s)
        .map_err(|_| Erro::Destino("o conteúdo passa do limite do formato (8 TiB)".into()))
}

fn dispor_tabelas(itens: &mut [Item], prox: &mut u64) -> Resultado<()> {
    // em largura: as tabelas deste nível, depois as dos filhos
    for it in itens.iter_mut().filter(|i| i.diretorio) {
        it.setor = setor_u32(*prox)?;
        *prox += (tamanho_tabela(&it.filhos) as u64).div_ceil(SETOR);
    }
    for it in itens.iter_mut().filter(|i| i.diretorio) {
        dispor_tabelas(&mut it.filhos, prox)?;
    }
    Ok(())
}

fn dispor_arquivos(itens: &mut [Item], prox: &mut u64) -> Resultado<()> {
    for it in itens.iter_mut() {
        if it.diretorio {
            dispor_arquivos(&mut it.filhos, prox)?;
        } else {
            // arquivo vazio aponta para o próximo setor livre, sem ocupá-lo
            it.setor = setor_u32(*prox)?;
            *prox += (it.tamanho as u64).div_ceil(SETOR);
        }
    }
    setor_u32(*prox)?;
    Ok(())
}

pub struct Opcoes {
    pub sobrescrever: bool,
    pub sem_atualizacao: bool,
    /// Libera o default.xbe para rodar de qualquer mídia (só na cópia gravada).
    pub liberar_midia: bool,
}

pub struct Resumo {
    pub arquivos: u64,
    pub bytes_conteudo: u64,
    pub tamanho_imagem: u64,
    pub midia: Option<Midia>,
}

// ---------------------------------------------------------------------------
// Remendo de mídia do XBE (opcional)
// ---------------------------------------------------------------------------
//
// O certificado do XBE diz de que mídias o jogo aceita rodar (campo "allowed
// media types"). Um jogo de disco aceita só o DVD do Xbox; com o campo
// liberado, o console (desbloqueado) aceita rodar o mesmo XBE do disco
// rígido ou de outra mídia. O certificado é assinado, então só serve em
// console desbloqueado ou emulador — e é por isso que é uma opção, nunca o
// padrão: a imagem deixa de ser idêntica ao disco.

/// Cabeçalho do XBE: assinatura, endereço base e endereço do certificado.
const XBE_ASSINATURA: &[u8; 4] = b"XBEH";
const XBE_BASE: usize = 0x104;
const XBE_CERTIFICADO: usize = 0x118;
/// No certificado: tipos de mídia permitidos.
const CERT_MIDIA: u32 = 0x9C;
/// Disco rígido, DVD/CD de todos os tipos e disco rígido não seguro.
const MIDIA_LIBERADA: u32 = 0x0000_00FF | 0x4000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Midia {
    pub antes: u32,
    pub depois: u32,
}

/// Onde fica o campo de mídia num XBE e o valor dele, lendo só o começo
/// do arquivo.
fn campo_midia(inicio: &[u8], tamanho: u32) -> Resultado<(u32, u32)> {
    let u32_em = |i: usize| {
        inicio
            .get(i..i + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    if inicio.get(0..4) != Some(XBE_ASSINATURA) {
        return Err(Erro::Destino(
            "default.xbe não começa com XBEH: não é um XBE válido".into(),
        ));
    }
    let (Some(base), Some(cert)) = (u32_em(XBE_BASE), u32_em(XBE_CERTIFICADO)) else {
        return Err(Erro::Destino("default.xbe tem o cabeçalho cortado".into()));
    };
    let pos = cert
        .checked_sub(base)
        .and_then(|c| c.checked_add(CERT_MIDIA))
        .filter(|&p| {
            p.checked_add(4)
                .is_some_and(|f| f <= tamanho && (f as usize) <= inicio.len())
        })
        .ok_or_else(|| {
            Erro::Destino("default.xbe: o certificado aponta para fora do cabeçalho".into())
        })?;
    Ok((pos, u32_em(pos as usize).unwrap()))
}

/// Prepara o remendo no default.xbe da raiz. Lê só o cabeçalho da origem.
fn preparar_midia(fonte: &mut Fonte, itens: &mut [Item]) -> Resultado<Midia> {
    let Some(xbe) = itens
        .iter_mut()
        .find(|i| !i.diretorio && i.nome.eq_ignore_ascii_case(b"default.xbe"))
    else {
        return Err(Erro::Destino(
            "--liberar-midia é só para jogos de Xbox: não há default.xbe na raiz".into(),
        ));
    };
    // o cabeçalho de um XBE cabe folgado em 64 KiB
    let mut inicio = vec![0u8; (xbe.tamanho as usize).min(64 * 1024)];
    match (&xbe.origem, fonte) {
        (Origem::Pasta(p), _) => File::open(p)?.read_exact(&mut inicio)?,
        (Origem::Imagem { setor }, Fonte::Imagem(img, _)) => {
            img.leitor_em(*setor)?.read_exact(&mut inicio)?
        }
        (Origem::Imagem { .. }, Fonte::Pasta(_)) => {
            unreachable!("item de imagem numa fonte de pasta")
        }
    }
    let (pos, antes) = campo_midia(&inicio, xbe.tamanho)?;
    let depois = antes | MIDIA_LIBERADA;
    if depois != antes {
        xbe.remendo = Some((pos, depois.to_le_bytes()));
    }
    Ok(Midia { antes, depois })
}

/// Aplica `remendo` ao trecho `buf`, que começa em `inicio` do arquivo.
fn aplicar_remendo(buf: &mut [u8], inicio: u64, remendo: Option<(u32, [u8; 4])>) {
    let Some((pos, novos)) = remendo else { return };
    for (k, b) in novos.iter().enumerate() {
        let alvo = pos as u64 + k as u64;
        if alvo >= inicio && alvo < inicio + buf.len() as u64 {
            buf[(alvo - inicio) as usize] = *b;
        }
    }
}

/// De onde vem o conteúdo.
pub enum Fonte<'a> {
    Pasta(&'a Path),
    Imagem(&'a mut Imagem, Vec<Entrada>),
}

/// Quanto conteúdo vai ser gravado (para a barra de progresso).
pub fn preparar(fonte: &mut Fonte, opcoes: &Opcoes) -> Resultado<Preparado> {
    let mut itens = match &*fonte {
        Fonte::Pasta(p) => {
            if !p.is_dir() {
                return Err(Erro::Destino(format!("{} não é uma pasta", p.display())));
            }
            de_pasta(p)?
        }
        Fonte::Imagem(_, entradas) => de_imagem(entradas),
    };
    if opcoes.sem_atualizacao {
        itens.retain(|i| !(i.diretorio && i.nome.eq_ignore_ascii_case(b"$SystemUpdate")));
    }
    let midia = if opcoes.liberar_midia {
        Some(preparar_midia(fonte, &mut itens)?)
    } else {
        None
    };
    // valida todas as tabelas antes de gravar qualquer byte
    conferir_tabelas(&itens, "")?;
    let mut arquivos = 0u64;
    let mut bytes = 0u64;
    contar(&itens, &mut arquivos, &mut bytes);
    Ok(Preparado {
        itens,
        midia,
        arquivos,
        bytes,
    })
}

pub struct Preparado {
    itens: Vec<Item>,
    pub midia: Option<Midia>,
    pub arquivos: u64,
    pub bytes: u64,
}

fn conferir_tabelas(itens: &[Item], caminho: &str) -> Resultado<()> {
    montar_tabela(itens, caminho)?;
    for it in itens.iter().filter(|i| i.diretorio) {
        let n = String::from_utf8_lossy(&it.nome);
        let cam = if caminho.is_empty() {
            n.to_string()
        } else {
            format!("{caminho}/{n}")
        };
        conferir_tabelas(&it.filhos, &cam)?;
    }
    Ok(())
}

fn contar(itens: &[Item], arquivos: &mut u64, bytes: &mut u64) {
    for it in itens {
        if it.diretorio {
            contar(&it.filhos, arquivos, bytes);
        } else {
            *arquivos += 1;
            *bytes += it.tamanho as u64;
        }
    }
}

/// Grava a imagem em `saida`.
pub fn gravar(
    mut fonte: Fonte,
    mut prep: Preparado,
    saida: &Path,
    opcoes: &Opcoes,
    progresso: &Progresso,
) -> Resultado<Resumo> {
    if saida.exists() && !opcoes.sobrescrever {
        return Err(Erro::Destino(format!(
            "{} já existe; escolha outro nome ou use --sobrescrever",
            saida.display()
        )));
    }
    if saida.is_dir() {
        return Err(Erro::Destino(format!("{} é uma pasta", saida.display())));
    }
    if let Fonte::Pasta(p) = &fonte
        && let (Ok(a), Some(pai)) = (fs::canonicalize(p), saida.parent())
        && let Ok(b) = fs::canonicalize(if pai.as_os_str().is_empty() {
            Path::new(".")
        } else {
            pai
        })
        && b.starts_with(&a)
    {
        return Err(Erro::Destino(
            "a imagem não pode ser gravada dentro da própria pasta de origem".into(),
        ));
    }

    let (setor_raiz, tamanho_raiz, setores) = dispor(&mut prep.itens)?;
    let tamanho_imagem = (setores * SETOR).div_ceil(ALINHAMENTO_FIM) * ALINHAMENTO_FIM;
    if let Some(livre) = sistema::espaco_livre(saida)
        && livre < tamanho_imagem
    {
        return Err(Erro::Destino(format!(
            "não cabe: a imagem vai ter {} e há {} livres",
            fmt_bytes(tamanho_imagem),
            fmt_bytes(livre)
        )));
    }

    let mut parcial = saida.as_os_str().to_owned();
    parcial.push(SUFIXO_PARCIAL);
    let parcial = PathBuf::from(parcial);
    let r = gravar_em(
        &mut fonte,
        &prep.itens,
        &parcial,
        setor_raiz,
        tamanho_raiz,
        tamanho_imagem,
        progresso,
    )
    .and_then(|()| {
        // relê a estrutura antes de dar o nome final
        let mut img = Imagem::abrir(&parcial)?;
        let t = arvore::totais(&arvore::ler(&mut img)?);
        if t.arquivos != prep.arquivos || t.bytes != prep.bytes {
            return Err(imagem(
                "a imagem gravada não bate com o conteúdo de origem ao ser relida",
            ));
        }
        Ok(())
    })
    .and_then(|()| fs::rename(&parcial, saida).map_err(Erro::from));
    if r.is_err() {
        fs::remove_file(&parcial).ok();
    }
    r?;
    Ok(Resumo {
        arquivos: prep.arquivos,
        bytes_conteudo: prep.bytes,
        tamanho_imagem,
        midia: prep.midia,
    })
}

fn gravar_em(
    fonte: &mut Fonte,
    raiz: &[Item],
    caminho: &Path,
    setor_raiz: u32,
    tamanho_raiz: u32,
    tamanho_imagem: u64,
    progresso: &Progresso,
) -> Resultado<()> {
    let arquivo = File::create(caminho)?;
    let mut w = Escritor {
        saida: BufWriter::with_capacity(BLOCO, arquivo),
        posicao: 0,
    };

    // setores reservados e descritor
    w.zeros(32 * SETOR)?;
    let mut d = vec![0u8; S];
    d[0..20].copy_from_slice(ASSINATURA);
    d[20..24].copy_from_slice(&setor_raiz.to_le_bytes());
    d[24..28].copy_from_slice(&tamanho_raiz.to_le_bytes());
    d[28..36].copy_from_slice(&agora_filetime().to_le_bytes());
    d[0x7EC..0x800].copy_from_slice(ASSINATURA);
    w.bytes(&d)?;

    // tabelas, na mesma ordem da disposição
    w.ir_para(setor_raiz)?;
    w.bytes(&montar_tabela(raiz, "")?)?;
    gravar_tabelas(&mut w, raiz)?;

    gravar_arquivos(&mut w, fonte, raiz, "", progresso)?;
    let fim = tamanho_imagem - w.posicao;
    w.zeros(fim)?;
    let arquivo = w.saida.into_inner().map_err(|e| e.into_error())?;
    arquivo.sync_all()?;
    Ok(())
}

fn gravar_tabelas(w: &mut Escritor, itens: &[Item]) -> Resultado<()> {
    for it in itens.iter().filter(|i| i.diretorio) {
        w.ir_para(it.setor)?;
        w.bytes(&montar_tabela(&it.filhos, "")?)?;
    }
    for it in itens.iter().filter(|i| i.diretorio) {
        gravar_tabelas(w, &it.filhos)?;
    }
    Ok(())
}

fn gravar_arquivos(
    w: &mut Escritor,
    fonte: &mut Fonte,
    itens: &[Item],
    caminho: &str,
    progresso: &Progresso,
) -> Resultado<()> {
    let mut buf = Vec::new();
    for it in itens {
        if sistema::cancelado() {
            return Err(Erro::Cancelado);
        }
        let n = String::from_utf8_lossy(&it.nome);
        let cam = if caminho.is_empty() {
            n.to_string()
        } else {
            format!("{caminho}/{n}")
        };
        if it.diretorio {
            gravar_arquivos(w, fonte, &it.filhos, &cam, progresso)?;
            continue;
        }
        if it.tamanho == 0 {
            continue;
        }
        w.ir_para(it.setor)?;
        let mut leitor: Box<dyn Read + '_> = match (&it.origem, &mut *fonte) {
            (Origem::Pasta(p), _) => {
                let f = File::open(p)?;
                // o arquivo mudou de tamanho desde que a pasta foi lida?
                if f.metadata()?.len() != it.tamanho as u64 {
                    return Err(Erro::Destino(format!(
                        "{cam} mudou de tamanho durante a criação da imagem"
                    )));
                }
                Box::new(f)
            }
            (Origem::Imagem { setor }, Fonte::Imagem(img, _)) => Box::new(img.leitor_em(*setor)?),
            (Origem::Imagem { .. }, Fonte::Pasta(_)) => {
                unreachable!("item de imagem numa fonte de pasta")
            }
        };
        let mut restante = it.tamanho as usize;
        let mut lido = 0u64;
        buf.resize(BLOCO.min(restante), 0);
        while restante > 0 {
            if sistema::cancelado() {
                return Err(Erro::Cancelado);
            }
            let k = restante.min(buf.len());
            leitor.read_exact(&mut buf[..k]).map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    Erro::Destino(format!("{cam} ficou menor durante a criação da imagem"))
                } else {
                    e.into()
                }
            })?;
            aplicar_remendo(&mut buf[..k], lido, it.remendo);
            lido += k as u64;
            w.bytes(&buf[..k])?;
            restante -= k;
            progresso.avancar(k as u64, &n);
        }
    }
    Ok(())
}

/// Grava sequencialmente, preenchendo com zeros até cada setor pedido.
struct Escritor {
    saida: BufWriter<File>,
    posicao: u64,
}

impl Escritor {
    fn bytes(&mut self, b: &[u8]) -> Resultado<()> {
        self.saida.write_all(b)?;
        self.posicao += b.len() as u64;
        Ok(())
    }

    fn zeros(&mut self, mut n: u64) -> Resultado<()> {
        static Z: [u8; 64 * 1024] = [0; 64 * 1024];
        while n > 0 {
            let k = n.min(Z.len() as u64) as usize;
            self.bytes(&Z[..k])?;
            n -= k as u64;
        }
        Ok(())
    }

    fn ir_para(&mut self, setor: u32) -> Resultado<()> {
        let alvo = setor as u64 * SETOR;
        if alvo < self.posicao {
            return Err(imagem(format!(
                "erro interno: gravação voltaria do byte {} para o {alvo}",
                self.posicao
            )));
        }
        self.zeros(alvo - self.posicao)
    }
}

fn agora_filetime() -> u64 {
    let seg = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (seg + 11_644_473_600) * 10_000_000
}

/// Nome padrão da imagem criada a partir de uma pasta: ao lado dela.
pub fn saida_padrao_pasta(pasta: &Path) -> PathBuf {
    let nome = pasta
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_owned()))
        .unwrap_or_else(|| "imagem".into());
    let mut n = nome;
    n.push(".iso");
    pasta
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join(n)
}

/// Nome padrão da imagem reescrita: `<nome>.xiso.iso` ao lado da original.
pub fn saida_padrao_reescrita(iso: &Path) -> PathBuf {
    let mut n = iso.file_stem().unwrap_or_default().to_owned();
    n.push(".xiso.iso");
    iso.with_file_name(n)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn remendo_atravessando_blocos() {
        let r = Some((6u32, [1u8, 2, 3, 4]));
        let mut a = [0u8; 8];
        let mut b = [0u8; 8];
        aplicar_remendo(&mut a, 0, r);
        aplicar_remendo(&mut b, 8, r);
        assert_eq!(a, [0, 0, 0, 0, 0, 0, 1, 2]);
        assert_eq!(b, [3, 4, 0, 0, 0, 0, 0, 0]);
    }
}
