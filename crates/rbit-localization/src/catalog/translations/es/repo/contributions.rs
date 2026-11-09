//! Spanish (`es`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};
use crate::plural::Plural;

entries! { Es:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("Actividad de commits"),
        ContributionLabel::Loading => text.literal("Cargando la actividad de commits…"),
        ContributionLabel::ViewCommitActivity => text.literal("Ver la actividad de commits"),
        ContributionLabel::Year => text.literal("Año"),
        ContributionLabel::View => text.literal("Ver"),
        ContributionLabel::AnnualCalendar => text.literal("Calendario anual de commits"),
        ContributionLabel::QuarterlyCalendar => text.literal("Calendario de commits por trimestre"),
        ContributionLabel::Monday => text.literal("lun"),
        ContributionLabel::Wednesday => text.literal("mié"),
        ContributionLabel::Friday => text.literal("vie"),
        ContributionLabel::DatesUseUtc => text.literal("Las fechas están en UTC."),
        ContributionLabel::IntensityLegend => text.literal("Intensidad de commits, de menos a más"),
        ContributionLabel::Less => text.literal("Menos"),
        ContributionLabel::More => text.literal("Más"),
        ContributionLabel::Partial => text.literal(
            "Actividad parcial: al menos un repositorio superó el límite de análisis del historial de 100,000 \
             commits.",
        ),
        ContributionLabel::NoCommits => text.literal("No hay commits coincidentes en este año."),
        ContributionLabel::SelectDate => text.literal("Selecciona una fecha para ver su actividad de commits."),
        ContributionLabel::Commits => text.literal("Commits"),
        ContributionLabel::ActiveDays => text.literal("Días activos"),
        ContributionLabel::LongestStreak => text.literal("Racha más larga"),
        ContributionLabel::Repositories => text.literal("Repositorios"),
        ContributionLabel::HowCounted => text.literal("Actividad mensual y cómo se cuentan los commits"),
        ContributionLabel::HowCountedText => text.literal(
            "Se cuentan los commits alcanzables desde la rama predeterminada configurada, según su fecha de autoría. \
             Los commits idénticos compartidos por varios repositorios cuentan una vez. Solo se incluyen los \
             repositorios a los que tienes acceso. No se incluyen incidencias, solicitudes de cambios ni revisiones.",
        ),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            commits(text, count);
            text.literal(" en ");
            text.argument(&year.to_string());
        }
        ContributionMessage::ActivityInYear { year } => {
            text.literal("Actividad de commits en ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Description { count, days } => {
            commits(text, count);
            text.literal(" en ");
            text.number(days as u64);
            text.literal(match text.plural(days as u64) {
                Plural::One => " día activo",
                _ => " días activos",
            });
            text.literal(
                ". Una mayor intensidad indica más commits. Enfoca un día o pasa el puntero por encima para ver su \
                 recuento. Selecciona un día para ver los detalles.",
            );
        }
        ContributionMessage::Day { count, date } => {
            commits(text, count);
            text.literal(" el ");
            text.argument(date);
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.literal("Actividad de commits del ");
            text.argument(quarter);
            text.literal(" ");
            text.argument(&year.to_string());
        }
        ContributionMessage::Days { count } => {
            text.number(count as u64);
            text.literal(match text.plural(count as u64) {
                Plural::One => " día",
                _ => " días",
            });
        }
    }
}

/// "1 commit", "3 commits".
fn commits(text: &mut MessageWriter, count: usize) {
    text.number(count as u64);
    text.literal(match text.plural(count as u64) {
        Plural::One => " commit",
        _ => " commits",
    });
}
