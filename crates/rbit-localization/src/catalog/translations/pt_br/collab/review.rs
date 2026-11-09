//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/review.rs`.

use crate::catalog::collab::review::ReviewMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    ReviewMessage<'a> => MessageWriter |text| {
        ReviewMessage::Changes => text.literal("Alterações"),
        ReviewMessage::ToggleFileTree => text.literal("Mostrar ou ocultar a árvore de arquivos"),
        ReviewMessage::CollapseAll => text.literal("Recolher tudo"),
        ReviewMessage::ExpandAll => text.literal("Expandir tudo"),
        ReviewMessage::OutdatedComments { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comentário desatualizado está na conversa",
                _ => " comentários desatualizados estão na conversa",
            });
        }
        ReviewMessage::FinishReview => text.literal("Concluir sua revisão"),
        ReviewMessage::Truncated => {
            text.literal(
                "Esta alteração é muito grande, então alguns diffs não são mostrados. Clone o repositório para ver a \
                 alteração completa.",
            );
        }
        ReviewMessage::FileToggleHint => {
            text.literal("Clique para recolher ou expandir | Alt-clique para todos os arquivos");
        }
        ReviewMessage::ViewFile => text.literal("Ver arquivo"),
        ReviewMessage::CommentOnLine { line } => {
            text.literal("Comentar na linha ");
            text.number(line);
        }
        ReviewMessage::StartReview => text.literal("Iniciar uma revisão"),
        ReviewMessage::AddSingleComment => text.literal("Adicionar comentário único"),
        ReviewMessage::BinaryFile => text.literal("Arquivo binário não exibido."),
        ReviewMessage::TooLarge => text.literal("Diff grande demais para ser exibido."),
        ReviewMessage::SubmoduleCommit => text.literal("Commit de submódulo"),
        ReviewMessage::NoContentChanges => text.literal("Nenhuma alteração de conteúdo."),
        ReviewMessage::NoChangesTitle => text.literal("Nenhuma alteração"),
        ReviewMessage::NoChangesText => text.literal("A origem não acrescenta nada à base."),
        ReviewMessage::Added => text.literal("Adicionado"),
        ReviewMessage::Deleted => text.literal("Excluído"),
        ReviewMessage::Modified => text.literal("Modificado"),
        ReviewMessage::Renamed => text.literal("Renomeado"),
        ReviewMessage::Copied => text.literal("Copiado"),
        ReviewMessage::Outdated => text.literal("Desatualizado"),
        ReviewMessage::Pending => text.literal("Pendente"),
        ReviewMessage::ResolvedThread { path, line } => {
            text.literal("Conversa resolvida · ");
            text.argument(path);
            text.literal(" ");
            text.argument(line);
        }
        ReviewMessage::ReplyPlaceholder => text.literal("Responder…"),
        ReviewMessage::AddToReview => text.literal("Adicionar à revisão"),
        ReviewMessage::ReopenConversation => text.literal("Reabrir conversa"),
        ReviewMessage::ResolveConversation => text.literal("Resolver conversa"),
        ReviewMessage::ReviewChanges => text.literal("Revisar alterações"),
        ReviewMessage::PendingCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " pendente",
                _ => " pendentes",
            });
        }
        ReviewMessage::ReviewPlaceholder => text.literal("Deixe um comentário (Markdown é suportado)"),
        ReviewMessage::RequestChanges => text.literal("Solicitar alterações"),
        ReviewMessage::Approve => text.literal("Aprovar"),
        ReviewMessage::DiscardReview => text.literal("Descartar revisão pendente"),
        ReviewMessage::DismissReview => text.literal("Descartar revisão"),
        ReviewMessage::DismissReason => text.literal("Por que esta revisão está sendo descartada?"),
        ReviewMessage::Dismiss => text.literal("Descartar"),
        ReviewMessage::DismissedBy { by, message } => {
            text.literal("Descartada por ");
            text.argument(by);
            text.literal(": ");
            text.argument(message);
        }
        ReviewMessage::Reviewers => text.literal("Revisores"),
        ReviewMessage::NoReviews => text.literal("Ainda não há revisões"),
        ReviewMessage::RemoveRequestFor { name } => {
            text.literal("Remover a solicitação de revisão de ");
            text.argument(name);
        }
        ReviewMessage::RemoveRequest => text.literal("Remover solicitação"),
        ReviewMessage::RequestReviewFrom => text.literal("Solicitar uma revisão de"),
        ReviewMessage::Username => text.literal("nome de usuário"),
        ReviewMessage::RequestReview => text.literal("Solicitar revisão"),
        ReviewMessage::Approved => text.literal("Aprovado"),
        ReviewMessage::ChangesRequested => text.literal("Alterações solicitadas"),
        ReviewMessage::Dismissed => text.literal("Descartada"),
        ReviewMessage::Commented => text.literal("Comentou"),
        ReviewMessage::AwaitingReview => text.literal("Aguardando revisão"),
    }
}
