//! Korean (`ko`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};

entries! { Ko:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("커밋 활동"),
        ContributionLabel::Loading => text.literal("커밋 활동을 불러오는 중…"),
        ContributionLabel::ViewCommitActivity => text.literal("커밋 활동 보기"),
        ContributionLabel::Year => text.literal("연도"),
        ContributionLabel::View => text.literal("보기"),
        ContributionLabel::AnnualCalendar => text.literal("연간 커밋 달력"),
        ContributionLabel::QuarterlyCalendar => text.literal("분기별 커밋 달력"),
        ContributionLabel::Monday => text.literal("월"),
        ContributionLabel::Wednesday => text.literal("수"),
        ContributionLabel::Friday => text.literal("금"),
        ContributionLabel::DatesUseUtc => text.literal("날짜는 UTC 기준입니다."),
        ContributionLabel::IntensityLegend => text.literal("커밋 강도(적음에서 많음 순)"),
        ContributionLabel::Less => text.literal("적음"),
        ContributionLabel::More => text.literal("많음"),
        ContributionLabel::Partial => text.literal("일부 활동만 표시됩니다. 하나 이상의 저장소가 커밋 기록 검사 한도(100,000개)를 넘었습니다."),
        ContributionLabel::NoCommits => text.literal("이 연도에 일치하는 커밋이 없습니다."),
        ContributionLabel::SelectDate => text.literal("날짜를 선택하면 해당 날짜의 커밋 활동을 볼 수 있습니다."),
        ContributionLabel::Commits => text.literal("커밋"),
        ContributionLabel::ActiveDays => text.literal("활동한 날"),
        ContributionLabel::LongestStreak => text.literal("최장 연속 기록"),
        ContributionLabel::Repositories => text.literal("저장소"),
        ContributionLabel::HowCounted => text.literal("월별 활동과 커밋 집계 방식"),
        ContributionLabel::HowCountedText => text.literal("설정된 기본 브랜치에서 도달할 수 있는 커밋을 작성 날짜 기준으로 집계합니다. 여러 저장소에 있는 동일한 커밋은 한 번만 집계합니다. 접근할 수 있는 저장소만 포함됩니다. 이슈, 풀 리퀘스트, 리뷰는 포함되지 않습니다."),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year, .. } => {
            text.argument(&year.to_string());
            text.literal("년 커밋 ");
            text.number(count as u64);
            text.literal("개");
        }
        ContributionMessage::ActivityInYear { year, .. } => {
            text.argument(&year.to_string());
            text.literal("년 커밋 활동");
        }
        ContributionMessage::Description { count, days, .. } => {
            text.literal("활동한 ");
            text.number(days as u64);
            text.literal("일 동안 커밋 ");
            text.number(count as u64);
            text.literal("개. 강도가 높을수록 커밋이 많습니다. 날짜에 포커스를 두거나 마우스를 올리면 개수가 표시됩니다. 날짜를 선택하면 자세한 내용을 볼 수 있습니다.");
        }
        ContributionMessage::Day { count, date, .. } => {
            text.argument(date);
            text.literal(": 커밋 ");
            text.number(count as u64);
            text.literal("개");
        }
        ContributionMessage::QuarterActivity { quarter, year, .. } => {
            text.argument(&year.to_string());
            text.literal("년 ");
            text.argument(quarter);
            text.literal(" 커밋 활동");
        }
        ContributionMessage::Days { count, .. } => {
            text.number(count as u64);
            text.literal("일");
        }
    }
}
