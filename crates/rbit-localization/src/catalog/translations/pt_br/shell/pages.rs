//! Brazilian Portuguese (`pt-BR`) entries for `catalog/shell/pages.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::pages::{Failure, PageMessage};
use crate::plural::Plural;

entries! { PtBr:
    PageMessage<'a> => MessageWriter |text| {
        PageMessage::ErrorTitle(failure) => text.literal(match failure {
            Failure::NotFound => "Página não encontrada",
            Failure::Forbidden => "Acesso negado",
            Failure::BadRequest => "Solicitação inválida",
            Failure::SlowDown => "Vá com calma",
            Failure::Unavailable => "Temporariamente indisponível",
            Failure::Internal => "Algo deu errado",
        }),
        PageMessage::NotFoundText => {
            text.literal("A página que você procurava não existe ou você não tem acesso a ela.");
        }
        PageMessage::UnavailableText => {
            text.literal("O serviço está temporariamente indisponível. Tente novamente em instantes.");
        }
        PageMessage::InternalText => text.literal("Ocorreu um erro inesperado. Ele foi registrado."),
        PageMessage::GoHome => text.literal("Ir para o início"),
        PageMessage::Explore => text.literal("Explorar"),
        PageMessage::Offline => text.literal("Você está offline"),
        PageMessage::OfflineText => {
            text.literal("Não foi possível acessar o servidor. Verifique sua conexão e tente novamente.");
        }
        PageMessage::Retry => text.literal("Tentar novamente"),
        PageMessage::Private => text.literal("Privado"),
        PageMessage::Archived => text.literal("Arquivado"),
        PageMessage::Stars(count) => {
            let count = count.unsigned_abs();
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " estrela",
                _ => " estrelas",
            });
        }
        PageMessage::Pagination => text.literal("Paginação"),
        PageMessage::Previous => text.literal("Anterior"),
        PageMessage::Next => text.literal("Próxima"),
        PageMessage::PageOf(page, pages) => {
            text.literal("Página ");
            text.number(u64::from(page));
            text.literal(" de ");
            text.number(u64::from(pages));
        }
        PageMessage::EditProfile => text.literal("Editar perfil"),
        PageMessage::Joined(date) => {
            text.literal("Entrou em ");
            text.argument(date);
        }
        PageMessage::Profile => text.literal("Perfil"),
        PageMessage::Repositories => text.literal("Repositórios"),
        PageMessage::StarsTab => text.literal("Estrelas"),
        PageMessage::NewRepository => text.literal("Novo repositório"),
        PageMessage::NoStars(name) => {
            text.argument(name);
            text.literal(" ainda não marcou nenhum repositório com estrela.");
        }
        PageMessage::NoPublicRepositories(name) => {
            text.argument(name);
            text.literal(" ainda não tem repositórios públicos.");
        }
    }
}
