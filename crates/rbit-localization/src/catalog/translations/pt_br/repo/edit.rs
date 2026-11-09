//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/edit.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::edit::{RepoEditLabel, RepoEditMessage};
use crate::plural::Plural;

entries! { PtBr:
    RepoEditLabel => MessageWriter |text| {
        RepoEditLabel::NewFile => text.literal("Novo arquivo"),
        RepoEditLabel::Edit => text.literal("Editar"),
        RepoEditLabel::On => text.literal("Em"),
        RepoEditLabel::NewFileHint => text.literal("Digite um caminho com / para criar diretórios."),
        RepoEditLabel::EditHint => text.literal("Altere o caminho para renomear ou mover o arquivo."),
        RepoEditLabel::BackToFile => text.literal("Voltar para o arquivo"),
        RepoEditLabel::Path => text.literal("Caminho"),
        RepoEditLabel::Content => text.literal("Conteúdo"),
        RepoEditLabel::CommitChanges => text.literal("Fazer commit das alterações"),
        RepoEditLabel::Cancel => text.literal("Cancelar"),
        RepoEditLabel::Delete => text.literal("Excluir"),
        RepoEditLabel::DeleteHint => {
            text.literal("O arquivo continua no histórico; um diretório que ficar vazio desaparece.");
        }
        RepoEditLabel::DeleteFile => text.literal("Excluir arquivo"),
        RepoEditLabel::UploadFiles => text.literal("Enviar arquivos"),
        RepoEditLabel::Into => text.literal("Para"),
        RepoEditLabel::OnBranch => text.literal("em"),
        RepoEditLabel::UploadHint => text.literal("Arquivos com o mesmo nome são substituídos."),
        RepoEditLabel::Files => text.literal("Arquivos"),
        RepoEditLabel::NothingCommitted => text.literal("Nenhum commit foi feito."),
        RepoEditLabel::ReviewNewCommits => text.literal("Revise os novos commits"),
        RepoEditLabel::KeptAndCommittedOnTop => {
            text.literal(
                "depois faça commit novamente: suas alterações foram mantidas abaixo e entrarão por cima da nova versão.",
            );
        }
        RepoEditLabel::BranchGone => {
            text.literal(
                "Nenhum commit foi feito. Faça commit em um novo branch; suas alterações foram mantidas abaixo.",
            );
        }
        RepoEditLabel::CommitChangesLegend => text.literal("Fazer commit das alterações"),
        RepoEditLabel::CommitMessage => text.literal("Mensagem do commit"),
        RepoEditLabel::ExtendedDescription => text.literal("Descrição detalhada"),
        RepoEditLabel::Optional => text.literal("opcional"),
        RepoEditLabel::CommitDirectlyTo => text.literal("Fazer commit diretamente em"),
        RepoEditLabel::NotAllowed => text.literal("Não permitido:"),
        RepoEditLabel::NewBranchAndPull => text.literal("Criar um novo branch e abrir um pull request"),
        RepoEditLabel::NewBranchName => text.literal("Nome do novo branch"),
    }
    RepoEditMessage<'a> => MessageWriter |text| {
        RepoEditMessage::AtMost { size } => {
            text.literal("No máximo ");
            text.argument(size);
        }
        RepoEditMessage::UploadLimits { files, size } => {
            text.literal("Até ");
            text.number(files as u64);
            text.literal(match text.plural(files as u64) {
                Plural::One => " arquivo de no máximo ",
                _ => " arquivos de no máximo ",
            });
            text.argument(size);
            text.literal(" cada, em um único commit.");
        }
        RepoEditMessage::BranchChanged { branch } => {
            text.argument(branch);
            text.literal(" mudou desde que você começou");
        }
        RepoEditMessage::BranchMissing { branch } => {
            text.argument(branch);
            text.literal(" não existe mais");
        }
        RepoEditMessage::NowPointsAt => text.literal("Agora ele aponta para"),
        RepoEditMessage::NotEditable { limit } => {
            text.literal("Este arquivo é binário ou maior que ");
            text.argument(limit);
            text.literal(
                ", então não pode ser editado no navegador. Você pode excluí-lo ou enviar um substituto.",
            );
        }
        RepoEditMessage::EditingTitle { path } => {
            text.literal("Editando ");
            text.argument(path);
        }
        RepoEditMessage::DeleteTitle { path } => {
            text.literal("Excluir ");
            text.argument(path);
        }
    }
}
