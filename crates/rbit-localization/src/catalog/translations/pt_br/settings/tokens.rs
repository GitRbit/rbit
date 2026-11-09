//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/tokens.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::tokens::TokensSentence;
use crate::catalog::settings::tokens::{TokenText, TokensMessage};
use crate::plural::Plural;

entries! { PtBr:
    TokensMessage => MessageWriter |text| {
        TokensMessage::Title => text.literal("Tokens de acesso"),
        TokensMessage::CopyNow => {
            text.literal("Copie seu novo token agora. Você não poderá vê-lo novamente.");
        }
        TokensMessage::CopyToken => text.literal("Copiar token"),
        TokensMessage::Copy => text.literal("Copiar"),
        TokensMessage::NewToken => text.literal("Novo token"),
        TokensMessage::Name => text.literal("Nome"),
        TokensMessage::NamePlaceholder => text.literal("ex.: Notebook do trabalho"),
        TokensMessage::NameHint => text.literal("Algo para reconhecê-lo depois."),
        TokensMessage::Scope => text.literal("Escopo"),
        TokensMessage::ScopeHint => text.literal("Um token nunca pode fazer mais do que a sua conta."),
        TokensMessage::Expiration => text.literal("Expiração"),
        TokensMessage::Generate => text.literal("Gerar token"),
        TokensMessage::YourTokens => text.literal("Seus tokens"),
        TokensMessage::EmptyTitle => text.literal("Nenhum token de acesso"),
        TokensMessage::EmptyText => {
            text.literal("Gere um token para clonar, buscar e fazer push por HTTPS sem a sua senha.");
        }
        TokensMessage::Revoke => text.literal("Revogar"),
        TokensMessage::NoExpiration => text.literal("Sem expiração"),
        TokensMessage::OneDay => text.literal("1 dia"),
        TokensMessage::NeverUsed => text.literal("Nunca usado"),
        TokensMessage::ReadDescription => text.literal("Clonar e buscar; ler pela API."),
        TokensMessage::WriteDescription => {
            text.literal("Também fazer push, criar repositórios e marcar com estrela pela API.");
        }
        TokensMessage::AdminDescription => {
            text.literal(
                "Também alterar as configurações e os webhooks de repositórios e excluir repositórios pela API.",
            );
        }
    }
    TokenText<'a> => MessageWriter |text| {
        TokenText::Created(name) => {
            text.literal("Token “");
            text.argument(name);
            text.literal("” criado");
        }
        TokenText::RevokeLabel(name) => {
            text.literal("Revogar ");
            text.argument(name);
        }
        TokenText::Days(days) => {
            text.number(days);
            text.literal(match text.plural(days) {
                Plural::One => " dia",
                _ => " dias",
            });
        }
        TokenText::CreatedOn(date) => {
            text.literal("Criado em ");
            text.argument(date);
        }
        TokenText::LastUsed(when) => {
            text.literal("Último uso ");
            text.argument(when);
        }
        TokenText::Expires(date) => {
            text.literal("Expira em ");
            text.argument(date);
        }
        TokenText::Expired(date) => {
            text.literal("Expirou em ");
            text.argument(date);
        }
    }
    TokensSentence => RichWriter |text| {
        TokensSentence::Intro => {
            text.literal("Use um token em vez da sua senha para Git por HTTPS: digite seu nome de usuário ");
            text.slot(0);
            text.literal(" e cole o token quando o Git pedir uma senha. A API REST em ");
            text.slot(1);
            text.literal(" o recebe como ");
            text.slot(2);
            text.literal(". Os tokens começam com ");
            text.slot(3);
            text.literal("; tokens de versões anteriores começam com ");
            text.slot(4);
            text.literal(" e continuam funcionando.");
        }
    }
}
