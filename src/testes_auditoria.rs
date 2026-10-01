//! Um teste por correção da auditoria (ver AUDITORIA.md): cada um monta o
//! cenário que causava o problema e confere que ele não acontece mais.

// Vários cenários (links simbólicos, sinais) só existem no Unix.
#![cfg_attr(not(unix), allow(unused_imports, dead_code))]

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
    assert_eq!(
        fs::read_dir(&fora).unwrap().count(),
        0,
        "nada fora do destino"
    );
    // o que a extração criou antes do erro foi desfeito; o link continua
    let sobrou: Vec<_> = fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(sobrou, ["pasta"]);
}

/// S-2: um `.parcial` que é link simbólico não faz a gravação sobrescrever
/// o arquivo para onde ele aponta, nem vira um link no resultado.
#[cfg(unix)]
#[test]
fn s2_parcial_que_e_link_nao_sobrescreve_o_alvo() {
    use std::os::unix::fs::symlink;
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let alvo = t.0.join("alvo.txt");
    fs::write(&alvo, b"importante").unwrap();

    // extrair
    let d = t.0.join("destino");
    fs::create_dir(&d).unwrap();
    symlink(&alvo, d.join("a.bin.extract-xiso-pt.parcial")).unwrap();
    extrair_em(&iso, &d, &sobrescrever()).unwrap();
    assert_eq!(fs::read(&alvo).unwrap(), b"importante");
    let m = fs::symlink_metadata(d.join("a.bin")).unwrap();
    assert!(
        m.is_file(),
        "a.bin extraído tem que ser um arquivo, não um link"
    );
    assert_eq!(fs::read(d.join("a.bin")).unwrap(), vec![0xAB; 3000]);
    assert!(!d.join("a.bin.extract-xiso-pt.parcial").exists());

    // criar
    let j = t.0.join("jogo");
    fs::create_dir(&j).unwrap();
    fs::write(j.join("default.xex"), b"XEX2").unwrap();
    let saida = t.0.join("jogo.iso");
    symlink(&alvo, t.0.join("jogo.iso.extract-xiso-pt.parcial")).unwrap();
    crate::testes::criar_de(&j, &saida).unwrap();
    assert_eq!(fs::read(&alvo).unwrap(), b"importante");
    assert!(fs::symlink_metadata(&saida).unwrap().is_file());
}

/// Imagem em que cada nível tem dois diretórios apontando para a mesma
/// tabela seguinte, cada uma declarando 16 MB: sem limite, as visitas
/// dobram a cada nível e cada uma relê 16 MB.
fn imagem_de_tabelas_compartilhadas(niveis: u32) -> crate::testes::Construtor {
    use crate::testes::{Construtor, no};
    const S: usize = 2048;
    const TAM: u32 = 16 * 1024 * 1024;
    let mut c = Construtor::novo(33 + niveis as usize + 1 + TAM as usize / S);
    c.raiz(33, S as u32);
    for i in 0..niveis {
        let mut t = vec![0xFFu8; S];
        let a = no(0, 4, 34 + i, TAM, crate::arvore::ATTR_DIRETORIO, b"a");
        let b = no(0, 0, 34 + i, TAM, crate::arvore::ATTR_DIRETORIO, b"b");
        t[..a.len()].copy_from_slice(&a);
        t[16..16 + b.len()].copy_from_slice(&b);
        c.por(33 + i, &t);
    }
    c.por(33 + niveis, &[0xFF; S]); // diretório vazio no fundo
    c
}

/// B-3: a imagem de 16 MB que travava o `info` por tempo indeterminado
/// agora é recusada em segundos, com erro explicado.
#[test]
fn b3_tabelas_compartilhadas_nao_travam_a_leitura() {
    let t = Temp::nova();
    let iso = imagem_de_tabelas_compartilhadas(40).gravar(&t.0);
    let inicio = std::time::Instant::now();
    let r = crate::testes::ler(&iso);
    assert!(matches!(r, Err(Erro::Imagem(_))), "{r:?}");
    assert!(inicio.elapsed().as_secs() < 30, "{:?}", inicio.elapsed());

    // poucos níveis: as mesmas tabelas visitadas algumas vezes continuam
    // valendo (a árvore lida é a mesma de antes)
    let iso = imagem_de_tabelas_compartilhadas(3).gravar(&t.0);
    let raiz = crate::testes::ler(&iso).unwrap();
    assert_eq!(crate::arvore::totais(&raiz).diretorios, 2 + 4 + 8);
}
