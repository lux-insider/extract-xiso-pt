//! Testes do programa de verdade (o binário), para o que só aparece de fora:
//! códigos de saída, protocolo `--progresso-json`, saída padrão fechada.

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
