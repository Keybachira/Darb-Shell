# Darb Shell

> AI-native coding environment for the terminal, lightweight first (Rust + Ratatui + Crossterm).


Stack e estrutura: `Docs/Darb Shell — Stacks e Estrutura de Pastas.md`
Arquitetura: `Docs/Darb Shell — Arquitetura do Sistema.md`
Regras: `Darb Shell — Regras para Contribuição e Modificação do Código.md`
Design da TUI: `Docs/design.md`
Como o projecto avança: `Docs/WORKLOGIC.md`
Planos de implementação: `Docs/PLAN.md` (P0 Workspace → P3 Desktop)

## Workspace

```text
apps/darb            # bootstrap: despacha comandos, liga tudo
crates/darb-core     # config, events, permissions, i18n, errors
crates/darb-tools    # filesystem, shell, search, git, editor, registry
crates/darb-context  # indexer, project map, selector, tokenizer, relevance
crates/darb-memory   # sessões e histórico em SQLite
crates/darb-providers# trait Provider + adapter openai
crates/darb-agent    # loop, planner, executor, reviewer, tool calling
crates/darb-tui      # apresentação (Ratatui + Crossterm)
configs/             # default.toml + perfis (eco, fast, local, smart)
```

Não existe `darb-plugins`: foi removido por estar vazio (ver
`Docs/WORKLOGIC.md` §4). Volta quando houver um plugin real.

## Verificação

```bash
cargo fmt
cargo check --workspace -j 1
cargo clippy --workspace -j 1 -- -D warnings
cargo test --workspace -j 1
```

O `-j 1` não é opcional numa máquina com 8 GB: compilar em paralelo
mata o processo por falta de memória antes de chegar ao nosso código.
