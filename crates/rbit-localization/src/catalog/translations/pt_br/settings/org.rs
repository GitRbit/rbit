//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/org.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org::{OrgMessage, OrgText};
use crate::plural::Plural;

entries! { PtBr:
    OrgMessage => MessageWriter |text| {
        OrgMessage::ChoosePermission => text.literal("Escolha uma permissão."),
        OrgMessage::Organization => text.literal("Organização"),
        OrgMessage::Repositories => text.literal("Repositórios"),
        OrgMessage::Members => text.literal("Membros"),
        OrgMessage::Teams => text.literal("Equipes"),
        OrgMessage::Settings => text.literal("Configurações"),
        OrgMessage::NewOrganization => text.literal("Nova organização"),
        OrgMessage::NewIntro => {
            text.literal(
                "Uma organização é proprietária de repositórios em nome de uma equipe. Você será o primeiro \
                 proprietário dela.",
            );
        }
        OrgMessage::Name => text.literal("Nome"),
        OrgMessage::NameHint => {
            text.literal(
                "Usado nas URLs. Letras, dígitos, “-”, “_” e “.”. Usuários e organizações compartilham o mesmo espaço \
                 de nomes.",
            );
        }
        OrgMessage::DisplayName => text.literal("Nome de exibição"),
        OrgMessage::Optional => text.literal("opcional"),
        OrgMessage::Description => text.literal("Descrição"),
        OrgMessage::Cancel => text.literal("Cancelar"),
        OrgMessage::CreateOrganization => text.literal("Criar organização"),
        OrgMessage::New => text.literal("Novo"),
        OrgMessage::NoRepositories => text.literal("Ainda não há repositórios"),
        OrgMessage::NoRepositoriesText => {
            text.literal("Os repositórios desta organização serão listados aqui.");
        }
        OrgMessage::Profile => text.literal("Perfil"),
        OrgMessage::Website => text.literal("Site"),
        OrgMessage::WebsitePlaceholder => text.literal("https://"),
        OrgMessage::SaveProfile => text.literal("Salvar perfil"),
        OrgMessage::MemberPermission => text.literal("Permissão dos membros"),
        OrgMessage::PermissionLegend => text.literal("O que todo membro recebe em todos os repositórios"),
        OrgMessage::PermissionHint => {
            text.literal(
                "Equipes e colaboradores podem acrescentar a isso, nunca retirar. Os proprietários sempre administram \
                 todos os repositórios.",
            );
        }
        OrgMessage::SavePermission => text.literal("Salvar permissão"),
        OrgMessage::NoPermission => text.literal("Nenhuma permissão"),
        OrgMessage::NoPermissionText => {
            text.literal("Os membros só veem os repositórios públicos e aqueles a que as equipes deles têm acesso.");
        }
        OrgMessage::Read => text.literal("Leitura"),
        OrgMessage::ReadText => text.literal("Os membros podem navegar e clonar todos os repositórios."),
        OrgMessage::Write => text.literal("Escrita"),
        OrgMessage::WriteText => {
            text.literal("Os membros podem navegar, clonar e fazer push em todos os repositórios.");
        }
        OrgMessage::DangerZone => text.literal("Zona de perigo"),
        OrgMessage::DeleteOrganization => text.literal("Excluir esta organização"),
        OrgMessage::DeleteText => {
            text.literal(
                "Isso remove permanentemente a organização e todas as associações de membros. Não pode ser desfeito.",
            );
        }
        OrgMessage::TypeToConfirmLead => text.literal("Digite"),
        OrgMessage::TypeToConfirmTail => text.literal("para confirmar"),
        OrgMessage::DeleteOrganizationButton => text.literal("Excluir organização"),
        OrgMessage::AddMember => text.literal("Adicionar um membro"),
        OrgMessage::Username => text.literal("Nome de usuário"),
        OrgMessage::Role => text.literal("Papel"),
        OrgMessage::Member => text.literal("Membro"),
        OrgMessage::Owner => text.literal("Proprietário"),
        OrgMessage::Add => text.literal("Adicionar"),
        OrgMessage::MembersHint => {
            text.literal(
                "Os membros recebem a permissão base da organização em todos os repositórios, além do que as equipes \
                 deles concedem. Os proprietários também podem gerenciar membros, equipes e configurações.",
            );
        }
        OrgMessage::You => text.literal("(você)"),
        OrgMessage::Leave => text.literal("Sair"),
        OrgMessage::Remove => text.literal("Remover"),
        OrgMessage::MakeOwner => text.literal("Tornar proprietário"),
        OrgMessage::MakeMember => text.literal("Tornar membro"),
    }
    OrgText<'a> => MessageWriter |text| {
        OrgText::StillOwnsOne(org) => {
            text.argument(org);
            text.literal(" ainda é proprietária de 1 repositório. Exclua-o ou transfira-o primeiro.");
        }
        OrgText::StillOwns(org, count) => {
            text.argument(org);
            text.literal(" ainda é proprietária de ");
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositório. Exclua-o ou transfira-o primeiro.",
                _ => " repositórios. Exclua-os ou transfira-os primeiro.",
            });
        }
        OrgText::RemoveLabel(name) => {
            text.literal("Remover ");
            text.argument(name);
        }
        OrgText::RoleLabel(action, name) => {
            text.argument(action);
            text.literal(": ");
            text.argument(name);
        }
        OrgText::MembersTitle(org) => {
            text.literal("Membros · ");
            text.argument(org);
        }
        OrgText::SettingsTitle(org) => {
            text.literal("Configurações · ");
            text.argument(org);
        }
    }
}
