//! Synthetic render-loop output; compiler memory belongs to an external sampler.

use std::{hint::black_box, process::ExitCode, time::Instant};

use rbit_render_probe::{CASE_COUNT, COMPILED_MODES, FixtureData, Mode, Row, render_into};
use serde_json::{Value, json};

const USAGE: &str = "rbit-render-probe [--mode inheritance|segments|blocks] [--count N] \
[--iterations N] [--warmup N] [--label SOURCE_AND_FLAGS] [--emit]\n\
Count must match the compiled count feature. Timings cover synthetic rendering only.\n";

#[derive(Debug)]
struct Options {
    mode: Mode,
    iterations: u64,
    warmup: u64,
    label: String,
    emit: bool,
}

impl Options {
    fn parse(arguments: Vec<String>) -> Result<Self, String> {
        let mut options = Self { mode: COMPILED_MODES[0], iterations: 1000, warmup: 2, label: "unlabeled".into(), emit: false };
        let mut arguments = arguments.into_iter();
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--mode" => options.mode = Mode::parse(&value(&mut arguments, &argument)?).ok_or("unknown mode")?,
                "--count" => {
                    let requested: usize = value(&mut arguments, &argument)?.parse().map_err(|_| "invalid count")?;
                    if requested != CASE_COUNT {
                        return Err(format!("requested count {requested}, compiled count {CASE_COUNT}; rebuild with the matching feature"));
                    }
                }
                "--iterations" => options.iterations = number(&mut arguments, &argument, 1, 10_000_000)?,
                "--warmup" => options.warmup = number(&mut arguments, &argument, 0, 100_000)?,
                "--label" => options.label = value(&mut arguments, &argument)?,
                "--emit" => options.emit = true,
                _ => return Err(format!("unknown argument {argument}; use --help")),
            }
        }
        if !COMPILED_MODES.contains(&options.mode) {
            return Err(format!("mode {} was not compiled", options.mode.name()));
        }
        Ok(options)
    }
}

fn value(arguments: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    arguments.next().ok_or_else(|| format!("missing value for {option}"))
}

fn number(arguments: &mut impl Iterator<Item = String>, option: &str, minimum: u64, maximum: u64) -> Result<u64, String> {
    let number = value(arguments, option)?.parse().map_err(|_| format!("invalid number for {option}"))?;
    if !(minimum..=maximum).contains(&number) {
        return Err(format!("{option} must be in {minimum}..={maximum}"));
    }
    Ok(number)
}

fn round(mode: Mode, data: &FixtureData<'_>, output: &mut String) -> Result<u64, String> {
    let mut bytes = 0_u64;
    for case in 0..CASE_COUNT {
        output.clear();
        render_into(black_box(mode), black_box(case), black_box(data), output).map_err(|error| error.to_string())?;
        bytes = bytes.checked_add(u64::try_from(output.len()).map_err(|_| "document size overflow")?).ok_or("byte counter overflow")?;
        black_box(output.as_bytes());
    }
    Ok(bytes)
}

fn run(options: &Options) -> Result<Value, String> {
    let rows = [Row { key: "first", label: "Repository one" }, Row { key: "second", label: "Repository two" }];
    let data = FixtureData {
        title: "Repository discovery",
        site_name: "Render probe",
        viewer: Some("alice"),
        description: "Synthetic rows, not a real forge workload.",
        footer: "Development experiment",
        dark: false,
        rows: &rows,
    };
    let mut output = String::with_capacity(4096);
    for _ in 0..options.warmup {
        round(options.mode, &data, &mut output)?;
    }
    let start = Instant::now();
    let mut bytes = 0_u64;
    for _ in 0..options.iterations {
        bytes = bytes.checked_add(round(options.mode, &data, &mut output)?).ok_or("byte counter overflow")?;
    }
    let elapsed_ns = u64::try_from(start.elapsed().as_nanos()).map_err(|_| "elapsed time overflow")?;
    let documents = options
        .iterations
        .checked_mul(u64::try_from(CASE_COUNT).map_err(|_| "case counter overflow")?)
        .ok_or("document counter overflow")?;
    let sample = if options.emit {
        output.clear();
        render_into(options.mode, 0, &data, &mut output).map_err(|error| error.to_string())?;
        Some(output.as_str())
    } else {
        None
    };
    Ok(json!({
        "scope": "synthetic rendering only; no production adoption or compiler-memory claim",
        "recipe_version": 2,
        "label": options.label,
        "mode": options.mode.name(),
        "count": CASE_COUNT,
        "iterations": options.iterations,
        "warmup_rounds": options.warmup,
        "rendered_documents": documents,
        "rendered_bytes": bytes,
        "elapsed_ns": elapsed_ns,
        "buffer_reused": true,
        "buffer_capacity": output.capacity(),
        "sample_html": sample,
        "compiler_peak_memory_bytes": null,
        "compile_wall_ns": null,
        "build": {
            "package_version": env!("CARGO_PKG_VERSION"),
            "askama_version": "0.16.1",
            "askama_requested_features": ["derive", "std"],
            "compiled_modes": COMPILED_MODES.iter().map(|mode| mode.name()).collect::<Vec<_>>(),
            "debug_assertions": cfg!(debug_assertions),
            "profile": env!("RENDER_PROBE_PROFILE"),
            "target": env!("RENDER_PROBE_TARGET"),
            "opt_level": env!("RENDER_PROBE_OPT_LEVEL"),
            "debug_info": env!("RENDER_PROBE_DEBUG"),
            "target_features": env!("RENDER_PROBE_CARGO_CFG_TARGET_FEATURE"),
        }
    }))
}

fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments == ["--help"] {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    match Options::parse(arguments).and_then(|options| run(&options)) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", json!({"error": error}));
            ExitCode::FAILURE
        }
    }
}
