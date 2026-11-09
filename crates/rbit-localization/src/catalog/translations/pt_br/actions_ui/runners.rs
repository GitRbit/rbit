//! Brazilian Portuguese (`pt-BR`) entries for `catalog/actions_ui/runners.rs`.

use crate::catalog::actions_ui::runners::ActionsRunnersMessage;
use crate::catalog::fallback::MessageWriter;
use crate::plural::Plural;

entries! { PtBr:
    ActionsRunnersMessage<'a> => MessageWriter |text| {
        ActionsRunnersMessage::Title => text.literal("Executores"),
        ActionsRunnersMessage::AdminIntro { setting } => {
            text.literal(
                "Os executores da instância recebem jobs dos proprietários que um administrador permite, ou de todos \
                 os proprietários com ",
            );
            text.argument(setting);
            text.literal(".");
        }
        ActionsRunnersMessage::TokenCreated => text.literal("Token de registro criado"),
        ActionsRunnersMessage::TokenCreatedHint { command } => {
            text.literal(
                "Copie-o agora: ele não será exibido novamente. Na máquina que deve executar os jobs, execute o comando \
                 abaixo e cole o token quando ele for solicitado; depois inicie o executor com ",
            );
            text.argument(command);
            text.literal(".");
        }
        ActionsRunnersMessage::Token => text.literal("Token"),
        ActionsRunnersMessage::CopyToken => text.literal("Copiar o token"),
        ActionsRunnersMessage::Copy => text.literal("Copiar"),
        ActionsRunnersMessage::Command => text.literal("Comando"),
        ActionsRunnersMessage::CopyCommand => text.literal("Copiar o comando"),
        ActionsRunnersMessage::LabelsHint { container, host } => {
            text.literal("As etiquetas definem como os jobs são executados: ");
            text.argument(container);
            text.literal(" executa no Docker ou no Podman, ");
            text.argument(host);
            text.literal(" executa na própria máquina, sem isolamento — apenas para repositórios confiáveis.");
        }
        ActionsRunnersMessage::RepositoryIntro { program } => intro(text, program, None, "deste repositório"),
        ActionsRunnersMessage::OwnerIntro { program, owner } => intro(text, program, Some(owner), ""),
        ActionsRunnersMessage::AccountIntro { program } => intro(text, program, None, "dos seus repositórios"),
        ActionsRunnersMessage::InstanceIntro { program } => {
            intro(text, program, None, "de todos os proprietários com acesso aos executores da instância");
        }
        ActionsRunnersMessage::NoRunners => text.literal("Ainda não há executores"),
        ActionsRunnersMessage::NoRunnersHint => {
            text.literal(
                "Crie um token de registro abaixo e registre um executor com ele. Os jobs aguardam até que um executor \
                 com as etiquetas deles esteja online.",
            );
        }
        ActionsRunnersMessage::Online => text.literal("online"),
        ActionsRunnersMessage::Offline => text.literal("offline"),
        ActionsRunnersMessage::Disabled => text.literal("desativado"),
        ActionsRunnersMessage::NeverSeen => text.literal("nunca visto"),
        ActionsRunnersMessage::OwnerScope => text.literal("proprietário"),
        ActionsRunnersMessage::InstanceScope => text.literal("instância"),
        ActionsRunnersMessage::Ephemeral => text.literal("efêmero"),
        ActionsRunnersMessage::HostNoIsolation => text.literal("host — sem isolamento"),
        ActionsRunnersMessage::HostWarning => {
            text.literal("Os jobs nas etiquetas de host dele são executados como o usuário do executor, sem isolamento");
        }
        ActionsRunnersMessage::Capacity { count } => {
            text.number(count);
            text.literal(" ao mesmo tempo");
        }
        ActionsRunnersMessage::Seen { when } => {
            text.literal("visto ");
            text.argument(when);
        }
        ActionsRunnersMessage::Enable => text.literal("Ativar"),
        ActionsRunnersMessage::Disable => text.literal("Desativar"),
        ActionsRunnersMessage::Remove => text.literal("Remover"),
        ActionsRunnersMessage::RemoveRunner { name } => {
            text.literal("Remover ");
            text.argument(name);
        }
        ActionsRunnersMessage::TokensTitle => text.literal("Tokens de registro"),
        ActionsRunnersMessage::TokensIntro => {
            text.literal(
                "Um executor troca um token de registro pela própria credencial uma única vez. Os tokens são de uso \
                 único e expiram, a menos que você escolha outra opção.",
            );
        }
        ActionsRunnersMessage::ExpiresAfter => text.literal("Expira após"),
        ActionsRunnersMessage::OnlyTheseLabels => text.literal("Somente estas etiquetas"),
        ActionsRunnersMessage::LabelsPlaceholder => text.literal("ubuntu-latest, arm64"),
        ActionsRunnersMessage::Optional => text.literal("(opcional)"),
        ActionsRunnersMessage::Reusable => {
            text.literal("Permitir que qualquer número de executores se registre com ele até que expire");
        }
        ActionsRunnersMessage::NewToken => text.literal("Novo token de registro"),
        ActionsRunnersMessage::Created { when } => {
            text.literal("Criado ");
            text.argument(when);
        }
        ActionsRunnersMessage::Expires { when } => {
            text.literal("expira ");
            text.argument(when);
        }
        ActionsRunnersMessage::NoLongerUsable => text.literal("não pode mais ser usado"),
        ActionsRunnersMessage::Revoked => text.literal("revogado"),
        ActionsRunnersMessage::Expired => text.literal("expirado"),
        ActionsRunnersMessage::UnlimitedUses => text.literal("usos ilimitados"),
        ActionsRunnersMessage::UsedUp => text.literal("esgotado"),
        ActionsRunnersMessage::UsesLeft { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " uso restante",
                _ => " usos restantes",
            });
        }
        ActionsRunnersMessage::Revoke => text.literal("Revogar"),
        ActionsRunnersMessage::Hours { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " hora",
                _ => " horas",
            });
        }
        ActionsRunnersMessage::Days { count } => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " dia",
                _ => " dias",
            });
        }
    }
}

/// “Máquinas que executam {program} e recebem os jobs {de owner ou subject}.”
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.literal("Máquinas que executam ");
    text.argument(program);
    text.literal(" e recebem os jobs ");
    match owner {
        Some(owner) => {
            text.literal("de ");
            text.argument(owner);
        }
        None => text.literal(subject),
    }
    text.literal(".");
}
