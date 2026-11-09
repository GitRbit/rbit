//! Korean (`ko`) entries for `catalog/wiki.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::wiki::WikiMessage;

entries! { Ko:
    WikiMessage<'a> => MessageWriter |text| {
        WikiMessage::Tab => text.literal("위키"),
        WikiMessage::Pages => text.literal("페이지"),
        WikiMessage::AllPages => text.literal("모든 페이지"),
        WikiMessage::PagesLabel => text.literal("위키 페이지"),
        WikiMessage::BreadcrumbLabel => text.literal("이동 경로"),
        WikiMessage::SearchPages => text.literal("페이지 검색"),
        WikiMessage::Search => text.literal("검색"),
        WikiMessage::NewPage => text.literal("새 페이지"),
        WikiMessage::EditPage => text.literal("페이지 편집"),
        WikiMessage::Edit => text.literal("편집"),
        WikiMessage::History => text.literal("기록"),
        WikiMessage::Delete => text.literal("삭제"),
        WikiMessage::LastEdited => text.literal("마지막 편집"),
        WikiMessage::Revision { generation, .. } => {
            text.literal("리비전 ");
            text.number(u64::try_from(generation).unwrap_or_default());
        }
        WikiMessage::RevisionColumn => text.literal("리비전"),
        WikiMessage::Author => text.literal("작성자"),
        WikiMessage::Date => text.literal("날짜"),
        WikiMessage::UnknownAuthor => text.literal("알 수 없는 작성자"),
        WikiMessage::Created => text.literal("생성됨"),
        WikiMessage::Edited => text.literal("편집됨"),
        WikiMessage::Renamed => text.literal("이름 변경됨"),
        WikiMessage::Deleted => text.literal("삭제됨"),
        WikiMessage::Restored => text.literal("복원됨"),
        WikiMessage::View => text.literal("보기"),
        WikiMessage::CompareWithPrevious => text.literal("이전 리비전과 비교"),
        WikiMessage::Restore => text.literal("이 리비전 복원"),
        WikiMessage::OlderRevisions => text.literal("이전 리비전"),
        WikiMessage::OldRevisionNotice => text.literal("이 페이지의 이전 리비전을 보고 있습니다."),
        WikiMessage::ViewCurrent => text.literal("현재 페이지 보기"),
        WikiMessage::HistoryOf { title, .. } => {
            text.argument(title);
            text.literal(" 기록");
        }
        WikiMessage::Comparing { base, head, .. } => {
            text.literal("리비전 ");
            text.number(u64::try_from(base).unwrap_or_default());
            text.literal("에서 리비전 ");
            text.number(u64::try_from(head).unwrap_or_default());
            text.literal("까지의 변경 사항");
        }
        WikiMessage::Changes { additions, deletions, .. } => {
            text.literal("추가 ");
            text.number(additions);
            text.literal("개, 삭제 ");
            text.number(deletions);
            text.literal("개");
        }
        WikiMessage::NoDifferences => text.literal("두 리비전의 내용이 같습니다."),
        WikiMessage::Title => text.literal("제목"),
        WikiMessage::Path => text.literal("경로"),
        WikiMessage::PathHint => text.literal("영문자, 숫자, - _ .을 /로 구분해 입력합니다. 비워 두면 제목에서 경로를 만듭니다."),
        WikiMessage::Content => text.literal("내용"),
        WikiMessage::Markdown => text.literal("Markdown"),
        WikiMessage::Save => text.literal("페이지 저장"),
        WikiMessage::Preview => text.literal("미리 보기"),
        WikiMessage::NothingToPreview => text.literal("미리 볼 내용이 없습니다."),
        WikiMessage::Cancel => text.literal("취소"),
        WikiMessage::TitleEmpty => text.literal("제목을 입력하세요."),
        WikiMessage::TitleTooLong => text.literal("제목이 너무 깁니다."),
        WikiMessage::TitleInvalid => text.literal("제목에 허용되지 않는 문자가 있습니다."),
        WikiMessage::PathEmpty => text.literal("경로를 입력하거나, 경로를 만들 수 있는 제목을 입력하세요."),
        WikiMessage::PathTooLong => text.literal("경로 또는 경로의 한 부분이 너무 깁니다."),
        WikiMessage::PathTooDeep => text.literal("경로의 단계가 너무 많습니다."),
        WikiMessage::PathInvalid => text.literal("경로의 각 부분은 영문자나 숫자로 시작해야 하며 영문자, 숫자, - _ .만 사용할 수 있습니다."),
        WikiMessage::PathReserved => text.literal("경로에 예약된 이름이 사용되었습니다."),
        WikiMessage::ContentTooLarge => text.literal("내용이 너무 큽니다."),
        WikiMessage::ContentInvalid => text.literal("내용에 허용되지 않는 문자가 있습니다."),
        WikiMessage::Conflict => text.literal("편집을 시작한 후 이 페이지가 변경되었습니다. 작성한 텍스트는 아래에 보존되어 있으니 다시 저장하기 전에 현재 페이지를 검토하세요."),
        WikiMessage::CompareChanges => text.literal("변경 사항 비교"),
        WikiMessage::PathTaken => text.literal("이 경로는 이미 다른 페이지에서 사용 중입니다."),
        WikiMessage::Unchanged => text.literal("변경된 내용이 없습니다."),
        WikiMessage::DeleteTitle { title, .. } => {
            text.argument(title);
            text.literal(" 페이지를 삭제할까요?");
        }
        WikiMessage::DeleteWarning => text.literal("페이지와 그 기록이 위키에서 제거됩니다. 여기에서는 되돌릴 수 없습니다."),
        WikiMessage::DeletePage => text.literal("페이지 삭제"),
        WikiMessage::Disabled => text.literal("이 저장소의 위키가 꺼져 있습니다."),
        WikiMessage::About => text.literal("위키는 코드와 별도로 자체 기록을 가진 문서 페이지를 보관합니다."),
        WikiMessage::TurnOn => text.literal("위키 켜기"),
        WikiMessage::TurnOff => text.literal("위키 끄기"),
        WikiMessage::Empty => text.literal("이 위키에는 아직 페이지가 없습니다."),
        WikiMessage::CreateFirst => text.literal("첫 페이지 만들기"),
        WikiMessage::Missing => text.literal("이 경로에 해당하는 페이지가 없습니다."),
        WikiMessage::CreatePath { path, .. } => {
            text.argument(path);
            text.literal(" 만들기");
        }
        WikiMessage::SearchCount { count, query, .. } => {
            text.literal("“");
            text.argument(query);
            text.literal("”와(과) 일치하는 페이지 ");
            text.number(count);
            text.literal("개");
        }
        WikiMessage::NoResults { query, .. } => {
            text.literal("“");
            text.argument(query);
            text.literal("”와(과) 일치하는 페이지가 없습니다.");
        }
        WikiMessage::MorePages => text.literal("페이지 더 보기"),
    }
}
