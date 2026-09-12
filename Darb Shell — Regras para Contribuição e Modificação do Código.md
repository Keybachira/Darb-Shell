# Darb Shell — Regras para Contribuição e Modificação do Código

> **Documento obrigatório para qualquer pessoa, agente de IA ou ferramenta automatizada que altere o código do Darb Shell.**

**Versão:** 1.0  
**Projeto:** Darb Shell  
**Idioma da interface:** Português (`pt`)  
**Idioma técnico do código:** Inglês

---

## 1. Objetivo

Este documento define as regras que devem ser seguidas por qualquer entidade que leia, crie, modifique, mova ou remova código do Darb Shell.

Aplica-se a:

- Desenvolvedores humanos
- Agentes de IA
- Agentes autónomos
- Copilots
- Ferramentas de geração de código
- Scripts automatizados
- Bots
- Plugins
- Contribuidores externos

O objetivo é garantir que o projeto continue:

- simples;
- seguro;
- previsível;
- performático;
- modular;
- fácil de manter;
- consistente;
- compatível com a arquitetura do Darb.

---

# 2. Regra Principal

> **Não altere código que você não entende.**

Antes de modificar qualquer parte do projeto, o contribuidor deve entender:

1. onde o código está;
2. qual é a responsabilidade daquele módulo;
3. quem utiliza aquele módulo;
4. quais interfaces ele expõe;
5. quais dependências possui;
6. quais regras de negócio afetam aquela parte;
7. quais efeitos colaterais podem acontecer.

Nunca modificar simplesmente porque:

> "parece que é aqui que devo mexer."

---

# 3. Hierarquia das Regras

Quando duas decisões entrarem em conflito, utilizar esta prioridade:

```text
1. Segurança
2. Regras de negócio
3. Arquitetura
4. Correção
5. Compatibilidade
6. Performance
7. Manutenibilidade
8. Conveniência
```

Uma solução conveniente nunca deve quebrar uma regra de segurança ou arquitetura.

---

# 4. Antes de Codificar

Todo contribuidor deve seguir:

```text
ENTENDER
   ↓
INSPECIONAR
   ↓
PLANEJAR
   ↓
IMPLEMENTAR
   ↓
VERIFICAR
   ↓
TESTAR
   ↓
REVISAR
```

Não começar imediatamente a escrever código.

---

# 5. Primeiro Entenda o Projeto

Antes de uma alteração significativa, consultar:

```text
README.md
ARCHITECTURE.md
docs/
Cargo.toml
crates/
apps/
tests/
```

Documentos importantes:

```text
docs/ARCHITECTURE.md
docs/STACKS.md
docs/BUSINESS-RULES.md
docs/AGENTS.md
```

Se existir documentação específica do módulo que será alterado, ela também deve ser consultada.

---

# 6. Não Inventar Arquitetura

Não criar uma nova arquitetura porque uma solução parece mais conveniente.

Antes de introduzir:

- crate;
- serviço;
- sistema de eventos;
- abstraction layer;
- provider;
- ferramenta;
- banco;
- framework;
- biblioteca;

verificar se já existe uma solução equivalente.

### Regra

> **Reuse antes de criar.**

Se uma nova abstração for realmente necessária, ela deve ser justificada.

---

# 7. Respeitar a Estrutura de Crates

A estrutura modular do Darb deve ser preservada.

```text
apps/darb
      │
      ▼
darb-core
      │
      ├── darb-agent
      ├── darb-context
      ├── darb-memory
      ├── darb-tools
      ├── darb-providers
      ├── darb-plugins
      └── darb-tui
```

Cada crate possui uma responsabilidade específica.

Não colocar lógica de negócio aleatoriamente em:

```text
main.rs
TUI
widgets
providers
tools
```

---

# 8. `main.rs` Deve Permanecer Pequeno

`apps/darb/src/main.rs` deve funcionar principalmente como bootstrap.

Responsabilidades permitidas:

- inicializar configuração;
- inicializar runtime;
- iniciar componentes;
- iniciar aplicação;
- tratar argumentos CLI;
- iniciar TUI.

Evitar colocar em `main.rs`:

- lógica de agente;
- lógica de ferramentas;
- chamadas HTTP;
- lógica de contexto;
- regras de negócio;
- persistência;
- lógica complexa.

---

# 9. Código Deve Ter Uma Responsabilidade Clara

Cada módulo, função, struct e componente deve possuir uma responsabilidade clara.

Evitar funções que façam:

```text
ler arquivo
→ chamar IA
→ modificar arquivo
→ executar testes
→ atualizar TUI
→ salvar memória
```

Tudo ao mesmo tempo.

Preferir:

```text
Context
  ↓
Agent
  ↓
Tool
  ↓
Result
  ↓
Event
  ↓
TUI
```

---

# 10. Não Duplicar Código

Antes de criar uma função:

```rust
fn something_new()
```

procurar se já existe comportamento equivalente.

Evitar:

```text
duplicate logic
duplicate validation
duplicate provider handling
duplicate file parsing
duplicate permission checks
duplicate UI state
```

Se a mesma regra aparece em vários lugares, considerar extrair uma abstração apropriada.

---

# 11. Não Criar Abstrações Prematuramente

Nem todo código precisa de:

```text
trait
factory
manager
service
registry
provider
wrapper
adapter
```

Uma abstração deve resolver um problema real.

Não criar abstrações apenas para:

> "deixar mais profissional."

---

# 12. Código Rust

O código Rust deve seguir:

- `rustfmt`;
- `clippy`;
- ownership idiomático;
- borrowing correto;
- erros explícitos;
- tipos fortes;
- módulos pequenos;
- APIs claras.

Preferir:

```rust
Result<T, DarbError>
```

em vez de esconder erros.

Evitar:

```rust
unwrap()
expect()
panic!()
```

quando o erro puder acontecer em runtime.

`unwrap()` só deve ser utilizado quando a invariância estiver claramente garantida.

---

# 13. Tratamento de Erros

Erros devem ser:

- detectáveis;
- explicáveis;
- contextualizados;
- recuperáveis quando possível.

Um erro como:

```text
Error
```

não é suficiente.

Preferir algo como:

```text
Não foi possível ler o arquivo:
src/main.rs

Motivo:
Permissão negada.
```

Erros internos podem permanecer em inglês quando necessário para APIs técnicas, mas mensagens destinadas ao usuário devem seguir o sistema de internacionalização.

---

# 14. Nunca Esconder Erros

É proibido esconder silenciosamente:

```rust
let _ = operation();
```

quando a operação for relevante.

Se uma falha puder afetar o resultado, ela deve ser:

- retornada;
- registrada;
- exibida;
- tratada.

---

# 15. Internacionalização

O idioma padrão do Darb é:

```text
pt
```

Inglês:

```text
en
```

Strings de interface não devem ser espalhadas pelo código.

Evitar:

```rust
println!("Arquivo criado com sucesso");
```

Preferir o sistema de i18n:

```rust
t!("file.created")
```

Estrutura:

```text
i18n/
├── pt.toml
└── en.toml
```

---

# 16. Código Interno em Inglês

Identificadores técnicos devem permanecer em inglês.

Preferir:

```rust
struct ProjectContext
struct ToolRequest
struct AgentState
struct ProviderConfig
```

Evitar:

```rust
struct ContextoProjeto
struct PedidoFerramenta
struct EstadoAgente
```

Português é o idioma padrão da interface.

Inglês é o idioma técnico do código.

---

# 17. Comentários

Comentários devem explicar:

> **por quê**

e não apenas:

> **o quê**

Ruim:

```rust
// Incrementa o contador
counter += 1;
```

Bom:

```rust
// Limitamos o número de chamadas para impedir loops infinitos do agente.
counter += 1;
```

Não comentar código óbvio.

---

# 18. Não Apagar Código Sem Entender

Nunca remover código simplesmente porque:

- parece inútil;
- não está sendo usado imediatamente;
- parece antigo;
- parece duplicado;
- a IA acha que não é necessário.

Antes de remover:

1. procurar referências;
2. verificar testes;
3. verificar documentação;
4. verificar integrações;
5. verificar plugins;
6. verificar configuração.

---

# 19. Alterações Devem Ser Pequenas

Preferir:

```text
1 problema
1 mudança
1 objetivo
```

Evitar:

```text
corrigir bug
+ refatorar arquitetura
+ mudar UI
+ atualizar dependências
+ renomear 50 arquivos
```

na mesma alteração.

Quanto menor a alteração, menor o risco.

---

# 20. Regra de Escopo

Se a tarefa é:

> "corrigir o parser"

não alterar:

```text
TUI
tema
provider
sistema de memória
CLI
configuração
```

sem necessidade.

### Regra

> **Não conserte coisas fora do escopo sem autorização ou necessidade técnica real.**

---

# 21. Não Fazer Refactor Gratuito

Não transformar uma tarefa simples em um grande refactor.

Evitar:

```text
"Já que estou aqui, vou reescrever tudo."
```

Se um refactor for necessário:

1. explicar o motivo;
2. definir o escopo;
3. separar a mudança quando possível;
4. testar antes e depois.

---

# 22. Dependências

Não adicionar uma dependência apenas porque:

> "é mais fácil."

Antes de adicionar uma biblioteca:

- verificar se a funcionalidade já existe;
- verificar custo de memória;
- verificar impacto no binário;
- verificar manutenção;
- verificar compatibilidade;
- verificar licença;
- verificar se realmente é necessária.

O Darb prioriza baixo consumo de recursos.

---

# 23. Regra Especial para o Hardware

O Darb deve continuar adequado para máquinas de baixo recurso.

Referência mínima de desenvolvimento:

```text
CPU: AMD E2-1800
RAM: 8 GB
```

Evitar:

- processos permanentes;
- indexação contínua;
- threads desnecessárias;
- polling agressivo;
- rendering constante;
- cache gigantesco;
- consumo excessivo de RAM;
- dependências pesadas.

---

# 24. Não Usar Chromium por Conveniência

O Darb é terminal-native.

Não adicionar:

```text
Electron
Chromium
WebView pesada
```

para resolver problemas que podem ser resolvidos nativamente.

---

# 25. Eventos em Vez de Acoplamento

Quando componentes precisarem comunicar mudanças, preferir o sistema de eventos do Darb.

Exemplo:

```text
Agent
  ↓
AgentEvent
  ↓
Event Bus
  ↓
TUI
```

Evitar criar dependências diretas desnecessárias entre componentes.

---

# 26. TUI Não Deve Conter Lógica de Negócio

A TUI apresenta informações.

Ela não deve decidir:

```text
qual ferramenta executar
qual arquivo modificar
qual provider utilizar
qual permissão conceder
```

A TUI pode:

```text
receber evento
mostrar estado
capturar input
enviar comando
```

---

# 27. Agent Não Deve Implementar Ferramentas

O Agent deve solicitar:

```text
ToolRequest
```

e não implementar diretamente:

```text
filesystem
shell
git
search
```

Fluxo:

```text
Agent
  ↓
ToolRequest
  ↓
ToolRegistry
  ↓
Tool
  ↓
ToolResult
```

---

# 28. Nunca Ignorar o Sistema de Permissões

Nenhum código deve criar um caminho alternativo para executar operações perigosas.

É proibido:

```text
bypass permission
skip confirmation
disable safety
execute silently
```

Especialmente para:

```text
sudo
rm -rf
git reset --hard
git push --force
database drop
system commands
```

---

# 29. Segredos

Nunca adicionar ao código:

```text
API keys
tokens
passwords
private keys
credentials
```

Nunca fazer commit de:

```text
.env
.env.*
*.pem
*.key
credentials.*
secrets.*
```

Também não enviar segredos automaticamente para providers.

---

# 30. Alterações em Arquivos

Toda alteração feita pelo Agent deve ser rastreável.

O sistema deve conseguir identificar:

```text
arquivo
linha
tipo de alteração
ferramenta utilizada
momento
```

Sempre que possível, alterações devem aparecer como diff.

---

# 31. Não Modificar Binários ou Arquivos Gerados

Não editar manualmente arquivos gerados automaticamente sem necessidade.

Exemplos:

```text
target/
build/
dist/
.cache/
coverage/
```

Esses diretórios devem permanecer fora do fluxo normal de edição.

---

# 32. Testes

Toda mudança relevante deve possuir verificação adequada.

Antes de considerar uma tarefa concluída:

```text
cargo fmt
cargo check
cargo clippy
cargo test
```

Quando aplicável.

Para mudanças específicas:

```text
unit tests
integration tests
snapshot tests
TUI tests
provider tests
tool tests
```

---

# 33. Não Fazer Testes Falsos

Nunca criar testes apenas para:

```text
passar no CI
aumentar coverage
esconder um bug
```

Um teste deve verificar comportamento real.

---

# 34. Testar o Que Foi Alterado

Se foi alterado:

```text
darb-tools
```

testar ferramentas.

Se foi alterado:

```text
darb-providers
```

testar providers.

Se foi alterado:

```text
darb-context
```

testar seleção/indexação de contexto.

Não assumir que:

```text
"compilou = está correto"
```

---

# 35. Verificação Antes de Commit

Antes de criar um commit:

```text
[ ] código formatado
[ ] cargo check passou
[ ] clippy analisado
[ ] testes executados
[ ] diff revisado
[ ] nenhuma alteração inesperada
[ ] nenhum segredo
[ ] nenhum arquivo desnecessário
[ ] documentação atualizada quando necessário
```

---

# 36. Git

Commits devem ser pequenos e semânticos.

Preferir:

```text
feat: add provider profile system
fix: prevent agent retry loop
refactor: simplify context selector
docs: update agent architecture
test: add tool permission tests
```

Evitar:

```text
update
changes
fix stuff
final
final2
new
teste
```

---

# 37. Nunca Fazer Push Forçado

Nunca executar automaticamente:

```bash
git push --force
```

ou:

```bash
git push --force-with-lease
```

sem autorização explícita do usuário.

---

# 38. Git Reset

Operações destrutivas como:

```bash
git reset --hard
```

exigem confirmação explícita.

Nunca assumir que:

> "o usuário provavelmente quer voltar."

---

# 39. Agentes de IA

Agentes de IA devem seguir todas as mesmas regras dos humanos.

A IA não possui privilégios especiais.

Ser uma IA não significa:

```text
pode apagar
pode executar
pode instalar
pode modificar
pode ignorar
```

---

# 40. IA Deve Inspecionar Antes de Alterar

Antes de editar:

```text
read
search
understand
plan
edit
test
review
```

Nunca gerar código baseado apenas no nome de um arquivo.

---

# 41. IA Não Deve Inventar APIs

Se uma API, função, struct ou módulo não foi encontrado:

```text
não inventar.
```

Primeiro:

```text
search
inspect
verify
```

Se realmente não existir, criar apenas se fizer parte da tarefa.

---

# 42. IA Não Deve Assumir Dependências

Antes de escrever:

```rust
use some_crate::Something;
```

verificar se:

```text
Cargo.toml
workspace
lockfile
```

contêm essa dependência.

---

# 43. IA Deve Declarar Incerteza

Quando não tiver certeza:

```text
não assumir silenciosamente.
```

Deve indicar:

```text
"Não encontrei uma implementação existente para isso."
```

ou:

```text
"Esta alteração exige uma decisão arquitetural."
```

---

# 44. IA Deve Respeitar o Contexto

A IA deve carregar apenas o contexto necessário.

Prioridade:

```text
1. arquivo mencionado pelo usuário
2. arquivos diretamente relacionados
3. APIs utilizadas
4. testes relacionados
5. configuração relevante
6. documentação
7. histórico necessário
```

Não indexar o projeto inteiro sem necessidade.

---

# 45. IA Não Deve Alterar Arquivos Aleatórios

Uma solicitação como:

> "corrige este bug"

não autoriza automaticamente alterações em dezenas de arquivos.

A IA deve procurar a menor mudança capaz de resolver o problema.

---

# 46. Humanos Também Devem Seguir as Regras

Estas regras não são:

> "regras para a IA."

São regras para **qualquer pessoa que modifique o Darb**.

Isso inclui:

```text
Founder
Developer
Contributor
AI Agent
Maintainer
Plugin Developer
Automation
```

---

# 47. Regra do Menor Impacto

Quando existirem duas soluções equivalentes:

### Solução A

```text
10 arquivos
500 linhas
3 dependências
```

### Solução B

```text
2 arquivos
80 linhas
0 dependências
```

Preferir B, desde que mantenha qualidade e arquitetura.

---

# 48. Compatibilidade

Uma alteração não deve quebrar APIs existentes sem necessidade.

Antes de alterar uma API pública:

```text
search usages
check callers
check tests
check docs
check plugins
```

Se uma breaking change for necessária, documentá-la.

---

# 49. Configuração

Não adicionar configurações globais sem necessidade.

Preferir:

```text
defaults seguros
```

e configuração explícita quando necessária.

Exemplo:

```toml
[interface]
language = "pt"

[agent]
max_iterations = 20

[context]
max_tokens = 12000
```

---

# 50. Performance

Não otimizar prematuramente.

Primeiro:

```text
correctness
```

Depois:

```text
measurement
```

Depois:

```text
optimization
```

Nunca assumir que algo é lento sem medir.

---

# 51. Memória

O uso de memória deve ser considerado em:

- indexação;
- contexto;
- histórico;
- cache;
- TUI;
- providers;
- execução de ferramentas.

Evitar carregar projetos inteiros na RAM quando apenas alguns arquivos são necessários.

---

# 52. Concorrência

Tokio deve ser utilizado de forma consciente.

Não criar:

```text
spawn()
```

para tudo.

Toda tarefa assíncrona deve possuir uma razão clara.

Evitar:

```text
background loops
polling constante
tasks infinitas
```

---

# 53. Logs

Logs devem ajudar a diagnosticar problemas sem poluir a experiência normal.

Nunca registrar:

```text
password
API key
token
private key
secret
```

Logs de desenvolvimento podem ser mais detalhados.

Modo normal deve permanecer limpo.

---

# 54. Documentação

Alterações arquiteturais devem atualizar documentação.

Se alterar:

```text
architecture
provider
agent
tool
permission
configuration
CLI
```

verificar se algum:

```text
docs/*.md
README.md
```

precisa ser atualizado.

---

# 55. Novos Recursos

Para adicionar um recurso:

```text
1. Definir objetivo
2. Definir escopo
3. Identificar módulo responsável
4. Verificar arquitetura existente
5. Planejar implementação
6. Implementar
7. Testar
8. Revisar
9. Documentar
```

---

# 56. Checklist de Pull Request

Antes de aceitar uma alteração:

```text
## Código

[ ] segue arquitetura
[ ] não possui duplicação desnecessária
[ ] não possui código morto
[ ] não possui hacks
[ ] erros tratados corretamente
[ ] nomes claros

## Segurança

[ ] permissões respeitadas
[ ] nenhum segredo
[ ] nenhuma operação perigosa silenciosa
[ ] nenhum bypass

## Performance

[ ] nenhuma indexação desnecessária
[ ] nenhum loop permanente desnecessário
[ ] consumo de memória aceitável
[ ] nenhuma dependência pesada sem justificativa

## Testes

[ ] cargo check
[ ] cargo test
[ ] cargo clippy
[ ] testes específicos da alteração

## Documentação

[ ] docs atualizados quando necessário
[ ] configuração documentada quando necessário

## Git

[ ] diff revisado
[ ] commits claros
[ ] arquivos corretos
[ ] nenhum arquivo gerado acidentalmente
```

---

# 57. Quando Parar e Perguntar

O contribuidor deve parar e pedir orientação quando:

- existir ambiguidade arquitetural;
- uma operação for destrutiva;
- houver risco de perda de dados;
- houver necessidade de quebrar uma API;
- houver necessidade de alterar regras de segurança;
- houver necessidade de adicionar uma grande dependência;
- a solução exigir mudança em vários módulos sem escopo definido;
- houver conflito entre documentação e código;
- houver dúvida sobre intenção do usuário.

---

# 58. Quando Pode Agir Autonomamente

Pode agir sem confirmação quando:

- a tarefa estiver clara;
- o escopo estiver definido;
- a alteração for reversível;
- não houver risco significativo;
- as permissões permitirem;
- a arquitetura estiver clara;
- o contexto for suficiente.

Exemplo:

```text
corrigir erro de compilação localizado
```

pode ser executado autonomamente.

Exemplo:

```text
apagar todos os arquivos temporários do projeto
```

deve exigir confirmação quando houver risco.

---

# 59. Regra de Transparência

Após uma alteração significativa, o contribuidor deve conseguir explicar:

```text
O que foi alterado?
Por quê?
Onde?
Quais arquivos?
Quais testes?
Quais riscos?
```

Para agentes de IA, a resposta final deve ser objetiva.

Formato recomendado:

```text
Resumo:
...

Arquivos alterados:
- ...
- ...

Testes:
- cargo check
- cargo test

Avisos:
- ...

Próximos passos:
- ...
```

---

# 60. Regra de Reversibilidade

Sempre que possível, alterações devem ser fáceis de desfazer.

Preferir:

```text
diff pequeno
commit isolado
mudança localizada
```

Evitar alterações irreversíveis sem necessidade.

---

# 61. Regra Contra Hacks

É proibido introduzir soluções como:

```text
sleep(5)
retry forever
hardcoded path
magic number
silent fallback
ignore error
disable check
```

apenas para fazer algo "funcionar".

Se existir um workaround inevitável, ele deve:

1. ser documentado;
2. possuir motivo;
3. possuir escopo limitado;
4. possuir plano para remoção quando possível.

---

# 62. Código Experimental

Experimentos devem ficar claramente separados do código estável.

Preferir:

```text
experiments/
```

ou branches específicas.

Não misturar protótipos com o núcleo do produto sem validação.

---

# 63. Regra de Simplicidade

Quando duas implementações resolvem o mesmo problema:

> **Escolha a mais simples que mantenha segurança, arquitetura e qualidade.**

O Darb não deve se tornar complexo apenas porque pode.

---

# 64. Regra de Ouro

> **Faça a menor alteração correta, segura, testável e compreensível capaz de resolver o problema.**

---

# 65. Contrato para Agentes de IA

Todo agente que operar no repositório deve considerar:

```text
Eu não sou o dono do código.

Eu sou um operador do código.

Eu devo entender antes de modificar.

Eu não devo inventar APIs.

Eu não devo ignorar erros.

Eu não devo ignorar permissões.

Eu não devo executar operações destrutivas silenciosamente.

Eu devo preservar a arquitetura.

Eu devo testar minhas alterações.

Eu devo deixar o projeto melhor ou, no mínimo, não pior.
```

---

# 66. Contrato para Humanos

Todo desenvolvedor que trabalhar no Darb deve considerar:

```text
Eu não devo alterar arquitetura sem necessidade.

Eu não devo introduzir complexidade gratuita.

Eu devo revisar minhas alterações.

Eu devo respeitar as regras de segurança.

Eu devo escrever código que outra pessoa consiga manter.

Eu devo tratar agentes de IA como colaboradores,
não como substitutos da revisão humana.
```

---

# 67. Princípio Final

O Darb deve permitir alta autonomia sem perder controle humano.

```text
AUTONOMIA
    +
SEGURANÇA
    +
TRANSPARÊNCIA
    +
SIMPLICIDADE
    +
QUALIDADE
    =
DARB
```

### Regra definitiva

> **Qualquer humano ou IA pode modificar o Darb, mas ninguém pode modificar o Darb ignorando suas regras.**