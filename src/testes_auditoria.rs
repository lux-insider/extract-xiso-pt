//! Um teste por correção da auditoria (ver AUDITORIA.md): cada um monta o
//! cenário que causava o problema e confere que ele não acontece mais.

use std::fs;

use crate::erro::Erro;
use crate::extrair::Opcoes;
use crate::testes::{EXTRACAO, Temp, extrair_em, imagem_valida, opcoes};

fn sobrescrever() -> Opcoes {
    Opcoes {
        sobrescrever: true,
        ..opcoes()
    }
}

/// S-1: uma pasta do destino que é link simbólico não leva a extração para
/// fora dele.
#[cfg(unix)]
#[test]
fn s1_link_no_destino_nao_leva_a_extracao_para_fora() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let fora = t.0.join("fora");
    fs::create_dir(&fora).unwrap();
    let d = t.0.join("destino");
    fs::create_dir(&d).unwrap();
    // "pasta" é um diretório da imagem
    std::os::unix::fs::symlink(&fora, d.join("pasta")).unwrap();

    let r = extrair_em(&iso, &d, &sobrescrever());
    assert!(matches!(r, Err(Erro::Destino(_))), "{r:?}");
    assert_eq!(fs::read_dir(&fora).unwrap().count(), 0, "nada fora do destino");
    // o que a extração criou antes do erro foi desfeito; o link continua
    let sobrou: Vec<_> = fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(sobrou, ["pasta"]);
}
