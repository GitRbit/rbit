//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/nav.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::nav::RepoNavSentence;
use crate::catalog::repo::nav::{RepoNavLabel, RepoNavMessage};
use crate::plural::Plural;

entries! { PtBr:
    RepoNavLabel => MessageWriter |text| {
        RepoNavLabel::ArchivedNote => {
            text.literal(
                "Este repositório foi arquivado. Ele é somente leitura: você pode cloná-lo, fazer fork dele e \
                 marcá-lo com estrela, mas não alterá-lo.",
            );
        }
        RepoNavLabel::Private => text.literal("Privado"),
        RepoNavLabel::Public => text.literal("Público"),
        RepoNavLabel::Repository => text.literal("Repositório"),
        RepoNavLabel::Code => text.literal("Código"),
        RepoNavLabel::Issues => text.literal("Issues"),
        RepoNavLabel::PullRequests => text.literal("Pull requests"),
        RepoNavLabel::Commits => text.literal("Commits"),
        RepoNavLabel::Branches => text.literal("Branches"),
        RepoNavLabel::Tags => text.literal("Tags"),
        RepoNavLabel::Releases => text.literal("Releases"),
        RepoNavLabel::Activity => text.literal("Atividade"),
        RepoNavLabel::Actions => text.literal("Actions"),
        RepoNavLabel::Settings => text.literal("Configurações"),
        RepoNavLabel::ForkThisRepository => text.literal("Fazer fork deste repositório"),
        RepoNavLabel::Fork => text.literal("Fork"),
        RepoNavLabel::Forks => text.literal("Forks"),
        RepoNavLabel::Clone => text.literal("Clonar"),
        RepoNavLabel::CloneUrl => text.literal("URL de clone"),
        RepoNavLabel::CopyCloneUrl => text.literal("Copiar URL de clone"),
        RepoNavLabel::SshCloneUrl => text.literal("URL de clone SSH"),
        RepoNavLabel::CopySshCloneUrl => text.literal("Copiar URL de clone SSH"),
        RepoNavLabel::Copy => text.literal("Copiar"),
        RepoNavLabel::YourSettings => text.literal("suas configurações"),
        RepoNavLabel::YourSettingsEnd => text.literal("."),
        RepoNavLabel::DownloadZip => text.literal("Baixar ZIP"),
        RepoNavLabel::Star => text.literal("Estrela"),
        RepoNavLabel::Starred => text.literal("Com estrela"),
        RepoNavLabel::StarThisRepository => text.literal("Marcar este repositório com estrela"),
        RepoNavLabel::UnstarThisRepository => text.literal("Remover a estrela deste repositório"),
        RepoNavLabel::SignInToStar => text.literal("Entre para marcar este repositório com estrela"),
        RepoNavLabel::Stargazers => text.literal("Estrelas"),
    }
    RepoNavMessage => MessageWriter |text| {
        RepoNavMessage::ForkCount { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " fork",
                _ => " forks",
            });
        }
        RepoNavMessage::StargazerCount { count } => {
            text.number(count.unsigned_abs());
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => " estrela",
                _ => " estrelas",
            });
        }
    }
    RepoNavSentence => RichWriter |text| {
        RepoNavSentence::ForkedFrom => {
            text.literal("fork de ");
            text.slot(0);
        }
        RepoNavSentence::MirrorOf => {
            text.literal("espelho de ");
            text.slot(0);
        }
        RepoNavSentence::SshKeyNote => {
            text.literal("O SSH exige uma chave em ");
            text.slot(0);
            text.literal(".");
        }
    }
}
