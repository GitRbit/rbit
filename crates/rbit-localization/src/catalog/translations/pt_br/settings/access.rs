//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/access.rs`.

use crate::catalog::collab::rich::RichWriter;
use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::access::AccessSentence;
use crate::catalog::settings::access::{AccessMessage, AccessText};

entries! { PtBr:
    AccessMessage => MessageWriter |text| {
        AccessMessage::Settings => text.literal("Configurações"),
        AccessMessage::Collaborators => text.literal("Colaboradores"),
        AccessMessage::AddCollaborator => text.literal("Adicionar um colaborador"),
        AccessMessage::Username => text.literal("Nome de usuário"),
        AccessMessage::Permission => text.literal("Permissão"),
        AccessMessage::Add => text.literal("Adicionar"),
        AccessMessage::People => text.literal("Pessoas"),
        AccessMessage::NoCollaborators => text.literal("Nenhum colaborador"),
        AccessMessage::NoCollaboratorsText => {
            text.literal("Adicione alguém pelo nome de usuário para dar acesso a este repositório.");
        }
        AccessMessage::You => text.literal("(você)"),
        AccessMessage::Save => text.literal("Salvar"),
        AccessMessage::Leave => text.literal("Sair"),
        AccessMessage::Remove => text.literal("Remover"),
        AccessMessage::Teams => text.literal("Equipes"),
        AccessMessage::ManageTeams => text.literal("Gerenciar equipes"),
        AccessMessage::NoTeams => text.literal("Nenhuma equipe"),
        AccessMessage::NoTeamsText => {
            text.literal(
                "Conceda acesso a uma equipe na página da equipe; os membros dela passam a ter essa permissão aqui.",
            );
        }
        AccessMessage::Transfer => text.literal("Transferir propriedade"),
        AccessMessage::TransferLead => text.literal("Mover"),
        AccessMessage::TransferTail => {
            text.literal(
                "para outra conta. O histórico, os colaboradores e as estrelas vão junto; links e URLs de clone que \
                 usam o endereço antigo deixam de funcionar, e as equipes da organização atual perdem o acesso.",
            );
        }
        AccessMessage::NewOwner => text.literal("Novo proprietário"),
        AccessMessage::AdminEntry => {
            text.literal("Como administrador do site, você pode digitar qualquer usuário ou organização.");
        }
        AccessMessage::Organization => text.literal("Organização"),
        AccessMessage::OwnerHint => text.literal("Sua conta ou uma organização da qual você faz parte."),
        AccessMessage::TypeLead => text.literal("Digite"),
        AccessMessage::TypeTail => text.literal("para confirmar"),
        AccessMessage::TransferRepository => text.literal("Transferir repositório"),
        AccessMessage::Nowhere => text.literal("Não há para onde transferir"),
        AccessMessage::NowhereText => {
            text.literal(
                "Você não faz parte de nenhuma organização em que possa criar repositórios, e este repositório já é \
                 seu.",
            );
        }
    }
    AccessText<'a> => MessageWriter |text| {
        AccessText::PermissionFor(name) => {
            text.literal("Permissão de ");
            text.argument(name);
        }
        AccessText::SavePermissionFor(name) => {
            text.literal("Salvar a permissão de ");
            text.argument(name);
        }
        AccessText::RemoveLabel(name) => {
            text.literal("Remover ");
            text.argument(name);
        }
        AccessText::Added(date) => {
            text.literal("Adicionado em ");
            text.argument(date);
        }
        AccessText::CollaboratorsTitle(repository) => {
            text.literal("Colaboradores · ");
            text.argument(repository);
        }
        AccessText::TransferTitle(repository) => {
            text.literal("Transferir · ");
            text.argument(repository);
        }
    }
    AccessSentence => RichWriter |text| {
        AccessSentence::CollaboratorsIntro { organization } => {
            text.literal("As pessoas adicionadas aqui recebem acesso a ");
            text.slot(0);
            text.literal(" diretamente");
            if organization {
                text.literal(", pertençam ou não à organização");
            }
            text.literal(".");
        }
    }
}
