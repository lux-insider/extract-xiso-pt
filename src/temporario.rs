//! Arquivos temporários (`.parcial`): o arquivo é gravado com este sufixo e
//! só ganha o nome final, num `rename`, depois de completo.
//!
//! O temporário é sempre criado de forma exclusiva (`create_new`, que não
//! segue link simbólico): o que estiver no caminho dele — um `.parcial`
//! esquecido por uma execução interrompida, ou um link plantado no destino
//! — é apagado antes (`remove_file` apaga o link, nunca o arquivo para onde
//! ele aponta). Assim gravar o temporário nunca escreve fora do lugar
//! pedido.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Sufixo dos arquivos ainda sendo gravados.
pub const SUFIXO: &str = ".extract-xiso-pt.parcial";

/// `caminho` + sufixo temporário.
pub fn caminho_de(caminho: &Path) -> PathBuf {
    let mut p = caminho.as_os_str().to_owned();
    p.push(SUFIXO);
    PathBuf::from(p)
}

/// `caminho` + `.n` + sufixo temporário: a alternativa quando o nome
/// normal já é um arquivo de verdade (ver `extrair`).
pub fn caminho_numerado(caminho: &Path, n: u32) -> PathBuf {
    let mut p = caminho.as_os_str().to_owned();
    p.push(format!(".{n}{SUFIXO}"));
    PathBuf::from(p)
}

/// Cria `caminho` para escrita, vazio, sem seguir links.
pub fn criar(caminho: &Path) -> io::Result<File> {
    match fs::symlink_metadata(caminho) {
        Ok(m) if m.is_dir() => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "já existe uma pasta com o nome do arquivo temporário",
            ));
        }
        Ok(_) => fs::remove_file(caminho)?,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(caminho)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn criar_apaga_o_que_havia_e_nao_segue_link() {
        let d = std::env::temp_dir().join(format!("extract-xiso-pt-temp-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let t = caminho_de(&d.join("a.bin"));
        assert!(
            t.to_string_lossy()
                .ends_with("a.bin.extract-xiso-pt.parcial")
        );

        // um .parcial esquecido é trocado por um vazio
        fs::write(&t, b"resto de uma execucao anterior").unwrap();
        drop(criar(&t).unwrap());
        assert_eq!(fs::read(&t).unwrap(), b"");

        #[cfg(unix)]
        {
            use std::io::Write;
            // um link no lugar do temporário: o alvo fica intacto
            let alvo = d.join("alvo.txt");
            fs::write(&alvo, b"importante").unwrap();
            fs::remove_file(&t).unwrap();
            std::os::unix::fs::symlink(&alvo, &t).unwrap();
            criar(&t).unwrap().write_all(b"conteudo do jogo").unwrap();
            assert_eq!(fs::read(&alvo).unwrap(), b"importante");
            assert!(!fs::symlink_metadata(&t).unwrap().file_type().is_symlink());
        }

        // uma pasta no lugar do temporário é erro, não é apagada
        fs::remove_file(&t).unwrap();
        fs::create_dir(&t).unwrap();
        assert!(criar(&t).is_err());
        assert!(t.is_dir());
        fs::remove_dir_all(&d).ok();
    }
}
