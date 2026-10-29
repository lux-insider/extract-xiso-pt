# Histórico de versões

Todas as mudanças relevantes do extract-xiso-pt, da mais nova para a mais
antiga. As versões seguem o [Versionamento Semântico](https://semver.org/lang/pt-BR/).

| Versão | Data | Destaque |
|---|---|---|
| [0.2.4](#024--2026-10-20) | 20/10/2026 | Cancelamento sem console e ajuda em português |
| [0.2.3](#023--2026-10-01) | 01/10/2026 | Robustez contra arquivos corrompidos e nomes com acento |
| [0.2.2](#022--2026-09-30) | 30/09/2026 | Todos os tamanhos de disco completo do Redump |
| [0.2.1](#021--2026-09-30) | 30/09/2026 | ISO enxuta reconhecida como cópia íntegra |
| [0.2.0](#020--2026-09-29) | 29/09/2026 | `.dat` do Redump integrados ao `verificar` |
| [0.1.0](#010--2026-09-29) | 29/09/2026 | Primeira versão |

---

## 0.2.4 — 2026-10-20

### Correções depois do lançamento
As correções entram na própria versão (o download da release é atualizado).

- **29/10 — Emojis e tamanhos:** os emojis que alguns terminais desenhavam
  com 1 coluna (⚠️, ⚙️, ⏱️...) desalinhavam as caixas; foram trocados por
  emojis que todo terminal desenha com 2 colunas. Arquivos com menos de
  1 KiB aparecem em bytes ("5 B"), não "0.00 KiB".

### Adicionado
- **`--cancelar-pela-entrada`:** fechar a entrada padrão cancela a operação
  e apaga o que ela criou, como o Ctrl+C. É para programas que chamam o
  extract-xiso-pt sem console: no Windows, um programa com janela não
  consegue mandar Ctrl+C, e encerrar o processo à força deixava arquivos
  pela metade. Vale para todos os comandos; sem a opção, nada muda.

### Alterado
- **Ajuda e erros de uso em português** ("Uso", "Opções", "argumento
  inesperado"...); antes essas partes saíam em inglês.
- A ajuda descreve os argumentos `<IMAGEM>` e `<ARQUIVOS>`.

### Segurança
- **Mais nomes reservados do Windows recusados:** `CONIN$`, `CONOUT$`,
  `COM0`, `LPT0`, `COM¹²³`, `LPT¹²³`, `CLOCK$`, `CONFIG$` e nomes com
  espaço antes do ponto (`CON .txt`). Uma imagem com um desses nomes passa a
  ser recusada, como já acontecia com `CON` e `COM1`; nenhum jogo conhecido
  usa esses nomes.

---

## 0.2.3 — 2026-10-01

### Corrigido
- **`reescrever` com nomes acentuados:** nomes como `Ação.wav` eram
  regravados com outros bytes na ISO nova, e o console procura o arquivo pelos
  bytes. Agora os nomes são copiados exatamente como estão na origem.

### Segurança
- ISOs, `.dat` e `.zip` malformados viram erro explicado em vez de travar ou
  esgotar a memória (ex.: diretórios apontando para as mesmas tabelas, ou um
  `.zip` pequeno que declarava terabytes).
- Com `--sobrescrever`, um link simbólico no destino não leva mais a extração
  para fora da pasta, e uma falha não apaga um arquivo já substituído.
- Fechar o terminal ou a janela no meio de uma operação cancela e apaga o que
  ela criou, como o Ctrl+C.
- Ctrl+C também funciona durante a leitura da árvore de arquivos.
- Os erros de leitura e gravação informam o arquivo e a operação.

### Desempenho
- Hashes do `verificar` até 45% mais rápidos em processadores sem instruções
  SHA.
- A extração lê os arquivos na ordem física do disco, o que ajuda em HD
  mecânico.

---

## 0.2.2 — 2026-09-30

### Corrigido
- **`verificar`:** o Redump cataloga discos de Xbox 360 com mais de um tamanho
  conforme a versão do disco (três no XGD2, dois no XGD3). Todos passam a ser
  reconhecidos como disco completo.

---

## 0.2.1 — 2026-09-30

### Corrigido
- **`verificar`:** uma XISO (sem a partição de vídeo) com o nome de um jogo do
  Redump era informada como "modificada ou corrompida". O Redump guarda o hash
  do disco completo; agora ela é informada como **cópia enxuta** (situação
  `enxuta` no JSON), com código de saída 0.

---

## 0.2.0 — 2026-09-29

### Adicionado
- **`dats instalar`:** instala os `.dat` do Redump, aceitando o `.zip` do site
  direto e substituindo a versão antiga do mesmo console.
- **`verificar` usa os `.dat` instalados** e informa se a imagem é a original,
  se não confere ou se não está no Redump.
- `dats` mostra os instalados; `--dat` compara com um arquivo específico;
  `--sem-dat` confere só a integridade.

### Validação
- Resident Evil 5 (XGD2) e Resident Evil 6 (XGD3) idênticos ao Redump.

---

## 0.1.0 — 2026-09-29

Primeira versão.

### Adicionado
- `info` e `listar`: layout (XISO, XGD1, XGD2, XGD3), console, data e
  conteúdo.
- `extrair`: idêntico byte a byte ao `extract-xiso` (conferido nos 3.696
  arquivos do Resident Evil 5).
- `criar`: pasta de jogo de Xbox ou Xbox 360 → XISO.
- `reescrever`: disco completo → XISO enxuta, com `--substituir`.
- `verificar`: estrutura, leitura completa, CRC32, MD5, SHA-1 e comparação com
  `.dat` do Redump.
- `--liberar-midia` (só Xbox, desligado por padrão).

### Segurança
- Nomes que sairiam da pasta de destino ou que o Windows não aceita, laços na
  árvore e trechos além do fim do arquivo são recusados antes de gravar.
- Ctrl+C ou falha apagam o que a operação criou.
- 28 testes, incluindo 3.000 imagens corrompidas de propósito.
