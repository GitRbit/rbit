//! Brazilian Portuguese (`pt-BR`) entries for `catalog/actions_ui/settings.rs`.

use crate::catalog::actions_ui::settings::ActionsSettingsMessage;
use crate::catalog::fallback::MessageWriter;

entries! { PtBr:
    ActionsSettingsMessage<'a> => MessageWriter |text| {
        ActionsSettingsMessage::RepositoryIntro { directory, repository } => {
            text.literal("Os fluxos de trabalho em ");
            text.argument(directory);
            text.literal(" são executados nos executores quando os eventos deles ocorrem em ");
            text.argument(repository);
            text.literal(".");
        }
        ActionsSettingsMessage::OrganizationIntro { organization } => {
            text.literal("Segredos e variáveis para os fluxos de trabalho dos repositórios de ");
            text.argument(organization);
            text.literal(". Um segredo ou variável de mesmo nome definido no próprio repositório prevalece.");
        }
        ActionsSettingsMessage::AccountIntro => {
            text.literal(
                "Segredos e variáveis para os fluxos de trabalho dos seus repositórios. Um segredo ou variável de mesmo \
                 nome definido no próprio repositório prevalece.",
            );
        }
        ActionsSettingsMessage::General => text.literal("Geral"),
        ActionsSettingsMessage::RunWorkflows => text.literal("Executar fluxos de trabalho neste repositório"),
        ActionsSettingsMessage::RunWorkflowsHint => {
            text.literal(
                "Desativado: nenhuma nova execução começa e a aba Actions fica oculta. As execuções anteriores e os \
                 logs delas são mantidos.",
            );
        }
        ActionsSettingsMessage::DefaultPermission => {
            text.literal("Permissão padrão do token dos fluxos de trabalho");
        }
        ActionsSettingsMessage::PermissionHint { key } => {
            text.literal("A chave ");
            text.argument(key);
            text.literal(
                " de um fluxo de trabalho só pode reduzi-la. Execuções vindas de forks e de autores externos sempre \
                 recebem um token somente leitura.",
            );
        }
        ActionsSettingsMessage::PermissionRead => text.literal("Leitura"),
        ActionsSettingsMessage::PermissionReadHint => {
            text.literal("Os fluxos de trabalho podem ler o conteúdo, as issues e os pull requests do repositório.");
        }
        ActionsSettingsMessage::PermissionWrite => text.literal("Leitura e escrita"),
        ActionsSettingsMessage::PermissionWriteHint => {
            text.literal(
                "Os fluxos de trabalho também podem fazer push e comentar em issues e pull requests. A proteção de \
                 branch continua valendo.",
            );
        }
        ActionsSettingsMessage::SaveChanges => text.literal("Salvar alterações"),
        ActionsSettingsMessage::RepositoryRunnersHint => {
            text.literal("Os executores deste repositório, os que ele herda e os tokens de registro.");
        }
        ActionsSettingsMessage::OrganizationRunnersHint => {
            text.literal("Executores para todos os repositórios da organização e tokens de registro.");
        }
        ActionsSettingsMessage::AccountRunnersHint => {
            text.literal("Executores para todos os repositórios da sua conta e tokens de registro.");
        }
        ActionsSettingsMessage::EnvironmentsHint => {
            text.literal(
                "Ambientes de implantação: revisores obrigatórios, branches permitidos e os próprios segredos e \
                 variáveis.",
            );
        }
        ActionsSettingsMessage::TrustedTitle => text.literal("Autores de pull requests confiáveis"),
        ActionsSettingsMessage::TrustedIntro => {
            text.literal(
                "Os pull requests deles vindos de forks são executados sem aguardar aprovação, ainda com um token \
                 somente leitura e sem segredos nem variáveis. Os autores são adicionados na aprovação de uma execução, \
                 com “Sempre executar os pull requests deste autor”.",
            );
        }
        ActionsSettingsMessage::NoTrustedAuthors => text.literal("Nenhum autor confiável"),
        ActionsSettingsMessage::NoTrustedAuthorsHint => {
            text.literal(
                "Pull requests vindos de forks e de pessoas sem acesso de escrita aguardam que alguém com acesso de \
                 escrita aprove cada execução.",
            );
        }
        ActionsSettingsMessage::TrustedSince { since } => {
            text.literal("Marcado como confiável ");
            text.argument(since);
        }
        ActionsSettingsMessage::TrustedSinceBy { since, approver } => {
            text.literal("Marcado como confiável ");
            text.argument(since);
            text.literal(", por ");
            text.argument(approver);
        }
        ActionsSettingsMessage::StopTrusting { name } => {
            text.literal("Deixar de confiar em ");
            text.argument(name);
        }
        ActionsSettingsMessage::Remove => text.literal("Remover"),
        ActionsSettingsMessage::Saved => text.literal("Configurações salvas."),
        ActionsSettingsMessage::SecretSaved => text.literal("Segredo salvo."),
        ActionsSettingsMessage::SecretDeleted => text.literal("Segredo excluído."),
        ActionsSettingsMessage::VariableSaved => text.literal("Variável salva."),
        ActionsSettingsMessage::VariableDeleted => text.literal("Variável excluída."),
        ActionsSettingsMessage::Untrusted => {
            text.literal("Os pull requests do autor voltaram a precisar de aprovação.");
        }
        ActionsSettingsMessage::RepositoryAccess => text.literal("Acesso aos repositórios"),
        ActionsSettingsMessage::AllRepositories => text.literal("Todos os repositórios"),
        ActionsSettingsMessage::AllRepositoriesHint => {
            text.literal("Todos os repositórios deste proprietário, inclusive os criados depois.");
        }
        ActionsSettingsMessage::SelectedRepositories => text.literal("Repositórios selecionados"),
        ActionsSettingsMessage::SelectedRepositoriesHint => {
            text.literal("Somente os repositórios marcados abaixo.");
        }
        ActionsSettingsMessage::NoRepositoriesYet => text.literal("Este proprietário ainda não tem repositórios."),
    }
}
