use super::*;

const SCOPE: &str = "/alice/demo";

fn page(scope: &str) -> String {
    format!(
        "<!doctype html><html><head>{TITLE_START}<title>Issues · alice/demo</title>{TITLE_END}</head><body>\
         <header><nav>top bar</nav>{SUBNAV_START}\n<div id=\"repo-subnav\" data-repo-scope=\"{scope}\" class=\"x\"><a href=\"{scope}/issues\">Issues</a></div>\n{SUBNAV_END}</header>\
         {MAIN_START}<main id=\"main\"><p>list</p></main>{MAIN_END}<footer>footer</footer></body></html>"
    )
}

#[test]
fn same_repository_pages_are_cut_to_title_main_and_tabs() {
    let partial = String::from_utf8(slice(page(SCOPE).as_bytes(), SCOPE.as_bytes()).unwrap()).unwrap();
    assert_eq!(
        partial,
        "<title>Issues · alice/demo</title>\n<main id=\"main\"><p>list</p></main>\n\
         <div id=\"repo-subnav\" hx-swap-oob=\"true\" data-repo-scope=\"/alice/demo\" class=\"x\"><a href=\"/alice/demo/issues\">Issues</a></div>\n"
    );
    assert!(!partial.contains("top bar") && !partial.contains("footer"));
}

#[test]
fn other_repositories_get_the_full_page() {
    let full = page(SCOPE);
    for scope in ["/alice/demo2", "/alice/dem", "/alice", "", "/bob/demo"] {
        assert_eq!(slice(full.as_bytes(), scope.as_bytes()), None, "{scope:?}");
    }
}

#[test]
fn pages_without_every_marker_get_the_full_page() {
    let full = page(SCOPE);
    for marker in [TITLE_START, TITLE_END, SUBNAV_START, SUBNAV_END, MAIN_START, MAIN_END] {
        let without = full.replacen(marker, "", 1);
        assert_eq!(slice(without.as_bytes(), SCOPE.as_bytes()), None, "{marker}");
    }
    // Out of order: the tabs after <main>.
    let swapped = format!(
        "{TITLE_START}<title>t</title>{TITLE_END}{MAIN_START}<main id=\"main\"></main>{MAIN_END}{SUBNAV_START}<div id=\"repo-subnav\" data-repo-scope=\"{SCOPE}\"></div>{SUBNAV_END}"
    );
    assert_eq!(slice(swapped.as_bytes(), SCOPE.as_bytes()), None);
    // Markup the cut does not recognise.
    let other = full.replace("<div id=\"repo-subnav\" data-repo-scope", "<div data-repo-scope");
    assert_eq!(slice(other.as_bytes(), SCOPE.as_bytes()), None);
}

#[test]
fn markers_inside_main_cannot_move_the_cut() {
    let full = page(SCOPE).replace("<p>list</p>", &format!("<p>{MAIN_END}{SUBNAV_START}{MAIN_START}</p>"));
    let partial = String::from_utf8(slice(full.as_bytes(), SCOPE.as_bytes()).unwrap()).unwrap();
    assert!(partial.contains(&format!("<main id=\"main\"><p>{MAIN_END}{SUBNAV_START}{MAIN_START}</p></main>")), "{partial}");
    assert!(!partial.contains("footer"));
}

#[test]
fn vary_appends_the_partial_header_once() {
    assert_eq!(vary_with_partial(None), "Rbit-Partial");
    assert_eq!(vary_with_partial(Some("")), "Rbit-Partial");
    assert_eq!(vary_with_partial(Some("HX-Request")), "HX-Request, Rbit-Partial");
}
