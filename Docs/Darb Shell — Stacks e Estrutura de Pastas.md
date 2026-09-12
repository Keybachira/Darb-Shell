# Darb Shell — Stacks e Estrutura de Pastas

> Definição oficial das tecnologias, dependências e organização do código do Darb Shell.

**Versão:** 0.1.0  
**Status:** Architecture  
**Runtime:** Rust  
**Interface:** TUI  
**Default Language:** Português (`pt`)  
**Secondary Language:** English (`en`)  
**Primary Target:** Low-resource hardware

---

# 1. Stack Principal

| Categoria | Tecnologia | Função |
|---|---|---|
| Linguagem | Rust | Core do sistema |
| Build | Cargo | Build e gerenciamento |
| Workspace | Cargo Workspace | Monorepo |
| TUI | Ratatui | Interface terminal |
| Terminal | Crossterm | Input/output terminal |
| Async | Tokio | Operações assíncronas |
| HTTP | Reqwest | APIs de providers |
| Serialization | Serde | JSON/configuração |
| Config | TOML | Configuração |
| Errors | thiserror | Erros tipados |
| Errors Runtime | anyhow | Contexto de erros |
| Parsing | Tree-sitter | Análise de código |
| Database | SQLite | Memória/persistência |
| Search | ripgrep | Busca no projeto |
| Git | Git CLI | Controle de versão |
| Local AI | Ollama | Provider local |
| Tests | Rust test | Testes |
| Formatting | rustfmt | Formatação |
| Linting | Clippy | Qualidade |

---

# 2. Princípios da Stack

A stack deve seguir:

```text
LIGHTWEIGHT
PERFORMANT
CROSS-PLATFORM
MODULAR
EXTENSIBLE
LOW-MEMORY
TERMINAL-NATIVE
```

O Darb não deve adicionar uma dependência apenas porque ela é popular.

Cada dependência precisa justificar:

1. utilidade;
2. custo;
3. estabilidade;
4. manutenção;
5. impacto no hardware.

---

# 3. Rust

Rust é a linguagem principal.

Motivos:

- baixo consumo;
- alta performance;
- segurança de memória;
- excelente suporte para CLI;
- excelente suporte para concorrência;
- binários nativos;
- ausência de runtime pesado;
- ótimo para Linux/Windows/macOS.

---

# 4. Ratatui

Ratatui será responsável pela TUI.

Responsabilidades:

```text
Layout
Widgets
Panels
Tabs
Dialogs
Tables
Progress
Diff
Chat
Explorer
```

Não deve existir uma camada web dentro da aplicação.

---

# 5. Crossterm

Responsável por:

```text
Keyboard
Mouse
Terminal Events
Colors
Cursor
Terminal Resize
Raw Mode
```

---

# 6. Tokio

Tokio será utilizado apenas onde concorrência/async for necessária.

Exemplos:

```text
HTTP
Provider Streaming
Process Execution
Background Tasks
Event Handling
```

Evitar criar tasks desnecessárias.

---

# 7. Reqwest

Responsável pelas comunicações HTTP dos providers.

```text
Darb
 ↓
Provider Interface
 ↓
Provider Adapter
 ↓
Reqwest
 ↓
API
```

---

# 8. Serde

Utilizado para:

```text
JSON
Provider Requests
Provider Responses
Configuration
Plugin Metadata
Project Metadata
```

---

# 9. TOML

Configurações humanas devem utilizar TOML.

Exemplo:

```toml
[interface]
language = "pt"
mouse = true

[provider]
name = "openai"
model = "..."

[performance]
profile = "eco"
```

---

# 10. Tree-sitter

Utilizado para análise estrutural de código.

Responsabilidades:

```text
Functions
Classes
Imports
Symbols
AST
Code Structure
```

A análise deve ser lazy.

---

# 11. SQLite

SQLite será utilizado para:

```text
Sessions
Messages
Memory
Tasks
Agent History
Project Metadata
```

Não deve ser utilizado para dados temporários que podem permanecer apenas em memória.

---

# 12. ripgrep

O Darb utilizará `ripgrep` para pesquisa rápida.

Preferir:

```text
ripgrep
```

a implementar um mecanismo de busca completo próprio na primeira versão.

---

# 13. Git

Inicialmente, Git será integrado através do CLI.

Exemplos:

```text
git status
git diff
git log
git branch
git add
git commit
```

Uma integração Git nativa poderá ser considerada posteriormente.

---

# 14. Ollama

Ollama será um provider opcional.

```text
Darb
 ↓
Provider Interface
 ↓
Ollama Adapter
 ↓
Local Model
```

O uso local de LLM em máquinas como AMD E2-1800 deve ser tratado como experimental.

---

# 15. Arquitetura de Crates

```text
darb-shell/
│
├── apps/
│   └── darb/
│       └── src/
│           └── main.rs
│
├── crates/
│   │
│   ├── darb-core/
│   ├── darb-agent/
│   ├── darb-tui/
│   ├── darb-tools/
│   ├── darb-context/
│   ├── darb-memory/
│   ├── darb-providers/
│   └── darb-plugins/
│
├── docs/
├── tests/
├── configs/
│
├── Cargo.toml
├── Cargo.lock
├── README.md
└── LICENSE
```

---

# 16. `darb-core`

```text
darb-core/
└── src/
    ├── config/
    ├── events/
    ├── runtime/
    ├── session/
    ├── permissions/
    ├── i18n/
    ├── errors/
    └── lib.rs
```

Responsável por:

```text
Configuration
Runtime
Events
Sessions
Permissions
Internationalization
Errors
```

---

# 17. `darb-agent`

```text
darb-agent/
└── src/
    ├── planner/
    ├── executor/
    ├── reviewer/
    ├── loop/
    ├── tool_calling/
    ├── state/
    └── lib.rs
```

---

# 18. `darb-tui`

```text
darb-tui/
└── src/
    ├── app/
    ├── layout/
    ├── components/
    │   ├── explorer/
    │   ├── chat/
    │   ├── context/
    │   ├── diff/
    │   ├── terminal/
    │   ├── git/
    │   └── tasks/
    │
    ├── widgets/
    ├── dialogs/
    ├── keybindings/
    ├── theme/
    └── lib.rs
```

---

# 19. `darb-tools`

```text
darb-tools/
└── src/
    ├── filesystem/
    ├── shell/
    ├── search/
    ├── git/
    ├── tests/
    ├── diagnostics/
    ├── editor/
    ├── registry/
    └── lib.rs
```

---

# 20. `darb-context`

```text
darb-context/
└── src/
    ├── indexer/
    ├── project_map/
    ├── selector/
    ├── tokenizer/
    ├── compression/
    ├── relevance/
    └── lib.rs
```

---

# 21. `darb-memory`

```text
darb-memory/
└── src/
    ├── session/
    ├── project/
    ├── storage/
    ├── history/
    └── lib.rs
```

---

# 22. `darb-providers`

```text
darb-providers/
└── src/
    ├── interface/
    ├── openai/
    ├── anthropic/
    ├── google/
    ├── openrouter/
    ├── ollama/
    ├── custom/
    └── lib.rs
```

---

# 23. `darb-plugins`

```text
darb-plugins/
└── src/
    ├── api/
    ├── loader/
    ├── registry/
    ├── manifest/
    └── lib.rs
```

---

# 24. Dependências entre Crates

```text
                    darb-tui
                       │
                       ▼
                   darb-core
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
     darb-agent    darb-context  darb-memory
          │
          ▼
     darb-tools
          │
          ▼
   darb-providers
```

Regra:

> Dependências devem fluir para baixo, nunca criar ciclos.

---

# 25. Aplicação

O binário principal será:

```text
apps/darb
```

Responsabilidade:

```text
Bootstrap
Config Loading
Runtime Initialization
TUI Initialization
Shutdown
```

A lógica de negócio não deve ficar em `main.rs`.

Idealmente:

```rust
fn main() {
    darb::run();
}
```

---

# 26. Configuração

Arquivos:

```text
configs/
├── default.toml
└── profiles/
    ├── eco.toml
    ├── fast.toml
    ├── smart.toml
    └── local.toml
```

Configuração por projeto:

```text
.darb/
└── config.toml
```

---

# 27. Performance

O Darb deve evitar:

```text
Electron
Chromium
WebView
Heavy GUI
Continuous Indexing
60 FPS Rendering
Large Memory Cache
```

Preferir:

```text
Native Rust
Event-driven rendering
Lazy loading
Lazy indexing
On-demand analysis
Small caches
Remote inference
```

---

# 28. Regra de Dependências

Antes de adicionar uma dependência:

```text
Does it solve a real problem?
        ↓
Can Rust standard library solve it?
        ↓
Is the dependency maintained?
        ↓
What is its memory cost?
        ↓
What is its compile-time cost?
        ↓
Is it worth adding?
```

---

# 29. Target Platforms

Primeira prioridade:

```text
Linux
```

Segunda:

```text
Windows
```

Terceira:

```text
macOS
```

A arquitetura deve permanecer cross-platform.

---

# 30. Stack Final

```text
DARB SHELL

Rust
├── Ratatui
├── Crossterm
├── Tokio
├── Reqwest
├── Serde
├── TOML
├── Tree-sitter
├── SQLite
├── thiserror
└── anyhow

External
├── Git
├── ripgrep
└── Ollama (optional)
```