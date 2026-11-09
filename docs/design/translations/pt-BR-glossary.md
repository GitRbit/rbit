# Brazilian Portuguese (pt-BR) glossary

One term per concept for the `pt_br` tables under
`crates/rbit-localization/src/catalog/translations/pt_br/`. Terminology follows the pt-BR
interfaces of mainstream developer tools (GitHub, GitLab) where they agree; where they do
not, the choice below wins. See `docs/design/localization-contracts.md`, "Translating a
locale", for the file layout and typography rules.

## Style

- Address the reader as **você**. Buttons and menu items use the infinitive: “Salvar”,
  “Excluir”, “Criar repositório”.
- Sentence case for labels and headings (“Novo repositório”, not “Novo Repositório”).
- Quotation marks: “…”. Ellipsis: `…` (one character).
- Plurals: 0 and 1 take `Plural::One` (CLDR `pt`); every other count takes the `_` arm
  (`Many`, whole millions, shares the plural form).
- Machine values, commands, file names, keys, ref names and product names stay as they
  are: rbit, Git, GitHub, Git LFS, SSH, HTTPS, SHA-256, OpenID Connect, `git push`.
- English loanwords that Brazilian developers use as nouns keep their English spelling
  and take the gender given below: “o branch”, “o commit”, “o pull request”,
  “a issue”, “o fork”, “a tag”, “a release”, “o job”, “o webhook”, “o token”, “o log”.

## Terms

| English | pt-BR | Notes |
|---|---|---|
| repository | repositório | |
| branch | branch (m.) | “branch padrão”, “branch base” |
| default branch | branch padrão | |
| commit (noun) | commit (m.) | |
| commit (verb) | fazer commit | “Fazer commit das alterações” |
| pull request | pull request (m.) | |
| issue | issue (f.) | “Nova issue”, “issues abertas” |
| merge (verb) | mesclar | “Mesclar pull request” |
| merge (noun) | mesclagem | “commit de mesclagem”, “conflito de mesclagem” |
| merged | mesclado | |
| squash and merge | fazer squash e mesclar | |
| rebase and merge | fazer rebase e mesclar | |
| fork | fork (m.); fazer fork | |
| star | estrela; marcar com estrela | “Remover estrela” for unstar |
| watch | acompanhar | |
| clone | clonar | |
| push / pull (Git) | push / pull; fazer push | |
| tag | tag (f.) | |
| release | release (f.) | |
| asset (release) | arquivo da release | |
| label | etiqueta | |
| milestone | marco | |
| assignee | responsável | “Atribuir a” for assign |
| reviewer / review | revisor / revisão | |
| request changes | solicitar alterações | |
| approve | aprovar | |
| comment | comentário; comentar | |
| conversation, thread | conversa | |
| resolve (a conversation) | resolver | |
| draft | rascunho | |
| check, status check | verificação, verificação de status | |
| required check | verificação obrigatória | |
| diff | diff (m.) | |
| compare | comparar | |
| blame | blame (m.) | |
| file / directory / folder | arquivo / diretório / pasta | |
| path | caminho | |
| upload / download | enviar / baixar | |
| archive (repository) | arquivar; arquivado | |
| mirror | espelho; espelhar | “espelho de pull”, “espelho de push” |
| wiki | wiki (f.) | |
| page (wiki) | página | |
| workflow | fluxo de trabalho | |
| run (Actions) | execução | |
| job | job (m.) | kept in English to avoid clashing with “fluxo de trabalho” |
| step | etapa | |
| runner | executor | “executor auto-hospedado” |
| artifact | artefato | |
| secret | segredo | |
| variable | variável | |
| environment | ambiente | |
| deployment | implantação | |
| deploy key | chave de implantação | |
| re-run | executar novamente | |
| cancel | cancelar | |
| retry | tentar novamente | |
| settings | configurações | |
| account | conta | |
| profile | perfil | |
| appearance / theme | aparência / tema | |
| language | idioma | |
| organization | organização | |
| team | equipe | |
| member | membro | |
| owner | proprietário | |
| collaborator | colaborador | |
| contributor | contribuidor | |
| permission | permissão | read/write/admin: leitura/escrita/administração |
| visibility | visibilidade | public/private/internal: público/privado/interno |
| protection rule | regra de proteção | “proteção de branch” |
| webhook | webhook (m.) | |
| delivery (webhook) | entrega | |
| payload | payload (m.) | |
| access token | token de acesso | “token de acesso pessoal” |
| scope | escopo | |
| SSH key | chave SSH | |
| passkey | chave de acesso | |
| security key | chave de segurança | |
| two-factor authentication | autenticação em dois fatores | |
| recovery code | código de recuperação | |
| authenticator app | aplicativo autenticador | |
| session | sessão | |
| sign in / sign out | entrar / sair | “Entrar com GitHub” |
| sign up, registration | criar conta, cadastro | |
| username | nome de usuário | |
| password | senha | |
| email | e-mail | |
| notification | notificação | |
| inbox | caixa de entrada | |
| unsubscribe | cancelar inscrição | |
| dashboard | painel | |
| site administrator | administrador do site | |
| instance | instância | |
| audit log | log de auditoria | |
| log (output) | log (m.) | |
| suspend | suspender | |
| delete / remove | excluir / remover | delete = destroy; remove = take out of a list |
| edit / save | editar / salvar | |
| search / filter | pesquisar / filtrar | |
| explore | explorar | |
| open / closed (threads) | aberto / fechado | agree in gender: “issue aberta”, “pull request aberto” |
| reopen | reabrir | |
| self-hosted | auto-hospedado | |
| upstream | upstream | |
| cache | cache (m.) | |
