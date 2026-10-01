//! Os .dat do Redump instalados: onde ficam, como instalar (direto do .zip
//! que o site entrega) e como carregar todos para o `verificar`.
//!
//! Onde procura, nesta ordem (o primeiro que existir):
//!   1. `$EXTRACT_XISO_PT_DATS`
//!   2. `dats/` ao lado do executável (pasta portátil, como a do Windows)
//!   3. Linux: `$XDG_DATA_HOME/extract-xiso-pt/dats` ou
//!      `~/.local/share/extract-xiso-pt/dats`; Windows: `%APPDATA%\extract-xiso-pt\dats`
//!
//! `dats instalar` grava no primeiro que já existir, ou cria o 3.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::erro::{Erro, Resultado};
use crate::temporario;
use crate::verificar::{Rom, ler_dat};

/// Um .dat é texto de poucos MB; mais que isto não é um .dat.
const MAX_DAT: usize = 64 * 1024 * 1024;

pub struct Dat {
    /// Nome do arquivo instalado.
    pub arquivo: String,
    /// `<name>` do cabeçalho (ex.: "Microsoft - Xbox 360").
    pub sistema: String,
    /// `<version>` do cabeçalho.
    pub versao: String,
    pub roms: Vec<Rom>,
}

fn candidatas() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os("EXTRACT_XISO_PT_DATS").filter(|p| !p.is_empty()) {
        v.push(PathBuf::from(p));
    }
    if let Some(pasta) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        v.push(pasta.join("dats"));
    }
    #[cfg(windows)]
    if let Some(a) = std::env::var_os("APPDATA") {
        v.push(PathBuf::from(a).join("extract-xiso-pt").join("dats"));
    }
    #[cfg(not(windows))]
    {
        if let Some(x) = std::env::var_os("XDG_DATA_HOME").filter(|p| !p.is_empty()) {
            v.push(PathBuf::from(x).join("extract-xiso-pt").join("dats"));
        } else if let Some(h) = std::env::var_os("HOME") {
            v.push(PathBuf::from(h).join(".local/share/extract-xiso-pt/dats"));
        }
    }
    v
}

/// A pasta de .dat em uso: a primeira que existe, ou a padrão do usuário.
pub fn pasta() -> Option<PathBuf> {
    let c = candidatas();
    c.iter()
        .find(|p| p.is_dir())
        .cloned()
        .or_else(|| c.last().cloned())
}

/// Lê um .dat, ou todos os .dat de dentro de um .zip.
pub fn ler_arquivo(caminho: &Path) -> Resultado<Vec<(String, String)>> {
    let bytes = fs::read(caminho)?;
    if bytes.len() > MAX_DAT {
        return Err(Erro::Destino(format!(
            "{} é grande demais para um .dat",
            caminho.display()
        )));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        let dats: Vec<(String, String)> = zip::ler(&bytes)?
            .into_iter()
            .filter(|(n, _)| n.to_ascii_lowercase().ends_with(".dat"))
            .map(|(n, b)| (n, String::from_utf8_lossy(&b).into_owned()))
            .collect();
        if dats.is_empty() {
            return Err(Erro::Destino(format!(
                "{} não tem nenhum .dat dentro",
                caminho.display()
            )));
        }
        return Ok(dats);
    }
    let nome = caminho
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(vec![(nome, String::from_utf8_lossy(&bytes).into_owned())])
}

/// Um .dat já lido: confere que é mesmo um .dat e tira o cabeçalho.
fn montar(arquivo: String, texto: &str) -> Resultado<Dat> {
    let roms = ler_dat(texto);
    if roms.is_empty() {
        return Err(Erro::Destino(format!(
            "{arquivo} não tem nenhuma entrada <rom>: não parece um .dat"
        )));
    }
    let cab = texto
        .find("<header>")
        .and_then(|i| texto[i..].find("</header>").map(|f| &texto[i..i + f]));
    let campo = |tag: &str| {
        cab.and_then(|c| {
            let a = c.find(&format!("<{tag}>"))? + tag.len() + 2;
            let f = c[a..].find(&format!("</{tag}>"))?;
            Some(c[a..a + f].trim().to_string())
        })
        .unwrap_or_default()
    };
    Ok(Dat {
        arquivo,
        sistema: campo("name"),
        versao: campo("version"),
        roms,
    })
}

/// Carrega o que o usuário passou em `--dat` (.dat ou .zip).
pub fn carregar(caminho: &Path) -> Resultado<Vec<Dat>> {
    ler_arquivo(caminho)?
        .into_iter()
        .map(|(n, t)| montar(n, &t))
        .collect()
}

/// Todos os .dat instalados. Um arquivo que não dá para ler é pulado com
/// aviso, para um .dat estragado não impedir o `verificar`.
pub fn instalados() -> (Vec<Dat>, Vec<String>) {
    let mut dats = Vec::new();
    let mut avisos = Vec::new();
    let Some(p) = pasta().filter(|p| p.is_dir()) else {
        return (dats, avisos);
    };
    let Ok(ler) = fs::read_dir(&p) else {
        return (dats, avisos);
    };
    let mut arquivos: Vec<PathBuf> = ler
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dat")))
        .collect();
    arquivos.sort();
    for a in arquivos {
        match carregar(&a) {
            Ok(mut d) => dats.append(&mut d),
            Err(e) => avisos.push(format!("{}: {e}", a.display())),
        }
    }
    (dats, avisos)
}

/// Instala os .dat de `origem` (.dat ou .zip) na pasta de .dat. Um .dat mais
/// antigo do mesmo sistema é substituído. Devolve (sistema, versão, arquivo).
pub fn instalar(origem: &Path) -> Resultado<Vec<(String, String, PathBuf)>> {
    let destino =
        pasta().ok_or_else(|| Erro::Destino("não achei uma pasta para guardar os .dat".into()))?;
    fs::create_dir_all(&destino)?;
    let (existentes, _) = instalados();
    let mut feitos = Vec::new();
    for (nome, texto) in ler_arquivo(origem)? {
        let d = montar(nome.clone(), &texto)?;
        // só o nome do arquivo, nunca um caminho vindo de dentro do zip
        let base = Path::new(&nome.replace('\\', "/"))
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty() && n != "." && n != "..")
            .ok_or_else(|| Erro::Destino(format!("nome de .dat inválido: {nome}")))?;
        let alvo = destino.join(&base);
        let temp = temporario::caminho_de(&alvo);
        if let Err(e) = temporario::criar(&temp).and_then(|mut f| f.write_all(texto.as_bytes())) {
            fs::remove_file(&temp).ok();
            return Err(e.into());
        }
        fs::rename(&temp, &alvo).inspect_err(|_| {
            fs::remove_file(&temp).ok();
        })?;
        for velho in existentes
            .iter()
            .filter(|e| !d.sistema.is_empty() && e.sistema == d.sistema && e.arquivo != base)
        {
            fs::remove_file(destino.join(&velho.arquivo)).ok();
        }
        feitos.push((d.sistema, d.versao, alvo));
    }
    Ok(feitos)
}

/// Leitor mínimo de .zip: só o necessário para os .zip do Redump (sem
/// criptografia, sem ZIP64, métodos "guardado" e "deflate"). Tudo que vem do
/// arquivo é conferido: posições dentro dele, tamanho descomprimido limitado
/// e CRC32 de cada entrada.
mod zip {
    use crate::erro::{Erro, Resultado};

    fn u16_em(b: &[u8], i: usize) -> Option<u16> {
        b.get(i..i + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
    }
    fn u32_em(b: &[u8], i: usize) -> Option<u32> {
        b.get(i..i + 4)
            .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
    }
    fn ruim(o_que: &str) -> Erro {
        Erro::Destino(format!(
            "o .zip está corrompido ou num formato não suportado ({o_que})"
        ))
    }

    pub fn ler(b: &[u8]) -> Resultado<Vec<(String, Vec<u8>)>> {
        // fim do diretório central: nos últimos 64 KiB + 22 bytes
        let inicio_busca = b.len().saturating_sub(65_557);
        let eocd = (inicio_busca..b.len().saturating_sub(21))
            .rev()
            .find(|&i| b[i..].starts_with(b"PK\x05\x06"))
            .ok_or_else(|| ruim("sem diretório central"))?;
        let entradas = u16_em(b, eocd + 10).ok_or_else(|| ruim("fim"))? as usize;
        let mut p = u32_em(b, eocd + 16).ok_or_else(|| ruim("fim"))? as usize;
        let mut saida = Vec::new();
        for _ in 0..entradas {
            if u32_em(b, p) != Some(0x0201_4b50) {
                return Err(ruim("entrada do diretório"));
            }
            let flags = u16_em(b, p + 8).ok_or_else(|| ruim("entrada"))?;
            let metodo = u16_em(b, p + 10).ok_or_else(|| ruim("entrada"))?;
            let crc = u32_em(b, p + 16).ok_or_else(|| ruim("entrada"))?;
            let comp = u32_em(b, p + 20).ok_or_else(|| ruim("entrada"))? as usize;
            let tam = u32_em(b, p + 24).ok_or_else(|| ruim("entrada"))? as usize;
            let n = u16_em(b, p + 28).ok_or_else(|| ruim("entrada"))? as usize;
            let extra = u16_em(b, p + 30).ok_or_else(|| ruim("entrada"))? as usize;
            let coment = u16_em(b, p + 32).ok_or_else(|| ruim("entrada"))? as usize;
            let local = u32_em(b, p + 42).ok_or_else(|| ruim("entrada"))? as usize;
            let nome = b.get(p + 46..p + 46 + n).ok_or_else(|| ruim("nome"))?;
            let nome = String::from_utf8_lossy(nome).into_owned();
            p += 46 + n + extra + coment;
            if nome.ends_with('/') {
                continue; // pasta
            }
            if flags & 1 != 0 {
                return Err(ruim("arquivo com senha"));
            }
            if tam > super::MAX_DAT || comp == u32::MAX as usize {
                return Err(ruim("entrada grande demais"));
            }
            if u32_em(b, local) != Some(0x0403_4b50) {
                return Err(ruim("cabeçalho local"));
            }
            let ln = u16_em(b, local + 26).ok_or_else(|| ruim("cabeçalho local"))? as usize;
            let le = u16_em(b, local + 28).ok_or_else(|| ruim("cabeçalho local"))? as usize;
            let ini = local + 30 + ln + le;
            let dados = b
                .get(ini..ini.checked_add(comp).ok_or_else(|| ruim("tamanho"))?)
                .ok_or_else(|| ruim("dados"))?;
            let conteudo = match metodo {
                0 => dados.to_vec(),
                8 => miniz_oxide::inflate::decompress_to_vec_with_limit(dados, tam)
                    .map_err(|_| ruim("deflate"))?,
                _ => return Err(ruim("método de compressão")),
            };
            if conteudo.len() != tam || crc32fast::hash(&conteudo) != crc {
                return Err(ruim("CRC32 não confere"));
            }
            saida.push((nome, conteudo));
        }
        Ok(saida)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const DAT: &str = r#"<?xml version="1.0"?>
<datafile><header><name>Microsoft - Xbox 360</name><version>2026-06-15</version></header>
<game name="Jogo (World)"><rom name="Jogo (World).iso" size="3" sha1="a9993e364706816aba3e25717850c26c9cd0d89d"/></game>
</datafile>"#;

    /// .zip com uma entrada comprimida em deflate, montado à mão.
    fn zip_com(nome: &str, conteudo: &[u8]) -> Vec<u8> {
        let comp = miniz_oxide::deflate::compress_to_vec(conteudo, 6);
        let crc = crc32fast::hash(conteudo);
        let mut z = Vec::new();
        let local = |z: &mut Vec<u8>| {
            z.extend(0x0403_4b50u32.to_le_bytes());
            z.extend([20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
            z.extend(crc.to_le_bytes());
            z.extend((comp.len() as u32).to_le_bytes());
            z.extend((conteudo.len() as u32).to_le_bytes());
            z.extend((nome.len() as u16).to_le_bytes());
            z.extend(0u16.to_le_bytes());
            z.extend(nome.as_bytes());
        };
        local(&mut z);
        z.extend(&comp);
        let cd = z.len();
        z.extend(0x0201_4b50u32.to_le_bytes());
        z.extend([20, 0, 20, 0, 0, 0, 8, 0, 0, 0, 0, 0]);
        z.extend(crc.to_le_bytes());
        z.extend((comp.len() as u32).to_le_bytes());
        z.extend((conteudo.len() as u32).to_le_bytes());
        z.extend((nome.len() as u16).to_le_bytes());
        z.extend([0u8; 12]);
        z.extend(0u32.to_le_bytes()); // cabeçalho local no byte 0
        z.extend(nome.as_bytes());
        let tam_cd = z.len() - cd;
        z.extend(0x0605_4b50u32.to_le_bytes());
        z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend((tam_cd as u32).to_le_bytes());
        z.extend((cd as u32).to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z
    }

    #[test]
    fn le_zip_e_cabecalho() {
        let z = zip_com("pasta/x.dat", DAT.as_bytes());
        let e = zip::ler(&z).unwrap();
        assert_eq!(e[0].0, "pasta/x.dat");
        assert_eq!(e[0].1, DAT.as_bytes());
        let d = montar("x.dat".into(), DAT).unwrap();
        assert_eq!(
            (d.sistema.as_str(), d.versao.as_str(), d.roms.len()),
            ("Microsoft - Xbox 360", "2026-06-15", 1)
        );
    }

    #[test]
    fn zip_corrompido_vira_erro() {
        let z = zip_com("x.dat", DAT.as_bytes());
        for i in 0..z.len() {
            let mut c = z.clone();
            c[i] ^= 0x5A;
            let _ = zip::ler(&c); // nunca pânico
        }
        let mut c = z.clone();
        let meio = 60;
        c[meio] ^= 0xFF; // dados comprimidos
        assert!(zip::ler(&c).is_err());
        assert!(zip::ler(b"PK\x03\x04lixo").is_err());
    }

    #[test]
    fn instalar_troca_a_versao_antiga_do_mesmo_sistema() {
        let d = std::env::temp_dir().join(format!("extract-xiso-pt-dats-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        // SAFETY: só este teste usa a variável
        unsafe { std::env::set_var("EXTRACT_XISO_PT_DATS", &d) };
        let velho = d.join("velho.zip");
        fs::write(&velho, zip_com("Xbox 360 (1).dat", DAT.as_bytes())).unwrap();
        instalar(&velho).unwrap();
        let novo = d.join("novo.dat");
        fs::write(&novo, DAT.replace("2026-06-15", "2026-07-01")).unwrap();
        instalar(&novo).unwrap();
        let (dats, avisos) = instalados();
        unsafe { std::env::remove_var("EXTRACT_XISO_PT_DATS") };
        fs::remove_dir_all(&d).ok();
        assert!(avisos.is_empty());
        assert_eq!(dats.len(), 1);
        assert_eq!(dats[0].versao, "2026-07-01");
    }
}
