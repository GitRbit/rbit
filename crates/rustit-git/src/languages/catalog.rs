//! The languages Rustit recognises, with GitHub Linguist's names, colours and kinds so the
//! breakdown looks familiar.
//!
//! Detection is by exact file name first (`Makefile`, `Dockerfile`), then by extension.
//! Where Linguist resolves an ambiguous extension with content heuristics (`.h`, `.m`,
//! `.pl`, `.v`), this table assigns it to the most common language instead: Rustit never
//! reads blob contents for statistics.

/// Linguist's coarse classification. Only [`Programming`](Self::Programming) and
/// [`Markup`](Self::Markup) count toward a repository's breakdown by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageKind {
    Programming,
    Markup,
    Data,
    Prose,
}

impl LanguageKind {
    /// Whether files of this kind are counted unless `.gitattributes` says otherwise.
    pub fn is_detectable_by_default(self) -> bool {
        matches!(self, Self::Programming | Self::Markup)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub name: &'static str,
    /// `#rrggbb`, as published by Linguist.
    pub color: &'static str,
    pub kind: LanguageKind,
    /// Lower-case, without the leading dot.
    pub extensions: &'static [&'static str],
    /// Exact, case-sensitive file names.
    pub filenames: &'static [&'static str],
}

/// Colour for languages Linguist publishes no colour for.
pub const FALLBACK_COLOR: &str = "#8f8f8f";

const fn language(name: &'static str, color: &'static str, kind: LanguageKind, extensions: &'static [&'static str], filenames: &'static [&'static str]) -> Language {
    Language { name, color, kind, extensions, filenames }
}

use LanguageKind::{Data, Markup, Programming, Prose};

/// Each extension and file name appears at most once (checked by a unit test).
pub static LANGUAGES: &[Language] = &[
    // ---- Systems and compiled languages
    language("Rust", "#dea584", Programming, &["rs"], &[]),
    language("C", "#555555", Programming, &["c", "h"], &[]),
    language("C++", "#f34b7d", Programming, &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++", "ipp", "tpp", "ino"], &[]),
    language("C#", "#178600", Programming, &["cs", "csx"], &[]),
    language("Go", "#00ADD8", Programming, &["go"], &[]),
    language("Zig", "#ec915c", Programming, &["zig"], &[]),
    language("Odin", "#60AFFE", Programming, &["odin"], &[]),
    language("Nim", "#ffc200", Programming, &["nim", "nims", "nimble"], &[]),
    language("D", "#ba595e", Programming, &["d", "di"], &[]),
    language("Crystal", "#000100", Programming, &["cr"], &[]),
    language("Hare", "#9d7424", Programming, &["ha"], &[]),
    language("Mojo", "#ff4c1f", Programming, &["mojo"], &[]),
    language("Fortran", "#4d41b1", Programming, &["f", "for", "f77", "f90", "f95", "f03", "f08"], &[]),
    language("Ada", "#02f88c", Programming, &["adb", "ads", "ada"], &[]),
    language("Pascal", "#E3F171", Programming, &["pas", "dpr", "lpr"], &[]),
    language("Assembly", "#6E4C13", Programming, &["asm", "nasm", "s"], &[]),
    language("LLVM", "#185619", Programming, &["ll"], &[]),
    language("MLIR", "#5EC8DB", Programming, &["mlir"], &[]),
    language("WebAssembly", "#04133b", Programming, &["wat", "wast"], &[]),
    language("CUDA", "#3A4E3A", Programming, &["cu", "cuh"], &[]),
    language("Metal", "#8f14e9", Programming, &["metal"], &[]),
    language("GLSL", "#5686a5", Programming, &["glsl", "vert", "frag", "geom", "comp", "tesc", "tese"], &[]),
    language("HLSL", "#aace60", Programming, &["hlsl", "fx", "fxh"], &[]),
    language("WGSL", "#1a5e9a", Programming, &["wgsl"], &[]),
    language("ShaderLab", "#222c37", Programming, &["shader"], &[]),
    language("Verilog", "#b2b7f8", Programming, &["v", "veo"], &[]),
    language("SystemVerilog", "#DAE1C2", Programming, &["sv", "svh"], &[]),
    language("VHDL", "#adb2cb", Programming, &["vhd", "vhdl"], &[]),
    // ---- JVM and .NET
    language("Java", "#b07219", Programming, &["java"], &[]),
    language("Kotlin", "#A97BFF", Programming, &["kt", "kts"], &[]),
    language("Scala", "#c22d40", Programming, &["scala", "sc"], &[]),
    language("Groovy", "#4298b8", Programming, &["groovy", "gradle", "gvy"], &["Jenkinsfile"]),
    language("Clojure", "#db5855", Programming, &["clj", "cljs", "cljc", "edn"], &[]),
    language("F#", "#b845fc", Programming, &["fs", "fsi", "fsx"], &[]),
    language("Visual Basic .NET", "#945db7", Programming, &["vb"], &[]),
    language("VBA", "#867db1", Programming, &["vba", "bas"], &[]),
    // ---- Apple
    language("Swift", "#F05138", Programming, &["swift"], &[]),
    language("Objective-C", "#438eff", Programming, &["m"], &[]),
    language("Objective-C++", "#6866fb", Programming, &["mm"], &[]),
    language("AppleScript", "#101F1F", Programming, &["applescript", "scpt"], &[]),
    // ---- Scripting
    language("Python", "#3572A5", Programming, &["py", "pyw", "pyi"], &["SConstruct", "SConscript"]),
    language("Cython", "#fedf5b", Programming, &["pyx", "pxd"], &[]),
    language("JavaScript", "#f1e05a", Programming, &["js", "mjs", "cjs", "jsx"], &[]),
    language("TypeScript", "#3178c6", Programming, &["ts", "mts", "cts"], &[]),
    language("TSX", "#3178c6", Programming, &["tsx"], &[]),
    language("CoffeeScript", "#244776", Programming, &["coffee"], &[]),
    language("LiveScript", "#499886", Programming, &["ls"], &[]),
    language("Ruby", "#701516", Programming, &["rb", "rake", "gemspec", "ru"], &["Rakefile", "Gemfile", "Vagrantfile", "Podfile", "Fastfile", "Brewfile", "Guardfile"]),
    language("PHP", "#4F5D95", Programming, &["php", "phtml", "php3", "php4", "php5"], &[]),
    language("Hack", "#878787", Programming, &["hack"], &[]),
    language("Perl", "#0298c3", Programming, &["pl", "pm"], &[]),
    language("Raku", "#0000fb", Programming, &["raku", "rakumod", "p6", "pm6"], &[]),
    language("Lua", "#000080", Programming, &["lua"], &[]),
    language("Fennel", "#fff3d7", Programming, &["fnl"], &[]),
    language("Tcl", "#e4cc98", Programming, &["tcl"], &[]),
    language("R", "#198CE7", Programming, &["r", "rmd"], &[]),
    language("Julia", "#a270ba", Programming, &["jl"], &[]),
    language("MATLAB", "#e16737", Programming, &["matlab"], &[]),
    language("Wolfram Language", "#dd1100", Programming, &["wl", "wls"], &[]),
    language("SAS", "#B34936", Programming, &["sas"], &[]),
    language("Stata", "#1a5f91", Programming, &["do", "ado"], &[]),
    language("Dart", "#00B4AB", Programming, &["dart"], &[]),
    language("Haxe", "#df7900", Programming, &["hx", "hxsl"], &[]),
    language("GDScript", "#355570", Programming, &["gd"], &[]),
    language("QML", "#44a51c", Programming, &["qml"], &[]),
    language("Processing", "#0096D8", Programming, &["pde"], &[]),
    language("AutoHotkey", "#6594b9", Programming, &["ahk"], &[]),
    language("Vim Script", "#199f4b", Programming, &["vim"], &[".vimrc", "vimrc", ".gvimrc"]),
    language("Emacs Lisp", "#c065db", Programming, &["el"], &[".emacs"]),
    // ---- Shells and build tooling
    language("Shell", "#89e051", Programming, &["sh", "bash", "zsh", "ksh", "bats"], &[".bashrc", ".bash_profile", ".zshrc", ".profile"]),
    language("Fish", "#4aae47", Programming, &["fish"], &[]),
    language("Nushell", "#4E9906", Programming, &["nu"], &[]),
    language("PowerShell", "#012456", Programming, &["ps1", "psm1", "psd1"], &[]),
    language("Batchfile", "#C1F12E", Programming, &["bat", "cmd"], &[]),
    language("AWK", "#c30e9b", Programming, &["awk"], &[]),
    language("sed", "#64b970", Programming, &["sed"], &[]),
    language("Makefile", "#427819", Programming, &["mk", "mak"], &["Makefile", "makefile", "GNUmakefile"]),
    language("CMake", "#DA3434", Programming, &["cmake"], &["CMakeLists.txt"]),
    language("Meson", "#007800", Programming, &[], &["meson.build", "meson_options.txt", "meson.options"]),
    language("Starlark", "#76d275", Programming, &["bzl", "star"], &["BUILD", "BUILD.bazel", "WORKSPACE", "WORKSPACE.bazel", "MODULE.bazel"]),
    language("Just", "#384d54", Programming, &["just"], &["justfile", "Justfile", ".justfile"]),
    language("Dockerfile", "#384d54", Programming, &["dockerfile", "containerfile"], &["Dockerfile", "Containerfile"]),
    language("Earthly", "#2af0ff", Programming, &[], &["Earthfile"]),
    language("Nix", "#7e7eff", Programming, &["nix"], &[]),
    language("HCL", "#844FBA", Programming, &["hcl", "tf", "tfvars"], &[]),
    language("Bicep", "#519aba", Programming, &["bicep"], &[]),
    language("Jsonnet", "#0064bd", Programming, &["jsonnet", "libsonnet"], &[]),
    language("Dhall", "#dfafff", Programming, &["dhall"], &[]),
    language("CUE", "#5886E1", Programming, &["cue"], &[]),
    language("Puppet", "#302B6D", Programming, &["pp"], &["Puppetfile"]),
    language("Inno Setup", "#264b99", Programming, &["iss", "isl"], &[]),
    language("Nextflow", "#3ac486", Programming, &["nf"], &[]),
    // ---- Functional
    language("Haskell", "#5e5086", Programming, &["hs", "lhs", "hs-boot"], &[]),
    language("Elm", "#60B5CC", Programming, &["elm"], &[]),
    language("PureScript", "#1D222D", Programming, &["purs"], &[]),
    language("OCaml", "#ef7a08", Programming, &["ml", "mli", "mll", "mly"], &[]),
    language("Reason", "#ff5847", Programming, &["re", "rei"], &[]),
    language("ReScript", "#ed5051", Programming, &["res", "resi"], &[]),
    language("Standard ML", "#dc566d", Programming, &["sml", "sig"], &[]),
    language("Elixir", "#6e4a7e", Programming, &["ex", "exs"], &[]),
    language("Erlang", "#B83998", Programming, &["erl", "hrl"], &["rebar.config"]),
    language("Gleam", "#ffaff3", Programming, &["gleam"], &[]),
    language("Common Lisp", "#3fb68b", Programming, &["lisp", "lsp", "cl", "asd"], &[]),
    language("Scheme", "#1e4aec", Programming, &["scm", "ss", "sld"], &[]),
    language("Racket", "#3c5caa", Programming, &["rkt", "rktl"], &[]),
    language("Janet", "#0886a5", Programming, &["janet"], &[]),
    language("Idris", "#b30000", Programming, &["idr", "lidr"], &[]),
    language("Agda", "#315665", Programming, &["agda"], &[]),
    language("Smalltalk", "#596706", Programming, &["st"], &[]),
    language("Forth", "#341708", Programming, &["fth", "4th", "forth"], &[]),
    // ---- Smart contracts and niche
    language("Solidity", "#AA6746", Programming, &["sol"], &[]),
    language("Vyper", "#9F4CF2", Programming, &["vy"], &[]),
    language("Move", "#4a137a", Programming, &["move"], &[]),
    language("Cairo", "#ff4a48", Programming, &["cairo"], &[]),
    language("Q#", "#fed659", Programming, &["qs"], &[]),
    language("Ballerina", "#FF5000", Programming, &["bal"], &[]),
    language("Chapel", "#8dc63f", Programming, &["chpl"], &[]),
    language("Vala", "#a56de2", Programming, &["vala", "vapi"], &[]),
    language("ABAP", "#E8274B", Programming, &["abap"], &[]),
    language("Gherkin", "#5B2063", Programming, &["feature"], &[]),
    language("XSLT", "#EB8CEB", Programming, &["xsl", "xslt"], &[]),
    language("ANTLR", "#9DC3FF", Programming, &["g4"], &[]),
    language("Yacc", "#4B6C4B", Programming, &["y", "yy", "yacc"], &[]),
    language("Lex", "#DBCA00", Programming, &["l", "lex"], &[]),
    language("Gnuplot", "#f0a9f0", Programming, &["gp", "gnuplot", "plt"], &[]),
    language("Thrift", "#D12127", Programming, &["thrift"], &[]),
    language("Cap'n Proto", "#c42727", Programming, &["capnp"], &[]),
    // ---- Markup and templates
    language("HTML", "#e34c26", Markup, &["html", "htm", "xhtml"], &[]),
    language("CSS", "#663399", Markup, &["css"], &[]),
    language("SCSS", "#c6538c", Markup, &["scss"], &[]),
    language("Sass", "#a53b70", Markup, &["sass"], &[]),
    language("Less", "#1d365d", Markup, &["less"], &[]),
    language("Stylus", "#ff6347", Markup, &["styl"], &[]),
    language("Vue", "#41b883", Markup, &["vue"], &[]),
    language("Svelte", "#ff3e00", Markup, &["svelte"], &[]),
    language("Astro", "#ff5a03", Markup, &["astro"], &[]),
    language("Jupyter Notebook", "#DA5B0B", Markup, &["ipynb"], &[]),
    language("MDX", "#fcb32c", Markup, &["mdx"], &[]),
    language("TeX", "#3D6117", Markup, &["tex", "sty", "cls", "ltx", "dtx"], &[]),
    language("BibTeX", "#778899", Markup, &["bib", "bibtex"], &[]),
    language("Typst", "#239dad", Markup, &["typ"], &[]),
    language("PostScript", "#da291c", Markup, &["ps", "eps"], &[]),
    language("Mermaid", "#ff3670", Markup, &["mmd", "mermaid"], &[]),
    language("Twig", "#c1d026", Markup, &["twig"], &[]),
    language("Jinja", "#a52a22", Markup, &["jinja", "jinja2", "j2"], &[]),
    language("Nunjucks", "#3d8137", Markup, &["njk", "nunjucks"], &[]),
    language("Handlebars", "#f7931e", Markup, &["hbs", "handlebars"], &[]),
    language("Mustache", "#724b3b", Markup, &["mustache"], &[]),
    language("EJS", "#a91e50", Markup, &["ejs"], &[]),
    language("Pug", "#a86454", Markup, &["pug", "jade"], &[]),
    language("Haml", "#ece2a9", Markup, &["haml"], &[]),
    language("Slim", "#2b2b2b", Markup, &["slim"], &[]),
    language("Liquid", "#67b8de", Markup, &["liquid"], &[]),
    language("Smarty", "#f0c040", Markup, &["tpl", "smarty"], &[]),
    language("HTML+ERB", "#701516", Markup, &["erb", "rhtml"], &[]),
    language("HTML+EEx", "#6e4a7e", Markup, &["eex", "heex", "leex"], &[]),
    language("HTML+Razor", "#512be4", Markup, &["cshtml", "razor"], &[]),
    // ---- Data
    language("JSON", "#292929", Data, &["json", "jsonc", "geojson", "webmanifest"], &[".babelrc", ".eslintrc"]),
    language("JSON5", "#267CB9", Data, &["json5"], &[]),
    language("YAML", "#cb171e", Data, &["yml", "yaml"], &[".clang-format", ".clang-tidy"]),
    language("TOML", "#9c4221", Data, &["toml"], &[]),
    language("XML", "#0060ac", Data, &["xml", "xsd", "plist", "csproj", "vcxproj", "props", "targets", "resx", "xaml", "wsdl"], &[]),
    language("SVG", "#ff9900", Data, &["svg"], &[]),
    language("INI", "#d1dbe0", Data, &["ini", "cfg", "prefs", "properties"], &[".editorconfig", ".gitconfig"]),
    language("CSV", "#237346", Data, &["csv", "tsv"], &[]),
    language("SQL", "#e38c00", Data, &["sql"], &[]),
    language("GraphQL", "#e10098", Data, &["graphql", "gql", "graphqls"], &[]),
    language("Protocol Buffer", FALLBACK_COLOR, Data, &["proto"], &[]),
    language("PlantUML", "#fbbd16", Data, &["puml", "plantuml", "iuml"], &[]),
    language("Graphviz (DOT)", "#2596be", Data, &["dot", "gv"], &[]),
    // ---- Prose
    language("Markdown", "#083fa1", Prose, &["md", "markdown", "mdown", "mkd", "mkdn"], &[]),
    language("reStructuredText", "#141414", Prose, &["rst", "rest"], &[]),
    language("AsciiDoc", "#73a0c5", Prose, &["adoc", "asciidoc", "asc"], &[]),
    language("Org", "#77aa99", Prose, &["org"], &[]),
    language("Text", FALLBACK_COLOR, Prose, &["txt"], &["LICENSE", "COPYING"]),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn extensions_and_filenames_are_unique() {
        let mut extensions = HashSet::new();
        let mut filenames = HashSet::new();
        for language in LANGUAGES {
            for extension in language.extensions {
                assert!(extensions.insert(*extension), "duplicate extension {extension}");
                assert_eq!(*extension, extension.to_lowercase(), "extensions are lower-case");
            }
            for filename in language.filenames {
                assert!(filenames.insert(*filename), "duplicate filename {filename}");
            }
        }
    }

    #[test]
    fn colors_are_hex() {
        for language in LANGUAGES {
            let hex = language.color.strip_prefix('#').unwrap_or_default();
            assert!(hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()), "{}: {}", language.name, language.color);
        }
    }

    #[test]
    fn covers_the_common_languages() {
        assert!(LANGUAGES.len() >= 120, "{} languages", LANGUAGES.len());
    }
}
