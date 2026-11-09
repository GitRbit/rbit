//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/merge.rs`.

use crate::catalog::collab::merge::MergeMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    MergeMessage<'a> => MessageWriter |text| {
        MergeMessage::Merge => text.literal("Mesclagem"),
        MergeMessage::ConflictingFiles => text.literal("Arquivos em conflito"),
        MergeMessage::WorkInProgress => text.literal("Este pull request ainda é um trabalho em andamento."),
        MergeMessage::ReadyForReview => text.literal("Pronto para revisão"),
        MergeMessage::MergeMethod => text.literal("Método de mesclagem"),
        MergeMessage::CreateMergeCommit => text.literal("Criar um commit de mesclagem"),
        MergeMessage::SquashAndMerge => text.literal("Fazer squash e mesclar"),
        MergeMessage::RebaseAndMerge => text.literal("Fazer rebase e mesclar"),
        MergeMessage::EditMessage => text.literal("Editar a mensagem do commit"),
        MergeMessage::Message => text.literal("Mensagem"),
        MergeMessage::TitlePlaceholder { title, squash } => {
            text.argument(title);
            text.literal(" — ou, ao fazer squash, ");
            text.argument(squash);
        }
        MergeMessage::DeleteHead => text.literal("Excluir o branch de origem após mesclar"),
        MergeMessage::Bypass => {
            text.literal("Mesclar sem cumprir os requisitos de proteção do branch (isto é auditado)");
        }
        MergeMessage::MergePull => text.literal("Mesclar pull request"),
        MergeMessage::WriteAccessOnly => {
            text.literal("Somente pessoas com acesso de escrita a este repositório podem mesclar.");
        }
        MergeMessage::PullOpen => text.literal("O pull request está aberto"),
        MergeMessage::PullClosed => text.literal("O pull request está fechado"),
        MergeMessage::StillDraft => text.literal("Este pull request ainda é um rascunho"),
        MergeMessage::HeadExists => text.literal("O branch de origem existe"),
        MergeMessage::HeadDeleted => text.literal("O branch de origem foi excluído"),
        MergeMessage::NoConflicts => text.literal("Nenhum conflito com o branch base"),
        MergeMessage::Conflicts => text.literal("Este branch tem conflitos que precisam ser resolvidos"),
        MergeMessage::CheckingConflicts => text.literal("Verificando conflitos…"),
        MergeMessage::MergeabilityError => text.literal("Não foi possível verificar se a mesclagem é possível"),
        MergeMessage::Approvals { have, need } => {
            text.number(have);
            text.literal(" de ");
            text.number(need);
            text.literal(match text.plural(need) {
                Plural::One => " aprovação obrigatória",
                _ => " aprovações obrigatórias",
            });
        }
        MergeMessage::NoChangesRequested => text.literal("Nenhuma alteração solicitada"),
        MergeMessage::ChangesRequestedBy { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " revisor solicitou alterações",
                _ => " revisores solicitaram alterações",
            });
        }
        MergeMessage::MayPush => text.literal("Você pode fazer push para o branch base"),
        MergeMessage::MayNotPush => text.literal("Você não tem permissão para fazer push para o branch base"),
        MergeMessage::NoChecks => text.literal("Nenhuma verificação de status"),
        MergeMessage::ChecksPending => text.literal("Algumas verificações ainda não terminaram"),
        MergeMessage::ChecksPassed => text.literal("Todas as verificações foram aprovadas"),
        MergeMessage::ChecksFailed => text.literal("Algumas verificações não foram bem-sucedidas"),
        MergeMessage::RequiredPassed { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " verificação obrigatória aprovada",
                _ => " verificações obrigatórias aprovadas",
            });
        }
        MergeMessage::RequiredFailed { failing, required } => {
            text.number(failing);
            text.literal(" de ");
            text.number(required);
            text.literal(" verificações obrigatórias falharam");
        }
        MergeMessage::RequiredWaiting { waiting, required } => {
            text.literal("Aguardando ");
            text.number(waiting);
            text.literal(" de ");
            text.number(required);
            text.literal(" verificações obrigatórias");
        }
    }
}
