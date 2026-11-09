//! Brazilian Portuguese (`pt-BR`) entries for `catalog/collab/thread.rs`.

use crate::catalog::collab::thread::ThreadMessage;
use crate::catalog::fallback::MessageWriter;

entries! { PtBr:
    ThreadMessage => MessageWriter |text| {
        ThreadMessage::Open => text.literal("Aberto"),
        ThreadMessage::Closed => text.literal("Fechado"),
        ThreadMessage::Merged => text.literal("Mesclado"),
        ThreadMessage::Draft => text.literal("Rascunho"),
        ThreadMessage::Title => text.literal("Título"),
        ThreadMessage::Description => text.literal("Descrição"),
        ThreadMessage::Edit => text.literal("Editar"),
        ThreadMessage::Save => text.literal("Salvar"),
        ThreadMessage::Cancel => text.literal("Cancelar"),
        ThreadMessage::Delete => text.literal("Excluir"),
        ThreadMessage::Optional => text.literal("opcional"),
        ThreadMessage::MarkdownSupported => text.literal("Markdown é suportado"),
        ThreadMessage::SignIn => text.literal("Entre"),
        ThreadMessage::LoadMore => text.literal("Carregar mais"),
        ThreadMessage::AddComment => text.literal("Adicionar um comentário"),
        ThreadMessage::CommentPlaceholder => {
            text.literal("Escreva um comentário. Use @ para usuários e # para issues ou pull requests.");
        }
        ThreadMessage::MentionSuggestions => text.literal("Sugestões de menção"),
        ThreadMessage::CommentHint => {
            text.literal(
                "Markdown é suportado. Use @nome, #123 ou proprietário/repositório#123 para criar um link para um \
                 usuário, issue ou pull request.",
            );
        }
        ThreadMessage::LockedHint => {
            text.literal("Esta conversa está bloqueada; somente colaboradores podem comentar.");
        }
        ThreadMessage::CloseIssue => text.literal("Fechar issue"),
        ThreadMessage::ClosePull => text.literal("Fechar pull request"),
        ThreadMessage::CloseNotPlanned => text.literal("Fechar como não planejada"),
        ThreadMessage::ReopenIssue => text.literal("Reabrir issue"),
        ThreadMessage::ReopenPull => text.literal("Reabrir pull request"),
        ThreadMessage::Comment => text.literal("Comentar"),
        ThreadMessage::LockConversation => text.literal("Bloquear conversa"),
        ThreadMessage::UnlockConversation => text.literal("Desbloquear conversa"),
        ThreadMessage::Bot => text.literal("bot"),
        ThreadMessage::Edited => text.literal("editado"),
        ThreadMessage::Author => text.literal("Autor"),
        ThreadMessage::Reply => text.literal("Responder"),
        ThreadMessage::DeleteCommentConfirm => text.literal("Excluir este comentário?"),
        ThreadMessage::DeleteComment => text.literal("Excluir comentário"),
        ThreadMessage::NoDescription => text.literal("Nenhuma descrição fornecida."),
    }
}
