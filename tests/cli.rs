//! Testes do programa de verdade (o binário), para o que só aparece de fora:
//! códigos de saída, protocolo `--progresso-json`, saída padrão fechada.

// Alguns cenários (sinais) só existem no Unix.
#![cfg_attr(not(unix), allow(dead_code))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_extract-xiso-pt");

/// Pasta temporária própria de cada teste, apagada no fim.
struct Temp(PathBuf);

impl Temp {
    fn nova(nome: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("extract-xiso-pt-cli-{nome}-{}", std::process::id()));
        fs::remove_dir_all(&p).ok();
        fs::create_dir_all(&p).unwrap();
        Temp(p)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn rodar(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// Roda com a saída padrão ligada a um pipe cuja ponta de leitura já foi
/// fechada: toda escrita nela falha (EPIPE).
fn rodar_com_saida_fechada(args: &[&std::ffi::OsStr]) -> Output {
    let (leitor, escritor) = std::io::pipe().unwrap();
    drop(leitor);
    Command::new(BIN)
        .args(args)
        .stdout(escritor)
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

/// Uma pasta de jogo com muitos arquivos (a listagem passa de 64 KiB, o
/// tamanho do buffer de um pipe) e a imagem criada a partir dela.
fn jogo_com_muitos_arquivos(t: &Temp) -> (PathBuf, PathBuf) {
    let jogo = t.0.join("jogo");
    fs::create_dir_all(jogo.join("media")).unwrap();
    fs::write(jogo.join("default.xex"), b"XEX2").unwrap();
    for i in 0..3000 {
        fs::write(
            jogo.join(format!("media/arquivo_{i:04}.txt")),
            format!("{i}\n"),
        )
        .unwrap();
    }
    let iso = t.0.join("jogo.iso");
    let o = rodar(&[
        "criar".as_ref(),
        jogo.as_os_str(),
        "-s".as_ref(),
        iso.as_os_str(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    (jogo, iso)
}

fn sem_parcial(p: &Path) -> bool {
    fs::read_dir(p).unwrap().all(|e| {
        let e = e.unwrap();
        let ok = !e.file_name().to_string_lossy().ends_with(".parcial");
        ok && (!e.path().is_dir() || sem_parcial(&e.path()))
    })
}

/// S-4: saída padrão fechada não vira pânico nem aborta a operação no meio.
#[test]
fn s4_saida_padrao_fechada_nao_aborta() {
    let t = Temp::nova("s4");
    let (_, iso) = jogo_com_muitos_arquivos(&t);

    let o = rodar_com_saida_fechada(&["listar".as_ref(), iso.as_os_str()]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert!(o.status.success(), "{:?} {err}", o.status);

    // o caso grave: o programa que lia o progresso fechou o pipe
    let d = t.0.join("saida");
    let o = rodar_com_saida_fechada(&[
        "extrair".as_ref(),
        iso.as_os_str(),
        "-d".as_ref(),
        d.as_os_str(),
        "--progresso-json".as_ref(),
    ]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!err.contains("panicked"), "{err}");
    assert!(o.status.success(), "{:?} {err}", o.status);
    assert_eq!(fs::read_dir(d.join("media")).unwrap().count(), 3000);
    assert!(sem_parcial(&d));
}

/// XISO mínima montada à mão: um arquivo `grande.bin` de `tamanho` bytes
/// (esparso, para não ocupar disco na origem).
fn imagem_com_um_arquivo_grande(t: &Temp, tamanho: u32) -> PathBuf {
    use std::io::{Seek, SeekFrom, Write};
    const S: u64 = 2048;
    let mut descritor = vec![0u8; S as usize];
    descritor[0..20].copy_from_slice(b"MICROSOFT*XBOX*MEDIA");
    descritor[20..24].copy_from_slice(&33u32.to_le_bytes());
    descritor[24..28].copy_from_slice(&(S as u32).to_le_bytes());
    descritor[0x7EC..0x800].copy_from_slice(b"MICROSOFT*XBOX*MEDIA");
    let mut tabela = vec![0xFFu8; S as usize];
    let nome = b"grande.bin";
    tabela[0..4].copy_from_slice(&[0, 0, 0, 0]);
    tabela[4..8].copy_from_slice(&34u32.to_le_bytes());
    tabela[8..12].copy_from_slice(&tamanho.to_le_bytes());
    tabela[12] = 0x20;
    tabela[13] = nome.len() as u8;
    tabela[14..14 + nome.len()].copy_from_slice(nome);

    let iso = t.0.join("grande.iso");
    let mut f = fs::File::create(&iso).unwrap();
    f.set_len(34 * S + tamanho as u64).unwrap();
    f.seek(SeekFrom::Start(32 * S)).unwrap();
    f.write_all(&descritor).unwrap();
    f.write_all(&tabela).unwrap();
    iso
}

/// S-5: fechar o terminal (SIGHUP) no meio da extração cancela de forma
/// limpa: código 130 e nada do que foi criado fica para trás.
#[cfg(unix)]
#[test]
fn s5_sighup_no_meio_da_extracao_limpa_tudo() {
    use std::io::{BufRead, BufReader};
    let t = Temp::nova("s5");
    let iso = imagem_com_um_arquivo_grande(&t, 1 << 30);
    let d = t.0.join("saida");
    let mut filho = Command::new(BIN)
        .args([
            "extrair".as_ref(),
            iso.as_os_str(),
            "-d".as_ref(),
            d.as_os_str(),
        ])
        .arg("--progresso-json")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut linhas = BufReader::new(filho.stdout.take().unwrap()).lines();
    // o primeiro evento de progresso: o .parcial já existe
    for l in linhas.by_ref() {
        if l.unwrap().contains("\"evento\":\"progresso\"") {
            break;
        }
    }
    let ok = Command::new("kill")
        .args(["-HUP", &filho.id().to_string()])
        .status()
        .unwrap();
    assert!(ok.success());
    let ultima = linhas.map(|l| l.unwrap()).last().unwrap_or_default();
    let status = filho.wait().unwrap();
    assert_eq!(status.code(), Some(130), "{status:?}");
    assert!(ultima.contains("\"evento\":\"erro\""), "{ultima}");
    assert!(
        !d.exists(),
        "a pasta criada pela extração deveria ter sido apagada"
    );
}
