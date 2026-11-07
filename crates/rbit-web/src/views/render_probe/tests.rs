//! Exact source and rendered-byte gates; these tests make no performance assertion.

use askama::Template;
use rbit_localization::Locale;

use super::{MODES, Mode, explore_into, fixtures, login_into};
use crate::{
    assets,
    themes::{self, Theme},
    views::explore::ExploreFragment,
};
use fixtures::{Actor, UNTRUSTED};

const IMPORT: &str = "{% import \"macros.html\" as ui %}";
const BOUNDARY: &str = "{# probe-boundary: preserve source whitespace #}";

#[derive(Debug, Template)]
#[template(source = "{{ value }}", ext = "html")]
struct Escaped<'a> {
    value: &'a str,
}

fn content(source: &str) -> &str {
    source.split_once("{% block content %}").unwrap().1.rsplit_once("{% endblock %}").unwrap().0
}

#[test]
fn committed_probe_templates_exactly_track_real_shell_and_content_sources() {
    let base = include_str!("../../../templates/base.html");
    let (open, close) = base.split_once("{% block content %}{% endblock %}").unwrap();
    assert_eq!(include_str!("../../../templates/render_probe/shell_open.html").strip_suffix(BOUNDARY).unwrap(), open);
    // Askama 0.16.1 strips exactly one terminal LF per standalone template.
    assert_eq!(
        include_str!("../../../templates/render_probe/shell_close.html").strip_prefix(IMPORT).unwrap().strip_suffix(BOUNDARY).unwrap(),
        close.strip_suffix('\n').unwrap_or(close)
    );
    for (original, body) in [
        (include_str!("../../../templates/home/explore.html"), include_str!("../../../templates/render_probe/explore_body.html")),
        (include_str!("../../../templates/auth/login.html"), include_str!("../../../templates/render_probe/login_body.html")),
    ] {
        let raw_body = content(original);
        assert_eq!(original, format!("{{% extends \"base.html\" %}}\n{IMPORT}\n{{% block content %}}{raw_body}{{% endblock %}}\n"));
        assert_eq!(body.strip_prefix(IMPORT).unwrap().strip_suffix(BOUNDARY).unwrap(), content(original));
    }
}

fn assert_untrusted_is_escaped_once(html: &str) {
    let once = Escaped { value: UNTRUSTED }.render().unwrap();
    let twice = Escaped { value: &once }.render().unwrap();
    assert!(html.contains(&once));
    assert!(!html.contains(&twice));
    assert!(!html.contains("<script>probe</script>"));
    assert!(!html.contains("</title><!--partial:main-->"));
}

fn assert_styles(html: &str, theme: Theme, include: bool) {
    assert_eq!(html.contains("<style data-app-stylesheet>"), include);
    if include {
        assert!(html.contains(&format!("<style data-app-stylesheet>{}</style>", assets::inline_css("app.css").as_str())));
        if let Some(path) = theme.stylesheet_path() {
            assert!(html.contains(assets::inline_css(&path).as_str()));
            assert!(html.contains("<style data-theme-stylesheet"));
        }
    } else {
        assert!(!html.contains("<style"));
    }
}

#[tokio::test]
async fn explore_candidates_are_byte_identical_across_real_page_axes() {
    assert!(crate::timing::elapsed().is_none());
    let tinted = Theme::Tinted(&themes::tinted()[0]);
    for actor in [Actor::Anonymous, Actor::Member, Actor::Admin] {
        for theme in [Theme::System, Theme::Light, Theme::Dark, Theme::DnaDay, Theme::DnaDark, tinted] {
            for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
                let view = fixtures::explore(actor, theme, locale, false);
                for include in [false, true] {
                    assets::with_head_styles(include, async {
                        let expected = view.render().unwrap();
                        for mode in MODES {
                            let mut actual = String::new();
                            explore_into(mode, &view, &mut actual).unwrap();
                            assert_eq!(actual, expected, "Explore {mode:?} {actor:?} {theme:?} {locale:?} head={include}");
                            assert_untrusted_is_escaped_once(&actual);
                            assert_styles(&actual, theme, include);
                            assert!(actual.contains(&format!("<html lang=\"{}\" dir=\"{}\"", locale.tag(), locale.direction().as_str())));
                            assert!(actual.contains("method=\"get\" action=\"/explore\""));
                            assert!(actual.contains("hx-target=\"#explore-results\""));
                            assert!(actual.contains("aria-current=\"true\""));
                            assert_eq!(actual.contains("data-signed-in"), !matches!(actor, Actor::Anonymous));
                            assert_eq!(actual.contains("href=\"/admin\""), matches!(actor, Actor::Admin));
                        }
                    })
                    .await;
                }
            }
        }
    }
}

#[tokio::test]
async fn login_candidates_preserve_forms_provider_errors_and_shared_navigation() {
    for actor in [Actor::Anonymous, Actor::Member, Actor::Admin] {
        for theme in [Theme::System, Theme::Dark, Theme::Tinted(&themes::tinted()[0])] {
            for extra in [false, true] {
                let view = fixtures::login(actor, theme, extra);
                for include in [false, true] {
                    assets::with_head_styles(include, async {
                        let expected = view.render().unwrap();
                        for mode in MODES {
                            let mut actual = String::new();
                            login_into(mode, &view, &mut actual).unwrap();
                            assert_eq!(actual, expected, "Login {mode:?} {actor:?} {theme:?} extra={extra} head={include}");
                            assert_untrusted_is_escaped_once(&actual);
                            assert_styles(&actual, theme, include);
                            assert!(actual.contains("<html lang=\"en\" dir=\"ltr\""));
                            assert!(actual.contains("method=\"post\" action=\"/login\""));
                            assert!(actual.contains("type=\"password\" autocomplete=\"current-password\" required"));
                            assert_eq!(actual.contains("Sign in with"), extra);
                        }
                    })
                    .await;
                }
            }
        }
    }
}

#[test]
fn empty_results_and_direct_fragment_keep_the_same_feature_component() {
    for empty_query in [false, true] {
        let mut view = fixtures::explore(Actor::Anonymous, Theme::Dark, Locale::PseudoRtl, true);
        view.results.query_is_empty = empty_query;
        let reference = view.render().unwrap();
        for mode in [Mode::Segments, Mode::Blocks] {
            let mut output = String::new();
            explore_into(mode, &view, &mut output).unwrap();
            assert_eq!(output, reference);
        }
        let fragment = ExploreFragment { page: &view.page, query: &view.query, results: &view.results }.render().unwrap();
        let marker = "<div id=\"explore-results\"";
        let component = fragment.split_once(marker).unwrap().1.trim();
        let full_component = reference.split_once(marker).unwrap().1.split_once("</main>").unwrap().0.trim();
        assert_eq!(component, full_component);
        assert!(!fragment.contains("<head>") && !fragment.contains("<form"));
    }
}
