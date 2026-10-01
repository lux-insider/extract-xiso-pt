# extract-xiso-pt

Ferramenta para imagens de disco de **Xbox** e **Xbox 360** (sistema de
arquivos XDVDFS): listar, extrair, criar a partir de uma pasta, reescrever
como XISO enxuta e verificar a integridade — escrita do zero em **Rust**, com
binários pequenos para Linux e Windows e a mesma interface de terminal do
[iso2god-pt](https://github.com/lux-insider/iso2god-pt).

Este projeto é uma implementação independente. Ele foi escrito a partir da
documentação pública do formato; o `extract-xiso` do XboxDev serviu só como
referência **de fora**: as saídas dos dois foram comparadas em imagens de
teste, e nenhum código-fonte dele foi usado.

## Recursos

- **Todos os layouts**: XISO, XGD1 (Xbox), XGD2 e XGD3 (Xbox 360), detectados
  sozinhos.
- **`extrair`** igual byte a byte ao `extract-xiso` (conferido nos 3696
  arquivos do Resident Evil 5 e em imagens de teste de Xbox), no mesmo tempo.
- **`criar`** uma XISO a partir da pasta de um jogo de Xbox ou de Xbox 360,
  que o `extract-xiso` oficial e o iso2god-pt leem normalmente.
- **`reescrever`** um disco completo como XISO enxuta: tira a partição de
  vídeo e o espaço vazio (o RE5 foi de 7,30 GiB para 6,71 GiB, com os 3696
  arquivos idênticos ao original).
- **`verificar`** a integridade: estrutura inteira, leitura de todos os bytes
  e hashes CRC32, MD5 e SHA-1 — e, com `--dat`, a comparação com um `.dat` do
  [Redump](http://redump.org), dizendo se a imagem é a do disco original.
- **Liberar a mídia do XBE**, só como opção (`--liberar-midia`, desligado por
  padrão), só na cópia dentro da imagem nova.
- **Segura com imagens estranhas ou maliciosas.** Nomes que sairiam da pasta
  de destino (`..`, `/`, `C:`), nomes que o Windows não aceita, laços na
  árvore, profundidade absurda, tabelas compartilhadas e trechos além do fim
  do arquivo viram um erro explicado, antes de gravar qualquer coisa. A
  extração nunca grava através de um link simbólico (ou junção) que já
  esteja no destino. Testado com milhares de imagens corrompidas de
  propósito.
- **Nada pela metade.** Ctrl+C, SIGTERM, fechar o terminal ou a janela do
  console e qualquer falha apagam o que a operação criou. Um arquivo
  incompleto nunca fica com o nome de pronto, e uma imagem nova só ganha o
  nome final depois de gravada e relida.
- **Erros que dizem o que houve:** qual arquivo, qual operação e a causa
  (não existe, sem permissão, disco cheio...).
- Espaço livre conferido antes de começar, em vez de falhar a 90%.
- Protocolo `--progresso-json`, o mesmo do iso2god-pt, para outro programa
  (como o xiso-manager) mostrar a barra de progresso.

## Instalação / build

Baixe um binário pronto na [página de Releases](https://github.com/lux-insider/extract-xiso-pt/releases),
ou compile a partir do código-fonte (requer o [Rust](https://rustup.rs),
edição 2024):

```bash
git clone https://github.com/lux-insider/extract-xiso-pt.git
cd extract-xiso-pt
cargo build --release
cp target/release/extract-xiso-pt ~/.local/bin/
```

### Windows

O `extract-xiso-pt.exe` da página de Releases roda no Windows 10/11 (64
bits) sem instalar nada. Para gerá-lo a partir do Linux:

```bash
sudo apt install clang lld llvm
rustup target add x86_64-pc-windows-msvc
cargo install --locked cargo-xwin
cargo xwin build --release --target x86_64-pc-windows-msvc
```

O `.exe` fica em `target/x86_64-pc-windows-msvc/release/`. O CRT é estático:
ele não depende do Visual C++ Redistributable.

## Uso

```bash
# informações: layout, console, data, conteúdo
extract-xiso-pt info jogo.iso

# listar sem extrair (ou a árvore inteira em JSON com --json)
extract-xiso-pt listar jogo.iso

# extrair (sem -d: numa pasta com o nome da imagem, ao lado dela)
extract-xiso-pt extrair jogo.iso -d pasta/
extract-xiso-pt extrair jogo.iso -s              # sem a pasta $SystemUpdate

# criar uma XISO a partir de uma pasta (sem -s: pasta.iso ao lado dela)
extract-xiso-pt criar pasta/ -s jogo.iso

# disco completo -> XISO enxuta (sem -s: jogo.xiso.iso ao lado)
extract-xiso-pt reescrever jogo.iso
extract-xiso-pt reescrever jogo.iso --substituir # troca o original, só no fim

# .dat do Redump: instale uma vez (aceita o .zip baixado do site)
extract-xiso-pt dats instalar "Microsoft - Xbox 360 - Datfile (3691).zip"
extract-xiso-pt dats                             # mostra os instalados

# integridade e hashes; com os .dat instalados, diz se é a imagem original
extract-xiso-pt verificar jogo.iso
extract-xiso-pt verificar jogo.iso --dat outro.dat   # um .dat específico
extract-xiso-pt verificar jogo.iso --sem-dat         # só a integridade
```

`criar` e `reescrever` aceitam `-u`/`--sem-atualizacao` (deixa a
`$SystemUpdate` de fora) e, para jogos de Xbox, `--liberar-midia`.

### Sobre o `.dat` do Redump

Baixe os .dat em [redump.org/downloads](http://redump.org/downloads/)
("Microsoft - Xbox" e "Microsoft - Xbox 360") e instale com `dats instalar`.
Eles não vêm junto com o programa: o Redump atualiza os .dat sempre, e
instalando você fica com a versão mais nova (a antiga do mesmo console é
trocada). Ficam em `~/.local/share/extract-xiso-pt/dats` no Linux,
`%APPDATA%\extract-xiso-pt\dats` no Windows, ou numa pasta `dats` ao lado do
executável.

O Redump cataloga **discos completos**. Uma imagem com o tamanho de um disco
XGD completo pode conferir; uma XISO enxuta (de `reescrever` ou de outra
ferramenta) tem outros bytes e nunca confere. O `verificar` avisa isso.

### Sobre `--liberar-midia`

O certificado do `default.xbe` diz de quais mídias o jogo aceita rodar. A
opção acrescenta o disco rígido e os DVD/CD gravados a esse campo, **só na
cópia dentro da imagem nova**: a pasta ou imagem de origem não muda. O
certificado é assinado, então isso serve para console desbloqueado ou
emulador, e a imagem deixa de ser idêntica ao disco. Por isso é opção, e não
o padrão.

### Códigos de saída

| Código | Significado |
|---|---|
| 0 | tudo certo |
| 1 | erro (imagem inválida, destino ocupado, sem espaço, opção desconhecida...) |
| 2 | `verificar`: o Redump tem este jogo com outro SHA-1 (modificado, corrompido ou outra versão); com `--dat`, também quando a imagem não está no .dat |
| 130 | cancelado (Ctrl+C, SIGTERM, terminal ou janela fechados) |

### Modo máquina

```bash
extract-xiso-pt extrair jogo.iso -d pasta/ --progresso-json
extract-xiso-pt verificar jogo.iso --progresso-json   # relatório no evento "verificado"
extract-xiso-pt info jogo.iso --json
```

Com isso, o programa emite uma linha JSON por evento em stdout: `fase`,
`progresso` (bytes, velocidade e ETA), `concluido` ou `erro`.

## Estrutura do repositório

```
src/imagem.rs     onde fica a partição, descritor de volume, leitura segura
src/arvore.rs     árvore de arquivos, validada
src/extrair.rs    extração atômica
src/criar.rs      gravação de XISO (pasta ou outra imagem), --liberar-midia
src/verificar.rs  estrutura, hashes e comparação com .dat
src/dats.rs       .dat instalados, leitura de .zip
src/testes.rs     imagens sintéticas, entradas hostis e fuzz
PLANO.md          o formato e as decisões de segurança
```

## Créditos

- O formato XDVDFS vem da documentação pública da comunidade do Xbox.
- [extract-xiso](https://github.com/XboxDev/extract-xiso) (XboxDev), usado só
  como referência de comportamento para comparar as saídas.

## Licença

[MIT](LICENSE).
