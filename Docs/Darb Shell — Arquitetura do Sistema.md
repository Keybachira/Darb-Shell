# Darb Shell — Arquitetura do Sistema

> **AI-native coding environment built for the terminal, designed first for low-resource machines.**

**Versão:** 0.1.0  
**Status:** Architecture Draft  
**Idioma padrão:** Português (`pt`)  
**Idioma secundário:** Inglês (`en`)  
**Runtime principal:** Rust  
**Interface:** TUI  
**Target principal:** AMD E2-1800 + 8 GB RAM

---

## 1. Visão

O **Darb Shell** é um ambiente de desenvolvimento assistido por IA executado diretamente no terminal.

O objetivo não é simplesmente criar mais um chatbot para programação.

O Darb deve combinar:

- agente de programação;
- terminal;
- explorador de projeto;
- editor de alterações;
- Git;
- contexto inteligente;
- memória;
- múltiplos provedores de IA;
- sistema de permissões;
- interface TUI interativa.

A experiência deve aproximar-se de uma pequena IDE, mas permanecer:

- terminal-first;
- extremamente leve;
- rápida;
- keyboard-friendly;
- clicável quando o terminal suportar mouse;
- extensível;
- independente do provedor de IA.

### Princípio central

> **O LLM é o cérebro. O Darb é o sistema nervoso.**

O modelo pensa e gera decisões.

O Darb:

- entende o projeto;
- seleciona contexto;
- executa ferramentas;
- controla permissões;
- gerencia sessões;
- controla memória;
- apresenta resultados;
- conecta diferentes provedores.

---

# 2. Objetivos

## 2.1 Objetivos principais

O Darb deve:

1. funcionar bem em máquinas com poucos recursos;
2. oferecer uma TUI moderna;
3. suportar múltiplos provedores de IA;
4. possuir um agente capaz de executar tarefas de desenvolvimento;
5. possuir ferramentas seguras para manipulação do sistema;
6. reduzir o contexto enviado ao LLM;
7. possuir memória por projeto e sessão;
8. permitir extensões através de plugins;
9. ser configurável;
10. suportar Português e Inglês;
11. funcionar offline para operações que não dependam de IA remota;
12. permitir IA local através de Ollama;
13. manter o core independente dos providers.

---

# 3. Não-objetivos

Na primeira versão, o Darb não deve tentar:

- ser uma IDE gráfica;
- incluir Chromium;
- incluir Electron;
- executar modelos gigantes localmente;
- indexar continuamente todos os projetos;
- substituir completamente um editor de código;
- criar uma plataforma cloud obrigatória;
- depender de um único provedor;
- executar comandos perigosos sem autorização.

---

# 4. Filosofia

A arquitetura do Darb segue quatro princípios.

## 4.1 Lightweight First

Tudo deve ser pensado para hardware limitado.

```text
Low RAM
Low CPU
Low Disk I/O
Low Background Activity
```

---

## 4.2 Remote Intelligence

A inteligência pesada pode ficar no provedor remoto.

```text
┌─────────────────────┐
│      DARb           │
│                     │
│ TUI                 │
│ Agent Runtime       │
│ Context             │
│ Tools               │
│ Git                 │
│ Memory              │
└──────────┬──────────┘
           │
           │ HTTPS
           ▼
┌─────────────────────┐
│     LLM Provider    │
│                     │
│ OpenAI              │
│ Anthropic           │
│ Gemini              │
│ OpenRouter          │
└─────────────────────┘
```

---

## 4.3 Provider Agnostic

O Agent não deve conhecer detalhes específicos de cada provider.

```text
Agent
  │
  ▼
Provider Interface
  │
  ├── OpenAI
  ├── Anthropic
  ├── Gemini
  ├── OpenRouter
  ├── Ollama
  └── Custom
```

---

## 4.4 Terminal Native

A interface não deve parecer um dashboard web colocado dentro de um terminal.

Deve parecer um produto criado especificamente para o terminal.

---

# 5. Arquitetura Geral

```text
                         ┌───────────────────────┐
                         │      DARB SHELL       │
                         │ AI Development Env.   │
                         └───────────┬───────────┘
                                     │
                         ┌───────────▼───────────┐
                         │       DARB TUI        │
                         │                       │
                         │ Explorer │ AI │Context│
                         │ Chat │ Diff │ Terminal│
                         │ Git │ Tasks │ Commands│
                         └───────────┬───────────┘
                                     │
                         ┌───────────▼───────────┐
                         │       DARB CORE       │
                         │                       │
                         │ Runtime               │
                         │ Events                │
                         │ Config                │
                         │ Sessions              │
                         │ Permissions           │
                         └───────────┬───────────┘
                                     │
          ┌──────────────────────────┼─────────────────────────┐
          │                          │                         │
┌─────────▼─────────┐      ┌─────────▼─────────┐     ┌────────▼────────┐
│    DARB AGENT     │      │   DARB CONTEXT    │     │   DARB MEMORY   │
│                   │      │                   │     │                 │
│ Planner           │      │ Project Map       │     │ Sessions        │
│ Executor          │      │ File Selection    │     │ Project Memory  │
│ Reviewer          │      │ Token Budget      │     │ History         │
│ Tool Calling      │      │ Compression       │     │ Decisions       │
└─────────┬─────────┘      └───────────────────┘     └─────────────────┘
          │
┌─────────▼──────────────────────────────────────────┐
│                    DARB TOOLS                      │
│                                                    │
│ Filesystem │ Search │ Shell │ Git │ Tests │ Diff │
└──────────────────────────┬─────────────────────────┘
                           │
                 ┌─────────▼─────────┐
                 │  DARB PROVIDERS   │
                 │                   │
                 │ OpenAI            │
                 │ Anthropic         │
                 │ Gemini            │
                 │ OpenRouter        │
                 │ Ollama            │
                 │ Custom            │
                 └───────────────────┘
```

---

# 6. Workspace do Monorepo

O projeto será organizado como um Rust Workspace.

```text
darb-shell/
│
├── crates/
│   │
│   ├── darb-core/
│   │
│   ├── darb-agent/
│   │
│   ├── darb-tui/
│   │
│   ├── darb-tools/
│   │
│   ├── darb-context/
│   │
│   ├── darb-memory/
│   │
│   ├── darb-providers/
│   │
│   └── darb-plugins/
│
├── apps/
│   │
│   └── darb/
│
├── configs/
│   ├── default.toml
│   │
│   └── profiles/
│       ├── eco.toml
│       ├── fast.toml
│       ├── smart.toml
│       └── local.toml
│
├── docs/
│   ├── architecture.md
│   ├── providers.md
│   ├── tools.md
│   ├── plugins.md
│   └── contributing.md
│
├── tests/
│
├── Cargo.toml
├── Cargo.lock
├── README.md
└── LICENSE
```

---

# 7. Darb Core

O `darb-core` é o núcleo do sistema.

Ele coordena:

- configuração;
- runtime;
- eventos;
- sessões;
- permissões;
- estado global.

## Estrutura

```text
darb-core/
├── config/
├── events/
├── runtime/
├── session/
├── permissions/
├── i18n/
└── lib.rs
```

---

# 8. Event Bus

O Darb utiliza uma arquitetura orientada a eventos.

Exemplo:

```text
UserInput
    │
    ▼
AgentStarted
    │
    ▼
ContextRequested
    │
    ▼
ContextReady
    │
    ▼
ProviderRequest
    │
    ▼
ToolRequested
    │
    ▼
PermissionRequested
    │
    ▼
ToolExecuting
    │
    ▼
ToolCompleted
    │
    ▼
ReviewRequested
    │
    ▼
AgentFinished
```

Isso permite que:

- a TUI reaja aos eventos;
- o Agent permaneça independente da interface;
- plugins observem eventos;
- logs sejam gerados;
- o sistema seja testável.

---

# 9. Darb Agent

O Agent é responsável pela execução de tarefas de desenvolvimento.

## Componentes

```text
darb-agent/
├── planner/
├── executor/
├── reviewer/
├── loop/
├── tool_calling/
└── lib.rs
```

## Fluxo

```text
User Request
     │
     ▼
Understand
     │
     ▼
Build Context
     │
     ▼
Plan
     │
     ▼
Provider
     │
     ▼
Tool Call
     │
     ▼
Execute
     │
     ▼
Review
     │
     ├── Problem ──► Fix ──► Review
     │
     └── Good ─────────────► Done
```

---

# 10. Planner

O Planner transforma um pedido em um plano de execução.

Exemplo:

```text
Pedido:

"Corrige o sistema de autenticação."

Plano:

1. Encontrar módulos de autenticação
2. Analisar login
3. Analisar sessão
4. Identificar validações duplicadas
5. Implementar correção
6. Executar testes
7. Revisar alterações
```

O plano pode ser exibido na TUI.

---

# 11. Executor

O Executor transforma decisões do Agent em ações.

```text
Agent
  │
  ▼
Executor
  │
  ├── read_file
  ├── edit_file
  ├── search
  ├── run_command
  ├── git_diff
  └── run_tests
```

O Executor nunca deve ignorar o Permission Manager.

---

# 12. Reviewer

Depois das alterações:

```text
Changes
   │
   ▼
Reviewer
   │
   ├── Syntax
   ├── Tests
   ├── Diagnostics
   ├── Diff
   └── Agent Review
```

O objetivo é reduzir alterações incorretas antes de informar o utilizador.

---

# 13. Darb Providers

Os providers são responsáveis pela comunicação com modelos de IA.

```text
darb-providers/
│
├── interface/
│
├── openai/
├── anthropic/
├── google/
├── openrouter/
├── ollama/
├── custom/
│
└── lib.rs
```

---

# 14. Provider Interface

O Agent deve comunicar através de uma abstração comum.

```rust
trait Provider {
    async fn complete(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderResponse>;

    async fn stream(
        &self,
        request: ProviderRequest,
    ) -> Result<ProviderStream>;

    fn capabilities(&self) -> ProviderCapabilities;
}
```

O Agent não deve saber se está a utilizar:

```text
OpenAI
Gemini
Anthropic
Ollama
```

Ele simplesmente utiliza:

```text
Provider
```

---

# 15. Provider Capabilities

Cada provider pode declarar capacidades.

```text
ProviderCapabilities
│
├── streaming
├── tool_calling
├── vision
├── reasoning
├── structured_output
├── embeddings
└── max_context
```

Isso permite ao Agent adaptar o comportamento.

---

# 16. Profiles

O utilizador poderá definir perfis.

### Eco

```text
CPU local: mínimo
Contexto: reduzido
Requests: otimizados
```

### Fast

```text
Modelo rápido
Contexto moderado
Baixa latência
```

### Smart

```text
Modelo mais inteligente
Contexto maior
Melhor raciocínio
```

### Local

```text
Provider: Ollama
Internet: não necessária
```

Exemplo:

```bash
darb --profile eco
darb --profile smart
darb --profile local
```

---

# 17. Darb Context

O Context Manager evita enviar projetos inteiros ao modelo.

## Pipeline

```text
Project
   │
   ▼
Project Map
   │
   ▼
Relevant Files
   │
   ▼
Code Analysis
   │
   ▼
Token Budget
   │
   ▼
Context Compression
   │
   ▼
Provider
```

---

# 18. Project Map

O Darb mantém uma representação leve do projeto.

Exemplo:

```text
src/
├── auth/
│   ├── login.ts
│   ├── session.ts
│   └── middleware.ts
│
├── users/
├── payments/
├── dashboard/
└── utils/
```

Quando o utilizador pergunta:

> Corrige o login.

O Context Manager prioriza:

```text
auth/login.ts
auth/session.ts
auth/middleware.ts
```

e evita carregar:

```text
payments/
dashboard/
```

---

# 19. Tree-sitter

Tree-sitter será utilizado quando análise estrutural do código for necessária.

Possíveis usos:

- detectar funções;
- detectar classes;
- encontrar imports;
- identificar símbolos;
- analisar relações;
- construir project maps;
- selecionar contexto relevante.

A indexação deve ser **lazy/on-demand**.

Não queremos manter um indexador pesado funcionando constantemente.

---

# 20. Token Budget

Cada request deve possuir um orçamento de contexto.

```text
Context Budget
│
├── System Prompt
├── User Request
├── Project Context
├── Relevant Files
├── Tool Results
└── Conversation
```

Exemplo:

```text
16k tokens
│
├── 2k system
├── 1k user
├── 5k project
├── 5k files
└── 3k history
```

O Context Manager deve evitar ultrapassar o limite.

---

# 21. Darb Tools

As Tools representam as ações que o Agent pode executar.

```text
darb-tools/
│
├── filesystem/
├── shell/
├── search/
├── git/
├── editor/
├── tests/
├── diagnostics/
└── lib.rs
```

## Tools iniciais

```text
read_file
write_file
edit_file
list_directory
search
run_command
git_status
git_diff
git_log
run_tests
```

---

# 22. Filesystem Tool

Responsável por:

```text
read
write
create
delete
rename
list
```

Operações destrutivas devem exigir autorização.

---

# 23. Search Tool

A busca deve privilegiar ferramentas eficientes do sistema.

Exemplo:

```text
ripgrep
```

O Darb não precisa carregar todos os arquivos para procurar uma string.

```text
Query
 ↓
ripgrep
 ↓
Matching files
 ↓
Relevant context
```

---

# 24. Shell Tool

Permite executar comandos do sistema.

Exemplo:

```bash
npm test
cargo check
git status
npm run build
```

Por segurança:

```text
Agent
  │
  ▼
Shell Tool
  │
  ▼
Permission Manager
  │
  ├── Allow
  ├── Deny
  └── Ask
```

---

# 25. Permission System

As ferramentas terão níveis de risco.

## Safe

```text
read_file
list_directory
git_status
git_diff
search
```

## Moderate

```text
write_file
edit_file
install_package
```

## Dangerous

```text
shell
delete
git_push
sudo
```

---

# 26. Permission Configuration

```toml
[permissions]

read = "allow"
edit = "ask"
shell = "ask"
delete = "ask"
git_push = "deny"
```

Quando uma ação requer confirmação:

```text
┌──────────────────────────────────────────┐
│ Permission Required                      │
│                                          │
│ Darb wants to execute:                   │
│                                          │
│ npm install                              │
│                                          │
│ [ Allow ] [ Deny ] [ Always Allow ]     │
└──────────────────────────────────────────┘
```

---

# 27. Darb Memory

Memory e Context são sistemas diferentes.

### Context

> O que é relevante neste momento.

### Memory

> O que o Darb aprendeu ou armazenou anteriormente.

```text
Darb Memory
│
├── Session Memory
│
├── Project Memory
│
└── Agent History
```

---

# 28. Session Memory

Armazena a sessão atual.

```text
Session
├── messages
├── tool_calls
├── changes
├── decisions
└── errors
```

---

# 29. Project Memory

Informações persistentes do projeto:

```text
Project Memory
│
├── stack
├── architecture
├── conventions
├── commands
├── preferences
└── important files
```

---

# 30. Diretório `.darb`

Cada projeto poderá ter:

```text
.darb/
├── config.toml
├── project.json
├── memory.db
└── sessions/
```

O `.darb` deve ser pequeno e controlado.

---

# 31. Storage

SQLite será utilizado quando persistência estruturada for necessária.

Possíveis tabelas:

```text
projects
sessions
messages
tool_calls
memories
decisions
tasks
```

O sistema deve evitar escrever continuamente no disco.

---

# 32. Darb TUI

A interface será construída com:

```text
Ratatui
+
Crossterm
```

A TUI deve possuir:

- mouse support;
- keyboard navigation;
- tabs;
- panels;
- command palette;
- clickable areas;
- focus states;
- streaming;
- diff viewer;
- terminal integrado.

---

# 33. Layout Principal

```text
┌──────────────────────────────────────────────────────────────┐
│ ◈ DARB SHELL                              ● ONLINE  v0.1.0   │
├──────────────┬──────────────────────────────┬───────────────┤
│              │                              │               │
│   PROJECT    │        AI WORKSPACE          │   CONTEXT     │
│              │                              │               │
│   src/       │ > Refactor authentication    │ Files: 6      │
│    auth/     │                              │ Tokens: 3.2k  │
│    users/    │ ✓ Project analyzed           │ Model: Remote │
│    utils/    │ ✓ Auth modules found         │               │
│              │ ◉ Creating plan               │ Active files │
│   Git        │ ○ Applying changes            │ login.ts      │
│   ✓ Clean    │ ○ Running tests               │ session.ts    │
│              │                              │               │
├──────────────┴──────────────────────────────┴───────────────┤
│ Chat │ Files │ Changes │ Terminal │ Git │ Tasks              │
├──────────────────────────────────────────────────────────────┤
│ > Ask Darb to modify your project...             ⌘K Commands│
└──────────────────────────────────────────────────────────────┘
```

---

# 34. TUI Components

```text
darb-tui/
│
├── app/
│
├── layout/
│
├── components/
│   ├── explorer
│   ├── chat
│   ├── context
│   ├── diff
│   ├── terminal
│   ├── git
│   └── tasks
│
├── widgets/
│
├── keybindings/
│
├── theme/
│
└── lib.rs
```

---

# 35. Command Palette

Atalho:

```text
Ctrl + K
```

Exemplo:

```text
┌───────────────────────────────────────┐
│ Search commands...                    │
├───────────────────────────────────────┤
│ > Open Project                        │
│   New Chat                            │
│   Run Tests                           │
│   Git Status                          │
│   Change Provider                     │
│   Change Model                        │
│   Settings                            │
│   Toggle Eco Mode                     │
└───────────────────────────────────────┘
```

---

# 36. Modos de Interface

## Focus Mode

Interface reduzida:

```text
Chat
Agent
Output
```

## Studio Mode

Interface completa:

```text
Explorer
AI
Context
Terminal
Git
Tasks
```

O Studio Mode será a experiência principal.

---

# 37. Internationalization

O Darb será multilíngue desde a arquitetura inicial.

## Idiomas

```text
pt = Português
en = English
```

### Default

```text
pt
```

### Fallback

```text
pt → en
```

Caso uma tradução portuguesa não exista, o sistema utiliza a tradução inglesa.

---

# 38. Estrutura i18n

```text
darb-core/
└── i18n/
    ├── mod.rs
    ├── locale.rs
    └── locales/
        ├── pt.toml
        └── en.toml
```

Exemplo:

```toml
[agent]

thinking = "A pensar..."
planning = "A criar plano..."
executing = "A executar..."
completed = "Concluído"
```

English:

```toml
[agent]

thinking = "Thinking..."
planning = "Creating plan..."
executing = "Executing..."
completed = "Completed"
```

---

# 39. Configuração de idioma

```toml
[interface]

language = "pt"
```

CLI:

```bash
darb --lang pt
darb --lang en
```

Configuração persistente:

```bash
darb config language pt
```

---

# 40. Separação de idioma

O idioma do utilizador não deve afetar a arquitetura interna.

```text
Code
       → English

API
       → English

Variables
       → English

Tool Names
       → English

Logs internos
       → English

UI
       → Portuguese default

Agent Messages
       → Portuguese default

Documentation
       → Portuguese + English
```

---

# 41. Darb Plugins

O sistema será extensível.

```text
darb-plugins/
│
├── loader/
├── registry/
├── api/
└── lib.rs
```

Tipos:

```text
Provider Plugin
Tool Plugin
Command Plugin
Theme Plugin
Language Plugin
Integration Plugin
```

---

# 42. Plugin Architecture

```text
                 DARB
                  │
             Plugin API
                  │
       ┌──────────┼───────────┐
       │          │           │
    Provider     Tool       Theme
       │          │           │
    Gemini      Docker      Custom
```

Plugins não devem ter acesso irrestrito ao sistema.

O sistema de permissões também deverá controlar plugins.

---

# 43. Performance Architecture

O Darb será projetado primeiro para máquinas como:

```text
AMD E2-1800
8 GB RAM
SSD
```

## Regras

### Não utilizar

```text
Electron
Chromium
WebView
Heavy GUI framework
Background indexing permanente
60 FPS rendering
Large in-memory caches
```

### Utilizar

```text
Rust
Ratatui
Crossterm
Tokio
Reqwest
Serde
Tree-sitter
SQLite
ripgrep
```

---

# 44. Rendering Strategy

A TUI deve ser orientada a eventos.

```text
Event
  │
  ▼
State Changed?
  │
  ├── NO ──► Sleep
  │
  └── YES
       │
       ▼
     Render
```

Não existe necessidade de redesenhar a interface constantemente.

---

# 45. Lazy Loading

Componentes pesados devem ser carregados apenas quando necessários.

Exemplo:

```text
Darb Start
   │
   ├── Core       → Load
   ├── TUI        → Load
   ├── Config     → Load
   │
   ├── Git        → Lazy
   ├── Tree-sitter → Lazy
   ├── Memory DB  → Lazy
   └── Provider   → On demand
```

---

# 46. CPU Strategy

O Darb não deve executar tarefas pesadas em background sem necessidade.

Evitar:

```text
CPU
████████████████████
```

quando o utilizador está simplesmente lendo a interface.

Preferir:

```text
Idle
  ↓
User Action
  ↓
Process
  ↓
Idle
```

---

# 47. Data Flow

Exemplo completo:

```text
User
 │
 │ "Corrige o login"
 ▼
TUI
 │
 ▼
Core
 │
 ▼
Agent
 │
 ├── Context
 │     ├── Project Map
 │     ├── Search
 │     └── Token Budget
 │
 ▼
Provider
 │
 ▼
LLM
 │
 ▼
Tool Call
 │
 ▼
Permission Manager
 │
 ▼
Tool
 │
 ▼
Filesystem
 │
 ▼
Result
 │
 ▼
Reviewer
 │
 ▼
TUI
 │
 ▼
User
```

---

# 48. Agent State Machine

O Agent pode ser representado como uma máquina de estados.

```text
IDLE
 │
 ▼
ANALYZING
 │
 ▼
PLANNING
 │
 ▼
WAITING_PROVIDER
 │
 ▼
EXECUTING
 │
 ▼
REVIEWING
 │
 ├──────────────┐
 │              │
 ▼              ▼
COMPLETED      RETRYING
                  │
                  └──────► EXECUTING
```

---

# 49. Erros

Os erros devem possuir categorias.

```text
DarbError
│
├── ConfigError
├── ProviderError
├── ContextError
├── ToolError
├── PermissionError
├── StorageError
├── GitError
├── NetworkError
└── InternalError
```

A TUI deve transformar erros técnicos em mensagens compreensíveis.

---

# 50. Offline Mode

O Darb deve continuar funcional sem internet.

Sem internet:

```text
✓ navegar no projeto
✓ ler arquivos
✓ editar arquivos
✓ pesquisar
✓ Git
✓ executar comandos
✓ executar testes
✓ visualizar diffs
✓ memória local
```

Não disponível:

```text
✗ Remote LLM
```

Se Ollama estiver configurado:

```text
✓ Local LLM
```

---

# 51. Local AI

Ollama será opcional.

```text
Darb
 │
 ▼
Provider Interface
 │
 └── Ollama
       │
       ▼
    Local Model
```

No AMD E2-1800, modelos locais devem ser considerados **experimentais**.

O objetivo principal da máquina fraca continua sendo:

```text
Local CPU → orchestration
Remote GPU → intelligence
```

---

# 52. Configuração Global

Exemplo:

```toml
[app]

language = "pt"
profile = "eco"

[provider]

name = "openai"
model = "..."

[interface]

mouse = true
animations = false
compact = false

[context]

max_tokens = 16000
lazy_indexing = true

[permissions]

read = "allow"
edit = "ask"
shell = "ask"
delete = "ask"
git_push = "deny"
```

---

# 53. Profiles de Performance

## Eco

```text
Animations: disabled
Indexing: lazy
Background tasks: minimum
Context: optimized
Rendering: event-driven
```

## Fast

```text
Animations: minimal
Context: moderate
Caching: enabled
```

## Smart

```text
Context: expanded
Higher quality model
More analysis
```

## Local

```text
Provider: Ollama
Remote requests: disabled
```

---

# 54. Segurança

O Darb será considerado um sistema que possui acesso ao computador do utilizador.

Portanto:

```text
Never trust tool calls blindly.
```

Regras:

1. ferramentas possuem permissões;
2. comandos perigosos exigem confirmação;
3. plugins possuem sandbox lógico;
4. API keys nunca devem aparecer na TUI;
5. secrets não devem ser enviados ao LLM sem necessidade;
6. comandos destrutivos devem ser destacados;
7. `git push` deve exigir confirmação;
8. `sudo` deve ser bloqueado por padrão.

---

# 55. Secrets

As chaves dos providers não devem ficar diretamente em:

```text
project.toml
```

Nem em:

```text
.darb/config.toml
```

Preferir:

```text
Environment Variables
OS Credential Store
Secret Manager
```

Exemplo:

```bash
DARB_OPENAI_API_KEY
DARB_ANTHROPIC_API_KEY
DARB_GEMINI_API_KEY
```

---

# 56. Observabilidade

O Darb deverá possuir logs internos.

```text
.darb/
└── logs/
    └── darb.log
```

Mas os logs não devem gerar escrita constante no disco.

Devem existir níveis:

```text
error
warn
info
debug
trace
```

---

# 57. CLI

Comandos iniciais:

```bash
darb
darb init
darb run
darb config
darb provider
darb model
darb doctor
darb version
```

Exemplos:

```bash
darb
```

Abre o ambiente.

```bash
darb init
```

Inicializa configuração do projeto.

```bash
darb provider
```

Gerencia providers.

```bash
darb doctor
```

Verifica o ambiente.

---

# 58. `darb doctor`

Exemplo:

```text
Darb Doctor

✓ Rust runtime
✓ Terminal capabilities
✓ Git
✓ ripgrep
✓ Project configuration
✓ Provider configuration
✓ Storage

System

CPU: AMD E2-1800
RAM: 8 GB
Mode: ECO
```

---

# 59. Command Palette

Atalhos principais:

```text
Ctrl + K     Command Palette
Ctrl + P     Quick File
Ctrl + G     Git
Ctrl + T     Terminal
Ctrl + L     Toggle Layout
Ctrl + B     Toggle Explorer
Ctrl + C     Cancel / Exit
Enter        Confirm
Esc          Back
```

Os atalhos poderão ser configurados posteriormente.

---

# 60. Git Integration

O Git será inicialmente integrado através do Git CLI.

Funcionalidades:

```text
git status
git diff
git log
git branch
git checkout
git add
git commit
```

Na TUI:

```text
Git
│
├── Changed Files
├── Staged
├── Untracked
├── Diff
└── Branch
```

---

# 61. Diff Viewer

Depois de uma alteração:

```text
┌──────────────────────────────────────┐
│ Changes                              │
├──────────────────────────────────────┤
│ src/auth/login.ts                    │
│                                      │
│ - const user = await findUser();     │
│ + const user = await findUser(id);   │
│                                      │
│ - return user;                       │
│ + return validateUser(user);         │
└──────────────────────────────────────┘
```

O utilizador poderá revisar antes de aceitar.

---

# 62. Testing

O Agent poderá executar:

```text
npm test
pnpm test
yarn test
bun test
cargo test
pytest
go test
```

O Darb deve detectar comandos do projeto sempre que possível.

---

# 63. Project Detection

O Context Manager pode detectar:

```text
package.json
Cargo.toml
pyproject.toml
go.mod
pubspec.yaml
deno.json
composer.json
```

e inferir:

```text
Language
Framework
Package Manager
Test Runner
Build System
```

---

# 64. Compatibilidade

O Darb deverá funcionar em:

```text
Linux
Windows
macOS
```

Prioridade de desenvolvimento:

```text
Linux
   ↓
Windows
   ↓
macOS
```

A arquitetura não deve depender de APIs específicas de um sistema.

---

# 65. Dependências Principais

Stack inicial:

```text
Rust
├── Ratatui
├── Crossterm
├── Tokio
├── Reqwest
├── Serde
├── Serde JSON
├── TOML
├── Tree-sitter
├── SQLite
└── Thiserror / Anyhow
```

Ferramentas externas:

```text
Git
ripgrep
```

---

# 66. Dependency Philosophy

Cada dependência deve justificar sua existência.

Pergunta antes de adicionar:

> "Precisamos realmente desta biblioteca?"

Isso evita transformar o Darb num software pesado.

---

# 67. Test Architecture

```text
tests/
│
├── core/
├── agent/
├── context/
├── providers/
├── tools/
├── permissions/
├── memory/
└── tui/
```

Testes devem existir principalmente para:

- Agent loop;
- provider abstraction;
- permission system;
- context selection;
- tool execution;
- configuration;
- serialization.

---

# 68. Development Phases

## Phase 1 — Kernel

```text
Darb CLI
Darb Core
Config
Events
Session
```

## Phase 2 — TUI

```text
Explorer
Chat
Context
Tabs
Command Palette
```

## Phase 3 — Tools

```text
Filesystem
Search
Shell
Git
Tests
Diff
```

## Phase 4 — Agent

```text
Planner
Executor
Reviewer
Tool Calling
Permissions
```

## Phase 5 — Providers

```text
OpenAI
Anthropic
Gemini
OpenRouter
Ollama
Custom
```

## Phase 6 — Context

```text
Project Map
Tree-sitter
Token Budget
Context Selection
Compression
```

## Phase 7 — Memory

```text
Sessions
Project Memory
History
SQLite
```

## Phase 8 — Plugins

```text
Plugin API
Provider Plugins
Tool Plugins
Themes
Integrations
```

---

# 69. MVP

O primeiro MVP não precisa de tudo.

O MVP ideal:

```text
✓ Rust
✓ Ratatui
✓ Crossterm
✓ Core
✓ TUI
✓ OpenAI Provider
✓ Provider Interface
✓ read_file
✓ edit_file
✓ search
✓ shell
✓ git diff
✓ permissions
✓ basic context
✓ PT/EN
```

Depois:

```text
Anthropic
Gemini
OpenRouter
Ollama
Memory
Tree-sitter
Plugins
```

---

# 70. Definition of Done — MVP

O MVP será considerado funcional quando conseguir:

```text
1. Abrir um projeto
2. Mostrar arquivos
3. Conversar com o Agent
4. Analisar arquivos
5. Selecionar contexto
6. Editar código
7. Executar testes
8. Mostrar diff
9. Pedir autorização para comandos perigosos
10. Trabalhar com pelo menos um provider
11. Trocar idioma PT/EN
12. Funcionar confortavelmente em hardware limitado
```

---

# 71. Arquitetura Final

```text
                           ┌─────────────────┐
                           │      USER       │
                           └────────┬────────┘
                                    │
                                    ▼
                         ┌─────────────────────┐
                         │      DARB TUI       │
                         │                     │
                         │ Chat                │
                         │ Explorer            │
                         │ Context             │
                         │ Diff                │
                         │ Terminal            │
                         │ Git                 │
                         │ Tasks               │
                         └──────────┬──────────┘
                                    │
                                    ▼
                         ┌─────────────────────┐
                         │     DARB CORE       │
                         │                     │
                         │ Runtime             │
                         │ Events              │
                         │ Config              │
                         │ Sessions            │
                         │ Permissions         │
                         │ i18n                │
                         └──────────┬──────────┘
                                    │
              ┌─────────────────────┼─────────────────────┐
              │                     │                     │
              ▼                     ▼                     ▼
       ┌─────────────┐       ┌─────────────┐       ┌─────────────┐
       │ DARB AGENT  │       │DARB CONTEXT │       │ DARB MEMORY │
       │             │       │             │       │             │
       │ Planner     │       │ Project Map │       │ Sessions    │
       │ Executor    │       │ Selection   │       │ Projects    │
       │ Reviewer    │       │ Tokens      │       │ History     │
       └──────┬──────┘       └─────────────┘       └─────────────┘
              │
              ▼
       ┌─────────────────────────────────────────┐
       │              DARB TOOLS                 │
       │                                         │
       │ Files │ Search │ Shell │ Git │ Tests   │
       └────────────────────┬────────────────────┘
                            │
                            ▼
                    ┌───────────────┐
                    │  PERMISSIONS  │
                    └───────┬───────┘
                            │
                            ▼
                    ┌───────────────┐
                    │ SYSTEM / CODE │
                    └───────────────┘


                            AGENT
                              │
                              ▼
                     PROVIDER INTERFACE
                              │
          ┌───────────┬───────┼────────┬───────────┐
          ▼           ▼       ▼        ▼           ▼
       OpenAI     Anthropic Gemini OpenRouter    Ollama
```

---

# 72. Princípio Definitivo

A arquitetura do Darb pode ser resumida em:

```text
                    THINK
                      │
                      ▼
                  PROVIDER
                      │
                      ▼
                    PLAN
                      │
                      ▼
                    ACT
                      │
                      ▼
                   TOOLS
                      │
                      ▼
                  REVIEW
                      │
                      ▼
                  PRESENT
                      │
                      ▼
                     TUI
```

E em termos de recursos:

```text
             ┌──────────────────────┐
             │       DARB           │
             │                      │
             │ Local Machine        │
             │                      │
             │ TUI                  │
             │ Agent Runtime        │
             │ Context              │
             │ Tools                │
             │ Memory               │
             │ Git                  │
             └──────────┬───────────┘
                        │
                  Heavy Intelligence
                        │
                        ▼
             ┌──────────────────────┐
             │     REMOTE LLM       │
             │                      │
             │ GPU / Large Model    │
             └──────────────────────┘
```

> **Darb Shell não deve ser um clone de outro coding agent.**
>
> Deve ser um **AI Development Environment terminal-first**, com uma TUI que parece uma IDE, arquitetura multi-provider, contexto inteligente e uma obsessão por eficiência.
>
> **Think → Plan → Act → Review → Present.**

---

## 73. Regra de ouro

Toda nova funcionalidade deve responder a três perguntas:

### 1. É útil?

```text
Resolve um problema real?
```

### 2. É leve?

```text
Quanto CPU/RAM/disco consome?
```

### 3. É modular?

```text
Podemos adicionar/remover sem quebrar o Core?
```

Se uma funcionalidade falhar nos três critérios, ela não deve entrar no Core.

---

# 74. Identidade Técnica

```text
DARB SHELL

Language:
Rust

Interface:
Ratatui + Crossterm

Architecture:
Modular + Event Driven

AI:
Multi Provider

Default Language:
Português

Secondary Language:
English

Primary Target:
Low Resource Hardware

Primary Philosophy:
Lightweight AI Development

Core Principle:
LLM = Brain
Darb = Nervous System
```

---

# 75. Frase do Projeto

> **Darb Shell — Code with intelligence. Run with control.**

Ou, para a identidade lusófona:

> **Darb Shell — Inteligência para criar. Controle para executar.**