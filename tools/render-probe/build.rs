//! Creates synthetic benchmark fixtures only, never application business logic.
//! Each selected mode gets distinct concrete Template derives, not const generics.

use std::{
    env,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
};

const COUNTS: [usize; 4] = [1, 8, 32, 96];
const MODES: [&str; 3] = ["inheritance", "segments", "blocks"];

fn main() -> io::Result<()> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo build output directory"));
    let counts: Vec<_> = COUNTS.into_iter().filter(|count| enabled(&format!("count-{count}"))).collect();
    assert_eq!(counts.len(), 1, "select exactly one count-1/count-8/count-32/count-96 feature");
    let count = counts[0];
    let modes: Vec<_> = MODES.into_iter().filter(|mode| enabled(mode)).collect();
    assert!(!modes.is_empty(), "select inheritance, segments or blocks (all three for semantic tests)");
    let templates = out.join("templates");
    fs::create_dir_all(&templates)?;
    let mut sources = Vec::new();
    for name in ["shell_open.html", "shell_close.html", "body.html", "macros.html"] {
        let input = manifest.join("templates").join(name);
        println!("cargo:rerun-if-changed={}", input.display());
        let source = fs::read_to_string(input)?;
        write_if_changed(&templates.join(name), &source)?;
        sources.push(source);
    }
    // Askama 0.16.1 removes one final LF from each independently loaded template.
    // Concatenation makes those LFs internal, so assemble the inherited source from
    // the bytes each standalone segment actually renders. Keep the final comment
    // so loading base.html cannot remove a second LF from a multi-newline suffix.
    let base = format!(
        "{}{{% block content %}}{{% endblock %}}{}{{# segment-end #}}",
        rendered_segment(&sources[0]),
        rendered_segment(&sources[1])
    );
    write_if_changed(&templates.join("base.html"), &base)?;
    // Askama imports are top-level declarations, not legal inside a content block.
    let body = rendered_segment(&sources[2]).strip_prefix("{% import \"macros.html\" as ui %}").expect("body macro import prefix");
    let inherited = format!(
        "{{% extends {:?} %}}{{% import \"macros.html\" as ui %}}{{% block content %}}{body}{{% endblock %}}",
        template_path(&templates.join("base.html"))
    );
    write_if_changed(&templates.join("inherited.html"), &inherited)?;
    let mut root = format!("pub const CASE_COUNT: usize = {count};\n");
    for mode in modes {
        generate_mode(&out, &templates, mode, count)?;
        writeln!(root, "#[path = {:?}] mod {mode};", template_path(&out.join(mode).join("mod.rs"))).expect("writing generated source");
    }
    write_if_changed(&out.join("generated.rs"), &root)?;
    for name in ["TARGET", "PROFILE", "OPT_LEVEL", "DEBUG", "CARGO_CFG_TARGET_FEATURE"] {
        println!("cargo:rustc-env=RENDER_PROBE_{name}={}", env::var(name).unwrap_or_default());
    }
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}

fn enabled(feature: &str) -> bool {
    let variable = format!("CARGO_FEATURE_{}", feature.replace('-', "_").to_ascii_uppercase());
    println!("cargo:rerun-if-env-changed={variable}");
    env::var_os(variable).is_some()
}

fn generate_mode(out: &Path, templates: &Path, mode: &str, count: usize) -> io::Result<()> {
    let directory = out.join(mode);
    fs::create_dir_all(&directory)?;
    let template = templates.join(if mode == "segments" { "body.html" } else { "inherited.html" });
    let mut module = String::from("//! Generated synthetic fixture dispatch; no application routes.\n");
    for index in 0..count {
        writeln!(module, "mod case_{index};").expect("writing generated source");
        let attribute = if mode == "blocks" { ", block = \"content\"" } else { "" };
        let mut case = format!(
            "//! Synthetic concrete template context {index}.\nuse askama::Template;\n\
             #[derive(Template)]\n#[template(path = {:?}{attribute})]\n\
             pub(super) struct Case{index}<'a> {{\n\
             pub(super) data: &'a crate::FixtureData<'a>,\n\
             pub(super) case: usize,\n}}\n",
            template_path(&template)
        );
        if mode != "inheritance" {
            write!(
                case,
                "impl crate::composition::sealed::ApprovedHtml for Case{index}<'_> {{}}\n\
                 impl crate::composition::ApprovedHtml for Case{index}<'_> {{\n\
                 fn render_into(&self, out: &mut dyn std::fmt::Write) -> askama::Result<()> {{\n\
                 askama::Template::render_into(self, out)\n}}\n}}\n"
            )
            .expect("writing generated source");
        }
        write_if_changed(&directory.join(format!("case_{index}.rs")), &case)?;
    }
    module.push_str("pub(super) fn render(case: usize, data: &crate::FixtureData<'_>, out: &mut String) -> Result<(), crate::ProbeError> {\nmatch case {\n");
    for index in 0..count {
        let constructor = format!("case_{index}::Case{index} {{ data, case }}");
        let render = if mode == "inheritance" {
            format!("askama::Template::render_into(&{constructor}, out)")
        } else {
            format!("crate::composition::render(data, &{constructor}, out)")
        };
        writeln!(module, "{index} => {render}.map_err(crate::ProbeError::Render),").expect("writing generated source");
    }
    module.push_str("_ => Err(crate::ProbeError::CaseOutOfRange { requested: case, available: crate::CASE_COUNT }),\n}\n}\n");
    write_if_changed(&directory.join("mod.rs"), &module)
}

fn template_path(path: &Path) -> String {
    path.to_str().expect("UTF-8 benchmark fixture path").replace('\\', "/")
}

fn rendered_segment(source: &str) -> &str {
    source.strip_suffix('\n').unwrap_or(source)
}

fn write_if_changed(path: &Path, source: &str) -> io::Result<()> {
    if fs::read(path).is_ok_and(|previous| previous == source.as_bytes()) {
        return Ok(());
    }
    fs::write(path, source)
}
