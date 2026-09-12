# Darb Shell — Agents

> Especificação oficial do sistema de agentes do Darb Shell.

**Versão:** 0.1.0  
**Status:** Architecture  
**Default Language:** Português (`pt`)  
**Fallback:** English (`en`)

---

# 1. Objetivo

O sistema de Agents do Darb é responsável por transformar pedidos humanos em ações concretas de desenvolvimento.

Exemplo:

```text
"Corrige o sistema de autenticação."
```

deve transformar-se em:

```text
Understand
    ↓
Analyze
    ↓
Context
    ↓
Plan
    ↓
Execute
    ↓
Review
    ↓
Test
    ↓
Complete
```

---

# 2. Arquitetura

```text
                       USER
                        │
                        ▼
                  DARB AGENT
                        │
              ┌─────────┼─────────┐
              │         │         │
              ▼         ▼         ▼
           Planner   Executor  Reviewer
              │         │         │
              └─────────┼─────────┘
                        │
                        ▼
                    Context
                        │
                        ▼
                    Provider
                        │
                        ▼
                       LLM
                        │
                        ▼
                     Tools
                        │
                        ▼
                    Project
```

---

# 3. Princípio

O Agent não deve ser responsável diretamente por:

```text
Rendering
HTTP
Filesystem implementation
Git implementation
Storage
```

Ele coordena essas capacidades através das interfaces do sistema.

---

# 4. Agent Core

O Agent possui:

```text
Planner
Executor
Reviewer
State Machine
Tool Calling
Context Manager
Provider
Permission Manager
```

---

# 5. Planner

O Planner converte o pedido em um plano.

Input:

```text
User Request
```

Output:

```text
Execution Plan
```

Exemplo:

```text
Task:
"Adiciona dark mode."

Plan:

1. Encontrar sistema de temas
2. Identificar tokens de cor
3. Analisar configuração atual
4. Implementar dark theme
5. Atualizar componentes
6. Executar testes
7. Rever diff
```

---

# 6. Executor

O Executor transforma o plano em ações.

```text
Plan
 ↓
Action
 ↓
Tool
 ↓
Result
```

Exemplo:

```text
read_file
edit_file
search
run_command
run_tests
```

---

# 7. Reviewer

O Reviewer verifica:

```text
Syntax
Logic
Tests
Diff
Unexpected Changes
Potential Regressions
```

---

# 8. Agent Loop

Fluxo principal:

```text
┌──────────────┐
│     IDLE     │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│   ANALYZE    │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│    PLAN      │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│   EXECUTE    │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│    REVIEW    │
└──────┬───────┘
       │
   ┌───┴────┐
   │        │
   ▼        ▼
  FAIL     PASS
   │        │
   ▼        ▼
  FIX     COMPLETE
   │
   └──────────────► EXECUTE
```

---

# 9. Agent States

```text
IDLE
ANALYZING
PLANNING
WAITING_PROVIDER
WAITING_PERMISSION
EXECUTING
REVIEWING
RETRYING
COMPLETED
FAILED
CANCELLED
```

---

# 10. State Machine

Cada mudança de estado deve gerar um evento.

Exemplo:

```text
AgentStateChanged {
    from: Planning,
    to: Executing
}
```

A TUI pode então atualizar:

```text
◉ Executing changes...
```

---

# 11. Tool Calling

O Agent não executa ferramentas diretamente.

Ele cria uma intenção:

```text
ToolRequest
```

Exemplo:

```json
{
  "tool": "read_file",
  "arguments": {
    "path": "src/auth/login.ts"
  }
}
```

O Tool Registry resolve a ferramenta.

---

# 12. Tool Registry

```text
Agent
 │
 ▼
Tool Registry
 │
 ├── read_file
 ├── edit_file
 ├── search
 ├── shell
 ├── git
 └── tests
```

---

# 13. Tool Result

Toda ferramenta deve devolver um resultado estruturado.

```text
ToolResult
│
├── success
├── output
├── error
├── metadata
└── duration
```

---

# 14. Permission Flow

```text
Agent
 │
 ▼
Tool Request
 │
 ▼
Permission Manager
 │
 ├── Allowed ─────► Execute
 │
 ├── Denied ──────► Return denial
 │
 └── Ask ─────────► User
                         │
                    ┌────┴────┐
                    ▼         ▼
                  Allow     Deny
                    │         │
                    ▼         ▼
                 Execute    Stop
```

---

# 15. Context Flow

Antes de pedir ao LLM:

```text
User Request
     ↓
Project Map
     ↓
Search
     ↓
Relevant Files
     ↓
Token Budget
     ↓
Context
     ↓
Provider
```

---

# 16. Context Relevance

O Agent deve priorizar:

```text
1. Explicitly mentioned files
2. Files referenced by current code
3. Files matching search
4. Related modules
5. Project configuration
6. Relevant history
```

---

# 17. Context Exclusion

O Agent deve evitar:

```text
node_modules
.git
target
dist
build
.cache
coverage
.env
secrets
```

salvo quando explicitamente necessário.

---

# 18. Planning Policy

O Agent deve criar planos proporcionais à tarefa.

### Pequena tarefa

```text
1–3 steps
```

### Média

```text
3–7 steps
```

### Grande

```text
7+ steps
```

Não criar planos gigantes para tarefas triviais.

---

# 19. Autonomia

O Agent pode continuar autonomamente quando:

```text
Action is safe
Context is sufficient
No permission is required
No ambiguity exists
```

Deve parar e perguntar quando:

```text
Action is dangerous
Requirements are ambiguous
Multiple destructive paths exist
Credentials are needed
Architecture decision is unclear
```

---

# 20. Ambiguity

Se o pedido tiver múltiplas interpretações importantes:

```text
Do not guess silently.
```

O Agent deve perguntar.

Exemplo:

```text
"Quer que eu substitua completamente o sistema atual
ou mantenha compatibilidade com a implementação existente?"
```

---

# 21. Retry

Falhas podem ser corrigidas automaticamente quando forem seguras.

Exemplo:

```text
Test failed
 ↓
Read error
 ↓
Analyze
 ↓
Fix
 ↓
Test again
```

Limite:

```text
3 retries by default
```

---

# 22. Infinite Loop Protection

O Agent deve possuir:

```text
max_iterations
max_tool_calls
max_retries
timeout
```

Exemplo:

```toml
[agent]
max_iterations = 20
max_tool_calls = 100
max_retries = 3
timeout_seconds = 900
```

---

# 23. Reviewer Policy

O Reviewer deve procurar:

```text
Unused imports
Broken syntax
Type errors
Failed tests
Unexpected files
Unintended deletions
Security issues
Regression
```

---

# 24. Change Scope

O Agent deve respeitar o escopo da tarefa.

Se o utilizador pedir:

```text
"Corrige o login"
```

não deve reformular:

```text
Dashboard
Payments
Homepage
Database
```

sem necessidade.

---

# 25. Minimal Change Principle

Preferir:

```text
Smallest Correct Change
```

em vez de:

```text
Rewrite Everything
```

---

# 26. Agent Communication

O Agent deve comunicar progresso.

Exemplo:

```text
✓ Projeto analisado
✓ Módulo de autenticação encontrado
✓ Problema identificado
◉ Aplicando correção
○ Executando testes
○ Revisando alterações
```

---

# 27. User-Facing Language

Por padrão:

```text
Português
```

Exemplo:

```text
A analisar o projeto...
A encontrar os módulos relevantes...
A criar o plano...
A executar alterações...
A executar testes...
Concluído.
```

English:

```text
Analyzing project...
Finding relevant modules...
Creating plan...
Applying changes...
Running tests...
Completed.
```

---

# 28. Agent Identity

O Agent principal será:

```text
Darb
```

Não criar múltiplas personalidades desnecessárias.

Sub-agents são componentes técnicos, não personas.

---

# 29. Sub-Agents

No futuro, o Darb poderá utilizar sub-agents especializados.

```text
                    DARB
                      │
          ┌───────────┼───────────┐
          ▼           ▼           ▼
       Planner     Coder       Reviewer
          │           │           │
          └───────────┼───────────┘
                      ▼
                    Result
```

---

# 30. Planner Agent

Responsável por:

```text
Architecture Analysis
Task Breakdown
Dependency Analysis
Risk Identification
```

Não deve modificar arquivos diretamente.

---

# 31. Coder Agent

Responsável por:

```text
Implementation
Editing
Refactoring
Bug Fixing
```

Pode utilizar Tools.

---

# 32. Reviewer Agent

Responsável por:

```text
Code Review
Test Analysis
Diff Analysis
Regression Detection
```

Não deve fazer alterações automaticamente sem autorização do Agent principal.

---

# 33. Research Agent

Pode existir futuramente.

Responsável por:

```text
Documentation Research
Project Exploration
API Discovery
Dependency Research
```

Não deve ser ativado para todas as tarefas.

---

# 34. Sub-Agent Policy

Sub-agents:

```text
Must have a limited scope
Must have a task
Must have a context budget
Must have a timeout
Must report results
Must not bypass permissions
```

---

# 35. Agent Hierarchy

```text
Darb Agent
│
├── Planner
│
├── Coder
│
├── Reviewer
│
└── Researcher
```

O Agent principal continua responsável pela decisão final.

---

# 36. Provider Independence

O Agent não deve conhecer:

```text
OpenAI-specific APIs
Gemini-specific APIs
Anthropic-specific APIs
```

Somente:

```text
Provider Interface
```

---

# 37. Structured Responses

Sempre que possível, decisões internas devem utilizar estruturas.

Exemplo:

```text
AgentDecision
│
├── type
├── reasoning_summary
├── action
├── tool
├── arguments
└── confidence
```

O reasoning interno completo do modelo não deve ser exposto como se fosse um log técnico obrigatório.

A interface deve mostrar apenas um resumo operacional útil.

---

# 38. Agent Context

Cada execução deve possuir:

```text
AgentContext
│
├── UserRequest
├── ProjectInfo
├── RelevantFiles
├── ToolResults
├── SessionHistory
├── Memory
└── ProviderCapabilities
```

---

# 39. Agent Task

Uma task deve possuir:

```text
Task
│
├── id
├── description
├── status
├── priority
├── created_at
├── updated_at
├── steps
└── result
```

---

# 40. Task Status

```text
PENDING
RUNNING
WAITING
COMPLETED
FAILED
CANCELLED
```

---

# 41. Agent Cancellation

O utilizador pode interromper:

```text
Ctrl + C
```

O Agent deve:

```text
Stop new tool calls
Cancel running operations when possible
Preserve state
Show cancellation
```

---

# 42. Agent Memory

O Agent pode registrar:

```text
Decisions
Task Results
Project Preferences
Important Architecture Facts
```

Não deve guardar automaticamente:

```text
Passwords
API Keys
Private Credentials
Sensitive Secrets
```

---

# 43. Agent Observability

Cada execução deve possuir métricas básicas:

```text
Duration
Tool Calls
Provider Requests
Tokens
Files Changed
Retries
Errors
```

---

# 44. Agent Session

Uma sessão:

```text
Session
│
├── User Messages
├── Agent Messages
├── Tool Calls
├── Tool Results
├── Changes
├── Decisions
└── Final Result
```

---

# 45. Final Result

Ao terminar uma tarefa, o Agent deve apresentar:

```text
Summary
Changed Files
Tests
Warnings
Remaining Issues
```

Exemplo:

```text
✓ Autenticação corrigida

Alterações:
- src/auth/login.ts
- src/auth/session.ts

Testes:
✓ 18 passed

Avisos:
Nenhum
```

---

# 46. Agent Safety

Nenhum Agent:

```text
Planner
Coder
Reviewer
Researcher
Plugin Agent
```

pode ignorar:

```text
Permission Manager
Context Security
Secret Filtering
User Cancellation
Iteration Limits
```

---

# 47. Performance

Agents devem ser ativados sob demanda.

Não manter:

```text
Planner daemon
Reviewer daemon
Research daemon
```

rodando continuamente.

Preferir:

```text
Request
 ↓
Spawn/activate
 ↓
Execute
 ↓
Dispose/idle
```

---

# 48. Agent Pipeline Final

```text
USER
 │
 ▼
UNDERSTAND
 │
 ▼
CONTEXT
 │
 ▼
PLAN
 │
 ▼
PROVIDER
 │
 ▼
DECISION
 │
 ▼
PERMISSION
 │
 ▼
TOOL
 │
 ▼
RESULT
 │
 ▼
REVIEW
 │
 ├──── FAIL ────► FIX
 │                 │
 │                 └──► REVIEW
 │
 └──── PASS ────► COMPLETE
```

---

# 49. Regra de Ouro dos Agents

> **Agents devem maximizar autonomia sem ultrapassar o controle do utilizador.**

O Agent deve ser capaz de trabalhar sozinho durante uma tarefa, mas nunca deve transformar autonomia em acesso irrestrito.

---

# 50. Modelo Mental

```text
              ┌──────────────┐
              │     USER     │
              └──────┬───────┘
                     │
                     ▼
              ┌──────────────┐
              │     DARB     │
              │    AGENT     │
              └──────┬───────┘
                     │
          ┌──────────┼──────────┐
          ▼          ▼          ▼
       THINK       ACT       REVIEW
          │          │          │
          ▼          ▼          ▼
      Provider     Tools      Tests
                     │
                     ▼
                  PROJECT
```

> **Think → Act → Review.**

Esse é o ciclo fundamental do Agent do Darb Shell.