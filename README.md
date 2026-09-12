# Darb Shell

> AI-native coding environment for the terminal, lightweight first (Rust + Ratatui + Crossterm).


Stack e estrutura: `Docs/Darb Shell — Stacks e Estrutura de Pastas.md`
Arquitetura: `Docs/Darb Shell — Arquitetura do Sistema.md`
Regras: `Darb Shell — Regras para Contribuição e Modificação do Código.md`

## Workspace

```text
apps/darb
crates/darb-core
crates/darb-agent
crates/darb-tui
crates/darb-tools
crates/darb-context
crates/darb-memory
crates/darb-providers
crates/darb-plugins
configs/
tests/
```

## Verificação

```bash
cargo fmt
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
```
