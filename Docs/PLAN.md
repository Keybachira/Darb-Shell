# Darb Shell — planos de implementação

> Planos derivados do diagnóstico de visão. Cada plano diz: **o que falta**,
> **os passos**, **como se sabe que acabou**, e **o que fica de fora**.
> Nenhum passo começa sem o anterior estar verde.
>
> Fontes: `Docs/design.md` (a interface), `Docs/WORKLOGIC.md` (as regras),
> e a visão (§12 workspace, §16-19 modos, §20-23 multi-provider).

Ordem: **P0 → P1 → P2 → P3**. O browser não aparece — ver `design.md` §5.8.

| Plano | Alcance | Depende de |
|---|---|---|
| P0 | Workspace — o estado único | — |
| P1 | Modos da IA (Mentor/Assist/Autonomous) | P0 |
| P2 | AI Router — provider e modelo por tarefa | P1 |
| P3 | Providers adicionais + Desktop | P2 |

---

## P0 — Workspace: o estado único

### Porquê primeiro

A §28 da visão diz que o Darb deve saber qual o projecto aberto, que
ficheiros estão activos, que erros apareceram, qual o estado do Git. Hoje
não há nada que responda a isso: `darb-context` indexa ficheiros, e cada
painel da TUI tem o seu próprio canto de estado.

Sem isto, P1 e P2 não têm onde viver — e o Desktop (que vai precisar do
mesmo estado) seria um fork.

### O que falta

- Não existe `Workspace` em lado nenhum. `darb-core/session` era um stub
  de 1 linha e foi removido na limpeza.
- `App` tem 40+ campos soltos, cada um preenchido por `apps/darb` numa
  tarefa própria (`set_files`, `push_terminal`, `set_git`…).
- `App` não tem: terminal activo, servidores a correr, última mensagem de
  erro por ficheiro, ficheiros activos.
- `AgentMode` tem **6 variantes** (`Auto Plan Code Debug Review Explain`)
  que são *tarefas* — o eixo de poder da visão não existe.

### Passos

1. **`Workspace` em `darb-core`** — struct de estado puro, sem I/O:
   `project_root`, `open_file`, `dirty_files`, `active_terminal`,
   `running_processes: Vec<Process>`, `diagnostics: Vec<Diagnostic>`,
   `git: GitSnapshot`, `mode: AiMode`. Sem `tokio`, sem `fs`.
2. **Eventos** — `WorkspaceChanged` no `DarbEvent`; `apps/darb` publica
   quando algo muda. A TUI recebe, não lê.
3. **`AiMode`** em `darb-core`: `Mentor | Assist | Autonomous`. Fica ao
   lado do `AgentMode` actual (eixos separados, `design.md` §5.5), não o
   substitui.
4. **Migrar `App`** — os 40 campos passam a ser lidos de `Workspace`
   onde fizer sentido. `App` continua a ser o *view-model*; `Workspace` é
   a *fonte*. Um passo de cada vez, sem reescrita de uma vez.
5. **Testes** — `Workspace` é puro, logo testável sem terminal: o teste
   é "aplicar `git` muda `git_changes`".

### Pronto quando

- `Workspace` tem testes a cobrir cada mutação. ✅ **feito** — 30 testes
- `App` lê modo e estado de `Workspace`, não tem campos duplicados. ✅
  **feito** — `git_branch`/`git_changes`/`git_staged`/`git_untracked` e a
  segunda cópia no `Ui` foram removidas; o `App` lê `workspace.git` através
  de `git_branch()` e `git_change_summary()`.
- `WorkspaceChanged` publicado e consumido. ✅ **feito** — o `Ui` tem o
  `EventBus` e emite; o `App::apply_event` adopta o estado inteiro.
- `cargo clippy --all-targets -D warnings` limpo. ✅
- `cargo test --workspace` verde. ✅ **feito** — 209 testes

### O que NÃO foi migrado (e porquê)

O passo 4(Migration) migrou o que era *duplicado*. Os restantes campos do
`App` (ficheiros explorados, linhas de terminal, preview, diff, contexto)
são **dados de apresentação**, não estado do projecto: o `Workspace` não os
tem porque não descrevem o projecto. Copiá-los para lá seria mover a
duplicação em vez de a eliminar.

Espera-se que o `Workspace` os absorva quando P1 começar a usá-los como
estado real (ficheiros activos, diagnósticos, processos).

### Fora

- `ProjectManager` multi-projecto (trocar de raiz) — Fase posterior.
- `ProcessManager` a spawnar processos — o registo existe, o spawn não.

---

## P1 — Modos da IA

### O quê

§16-19 da visão. Três níveis de poder, e o utilizador escolhe:

| Modo | A IA pode | A IA não pode |
|---|---|---|
| `Mentor` | explicar, sugerir | tocar em código, correr comandos |
| `Assist` | propor alterações | aplicar sem confirmação |
| `Autonomous` | editar, correr, testar | `sudo`, `git push` (sempre negado) |

O `PermissionManager` **já decide** — `Allow | Ask | Deny` por categoria,
com `sudo` sempre negado e comandos destrutivos sempre a pedir. O que
falta é a camada acima: um perfil de modo que molde as permissões e o
prompt.

### Passos

1. **Traduzir modo → permissões** em `darb-core/permissions`:
   `Mentor` ⇒ escrita `Deny`; `Assist` ⇒ leitura `Allow`, escrita `Ask`;
   `Autonomous` ⇒ leitura e escrita `Allow`, shell `Ask`. `sudo` e
   `git push` continuam `Deny` em qualquer modo.
2. **Testes de tabela** — os 3 modos × 6 categorias de ferramenta. É a
   parte que mais merece testes: é aqui que a segurança vive.
3. **Prompt por modo** — `Mentor` instrui "explica, não edites". Uma
   frase por modo; se precisar de mais, é sinal de que o prompt está a
   fazer trabalho de lógica.
4. **UI** — `Ctrl+O` cicla `AgentMode` (já existe). Ciclar `AiMode` é
   `Ctrl+Shift+M`, e mostra-se no `TOOLS · PERMISSIONS` do painel
   Context (`design.md` §5.10) e no rodapé.
5. **Consequência** — em `Mentor`, `KeyOutcome::SaveFile` e
   `TerminalCommand` não devem ser emitidos. Teste que o garante.

### Pronto quando

- Os 18 estados (3 modos × 6 categorias) têm teste.
- Mudar de modo muda o que o `Agent` pode fazer, não só o que se mostra.
- O rodapé reflecte o modo real.

### Fora

- Nível de poder **por ferramenta** (permitir `read`, negar `shell`, mas
  deixar `edit`). É refinamento para quando os 3 modos estiverem usados.

---

## P2 — AI Router

### O quê

§20-23: provider agnóstico, chaves do utilizador, e escolher o modelo
certo para a tarefa — em vez de mandar tudo para o mais caro.

Hoje `ProviderRequest` leva `model: String` explícito: **não há
routing**, há escolha manual. E há só `openai` implementado.

### Passos

1. **Catálogo de modelos** — `darb-providers`:
   `Model { id, provider, cost_tier, speed_tier, strengths: &[TaskKind] }`.
   Declarativo, em `configs/models.toml`, para não levar a lista no binário.
2. **`TaskKind`** — `Reason | Code | Quick | Long`. É o vocabulário do
   routing; 4 valores chegam.
3. **Router** — `darb-providers/src/router.rs`: dado um `TaskKind` e um
   orçamento, escolhe um `Model`. Regra determinística primeiro (ordem de
   preferência na config); routing por LLM **depois**, e só se o
   determinístico falhar.
4. **Local primeiro** — num E2-1800, um modelo local pequeno é mais
   rápido do que uma ida à rede. É uma regra, não um modelo.
5. **UI** — o `MODEL` do painel Context passa a mostrar *porquê*:
   `code → local-1.4 (rápido)`, não só o nome.
6. **Fallback** — se o provider escolhido falhar, o router tenta o
   seguinte da lista e **diz que tentou** (regra §7 do `design.md`: nunca
   esconder uma falha atrás de um número bonito).

### Pronto quando

- Trocar de modelo na config muda o comportamento sem tocar em código.
- O router é testável sem rede: dada uma config e um `TaskKind`, escolhe
  o mesmo modelo sempre.
- Cada escolha é explicável ao utilizador.

### Fora

- Jev / TypeSafe como camada de decisão rápida. O documento de visão
  marca-o como **investigação**, não dependência. A abstracção
  `TaskKind` deixa a porta aberta sem depender disso.

---

## P3 — Providers adicionais + Desktop

### Providers

`anthropic`, `google`, `ollama`, `openrouter`, `custom` — os stubs
foram removidos na limpeza (`design.md` §2). Cada um é `impl Provider`
por cima de `interface`, com o mesmo formato de `ProviderRequest` /
`StreamEvent`. `ollama` primeiro: é local, e é o que corre na máquina de
testes.

### Desktop (Slint)

§08/§10: mesmo Core, segundo renderer. **Só depois de P0 e P1**, porque
dependem de um `Workspace` fora da TUI — sem isso, o Desktop seria um
fork do `App` e o Core deixaria de ser único.

Ordem dentro do Desktop: layout primeiro (a mesma `shell_layout`, outro
renderizador) → painéis → o resto. A geometria já é uma função pura
testada, o que torna isto barato.

---

## Regra transversal

Cada plano tem de deixar o `Cargo.lock` **igual ou menor**. Se um passo o
faz crescer sem outro reduzir, o passo está errado — e é por isso que
`design.md` §2 tem a tabela de dependências.

Nenhuma destas fases reintroduz o browser, o editor com highlight, os
plugins ou o multi-agente. As quatro estão em `design.md` §10 com a
razão.
