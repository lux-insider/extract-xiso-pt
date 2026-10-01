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

/// B-4: Ctrl+C/SIGTERM valem também durante a leitura da árvore da imagem
/// e da pasta de origem do `criar`.
#[test]
fn b4_cancelamento_durante_a_leitura() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let j = t.0.join("jogo");
    fs::create_dir(&j).unwrap();
    fs::write(j.join("default.xex"), b"XEX2").unwrap();

    crate::sistema::marcar_cancelamento();
    let arvore = crate::testes::ler(&iso);
    let o = crate::criar::Opcoes {
        sobrescrever: false,
        sem_atualizacao: false,
        liberar_midia: false,
    };
    let pasta = crate::criar::preparar(&mut crate::criar::Fonte::Pasta(&j), &o);
    crate::sistema::limpar_cancelamento();
    assert!(matches!(arvore, Err(Erro::Cancelado)), "{arvore:?}");
    assert!(matches!(pasta, Err(Erro::Cancelado)));
    // sem o pedido, as duas leituras funcionam
    assert!(crate::testes::ler(&iso).is_ok());
    assert!(crate::criar::preparar(&mut crate::criar::Fonte::Pasta(&j), &o).is_ok());
}

/// S-6: com --sobrescrever, uma falha no meio não apaga o arquivo que
/// substituiu um do usuário (a versão antiga já foi trocada; apagar a nova
/// deixaria o usuário sem nenhuma das duas).
#[test]
fn s6_falha_com_sobrescrever_nao_apaga_arquivo_substituido() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let d = t.0.join("destino");
    fs::create_dir_all(d.join("pasta")).unwrap();
    fs::write(d.join("a.bin"), b"versao antiga").unwrap();
    // uma pasta no lugar do temporário de pasta/dentro.txt faz a extração
    // falhar nele, depois de a.bin (na ordem da árvore e na do disco)
    let bloqueio = d.join("pasta/dentro.txt.extract-xiso-pt.parcial");
    fs::create_dir(&bloqueio).unwrap();

    let r = extrair_em(&iso, &d, &sobrescrever());
    assert!(r.is_err());
    assert_eq!(fs::read(d.join("a.bin")).unwrap(), vec![0xAB; 3000]);
    assert!(bloqueio.is_dir());
    assert!(!d.join("pasta/dentro.txt").exists());
    let mut sobrou: Vec<_> = fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    sobrou.sort();
    assert_eq!(sobrou, ["a.bin", "pasta"]);
}

/// P-6 e S-7: os arquivos são extraídos na ordem do disco; um arquivo da
/// imagem chamado `x.extract-xiso-pt.parcial`, extraído antes de `x`, não
/// é atropelado pelo temporário de `x`.
#[test]
fn p6_s7_ordem_do_disco_e_temporario_que_colide_com_arquivo() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    // O .parcial vem antes de x no disco (então é extraído antes na ordem
    // do disco) e também na tabela, fora da ordem alfabética (então também
    // seria na ordem da árvore, a da versão 0.2.2).
    let tab = crate::testes::tabela(&[
        (b"x.extract-xiso-pt.parcial", 35, 6, 0x20),
        (b"x", 36, 5, 0x20),
        (b"y", 37, 2, 0x20),
    ]);
    let mut c = crate::testes::Construtor::novo(38);
    c.raiz(33, tab.len() as u32)
        .por(33, &tab)
        .por(35, b"sou eu")
        .por(36, b"xxxxx")
        .por(37, b"yy");
    let iso = c.gravar(&t.0);
    let d = t.0.join("saida");
    extrair_em(&iso, &d, &opcoes()).unwrap();
    assert_eq!(fs::read(d.join("x")).unwrap(), b"xxxxx");
    assert_eq!(
        fs::read(d.join("x.extract-xiso-pt.parcial")).unwrap(),
        b"sou eu"
    );
    assert_eq!(fs::read(d.join("y")).unwrap(), b"yy");
    assert_eq!(fs::read_dir(&d).unwrap().count(), 3);
}

/// P-6: a ordem é a do disco, não a da árvore. Na árvore: a, b, c; no
/// disco: c, a, b. Com o temporário de `a` bloqueado, a extração falha em
/// `a`; com --sobrescrever, o que já foi extraído fica (S-6): só `c`, que
/// vem antes de `a` no disco.
#[test]
fn p6_extracao_segue_a_ordem_dos_setores() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let tab = crate::testes::tabela(&[
        (b"a", 36, 3, 0x20),
        (b"b", 37, 3, 0x20),
        (b"c", 35, 3, 0x20),
    ]);
    let mut c = crate::testes::Construtor::novo(38);
    c.raiz(33, tab.len() as u32)
        .por(33, &tab)
        .por(35, b"CCC")
        .por(36, b"AAA")
        .por(37, b"BBB");
    let iso = c.gravar(&t.0);
    let d = t.0.join("saida");
    fs::create_dir(&d).unwrap();
    for n in ["a", "b", "c"] {
        fs::write(d.join(n), b"velho").unwrap();
    }
    fs::create_dir(d.join("a.extract-xiso-pt.parcial")).unwrap();
    assert!(extrair_em(&iso, &d, &sobrescrever()).is_err());
    assert_eq!(fs::read(d.join("c")).unwrap(), b"CCC");
    assert_eq!(fs::read(d.join("a")).unwrap(), b"velho");
    assert_eq!(fs::read(d.join("b")).unwrap(), b"velho");
}

/// S-8: `reescrever` cujo temporário seria a própria imagem de origem é
/// recusado antes de tocar nela.
#[test]
fn s8_temporario_igual_a_origem_e_recusado() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let origem = t.0.join("a.iso.extract-xiso-pt.parcial");
    fs::rename(&iso, &origem).unwrap();
    let antes = fs::read(&origem).unwrap();

    let o = crate::criar::Opcoes {
        sobrescrever: false,
        sem_atualizacao: false,
        liberar_midia: false,
    };
    let mut img = crate::imagem::Imagem::abrir(&origem).unwrap();
    let raiz = crate::arvore::ler(&mut img).unwrap();
    let mut fonte = crate::criar::Fonte::Imagem(&mut img, raiz);
    let prep = crate::criar::preparar(&mut fonte, &o).unwrap();
    let p = crate::progresso::Progresso::novo("", "", prep.bytes, true);
    let r = crate::criar::gravar(fonte, prep, &t.0.join("a.iso"), &o, &p);
    assert!(matches!(r, Err(Erro::Destino(_))), "{:?}", r.err());
    assert_eq!(fs::read(&origem).unwrap(), antes);
    assert!(!t.0.join("a.iso").exists());
}

/// V-1: um arquivo que contém dois menores gera aviso para os dois (antes,
/// só o vizinho na ordem dos setores era comparado).
#[test]
fn v1_sobreposicao_nao_vizinha_e_avisada() {
    let t = Temp::nova();
    let tab = crate::testes::tabela(&[
        (b"b", 50, 10, 0x20),
        (b"c", 60, 10, 0x20),
        (b"grande", 40, 100 * 2048, 0x20),
    ]);
    let mut c = crate::testes::Construtor::novo(140);
    c.raiz(33, tab.len() as u32).por(33, &tab);
    let iso = c.gravar(&t.0);
    let mut img = crate::imagem::Imagem::abrir(&iso).unwrap();
    let (_, avisos) = crate::verificar::estrutura(&mut img).unwrap();
    assert_eq!(
        avisos,
        [
            "grande e b usam os mesmos setores",
            "grande e c usam os mesmos setores"
        ]
    );
}
