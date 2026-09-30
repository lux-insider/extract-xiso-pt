//! Conferência de uma imagem: a estrutura inteira (a mesma validação da
//! leitura, mais sobreposição de trechos), a leitura de todos os bytes do
//! arquivo e os hashes CRC32, MD5 e SHA-1 — opcionalmente comparados com um
//! .dat do Redump (ou qualquer .dat no formato Logiqx XML).

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use md5::Md5;
use sha1::{Digest, Sha1};

use crate::arvore::{self, Entrada};
use crate::erro::{Erro, Resultado};
use crate::imagem::{Imagem, Layout, SETOR};
use crate::progresso::Progresso;
use crate::sistema;

const BLOCO: usize = 4 * 1024 * 1024;

/// Tamanho do arquivo de um disco completo (como o Redump cataloga).
fn tamanho_disco_completo(layout: Layout) -> Option<u64> {
    match layout {
        Layout::Xiso => None,
        Layout::Xgd1 => Some(7_825_162_240),
        Layout::Xgd2 => Some(7_835_492_352),
        Layout::Xgd3 => Some(8_738_846_720),
    }
}

#[derive(Debug, Default, serde::Serialize)]
pub struct Hashes {
    pub tamanho: u64,
    pub crc32: String,
    pub md5: String,
    pub sha1: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Situacao {
    /// SHA-1 e tamanho iguais a uma entrada do .dat.
    Confere,
    /// Há uma entrada com o mesmo nome de arquivo, mas os bytes diferem.
    NaoConfere,
    /// Nenhuma entrada com este hash nem com este nome.
    NaoEncontrada,
    /// Imagem enxuta (XISO) com o nome de um jogo do .dat: o Redump guarda o
    /// hash do disco completo, então o SHA-1 nunca poderia conferir — não é
    /// sinal de problema.
    Enxuta,
}

#[derive(Debug, serde::Serialize)]
pub struct ResultadoDat {
    pub situacao: Situacao,
    /// Jogo encontrado (quando confere) ou o jogo esperado pelo nome.
    pub jogo: Option<String>,
    pub rom: Option<String>,
    /// Em qual .dat estava (sistema e versão).
    pub fonte: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub struct Relatorio {
    pub layout: &'static str,
    /// Tamanho de um disco completo deste layout e se a imagem tem esse
    /// tamanho (uma imagem enxuta nunca confere com o Redump).
    pub disco_completo: Option<bool>,
    pub arquivos: u64,
    pub diretorios: u64,
    pub avisos: Vec<String>,
    pub hashes: Hashes,
    pub dat: Option<ResultadoDat>,
    /// Os .dat comparados (sistema e versão).
    pub dats_usados: Vec<String>,
}

/// Estrutura, trechos e total de bytes a ler.
pub fn estrutura(img: &mut Imagem) -> Resultado<(Vec<Entrada>, Vec<String>)> {
    let raiz = arvore::ler(img)?; // já recusa ciclos, nomes, trechos fora
    let mut avisos = Vec::new();

    // trechos ocupados: tabelas e arquivos com conteúdo
    let mut trechos: Vec<(u64, u64, String)> = Vec::new();
    trechos.push((
        img.setor_raiz as u64,
        setores(img.tamanho_raiz),
        "tabela da raiz".into(),
    ));
    arvore::percorrer(&raiz, &mut |e, caminho| {
        if e.tamanho > 0 {
            let o_que = if e.eh_diretorio() {
                format!("tabela de {caminho}")
            } else {
                caminho.to_string()
            };
            trechos.push((e.setor as u64, setores(e.tamanho), o_que));
        }
    });
    trechos.sort_by_key(|t| t.0);
    let mut sobrepostos = 0usize;
    for par in trechos.windows(2) {
        if par[0].0 + par[0].1 > par[1].0 {
            sobrepostos += 1;
            if sobrepostos <= 10 {
                avisos.push(format!(
                    "{} e {} usam os mesmos setores",
                    par[0].2, par[1].2
                ));
            }
        }
    }
    if sobrepostos > 10 {
        avisos.push(format!("... e mais {} sobreposições", sobrepostos - 10));
    }
    if trechos.first().is_some_and(|t| t.0 < 33) {
        avisos.push("há conteúdo antes do descritor de volume (setores reservados)".into());
    }
    Ok((raiz, avisos))
}

fn setores(bytes: u32) -> u64 {
    (bytes as u64).div_ceil(SETOR)
}

/// Lê o arquivo inteiro do início ao fim calculando os três hashes, cada um
/// numa thread: a leitura acontece uma vez só e o tempo total fica sendo o
/// do hash mais lento, não a soma dos três.
pub fn hashes(caminho: &Path, progresso: &Progresso) -> Resultado<Hashes> {
    use std::sync::Arc;
    use std::sync::mpsc::{Receiver, sync_channel};

    fn consumir<H>(
        rx: Receiver<Arc<Vec<u8>>>,
        mut h: H,
        mut atualizar: impl FnMut(&mut H, &[u8]),
    ) -> H {
        for bloco in rx {
            atualizar(&mut h, &bloco);
        }
        h
    }

    let mut f = File::open(caminho)?;
    let tamanho = f.metadata()?.len();
    std::thread::scope(|escopo| {
        // fila curta: no máximo alguns blocos na memória por hash
        let (tx_crc, rx_crc) = sync_channel::<Arc<Vec<u8>>>(4);
        let (tx_md5, rx_md5) = sync_channel::<Arc<Vec<u8>>>(4);
        let (tx_sha, rx_sha) = sync_channel::<Arc<Vec<u8>>>(4);
        let crc =
            escopo.spawn(move || consumir(rx_crc, crc32fast::Hasher::new(), |h, b| h.update(b)));
        let md5 = escopo.spawn(move || consumir(rx_md5, Md5::new(), |h, b| h.update(b)));
        let sha = escopo.spawn(move || consumir(rx_sha, Sha1::new(), |h, b| h.update(b)));

        let mut lido = 0u64;
        let leitura: Resultado<()> = (|| {
            loop {
                if sistema::cancelado() {
                    return Err(Erro::Cancelado);
                }
                let mut buf = vec![0u8; BLOCO];
                let mut n = 0;
                // enche o bloco (read pode devolver menos que o pedido)
                while n < BLOCO {
                    let k = f.read(&mut buf[n..])?;
                    if k == 0 {
                        break;
                    }
                    n += k;
                }
                if n == 0 {
                    return Ok(());
                }
                buf.truncate(n);
                let bloco = Arc::new(buf);
                for tx in [&tx_crc, &tx_md5, &tx_sha] {
                    // só falha se a thread morreu, o que o join abaixo relata
                    let _ = tx.send(Arc::clone(&bloco));
                }
                lido += n as u64;
                progresso.avancar(n as u64, "");
            }
        })();
        // fecha as filas para as threads terminarem, mesmo em caso de erro
        drop((tx_crc, tx_md5, tx_sha));
        let (crc, md5, sha) = (crc.join(), md5.join(), sha.join());
        leitura?;
        let (Ok(crc), Ok(md5), Ok(sha)) = (crc, md5, sha) else {
            return Err(Erro::Imagem("uma das threads de hash falhou".into()));
        };
        if lido != tamanho {
            return Err(Erro::Imagem(format!(
                "foram lidos {lido} de {tamanho} bytes: o arquivo mudou durante a leitura"
            )));
        }
        Ok(Hashes {
            tamanho,
            crc32: format!("{:08x}", crc.finalize()),
            md5: hex(&md5.finalize()),
            sha1: hex(&sha.finalize()),
        })
    })
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

// ---------------------------------------------------------------------------
// .dat (Logiqx XML, o formato do Redump)
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub struct Rom {
    pub jogo: String,
    pub nome: String,
    pub tamanho: Option<u64>,
    pub sha1: Option<String>,
}

/// Lê as entradas `<rom>` de um .dat, cada uma com o nome do `<game>` (ou
/// `<machine>`) em que está. Não é um leitor de XML completo: só o que o
/// formato usa (atributos entre aspas, entidades padrão).
pub fn ler_dat(texto: &str) -> Vec<Rom> {
    let mut roms = Vec::new();
    let mut jogo = String::new();
    let mut resto = texto;
    while let Some(i) = resto.find('<') {
        resto = &resto[i + 1..];
        let Some(fim) = resto.find('>') else { break };
        let tag = &resto[..fim];
        resto = &resto[fim + 1..];
        let (nome_tag, attrs) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        match nome_tag {
            "game" | "machine" => jogo = atributo(attrs, "name").unwrap_or_default(),
            "rom" => roms.push(Rom {
                jogo: jogo.clone(),
                nome: atributo(attrs, "name").unwrap_or_default(),
                tamanho: atributo(attrs, "size").and_then(|s| s.parse().ok()),
                sha1: atributo(attrs, "sha1").map(|s| s.to_ascii_lowercase()),
            }),
            _ => {}
        }
    }
    roms
}

fn atributo(attrs: &str, nome: &str) -> Option<String> {
    let mut resto = attrs;
    loop {
        let eq = resto.find('=')?;
        let chave = resto[..eq].trim();
        let depois = resto[eq + 1..].trim_start();
        let aspas = depois.chars().next()?;
        if aspas != '"' && aspas != '\'' {
            return None;
        }
        let fim = depois[1..].find(aspas)? + 1;
        let valor = &depois[1..fim];
        if chave == nome {
            return Some(entidades(valor));
        }
        resto = &depois[fim + 1..];
    }
}

fn entidades(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut saida = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(i) = resto.find('&') {
        saida.push_str(&resto[..i]);
        resto = &resto[i..];
        let Some(fim) = resto.find(';') else { break };
        let ent = &resto[1..fim];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16)
                .ok()
                .and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                saida.push(c);
                resto = &resto[fim + 1..];
            }
            None => {
                saida.push('&');
                resto = &resto[1..];
            }
        }
    }
    saida.push_str(resto);
    saida
}

pub fn comparar_dat(roms: &[Rom], h: &Hashes, nome_arquivo: &str) -> ResultadoDat {
    if let Some(r) = roms.iter().find(|r| {
        r.sha1.as_deref() == Some(h.sha1.as_str()) && r.tamanho.is_none_or(|t| t == h.tamanho)
    }) {
        return ResultadoDat {
            situacao: Situacao::Confere,
            jogo: Some(r.jogo.clone()),
            rom: Some(r.nome.clone()),
            fonte: None,
        };
    }
    if let Some(r) = roms
        .iter()
        .find(|r| r.nome.eq_ignore_ascii_case(nome_arquivo))
    {
        return ResultadoDat {
            situacao: Situacao::NaoConfere,
            jogo: Some(r.jogo.clone()),
            rom: Some(r.nome.clone()),
            fonte: None,
        };
    }
    ResultadoDat {
        situacao: Situacao::NaoEncontrada,
        jogo: None,
        rom: None,
        fonte: None,
    }
}

/// Tudo junto: estrutura, leitura completa e .dat (já carregados: um .dat
/// errado tem que falhar antes da leitura inteira, não depois).
pub fn verificar(
    caminho: &Path,
    dats: &[crate::dats::Dat],
    progresso_de: impl FnOnce(u64) -> Progresso,
) -> Resultado<Relatorio> {
    let mut img = Imagem::abrir(caminho)?;
    let (raiz, avisos) = estrutura(&mut img)?;
    let t = arvore::totais(&raiz);
    let layout = img.layout;
    drop(img);

    let tamanho = fs::metadata(caminho)?.len();
    let p = progresso_de(tamanho);
    let r = hashes(caminho, &p);
    p.terminar(r.is_ok());
    let h = r?;

    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dat = (!dats.is_empty()).then(|| {
        // o que confere, em qualquer .dat; senão, o primeiro que conhece o nome
        let resultados: Vec<ResultadoDat> = dats
            .iter()
            .map(|d| comparar_dat(&d.roms, &h, &nome))
            .collect();
        let melhor = resultados
            .into_iter()
            .enumerate()
            .min_by_key(|(_, r)| match r.situacao {
                Situacao::Confere => 0,
                Situacao::NaoConfere => 1,
                Situacao::NaoEncontrada | Situacao::Enxuta => 2,
            });
        let (i, mut r) = melhor.expect("há pelo menos um .dat");
        if !matches!(r.situacao, Situacao::NaoEncontrada) {
            r.fonte = Some(format!("{} ({})", dats[i].sistema, dats[i].versao));
        }
        // uma XISO não tem os bytes do disco: pelo nome, só dá para dizer de
        // que jogo ela é uma cópia enxuta
        if matches!(r.situacao, Situacao::NaoConfere) && layout == Layout::Xiso {
            r.situacao = Situacao::Enxuta;
        }
        r
    });
    let dats_usados = dats
        .iter()
        .map(|d| format!("{} ({})", d.sistema, d.versao))
        .collect();
    Ok(Relatorio {
        layout: layout.rotulo(),
        disco_completo: tamanho_disco_completo(layout).map(|c| c == tamanho),
        arquivos: t.arquivos,
        diretorios: t.diretorios,
        avisos,
        hashes: h,
        dat,
        dats_usados,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    const DAT: &str = r#"<?xml version="1.0"?>
<datafile>
  <header><name>Microsoft - Xbox 360</name></header>
  <game name="Jogo &amp; Cia (USA)">
    <category>Games</category>
    <rom name="Jogo &amp; Cia (USA).iso" size="3" crc="352441c2" md5="900150983cd24fb0d6963f7d28e17f72" sha1="A9993E364706816ABA3E25717850C26C9CD0D89D"/>
  </game>
  <machine name='Outro'><rom size='1' name='outro.iso' sha1='00'/></machine>
</datafile>"#;

    #[test]
    fn le_dat_com_entidades_e_aspas_simples() {
        let r = ler_dat(DAT);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].jogo, "Jogo & Cia (USA)");
        assert_eq!(r[0].nome, "Jogo & Cia (USA).iso");
        assert_eq!(r[0].tamanho, Some(3));
        assert_eq!(
            r[0].sha1.as_deref(),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
        assert_eq!(r[1].jogo, "Outro");
        assert_eq!(entidades("a&#233;&#xE7;&lt;&bogus;"), "aéç<&bogus;");
    }

    #[test]
    fn hashes_conhecidos_e_comparacao() {
        let d = std::env::temp_dir().join(format!("extract-xiso-pt-hash-{}", std::process::id()));
        fs::write(&d, b"abc").unwrap();
        let h = hashes(&d, &Progresso::novo("", "", 3, true)).unwrap();
        fs::remove_file(&d).ok();
        assert_eq!(h.crc32, "352441c2");
        assert_eq!(h.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(h.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");

        let roms = ler_dat(DAT);
        assert!(matches!(
            comparar_dat(&roms, &h, "x.iso").situacao,
            Situacao::Confere
        ));
        let outro = Hashes {
            sha1: "ff".into(),
            ..h
        };
        assert!(matches!(
            comparar_dat(&roms, &outro, "OUTRO.ISO").situacao,
            Situacao::NaoConfere
        ));
        assert!(matches!(
            comparar_dat(&roms, &outro, "nada.iso").situacao,
            Situacao::NaoEncontrada
        ));
    }
}
