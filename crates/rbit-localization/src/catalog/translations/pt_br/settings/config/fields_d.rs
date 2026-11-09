//! Brazilian Portuguese (`pt-BR`) labels and help of the configuration fields: Actions
//! and the sign-in provider cards.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

/// The help shared by the deployment settings that apply after a restart.
const RESTART: &str = "Configuração de implantação aplicada após reiniciar o servidor.";

config_entries! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "actions", key: "enabled" } => text.literal("Ativar este recurso"),
        ConfigText::Help { section: "actions", key: "enabled" } => text.literal(
            "Executa fluxos de trabalho no rbit-runner. Quando desativado, o Actions fica totalmente ausente: sem \
             páginas, sem endpoint de executores, sem trabalho em segundo plano, e os arquivos de fluxo de trabalho são \
             ignorados. Exige reinicialização.",
        ),
        ConfigText::Label { section: "actions", key: "enable_for_new_repositories" } => {
            text.literal("Ativar para novos repositórios");
        }
        ConfigText::Help { section: "actions", key: "enable_for_new_repositories" } => text.literal(
            "Ativa o Actions nos repositórios criados a partir de agora. Os repositórios existentes mantêm a própria \
             configuração.",
        ),
        ConfigText::Label { section: "actions", key: "secrets_key_file" } => text.literal("Arquivo da chave dos segredos"),
        ConfigText::Help { section: "actions", key: "secrets_key_file" } => text.literal(
            "Chave de criptografia dos segredos do Actions. Em branco usa actions/secrets.key no diretório de dados. \
             Faça backup dela separadamente.",
        ),
        ConfigText::Label { section: "actions", key: "log_retention_days" } => text.literal("Retenção dos logs (dias)"),
        ConfigText::Help { section: "actions", key: "log_retention_days" } => {
            text.literal("Dias em que os logs dos jobs são mantidos.");
        }
        ConfigText::Label { section: "actions", key: "run_retention_days" } => {
            text.literal("Retenção das execuções (dias)");
        }
        ConfigText::Help { section: "actions", key: "run_retention_days" } => {
            text.literal("Dias em que as execuções concluídas são mantidas.");
        }
        ConfigText::Label { section: "actions", key: "max_log_bytes" } => text.literal("Log máximo (bytes)"),
        ConfigText::Help { section: "actions", key: "max_log_bytes" } => {
            text.literal("Bytes de log mantidos por tarefa; as linhas posteriores são descartadas.");
        }
        ConfigText::Label { section: "actions", key: "max_workflow_bytes" } => {
            text.literal("Arquivo de fluxo de trabalho máximo (bytes)");
        }
        ConfigText::Help { section: "actions", key: "max_workflow_bytes" } => {
            text.literal("Maior arquivo de fluxo de trabalho lido, em bytes.");
        }
        ConfigText::Label { section: "actions", key: "zombie_timeout_seconds" } => {
            text.literal("Tempo limite de heartbeat (segundos)");
        }
        ConfigText::Help { section: "actions", key: "zombie_timeout_seconds" } => {
            text.literal("Uma tarefa em execução sem heartbeat por esse tempo falha.");
        }
        ConfigText::Label { section: "actions", key: "endless_timeout_seconds" } => {
            text.literal("Tempo limite da tarefa (segundos)");
        }
        ConfigText::Help { section: "actions", key: "endless_timeout_seconds" } => {
            text.literal("Uma tarefa em execução por esse tempo falha.");
        }
        ConfigText::Label { section: "actions", key: "abandoned_timeout_seconds" } => {
            text.literal("Tempo limite na fila (segundos)");
        }
        ConfigText::Help { section: "actions", key: "abandoned_timeout_seconds" } => {
            text.literal("Um job em espera que nenhum executor pegue por esse tempo é cancelado.");
        }
        ConfigText::Label { section: "actions", key: "evaluation_concurrency" } => {
            text.literal("Concorrência das avaliações");
        }
        ConfigText::Help { section: "actions", key: "evaluation_concurrency" } => text.literal(
            "Avaliações de fluxos de trabalho (leituras Git e análise de YAML) em execução ao mesmo tempo, de 1 a 16.",
        ),
        ConfigText::Label { section: "actions", key: "instance_runners_for_all" } => {
            text.literal("Executores da instância atendem a todos");
        }
        ConfigText::Help { section: "actions", key: "instance_runners_for_all" } => text.literal(
            "Permite que os executores da instância executem jobs de todos os proprietários. Quando desativado, eles \
             atendem apenas aos proprietários que um administrador permitiu.",
        ),
        ConfigText::Label { section: "actions", key: "max_log_storage_bytes" } => {
            text.literal("Orçamento de armazenamento de logs (bytes)");
        }
        ConfigText::Help { section: "actions", key: "max_log_storage_bytes" } => text.literal(
            "Bytes de log mantidos em toda a instância; acima disso, as execuções concluídas mais antigas perdem os \
             logs primeiro.",
        ),
        ConfigText::Label { section: "actions", key: "max_waiting_jobs_per_repository" } => {
            text.literal("Máximo de jobs em espera por repositório");
        }
        ConfigText::Help { section: "actions", key: "max_waiting_jobs_per_repository" } => {
            text.literal("Novas execuções além dessa quantidade de jobs em espera falham com um erro legível.");
        }
        ConfigText::Label { section: "actions", key: "max_runs_per_event" } => text.literal("Máximo de execuções por evento"),
        ConfigText::Help { section: "actions", key: "max_runs_per_event" } => {
            text.literal("Execuções que um evento de push ou pull request pode iniciar.");
        }
        ConfigText::Label { section: "actions", key: "max_jobs_per_event" } => text.literal("Máximo de jobs por evento"),
        ConfigText::Help { section: "actions", key: "max_jobs_per_event" } => {
            text.literal("Jobs que um evento pode criar no conjunto das execuções dele.");
        }
        ConfigText::Label { section: "actions", key: "max_schedules_per_repository" } => {
            text.literal("Máximo de agendamentos por repositório");
        }
        ConfigText::Help { section: "actions", key: "max_schedules_per_repository" } => {
            text.literal("Entradas de fluxos de trabalho agendados por repositório.");
        }
        ConfigText::Label { section: "actions", key: "long_poll_seconds" } => {
            text.literal("Long poll dos executores (segundos)");
        }
        ConfigText::Help { section: "actions", key: "long_poll_seconds" } => {
            text.literal("Por quanto tempo a consulta de trabalho de um executor fica aberta, de 1 a 25.");
        }
        ConfigText::Label { section: "actions", key: "max_polls" } => text.literal("Máximo de consultas de executores"),
        ConfigText::Help { section: "actions", key: "max_polls" } => text.literal(
            "Consultas de executores abertas ao mesmo tempo; acima disso, as consultas são recusadas até que uma \
             termine.",
        ),
        ConfigText::Label { section: "actions", key: "max_artifact_bytes" } => {
            text.literal("Tamanho máximo do artefato (bytes)");
        }
        ConfigText::Help { section: "actions", key: "max_artifact_bytes" } => {
            text.literal("Maior artefato que um job pode enviar.");
        }
        ConfigText::Label { section: "actions", key: "max_artifact_storage_bytes" } => {
            text.literal("Armazenamento de artefatos por repositório (bytes)");
        }
        ConfigText::Help { section: "actions", key: "max_artifact_storage_bytes" } => text.literal(
            "Bytes de artefatos mantidos por repositório; envios acima disso são recusados. Os backups (formato 5 ou \
             posterior) incluem os artefatos.",
        ),
        ConfigText::Label { section: "actions", key: "artifact_retention_days" } => {
            text.literal("Retenção dos artefatos (dias)");
        }
        ConfigText::Help { section: "actions", key: "artifact_retention_days" } => text.literal(
            "Dias em que os artefatos são mantidos. O retention-days de um fluxo de trabalho só pode reduzi-lo.",
        ),
        ConfigText::Label { section: "actions", key: "max_cache_storage_bytes" } => {
            text.literal("Armazenamento de cache por repositório (bytes)");
        }
        ConfigText::Help { section: "actions", key: "max_cache_storage_bytes" } => text.literal(
            "Bytes de actions/cache mantidos por repositório para todos os executores dele; as entradas usadas há \
             mais tempo saem primeiro. Os backups (formato 5 ou posterior) incluem as entradas de cache.",
        ),
        ConfigText::Label { section: "actions", key: "cache_retention_days" } => text.literal("Retenção do cache (dias)"),
        ConfigText::Help { section: "actions", key: "cache_retention_days" } => {
            text.literal("Dias em que uma entrada de cache é mantida sem ser restaurada.");
        }
        ConfigText::Label { section: "provider", key: "kind" } => text.literal("Tipo de provedor"),
        ConfigText::Help { section: "provider", key: "kind" } => text.literal(RESTART),
        ConfigText::Label { section: "provider", key: "id" } => text.literal("ID do provedor"),
        ConfigText::Help { section: "provider", key: "id" } => text.literal(
            "Um ID curto e único usado nas URLs de callback. Alterá-lo exige digitar novamente o segredo do provedor.",
        ),
        ConfigText::Label { section: "provider", key: "name" } => text.literal("Nome de exibição"),
        ConfigText::Help { section: "provider", key: "name" } => {
            text.literal("Exibido nos botões e nas configurações.");
        }
        ConfigText::Label { section: "provider", key: "issuer" } => text.literal("URL do emissor OIDC"),
        ConfigText::Help { section: "provider", key: "issuer" } => {
            text.literal("URL do emissor OIDC (somente kind = \"oidc\").");
        }
        ConfigText::Label { section: "provider", key: "base_url" } => text.literal("URL do GitHub Enterprise"),
        ConfigText::Help { section: "provider", key: "base_url" } => text.literal(
            "URL do GitHub Enterprise Server (somente kind = \"github\"); padrão https://github.com.",
        ),
        ConfigText::Label { section: "provider", key: "client_id" } => text.literal("ID do cliente"),
        ConfigText::Help { section: "provider", key: "client_id" } => text.literal(RESTART),
        ConfigText::Label { section: "provider", key: "client_secret" } => text.literal("Segredo do cliente"),
        ConfigText::Help { section: "provider", key: "client_secret" } => {
            text.literal("Nunca registrado em log nem exibido (veja a implementação de Debug).");
        }
        ConfigText::Label { section: "provider", key: "scopes" } => text.literal("Escopos solicitados"),
        ConfigText::Help { section: "provider", key: "scopes" } => text.literal(
            "Digite um escopo por linha. Use os padrões do provedor para solicitar os escopos recomendados por ele.",
        ),
        ConfigText::Label { section: "provider", key: "allow_registration" } => text.literal("Criação de contas"),
        ConfigText::Help { section: "provider", key: "allow_registration" } => text.literal(RESTART),
        ConfigText::Label { section: "provider", key: "trust_email" } => {
            text.literal("Confiar em endereços de e-mail verificados");
        }
        ConfigText::Help { section: "provider", key: "trust_email" } => text.literal(
            "Vincula uma primeira entrada à única conta cujo e-mail verificado seja igual ao e-mail verificado pelo \
             provedor. Somente para provedores que controlam os endereços dos seus usuários.",
        ),
        ConfigText::Label { section: "provider", key: "trust_mfa" } => {
            text.literal("Confiar na autenticação multifator do provedor");
        }
        ConfigText::Help { section: "provider", key: "trust_mfa" } => text.literal(
            "Pula a etapa de dois fatores da própria conta quando a declaração amr do ID token indica que o provedor \
             fez autenticação multifator (\"mfa\"). Somente OIDC.",
        ),
    }
}
