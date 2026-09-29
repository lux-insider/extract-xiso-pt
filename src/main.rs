//! extract-xiso-pt: imagens de disco de Xbox e Xbox 360 (XDVDFS).
//!
//!   imagem.rs    onde fica a partição, descritor de volume, leitura segura
//!   arvore.rs    árvore de arquivos (ponteiros da árvore binária, validada)
//!   extrair.rs   extração com saída atômica e desfazer em caso de falha
//!   criar.rs     gravação de XISO a partir de pasta ou de outra imagem
//!   verificar.rs integridade: estrutura, leitura completa, hashes e .dat
//!   progresso.rs barra no terminal ou eventos JSON (--progresso-json)
//!   terminal.rs  cores, caixas e console do Windows (do iso2god-pt)
//!   sistema.rs   espaço livre e Ctrl+C/SIGTERM limpos (do iso2god-pt)

mod arvore;
mod cli;
mod criar;
mod erro;
mod extrair;
mod imagem;
mod progresso;
mod sistema;
mod terminal;
#[cfg(test)]
mod testes;
mod verificar;

use std::path::Path;

use clap::Parser;

use cli::{Cli, Comando};
use erro::{Erro, Resultado};
use imagem::Imagem;
use terminal::{LARGURA, Tema, emo, fmt_bytes};

const SAIDA_CANCELADO: i32 = 130;
/// `verificar --dat`: a imagem está íntegra, mas não é a do .dat.
const SAIDA_NAO_CONFERE: i32 = 2;

fn main() {
    sistema::instalar_cancelamento();
    terminal::preparar_console();

    let cli = Cli::parse();
    let json = matches!(
        &cli.comando,
        Comando::Extrair {
            progresso_json: true,
            ..
        } | Comando::Criar {
            progresso_json: true,
            ..
        } | Comando::Reescrever {
            progresso_json: true,
            ..
        }
    );
    match executar(cli.comando) {
        Ok(()) => {}
        Err(Erro::Cancelado) => {
            progresso::erro_final("Operação cancelada.", json);
            if !json {
                Tema::detectar().aviso("Operação cancelada; o que ela tinha criado foi apagado.");
            }
            std::process::exit(SAIDA_CANCELADO);
        }
        Err(e) => {
            progresso::erro_final(&e.to_string(), json);
            if !json {
                Tema::detectar().erro(&e.to_string());
            }
            std::process::exit(1);
        }
    }
}

fn executar(comando: Comando) -> Resultado<()> {
    match comando {
        Comando::Info { imagem, json } => info(&imagem, json),
        Comando::Listar { imagem, json } => listar(&imagem, json),
        Comando::Extrair {
            imagem,
            destino,
            sem_atualizacao,
            sobrescrever,
            progresso_json,
        } => {
            let destino = destino.unwrap_or_else(|| extrair::destino_padrao(&imagem));
            extrair_cmd(
                &imagem,
                &destino,
                extrair::Opcoes {
                    sem_atualizacao,
                    sobrescrever,
                },
                progresso_json,
            )
        }
        Comando::Verificar {
            imagem,
            dat,
            json,
            progresso_json,
        } => verificar_cmd(&imagem, dat.as_deref(), json, progresso_json),
        Comando::Criar {
            pasta,
            saida,
            sem_atualizacao,
            sobrescrever,
            progresso_json,
        } => {
            let saida = saida.unwrap_or_else(|| criar::saida_padrao_pasta(&pasta));
            criar_cmd(
                &pasta,
                &saida,
                criar::Opcoes {
                    sobrescrever,
                    sem_atualizacao,
                },
                progresso_json,
            )
        }
        Comando::Reescrever {
            imagem,
            saida,
            sem_atualizacao,
            sobrescrever,
            substituir,
            progresso_json,
        } => {
            let saida = saida.unwrap_or_else(|| criar::saida_padrao_reescrita(&imagem));
            reescrever_cmd(
                &imagem,
                &saida,
                substituir,
                criar::Opcoes {
                    sobrescrever,
                    sem_atualizacao,
                },
                progresso_json,
            )
        }
    }
}

/// Console pelo executável na raiz: default.xex = Xbox 360, default.xbe = Xbox.
fn console(raiz: &[arvore::Entrada]) -> Option<&'static str> {
    let tem = |n: &str| {
        raiz.iter()
            .any(|e| !e.eh_diretorio() && e.nome.eq_ignore_ascii_case(n))
    };
    if tem("default.xex") {
        Some("Xbox 360")
    } else if tem("default.xbe") {
        Some("Xbox")
    } else {
        None
    }
}

/// FILETIME (centenas de ns desde 1601-01-01) como "AAAA-MM-DD", se plausível.
fn data(filetime: u64) -> Option<String> {
    let seg = (filetime / 10_000_000).checked_sub(11_644_473_600)? as i64;
    let dias = seg.div_euclid(86_400);
    // dias desde 1970 -> data civil (algoritmo de Howard Hinnant)
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    (1990..=2100)
        .contains(&a)
        .then(|| format!("{a:04}-{m:02}-{d:02}"))
}

#[derive(serde::Serialize)]
struct InfoJson<'a> {
    layout: &'a str,
    console: Option<&'a str>,
    criacao: Option<String>,
    tamanho_volume: u64,
    arquivos: u64,
    diretorios: u64,
    bytes: u64,
}

fn info(caminho: &Path, json: bool) -> Resultado<()> {
    let mut img = Imagem::abrir(caminho)?;
    let raiz = arvore::ler(&mut img)?;
    let t = arvore::totais(&raiz);
    let i = InfoJson {
        layout: img.layout.rotulo(),
        console: console(&raiz),
        criacao: data(img.criacao),
        tamanho_volume: img.tamanho_volume,
        arquivos: t.arquivos,
        diretorios: t.diretorios,
        bytes: t.bytes,
    };
    if json {
        println!("{}", serde_json::to_string(&i).unwrap_or_default());
        return Ok(());
    }
    let tema = Tema::detectar();
    println!(
        "{}",
        tema.caixa_titulo(&titulo_app(), emo::APP(), LARGURA, true)
    );
    println!(
        "{}",
        tema.campo(
            "Imagem",
            &terminal::encurtar_home(caminho),
            emo::ORIGEM(),
            16
        )
    );
    println!("{}", tema.campo("Layout", i.layout, emo::DISCO(), 16));
    println!(
        "{}",
        tema.campo(
            "Console",
            i.console.unwrap_or("não identificado"),
            emo::JOGO(),
            16
        )
    );
    if let Some(d) = &i.criacao {
        println!("{}", tema.campo("Criada em", d, emo::TEMPO(), 16));
    }
    println!(
        "{}",
        tema.campo(
            "Conteúdo",
            &format!(
                "{} arquivos em {} pastas · {}",
                i.arquivos,
                i.diretorios,
                fmt_bytes(i.bytes)
            ),
            emo::DADOS(),
            16
        )
    );
    println!(
        "{}",
        tema.campo("Volume", &fmt_bytes(i.tamanho_volume), emo::RESUMO(), 16)
    );
    Ok(())
}

fn listar(caminho: &Path, json: bool) -> Resultado<()> {
    let mut img = Imagem::abrir(caminho)?;
    let raiz = arvore::ler(&mut img)?;
    if json {
        println!("{}", serde_json::to_string(&raiz).unwrap_or_default());
        return Ok(());
    }
    let tema = Tema::detectar();
    arvore::percorrer(&raiz, &mut |e, caminho| {
        if e.eh_diretorio() {
            println!(
                "{}",
                tema.c(&format!("{caminho}/"), &[&terminal::c::azul()])
            );
        } else {
            println!(
                "{caminho}  {}",
                tema.c(&fmt_bytes(e.tamanho as u64), &[&terminal::c::cinza()])
            );
        }
    });
    let t = arvore::totais(&raiz);
    println!();
    tema.info_linha(
        emo::RESUMO(),
        &format!(
            "{} arquivos em {} pastas · {}",
            t.arquivos,
            t.diretorios,
            fmt_bytes(t.bytes)
        ),
    );
    Ok(())
}

fn extrair_cmd(
    caminho: &Path,
    destino: &Path,
    opcoes: extrair::Opcoes,
    json: bool,
) -> Resultado<()> {
    progresso::Progresso::fase(json, "lendo", "Lendo a árvore de arquivos da imagem...");
    let mut img = Imagem::abrir(caminho)?;
    let raiz = extrair::selecionar(arvore::ler(&mut img)?, &opcoes);
    let t = arvore::totais(&raiz);

    let tema = Tema::detectar();
    if !json {
        println!(
            "{}",
            tema.caixa_titulo(&titulo_app(), emo::APP(), LARGURA, true)
        );
        println!(
            "{}",
            tema.campo(
                "Imagem",
                &terminal::encurtar_home(caminho),
                emo::ORIGEM(),
                16
            )
        );
        println!(
            "{}",
            tema.campo(
                "Destino",
                &terminal::encurtar_home(destino),
                emo::DESTINO(),
                16
            )
        );
        println!(
            "{}",
            tema.campo(
                "Conteúdo",
                &format!("{} arquivos · {}", t.arquivos, fmt_bytes(t.bytes)),
                emo::DADOS(),
                16
            )
        );
    }
    progresso::Progresso::fase(json, "extraindo", "Extraindo...");
    let p = progresso::Progresso::novo("Extraindo", emo::PROGRESSO(), t.bytes, json);
    let r = extrair::extrair(&mut img, &raiz, destino, &opcoes, &p);
    p.terminar(r.is_ok());
    let bytes = r?;
    let msg = format!(
        "Concluído em {:.1}s: {} arquivos ({}) em {}",
        p.duracao(),
        t.arquivos,
        fmt_bytes(bytes),
        destino.display()
    );
    p.concluido(&destino.to_string_lossy(), &msg);
    if !json {
        tema.sucesso(&msg);
    }
    Ok(())
}

fn criar_cmd(pasta: &Path, saida: &Path, opcoes: criar::Opcoes, json: bool) -> Resultado<()> {
    progresso::Progresso::fase(json, "lendo", "Lendo a pasta...");
    let fonte = criar::Fonte::Pasta(pasta);
    let prep = criar::preparar(&fonte, &opcoes)?;
    let tema = Tema::detectar();
    if !json {
        println!(
            "{}",
            tema.caixa_titulo(&titulo_app(), emo::APP(), LARGURA, true)
        );
        println!(
            "{}",
            tema.campo("Pasta", &terminal::encurtar_home(pasta), emo::ORIGEM(), 16)
        );
        println!(
            "{}",
            tema.campo(
                "Imagem",
                &terminal::encurtar_home(saida),
                emo::DESTINO(),
                16
            )
        );
        println!(
            "{}",
            tema.campo(
                "Conteúdo",
                &format!("{} arquivos · {}", prep.arquivos, fmt_bytes(prep.bytes)),
                emo::DADOS(),
                16
            )
        );
    }
    gravar_cmd(fonte, prep, saida, &opcoes, json, "criando", "Criando")
}

fn reescrever_cmd(
    caminho: &Path,
    saida: &Path,
    substituir: bool,
    opcoes: criar::Opcoes,
    json: bool,
) -> Resultado<()> {
    progresso::Progresso::fase(json, "lendo", "Lendo a árvore de arquivos da imagem...");
    let mut img = Imagem::abrir(caminho)?;
    let raiz = arvore::ler(&mut img)?;
    let layout = img.layout.rotulo();
    let tema = Tema::detectar();
    if !json {
        println!(
            "{}",
            tema.caixa_titulo(&titulo_app(), emo::APP(), LARGURA, true)
        );
        println!(
            "{}",
            tema.campo(
                "Imagem",
                &format!("{} ({layout})", terminal::encurtar_home(caminho)),
                emo::ORIGEM(),
                16
            )
        );
        let destino = if substituir {
            "no lugar da original".to_string()
        } else {
            terminal::encurtar_home(saida)
        };
        println!("{}", tema.campo("Nova", &destino, emo::DESTINO(), 16));
    }
    let antes = std::fs::metadata(caminho)?.len();
    let fonte = criar::Fonte::Imagem(&mut img, raiz);
    let prep = criar::preparar(&fonte, &opcoes)?;
    // com --substituir grava ao lado e só troca depois de pronta e relida
    let alvo = if substituir {
        let mut n = caminho.as_os_str().to_owned();
        n.push(".extract-xiso-pt.nova");
        std::path::PathBuf::from(n)
    } else {
        saida.to_path_buf()
    };
    let opcoes_alvo = criar::Opcoes {
        sobrescrever: opcoes.sobrescrever || substituir,
        ..opcoes
    };
    gravar_cmd(
        fonte,
        prep,
        &alvo,
        &opcoes_alvo,
        json,
        "reescrevendo",
        "Reescrevendo",
    )?;
    drop(img); // no Windows um arquivo aberto não pode ser substituído
    let depois = std::fs::metadata(&alvo)?.len();
    if substituir && let Err(e) = std::fs::rename(&alvo, caminho) {
        std::fs::remove_file(&alvo).ok();
        return Err(e.into());
    }
    if !json {
        tema.info_linha(
            emo::RESUMO(),
            &format!(
                "{} → {} ({} a menos)",
                fmt_bytes(antes),
                fmt_bytes(depois),
                fmt_bytes(antes.saturating_sub(depois))
            ),
        );
    }
    Ok(())
}

fn gravar_cmd(
    fonte: criar::Fonte,
    prep: criar::Preparado,
    saida: &Path,
    opcoes: &criar::Opcoes,
    json: bool,
    fase: &str,
    rotulo: &str,
) -> Resultado<()> {
    progresso::Progresso::fase(json, fase, &format!("{rotulo}..."));
    let p = progresso::Progresso::novo(rotulo, emo::PROGRESSO(), prep.bytes, json);
    let r = criar::gravar(fonte, prep, saida, opcoes, &p);
    p.terminar(r.is_ok());
    let resumo = r?;
    let msg = format!(
        "Concluído em {:.1}s: {} arquivos ({}) numa imagem de {} em {}",
        p.duracao(),
        resumo.arquivos,
        fmt_bytes(resumo.bytes_conteudo),
        fmt_bytes(resumo.tamanho_imagem),
        saida.display()
    );
    p.concluido(&saida.to_string_lossy(), &msg);
    if !json {
        Tema::detectar().sucesso(&msg);
    }
    Ok(())
}

fn verificar_cmd(
    caminho: &Path,
    dat: Option<&Path>,
    json: bool,
    progresso_json: bool,
) -> Resultado<()> {
    let tema = Tema::detectar();
    let texto = !json && !progresso_json;
    if texto {
        println!(
            "{}",
            tema.caixa_titulo(&titulo_app(), emo::APP(), LARGURA, true)
        );
        println!(
            "{}",
            tema.campo(
                "Imagem",
                &terminal::encurtar_home(caminho),
                emo::ORIGEM(),
                16
            )
        );
    }
    progresso::Progresso::fase(progresso_json, "estrutura", "Conferindo a estrutura...");
    let rel = verificar::verificar(caminho, dat, |total| {
        progresso::Progresso::fase(progresso_json, "hashes", "Lendo a imagem inteira...");
        progresso::Progresso::novo_quieto("Lendo", emo::PROGRESSO(), total, progresso_json, json)
    })?;
    let nao_confere = rel
        .dat
        .as_ref()
        .is_some_and(|d| !matches!(d.situacao, verificar::Situacao::Confere));

    if json {
        println!("{}", serde_json::to_string(&rel).unwrap_or_default());
    } else if progresso_json {
        progresso::emitir_verificado(&rel);
    } else {
        let completo = match rel.disco_completo {
            Some(true) => " · disco completo",
            Some(false) => " · tamanho diferente de um disco completo",
            None => " · enxuta (só o sistema de arquivos)",
        };
        println!(
            "{}",
            tema.campo(
                "Layout",
                &format!("{}{completo}", rel.layout),
                emo::DISCO(),
                16
            )
        );
        println!(
            "{}",
            tema.campo(
                "Estrutura",
                &format!(
                    "íntegra · {} arquivos em {} pastas",
                    rel.arquivos, rel.diretorios
                ),
                emo::DADOS(),
                16
            )
        );
        for a in &rel.avisos {
            tema.aviso(a);
        }
        println!(
            "{}",
            tema.campo(
                "Tamanho",
                &format!(
                    "{} ({} bytes)",
                    fmt_bytes(rel.hashes.tamanho),
                    rel.hashes.tamanho
                ),
                emo::RESUMO(),
                16
            )
        );
        println!("{}", tema.campo("CRC32", &rel.hashes.crc32, "", 16));
        println!("{}", tema.campo("MD5", &rel.hashes.md5, "", 16));
        println!("{}", tema.campo("SHA-1", &rel.hashes.sha1, "", 16));
        match &rel.dat {
            None => {
                tema.sucesso("Imagem íntegra: a estrutura confere e todos os bytes foram lidos.")
            }
            Some(d) => match d.situacao {
                verificar::Situacao::Confere => tema.sucesso(&format!(
                    "Confere com o .dat: {} ({})",
                    d.jogo.as_deref().unwrap_or("?"),
                    d.rom.as_deref().unwrap_or("?")
                )),
                verificar::Situacao::NaoConfere => tema.erro(&format!(
                    "Não confere: o .dat tem {} com outro SHA-1 — a imagem foi modificada, está \
                     corrompida ou é de outra versão",
                    d.rom.as_deref().unwrap_or("?")
                )),
                verificar::Situacao::NaoEncontrada => {
                    let dica = if rel.disco_completo.is_none() {
                        " (o Redump cataloga discos completos; uma imagem enxuta nunca confere)"
                    } else {
                        ""
                    };
                    tema.aviso(&format!("O SHA-1 não está no .dat{dica}"))
                }
            },
        }
    }
    if nao_confere {
        std::process::exit(SAIDA_NAO_CONFERE);
    }
    Ok(())
}

fn titulo_app() -> String {
    format!("extract-xiso-pt v{}", env!("CARGO_PKG_VERSION"))
}
