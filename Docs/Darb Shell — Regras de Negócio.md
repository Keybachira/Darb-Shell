# Darb Shell — Regras de Negócio

> Regras funcionais, operacionais e de segurança que definem como o Darb Shell deve comportar-se.

**Versão:** 0.1.0  
**Status:** Architecture  
**Default Language:** Português (`pt`)  
**Fallback:** English (`en`)

---

# 1. Princípios Fundamentais

O Darb deve seguir:

```text
SAFE
PREDICTABLE
TRANSPARENT
LIGHTWEIGHT
USER-CONTROLLED
PROVIDER-AGNOSTIC
```

---

# 2. Regra de Autonomia

O Darb pode analisar e propor alterações automaticamente.

Porém:

> **O Darb nunca deve assumir controle irrestrito do computador.**

A execução de ações depende do nível de permissão.

---

# 3. Regra de Permissões

Toda Tool deve declarar:

```text
name
description
risk_level
required_permission
```

Níveis:

```text
SAFE
MODERATE
DANGEROUS
```

---

# 4. Operações Safe

Podem executar automaticamente quando permitido pela configuração.

```text
read_file
list_directory
search
git_status
git_diff
```

---

# 5. Operações Moderate

Normalmente devem solicitar confirmação.

```text
write_file
edit_file
create_file
install_package
```

---

# 6. Operações Dangerous

Devem exigir confirmação explícita.

```text
delete_file
run_shell
sudo
git_push
git_reset
database_drop
system_commands
```

---

# 7. Regra de Destruição

O Darb nunca deve apagar dados silenciosamente.

Antes de executar uma ação destrutiva:

```text
Explain
   ↓
Show target
   ↓
Ask confirmation
   ↓
Execute
```

---

# 8. Regra de Git Push

`git push` nunca deve ser executado silenciosamente.

Mesmo que:

```toml
shell = "allow"
```

o Darb deve tratar `git push` como operação de alto risco.

---

# 9. Regra de Sudo

Por padrão:

```text
sudo = DENY
```

O Agent não deve executar comandos `sudo` automaticamente.

---

# 10. Regra de Secrets

O Darb nunca deve enviar automaticamente:

```text
.env
.env.*
private keys
SSH keys
credentials
API keys
tokens
passwords
```

para um provider.

---

# 11. Contexto Mínimo

O Darb deve enviar ao LLM apenas o contexto necessário.

```text
Minimum Necessary Context
```

Não enviar o projeto inteiro quando apenas alguns arquivos são relevantes.

---

# 12. Context Filtering

Arquivos sensíveis devem possuir prioridade de exclusão.

Exemplo:

```text
.env
.env.local
*.pem
*.key
credentials.*
secrets.*
```

---

# 13. Regra de Provider

O Agent não pode depender diretamente de:

```text
OpenAI
Anthropic
Gemini
Ollama
```

Deve depender apenas da:

```text
Provider Interface
```

---

# 14. Troca de Provider

O utilizador pode trocar o provider sem alterar o projeto.

Exemplo:

```bash
darb provider openai
darb provider ollama
darb provider gemini
```

---

# 15. Regra de Idioma

Idioma padrão:

```text
pt
```

Segundo idioma:

```text
en
```

O sistema deve utilizar fallback:

```text
pt → en
```

---

# 16. Regra de Interface

A TUI deve ser:

```text
Keyboard-first
Mouse-friendly
Terminal-native
Accessible
Responsive
```

---

# 17. Regra de Performance

O Darb deve priorizar:

```text
CPU efficiency
RAM efficiency
Disk efficiency
Network efficiency
```

Nenhuma funcionalidade deve criar trabalho pesado em background sem necessidade.

---

# 18. Regra de Indexação

Indexação completa contínua é proibida por padrão.

Preferir:

```text
Lazy Indexing
On-demand Analysis
Incremental Updates
```

---

# 19. Regra de Contexto

Cada request deve respeitar:

```text
Provider Context Limit
Configured Token Budget
Local Memory Constraints
```

---

# 20. Regra de Alterações

Antes de modificar código, o Agent deve:

```text
Understand
 ↓
Plan
 ↓
Modify
 ↓
Review
```

---

# 21. Regra de Review

Alterações geradas pelo Agent devem poder ser revisadas.

A TUI deve fornecer:

```text
Diff
Changed Files
Added Files
Deleted Files
```

---

# 22. Regra de Testes

Quando o projeto possui testes, o Agent deve preferencialmente executá-los após alterações relevantes.

Fluxo:

```text
Modify
 ↓
Test
 ↓
Failure?
 ├── YES → Analyze → Fix
 └── NO  → Continue
```

---

# 23. Regra de Falha

Se uma Tool falhar:

```text
Tool Error
 ↓
Analyze
 ↓
Retry if safe
 ↓
Ask User if necessary
```

O Agent não deve repetir indefinidamente.

---

# 24. Retry Policy

Número máximo padrão:

```text
3 retries
```

Após atingir o limite:

```text
Stop
Explain
Ask for intervention
```

---

# 25. Regra de Loop

O Agent deve possuir limite de iterações.

Exemplo:

```toml
[agent]
max_iterations = 20
```

O limite impede loops infinitos.

---

# 26. Regra de Transparência

O utilizador deve conseguir saber:

```text
What Darb is doing
Why it is doing it
Which files it touched
Which tools it used
What failed
What changed
```

---

# 27. Regra de Tool Calls

Antes de uma ação importante, a TUI deve mostrar:

```text
Tool
Target
Action
Permission
Status
```

---

# 28. Regra de Estado

O Agent deve possuir estados claros:

```text
IDLE
ANALYZING
PLANNING
WAITING
EXECUTING
REVIEWING
COMPLETED
FAILED
CANCELLED
```

---

# 29. Cancelamento

O utilizador pode cancelar uma operação.

Exemplo:

```text
Ctrl + C
```

Durante uma operação:

```text
Running...
Press Ctrl+C to cancel
```

O cancelamento deve ser seguro.

---

# 30. Regra de Memória

O Darb pode armazenar:

```text
Project Preferences
Project Architecture
Sessions
Decisions
Agent History
```

Mas não deve armazenar secrets por padrão.

---

# 31. Regra de `.darb`

O diretório:

```text
.darb/
```

pertence ao Darb.

Ele pode conter:

```text
config
memory
sessions
metadata
```

---

# 32. Regra de Compatibilidade

Alterações de configuração devem tentar preservar compatibilidade.

Configurações antigas não devem quebrar silenciosamente.

---

# 33. Regra de Plugins

Plugins são tratados como código externo.

Portanto:

```text
Plugin
 ↓
Permission Boundary
 ↓
Plugin API
```

Nunca assumir confiança total.

---

# 34. Regra de UI

A interface deve permanecer útil mesmo quando:

```text
No internet
No provider
No Git
No project
```

Por exemplo, o utilizador ainda deve conseguir abrir o terminal e navegar no filesystem.

---

# 35. Regra Offline

Sem internet:

```text
Filesystem → YES
Search → YES
Git → YES
Terminal → YES
Tests → YES
Memory → YES
Remote AI → NO
Local AI → YES, if configured
```

---

# 36. Regra de Provider Offline

Se o provider remoto estiver indisponível:

```text
Remote failure
 ↓
Detect local provider
 ↓
If configured → offer fallback
 ↓
Otherwise → explain
```

Nunca trocar silenciosamente de provider se isso alterar custo ou comportamento significativamente.

---

# 37. Regra de Custos

O Darb deve informar quando uma ação pode consumir recursos externos relevantes.

Exemplo:

```text
Large context
High token usage
Expensive model
```

---

# 38. Regra de Modelo

O utilizador controla:

```text
Provider
Model
Profile
Context Budget
```

O Agent não deve trocar para um modelo pago mais caro sem autorização.

---

# 39. Regra de Comandos

O Agent deve detectar comandos potencialmente perigosos.

Exemplos:

```text
rm -rf
sudo
git reset --hard
git push --force
DROP DATABASE
format
```

Esses comandos devem possuir proteção adicional.

---

# 40. Regra de Segurança

Prioridade:

```text
User Safety
     ↓
Data Safety
     ↓
System Safety
     ↓
Task Completion
```

Nunca sacrificar segurança apenas para completar uma tarefa.

---

# 41. Regra de Autonomia do Agent

O Agent pode:

```text
Analyze
Plan
Search
Read
Suggest
Edit
Test
Review
```

Mas ações destrutivas ou de alto impacto dependem de autorização.

---

# 42. Regra de Explicação

Quando uma operação é recusada, o Darb deve explicar:

```text
What was blocked
Why it was blocked
How the user can allow it
```

---

# 43. Regra de Consistência

A mesma ação deve possuir comportamento previsível independentemente do provider.

OpenAI, Gemini e Ollama não devem alterar as regras de segurança do Darb.

---

# 44. Regra de Core

Nenhum provider, plugin ou ferramenta específica deve dominar o Core.

O Core permanece genérico.

---

# 45. Regra de Extensibilidade

Novos:

```text
Providers
Tools
Commands
Themes
Languages
Integrations
```

devem poder ser adicionados sem reescrever o Agent.

---

# 46. Regra Final

Quando existir conflito entre:

```text
Performance
Convenience
Autonomy
Safety
```

a prioridade será:

```text
Safety
 ↓
User Control
 ↓
Correctness
 ↓
Performance
 ↓
Convenience
```

---

# 47. Resumo

O Darb deve ser:

```text
LIGHT
SAFE
TRANSPARENT
PREDICTABLE
MODULAR
PROVIDER-AGNOSTIC
USER-CONTROLLED
```

> **O Agent pode pensar sozinho. A máquina continua pertencendo ao utilizador.**