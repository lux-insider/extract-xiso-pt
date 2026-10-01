//! A árvore de arquivos da imagem, lida seguindo os ponteiros da árvore
//! binária de cada tabela de diretório — só existe o que se alcança a partir
//! da raiz. Tudo que vem da imagem é conferido: ciclos, profundidade, nomes
//! que sairiam da pasta de destino e trechos além do fim do volume viram erro
//! explicado, nunca pânico, travamento ou arquivo gravado no lugar errado.

use std::collections::HashSet;

use crate::erro::{Erro, Resultado, imagem};
use crate::imagem::{Imagem, SETOR};

pub const ATTR_DIRETORIO: u8 = 0x10;
/// Mais que isto é imagem corrompida (um jogo real tem poucos níveis).
pub const PROFUNDIDADE_MAXIMA: usize = 64;
/// Teto de entradas na imagem inteira: um disco real tem alguns milhares.
const MAX_ENTRADAS: usize = 1_000_000;
/// Uma tabela de diretório real tem poucos KB; 16 MB já é absurdo.
const MAX_TABELA: u32 = 16 * 1024 * 1024;
/// Quanto de uma tabela os ponteiros alcançam: um filho fica a no máximo
/// `0xFFFF` palavras de 4 bytes do início, e o nó dele tem 14 bytes de
/// cabeçalho e até 255 de nome. O resto da tabela declarada nunca é lido
/// pela árvore, então não precisa sair do disco.
const ALCANCAVEL: usize = 0xFFFF * 4 + 14 + 255;
/// Teto de bytes de tabela lidos na imagem inteira. Um disco real lê poucos
/// MB; isto só para imagens em que vários diretórios apontam para as mesmas
/// tabelas, que de outro jeito seriam relidas a cada visita, com o número
/// de visitas dobrando a cada nível.
const MAX_LIDO_TABELAS: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Entrada {
    /// Nome como está no disco (UTF-8 se válido, senão Latin-1).
    pub nome: String,
    pub setor: u32,
    pub tamanho: u32,
    pub atributos: u8,
    /// Só em diretórios: o conteúdo, em ordem alfabética.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filhos: Vec<Entrada>,
}

impl Entrada {
    pub fn eh_diretorio(&self) -> bool {
        self.atributos & ATTR_DIRETORIO != 0
    }
}

/// Estatísticas da árvore inteira.
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct Totais {
    pub arquivos: u64,
    pub diretorios: u64,
    pub bytes: u64,
}

pub fn totais(entradas: &[Entrada]) -> Totais {
    let mut t = Totais::default();
    percorrer(entradas, &mut |e, _| {
        if e.eh_diretorio() {
            t.diretorios += 1;
        } else {
            t.arquivos += 1;
            t.bytes += e.tamanho as u64;
        }
    });
    t
}

/// Visita cada entrada (pai antes dos filhos) com o caminho relativo dela,
/// usando "/" como separador.
pub fn percorrer(entradas: &[Entrada], f: &mut dyn FnMut(&Entrada, &str)) {
    fn rec(entradas: &[Entrada], prefixo: &str, f: &mut dyn FnMut(&Entrada, &str)) {
        for e in entradas {
            let caminho = if prefixo.is_empty() {
                e.nome.clone()
            } else {
                format!("{prefixo}/{}", e.nome)
            };
            f(e, &caminho);
            if e.eh_diretorio() {
                rec(&e.filhos, &caminho, f);
            }
        }
    }
    rec(entradas, "", f)
}

/// Lê a árvore inteira a partir da raiz.
pub fn ler(img: &mut Imagem) -> Resultado<Vec<Entrada>> {
    let mut leitor = Leitor {
        ancestrais: HashSet::new(),
        contagem: 0,
        lido: 0,
    };
    let (setor, tamanho) = (img.setor_raiz, img.tamanho_raiz);
    leitor.diretorio(img, setor, tamanho, "", 0)
}

struct Leitor {
    /// Setores das tabelas no caminho atual (um diretório que aponta para um
    /// ancestral faria a leitura girar para sempre).
    ancestrais: HashSet<u32>,
    contagem: usize,
    /// Bytes de tabela lidos até agora (ver `MAX_LIDO_TABELAS`).
    lido: u64,
}

impl Leitor {
    fn diretorio(
        &mut self,
        img: &mut Imagem,
        setor: u32,
        tamanho: u32,
        caminho: &str,
        profundidade: usize,
    ) -> Resultado<Vec<Entrada>> {
        let onde = if caminho.is_empty() {
            "a raiz".to_string()
        } else {
            format!("o diretório {caminho}")
        };
        let da_tabela = if caminho.is_empty() {
            "a tabela da raiz".to_string()
        } else {
            format!("a tabela do diretório {caminho}")
        };
        if tamanho == 0 {
            return Ok(Vec::new()); // diretório vazio
        }
        if profundidade > PROFUNDIDADE_MAXIMA {
            return Err(imagem(format!(
                "{onde} está a mais de {PROFUNDIDADE_MAXIMA} níveis: a imagem está corrompida"
            )));
        }
        if tamanho > MAX_TABELA {
            return Err(imagem(format!(
                "{onde} declara uma tabela de {tamanho} bytes: a imagem está corrompida"
            )));
        }
        if !self.ancestrais.insert(setor) {
            return Err(imagem(format!(
                "{onde} aponta para uma tabela que já está no caminho até ele (setor {setor}): \
                 a imagem está corrompida"
            )));
        }
        // a tabela declarada inteira tem que estar dentro da imagem; dela,
        // só o trecho que os ponteiros alcançam é lido
        img.conferir_trecho(setor, tamanho as u64, &da_tabela)?;
        let alcancavel = (tamanho as usize).min(ALCANCAVEL);
        self.lido += alcancavel as u64;
        if self.lido > MAX_LIDO_TABELAS {
            return Err(imagem(
                "as tabelas de diretório somam mais de 1 GiB lido (diretórios apontando para \
                 as mesmas tabelas): a imagem está corrompida",
            ));
        }
        let tabela = img.ler(setor, alcancavel, &da_tabela)?;

        // Em ordem (esquerda, nó, direita): a árvore é ordenada pelo nome.
        let mut nos = Vec::new();
        let mut visitados = HashSet::new();
        // Um diretório vazio pode ter tabela de tamanho 0 (tratado acima) ou
        // um setor inteiro de preenchimento (0xFF), como grava o extract-xiso:
        // sem nó na raiz da tabela, não há entradas.
        if tabela.get(0..4).is_none_or(|b| b == [0xFF; 4]) {
            self.ancestrais.remove(&setor);
            return Ok(Vec::new());
        }
        em_ordem(&tabela, 0, &mut visitados, &mut nos, &da_tabela)?;

        let mut entradas = Vec::with_capacity(nos.len());
        let mut nomes = HashSet::new();
        for no in nos {
            self.contagem += 1;
            if self.contagem > MAX_ENTRADAS {
                return Err(imagem(
                    "mais de um milhão de entradas: a imagem está corrompida",
                ));
            }
            let caminho_e = if caminho.is_empty() {
                no.nome.clone()
            } else {
                format!("{caminho}/{}", no.nome)
            };
            validar_nome(&no.nome, &caminho_e)?;
            if !nomes.insert(no.nome.to_lowercase()) {
                return Err(Erro::NomeInseguro(format!(
                    "{caminho_e} aparece duas vezes no mesmo diretório (sem diferenciar \
                     maiúsculas, seriam o mesmo arquivo no Windows)"
                )));
            }
            let mut e = Entrada {
                nome: no.nome,
                setor: no.setor,
                tamanho: no.tamanho,
                atributos: no.atributos,
                filhos: Vec::new(),
            };
            if e.eh_diretorio() {
                e.filhos = self.diretorio(img, e.setor, e.tamanho, &caminho_e, profundidade + 1)?;
            } else if e.tamanho > 0 {
                img.conferir_trecho(e.setor, e.tamanho as u64, &format!("o arquivo {caminho_e}"))?;
            }
            entradas.push(e);
        }

        self.ancestrais.remove(&setor);
        Ok(entradas)
    }
}

struct No {
    nome: String,
    setor: u32,
    tamanho: u32,
    atributos: u8,
}

/// Percorre a árvore binária da tabela a partir do nó em `pos` (em bytes),
/// em ordem, sem recursão (uma árvore degenerada de milhares de nós não pode
/// estourar a pilha).
fn em_ordem(
    tabela: &[u8],
    raiz: usize,
    visitados: &mut HashSet<usize>,
    saida: &mut Vec<No>,
    onde: &str,
) -> Resultado<()> {
    // pilha de (posição, já desceu à esquerda?)
    let mut pilha: Vec<(usize, bool)> = vec![(raiz, false)];
    while let Some((pos, desceu)) = pilha.pop() {
        let no = ler_no(tabela, pos, onde)?;
        let (esq, dir) = (no.0, no.1);
        if !desceu {
            if !visitados.insert(pos) {
                return Err(imagem(format!(
                    "{onde} tem um nó que aponta de volta para outro (deslocamento \
                     {pos}): a imagem está corrompida"
                )));
            }
            pilha.push((pos, true));
            if esq != 0 {
                pilha.push((esq as usize * 4, false));
            }
        } else {
            saida.push(no.2);
            if dir != 0 {
                pilha.push((dir as usize * 4, false));
            }
        }
    }
    Ok(())
}

fn ler_no(tabela: &[u8], pos: usize, onde: &str) -> Resultado<(u16, u16, No)> {
    let fora = || {
        imagem(format!(
            "{onde} tem um nó fora dela (deslocamento {pos}): a imagem está corrompida"
        ))
    };
    let cab = tabela
        .get(pos..pos.checked_add(14).ok_or_else(fora)?)
        .ok_or_else(fora)?;
    let esq = u16::from_le_bytes([cab[0], cab[1]]);
    let dir = u16::from_le_bytes([cab[2], cab[3]]);
    if esq == 0xFFFF || dir == 0xFFFF {
        // preenchimento de fim de setor, não um nó
        return Err(fora());
    }
    let setor = u32::from_le_bytes(cab[4..8].try_into().unwrap());
    let tamanho = u32::from_le_bytes(cab[8..12].try_into().unwrap());
    let atributos = cab[12];
    let n = cab[13] as usize;
    let bytes = tabela.get(pos + 14..pos + 14 + n).ok_or_else(fora)?;
    // setor + tamanho de nó dentro do mesmo setor de tabela
    if (pos % SETOR as usize) + 14 + n > SETOR as usize {
        return Err(imagem(format!(
            "{onde} tem uma entrada atravessando o fim de um setor: a imagem está corrompida"
        )));
    }
    // Discos oficiais só usam ASCII. Imagens caseiras costumam gravar o nome
    // em UTF-8 (o que o sistema de arquivos de origem tinha); fora isso, Latin-1.
    let nome = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    };
    Ok((
        esq,
        dir,
        No {
            nome,
            setor,
            tamanho,
            atributos,
        },
    ))
}

/// Recusa nomes que não podem virar arquivo com segurança: sairiam da pasta
/// de destino, ou não existem (ou significam outra coisa) no Windows.
pub fn validar_nome(nome: &str, caminho: &str) -> Resultado<()> {
    let recusar = |motivo: &str| Err(Erro::NomeInseguro(format!("\"{caminho}\": {motivo}")));
    if nome.is_empty() {
        return recusar("nome vazio");
    }
    if nome == "." || nome == ".." {
        return recusar("nome reservado que apontaria para fora do diretório");
    }
    if let Some(c) = nome.chars().find(|c| {
        matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
            || (*c as u32) < 0x20
            || *c as u32 == 0x7F
    }) {
        return recusar(&format!("tem o caractere proibido {c:?}"));
    }
    if nome.ends_with(' ') || nome.ends_with('.') {
        return recusar("termina em espaço ou ponto (o Windows corta isso e o nome muda)");
    }
    let base = nome.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVADOS: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVADOS.contains(&base.as_str()) {
        return recusar("é um nome de dispositivo reservado no Windows");
    }
    Ok(())
}
