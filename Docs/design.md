# Darb Shell — design da TUI

> Especificação da interface, derivada da maquete em
> `Documents/Criar interface editável` (`src/App.tsx`, 1081 linhas +
> `src/index.css`). Este documento é a fonte de verdade; o `App.tsx` é a
> origem visual.
>
> **Cada elemento da maquete aparece aqui classificado: ENTRA, ADAPTA ou
> FORA** — com a razão. O mapa completo está em §5. Nada entra sem estar
> classificado.

Índice:

1. [Princípios](#1-princípios)
2. [Dependências](#2-dependências)
3. [Tokens](#3-tokens)
4. [Geometria](#4-geometria)
5. [Mapa da referência](#5-mapa-da-referencia-entra-adapta-fora)
6. [Regiões](#6-regiões)
7. [Estados reais](#7-estados-reais-nunca-inventados)
8. [Teclado](#8-teclado)
9. [Responsividade](#9-responsividade)
10. [O que não entra](#10-o-que-não-entra)

---

## 1. Princípios

A TUI é **apresentação pura**. Ela não decide nada:

- **Não executa.** Comandos saem como `KeyOutcome` para `apps/darb`, que
  os encaminha para o registry (sempre com permissão).
- **Não lê o disco.** Ficheiros, Git, diff, tokens e telemetria chegam
  por `set_*`.
- **Não fala com providers.** O texto chega como `TextDelta`.
- **Não inventa números.** Desconhecido = `—` ou a linha não é desenhada.
  Nunca `0`, nunca um guess.

A maquete é uma **GUI**: tem `hover`, diálogos, `select`, minimap,
gradientes. O terminal não tem nada disso. A adaptação não é cosmética —
muda *o que se pode mostrar*, não só o desenho.

## 2. Dependências

Cada dependência justifica-se por código que existe hoje.

| Dependência | Usada por |
|---|---|
| `ratatui` | tui, app — render |
| `crossterm` | tui, app — input |
| `tokio` | core, providers, agent, app |
| `reqwest` | providers — único I/O de rede |
| `serde` / `serde_json` | core, tools, providers, agent |
| `toml` | core — config humana |
| `rusqlite` | memory — `bundled`, sem dep. de sistema |
| `sysinfo` | app — CPU/RAM |

Removidos em 2026-09-26 por auditoria (zero referências): `anyhow`,
`thiserror`, `tree-sitter`. `darb-plugins` (9 linhas, 4 módulos vazios)
também foi removido.

Regra: nada entra "para mais tarde". Um ficheiro de uma linha é dívida.

## 3. Tokens

`theme::Theme` — porte directo do objecto `C` da maquete.

| Papel | Hex | Uso na maquete |
|---|---|---|
| `root` | `#060b14` | fundo, gradiente do input |
| `shell` | `#080e1a` | header, rail, terminal |
| `panel` | `#0b1322` | painéis, cards |
| `surface` | `#101b2e` | — |
| `elevated` | `#14213a` | botão send inactivo |
| `hover` | `#16233a` | hover de linha (web) |
| `border` | `#1a2942` | linhas, cards |
| `border2` | `#23405f` | input activo, drawer |
| `dim` | `#253550` | gauge de fundo, badge ⌘K |
| `text` | `#e6eefb` | texto principal |
| `secondary` | `#8ba3c2` | texto secundário |
| `muted` | `#4d6484` | labels, placeholders |
| `faint` | `#2c425f` | hints, metas |
| `accent` | `#00d4ff` | **a cor de selecção** |
| `accent2` | `#4d7cff` | untracked |
| `success` | `#00e67a` | ok, done, `+A` |
| `warning` | `#ffb224` | warn, `~M` |
| `danger` | `#ff5d5d` | erro, `✕` |

Derivados que a maquete usa e que **faltam** em `Theme` — devem ser
adicionados:

| Papel | Hex | Onde |
|---|---|---|
| `selected_bg` | `#12233d` | fundo da linha/tab activa |
| `accent_bg` | `#00d4ff0d` | tint de acento (chips) |
| `ok_bg` | `#00e67a14` | badge `● clean+3` |
| `warn_bg` | `#141007` | fundo de permissão/danger |

`primary` e `highlight` são aliases de `accent`.

Invariantes: rampa de superfícies crescente até `root`; sinais distintos.

## 4. Geometria

`layout::shell_layout(area, explorer_visible, context_visible,
bottom_open) -> ShellLayout`. Função pura, testável sem terminal.

A maquete é **redimensionável** (288/300/190 px, com arrastar). Num
terminal não há rato fiável nem largura contínua. As percentagens
substituem o drag, e `Ctrl+B`/`Ctrl+L`/`Ctrl+J` substituem o collapse.

Vertical:

| Região | Altura |
|---|---|
| `header` | 3 |
| corpo | `Min(1)` |
| `tabs` | 1 |
| `statusbar` | 1 |
| `footer` | 3 |

Corpo — três colunas, painéis ocultos a **largura zero**:

| Coluna | Largura |
|---|---|
| `explorer` | 20% |
| `workspace` | `Fill(1)` |

## 5. Mapa da referência: entra / adapta / fora

Toda a maquete, elemento a elemento. Nada entra sem estar aqui.

### 5.1 Header — ENTRA (adaptado)

| Maquete | TUI | Nota |
|---|---|---|
| `▣ DARB SHELL` + `AI DEVELOPMENT ENVIRONMENT` | idêntico, 1 linha | o subtítulo perde-se; o logo não |
| `◈ darb-project` + badge `● clean+3` | nome do projecto + contadores Git | o badge Git é a informação real |
| Selects de model/provider + `● DARB AI` | vão para o painel Context (§6.4) | no header ocupam colunas fixas |
| Sino + gear + avatar `edmil` | **FORA** | GUI; avatar não é informação de desenvolvimento |
| `v0.1.0` | **ENTRA**, à direita | |
| Search bar `⌘K` central | **ADAPTA** → `Ctrl+K Commands` no rodapé | o header fica com 1 linha |
| Drawer de notificações (EVENTS) | **ADAPTA** → toasts | já existem toasts; um drawer é GUI |

### 5.2 Activity rail — ADAPTA

A maquete tem 6 ícones verticais com badges. No terminal: **uma coluna
de 3 caracteres** com as iniciais, só para as secções que existem.

| Rail | TUI | Razão |
|---|---|---|
| `Expl` | `E` | árvore de ficheiros — entra |
| `Srch` | **FORA** | a busca é o `Ctrl+K` global; duas searches é redundância |
| `Git` | `G` | entra |
| `Task` | `T` | entra |
| `Agnts` | **FORA** | multi-agente (Sentinel, Debugger, Architect) não existe; `darb-agent` é um loop único |
| `Ext` | **FORA** | o crate `darb-plugins` foi removido; LSP/ESLint são processos externos |
| `● ECO` toggle | **FORA** | o perfil vem da config (`configs/profiles/`), não de um toggle de sessão |

A rail é o que mais colunas custa: 3 colunas num terminal de 80. Compensa
— sem ela, a coluna esquerda empilharia explorer, Git e Tasks, e o custo
passa a ser o header de cada um.

### 5.3 Sidebar — ENTRA (adaptado)

| Maquete | TUI |
|---|---|
| Título `PROJECT NAVIGATOR` / `SOURCE CONTROL` / `TASKS` | título da secção activa da rail |
| Input `Filter files… (fuzzy)` | **ADAPTA** → `/` filtra a lista activa (o fuzzy já existe na paleta) |
| Árvore com indent `10 + depth*12` | 2 colunas por nível |
| Seleccionado: `#12233d` + barra `2px accent` | `selected_bg` + barra `accent` de 1 coluna |
| Cor por tipo (`.tsx` accent, `.ts` azul, `.json` warn) | **FORA** — cor-por-tipo é GUI |
| Badge Git na linha (`M`/`A`/`U`) | **ENTRA** — é informação real |
| `●●` em `auth` (aviso) | **FORA** — não há fonte para "pasta com avisos" |

### 5.4 Tabs — ADAPTA (a maior mudança)

A maquete tem tabs **dinâmicas**: `AI Session`, `login.ts ×`,
`Dashboard.tsx ×`, `Git`, `Task #024`, `Preview`, com fechar e badge de
modificado.

Tabs dinâmicas por ficheiro são um browser de ficheiros com 80 colunas.
Cada tab consome ~15 colunas: com 5 ficheiros abertos, só as tabs comem
75 de 80. Decisão:

- Tabs de **secção**, **fixas**: `Chat │ Files │ Git │ Terminal │ Tasks`.
- O ficheiro aberto **não é uma tab** — é o conteúdo do painel Files,
  com breadcrumb no topo (`src / auth / login.ts  M`).
- Fechar tab (`×`) — **FORA**: não há tabs para fechar.
- Badge de modificado — **ENTRA** como `M` no breadcrumb.

| `context` | 25% |


### 5.5 AI Workspace — ENTRA (o coração)

Aqui a maquete está certa, e é o que já está implementado.

| Elemento | TUI |
|---|---|
| Separador `You` / `Darb AI` | **ENTRA** — icons `◈` / `✦` |
| Mensagem com timestamp à direita | **ENTRA** |
| Step `✓` + label + hora | **ENTRA** |
| Step activo com spinner e `>` | **ENTRA** |
| Step futuro com `○` em `faint` | **ENTRA** |
| Detail do step (action, result, tool, tokens) — hover | **ADAPTA** → toggle com `Enter`. Hover é GUI; o terminal tem selecção |
| `AGENT TIMELINE` + `82%` | **ENTRA** |
| Card `IMPLEMENTATION PLAN` `01..05` | **ENTRA** |
| Card `FILES · TOOLS` (chips) | **ADAPTA** → 2 linhas: paths, depois ferramentas |
| Input `Ask Darb [AUTO] — refactor, explain, fix…` | **ENTRA** — o modo no placeholder é boa ideia |
| `ctx 3.2k/16k` no input | **ENTRA** |
| Botão send | **ADAPTA** → `▸` à direita |
| Hint `↵ send · ⌘K commands · hover any step…` | **ENTRA**, reescrita **sem "hover"** |
| Card `PERMISSION REQUIRED` (`RISK: HIGH`, Allow once / for task / Deny) | **ENTRA** — é a §16-19 da visão, e o `PermissionManager` já decide |

Os **6 modos da maquete** (`AUTO PLAN CODE DEBUG REVIEW EXPLAIN`) —
**ADAPTA**. A visão define 3 (`Mentor / Assist / Autonomous`). São eixos
diferentes: os da maquete são *tarefas*, os da visão são *níveis de
poder*. Implementa-se o eixo de poder da visão; o eixo de tarefa cabe no
prompt. Os dois juntos seriam 18 estados para o utilizador escolher.

### 5.6 Editor — FORA (com excepção)

A maquete tem editor completo: numeração, syntax highlight, minimap,
selecção, gutter com `▌`, acções `✦ Explain / Refactor / Fix / Test`.

**FORA** — e é a decisão mais importante deste documento.

Um editor com highlight exige `syntect` + themes, ou um renderer por
linguagem. São megabytes e RAM extra, num processo que já tem
`rusqlite` e `reqwest`. Viola directamente o princípio "leve por baixo"
da visão.

**Excepção que fica**: o buffer simples que **já existe** — abrir, ver,
editar, gravar, via registry com permissão. É uma tab, não uma IDE. Sem
highlight, sem minimap, sem gutter de diff.

O `✦ Explain / Refactor / Fix / Test` no header do ficheiro — **FORA**:
são os mesmos comandos que a paleta `AI ACTIONS` já oferece.

### 5.7 Git — ENTRA (adaptado)

| Maquete | TUI |
|---|---|
| `SOURCE CONTROL` com `+stage` / `✓ staged` | **ENTRA** |
| Commits com hash + mensagem + tempo | **ENTRA** |
| Botões `checkout / branch / diff / revert` | **ADAPTA** → paleta `Ctrl+K` (são comandos, não botões) |
| Mini-diff por ficheiro (barras) | **ADAPTA** → caracteres `+`/`~` na linha |

### 5.8 Preview — FORA

`localhost:8443` com janela de browser simulada e botão `Increment`.
Rejeitado pela mesma razão do browser incorporado: é o único elemento que
exigiria um motor de renderização dentro do processo. O Darb abre o
browser externo.

### 5.9 Terminal — ADAPTA

| Maquete | TUI |
|---|---|
| Tabs de processo (`bash`, `npm test`) | **ADAPTA** → prefixo na linha: `[npm test] $ cmd` |
| `✕ 3 failed · exit 1` / `✓ passing` | **ENTRA** — estado real do processo |
| `↻ retry` | **FORA** — re-executar é `↑` no histórico, não um botão |
| `✦ DARB AI detected test failures` + `Fix automatically` | **ADAPTA** → um toast, não um card dentro do terminal |
| Saída com `✓`/`✕` coloridos | **ENTRA** |
| Faixa fechada `▴ TERMINAL ● 3 failed Ctrl+J` | **ENTRA** (já implementado) |

### 5.10 Context — ADAPTA

| Maquete | TUI |
|---|---|
| `CONTEXT BUDGET 3.2k / 16k` + gauge | **ENTRA** (já implementado) |
| Reparto `files 1.4k / history 1.1k / tools 0.7k` | **ENTRA** — explica o consumo |
| `6 files · est. 3,241 tokens` | **ENTRA** |
| Cards `FILES 6 selected` / `TASK 82%` | **ENTRA** |
| Cards `MODEL` / `PROVIDER` com selects | **ENTRA** — aqui há espaço |
| `● connected · 41ms` | **ENTRA** — latência real |
| `ACTIVE CONTEXT` com tokens por ficheiro | **ENTRA** |
| `RECENT` | **ENTRA** |
| `TOOLS · PERMISSIONS` (`✓ read_file · ⚠ shell`) | **ENTRA** — liga a §18 (Mentor) |
| Colapso a `CONTEXT 3.2K` vertical | **FORA** — texto vertical não existe em terminal |

## 6. Regiões

### 6.1 Header (3 linhas)

```
▣ DARB SHELL │ AI DEVELOPMENT ENVIRONMENT   ◈ darb-project ● clean+3   model-2.1  v0.1.0
```

- Esquerda: logo, subscrito, projecto, contadores Git.
- Direita: modelo, versão.
- Estreito: **o lado direito cai primeiro**, depois o esquerdo trunca.
  Nunca sobrepor.

### 6.2 Coluna esquerda (rail + sidebar)

- **Rail** (3 colunas): `E` `G` `T`, mais `ECO` no fundo.
- **Sidebar**: título da secção, lista, scroll.

### 6.3 Centro

- **Tabs fixas**: `Chat │ Files │ Git │ Terminal │ Tasks`.
- **Chat**: `◈ You` / `✦ Darb AI`, mensagens, timeline de steps, plano,
  input.
- **Files**: breadcrumb + buffer (sem highlight — §5.6).
- **Git**: changes com stage, histórico.
- **Terminal**: faixa fechada de 1 linha, ou 8 com tabs de processo.

### 6.4 Coluna direita — CONTEXT

Orçamento de tokens com gauge, repartição, `FILES`/`TASK`,
`MODEL`/`PROVIDER`, `ACTIVE CONTEXT` com tokens por ficheiro, `RECENT`,
`TOOLS · PERMISSIONS`.

### 6.5 Rodapé

`Ask Darb [MODO] — …  ctx 3.2k/16k  ▸` + hints de tecla.

## 7. Estados reais, nunca inventados

Regra dura: a TUI mostra **o que existe**, nunca o que parece bonito.

| Valor | Se não existe |
|---|---|
| CPU / RAM | não desenha a linha |
| tokens | `—` |
| branch / changes | não desenha a secção |
| latência do provider | não mostra `41ms` |
| ficheiros | linha de vazio |

`● connected` só aparece se a última chamada ao provider teve sucesso.
Um provider que falhou há 3 minutos não mostra `connected`.

## 8. Teclado

Mapeamento puro: `keybindings::action_for(KeyEvent) -> Action`.

| Tecla | Acção |
|---|---|
| `Ctrl+B` | toggle sidebar (rail + lista) |
| `Ctrl+L` | toggle contexto |
| `Ctrl+J` | toggle terminal |
| `Ctrl+K` | paleta de comandos |
| `Ctrl+P` | paleta (filtro `>` para comandos) |
| `Ctrl+Shift+G` | tab Git |
| `Ctrl+Shift+A` | tab Chat |
| `Ctrl+Enter` | submeter (o `Enter` simples é caractere) |
| `Esc` | fechar overlay / limpar selecção |
| `Tab` | alternar secção da rail |
| `↑` `↓` | scroll / histórico do input |
| `Enter` | toggle do detail do step activo |
| `/` | filtrar a lista da sidebar |
| `a` / `d` | permitir / negar permissão |

Notas:

- A maquete usa `⌘K` (mac). No terminal é **`Ctrl+K`** — e por isso o
  glyph é `Ctrl`, não `⌘`.
- A maquete submete com `Ctrl+Enter` porque tem `Enter` para newline
  num input GUI. **Num input de uma linha, `Enter` submete** — é o que
  o utilizador espera. `Ctrl+Enter` fica como alternativa.
- `Ctrl+M` **não** é usado: `Enter` e `Ctrl+M` são indistinguíveis no fio.
- `q` não sai — senão não se escreve "q" no input.
- `Action::Ignore` com `Char` sem modificadores = **input**, não acção.

## 9. Responsividade

Alvo: 80×24 num ecrã pequeno. O hardware de origem (AMD E2-1800, 8 GB) é
o caso de teste, não uma excepção.

- Estreito: colunas caem por ordem (contexto, depois rail+sidebar); o
  centro nunca desaparece.
- Baixo: painéis secundários colapsam; o centro tem sempre 1 linha.
- As larguras dos spans são medidas, não assumidas — `─` e CJK não
  partem as colunas.
- Sem animação por defeito: `animations = false` no perfil `eco`.
  Spinners avançam por tempo, e só enquanto o agente trabalha.

Com 80 colunas e as três colunas visíveis, sobram ~44 para o centro. É
justo — e é por isso que as tabs são fixas (§5.4).

## 10. O que não entra

Registado para ninguém reintroduzir sem decisão nova:

- **Browser incorporado / Preview.** Motor de renderização dentro do
  processo. Contrário ao princípio de leveza. §5.8.
- **Editor com syntax highlight.** `syntect` + themes é peso a mais. §5.6.
- **Tabs dinâmicas por ficheiro.** 15 colunas por tab. §5.4.
- **Multi-agente** (Sentinel, Debugger, Architect). `darb-agent` é um
  loop único. §5.2.
- **Extensões / plugins.** O crate foi removido; volta com o primeiro
  plugin real. §5.2.
- **Botões em vez de comandos.** `checkout`/`retry`/`Fix` vão para a
  paleta. §5.7, §5.9.
- **Cor por tipo de ficheiro.** GUI. §5.3.
- **Hover.** Não existe em terminal; vira `Enter` em algo seleccionado. §5.5.
- **Gradientes, sombras, texto vertical, minimap.** §5.3, §5.6, §5.10.
- **Routing automático de modelo.** Fase posterior, e opcional.

Regra prática: se um painel novo não cabe no `App` sem um setter novo,
**o setter é o trabalho** — e vale a pena? Se a resposta é não, o painel
não entra.
