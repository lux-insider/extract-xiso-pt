//! Testes com imagens sintéticas montadas byte a byte: estruturas válidas
//! que o leitor tem que aceitar e estruturas maliciosas ou corrompidas que
//! ele tem que recusar com erro — nunca pânico, laço infinito ou arquivo
//! gravado fora do destino.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::arvore::{self, ATTR_DIRETORIO, Entrada};
use crate::erro::Erro;
use crate::extrair::{self, Opcoes};
use crate::imagem::{ASSINATURA, Imagem};
use crate::progresso::Progresso;
use crate::sistema;

const S: usize = 2048;
const ARQ: u8 = 0x20;
const DIR: u8 = ATTR_DIRETORIO;

/// O cancelamento é global: os testes que extraem rodam um de cada vez.
static EXTRACAO: Mutex<()> = Mutex::new(());

/// Pasta temporária própria de cada teste, apagada no fim.
struct Temp(PathBuf);

impl Temp {
    fn nova() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "extract-xiso-pt-teste-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Temp(p)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// Um nó de tabela: (esquerda, direita) em palavras de 4 bytes.
fn no(esq: u16, dir: u16, setor: u32, tamanho: u32, attr: u8, nome: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend(esq.to_le_bytes());
    v.extend(dir.to_le_bytes());
    v.extend(setor.to_le_bytes());
    v.extend(tamanho.to_le_bytes());
    v.push(attr);
    v.push(nome.len() as u8);
    v.extend(nome);
    while v.len() % 4 != 0 {
        v.push(0xFF);
    }
    v
}

/// Tabela como lista encadeada pela direita (a árvore mais degenerada que
/// existe), sem nó atravessando setor. Entradas: (nome, setor, tamanho, attr).
fn tabela(entradas: &[(&[u8], u32, u32, u8)]) -> Vec<u8> {
    // primeiro as posições, respeitando o limite de setor
    let mut pos = Vec::new();
    let mut p = 0usize;
    for (nome, ..) in entradas {
        let t = (14 + nome.len()).div_ceil(4) * 4;
        if p % S + t > S {
            p = p.div_ceil(S) * S;
        }
        pos.push(p);
        p += t;
    }
    let mut v = vec![0xFFu8; p.div_ceil(S).max(1) * S];
    for (i, &(nome, setor, tamanho, attr)) in entradas.iter().enumerate() {
        let dir = pos.get(i + 1).map_or(0, |&q| (q / 4) as u16);
        let n = no(0, dir, setor, tamanho, attr, nome);
        v[pos[i]..pos[i] + n.len()].copy_from_slice(&n);
    }
    v
}

/// Imagem XISO em memória, com o descritor no setor 32.
struct Construtor {
    bytes: Vec<u8>,
}

impl Construtor {
    fn novo(setores: usize) -> Self {
        Self {
            bytes: vec![0; setores * S],
        }
    }

    fn por(&mut self, setor: u32, dados: &[u8]) -> &mut Self {
        let i = setor as usize * S;
        if self.bytes.len() < i + dados.len() {
            self.bytes.resize((i + dados.len()).div_ceil(S) * S, 0);
        }
        self.bytes[i..i + dados.len()].copy_from_slice(dados);
        self
    }

    fn raiz(&mut self, setor: u32, tamanho: u32) -> &mut Self {
        let mut d = vec![0u8; S];
        d[0..20].copy_from_slice(ASSINATURA);
        d[20..24].copy_from_slice(&setor.to_le_bytes());
        d[24..28].copy_from_slice(&tamanho.to_le_bytes());
        d[0x7EC..0x800].copy_from_slice(ASSINATURA);
        self.por(32, &d)
    }

    fn gravar(&self, pasta: &Path) -> PathBuf {
        let p = pasta.join("teste.iso");
        fs::write(&p, &self.bytes).unwrap();
        p
    }
}

fn ler(iso: &Path) -> Result<Vec<Entrada>, Erro> {
    let mut img = Imagem::abrir(iso)?;
    arvore::ler(&mut img)
}

/// Imagem pequena e válida: um arquivo, um diretório com um arquivo dentro,
/// um diretório vazio e um arquivo vazio.
fn imagem_valida() -> Construtor {
    let mut c = Construtor::novo(40);
    let sub = tabela(&[(b"dentro.txt", 38, 5, ARQ)]);
    let raiz = tabela(&[
        (b"a.bin", 36, 3000, ARQ),
        (b"pasta", 34, sub.len() as u32, DIR),
        (b"vazia", 35, S as u32, DIR), // um setor só de 0xFF
        (b"zero", 0, 0, ARQ),
    ]);
    c.raiz(33, raiz.len() as u32)
        .por(33, &raiz)
        .por(34, &sub)
        .por(35, &[0xFF; S]);
    c.por(36, &vec![0xAB; 3000]).por(38, b"oi!\n\n");
    c
}

#[test]
fn le_imagem_valida_em_ordem() {
    let t = Temp::nova();
    let raiz = ler(&imagem_valida().gravar(&t.0)).unwrap();
    let nomes: Vec<_> = raiz.iter().map(|e| e.nome.as_str()).collect();
    assert_eq!(nomes, ["a.bin", "pasta", "vazia", "zero"]);
    assert_eq!(raiz[1].filhos[0].nome, "dentro.txt");
    assert!(raiz[2].filhos.is_empty());
    let tot = arvore::totais(&raiz);
    assert_eq!((tot.arquivos, tot.diretorios, tot.bytes), (3, 2, 3005));
}

#[test]
fn arvore_balanceada_sai_em_ordem() {
    // raiz "m", esquerda "c", direita "x"
    let t = Temp::nova();
    let mut tab = no(0, 0, 0, 0, 0, b"");
    let c_ = no(0, 0, 0, 0, ARQ, b"c");
    let x = no(0, 0, 0, 0, ARQ, b"x");
    let (pc, px) = (tab.len() + 16, tab.len() + 32); // folga para o nó raiz
    let m = no((pc / 4) as u16, (px / 4) as u16, 0, 0, ARQ, b"m");
    tab = vec![0xFF; S];
    tab[..m.len()].copy_from_slice(&m);
    tab[pc..pc + c_.len()].copy_from_slice(&c_);
    tab[px..px + x.len()].copy_from_slice(&x);
    let mut img = Construtor::novo(35);
    img.raiz(33, S as u32).por(33, &tab);
    let raiz = ler(&img.gravar(&t.0)).unwrap();
    let nomes: Vec<_> = raiz.iter().map(|e| e.nome.as_str()).collect();
    assert_eq!(nomes, ["c", "m", "x"]);
}

#[test]
fn lista_degenerada_grande_nao_estoura_a_pilha() {
    let t = Temp::nova();
    let nomes: Vec<String> = (0..5000).map(|i| format!("arq_{i:05}")).collect();
    let ents: Vec<(&[u8], u32, u32, u8)> =
        nomes.iter().map(|n| (n.as_bytes(), 0, 0, ARQ)).collect();
    let tab = tabela(&ents);
    let mut img = Construtor::novo(34);
    img.raiz(33, tab.len() as u32).por(33, &tab);
    assert_eq!(ler(&img.gravar(&t.0)).unwrap().len(), 5000);
}

fn recusa_nome(nome: &[u8]) {
    let t = Temp::nova();
    let tab = tabela(&[(nome, 0, 0, ARQ)]);
    let mut img = Construtor::novo(35);
    img.raiz(33, tab.len() as u32).por(33, &tab);
    match ler(&img.gravar(&t.0)) {
        Err(Erro::NomeInseguro(_)) => {}
        r => panic!(
            "nome {:?} deveria ser recusado, veio {r:?}",
            String::from_utf8_lossy(nome)
        ),
    }
}

#[test]
fn recusa_nomes_que_saem_do_destino_ou_quebram_no_windows() {
    for n in [
        &b".."[..],
        b".",
        b"",
        b"a/b",
        b"..\\x",
        b"c:x",
        b"a\0b",
        b"x\n",
        b"fim.",
        b"fim ",
        b"CON",
        b"nul.txt",
        b"com1.dat",
        b"a*b",
        b"a?b",
        b"a|b",
        b"\"a\"",
        b"<a>",
    ] {
        recusa_nome(n);
    }
}

#[test]
fn recusa_nomes_iguais_sem_diferenciar_maiusculas() {
    let t = Temp::nova();
    let tab = tabela(&[(b"Default.xbe", 0, 0, ARQ), (b"default.XBE", 0, 0, ARQ)]);
    let mut img = Construtor::novo(35);
    img.raiz(33, tab.len() as u32).por(33, &tab);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::NomeInseguro(_))));
}

#[test]
fn recusa_diretorio_que_aponta_para_um_ancestral() {
    let t = Temp::nova();
    let raiz = tabela(&[(b"loop", 34, S as u32, DIR)]);
    let sub = tabela(&[(b"volta", 33, S as u32, DIR)]); // aponta para a raiz
    let mut img = Construtor::novo(36);
    img.raiz(33, S as u32).por(33, &raiz).por(34, &sub);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

#[test]
fn recusa_no_que_aponta_para_si_mesmo() {
    let t = Temp::nova();
    let mut tab = vec![0xFF; S];
    let n = no(0, 0, 0, 0, ARQ, b"eu");
    tab[..n.len()].copy_from_slice(&n);
    // o filho da direita do segundo nó volta para o primeiro
    let m = no(0, 0, 0, 0, ARQ, b"zz");
    tab[16..16 + m.len()].copy_from_slice(&m);
    tab[2..4].copy_from_slice(&4u16.to_le_bytes()); // eu -> zz
    tab[18..20].copy_from_slice(&4u16.to_le_bytes()); // zz -> zz
    let mut img = Construtor::novo(35);
    img.raiz(33, S as u32).por(33, &tab);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

#[test]
fn recusa_profundidade_absurda() {
    let t = Temp::nova();
    let niveis = arvore::PROFUNDIDADE_MAXIMA as u32 + 3;
    let mut img = Construtor::novo(34);
    img.raiz(33, S as u32);
    for i in 0..niveis {
        img.por(33 + i, &tabela(&[(b"d", 34 + i, S as u32, DIR)]));
    }
    img.por(33 + niveis, &[0xFF; S]);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

#[test]
fn recusa_arquivo_alem_do_fim_da_imagem() {
    let t = Temp::nova();
    let tab = tabela(&[(b"grande.bin", 34, 10 * S as u32, ARQ)]);
    let mut img = Construtor::novo(36);
    img.raiz(33, tab.len() as u32).por(33, &tab);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

#[test]
fn recusa_tabela_gigante_sem_alocar() {
    let t = Temp::nova();
    let tab = tabela(&[(b"d", 34, u32::MAX, DIR)]);
    let mut img = Construtor::novo(35);
    img.raiz(33, tab.len() as u32).por(33, &tab);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

#[test]
fn recusa_no_que_atravessa_setor_e_ponteiro_para_fora() {
    let t = Temp::nova();
    // nó começando a 8 bytes do fim do setor
    let mut tab = vec![0xFF; 2 * S];
    let a = no(0, ((S - 8) / 4) as u16, 0, 0, ARQ, b"a");
    tab[..a.len()].copy_from_slice(&a);
    let b = no(0, 0, 0, 0, ARQ, b"bbbbbbbb");
    tab[S - 8..S - 8 + b.len()].copy_from_slice(&b);
    let mut img = Construtor::novo(36);
    img.raiz(33, (2 * S) as u32).por(33, &tab);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));

    // filho da direita além do fim da tabela
    let mut tab = vec![0xFF; S];
    let a = no(0, 0xFFF0, 0, 0, ARQ, b"a");
    tab[..a.len()].copy_from_slice(&a);
    let mut img = Construtor::novo(35);
    img.raiz(33, S as u32).por(33, &tab);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

#[test]
fn recusa_imagem_sem_assinatura_ou_truncada() {
    let t = Temp::nova();
    let p = t.0.join("lixo.iso");
    fs::write(&p, vec![0x55; 100 * S]).unwrap();
    assert!(matches!(ler(&p), Err(Erro::Imagem(_))));
    fs::write(&p, b"curto").unwrap();
    assert!(matches!(ler(&p), Err(Erro::Imagem(_))));
    // raiz apontando além do fim
    let mut img = Construtor::novo(33);
    img.raiz(1000, S as u32);
    assert!(matches!(ler(&img.gravar(&t.0)), Err(Erro::Imagem(_))));
}

fn opcoes() -> Opcoes {
    Opcoes {
        sem_atualizacao: false,
        sobrescrever: false,
    }
}

fn extrair_em(iso: &Path, destino: &Path, o: &Opcoes) -> Result<u64, Erro> {
    let mut img = Imagem::abrir(iso)?;
    let raiz = extrair::selecionar(arvore::ler(&mut img)?, o);
    let p = Progresso::novo("", "", arvore::totais(&raiz).bytes, true);
    extrair::extrair(&mut img, &raiz, destino, o, &p)
}

#[test]
fn extrai_conteudo_exato() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let d = t.0.join("saida");
    assert_eq!(extrair_em(&iso, &d, &opcoes()).unwrap(), 3005);
    assert_eq!(fs::read(d.join("a.bin")).unwrap(), vec![0xAB; 3000]);
    assert_eq!(fs::read(d.join("pasta/dentro.txt")).unwrap(), b"oi!\n\n");
    assert!(d.join("vazia").is_dir());
    assert_eq!(fs::read(d.join("zero")).unwrap().len(), 0);
}

#[test]
fn recusa_destino_com_arquivos_sem_sobrescrever() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);
    let d = t.0.join("saida");
    fs::create_dir(&d).unwrap();
    fs::write(d.join("meu.txt"), b"importante").unwrap();
    assert!(matches!(
        extrair_em(&iso, &d, &opcoes()),
        Err(Erro::Destino(_))
    ));
    assert_eq!(fs::read(d.join("meu.txt")).unwrap(), b"importante");
    assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
    // com --sobrescrever extrai e não apaga o que já estava lá
    extrair_em(
        &iso,
        &d,
        &Opcoes {
            sobrescrever: true,
            ..opcoes()
        },
    )
    .unwrap();
    assert_eq!(fs::read(d.join("meu.txt")).unwrap(), b"importante");
    assert!(d.join("a.bin").is_file());
}

#[test]
fn cancelamento_apaga_so_o_que_criou() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let iso = imagem_valida().gravar(&t.0);

    let d = t.0.join("nova");
    sistema::marcar_cancelamento();
    let r = extrair_em(&iso, &d, &opcoes());
    sistema::limpar_cancelamento();
    assert!(matches!(r, Err(Erro::Cancelado)));
    assert!(
        !d.exists(),
        "a pasta criada pela extração deveria ter sido apagada"
    );

    // numa pasta que já existia, a pasta e o que havia nela ficam
    let d = t.0.join("existente");
    fs::create_dir(&d).unwrap();
    fs::write(d.join("meu.txt"), b"x").unwrap();
    sistema::marcar_cancelamento();
    let r = extrair_em(
        &iso,
        &d,
        &Opcoes {
            sobrescrever: true,
            ..opcoes()
        },
    );
    sistema::limpar_cancelamento();
    assert!(matches!(r, Err(Erro::Cancelado)));
    let sobrou: Vec<_> = fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(sobrou, ["meu.txt"]);
}

#[test]
fn sem_atualizacao_pula_a_pasta_systemupdate() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let upd = tabela(&[(b"su.bin", 36, 4, ARQ)]);
    let raiz = tabela(&[
        (b"$SystemUpdate", 34, upd.len() as u32, DIR),
        (b"default.xex", 36, 4, ARQ),
    ]);
    let mut img = Construtor::novo(37);
    img.raiz(33, raiz.len() as u32)
        .por(33, &raiz)
        .por(34, &upd)
        .por(36, b"XEX2");
    let iso = img.gravar(&t.0);
    let d = t.0.join("saida");
    extrair_em(
        &iso,
        &d,
        &Opcoes {
            sem_atualizacao: true,
            ..opcoes()
        },
    )
    .unwrap();
    assert!(!d.join("$SystemUpdate").exists());
    assert!(d.join("default.xex").is_file());
}

/// Gerador pseudoaleatório simples (xorshift), para o fuzz ser reproduzível.
struct Aleatorio(u64);

impl Aleatorio {
    fn prox(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// Corrompe bytes aleatórios das tabelas e do descritor de uma imagem válida:
/// o leitor pode aceitar ou recusar, mas nunca entrar em pânico, travar ou
/// deixar sair da pasta de destino.
#[test]
fn fuzz_imagem_corrompida() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let base = imagem_valida();
    let mut rng = Aleatorio(0x5EED_1234_ABCD_0001);
    for rodada in 0..3000 {
        let mut c = Construtor {
            bytes: base.bytes.clone(),
        };
        let quantos = 1 + rng.prox() % 8;
        for _ in 0..quantos {
            // descritor (setor 32) e tabelas (33..=35)
            let i = 32 * S + (rng.prox() as usize % (4 * S));
            c.bytes[i] = rng.prox() as u8;
        }
        let iso = c.gravar(&t.0);
        if let Ok(raiz) = ler(&iso) {
            let d = t.0.join(format!("s{rodada}"));
            if extrair_em(&iso, &d, &opcoes()).is_ok() {
                // tudo o que foi criado está dentro do destino
                arvore::percorrer(&raiz, &mut |_, cam| {
                    let p = d.join(cam);
                    assert!(p.starts_with(&d) && p.exists(), "{cam}");
                });
                fs::remove_dir_all(&d).unwrap();
            }
        }
    }
    // nada além da imagem e das saídas apagadas
    let resto: Vec<_> = fs::read_dir(&t.0)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(resto, ["teste.iso"]);
}

// ---------------------------------------------------------------------------
// criar / reescrever
// ---------------------------------------------------------------------------

use crate::criar;

fn criar_de(pasta: &Path, saida: &Path) -> Result<criar::Resumo, Erro> {
    let o = criar::Opcoes {
        sobrescrever: false,
        sem_atualizacao: false,
    };
    let fonte = criar::Fonte::Pasta(pasta);
    let prep = criar::preparar(&fonte, &o)?;
    let p = Progresso::novo("", "", prep.bytes, true);
    criar::gravar(fonte, prep, saida, &o, &p)
}

/// Compara duas pastas: mesmos nomes, mesmos bytes.
fn pastas_iguais(a: &Path, b: &Path) {
    let mut na: Vec<_> = fs::read_dir(a)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let mut nb: Vec<_> = fs::read_dir(b)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    na.sort();
    nb.sort();
    assert_eq!(na, nb, "{}", a.display());
    for n in na {
        let (x, y) = (a.join(&n), b.join(&n));
        if x.is_dir() {
            pastas_iguais(&x, &y);
        } else {
            assert_eq!(
                fs::read(&x).unwrap(),
                fs::read(&y).unwrap(),
                "{}",
                x.display()
            );
        }
    }
}

fn pasta_de_jogo(raiz: &Path) {
    fs::create_dir_all(raiz.join("media/sons")).unwrap();
    fs::create_dir_all(raiz.join("vazia")).unwrap();
    fs::create_dir_all(raiz.join("muitos")).unwrap();
    fs::write(raiz.join("default.xex"), b"XEX2 falso").unwrap();
    fs::write(raiz.join("zero.bin"), b"").unwrap();
    fs::write(
        raiz.join("media/grande.bin"),
        (0..3_000_000u32).map(|i| (i * 7) as u8).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(raiz.join("media/sons/Ação.wav"), b"RIFF").unwrap();
    // o bastante para a tabela ocupar vários setores
    for i in 0..700 {
        fs::write(
            raiz.join(format!("muitos/Arq_{i:04}_{}.dat", "x".repeat(i % 40))),
            vec![i as u8; i],
        )
        .unwrap();
    }
}

#[test]
fn criar_e_extrair_devolve_a_mesma_pasta() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let jogo = t.0.join("jogo");
    pasta_de_jogo(&jogo);
    let iso = t.0.join("jogo.iso");
    let r = criar_de(&jogo, &iso).unwrap();
    assert_eq!(r.arquivos, 704);
    assert_eq!(fs::metadata(&iso).unwrap().len() % (64 * 1024), 0);

    // a árvore gravada está em ordem e cada tabela é uma árvore balanceada
    let mut img = Imagem::abrir(&iso).unwrap();
    let raiz = arvore::ler(&mut img).unwrap();
    let muitos = raiz.iter().find(|e| e.nome == "muitos").unwrap();
    assert!(
        muitos.tamanho > 2048 * 3,
        "a tabela deveria ter vários setores"
    );
    let nomes: Vec<String> = muitos
        .filhos
        .iter()
        .map(|e| e.nome.to_ascii_uppercase())
        .collect();
    let mut ordenados = nomes.clone();
    ordenados.sort();
    assert_eq!(nomes, ordenados);

    let d = t.0.join("saida");
    extrair_em(&iso, &d, &opcoes()).unwrap();
    pastas_iguais(&jogo, &d);

    // reescrever a imagem criada dá o mesmo conteúdo
    let o = criar::Opcoes {
        sobrescrever: false,
        sem_atualizacao: false,
    };
    let mut img = Imagem::abrir(&iso).unwrap();
    let raiz = arvore::ler(&mut img).unwrap();
    let fonte = criar::Fonte::Imagem(&mut img, raiz);
    let prep = criar::preparar(&fonte, &o).unwrap();
    let p = Progresso::novo("", "", prep.bytes, true);
    let iso2 = t.0.join("de_novo.iso");
    criar::gravar(fonte, prep, &iso2, &o, &p).unwrap();
    let d2 = t.0.join("saida2");
    extrair_em(&iso2, &d2, &opcoes()).unwrap();
    pastas_iguais(&jogo, &d2);
}

#[test]
fn criar_recusa_o_que_nao_daria_uma_imagem_valida() {
    let t = Temp::nova();
    // nomes iguais sem diferenciar maiúsculas (possível no Linux)
    let j = t.0.join("dup");
    fs::create_dir_all(&j).unwrap();
    fs::write(j.join("A.txt"), b"1").unwrap();
    fs::write(j.join("a.TXT"), b"2").unwrap();
    assert!(matches!(
        criar_de(&j, &t.0.join("dup.iso")),
        Err(Erro::Destino(_))
    ));

    // nome que o Windows não aceita
    let j = t.0.join("res");
    fs::create_dir_all(&j).unwrap();
    fs::write(j.join("aux.txt"), b"1").unwrap();
    assert!(matches!(
        criar_de(&j, &t.0.join("res.iso")),
        Err(Erro::Destino(_))
    ));

    // imagem dentro da própria pasta de origem
    let j = t.0.join("dentro");
    fs::create_dir_all(&j).unwrap();
    fs::write(j.join("a"), b"1").unwrap();
    assert!(matches!(
        criar_de(&j, &j.join("x.iso")),
        Err(Erro::Destino(_))
    ));

    // saída que já existe
    let j = t.0.join("ok");
    fs::create_dir_all(&j).unwrap();
    fs::write(j.join("a"), b"1").unwrap();
    fs::write(t.0.join("ja.iso"), b"meu").unwrap();
    assert!(matches!(
        criar_de(&j, &t.0.join("ja.iso")),
        Err(Erro::Destino(_))
    ));
    assert_eq!(fs::read(t.0.join("ja.iso")).unwrap(), b"meu");

    // link simbólico para um ancestral
    #[cfg(unix)]
    {
        let j = t.0.join("laco");
        fs::create_dir_all(j.join("sub")).unwrap();
        std::os::unix::fs::symlink(&j, j.join("sub/volta")).unwrap();
        assert!(matches!(
            criar_de(&j, &t.0.join("laco.iso")),
            Err(Erro::Destino(_))
        ));
    }
    // nenhum temporário ficou para trás
    assert!(fs::read_dir(&t.0).unwrap().all(|e| {
        !e.unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".parcial")
    }));
}

#[test]
fn criar_cancelado_nao_deixa_nada() {
    let _v = EXTRACAO.lock().unwrap_or_else(|e| e.into_inner());
    let t = Temp::nova();
    let jogo = t.0.join("jogo");
    pasta_de_jogo(&jogo);
    sistema::marcar_cancelamento();
    let r = criar_de(&jogo, &t.0.join("jogo.iso"));
    sistema::limpar_cancelamento();
    assert!(matches!(r, Err(Erro::Cancelado)));
    let sobrou: Vec<_> = fs::read_dir(&t.0)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(sobrou, ["jogo"]);
}
