//! Masking (moved from `rbit-services`), append acceptance, line cleaning and the budget.
use super::*;

#[test]
fn secrets_and_their_lines_are_masked() {
    let masker = Masker::new(["hunter2-secret", "line-one\nline-two\nab", "abc"]);
    assert_eq!(masker.mask("password=hunter2-secret!"), "password=***!");
    assert_eq!(masker.mask("got line-two and ab"), "got *** and ab", "short lines stay");
    assert_eq!(masker.mask("abc is too short to mask"), "abc is too short to mask");
    assert_eq!(masker.mask("line-one\nline-two\nab"), "***", "the whole value at once");
    assert_eq!(masker.mask("line-one\nline-two"), "***\n***");
    assert_eq!(Masker::none().mask("hunter2-secret"), "hunter2-secret");
    assert_eq!(Masker::new(["x"]).mask("x"), "x");
}

#[test]
fn bodies_split_into_lines_within_the_call_limit() {
    assert_eq!(split_body(""), Ok(Vec::new()));
    assert_eq!(split_body("a\nb\n"), Ok(vec!["a", "b"]));
    assert_eq!(split_body("a\n\nb"), Ok(vec!["a", "", "b"]));
    assert_eq!(split_body("\n"), Ok(vec![""]));
    assert!(split_body(&"x\n".repeat(MAX_LINES_PER_CALL)).is_ok());
    assert_eq!(split_body(&"x\n".repeat(MAX_LINES_PER_CALL + 1)), Err("send at most 10000 lines per call".to_owned()));
}

#[test]
fn resends_overlap_harmlessly_and_gaps_name_the_line() {
    assert_eq!(accept_append(5, 3, 2), Append::Gap { current: 3 });
    assert_eq!(accept_append(0, 3, 3), Append::Nothing { current: 3 });
    assert_eq!(accept_append(1, 3, 1), Append::Nothing { current: 3 });
    assert_eq!(accept_append(1, 3, 4), Append::Store { skip: 2 });
    assert_eq!(accept_append(3, 3, 1), Append::Store { skip: 0 });
    assert_eq!(accept_append(3, 3, 0), Append::Nothing { current: 3 });
}

#[test]
fn lines_are_masked_unterminated_and_cut_on_a_character_boundary() {
    let masker = Masker::new(["hunter2-secret"]);
    assert_eq!(clean_line(&masker, "pw hunter2-secret\r", 100), "pw ***");
    let long = format!("{}é", "x".repeat(30));
    let cut = clean_line(&Masker::none(), &long, 30);
    assert!(cut.ends_with(" [line truncated]") && cut.len() <= 30, "{cut}");
    assert_eq!(clean_line(&Masker::none(), &"x".repeat(30), 30), "x".repeat(30), "a line at the limit stays");
}

#[test]
fn the_budget_keeps_counting_after_one_marker() {
    let start = LogProgress { lines: 2, bytes: 4, truncated: false };
    let lines: Vec<String> = ["abc", "defgh", "ij"].iter().map(|line| (*line).to_owned()).collect();
    let laid = layout(start, &lines, 12);
    let marker = "rbit: the log was cut here: a task may write at most 12 bytes\n";
    assert_eq!(laid.text, [b"abc\n".as_slice(), marker.as_bytes()].concat());
    assert_eq!(laid.ends, 8u64.to_le_bytes());
    let bytes = 8 + u64::try_from(marker.len()).unwrap();
    assert_eq!(laid.progress, LogProgress { lines: 5, bytes, truncated: true });
    let after = layout(laid.progress, &lines, 12);
    assert_eq!((after.text.len(), after.ends.len(), after.progress.lines), (0, 0, 8), "dropped lines still count");
}

#[test]
fn task_tokens_are_masked_by_their_shape_without_being_known() {
    let token = format!("rbat_{}", "aZ0_-".repeat(9).chars().take(43).collect::<String>());
    let masker = Masker::new([]);
    assert_eq!(masker.mask(&format!("git push https://x:{token}@host/r")), "git push https://x:***@host/r");
    assert_eq!(masker.mask(&format!("{token}{token}")), "******");
    assert_eq!(Masker::none().mask("rbat_short and rbat_"), "rbat_short and rbat_", "not a token's shape");
}

#[test]
fn cleaning_a_long_token_shaped_line_does_not_rescan_every_suffix() {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // A runner can upload a line up to its request-body limit before stored
        // line truncation. Every byte here is base64url and prefixes overlap.
        let source = "rbat_".repeat((1 << 20) / 5);
        let cleaned = clean_line(&Masker::none(), &source, 64 * 1024);
        done.send(cleaned).unwrap();
    });
    let cleaned =
        finished.recv_timeout(std::time::Duration::from_secs(5)).expect("token masking must inspect a fixed-size suffix per match");
    assert!(cleaned.len() <= 64 * 1024);
    assert!(cleaned.contains("***"));
}
