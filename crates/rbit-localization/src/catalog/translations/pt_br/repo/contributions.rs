//! Brazilian Portuguese (`pt-BR`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};
use crate::plural::Plural;

entries! { PtBr:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("Atividade de commits"),
        ContributionLabel::Loading => text.literal("Carregando a atividade de commits…"),
        ContributionLabel::ViewCommitActivity => text.literal("Ver atividade de commits"),
        ContributionLabel::Year => text.literal("Ano"),
        ContributionLabel::View => text.literal("Ver"),
        ContributionLabel::AnnualCalendar => text.literal("Calendário anual de commits"),
        ContributionLabel::QuarterlyCalendar => text.literal("Calendário de commits por trimestre"),
        ContributionLabel::Monday => text.literal("seg"),
        ContributionLabel::Wednesday => text.literal("qua"),
        ContributionLabel::Friday => text.literal("sex"),
        ContributionLabel::DatesUseUtc => text.literal("As datas usam UTC."),
        ContributionLabel::IntensityLegend => text.literal("Intensidade de commits, de menos para mais"),
        ContributionLabel::Less => text.literal("Menos"),
        ContributionLabel::More => text.literal("Mais"),
        ContributionLabel::Partial => {
            text.literal(
                "Atividade parcial: pelo menos um repositório excedeu o limite de varredura de histórico de 100.000 \
                 commits.",
            );
        }
        ContributionLabel::NoCommits => text.literal("Nenhum commit correspondente neste ano."),
        ContributionLabel::SelectDate => text.literal("Selecione uma data para ver a atividade de commits dela."),
        ContributionLabel::Commits => text.literal("Commits"),
        ContributionLabel::ActiveDays => text.literal("Dias ativos"),
        ContributionLabel::LongestStreak => text.literal("Maior sequência"),
        ContributionLabel::Repositories => text.literal("Repositórios"),
        ContributionLabel::HowCounted => text.literal("Atividade mensal e como os commits são contados"),
        ContributionLabel::HowCountedText => {
            text.literal(
                "As contagens incluem os commits alcançáveis a partir do branch padrão configurado, atribuídos à data \
                 do autor. Commits idênticos compartilhados entre repositórios contam uma vez. Somente repositórios \
                 que você pode acessar são incluídos. Issues, pull requests e revisões não são incluídos.",
            );
        }
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            commits(text, count);
            text.literal(" em ");
            text.argument(&year.to_string());
        }
        ContributionMessage::ActivityInYear { year } => {
            text.literal("Atividade de commits em ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Description { count, days } => {
            commits(text, count);
            text.literal(" em ");
            text.number(days as u64);
            text.literal(match text.plural(days as u64) {
                Plural::One => " dia ativo",
                _ => " dias ativos",
            });
            text.literal(
                ". Maior intensidade indica mais commits. Foque ou passe o mouse sobre um dia para ver a contagem. \
                 Selecione um dia para ver os detalhes.",
            );
        }
        ContributionMessage::Day { count, date } => {
            commits(text, count);
            text.literal(" em ");
            text.argument(date);
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.literal("Atividade de commits em ");
            text.argument(quarter);
            text.literal(" ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Days { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " dia",
                _ => " dias",
            });
        }
    }
}

/// “1 commit”, “3 commits”.
fn commits(text: &mut MessageWriter, count: usize) {
    text.number(count as u64);
    text.literal(match text.plural(count as u64) {
        Plural::One => " commit",
        _ => " commits",
    });
}
