# Darb Shell — lógica de trabalho

> Como o projecto avança sem inflar. Este documento existe para proteger a
> finalização do projecto: cada regra aqui é uma coisa que **não** vamos
> fazer. Complementa `Docs/design.md` (o que a TUI mostra) e o documento
> de visão (para onde vamos).

Índice:

1. [A regra única](#1-a-regra-única)
2. [Gate de funcionalidades](#2-gate-de-funcionalidades)
3. [Ordem de trabalho](#3-ordem-de-trabalho)
4. [Tamanho de cada coisa](#4-tamanho-de-cada-coisa)
5. [Antes de escrever código](#5-antes-de-escrever-código)
6. [Fim de um passo](#6-fim-de-um-passo)
7. [Sinais de que estamos a fazer errado](#7-sinais-de-que-estamos-a-fazer-errado)

---

## 1. A regra única

> **Uma funcionalidade entra quando resolve um problema que já doeu.
> Não entra porque "faz sentido num IDE".**

O Darb nasceu de uma máquina que não conseguia correr as ferramentas
modernas. Essa é a barra: uma funcionalidade justifica-se se
**poupa tempo, memória ou atenção** no uso real.

A consequência prática: o Darb é aquilo que o criador usa **todos os
dias**, não aquilo que um produto de mercado pediria. Se uma ideia só
faz sentido para "quando o Darb for grande", ela espera.

## 2. Gate de funcionalidades

Nenhuma funcionalidade entra sem passar este gate, por escrito:

1. **Problema** — que dor concreta elimina? ("trocar 6 janelas por 1")
2. **Gatilho** — o que o faz ser accionado, e com que frequência?
3. **Peso** — quantos ficheiros, que dependências, quanta RAM?
4. **Alternativa** — porque não com um alias / script / atalho?
5. **Saída** — como se desliga se estiver a atrapalhar?

Se o passo 3 for grande e o passo 2 for raro, a resposta é não — mesmo
que a funcionalidade seja bonita. A excepção é o que já está no §2 do
`design.md`: **workspace e controlo da IA**, porque sustentam tudo o que
segue.

## 3. Ordem de trabalho

A sequência é deliberada. Não se salta etapas.

```text
1. Workspace        — o que está aberto, a correr, com que erros
2. Modos da IA      — Mentor / Assist / Autonomous sobre permissions
3. AI Router        — escolhe provider e modelo por tarefa
4. Desktop (Slint)  — mesmo Core, segundo renderer
5. eventual produto
```

O browser não está na lista, e não por esquecimento: foi retirado porque
compromete o princípio de leveza. Ver `design.md` §9.

Cada passo só começa quando o anterior **funciona e é usado**. "Funciona"
significa: foi aberto, usado de verdade, e ninguém se queixou.

## 4. Tamanho de cada coisa

Limites explícitos, para não crescer por omissão:

| Coisa | Limite |
|---|---|
| Um crate novo | só depois de 3 ou mais ficheiros com o mesmo nome |
| Um painel novo | só depois de o `App` ter um setter que o justifique |
| Um provider novo | só depois de o `Provider` trait mudar |
| Dependência nova | escreve primeiro o porquê no `Cargo.toml` |
| Ficheiro de 1 linha | é dívida — implementa ou apaga |

O crate `darb-plugins` foi exactamente isto: quatro ficheiros de uma
linha, zero código, e um `pub mod` em cada `lib.rs` a dar-lhe ar de
arquitectura. Foi apagado. A visão quer plugins; plugins que não
existem não custam nada.

## 5. Antes de escrever código

Perguntas, nesta ordem. Se a resposta a uma é "não", para aqui:

1. **Isto já existe?** recoverable, `rg`, `Ctrl+K`.
2. **É problema de UI ou de lógica?** Se é de lógica, não vai para
   `darb-tui` — a TUI não decide (§1 do `design.md`).
3. **Precisa de uma dependência nova?** Se sim, a pergunta 3 do gate.
4. **Cabe no perfil `eco`?** A máquina alvo tem 8 GB.
5. **Como se desliga?** Um painel sem caminho para sair é uma decisão
   permanente, e decisões permanentes custam caro.

## 6. Fim de um passo

Um passo está feito quando:

- compila sem warnings;
- os testes passam;
- a funcionalidade é usável **sem ler o código**;
- o que não foi feito está escrito, não é adivinhado.

E termina-se o trabalho com `cargo fmt`, `cargo clippy -- -D warnings` e
`cargo test`. Um passo a meio deixa o projecto num estado que ninguém
consegue usar — e isso é pior do que não começar.

## 7. Sinais de que estamos a fazer errado

Vale a pena parar e rever se aparecer algum destes:

- **A TUI está a fazer trabalho.** Se um painel lê o disco, chama um
  provider ou decide permissões, está a violar §1 do `design.md`.
- **Um crate tem ficheiros de uma linha.** É a forma como o peso entra
  sem se notar.
- **Falta um "porque" numa dependência.** Ou uma feature flag, ou lixo.
- **Estamos a construir para "quando o Darb for grande".** A §33 da
  visão: primeiro resolve o problema real de quem está a construir.
- **A lista de tabs cresceu sem painéis novos atrás.** Tabs são
  compromisso visual com o utilizador.
- **O `Cargo.lock` cresceu sem uma funcionalidade correspondente.**
  Regressão directa ao princípio de leveza.
