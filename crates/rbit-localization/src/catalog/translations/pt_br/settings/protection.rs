//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/protection.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::protection::{ProtectionMessage, ProtectionText, RuleBadge};
use crate::plural::Plural;

entries! { PtBr:
    ProtectionMessage => MessageWriter |text| {
        ProtectionMessage::TickToConfirm => text.literal("Marque a caixa para confirmar."),
        ProtectionMessage::Settings => text.literal("Configurações"),
        ProtectionMessage::Title => text.literal("Proteção de branches"),
        ProtectionMessage::IntroLead => text.literal("As regras se aplicam a todo push para"),
        ProtectionMessage::IntroTail => {
            text.literal(
                ", por HTTP e SSH, inclusive de administradores, a menos que uma regra permita que eles a ignorem.",
            );
        }
        ProtectionMessage::AddRule => text.literal("Adicionar regra"),
        ProtectionMessage::Deleted => text.literal("Regra excluída."),
        ProtectionMessage::Empty => text.literal("Nenhum branch protegido"),
        ProtectionMessage::EmptyText => {
            text.literal(
                "Adicione uma regra para impedir que um branch como main receba force-push, seja excluído ou alterado \
                 sem um pull request.",
            );
        }
        ProtectionMessage::NoMatchYet => text.literal("Ainda não corresponde a nenhum branch."),
        ProtectionMessage::MatchesLabel => text.literal("Corresponde a:"),
        ProtectionMessage::NewRule => text.literal("Nova regra de proteção de branch"),
        ProtectionMessage::NewRuleIntro => {
            text.literal("A regra se aplica a todos os branches a que o padrão corresponder, agora e no futuro.");
        }
        ProtectionMessage::Saved => text.literal("Regra salva."),
        ProtectionMessage::Rule => text.literal("Regra"),
        ProtectionMessage::Pattern => text.literal("Padrão de nome de branch"),
        ProtectionMessage::PatternPlaceholder => text.literal("main"),
        ProtectionMessage::PatternLead => text.literal("Um nome de branch ou um padrão:"),
        ProtectionMessage::PatternOne => text.literal("corresponde dentro de uma parte do nome,"),
        ProtectionMessage::PatternAcross => text.literal("entre partes ("),
        ProtectionMessage::PatternChar => text.literal("), "),
        ProtectionMessage::PatternTail => {
            text.literal("a um único caractere. Diferencia maiúsculas de minúsculas.");
        }
        ProtectionMessage::Protect => text.literal("Proteger os branches correspondentes"),
        ProtectionMessage::BlockForcePush => text.literal("Bloquear force-push"),
        ProtectionMessage::BlockForcePushText => {
            text.literal("Os pushes só podem adicionar commits; o histórico não pode ser reescrito.");
        }
        ProtectionMessage::BlockDeletion => text.literal("Bloquear exclusão"),
        ProtectionMessage::BlockDeletionText => text.literal("Os branches correspondentes não podem ser excluídos."),
        ProtectionMessage::RequirePullRequest => text.literal("Exigir um pull request"),
        ProtectionMessage::RequirePullRequestText => {
            text.literal(
                "As alterações só chegam pela mesclagem de um pull request; pushes diretos para um branch existente \
                 são recusados.",
            );
        }
        ProtectionMessage::RequiredApprovals => text.literal("Aprovações obrigatórias"),
        ProtectionMessage::DismissStale => text.literal("Descartar aprovações quando novos commits receberem push"),
        ProtectionMessage::BlockOnChanges => {
            text.literal("Bloquear a mesclagem enquanto houver alterações solicitadas");
        }
        ProtectionMessage::RestrictPushers => text.literal("Restringir quem pode fazer push"),
        ProtectionMessage::RestrictPeople => {
            text.literal(
                "Somente as pessoas abaixo podem fazer push ou mesclar. Chaves de implantação nunca podem.",
            );
        }
        ProtectionMessage::RestrictPeopleTeams => {
            text.literal(
                "Somente as pessoas e equipes abaixo podem fazer push ou mesclar. Chaves de implantação nunca podem.",
            );
        }
        ProtectionMessage::People => text.literal("Pessoas"),
        ProtectionMessage::PeoplePlaceholder => text.literal("alice bob"),
        ProtectionMessage::PeopleHint => {
            text.literal(
                "Nomes de usuário separados por espaços ou vírgulas. Cada um precisa de acesso de escrita.",
            );
        }
        ProtectionMessage::Teams => text.literal("Equipes"),
        ProtectionMessage::NoTeams => text.literal("Esta organização ainda não tem equipes."),
        ProtectionMessage::RequireChecks => text.literal("Exigir que as verificações de status sejam aprovadas"),
        ProtectionMessage::RequireChecksText => {
            text.literal(
                "Os pull requests só são mesclados quando o status mais recente de cada verificação abaixo é de \
                 sucesso no commit de origem deles.",
            );
        }
        ProtectionMessage::RequiredChecks => text.literal("Verificações obrigatórias"),
        ProtectionMessage::ChecksPlaceholder => text.literal("ci/build"),
        ProtectionMessage::AdminBypass => text.literal("Permitir que administradores ignorem"),
        ProtectionMessage::AdminBypassText => {
            text.literal(
                "Os administradores do repositório não ficam sujeitos a esta regra. Cada desvio é registrado no log de \
                 auditoria.",
            );
        }
        ProtectionMessage::SaveRule => text.literal("Salvar regra"),
        ProtectionMessage::DangerZone => text.literal("Zona de perigo"),
        ProtectionMessage::DeleteTitle => text.literal("Excluir esta regra"),
        ProtectionMessage::DeleteLead => text.literal("Os branches que correspondem a"),
        ProtectionMessage::DeleteTail => text.literal("perdem a proteção dela imediatamente."),
        ProtectionMessage::DeleteConfirm => text.literal("Entendo que esta regra será excluída"),
        ProtectionMessage::DeleteRule => text.literal("Excluir regra"),
        ProtectionMessage::NoMatches => text.literal("Ainda não corresponde a nenhum branch"),
    }
    ProtectionText<'a> => MessageWriter |text| {
        ProtectionText::MaxRules(max) => {
            text.literal("Este repositório já tem o máximo de ");
            text.number(max);
            text.literal(" regras. Exclua uma para adicionar outra.");
        }
        ProtectionText::ChecksHint(max) => {
            text.literal(
                "Um contexto por linha, exatamente como o seu CI o informa (diferencia maiúsculas de minúsculas), no \
                 máximo ",
            );
            text.number(max);
            text.literal(".");
        }
        ProtectionText::Matches(names) => {
            text.literal("Corresponde a ");
            text.argument(names);
        }
        ProtectionText::MatchesMore(names, more) => {
            text.literal("Corresponde a ");
            text.argument(names);
            text.literal(" e mais ");
            text.number(more);
        }
        ProtectionText::ListTitle(repository) => {
            text.literal("Proteção de branches · ");
            text.argument(repository);
        }
        ProtectionText::NewTitle(repository) => {
            text.literal("Nova regra de proteção de branch · ");
            text.argument(repository);
        }
        ProtectionText::EditTitle(pattern, repository) => {
            text.argument(pattern);
            text.literal(" · Proteção de branches · ");
            text.argument(repository);
        }
    }
    RuleBadge => MessageWriter |text| {
        RuleBadge::NoForcePushes => text.literal("Sem force-push"),
        RuleBadge::NoDeletion => text.literal("Sem exclusão"),
        RuleBadge::PullRequest => text.literal("Pull request"),
        RuleBadge::Approvals(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " aprovação",
                _ => " aprovações",
            });
        }
        RuleBadge::StatusChecks => text.literal("Verificações de status"),
        RuleBadge::Restricted => text.literal("Restrito"),
        RuleBadge::AdminsBypass => text.literal("Administradores podem ignorar"),
    }
}
