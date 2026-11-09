//! Brazilian Portuguese (`pt-BR`) labels and help of the configuration fields:
//! notifications, operations, the API, webhooks, releases, mirrors, imports, issues,
//! pull requests, the browser editor and Git LFS.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::config::ConfigText;

/// The label of every feature switch.
const ENABLE: &str = "Ativar este recurso";
/// The label of every private-network switch.
const PRIVATE: &str = "Permitir destinos em redes privadas";

config_entries! {
    ConfigText<'a> => MessageWriter |text| {
        ConfigText::Label { section: "notifications", key: "email" } => text.literal("Enviar e-mails de notificação"),
        ConfigText::Help { section: "notifications", key: "email" } => {
            text.literal("Se as notificações também são enviadas por e-mail.");
        }
        ConfigText::Label { section: "notifications", key: "default_email" } => {
            text.literal("Inscrição de e-mail padrão");
        }
        ConfigText::Help { section: "notifications", key: "default_email" } => {
            text.literal("A preferência dos usuários que nunca escolheram uma.");
        }
        ConfigText::Label { section: "notifications", key: "digest_hour" } => {
            text.literal("Hora padrão do resumo (UTC)");
        }
        ConfigText::Help { section: "notifications", key: "digest_hour" } => text.literal(
            "A hora, de 0 a 23 em UTC, em que os resumos diários e semanais são enviados aos usuários que não \
             escolheram uma. Administração do site → Configurações a substitui em tempo de execução.",
        ),
        ConfigText::Label { section: "operations", key: "metrics" } => text.literal("Ativar o endpoint de métricas"),
        ConfigText::Help { section: "operations", key: "metrics" } => {
            text.literal("Expõe métricas do Prometheus em /metrics.");
        }
        ConfigText::Label { section: "operations", key: "metrics_token" } => text.literal("Token de acesso às métricas"),
        ConfigText::Help { section: "operations", key: "metrics_token" } => text.literal(
            "Quando definido, /metrics exige Authorization: Bearer <token>. Deixe sem definir apenas se o endpoint não \
             puder ser acessado de redes não confiáveis.",
        ),
        ConfigText::Label { section: "operations", key: "warm_caches_on_push" } => {
            text.literal("Aquecer os caches a cada push");
        }
        ConfigText::Help { section: "operations", key: "warm_caches_on_push" } => text.literal(
            "Aquece os caches de linguagens e do último commit do branch padrão após cada push, para que o primeiro \
             visitante depois de um push não pague pelo cálculo.",
        ),
        ConfigText::Label { section: "operations", key: "maintenance_interval_minutes" } => {
            text.literal("Intervalo de manutenção (minutos)");
        }
        ConfigText::Help { section: "operations", key: "maintenance_interval_minutes" } => text.literal(
            "Minutos entre as execuções de manutenção (git gc --auto nos repositórios que receberam push desde a \
             última execução e, depois, limpeza do cache de insights). 0 desativa a manutenção periódica.",
        ),
        ConfigText::Label { section: "operations", key: "job_concurrency" } => text.literal("Concorrência de jobs"),
        ConfigText::Help { section: "operations", key: "job_concurrency" } => text.literal(
            "Limite superior de jobs em segundo plano (aquecimento de cache, git gc) em execução ao mesmo tempo.",
        ),
        ConfigText::Label { section: "operations", key: "insights_cache_max_age_days" } => {
            text.literal("Idade máxima do cache de insights (dias)");
        }
        ConfigText::Help { section: "operations", key: "insights_cache_max_age_days" } => text.literal(
            "Entradas do cache de insights mais antigas que isso são removidas pela manutenção. 0 mantém as entradas \
             independentemente da idade.",
        ),
        ConfigText::Label { section: "operations", key: "insights_cache_max_entries" } => {
            text.literal("Máximo de entradas do cache de insights");
        }
        ConfigText::Help { section: "operations", key: "insights_cache_max_entries" } => text.literal(
            "Quando o cache de insights tem mais entradas que isso, a manutenção remove as mais antigas. 0 significa \
             sem limite.",
        ),
        ConfigText::Label { section: "api", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "api", key: "enabled" } => {
            text.literal("Serve /api/v1. Quando desativado, todo caminho da API responde 404.");
        }
        ConfigText::Label { section: "api", key: "default_page_size" } => text.literal("Tamanho padrão da página"),
        ConfigText::Help { section: "api", key: "default_page_size" } => {
            text.literal("Itens por página quando a requisição não informa.");
        }
        ConfigText::Label { section: "api", key: "max_page_size" } => text.literal("Tamanho máximo da página"),
        ConfigText::Help { section: "api", key: "max_page_size" } => text.literal("Limite superior de ?per_page=."),
        ConfigText::Label { section: "webhooks", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "webhooks", key: "enabled" } => text.literal(
            "Entrega webhooks. Quando desativado, os hooks ainda podem ser configurados, mas nada é enviado.",
        ),
        ConfigText::Label { section: "webhooks", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "webhooks", key: "allow_private_addresses" } => text.literal(
            "Permite destinos que resolvem para endereços de loopback, privados, link-local ou unique-local.",
        ),
        ConfigText::Label { section: "webhooks", key: "timeout_seconds" } => text.literal("Tempo limite (segundos)"),
        ConfigText::Help { section: "webhooks", key: "timeout_seconds" } => text.literal(
            "Limite superior de uma tentativa de entrega (conectar, enviar e ler o status da resposta).",
        ),
        ConfigText::Label { section: "webhooks", key: "max_attempts" } => text.literal("Máximo de tentativas"),
        ConfigText::Help { section: "webhooks", key: "max_attempts" } => {
            text.literal("Tentativas por entrega, incluindo a primeira, antes de ela ser marcada como falha.");
        }
        ConfigText::Label { section: "webhooks", key: "concurrency" } => text.literal("Concorrência"),
        ConfigText::Help { section: "webhooks", key: "concurrency" } => {
            text.literal("Entregas em andamento ao mesmo tempo em toda a instância.");
        }
        ConfigText::Label { section: "webhooks", key: "delivery_retention_days" } => {
            text.literal("Retenção das entregas (dias)");
        }
        ConfigText::Help { section: "webhooks", key: "delivery_retention_days" } => {
            text.literal("Dias em que o log de entregas mantém as requisições e respostas.");
        }
        ConfigText::Label { section: "releases", key: "max_asset_bytes" } => text.literal("Arquivo máximo (bytes)"),
        ConfigText::Help { section: "releases", key: "max_asset_bytes" } => {
            text.literal("Maior arquivo anexado, em bytes.");
        }
        ConfigText::Label { section: "releases", key: "max_assets_per_release" } => {
            text.literal("Máximo de arquivos por release");
        }
        ConfigText::Help { section: "releases", key: "max_assets_per_release" } => {
            text.literal("Arquivos anexados por release.");
        }
        ConfigText::Label { section: "releases", key: "upload_timeout_seconds" } => {
            text.literal("Tempo limite de envio (segundos)");
        }
        ConfigText::Help { section: "releases", key: "upload_timeout_seconds" } => {
            text.literal("Limite superior, em segundos, de um envio.");
        }
        ConfigText::Label { section: "mirrors", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "mirrors", key: "enabled" } => {
            text.literal("Permite criar espelhos e executa as buscas agendadas.");
        }
        ConfigText::Label { section: "mirrors", key: "default_interval_minutes" } => {
            text.literal("Intervalo padrão (minutos)");
        }
        ConfigText::Help { section: "mirrors", key: "default_interval_minutes" } => {
            text.literal("Intervalo de busca dos novos espelhos.");
        }
        ConfigText::Label { section: "mirrors", key: "min_interval_minutes" } => text.literal("Intervalo mínimo (minutos)"),
        ConfigText::Help { section: "mirrors", key: "min_interval_minutes" } => {
            text.literal("O menor intervalo que um espelho pode ter configurado.");
        }
        ConfigText::Label { section: "mirrors", key: "fetch_timeout_seconds" } => {
            text.literal("Tempo limite da busca (segundos)");
        }
        ConfigText::Help { section: "mirrors", key: "fetch_timeout_seconds" } => {
            text.literal("Limite superior de uma busca.");
        }
        ConfigText::Label { section: "mirrors", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "mirrors", key: "allow_private_addresses" } => text.literal(
            "Permite URLs de upstream que resolvem para endereços de loopback, privados, link-local ou unique-local.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "push_mirrors", key: "enabled" } => {
            text.literal("Permite configurar espelhos de push e fazer push para eles.");
        }
        ConfigText::Label { section: "push_mirrors", key: "interval_minutes" } => text.literal("Intervalo (minutos)"),
        ConfigText::Help { section: "push_mirrors", key: "interval_minutes" } => {
            text.literal("Faz push para cada espelho pelo menos com esta frequência.");
        }
        ConfigText::Label { section: "push_mirrors", key: "debounce_seconds" } => text.literal("Espera (segundos)"),
        ConfigText::Help { section: "push_mirrors", key: "debounce_seconds" } => {
            text.literal("Atraso após um push antes de fazer push para os espelhos.");
        }
        ConfigText::Label { section: "push_mirrors", key: "push_timeout_seconds" } => {
            text.literal("Tempo limite do push (segundos)");
        }
        ConfigText::Help { section: "push_mirrors", key: "push_timeout_seconds" } => {
            text.literal("Limite superior de um git push.");
        }
        ConfigText::Label { section: "push_mirrors", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "push_mirrors", key: "allow_private_addresses" } => text.literal(
            "Permite remotos que resolvem para endereços de loopback, privados, link-local ou unique-local.",
        ),
        ConfigText::Label { section: "push_mirrors", key: "allow_local_urls" } => {
            text.literal("Permitir remotos de arquivos locais (somente testes)");
        }
        ConfigText::Help { section: "push_mirrors", key: "allow_local_urls" } => {
            text.literal("Aceita remotos file:// (somente testes; veja a documentação do módulo).");
        }
        ConfigText::Label { section: "imports", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "imports", key: "enabled" } => {
            text.literal("Oferece a importação na página “Novo repositório”.");
        }
        ConfigText::Label { section: "imports", key: "allow_private_addresses" } => text.literal(PRIVATE),
        ConfigText::Help { section: "imports", key: "allow_private_addresses" } => text.literal(
            "Permite origens que resolvem para endereços de loopback, privados, link-local ou unique-local.",
        ),
        ConfigText::Label { section: "imports", key: "max_concurrent" } => text.literal("Máximo simultâneo"),
        ConfigText::Help { section: "imports", key: "max_concurrent" } => {
            text.literal("Importações em execução ao mesmo tempo.");
        }
        ConfigText::Label { section: "imports", key: "git_timeout_seconds" } => {
            text.literal("Tempo limite do Git (segundos)");
        }
        ConfigText::Help { section: "imports", key: "git_timeout_seconds" } => {
            text.literal("Limite superior da cópia Git.");
        }
        ConfigText::Label { section: "imports", key: "api_timeout_seconds" } => {
            text.literal("Tempo limite da API (segundos)");
        }
        ConfigText::Help { section: "imports", key: "api_timeout_seconds" } => {
            text.literal("Limite superior de uma requisição à API.");
        }
        ConfigText::Label { section: "imports", key: "rate_limit_max_wait_seconds" } => {
            text.literal("Espera máxima pelo limite de requisições (segundos)");
        }
        ConfigText::Help { section: "imports", key: "rate_limit_max_wait_seconds" } => {
            text.literal("Espera máxima até que o limite de requisições da origem seja redefinido.");
        }
        ConfigText::Label { section: "imports", key: "author_mapping" } => {
            text.literal("Correspondência dos autores importados");
        }
        ConfigText::Help { section: "imports", key: "author_mapping" } => {
            text.literal("Como os autores na origem se tornam contas locais.");
        }
        ConfigText::Label { section: "issues", key: "scan_commit_limit" } => {
            text.literal("Limite de commits na varredura");
        }
        ConfigText::Help { section: "issues", key: "scan_commit_limit" } => text.literal(
            "Commits varridos por atualização de ref com push em busca de referências e palavras-chave de \
             fechamento.",
        ),
        ConfigText::Label { section: "issues", key: "notifications" } => text.literal("Notificações"),
        ConfigText::Help { section: "issues", key: "notifications" } => {
            text.literal("Se as notificações no aplicativo são geradas e exibidas.");
        }
        ConfigText::Label { section: "issues", key: "notification_retention_days" } => {
            text.literal("Retenção das notificações (dias)");
        }
        ConfigText::Help { section: "issues", key: "notification_retention_days" } => {
            text.literal("Dias em que uma notificação lida é mantida.");
        }
        ConfigText::Label { section: "pulls", key: "merge_timeout_seconds" } => {
            text.literal("Tempo limite da mesclagem (segundos)");
        }
        ConfigText::Help { section: "pulls", key: "merge_timeout_seconds" } => text.literal(
            "Limite superior, em segundos, de uma mesclagem ou mesclagem de teste: todo o trabalho git dela, da \
             criação do repositório temporário até o fetch e a atualização de ref do repositório base.",
        ),
        ConfigText::Label { section: "pulls", key: "max_concurrent_merges" } => {
            text.literal("Máximo de mesclagens simultâneas");
        }
        ConfigText::Help { section: "pulls", key: "max_concurrent_merges" } => text.literal(
            "Mesclagens em execução ao mesmo tempo em toda a instância (sempre uma por repositório base).",
        ),
        ConfigText::Label { section: "pulls", key: "mergeability_concurrency" } => {
            text.literal("Concorrência das verificações de mesclagem");
        }
        ConfigText::Help { section: "pulls", key: "mergeability_concurrency" } => {
            text.literal("Mesclagens de teste para verificar a possibilidade de mesclagem em execução ao mesmo tempo.");
        }
        ConfigText::Label { section: "pulls", key: "max_commits" } => text.literal("Máximo de commits"),
        ConfigText::Help { section: "pulls", key: "max_commits" } => {
            text.literal("Commits exibidos por pull request.");
        }
        ConfigText::Label { section: "pulls", key: "noreply_domain" } => text.literal("Domínio de e-mail no-reply"),
        ConfigText::Help { section: "pulls", key: "noreply_domain" } => text.literal(
            "Domínio usado para autores no-reply nos commits gerados. Em branco usa noreply seguido do nome do seu \
             host público.",
        ),
        ConfigText::Label { section: "editor", key: "max_upload_bytes" } => text.literal("Envio máximo (bytes)"),
        ConfigText::Help { section: "editor", key: "max_upload_bytes" } => {
            text.literal("Maior arquivo enviado, em bytes.");
        }
        ConfigText::Label { section: "editor", key: "max_upload_files" } => {
            text.literal("Máximo de arquivos por envio");
        }
        ConfigText::Help { section: "editor", key: "max_upload_files" } => text.literal("Arquivos por envio."),
        ConfigText::Label { section: "editor", key: "upload_timeout_seconds" } => {
            text.literal("Tempo limite de envio (segundos)");
        }
        ConfigText::Help { section: "editor", key: "upload_timeout_seconds" } => {
            text.literal("Limite superior, em segundos, de uma requisição de envio.");
        }
        ConfigText::Label { section: "lfs", key: "enabled" } => text.literal(ENABLE),
        ConfigText::Help { section: "lfs", key: "enabled" } => {
            text.literal("Serve a API do LFS e o comando SSH git-lfs-authenticate.");
        }
        ConfigText::Label { section: "lfs", key: "max_object_bytes" } => text.literal("Objeto máximo (bytes)"),
        ConfigText::Help { section: "lfs", key: "max_object_bytes" } => text.literal("Maior objeto, em bytes."),
        ConfigText::Label { section: "lfs", key: "token_lifetime_seconds" } => {
            text.literal("Validade do token (segundos)");
        }
        ConfigText::Help { section: "lfs", key: "token_lifetime_seconds" } => {
            text.literal("Validade dos cabeçalhos de transferência assinados, em segundos.");
        }
    }
}
