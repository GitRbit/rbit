//! Brazilian Portuguese (`pt-BR`) entries for `catalog/shell/home.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::shell::home::{Feature, HomeMessage, Involvement};
use crate::plural::Plural;

entries! { PtBr:
    HomeMessage<'a> => MessageWriter |text| {
        HomeMessage::SearchPublic => text.literal("Pesquisar repositórios públicos"),
        HomeMessage::Search => text.literal("Pesquisar"),
        HomeMessage::ExploreAll => text.literal("Explorar todos os repositórios"),
        HomeMessage::ThisInstance => text.literal("Esta instância"),
        HomeMessage::RepositoryCount(count) => {
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "Repositório",
                _ => "Repositórios",
            });
        }
        HomeMessage::UserCount(count) => {
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "Usuário",
                _ => "Usuários",
            });
        }
        HomeMessage::OrganizationCount(count) => {
            text.literal(match text.plural(count.unsigned_abs()) {
                Plural::One => "Organização",
                _ => "Organizações",
            });
        }
        HomeMessage::RecentlyUpdated => text.literal("Atualizados recentemente"),
        HomeMessage::ViewAll => text.literal("Ver todos"),
        HomeMessage::NoPublicRepositories => text.literal("Ainda não há repositórios públicos"),
        HomeMessage::NoPublicRepositoriesText => {
            text.literal("Os repositórios tornados públicos neste servidor aparecerão aqui.");
        }
        HomeMessage::FreshServer => {
            text.literal("Este servidor ainda não tem contas. A primeira conta criada se torna a administradora.");
        }
        HomeMessage::BuiltForSelfHosting => text.literal("Feito para auto-hospedagem"),
        HomeMessage::FeatureTitle(feature) => text.literal(match feature {
            Feature::Binary => "Um único binário rápido",
            Feature::Protocols => "Git por HTTPS e SSH",
            Feature::Teams => "Organizações e equipes",
            Feature::Releases => "Releases, espelhos e webhooks",
            Feature::Security => "Seguro por padrão",
            Feature::Screens => "Em qualquer tela",
        }),
        HomeMessage::FeatureText(feature) => text.literal(match feature {
            Feature::Binary => "SQLite e um armazenamento embutido. Nenhum outro serviço para instalar ou executar.",
            Feature::Protocols => "Smart HTTP com protocolo v2, servidor SSH embutido e chaves de implantação.",
            Feature::Teams => "Compartilhe repositórios com equipes e colaboradores, papel por papel.",
            Feature::Releases => "Publique releases, espelhe projetos upstream e notifique outros serviços.",
            Feature::Security => "Entrada em dois fatores, tokens de acesso com escopo e log de auditoria.",
            Feature::Screens => "Responsivo até o tamanho de um celular e instalável como aplicativo.",
        }),
        HomeMessage::Dashboard => text.literal("Painel"),
        HomeMessage::ViewProfile => text.literal("Ver perfil"),
        HomeMessage::New => text.literal("Novo"),
        HomeMessage::NewRepository => text.literal("Novo repositório"),
        HomeMessage::PersonalDashboards => text.literal("Painéis pessoais"),
        HomeMessage::Issues => text.literal("Issues"),
        HomeMessage::PullRequests => text.literal("Pull requests"),
        HomeMessage::Repositories => text.literal("Repositórios"),
        HomeMessage::PrivateCount(count) => {
            let count = u64::try_from(count).unwrap_or(u64::MAX);
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " privado",
                _ => " privados",
            });
        }
        HomeMessage::FindRepository => text.literal("Encontrar um repositório…"),
        HomeMessage::FilterRepositories => text.literal("Filtrar seus repositórios"),
        HomeMessage::NoRepositories => text.literal("Ainda não há repositórios"),
        HomeMessage::NoRepositoriesText => {
            text.literal("Crie um repositório para começar a hospedar código neste servidor.");
        }
        HomeMessage::CreateRepository => text.literal("Criar repositório"),
        HomeMessage::NoFilterMatch => text.literal("Nenhum repositório corresponde ao seu filtro."),
        HomeMessage::Organizations => text.literal("Organizações"),
        HomeMessage::NewOrganization => text.literal("Nova organização"),
        HomeMessage::NotAMember => text.literal("Você não é membro de nenhuma organização."),
        HomeMessage::CreateOne => text.literal("Crie uma"),
        HomeMessage::ShareWithTeam => text.literal("para compartilhar repositórios com uma equipe."),
        HomeMessage::Explore => text.literal("Explorar"),
        HomeMessage::NothingPublic => text.literal("Ainda não há nada público."),
        HomeMessage::YourInvolvement => text.literal("Sua participação"),
        HomeMessage::InvolvementTab(tab) => text.literal(match tab {
            Involvement::Created => "Criados por mim",
            Involvement::Assigned => "Atribuídos",
            Involvement::Mentioned => "Menções",
            Involvement::ReviewRequested => "Revisão solicitada",
        }),
        HomeMessage::SearchIn(title) => {
            text.literal("Pesquisar ");
            text.argument(title);
        }
        HomeMessage::SearchHelp => text.literal("Ajuda da pesquisa"),
        HomeMessage::SearchHelpText => {
            text.literal(
                "Use is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee ou \
                 sort:updated-desc. O texto pesquisa títulos e descrições. Os resultados incluem apenas repositórios \
                 que você pode ler no momento. A aba Menções inclui as menções conhecidas em descrições, comentários e \
                 revisões enviadas, mesmo depois que as notificações forem lidas.",
            );
        }
        HomeMessage::Results => text.literal("Resultados"),
        HomeMessage::OpenCount(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " aberto",
                _ => " abertos",
            });
        }
        HomeMessage::ClosedCount(count) => {
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " fechado",
                _ => " fechados",
            });
        }
        HomeMessage::NoMatchingThreads => text.literal("Nenhuma conversa corresponde"),
        HomeMessage::NoMatchingThreadsText => text.literal("Tente outro filtro ou outra aba de participação."),
        HomeMessage::CommentCount(count) => {
            let count = u64::from(count);
            text.number(count);
            text.literal(match text.plural(count) {
                Plural::One => " comentário",
                _ => " comentários",
            });
        }
    }
}
