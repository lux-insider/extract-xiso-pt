//! Saída validada, byte a byte.
//!
//! A ferramenta já foi conferida em jogos reais no Xbox 360: o que ela grava
//! hoje para uma entrada válida é a referência. Estes testes guardam o SHA-1
//! do que cada comando produz (arquivos extraídos, imagens criadas e
//! reescritas, JSON de `listar` e de `verificar`) e falham se um único byte
//! mudar. Os valores foram gravados com a versão 0.2.2, antes de qualquer
//! correção da auditoria.
//!
//! A única parte de uma imagem nova que muda de uma execução para outra é a
//! data de criação no descritor de volume (8 bytes de FILETIME); ela é
//! zerada antes do hash.

use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

use sha1::{Digest, Sha1};

use crate::arvore;
use crate::criar;
use crate::imagem::{Imagem, Layout};
use crate::progresso::Progresso;
use crate::testes::{
    Construtor, EXTRACAO, Temp, criar_com, criar_de, extrair_em, imagem_valida, opcoes,
    pasta_de_jogo, tabela, xbe_falso,
};
use crate::verificar;

const S: usize = 2048;
const ARQ: u8 = 0x20;
const DIR: u8 = arvore::ATTR_DIRETORIO;
/// FILETIME de criação dentro do descritor (setor 32, bytes 28..36).
const DATA_NO_DESCRITOR: std::ops::Range<usize> = 32 * S + 28..32 * S + 36;

fn sha1_hex(b: &[u8]) -> String {
    Sha1::digest(b).iter().map(|x| format!("{x:02x}")).collect()
}

/// Lista de tudo o que há numa pasta: tipo, caminho, tamanho e SHA-1 de cada
/// arquivo, em ordem de bytes do caminho.
fn manifesto(raiz: &Path) -> String {
    fn rec(raiz: &Path, pasta: &Path, linhas: &mut Vec<String>) {
        for e in fs::read_dir(pasta).unwrap() {
            let p = e.unwrap().path();
            let rel = p.strip_prefix(raiz).unwrap().to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                linhas.push(format!("d {rel}"));
                rec(raiz, &p, linhas);
            } else {
                let b = fs::read(&p).unwrap();
                linhas.push(format!("f {rel} {} {}", b.len(), sha1_hex(&b)));
            }
        }
    }
    let mut linhas = Vec::new();
    rec(raiz, raiz, &mut linhas);
    linhas.sort();
    linhas.join("\n")
}

/// SHA-1 de uma imagem com a data de criação zerada, e o tamanho dela.
fn hash_imagem(iso: &Path) -> (u64, String) {
    let mut b = fs::read(iso).unwrap();
    b[DATA_NO_DESCRITOR].fill(0);
    (b.len() as u64, sha1_hex(&b))
}

#[track_caller]
fn conferir(o_que: &str, obtido: &str, esperado: &str, detalhe: &str) {
    assert_eq!(
        obtido, esperado,
        "a saída de {o_que} mudou (deveria ser idêntica byte a byte)\n{detalhe}"
    );
}

fn reescrever(origem: &Path, saida: &Path, sem_atualizacao: bool, liberar_midia: bool) {
    let o = criar::Opcoes {
        sobrescrever: false,
        sem_atualizacao,
        liberar_midia,
    };
    let mut img = Imagem::abrir(origem).unwrap();
    let raiz = arvore::ler(&mut img).unwrap();
    let mut fonte = criar::Fonte::Imagem(&mut img, raiz);
    let prep = criar::preparar(&mut fonte, &o).unwrap();
    let p = Progresso::novo("", "", prep.bytes, true);
    criar::gravar(fonte, prep, saida, &o, &p).unwrap();
}

/// Bytes pseudoaleatórios reproduzíveis (xorshift).
fn bytes(semente: u64, n: usize) -> Vec<u8> {
    let mut x = semente | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

/// Disco de Xbox 360 no layout XGD3: partição de vídeo com lixo antes, nomes
/// em UTF-8 e em Latin-1, `$SystemUpdate`, tabela de vários setores,
/// subpastas, pasta vazia dos dois jeitos, arquivo vazio e dois arquivos que
/// apontam para os mesmos setores (como em discos que deduplicam conteúdo).
fn disco_xgd3(pasta: &Path) -> std::path::PathBuf {
    let mut prox = 200u32; // conteúdo dos arquivos a partir daqui
    let mut c = Construtor::novo(200);
    let mut dados = |c: &mut Construtor, semente: u64, n: usize| {
        let setor = prox;
        c.por(setor, &bytes(semente, n));
        prox += (n.div_ceil(S) as u32).max(1);
        (setor, n as u32)
    };

    let (su_s, su_t) = dados(&mut c, 1, 9000);
    let upd = tabela(&[(b"su20076000_00000000", su_s, su_t, ARQ)]);

    let (fundo_s, fundo_t) = dados(&mut c, 2, 70_000);
    let sub = tabela(&[(b"fundo.bin", fundo_s, fundo_t, ARQ)]);

    let nomes: Vec<String> = (0..150).map(|i| format!("som_{i:03}.wav")).collect();
    let mut media: Vec<(&[u8], u32, u32, u8)> = Vec::new();
    for (i, n) in nomes.iter().enumerate() {
        let (s, t) = dados(&mut c, 100 + i as u64, 37 * i + 1);
        media.push((n.as_bytes(), s, t, ARQ));
    }
    media.push((&b"sub"[..], 40, sub.len() as u32, DIR));
    let media = tabela(&media);
    assert!(
        media.len() > 2 * S,
        "a tabela de media deve ter vários setores"
    );

    let (acao_s, acao_t) = dados(&mut c, 3, 10);
    let (latin_s, latin_t) = dados(&mut c, 4, 3);
    let (xex_s, xex_t) = dados(&mut c, 5, 5000);
    let raiz = tabela(&[
        (b"$SystemUpdate", 34, upd.len() as u32, DIR),
        ("Ação.txt".as_bytes(), acao_s, acao_t, ARQ),
        (&[b'A', 0xE9, b'.', b'b', b'i', b'n'], latin_s, latin_t, ARQ),
        (b"copia.xex", xex_s, xex_t, ARQ),
        (b"default.xex", xex_s, xex_t, ARQ),
        (b"media", 35, media.len() as u32, DIR),
        (b"vazia", 0, 0, DIR),
        (b"vazia2", 41, S as u32, DIR),
        (b"zero.bin", 0, 0, ARQ),
    ]);
    c.raiz(33, raiz.len() as u32)
        .por(33, &raiz)
        .por(34, &upd)
        .por(35, &media)
        .por(40, &sub)
        .por(41, &[0xFF; S]);
    assert!(35 + media.len().div_ceil(S) as u32 <= 40);

    // partição de vídeo: lixo no começo, o resto esparso
    let deslocamento = Layout::Xgd3.deslocamento();
    let iso = pasta.join("disco.iso");
    let mut f = File::create(&iso).unwrap();
    f.write_all(&bytes(9, 64 * 1024)).unwrap();
    f.set_len(deslocamento).unwrap();
    f.seek(SeekFrom::Start(deslocamento)).unwrap();
    f.write_all(&c.bytes).unwrap();
    iso
}

#[test]
fn extrair_imagem_pequena_identico() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let d = t.0.join("saida");
    extrair_em(&iso, &d, &opcoes()).unwrap();
    let m = manifesto(&d);
    conferir(
        "extrair (imagem pequena)",
        &sha1_hex(m.as_bytes()),
        "0e1aa96c4a5dd5d1eacfe4de99f28a590af83e36",
        &m,
    );
}

#[test]
fn criar_reescrever_e_extrair_pasta_de_jogo_identico() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let jogo = t.0.join("jogo");
    pasta_de_jogo(&jogo);

    let iso = t.0.join("jogo.iso");
    criar_de(&jogo, &iso).unwrap();
    let (tam, h) = hash_imagem(&iso);
    conferir(
        "criar",
        &format!("{tam} {h}"),
        "4587520 ef1cd9baed362e5f76c2f3b192855386c319bb67",
        "",
    );

    let d = t.0.join("saida");
    extrair_em(&iso, &d, &opcoes()).unwrap();
    let m = manifesto(&d);
    conferir(
        "extrair (criada)",
        &sha1_hex(m.as_bytes()),
        "6c4a8dee5e5aa4245e6816825f7ec311a32ee053",
        &m,
    );

    // Difere da imagem criada em 10 bytes: o nome "Ação.wav" (UTF-8 na
    // origem) sai em Latin-1 na reescrita. É o comportamento da 0.2.2,
    // registrado aqui de propósito; ver AUDITORIA.md, item A-6.
    let iso2 = t.0.join("reescrita.iso");
    reescrever(&iso, &iso2, false, false);
    let (tam, h) = hash_imagem(&iso2);
    conferir(
        "reescrever (XISO)",
        &format!("{tam} {h}"),
        "4587520 441bf9dcea205e0d363977e75233537a0709e7d0",
        "",
    );

    let mut img = Imagem::abrir(&iso).unwrap();
    let raiz = arvore::ler(&mut img).unwrap();
    let json = serde_json::to_string(&raiz).unwrap();
    conferir(
        "listar --json (criada)",
        &sha1_hex(json.as_bytes()),
        "6bca568c933d63e44c32f528826f959d9d8e4e5f",
        "",
    );
}

#[test]
fn disco_xgd3_extrair_reescrever_listar_verificar_identico() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = disco_xgd3(&t.0);

    let mut img = Imagem::abrir(&iso).unwrap();
    assert_eq!(img.layout, Layout::Xgd3);
    let raiz = arvore::ler(&mut img).unwrap();
    drop(img);
    let json = serde_json::to_string(&raiz).unwrap();
    conferir(
        "listar --json (XGD3)",
        &sha1_hex(json.as_bytes()),
        "9efd4df775a493f1bd37d4a5e7d3e60109bdba30",
        &json,
    );

    let d = t.0.join("saida");
    extrair_em(&iso, &d, &opcoes()).unwrap();
    let m = manifesto(&d);
    conferir(
        "extrair (XGD3)",
        &sha1_hex(m.as_bytes()),
        "05809aaddd3c03cd6e8f5ac2d71b9406e2366f6a",
        &m,
    );

    let d = t.0.join("saida_sem_atualizacao");
    extrair_em(
        &iso,
        &d,
        &crate::extrair::Opcoes {
            sem_atualizacao: true,
            ..opcoes()
        },
    )
    .unwrap();
    let m = manifesto(&d);
    conferir(
        "extrair -s (XGD3)",
        &sha1_hex(m.as_bytes()),
        "9cd2d8b1baabe4ce064417eb1b60503115ea87db",
        &m,
    );

    let x = t.0.join("enxuta.iso");
    reescrever(&iso, &x, false, false);
    let (tam, h) = hash_imagem(&x);
    conferir(
        "reescrever (XGD3)",
        &format!("{tam} {h}"),
        "786432 1b19cdc972dc2cfacf9102c9df8f4335f2f792f5",
        "",
    );

    let x = t.0.join("enxuta_sem_atualizacao.iso");
    reescrever(&iso, &x, true, false);
    let (tam, h) = hash_imagem(&x);
    conferir(
        "reescrever -u (XGD3)",
        &format!("{tam} {h}"),
        "786432 cabdeee060a384672c6eb681050fdcf19f0eb587",
        "",
    );

    let rel = verificar::verificar(&iso, &[], |total| {
        Progresso::novo_quieto("", "", total, false, true)
    })
    .unwrap();
    let json = serde_json::to_string(&rel).unwrap();
    conferir(
        "verificar --json (XGD3)",
        &json,
        r#"{"layout":"XGD3","disco_completo":false,"arquivos":157,"diretorios":5,"avisos":["copia.xex e default.xex usam os mesmos setores"],"hashes":{"tamanho":35160064,"crc32":"2cf97f69","md5":"cf1890f7f82ec0d7f9ead938889960da","sha1":"fc79441bd561d9986ef201ad587d39534879a863"},"dat":null,"dats_usados":[]}"#,
        "",
    );
}

#[test]
fn liberar_midia_identico() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let j = t.0.join("jogo");
    fs::create_dir_all(j.join("dados")).unwrap();
    fs::write(j.join("default.xbe"), xbe_falso(0x0000_0002)).unwrap();
    fs::write(j.join("dados/mapa.bin"), bytes(7, 12_345)).unwrap();

    let iso = t.0.join("liberada.iso");
    criar_com(&j, &iso, true).unwrap();
    let (tam, h) = hash_imagem(&iso);
    conferir(
        "criar --liberar-midia",
        &format!("{tam} {h}"),
        "131072 aa131354f9e6254ab01acd03f273f77d73cec899",
        "",
    );

    let normal = t.0.join("normal.iso");
    criar_com(&j, &normal, false).unwrap();
    let x = t.0.join("reescrita.iso");
    reescrever(&normal, &x, false, true);
    let (tam, h) = hash_imagem(&x);
    conferir(
        "reescrever --liberar-midia",
        &format!("{tam} {h}"),
        "131072 aa131354f9e6254ab01acd03f273f77d73cec899",
        "",
    );
}
