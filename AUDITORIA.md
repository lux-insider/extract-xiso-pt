# Auditoria do extract-xiso-pt 0.2.2

Escopo: todo o código em `src/` e o `Cargo.toml` da versão 0.2.2 (commit
`191bb8d`). Os números de linha citados são dessa versão.

**Regra que manda em tudo:** a ferramenta já foi validada em jogos reais.
Nenhuma correção pode mudar os bytes gravados hoje para entradas válidas.
Por isso, antes de qualquer correção, `src/testes_saida.rs` passou a guardar
o SHA-1 do que cada comando produz hoje (extrair, criar, reescrever,
`--liberar-midia`, `listar --json` e o relatório do `verificar`, em XISO e
XGD3, com nomes UTF-8 e Latin-1). Toda correção abaixo passa por esses
testes sem alterar um único valor. Uma correção que mudaria esses bytes não
foi aplicada: ficou só descrita (marcada **só descrito**). A única exceção é
o A-6, corrigido depois com autorização do mantenedor: ele muda de propósito
três valores golden do `reescrever` com nome acentuado (ver o item).

## Gravidade

| Nível | Critério |
|---|---|
| **Crítica** | grava, sobrescreve ou apaga arquivo fora do que o usuário pediu |
| **Alta** | trava, esgota a memória ou aborta com uma entrada pequena; deixa lixo de vários GB num caso comum; muda dados do jogo sem aviso |
| **Média** | erro que não explica o que falhou; comportamento errado em caso raro; ganho de desempenho claro |
| **Baixa** | robustez em caso artificial, ganho pequeno, cosmético |

## Resumo

| # | Gravidade | Onde | Problema | Decisão |
|---|---|---|---|---|
| S-1 | Crítica | `extrair.rs:132-138` | link simbólico no destino (`--sobrescrever`) faz gravar fora dele | corrigido em `847103f` |
| S-2 | Crítica | `extrair.rs:153-157`, `criar.rs:599-602, 644` | `.parcial` que é link: sobrescreve o arquivo apontado | corrigido em `d6a8cd1` |
| S-3 | Crítica | `dats.rs:178-183` | `dats instalar` apaga arquivo fora da pasta dos .dat | corrigido em `dbebc4c` |
| B-1 | Alta | `dats.rs:209-265` | .zip sem limite de total descompactado (3,4 MB → 3,8 TiB) | corrigido em `1429d3e` |
| B-2 | Alta | `dats.rs:68` | `.dat` lido inteiro antes de conferir o tamanho (`/dev/zero`, ISO) | corrigido em `32b0afe` |
| B-3 | Alta | `arvore.rs:96-185` | tabela compartilhada relida (até 16 MB) a cada visita: trava | corrigido em `92fdeba` |
| S-4 | Alta | `progresso.rs:39,152`, `terminal.rs`, `main.rs` | stdout fechado vira pânico e aborto, sem limpeza | corrigido em `6d6b741` |
| S-5 | Alta | `sistema.rs:72-114` | SIGHUP e fechar a janela no Windows matam sem limpar | corrigido em `050039a` |
| A-6 | Alta | `criar.rs:176-187` | `reescrever` troca nomes UTF-8 acentuados por Latin-1 | corrigido em `0df4031` (autorizado; muda bytes do `reescrever`) |
| B-4 | Média | `arvore.rs`, `criar.rs:69-155` | Ctrl+C/SIGTERM ignorados durante a leitura da árvore | corrigido em `4c51c7c` |
| E-1 | Média | `erro.rs:9` e todo `?` em E/S | erro de E/S sem dizer arquivo nem operação | corrigido em `be00f0b` |
| E-2 | Média | `main.rs:45-57` | `verificar --progresso-json` não emite o evento `erro` | corrigido em `7e5822b` |
| E-3 | Média | `main.rs:44` | pânico sai em inglês; erro de uso sai com código 2 (= "não confere") | corrigido em `7e5822b` |
| S-6 | Média | `extrair.rs:108-118, 177-181` | falha com `--sobrescrever` apaga o arquivo que substituiu o do usuário | corrigido em `33ec972` |
| P-1 | Média | `Cargo.toml` | `opt-level = "z"` deixa o SHA-1 por software 45% mais lento | corrigido em `a3ffd16` |
| P-6 | Média | `extrair.rs:121-144` | extração lê na ordem alfabética, não na do disco | corrigido em `ede49db` |
| S-7 | Baixa | `extrair.rs:153-181` | nome temporário pode colidir com outro arquivo da imagem | corrigido em `ede49db` |
| S-8 | Baixa | `criar.rs:599-602` | temporário da saída igual à imagem de origem a destrói | corrigido em `a8d7a98` |
| S-9 | Baixa | `extrair.rs:91-93` | `-d a/b/c` sem `a`: falha deixa `a` e `a/b` vazias | corrigido em `1ddfb31` |
| V-1 | Baixa | `verificar.rs:104-116` | sobreposição só detectada entre trechos vizinhos | corrigido em `5c35a25` |
| V-2 | Baixa | `verificar.rs:160-188` | leitura dos hashes sem limite se o arquivo trocar no meio | corrigido em `7e03bc2` |
| T-1 | Baixa | `main.rs:270-282`, `terminal.rs:1016` | nomes da imagem com controles C1/bidi vão crus ao terminal | corrigido em `0b34022` |
| P-2 | Baixa | `verificar.rs:166` | 4 MB zerados alocados por bloco lido | corrigido em `2f98475` |
| P-3 | Baixa | `extrair.rs:163` | buffer de até 1 MB zerado por arquivo extraído | corrigido em `ede49db` |
| P-4 | Baixa | `arvore.rs:133` | lê a tabela declarada inteira; só 262.409 bytes são alcançáveis | corrigido em `92fdeba` |
| P-5 | Baixa | `main.rs:266, 270-282` | `listar` faz uma escrita por linha e monta o JSON inteiro na memória | corrigido em `f0f5233` (texto); JSON ver nota |
| L-1 | Baixa | `Cargo.toml`, `terminal.rs:302-306` | `terminal_size` puxa `rustix`, `linux-raw-sys`, `bitflags` | corrigido em `c24b935` |
| W-1 | Baixa | `arvore.rs:277-303` | nomes reservados do Windows incompletos | **só descrito** |
| D-1 | Baixa | `extrair.rs:175-177` | sem `fsync` antes do `rename` | **só descrito** |
| D-2 | Baixa | `criar.rs:564`, `extrair.rs:177` | conferência de "já existe" e `rename` não são atômicos | **só descrito** |
| D-3 | Baixa | `sistema.rs:53` | segundo Ctrl+C sai na hora e deixa `.parcial` | **só descrito** (proposital) |
| D-4 | Baixa | `arvore.rs:162` | NFC/NFD no macOS: dois nomes viram o mesmo arquivo | **só descrito** |
| P-7 | Baixa | `criar.rs:684-749` | `reescrever` lê a origem fora da ordem do disco | **só descrito** |
| L-2 | Baixa | `Cargo.toml` | `clap` é a maior dependência | **só descrito** |

Os itens estão detalhados abaixo com o cenário, a correção e o teste. A
seção [Resultado](#resultado-da-fase-2) no fim resume as medições, os
testes e o que fica para decisão do mantenedor. O S-9 foi encontrado
durante a fase 2, na revisão do próprio diff.

---

## 1. Bugs ocultos com entradas corrompidas, truncadas ou malformadas

O que já estava bem feito, conferido linha a linha e mantido: a árvore é
lida sem recursão por nó (`em_ordem`, pilha explícita); ciclos dentro de
uma tabela e diretório que aponta para um ancestral viram erro; a
profundidade é limitada a 64; todo trecho é conferido contra o fim do
volume **antes** de alocar; `ler_no` usa `get` em todos os acessos; o leitor
de XBE usa aritmética conferida (`checked_sub`/`checked_add`); o leitor de
zip confere toda posição com `get`. Não achei estouro de inteiro alcançável:
`setor as u64 * 2048 + tamanho` cabe folgado em `u64`, `(p / 4) as u16` em
`criar.rs:305` só roda depois de `montar_tabela` recusar tabelas acima de
`0xFFFF * 4`, e as somas do leitor de zip partem de campos de 16 e 32 bits
em `usize` de 64 bits. O fuzz existente (3000 imagens corrompidas) passa.

### B-1 (Alta) — .zip sem limite de total descompactado

`dats.rs:209-265`. Cada entrada é limitada a `MAX_DAT` (64 MB), mas o
número de entradas vem do arquivo (até 65.535) e nada impede que todas as
entradas do diretório central apontem para o **mesmo** cabeçalho local.
Todas ficam juntas na memória em `saida`.

*Cenário (reproduzido):* um `.zip` de 3,4 MB com uma entrada deflate de 60
MB repetida 65.535 vezes declara 3,8 TiB. `verificar --dat bomba.zip` (ou
`dats instalar bomba.zip`) aborta com "memory allocation failed" (status
134); sem limite de memória, derruba a máquina antes.

*Correção:* somar o tamanho declarado de todas as entradas e recusar antes
de descompactar quando passar de `MAX_DAT`. Um .zip do Redump tem um .dat
de poucos MB: nada muda para ele.

### B-2 (Alta) — .dat lido inteiro antes de conferir o tamanho

`dats.rs:68`: `fs::read(caminho)?` e só depois `bytes.len() > MAX_DAT`.

*Cenário (reproduzido):* `verificar jogo.iso --dat /dev/zero` lê até
esgotar a memória ("erro de E/S: out of memory" com `ulimit` de 2 GB).
Também `--dat jogo.iso` (8 GB para a memória) e, pior, qualquer arquivo
grande ou link para `/dev/zero` chamado `*.dat` na pasta dos .dat: ele é
carregado em **todo** `verificar`, sem `--dat`.

*Correção:* ler com `take(MAX_DAT + 1)` e recusar se passar.

### B-3 (Alta) — tabela de diretório compartilhada: travamento

`arvore.rs:96-185`. `ancestrais` impede só ciclos no caminho atual. Dois
diretórios podem apontar para a mesma tabela, e cada visita relê a tabela
inteira declarada — até 16 MB (`MAX_TABELA`), embora só os primeiros
`0xFFFF * 4 + 14 + 255 = 262.409` bytes sejam alcançáveis pelos ponteiros
de 16 bits. Com dois diretórios por nível apontando para o mesmo filho, o
número de visitas dobra a cada nível.

*Cenário (reproduzido):* uma imagem de 16 MB com 40 níveis (cada tabela com
dois diretórios declarando 16 MB e apontando para a tabela seguinte) deixa
`info` rodando por mais de 90 s, relendo 16 MB por visita, e só morre com
SIGKILL (ver B-4). `MAX_ENTRADAS` não ajuda: cada visita custa 16 MB de
leitura antes de contar uma entrada.

*Correção, sem mudar o resultado de nenhuma imagem:* (1) ler da tabela só o
prefixo alcançável (`min(tamanho, 262.409)`); a conferência contra o fim do
volume continua usando o tamanho declarado inteiro, e qualquer nó alcançável
está dentro do prefixo, então a árvore lida é a mesma byte a byte; (2) um
orçamento de 256 MiB de tabelas lidas na imagem inteira — um disco real lê
poucos MB. O teste monta a imagem do cenário e exige resposta em segundos.

### B-4 (Média) — cancelamento ignorado durante a leitura da árvore

`arvore.rs` e `criar.rs:69-155` (leitura da pasta) não consultam
`sistema::cancelado()`. O SIGTERM e o primeiro Ctrl+C só marcam o
cancelamento; numa imagem como a de B-3 o processo não para (o `timeout 60`
do teste acima não conseguiu encerrá-lo). *Correção:* consultar o
cancelamento a cada tabela lida e a cada pasta lida.

### V-2 (Baixa) — leitura dos hashes sem limite

`verificar.rs:160-188` lê até o fim do arquivo. O tamanho é conferido no
fim, mas se o caminho for trocado entre a abertura da estrutura e a dos
hashes (por um link para `/dev/zero`, por exemplo), a leitura não termina.
*Correção:* ler no máximo `tamanho + 1` bytes; o erro "o arquivo mudou
durante a leitura" continua igual.

### V-1 (Baixa) — sobreposição de trechos só entre vizinhos

`verificar.rs:104-116` compara cada trecho só com o seguinte na ordem do
setor. Um arquivo grande que contém dois menores tem a segunda sobreposição
ignorada (A=[0,100), B=[10,20), C=[30,40): A–C passa sem aviso).
*Correção:* comparar com o maior fim visto até ali. Trechos vizinhos
continuam gerando o mesmo aviso (o teste golden do XGD3 tem um).

---

## 2. Segurança de arquivos

### S-1 (Crítica) — link simbólico no destino: gravação fora dele

`extrair.rs:132-138`: `if !alvo.is_dir()` segue links. Com
`--sobrescrever` num destino que já tem `media -> /home/usuario/.config`,
todo o conteúdo de `media/` da imagem vai para fora do destino. No Windows,
o mesmo vale para junções.

*Cenário (reproduzido):* destino com `media` apontando para outra pasta:
os 3000 arquivos de `media/` foram gravados nela. Isso contradiz a promessa
do README ("nada é gravado fora do destino").

*Correção:* usar `symlink_metadata`; um link (ou junção) no caminho de um
diretório da imagem vira erro explicado, antes de gravar dentro dele.

### S-2 (Crítica) — `.parcial` que é link sobrescreve o arquivo apontado

`extrair.rs:153-157` e `criar.rs:644` usam `File::create`, que segue links
e trunca. `criar.rs:599-602` monta o nome `saida.extract-xiso-pt.parcial`.

*Cenário (reproduzido):* um destino com
`default.xex.extract-xiso-pt.parcial -> ~/alvo.txt`. A extração
sobrescreveu `alvo.txt` com o conteúdo do jogo e, no `rename`, o link virou
o `default.xex` extraído (um link para fora do destino).

*Correção:* apagar o que estiver no caminho do temporário (`remove_file`
apaga o link, não o alvo) e criar com `create_new` (`O_EXCL`, que não segue
link). Um `.parcial` esquecido por uma execução anterior (Ctrl+C duplo,
queda de energia) é limpo assim que a mesma extração roda de novo.

### S-3 (Crítica) — `dats instalar` apaga arquivo fora da pasta dos .dat

`dats.rs:178-183` apaga `destino.join(&velho.arquivo)`, mas `arquivo` é o
nome **de dentro do .zip** quando um `.dat` instalado é na verdade um .zip
(`ler_arquivo` reconhece pelo `PK\x03\x04`). Um nome absoluto ou com `..`
dentro do zip escapa da pasta. E num sistema de arquivos que não diferencia
maiúsculas, reinstalar `X.dat` como `x.dat` apaga o arquivo recém-gravado.

*Cenário (reproduzido):* `dats/disfarcado.dat` (um zip cuja entrada se chama
`/caminho/vitima/importante.dat`); `dats instalar novo.dat` do mesmo sistema
apagou `/caminho/vitima/importante.dat` e deixou `disfarcado.dat` no lugar.

*Correção:* `instalados()` guarda o nome real do arquivo na pasta; só se
apaga um nome simples, que existe na pasta, e nunca um igual ao recém
instalado sem diferenciar maiúsculas. Os nomes vindos do zip passam por
`validar_nome` (nada de `CON.dat` ou `a.dat:fluxo` no Windows).

### S-4 (Alta) — saída padrão fechada: pânico e aborto sem limpeza

`progresso.rs:39,152`, `terminal.rs` e `main.rs` usam `println!`, que entra
em pânico se a escrita falha; com `panic = "abort"` o processo morre na
hora, sem apagar o que criou.

*Cenário (reproduzido):* `listar jogo.iso | head -1` imprime o pânico em
inglês e sai com 134. O caso grave é o `--progresso-json`: se o programa
que lê o progresso (o xiso-manager) fechar o pipe ou morrer no meio da
extração, o próximo evento derruba o processo e ficam o `.parcial` e as
pastas criadas. No Windows o mesmo acontece ao fechar a janela (S-5), e
também com o `eprintln!`.

*Correção:* eventos, barra e mensagens escrevem ignorando erro de escrita
(a operação segue e limpa ou termina normalmente); `listar` e `info` param
em silêncio quando o leitor fecha o pipe.

### S-5 (Alta) — SIGHUP e fechar a janela no Windows: morte sem limpeza

`sistema.rs:72-114`. Só SIGINT e SIGTERM são tratados. Fechar o terminal
(SIGHUP) mata o processo; no Windows, `tratar_console` devolve `FALSE` para
`CTRL_CLOSE_EVENT`, `CTRL_LOGOFF_EVENT` e `CTRL_SHUTDOWN_EVENT`, e o sistema
encerra o processo na hora.

*Cenário (reproduzido no Linux):* SIGHUP no meio de uma extração deixou
`hup/dados.bin.extract-xiso-pt.parcial` (69 MB) e a pasta. No `criar`/
`reescrever`, sobra a imagem `.parcial` inteira — vários GB.

*Correção:* SIGHUP tratado como SIGTERM. No Windows, nos três eventos de
encerramento o tratador marca o cancelamento e espera (o Windows dá 5 s)
para a thread principal apagar o que criou e sair; quem chama `exit`
encerra o processo antes do prazo. Só funciona junto com S-4 (com a janela
fechada, escrever no console falha).

### S-6 (Média) — falha com `--sobrescrever` apaga o arquivo que substituiu o do usuário

`extrair.rs:177-181` registra o arquivo final em `criados` mesmo quando o
`rename` substituiu um arquivo que já existia. Se a extração falhar depois,
`desfazer` apaga esse caminho: o usuário fica sem a versão antiga (trocada
no `rename`) e sem a nova. *Correção:* um arquivo que substituiu outro não
entra na lista do desfazer (ele está completo e é o que a imagem tem).

### S-7 (Baixa) — colisão do nome temporário

O temporário de `x` é `x.extract-xiso-pt.parcial`. Se a imagem tiver também
um arquivo com esse nome e ele for extraído antes (tabela fora de ordem, ou
a ordem do disco de P-6), o temporário de `x` trunca o arquivo já pronto e
ele some do resultado. *Correção:* se o caminho do temporário for um
arquivo que esta extração já gravou, usa-se outro nome (`.1.parcial`, ...).

### S-9 (Baixa) — pastas-pai criadas pelo `-d` ficavam após uma falha

`extrair.rs:91-93`: com `-d a/b/c` e `a` inexistente, `create_dir_all`
criava `a`, `a/b` e `a/b/c`, mas só `a/b/c` entrava na lista do desfazer.
Um Ctrl+C ou uma falha deixava `a` e `a/b` vazias. *Correção:* todas as
pastas criadas entram na lista (o desfazer só apaga pasta vazia).

### S-8 (Baixa) — temporário da saída igual à imagem de origem

`reescrever a.iso.extract-xiso-pt.parcial -s a.iso` grava o temporário em
cima da própria origem enquanto lê dela. *Correção:* recusar quando o
temporário é o mesmo arquivo que a origem.

### W-1 (Baixa, só descrito) — nomes reservados do Windows incompletos

`arvore.rs:296` não tem `CONIN$`, `CONOUT$`, `COM0`/`LPT0`, `COM¹²³`/
`LPT¹²³` nem trata espaço antes do ponto (`CON .txt`). Nenhum jogo real usa
esses nomes, mas recusá-los muda a validação de entrada (uma imagem que hoje
lista e extrai no Linux passaria a ser recusada). Proposta: recusar só na
extração no Windows, ou acrescentar à lista numa versão anunciada.

### D-1, D-2, D-3, D-4 (Baixa, só descritos)

- **D-1** sem `fsync` antes do `rename` na extração: numa queda de energia
  o arquivo pode aparecer com o nome final e conteúdo vazio (ext4 com
  alocação atrasada). Um `fsync` por arquivo deixaria a extração de milhares
  de arquivos bem mais lenta; uma opção `--sincronizar` resolveria para
  quem quer. O `criar` já faz `sync_all` na imagem.
- **D-2** `criar.rs:564` confere "já existe" e só no fim faz o `rename`, que
  substitui. Um arquivo criado por outro programa nesse meio-tempo seria
  trocado. Proposta: `renameat2(RENAME_NOREPLACE)` no Linux e `MoveFileExW`
  sem `REPLACE_EXISTING` no Windows quando não há `--sobrescrever`.
- **D-3** o segundo Ctrl+C chama `_exit(130)` e deixa os `.parcial`: é a
  saída de emergência, de propósito. Com S-2, rodar a mesma operação de novo
  limpa esses restos.
- **D-4** no macOS (APFS), `é` em NFC e em NFD são o mesmo arquivo; a
  validação de nomes iguais compara sem diferenciar maiúsculas, não por
  normalização. Fora do escopo (o programa não tem binário para macOS).

### A-6 (Alta, corrigido depois) — `reescrever` troca nomes UTF-8 por Latin-1

`criar.rs:176-187`, `bytes_do_nome`. A leitura decodifica o nome como UTF-8
se for válido, senão como Latin-1, e guarda só o texto. Para voltar aos
bytes, `bytes_do_nome` escolhe Latin-1 sempre que todos os caracteres cabem
em um byte e o resultado não é UTF-8 válido. Só que um nome UTF-8 com
acentos do português (todos até U+00FF) cai exatamente nesse caso.

*Cenário (encontrado pelos testes golden):* `criar` grava `Ação.wav` em
UTF-8 (`41 C3 A7 C3 A3 6F …`, 10 bytes); `reescrever` dessa imagem grava
`41 E7 E3 6F …` (8 bytes). Na extração o nome sai igual (as duas formas
decodificam para o mesmo texto), por isso passou despercebido; mas no
console o jogo procura o arquivo pelos bytes. Discos oficiais só têm nomes
ASCII e não são afetados; traduções e imagens caseiras com acento são. O
caso inverso também existe: um nome Latin-1 cujos bytes formam UTF-8 válido
(`C3 A9`) volta como `E9`.

*Por que ficou de fora na primeira rodada:* a correção muda os bytes que o
`reescrever` grava para uma entrada válida, e a regra era não mudar nenhum.

*Correção (`0df4031`, aplicada depois, com autorização):* `Entrada` guarda
também os bytes do nome lidos do disco (`#[serde(skip)] nome_bytes`, fora
do JSON do `listar`), e `de_imagem` os copia como estão; `bytes_do_nome`
saiu. Extração, validação de nomes e texto mostrado não mudam.

Mudaram de propósito três valores golden, todos de `reescrever` com nome
acentuado; os outros 10 (extração, `criar`, `listar --json`, `verificar`,
`--liberar-midia`) continuam iguais:

| Golden | Antes | Depois | Por quê |
|---|---|---|---|
| `reescrever (XISO)` | `441bf9dc…` | `ef1cd9ba…` | agora é o mesmo hash do `criar` da mesma pasta: a imagem reescrita é idêntica à criada |
| `reescrever (XGD3)` | `1b19cdc9…` | `c94a0d5b…` | só o nó de `Ação.txt` muda: tamanho do nome 8 → 10, bytes `41 E7 E3 6F…` → `41 C3 A7 C3 A3 6F…` |
| `reescrever -u (XGD3)` | `cabdeee0…` | `73cacfe6…` | o mesmo nó; o nome Latin-1 `Aé.bin` já saía igual |

Testes (`a6_*` em `src/testes_auditoria.rs`), que leem os nomes direto das
tabelas gravadas, sem passar pelo leitor do programa:

- (a) nome UTF-8 com acento, numa pasta e num arquivo dentro dela: a
  reescrita de uma imagem criada é idêntica a ela. Falhava antes.
- (b) nomes Latin-1 que não são UTF-8 válido voltam com os mesmos bytes.
  Passava também antes, porque a adivinhação antiga acertava esse caso;
  fica como proteção contra regressão.
- (c) nomes Latin-1 cujos bytes formam UTF-8 válido (`43 C3 A9`) voltam
  iguais. Falhava antes (saíam como `43 E9`).

---

## 3. Tratamento de erros

### E-1 (Média) — erro de E/S sem arquivo nem operação

`erro.rs:9` tem `Io(#[from] std::io::Error)`, e todo `?` em E/S cai nele.
O usuário vê "erro de E/S: No such file or directory (os error 2)" sem
saber qual arquivo nem o que se tentava (abrir a imagem? criar a pasta?
renomear o temporário?). Exemplos: `imagem.rs:65`, `extrair.rs:69,92,135,
157,169,170,177`, `criar.rs:88,96,118,459,644,669,712`, `main.rs:430,455`,
`dats.rs:68,161,174,175`, `verificar.rs:148,353`.

*Correção:* o `From<io::Error>` sai; em seu lugar, `Erro::Arquivo {
operacao, caminho, fonte }` e `Erro::Renomear { de, para, fonte }`, com a
causa traduzida para os casos comuns (não existe, sem permissão, disco
cheio, arquivo em uso...). Sem o `From`, o compilador obriga cada chamada a
dizer o que fazia. As mensagens de imagem corrompida, nome inseguro e
destino não mudam.

### E-2 (Média) — `verificar --progresso-json` sem evento de erro

`main.rs:45-57` só liga o modo JSON de erro para `extrair`, `criar` e
`reescrever`. Um erro em `verificar --progresso-json` sai só como texto no
stderr: quem lê o protocolo não recebe o evento `erro`. *Correção:* emitir
o evento também no `verificar` (mantendo a mensagem no stderr, que era o
que esse modo fazia).

### E-3 (Média) — pânico em inglês e código 2 para erro de uso

Um pânico (bug) imprime "thread 'main' panicked at …" em inglês e aborta.
E o `clap` sai com código 2 num erro de uso, o mesmo código que o
`verificar` usa para "não confere": um xiso-manager mais novo passando uma
opção que esta versão não conhece leria "imagem modificada". *Correção:*
gancho de pânico com mensagem em português (e evento `erro` no modo JSON);
erro de uso sai com 1, como diz a tabela do README. `--help` e `--version`
continuam saindo com 0.

---

## 4. Memória e velocidade

### P-1 (Média) — `opt-level = "z"` e os hashes

Medido com uma imagem de 1 GiB (média de 2 execuções, 4 núcleos):

| Perfil | Binário | `verificar` (SHA-NI) | `verificar` (SHA-1 por software) |
|---|---|---|---|
| `z` (atual) | 863.272 B | 2,04 s | **2,95 s** |
| `z` + `opt-level = 3` só em `md-5`, `sha1`, `crc32fast`, `digest`, `block-buffer` | 862.616 B | 2,03 s | **2,03 s** |
| `3` em tudo | 1.071.128 B | 2,01 s | 1,99 s |

Numa CPU com SHA-NI o gargalo é o MD5 (~500 MB/s, inerente ao algoritmo) e
o perfil não importa. Sem SHA-NI (boa parte dos Intel de desktop até a 10ª
geração), o SHA-1 compilado para tamanho vira o gargalo. *Correção:*
`opt-level = 3` só nos crates de hash. O binário não cresce (fica 656 bytes
menor) e o hash mais lento volta a ser o MD5.

### P-6 (Média) — extração fora da ordem do disco

`extrair.rs:121-144` extrai na ordem da árvore (alfabética). Num disco
real o conteúdo não está em ordem alfabética, e cada troca de arquivo vira
um salto da cabeça de leitura num HD (ou num DVD montado). *Correção:*
criar as pastas primeiro e extrair os arquivos em ordem de setor. Os bytes
e nomes gravados são os mesmos (os testes golden de extração conferem);
muda só a ordem em que os arquivos aparecem no progresso. Num SSD ou com a
imagem no cache o ganho é nulo; num HD é a diferença entre leitura
sequencial e milhares de buscas.

### P-7 (Baixa, só descrito) — `reescrever` lê a origem fora de ordem

A imagem nova é gravada em sequência na ordem da árvore (e tem que
continuar assim: é o layout validado). Ler a origem em ordem de setor
exigiria gravar fora de ordem num arquivo pré-alocado; no NTFS isso força o
preenchimento com zeros até cada posição (o dobro de escrita). Não vale o
risco.

### P-2, P-3, P-4, P-5 (Baixa)

- **P-2** `verificar.rs:166` aloca e zera 4 MB por bloco (cerca de 2000
  vezes numa imagem de 8 GB). *Correção:* as threads de hash devolvem os
  blocos por um canal de volta e eles são reaproveitados.
- **P-3** `extrair.rs:163` aloca e zera até 1 MB por arquivo. *Correção:* um
  buffer só para a extração inteira.
- **P-4** `arvore.rs:133` lê a tabela declarada inteira (até 16 MB) — ver B-3.
- **P-5** `main.rs:270-282` imprime cada linha do `listar` com `println!`
  (uma escrita no terminal por linha) e `main.rs:266` monta o JSON inteiro
  numa `String` antes de imprimir. *Correção:* `BufWriter`, com a mesma
  saída byte a byte. O JSON chegou a sair com `serde_json::to_writer`, mas
  isso criava uma segunda cópia do serializador no binário (7 KB); ficou o
  `to_vec` da 0.2.2, que tem teto (um milhão de entradas). Ver
  [Medições](#medições).

Clones e `String`s: o resto é pequeno perto da E/S (um `format!` de caminho
por entrada, `jogo.clone()` por rom no .dat). Mexer neles não muda o tempo
de nenhum comando de forma mensurável; ficaram como estão.

---

## 5. Concorrência e E/S

- **`verificar`** já faz o certo: uma thread lê, três calculam CRC32, MD5 e
  SHA-1 em paralelo, com fila curta (4 blocos). Com P-1 e P-2 o tempo é o
  do MD5. Não vale mais threads: o MD5 é sequencial por natureza.
- **`extrair`**: uma thread de leitura e outra de escrita com dois buffers
  só ajudaria com origem e destino em discos físicos diferentes e o cache
  de escrita do sistema saturado. O ganho é pequeno (o sistema já grava em
  segundo plano), e o custo é dividir o cancelamento e o desfazer entre
  threads. Não vale agora; P-6 dá mais ganho com risco menor.
- **Não paralelizar:** extrair vários arquivos ao mesmo tempo da mesma
  imagem (num HD, as leituras concorrentes viram buscas) e gravar a imagem
  nova em paralelo (o layout é sequencial e validado).
- **`criar` de uma pasta com milhares de arquivos pequenos:** abrir o
  próximo arquivo enquanto o atual é copiado ajudaria pouco; fica como
  ideia.

## 6. Binário leve

Dependências diretas: `clap`, `serde`/`serde_json`, `thiserror`,
`unicode-width`, `terminal_size`, `crc32fast`, `md-5`, `sha1`,
`miniz_oxide`, `libc` e `windows-sys`.

- **L-1 (corrigir)** `terminal_size` existe só para saber a largura do
  terminal, e puxa `rustix`, `linux-raw-sys` e `bitflags` no Linux (e um
  segundo `windows-sys` no Windows). `libc` (`ioctl(TIOCGWINSZ)`) e
  `windows-sys` (`GetConsoleScreenBufferInfo`) já são dependências.
- **L-2 (só descrito)** `clap` é a maior parte do binário. Um leitor de
  argumentos à mão economizaria algumas centenas de KB, mas muda a ajuda,
  as mensagens de erro e as sugestões ("você quis dizer…"). Tirar as
  features `color` e `suggestions` economiza menos e também muda o que o
  usuário vê. Fica para decisão do mantenedor.
- `thiserror` não pesa no binário (só gera código em tempo de compilação).
- `md-5`, `sha1`, `crc32fast`, `miniz_oxide` e `serde_json` são o mínimo
  para o que o programa faz.
- Perfil: `lto = true`, `codegen-units = 1`, `panic = "abort"` e
  `strip = true` ficam; com P-1, os trechos pesados (hashes) ficam com
  `opt-level = 3` sem aumentar o binário.

---

## Resultado da fase 2

Um commit por correção, da gravidade mais alta para a mais baixa, cada
uma com o teste do cenário. Os
testes de correção foram conferidos nos dois sentidos: falham no código de
antes (os de travamento foram mortos por tempo: B-3 passou de 45 s, V-2 de
60 s) e passam no de depois.

### A regra principal

Os 13 valores de `src/testes_saida.rs`, gravados com a 0.2.2 antes de
qualquer correção, continuam idênticos depois de todas elas, em
`cargo test` e em `cargo test --release` (o perfil que vai para o usuário,
com o P-1). A listagem em texto e em JSON do `listar` também foi comparada
com o binário 0.2.2 numa imagem de 60 mil arquivos: mesmo SHA-1.

### Testes e clippy

| | Antes | Depois |
|---|---|---|
| Testes de unidade | 32 | 58 |
| Testes do programa (`tests/cli.rs`) | 0 | 6 |
| `cargo clippy --all-targets` (Linux) | sem avisos | sem avisos |
| `cargo clippy --all-targets --target x86_64-pc-windows-msvc` | sem avisos | sem avisos |

O alvo Windows foi compilado e passou no clippy, mas não foi executado
(não há Windows nem Wine neste ambiente). Ficam sem execução real: o
tratador de `CTRL_CLOSE_EVENT` (S-5), a largura do console por
`GetConsoleScreenBufferInfo` (L-1) e `mesmo_arquivo` por caminho canônico
(S-8). Vale um teste manual no Windows antes do próximo release: fechar a
janela no meio de um `extrair` e conferir que a pasta some.

Um efeito colateral do B-4 apareceu na suíte: o pedido de cancelamento é
global, e um teste que o marcava fazia outro, rodando em paralelo, receber
`Cancelado` de vez em quando. Nos testes, o pedido simulado agora vale só
para a thread que o fez (`36839f0`); a suíte passou 5 vezes seguidas.

### Medições

- **P-1:** `verificar` de 1 GiB com SHA-1 por software: 2,9-3,2 s → 2,0 s.
  Com SHA-NI, igual (o gargalo é o MD5).
- **P-5:** `listar` de 60 mil arquivos num pipe: 60.023 → 208 chamadas
  `write`, cerca de 40% mais rápido.
- **P-2:** sem diferença no Linux (o glibc já reaproveitava a memória); o
  ganho esperado é no Windows, onde não pôde ser medido.
- **P-6:** sem diferença num SSD com a imagem no cache, como esperado; o
  ganho é num HD ou DVD, onde não pôde ser medido.
- **Binário** (Linux x86_64, release): 863.272 → 885.976 bytes (+2,6%).
  Medido commit a commit, as mensagens de erro estruturadas (E-1) são a
  maior parte. Dois ajustes recuperaram 4,8 KB (`85463d8`); por isso o
  `listar --json` continua montando o texto do JSON na memória, como na
  0.2.2 (o `to_writer` custaria 7 KB, e a árvore tem teto de um milhão de
  entradas). O L-1 tirou cinco pacotes da compilação.

### Para decisão do mantenedor (não aplicado)

1. **W-1** — nomes reservados do Windows que faltam na validação.
2. **L-2** — trocar ou enxugar o `clap` (muda ajuda e mensagens de uso).
3. **D-1** — opção `--sincronizar` (fsync por arquivo) para quem extrai em
   disco externo e pode desconectar.
4. **D-2, D-4, P-7** — descritos acima; baixo risco, sem pressa.

O A-6, que abria esta lista, foi corrigido depois com autorização (ver o
item): era o único achado grave sem correção.
