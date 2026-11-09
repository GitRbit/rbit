//! Japanese (`ja`) entries for `catalog/repo/contributions.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::contributions::{ContributionLabel, ContributionMessage};

entries! { Ja:
    ContributionLabel => MessageWriter |text| {
        ContributionLabel::CommitActivity => text.literal("コミットアクティビティ"),
        ContributionLabel::Loading => text.literal("コミットアクティビティを読み込んでいます…"),
        ContributionLabel::ViewCommitActivity => text.literal("コミットアクティビティを表示"),
        ContributionLabel::Year => text.literal("年"),
        ContributionLabel::View => text.literal("表示"),
        ContributionLabel::AnnualCalendar => text.literal("年間コミットカレンダー"),
        ContributionLabel::QuarterlyCalendar => text.literal("四半期ごとのコミットカレンダー"),
        ContributionLabel::Monday => text.literal("月"),
        ContributionLabel::Wednesday => text.literal("水"),
        ContributionLabel::Friday => text.literal("金"),
        ContributionLabel::DatesUseUtc => text.literal("日付は UTC で表示されます。"),
        ContributionLabel::IntensityLegend => text.literal("コミット量（少ない順から多い順）"),
        ContributionLabel::Less => text.literal("少"),
        ContributionLabel::More => text.literal("多"),
        ContributionLabel::Partial => text.literal("一部のアクティビティのみ表示しています：少なくとも 1 つのリポジトリが履歴スキャンの上限（100,000 コミット）を超えました。"),
        ContributionLabel::NoCommits => text.literal("この年に一致するコミットはありません。"),
        ContributionLabel::SelectDate => text.literal("日付を選択すると、その日のコミットアクティビティを表示します。"),
        ContributionLabel::Commits => text.literal("コミット"),
        ContributionLabel::ActiveDays => text.literal("活動日数"),
        ContributionLabel::LongestStreak => text.literal("最長連続日数"),
        ContributionLabel::Repositories => text.literal("リポジトリ"),
        ContributionLabel::HowCounted => text.literal("月別アクティビティとコミットの集計方法"),
        ContributionLabel::HowCountedText => text.literal("設定されたデフォルトブランチから到達可能なコミットを、Author の日付で集計しています。複数のリポジトリで共有される同一のコミットは 1 回だけ数えます。アクセスできるリポジトリのみが対象です。Issue、プルリクエスト、レビューは含まれません。"),
    }
    ContributionMessage<'a> => MessageWriter |text| {
        ContributionMessage::CommitsInYear { count, year } => {
            text.argument(&year.to_string());
            text.literal(" 年のコミット ");
            text.number(count as u64);
            text.literal(" 件");
        }
        ContributionMessage::ActivityInYear { year } => {
            text.argument(&year.to_string());
            text.literal(" 年のコミットアクティビティ");
        }
        ContributionMessage::Description { count, days } => {
            text.number(days as u64);
            text.literal(" 日の活動日に ");
            text.number(count as u64);
            text.literal(" 件のコミットがあります。色が濃いほどコミットが多いことを示します。日付にフォーカスするかポインターを合わせると件数を表示し、選択すると詳細を表示します。");
        }
        ContributionMessage::Day { count, date } => {
            text.argument(date);
            text.literal(" のコミット ");
            text.number(count as u64);
            text.literal(" 件");
        }
        ContributionMessage::QuarterActivity { quarter, year } => {
            text.argument(&year.to_string());
            text.literal(" 年 ");
            text.argument(quarter);
            text.literal(" のコミットアクティビティ");
        }
        ContributionMessage::Days { count } => {
            text.number(count as u64);
            text.literal(" 日");
        }
    }
}
