//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};
use crate::plural::Plural;

entries! { PtBr:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("Releases"),
        ReleaseLabel::NewRelease => text.literal("Nova release"),
        ReleaseLabel::EditRelease => text.literal("Editar release"),
        ReleaseLabel::NoReleasesYet => text.literal("Ainda não há releases"),
        ReleaseLabel::EmptyForWriters => {
            text.literal(
                "Uma release empacota uma tag com notas e arquivos. Faça push de uma tag e depois crie uma release \
                 para ela.",
            );
        }
        ReleaseLabel::EmptyForReaders => {
            text.literal("As releases marcam versões deste projeto com notas e downloads.");
        }
        ReleaseLabel::Draft => text.literal("Rascunho"),
        ReleaseLabel::PreRelease => text.literal("Pré-release"),
        ReleaseLabel::Latest => text.literal("Mais recente"),
        ReleaseLabel::Bot => text.literal("bot"),
        ReleaseLabel::Created => text.literal("criou"),
        ReleaseLabel::Edit => text.literal("Editar"),
        ReleaseLabel::Downloads => text.literal("Downloads"),
        ReleaseLabel::SourceZip => text.literal("Código-fonte (zip)"),
        ReleaseLabel::SourceTarGz => text.literal("Código-fonte (tar.gz)"),
        ReleaseLabel::Files => text.literal("Arquivos"),
        ReleaseLabel::Remove => text.literal("Remover"),
        ReleaseLabel::FileToAttach => text.literal("Arquivo a anexar"),
        ReleaseLabel::Upload => text.literal("Enviar"),
        ReleaseLabel::DeleteTitle => text.literal("Excluir esta release"),
        ReleaseLabel::DeleteHintStart => {
            text.literal("A release, as notas e os arquivos dela serão removidos. A tag");
        }
        ReleaseLabel::DeleteHintEnd => text.literal("continua."),
        ReleaseLabel::DeleteRelease => text.literal("Excluir release"),
        ReleaseLabel::FormLead => {
            text.literal("Uma release descreve uma tag: as notas dela e arquivos para baixar.");
        }
        ReleaseLabel::Tag => text.literal("Tag"),
        ReleaseLabel::AnExisting => text.literal("Uma "),
        ReleaseLabel::TagLink => text.literal("tag existente"),
        ReleaseLabel::OrNewTag => text.literal(" ou uma nova criada a partir do branch abaixo."),
        ReleaseLabel::CreateTagFrom => text.literal("Criar uma nova tag a partir de"),
        ReleaseLabel::TargetHint => text.literal("usado apenas quando a tag não existe"),
        ReleaseLabel::OneOfTags => text.literal("Uma das "),
        ReleaseLabel::TagsEnd => text.literal("."),
        ReleaseLabel::TagsLink => text.literal("tags do repositório"),
        ReleaseLabel::Title => text.literal("Título"),
        ReleaseLabel::TitleHint => text.literal("opcional; por padrão, o nome da tag"),
        ReleaseLabel::Notes => text.literal("Notas"),
        ReleaseLabel::IsPreRelease => text.literal("Esta é uma pré-release"),
        ReleaseLabel::PreReleaseHint => {
            text.literal(
                "Marcada como não pronta para produção; uma release completa tem preferência como a mais recente.",
            );
        }
        ReleaseLabel::SaveRelease => text.literal("Salvar release"),
        ReleaseLabel::PublishRelease => text.literal("Publicar release"),
        ReleaseLabel::SaveDraft => text.literal("Salvar rascunho"),
        ReleaseLabel::Cancel => text.literal("Cancelar"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256 } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " download",
                _ => " downloads",
            });
        }
        ReleaseMessage::RemoveFile { name } => {
            text.literal("Remover ");
            text.argument(name);
        }
        ReleaseMessage::UploadLimit { size } => {
            text.literal("Até ");
            text.argument(size);
            text.literal(" por arquivo. Os downloads são sempre entregues como arquivos, nunca exibidos.");
        }
        ReleaseMessage::Uploaded { name } => {
            text.argument(name);
            text.literal(" foi enviado.");
        }
        ReleaseMessage::Removed { name } => {
            text.argument(name);
            text.literal(" foi removido.");
        }
        ReleaseMessage::EditTitle { tag } => {
            text.literal("Editar ");
            text.argument(tag);
        }
    }
}
