//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/org_teams.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::org_teams::{TeamMessage, TeamText};
use crate::plural::Plural;

entries! { PtBr:
    TeamMessage => MessageWriter |text| {
        TeamMessage::NewTeam => text.literal("Nova equipe"),
        TeamMessage::NoTeams => text.literal("Ainda não há equipes"),
        TeamMessage::NoTeamsText => {
            text.literal("As equipes agrupam membros e dão a cada grupo o próprio acesso aos repositórios.");
        }
        TeamMessage::OneMember => text.literal("1 membro"),
        TeamMessage::OneRepository => text.literal("1 repositório"),
        TeamMessage::NameHintLead => text.literal("Letras, dígitos,"),
        TeamMessage::NameHintAnd => text.literal("e"),
        TeamMessage::NameHintTail => text.literal("; usado no endereço da equipe."),
        TeamMessage::CreateTeam => text.literal("Criar equipe"),
        TeamMessage::TeamSettings => text.literal("Configurações da equipe"),
        TeamMessage::RenameHint => text.literal("Renomear altera o endereço da equipe."),
        TeamMessage::SaveChanges => text.literal("Salvar alterações"),
        TeamMessage::DeleteTeam => text.literal("Excluir esta equipe"),
        TeamMessage::DeleteTeamText => {
            text.literal("Os membros continuam na organização, mas perdem o acesso que esta equipe lhes dava.");
        }
        TeamMessage::DeleteTeamButton => text.literal("Excluir equipe"),
        TeamMessage::UsernamePlaceholder => text.literal("Nome de usuário"),
        TeamMessage::OnlyMembers => {
            text.literal("Somente membros da organização podem entrar nas equipes dela.");
        }
        TeamMessage::NoMembers => text.literal("Nenhum membro"),
        TeamMessage::NoMembersText => {
            text.literal("Os membros recebem o acesso desta equipe aos repositórios dela.");
        }
        TeamMessage::AddRepository => text.literal("Adicionar um repositório"),
        TeamMessage::RepositoryPlaceholder => text.literal("Nome do repositório"),
        TeamMessage::Permission => text.literal("Permissão"),
        TeamMessage::RepositoryHint => {
            text.literal(
                "Qualquer repositório da organização. Os membros da equipe recebem esta permissão nele.",
            );
        }
        TeamMessage::NoRepositories => text.literal("Nenhum repositório"),
        TeamMessage::NoRepositoriesText => {
            text.literal("Esta equipe não recebeu acesso a nenhum repositório que você possa ver.");
        }
        TeamMessage::Private => text.literal("Privado"),
        TeamMessage::Save => text.literal("Salvar"),
    }
    TeamText<'a> => MessageWriter |text| {
        TeamText::Members(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " membro",
                _ => " membros",
            });
        }
        TeamText::Repositories(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " repositório",
                _ => " repositórios",
            });
        }
        TeamText::PermissionOn(repository) => {
            text.literal("Permissão em ");
            text.argument(repository);
        }
        TeamText::SavePermissionOn(repository) => {
            text.literal("Salvar a permissão em ");
            text.argument(repository);
        }
        TeamText::TeamsTitle(org) => {
            text.literal("Equipes · ");
            text.argument(org);
        }
        TeamText::NewTeamTitle(org) => {
            text.literal("Nova equipe · ");
            text.argument(org);
        }
        TeamText::SettingsTitle(team) => {
            text.literal("Configurações · ");
            text.argument(team);
        }
    }
}
