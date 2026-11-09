//! Korean (`ko`) entries for `catalog/repo/releases.rs`.

use crate::catalog::fallback::MessageWriter;
use crate::catalog::repo::releases::{ReleaseLabel, ReleaseMessage};

entries! { Ko:
    ReleaseLabel => MessageWriter |text| {
        ReleaseLabel::Releases => text.literal("릴리스"),
        ReleaseLabel::NewRelease => text.literal("새 릴리스"),
        ReleaseLabel::EditRelease => text.literal("릴리스 편집"),
        ReleaseLabel::NoReleasesYet => text.literal("아직 릴리스가 없습니다"),
        ReleaseLabel::EmptyForWriters => text.literal("릴리스는 태그를 노트 및 파일과 함께 묶습니다. 태그를 푸시한 다음 해당 태그로 릴리스를 만드세요."),
        ReleaseLabel::EmptyForReaders => text.literal("릴리스는 이 프로젝트의 버전을 노트 및 다운로드와 함께 표시합니다."),
        ReleaseLabel::Draft => text.literal("초안"),
        ReleaseLabel::PreRelease => text.literal("사전 릴리스"),
        ReleaseLabel::Latest => text.literal("최신"),
        ReleaseLabel::Bot => text.literal("봇"),
        ReleaseLabel::Created => text.literal("생성됨"),
        ReleaseLabel::Edit => text.literal("편집"),
        ReleaseLabel::Downloads => text.literal("다운로드"),
        ReleaseLabel::SourceZip => text.literal("소스 코드(zip)"),
        ReleaseLabel::SourceTarGz => text.literal("소스 코드(tar.gz)"),
        ReleaseLabel::Files => text.literal("파일"),
        ReleaseLabel::Remove => text.literal("제거"),
        ReleaseLabel::FileToAttach => text.literal("첨부할 파일"),
        ReleaseLabel::Upload => text.literal("업로드"),
        ReleaseLabel::DeleteTitle => text.literal("이 릴리스 삭제"),
        ReleaseLabel::DeleteHintStart => text.literal("릴리스와 노트, 파일이 삭제됩니다. 태그"),
        ReleaseLabel::DeleteHintEnd => text.literal("자체는 유지됩니다."),
        ReleaseLabel::DeleteRelease => text.literal("릴리스 삭제"),
        ReleaseLabel::FormLead => text.literal("릴리스는 태그에 대한 설명으로, 노트와 다운로드할 파일을 포함합니다."),
        ReleaseLabel::Tag => text.literal("태그"),
        ReleaseLabel::AnExisting => text.literal("기존 "),
        ReleaseLabel::TagLink => text.literal("태그"),
        ReleaseLabel::OrNewTag => text.literal(" 또는 아래 브랜치에서 새로 만든 태그입니다."),
        ReleaseLabel::CreateTagFrom => text.literal("새 태그를 만들 기준"),
        ReleaseLabel::TargetHint => text.literal("태그가 없을 때만 사용됨"),
        ReleaseLabel::OneOfTags => text.literal("저장소에 있는 "),
        ReleaseLabel::TagsEnd => text.literal("."),
        ReleaseLabel::TagsLink => text.literal("태그 중 하나"),
        ReleaseLabel::Title => text.literal("제목"),
        ReleaseLabel::TitleHint => text.literal("선택 사항, 기본값은 태그"),
        ReleaseLabel::Notes => text.literal("노트"),
        ReleaseLabel::IsPreRelease => text.literal("사전 릴리스입니다"),
        ReleaseLabel::PreReleaseHint => text.literal("프로덕션에 사용할 준비가 되지 않은 것으로 표시되며, 최신 릴리스로는 정식 릴리스가 우선합니다."),
        ReleaseLabel::SaveRelease => text.literal("릴리스 저장"),
        ReleaseLabel::PublishRelease => text.literal("릴리스 게시"),
        ReleaseLabel::SaveDraft => text.literal("초안 저장"),
        ReleaseLabel::Cancel => text.literal("취소"),
    }
    ReleaseMessage<'a> => MessageWriter |text| {
        ReleaseMessage::Checksum { sha256, .. } => {
            text.literal("SHA-256 ");
            text.argument(sha256);
        }
        ReleaseMessage::Downloads { count, .. } => {
            text.literal("다운로드 ");
            text.number(count);
            text.literal("회");
        }
        ReleaseMessage::RemoveFile { name, .. } => {
            text.argument(name);
            text.literal(" 제거");
        }
        ReleaseMessage::UploadLimit { size, .. } => {
            text.literal("파일당 최대 ");
            text.argument(size);
            text.literal("입니다. 다운로드는 항상 파일로 제공되며 화면에 표시되지 않습니다.");
        }
        ReleaseMessage::Uploaded { name, .. } => {
            text.argument(name);
            text.literal(" 업로드 완료.");
        }
        ReleaseMessage::Removed { name, .. } => {
            text.argument(name);
            text.literal(" 파일을 제거했습니다.");
        }
        ReleaseMessage::EditTitle { tag, .. } => {
            text.argument(tag);
            text.literal(" 편집");
        }
    }
}
