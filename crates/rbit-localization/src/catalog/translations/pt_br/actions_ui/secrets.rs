//! Brazilian Portuguese (`pt-BR`) entries for `catalog/actions_ui/secrets.rs`.

use crate::catalog::actions_ui::secrets::ActionsSecretsMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    ActionsSecretsMessage<'a> => MessageWriter |text| {
        ActionsSecretsMessage::SecretsTitle => text.literal("Segredos"),
        ActionsSecretsMessage::SecretsIntro { expression } => {
            text.literal("Os fluxos de trabalho os leem como ");
            text.argument(expression);
            text.literal(
                ". Os valores são criptografados e não podem ser exibidos novamente, apenas substituídos. Execuções \
                 vindas de forks e de autores sem acesso de escrita nunca os recebem; qualquer pessoa que possa fazer \
                 push pode lê-los por meio de um fluxo de trabalho.",
            );
        }
        ActionsSecretsMessage::SecretsUnavailable { problem } => {
            text.literal("Não é possível armazenar nem usar segredos: ");
            text.argument(problem);
        }
        ActionsSecretsMessage::NoSecrets => text.literal("Nenhum segredo"),
        ActionsSecretsMessage::NoSecretsHint => {
            text.literal("Adicione um segredo para os tokens e chaves de que seus fluxos de trabalho precisam.");
        }
        ActionsSecretsMessage::CannotDecrypt => text.literal("Não pode ser descriptografado — defina-o novamente"),
        ActionsSecretsMessage::Updated { when } => {
            text.literal("Atualizado ");
            text.argument(when);
        }
        ActionsSecretsMessage::AllRepositories => text.literal("Todos os repositórios"),
        ActionsSecretsMessage::SelectedRepositories { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositório selecionado",
                _ => " repositórios selecionados",
            });
        }
        ActionsSecretsMessage::DeleteSecret { name } => {
            text.literal("Excluir o segredo ");
            text.argument(name);
        }
        ActionsSecretsMessage::Delete => text.literal("Excluir"),
        ActionsSecretsMessage::ReplaceValue => text.literal("Substituir o valor"),
        ActionsSecretsMessage::NewValueOf { name } => {
            text.literal("Novo valor de ");
            text.argument(name);
        }
        ActionsSecretsMessage::ReplaceSecret => text.literal("Substituir segredo"),
        ActionsSecretsMessage::ChooseRepositories => text.literal("Escolher repositórios"),
        ActionsSecretsMessage::Recipients { name } => {
            text.literal("Os repositórios cujos fluxos de trabalho recebem ");
            text.argument(name);
            text.literal(", sem alterar o valor.");
        }
        ActionsSecretsMessage::SaveRepositories => text.literal("Salvar repositórios"),
        ActionsSecretsMessage::NewSecret => text.literal("Novo segredo"),
        ActionsSecretsMessage::Name => text.literal("Nome"),
        ActionsSecretsMessage::SecretNameHint => {
            text.literal(
                "Letras, dígitos e sublinhados, sem começar com dígito nem com GITHUB_, GITEA_, FORGEJO_ ou RBIT_. Os \
                 nomes são armazenados em maiúsculas; um nome existente é substituído.",
            );
        }
        ActionsSecretsMessage::Value => text.literal("Valor"),
        ActionsSecretsMessage::SecretValueHint { kib } => {
            text.literal("No máximo ");
            text.number(kib);
            text.literal(" KiB. Não pode ser exibido novamente.");
        }
        ActionsSecretsMessage::AddSecret => text.literal("Adicionar segredo"),
        ActionsSecretsMessage::VariablesTitle => text.literal("Variáveis"),
        ActionsSecretsMessage::VariablesIntro { expression } => {
            text.literal("Os fluxos de trabalho as leem como ");
            text.argument(expression);
            text.literal(
                ". Elas não são secretas: qualquer pessoa que possa alterar um fluxo de trabalho pode imprimi-las. \
                 Execuções vindas de forks e de autores sem acesso de escrita nunca as recebem.",
            );
        }
        ActionsSecretsMessage::NoVariables => text.literal("Nenhuma variável"),
        ActionsSecretsMessage::NoVariablesHint => {
            text.literal(
                "Adicione uma variável para configurações que seus fluxos de trabalho compartilham, como uma região ou \
                 uma versão.",
            );
        }
        ActionsSecretsMessage::DeleteVariable { name } => {
            text.literal("Excluir a variável ");
            text.argument(name);
        }
        ActionsSecretsMessage::Edit => text.literal("Editar"),
        ActionsSecretsMessage::ValueOf { name } => {
            text.literal("Valor de ");
            text.argument(name);
        }
        ActionsSecretsMessage::SaveVariable => text.literal("Salvar variável"),
        ActionsSecretsMessage::NewVariable => text.literal("Nova variável"),
        ActionsSecretsMessage::VariableNameHint => {
            text.literal("As mesmas regras dos nomes de segredos; um nome existente é substituído.");
        }
        ActionsSecretsMessage::VariableValueHint { kib } => {
            text.literal("No máximo ");
            text.number(kib);
            text.literal(" KiB.");
        }
        ActionsSecretsMessage::AddVariable => text.literal("Adicionar variável"),
    }
}
