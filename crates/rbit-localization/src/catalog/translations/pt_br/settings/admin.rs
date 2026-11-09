//! Brazilian Portuguese (`pt-BR`) entries for `catalog/settings/admin.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::settings::admin::{AdminMessage, AdminText, DashboardMessage, MonitoringMessage};

entries! { PtBr:
    AdminMessage => MessageWriter |text| {
        AdminMessage::SiteAdministration => text.literal("Administração do site"),
        AdminMessage::Administration => text.literal("Administração"),
        AdminMessage::Dashboard => text.literal("Painel"),
        AdminMessage::Users => text.literal("Usuários"),
        AdminMessage::Organizations => text.literal("Organizações"),
        AdminMessage::Repositories => text.literal("Repositórios"),
        AdminMessage::Settings => text.literal("Configurações"),
        AdminMessage::AuditLog => text.literal("Log de auditoria"),
        AdminMessage::Configuration => text.literal("Configuração"),
        AdminMessage::Runners => text.literal("Executores"),
        AdminMessage::Search => text.literal("Pesquisar"),
        AdminMessage::Pagination => text.literal("Paginação"),
        AdminMessage::Previous => text.literal("Anterior"),
        AdminMessage::Next => text.literal("Próxima"),
        AdminMessage::NoResults => text.literal("Nenhum resultado"),
        AdminMessage::Cancel => text.literal("Cancelar"),
        AdminMessage::Actions => text.literal("Ações"),
        AdminMessage::Private => text.literal("Privado"),
        AdminMessage::Archived => text.literal("Arquivado"),
        AdminMessage::Delete => text.literal("Excluir"),
        AdminMessage::TypeToConfirmLead => text.literal("Digite"),
        AdminMessage::TypeToConfirmTail => text.literal("para confirmar"),
        AdminMessage::NewPassword => text.literal("Nova senha"),
        AdminMessage::ConfirmPassword => text.literal("Confirmar nova senha"),
        AdminMessage::Unavailable => text.literal("indisponível"),
    }
    DashboardMessage => MessageWriter |text| {
        DashboardMessage::Intro => text.literal("Uma visão geral desta instância."),
        DashboardMessage::RestartServer => text.literal("Reiniciar servidor"),
        DashboardMessage::RestartUnavailableHint => {
            text.literal("A reinicialização não está disponível neste host de servidor");
        }
        DashboardMessage::SoftwareUpdates => text.literal("Atualizações de software"),
        DashboardMessage::ConfigureUpdates => {
            text.literal("Configurar a verificação de atualizações e a privacidade da versão");
        }
        DashboardMessage::LiveMonitoring => text.literal("Monitoramento ao vivo"),
        DashboardMessage::MonitoringIntro => {
            text.literal("Tráfego, tempos de resposta e atividade Git nos últimos cinco minutos.");
        }
        DashboardMessage::Snapshot => text.literal("Instantâneo"),
        DashboardMessage::Pause => text.literal("Pausar"),
        DashboardMessage::NeedsJavaScript => {
            text.literal(
                "As atualizações ao vivo exigem JavaScript. Recarregue esta página para obter um novo instantâneo.",
            );
        }
        DashboardMessage::System => text.literal("Sistema"),
        DashboardMessage::InstanceSettings => text.literal("Configurações da instância"),
        DashboardMessage::Edit => text.literal("Editar"),
        DashboardMessage::Users => text.literal("Usuários"),
        DashboardMessage::Organizations => text.literal("Organizações"),
        DashboardMessage::SharedNamespaces => text.literal("Espaços de nomes compartilhados"),
        DashboardMessage::Repositories => text.literal("Repositórios"),
        DashboardMessage::AccessTokens => text.literal("Tokens de acesso"),
        DashboardMessage::AcrossAllUsers => text.literal("De todos os usuários"),
        DashboardMessage::Uptime => text.literal("Tempo de atividade"),
        DashboardMessage::DataDirectory => text.literal("Diretório de dados"),
        DashboardMessage::DiskUsage => text.literal("Uso de disco"),
        DashboardMessage::SiteName => text.literal("Nome do site"),
        DashboardMessage::Registration => text.literal("Cadastro"),
        DashboardMessage::RegistrationOpen => text.literal("Aberto a todos"),
        DashboardMessage::RegistrationClosed => text.literal("Fechado (administradores criam as contas)"),
        DashboardMessage::OrgPolicy => text.literal("Organizações"),
        DashboardMessage::AnyUserCreates => text.literal("Qualquer usuário pode criar"),
        DashboardMessage::AdminsCreate => text.literal("Somente administradores podem criar"),
        DashboardMessage::NewRepositories => text.literal("Novos repositórios"),
        DashboardMessage::PublicByDefault => text.literal("Públicos por padrão"),
        DashboardMessage::PrivateByDefault => text.literal("Privados por padrão"),
        DashboardMessage::EmailDigests => text.literal("Resumos por e-mail"),
        DashboardMessage::Restarting => text.literal("Reiniciando o servidor"),
        DashboardMessage::RestartRequested => text.literal("Reinicialização solicitada"),
        DashboardMessage::RestartingText => {
            text.literal(
                "O servidor está concluindo as requisições HTTP ativas, parando o SSH e os jobs em segundo plano e \
                 fechando o armazenamento antes de reiniciar. Transferências longas podem demorar para terminar.",
            );
        }
        DashboardMessage::Waiting => text.literal("Aguardando o servidor voltar…"),
        DashboardMessage::ReturnToDashboard => text.literal("Voltar ao painel"),
        DashboardMessage::WaitNoScript => {
            text.literal("Aguarde um momento e volte ao painel para reconectar.");
        }
        DashboardMessage::RestartQuestion => text.literal("Reiniciar esta aplicação rbit?"),
        DashboardMessage::RestartText => {
            text.literal(
                "O site ficará brevemente indisponível enquanto a aplicação é encerrada e iniciada novamente com a \
                 configuração salva. As requisições HTTP podem terminar; as conexões SSH serão fechadas. Os usuários \
                 manterão as sessões salvas.",
            );
        }
        DashboardMessage::RestartAudited => {
            text.literal("Esta ação é registrada no log de auditoria da administração.");
        }
        DashboardMessage::RestartUnavailable => text.literal("A reinicialização não está disponível"),
        DashboardMessage::RestartUnsupported => {
            text.literal(
                "Este host de servidor não permite reiniciar a aplicação. Reinicie o rbit pelo seu gerenciador de \
                 serviços.",
            );
        }
        DashboardMessage::BackToDashboard => text.literal("Voltar ao painel"),
    }
    MonitoringMessage => MessageWriter |text| {
        MonitoringMessage::Requests => text.literal("Requisições na janela"),
        MonitoringMessage::ErrorRate => text.literal("Taxa de erros do servidor"),
        MonitoringMessage::ProcessUptime => text.literal("Tempo de atividade do processo"),
        MonitoringMessage::Samples => text.literal("Amostras de 5 segundos"),
        MonitoringMessage::FiveMinutesAgo => text.literal("5 minutos atrás"),
        MonitoringMessage::LatestSample => text.literal("Amostra mais recente"),
        MonitoringMessage::Collecting => {
            text.literal("Coletando a primeira amostra. Os gráficos começam após cinco segundos.");
        }
        MonitoringMessage::HttpResponses => text.literal("Respostas HTTP"),
        MonitoringMessage::CurrentWindow => text.literal("Janela atual"),
        MonitoringMessage::Status => text.literal("Status"),
        MonitoringMessage::RequestsColumn => text.literal("Requisições"),
        MonitoringMessage::Share => text.literal("Proporção"),
        MonitoringMessage::GitActivity => text.literal("Atividade Git"),
        MonitoringMessage::Operation => text.literal("Operação"),
        MonitoringMessage::Exchanges => text.literal("Trocas"),
        MonitoringMessage::CloneFetch => text.literal("Clone / fetch"),
        MonitoringMessage::Push => text.literal("Push"),
        MonitoringMessage::GitNote => {
            text.literal(
                "Trocas Smart HTTP, incluindo falhas. SSH e anúncios de referências são excluídos. O tempo de resposta \
                 mede o tempo até os cabeçalhos; transferências em streaming podem continuar.",
            );
        }
        MonitoringMessage::Footnote => {
            text.literal(
                "Somente este processo; o histórico é zerado ao reiniciar. As requisições de monitoramento são \
                 excluídas.",
            );
        }
        MonitoringMessage::Informational => text.literal("Informativas"),
        MonitoringMessage::Successful => text.literal("Bem-sucedidas"),
        MonitoringMessage::Redirects => text.literal("Redirecionamentos"),
        MonitoringMessage::ClientErrors => text.literal("Erros do cliente"),
        MonitoringMessage::ServerErrors => text.literal("Erros do servidor"),
        MonitoringMessage::RequestTraffic => text.literal("Tráfego de requisições"),
        MonitoringMessage::ResponseTime => text.literal("Tempo de resposta"),
        MonitoringMessage::RequestsPerSecond => text.literal("req/s"),
        MonitoringMessage::MillisecondsAverage => text.literal("ms em média"),
        MonitoringMessage::WaitingFirst => text.literal("Aguardando a primeira amostra"),
    }
    AdminText<'a> => MessageWriter |text| {
        AdminText::ReleaseNotes(version) => {
            text.literal("Ver as notas da versão ");
            text.argument(version);
            text.literal(" do rbit");
        }
        AdminText::UsersDetail(admins, suspended) => {
            text.argument(admins);
            text.literal(" administradores · ");
            text.argument(suspended);
            text.literal(" suspensos");
        }
        AdminText::UsersDetailOne(suspended) => {
            text.literal("1 administrador · ");
            text.argument(suspended);
            text.literal(" suspensos");
        }
        AdminText::RepositoriesDetail(public, private) => {
            text.argument(public);
            text.literal(" públicos · ");
            text.argument(private);
            text.literal(" privados");
        }
        AdminText::Summary(first, last, total) => {
            text.argument(first);
            text.literal("–");
            text.argument(last);
            text.literal(" de ");
            text.argument(total);
        }
        AdminText::DigestsAt(hour) => {
            text.literal("Enviados às ");
            text.argument(hour);
            text.literal(", salvo escolha dos usuários");
        }
        AdminText::DeleteLabel(name) => {
            text.literal("Excluir ");
            text.argument(name);
        }
        AdminText::ActionsFor(name) => {
            text.literal("Ações para ");
            text.argument(name);
        }
        AdminText::PasswordLength(length) => {
            text.literal("Pelo menos ");
            text.number(length);
            text.literal(" caracteres.");
        }
        AdminText::ChartLabel(title, value, unit) => {
            text.argument(title);
            text.literal(" nos últimos cinco minutos. Mais recente: ");
            text.argument(value);
            text.literal(" ");
            text.argument(unit);
            text.literal(". Períodos sem requisições deixam lacunas no tempo de resposta.");
        }
        AdminText::UpdatedAt(time) => {
            text.literal("Atualizado às ");
            text.argument(time);
            text.literal(" UTC");
        }
        AdminText::Coverage(samples, limit) => {
            text.number(samples);
            text.literal(" de ");
            text.number(limit);
            text.literal(" amostras");
        }
        AdminText::InAdministration(page) => {
            text.argument(page);
            text.literal(" · Administração");
        }
        AdminText::SetPasswordFor(name) => {
            text.literal("Definir senha de ");
            text.argument(name);
            text.literal(" · Administração");
        }
        AdminText::DeletePageTitle(name) => {
            text.literal("Excluir ");
            text.argument(name);
            text.literal(" · Administração");
        }
    }
}
