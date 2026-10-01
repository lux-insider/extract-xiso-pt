# extract-xiso-pt — plano

Ferramenta própria, em Rust, para imagens de disco de Xbox e Xbox 360
(XDVDFS): listar, extrair, criar, reescrever/otimizar e verificar. Escrita a
partir da documentação pública do formato e validada **de fora** contra o
`extract-xiso` do XboxDev (comparando saídas em imagens de teste), sem usar o
código-fonte dele.

## O formato (XDVDFS / GDF)

Tudo little-endian; setor = 2048 bytes.

**Onde começa o sistema de arquivos.** A partição pode estar em quatro
deslocamentos; o descritor de volume fica no setor 32 a partir dela:

| Layout | Deslocamento da partição | Console |
|---|---|---|
| XISO (sem partição de vídeo) | 0 | os dois |
| XGD1 | 0x18300000 | Xbox |
| XGD2 | 0x0FD90000 | Xbox 360 |
| XGD3 | 0x02080000 | Xbox 360 |

**Descritor de volume** (setor 32 da partição, 2048 bytes):

| Offset | Tamanho | Campo |
|---|---|---|
| 0x000 | 20 | `MICROSOFT*XBOX*MEDIA` |
| 0x014 | 4 | setor da tabela do diretório raiz |
| 0x018 | 4 | tamanho da tabela do diretório raiz |
| 0x01C | 8 | data de criação (FILETIME) |
| 0x7EC | 20 | `MICROSOFT*XBOX*MEDIA` de novo (fim do setor) |

**Tabela de diretório.** Uma árvore binária de entradas, gravada em setores
inteiros; uma entrada nunca atravessa o fim de um setor (o resto do setor é
preenchido com 0xFF). O nó raiz da árvore está no deslocamento 0 da tabela.

| Offset | Tamanho | Campo |
|---|---|---|
| 0x0 | 2 | filho da esquerda (em palavras de 4 bytes; 0 = nenhum) |
| 0x2 | 2 | filho da direita (idem) |
| 0x4 | 4 | setor inicial do arquivo (ou da tabela, se for diretório) |
| 0x8 | 4 | tamanho em bytes |
| 0xC | 1 | atributos (0x01 só leitura, 0x02 oculto, 0x04 sistema, 0x10 diretório, 0x20 arquivo, 0x80 normal) |
| 0xD | 1 | tamanho do nome |
| 0xE | n | nome |
| … | | preenchimento até múltiplo de 4, com 0xFF |

A árvore é ordenada pelo nome, sem diferenciar maiúsculas. Um diretório vazio
tem tabela de tamanho 0 ou um setor inteiro de 0xFF (sem nó na posição 0) —
o `extract-xiso` grava assim, e o `criar` também.

## Decisões de segurança (valem para todo comando)

1. **Vale a árvore, não a varredura.** Só existe o que se alcança a partir da
   raiz pelos ponteiros esquerda/direita. Restos de entradas antigas na
   tabela são ignorados.
2. **Ciclos e profundidade.** Um nó visitado duas vezes, um diretório que
   aponta para um ancestral ou mais de 64 níveis param a leitura com erro
   explicado, sem travar.
3. **Nada fora do destino.** Nome vazio, `.`, `..`, com `/`, `\`, `:` ou
   caractere de controle é recusado **antes** de criar qualquer arquivo. Dois
   nomes que só diferem em maiúsculas no mesmo diretório também, porque no
   Windows seriam o mesmo arquivo. A extração não desce por um link
   simbólico (ou junção) que já esteja no destino, e o temporário é criado
   de forma exclusiva, sem seguir link.
4. **Tamanhos conferidos antes de alocar.** Tabela ou arquivo que passa do
   fim da imagem é erro, não alocação de gigabytes. De uma tabela só se lê o
   que os ponteiros alcançam, e a imagem inteira tem teto de tabelas lidas.
5. **Sem meio-termo no disco.** Ctrl+C, SIGTERM, SIGHUP, fechar a janela
   do console ou falha no meio apaga o que a extração criou; um arquivo pela
   metade nunca fica com o nome de pronto (grava em `nome.parcial` e
   renomeia no fim).
6. **Nunca altera o jogo sem pedir.** O patch de mídia do XBE só com opção
   explícita.

## Etapas

Etapas 1 a 4 prontas; falta a 5.

1. Leitura: detectar layout, ler o descritor, percorrer a árvore — `info`,
   `listar` (texto e `--json`).
2. `extrair`, com destino sempre explícito ou ao lado da ISO, progresso real
   (`--progresso-json`), Ctrl+C limpo.
3. `criar` a partir de pasta (Xbox e Xbox 360) e `reescrever`/otimizar
   (grava num temporário e só troca o original se tudo deu certo).
4. `verificar`: estrutura (sobreposição, fora do volume, leitura até o fim),
   hash CRC32/MD5/SHA-1 contra um `.dat` do Redump.
5. Integração no xiso-manager, no lugar do extract-xiso.

## Validação

- Testes com imagens sintéticas montadas nos próprios testes (árvores
  balanceadas e degeneradas, nomes no limite, entradas hostis).
- Fuzz com imagens corrompidas (sem pânico, sem travamento, sem escrever
  fora do destino).
- Caixa-preta contra o `extract-xiso` oficial: mesma lista de arquivos e
  mesmos bytes extraídos em ISOs criadas por ele e na ISO do Resident Evil 5.
