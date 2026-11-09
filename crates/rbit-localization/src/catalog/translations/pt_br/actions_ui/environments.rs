//! Brazilian Portuguese (`pt-BR`) entries for `catalog/actions_ui/environments.rs`.

use crate::catalog::actions_ui::environments::ActionsEnvironmentMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    ActionsEnvironmentMessage<'a> => MessageWriter |text| {
        ActionsEnvironmentMessage::Title => text.literal("Ambientes"),
        ActionsEnvironmentMessage::Intro { key } => {
            text.literal("Um job que indica um ambiente com ");
            text.argument(key);
            text.literal(
                " recebe os segredos e variáveis dele, aguarda um dos revisores dele quando houver e só é executado a \
                 partir dos branches que ele permite. Um ambiente aparece aqui na primeira vez que um fluxo de trabalho \
                 o menciona.",
            );
        }
        ActionsEnvironmentMessage::NoEnvironments => text.literal("Nenhum ambiente"),
        ActionsEnvironmentMessage::NoEnvironmentsHint { key } => {
            text.literal("Adicione um abaixo ou execute um fluxo de trabalho cujo job indique um com ");
            text.argument(key);
            text.literal(".");
        }
        ActionsEnvironmentMessage::NewEnvironment => text.literal("Novo ambiente"),
        ActionsEnvironmentMessage::Name => text.literal("Nome"),
        ActionsEnvironmentMessage::NamePlaceholder => text.literal("production"),
        ActionsEnvironmentMessage::AddEnvironment => text.literal("Adicionar ambiente"),
        ActionsEnvironmentMessage::Reviewers { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " revisor",
                _ => " revisores",
            });
        }
        ActionsEnvironmentMessage::ProtectedBranches => text.literal("branches protegidos"),
        ActionsEnvironmentMessage::Branches { patterns } => {
            text.literal("branches ");
            text.argument(patterns);
        }
        ActionsEnvironmentMessage::NoProtection => text.literal("Sem proteção"),
        ActionsEnvironmentMessage::ProtectionRules => text.literal("Regras de proteção"),
        ActionsEnvironmentMessage::RequiredReviewers => text.literal("Revisores obrigatórios"),
        ActionsEnvironmentMessage::ReviewersPlaceholder => text.literal("um nome de usuário por linha"),
        ActionsEnvironmentMessage::ReviewersHint => {
            text.literal(
                "Até seis pessoas que podem ler este repositório. Um job neste ambiente aguarda até que uma delas o \
                 aprove; um job rejeitado falha. Sem revisores, os jobs são executados sem revisão.",
            );
        }
        ActionsEnvironmentMessage::PreventSelfReview => text.literal("Impedir autorrevisão"),
        ActionsEnvironmentMessage::PreventSelfReviewHint => {
            text.literal("Quem iniciou uma execução não pode aprovar as implantações dela.");
        }
        ActionsEnvironmentMessage::WaitTimer => text.literal("Tempo de espera"),
        ActionsEnvironmentMessage::WaitTimerHint => {
            text.literal(
                "Minutos que um job neste ambiente aguarda, depois de pronto e aprovado, antes de ser executado. 0 \
                 para nenhum; no máximo 43200 (30 dias).",
            );
        }
        ActionsEnvironmentMessage::DeploymentBranches => text.literal("Branches e tags de implantação"),
        ActionsEnvironmentMessage::AnyBranch => text.literal("Qualquer branch ou tag"),
        ActionsEnvironmentMessage::ProtectedOnly => text.literal("Somente branches protegidos"),
        ActionsEnvironmentMessage::ProtectedOnlyHint => {
            text.literal("Branches que têm uma regra de proteção de branch.");
        }
        ActionsEnvironmentMessage::SelectedBranches => text.literal("Branches e tags selecionados"),
        ActionsEnvironmentMessage::PatternsLabel => text.literal("Padrões de branches e tags"),
        ActionsEnvironmentMessage::PatternsPlaceholder => text.literal("main"),
        ActionsEnvironmentMessage::PatternsHint { branch, tags, tag, exclude } => {
            text.literal("Um padrão por linha, como nos filtros de fluxo de trabalho. Branches pelo nome (");
            text.argument(branch);
            text.literal("), tags com ");
            text.argument(tags);
            text.literal(" (");
            text.argument(tag);
            text.literal("); ");
            text.argument(exclude);
            text.literal(" exclui.");
        }
        ActionsEnvironmentMessage::SaveRules => text.literal("Salvar regras de proteção"),
        ActionsEnvironmentMessage::DeleteTitle => text.literal("Excluir este ambiente"),
        ActionsEnvironmentMessage::DeleteHint => {
            text.literal(
                "Os segredos, variáveis e regras dele também serão excluídos. Os jobs que aguardam os revisores dele \
                 serão executados imediatamente.",
            );
        }
        ActionsEnvironmentMessage::DeleteEnvironment => text.literal("Excluir ambiente"),
    }
}
