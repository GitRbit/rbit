//! Linux development observations for known fixtures, not general process containment.

mod anchor;
mod case;
mod completion;
mod controller;
mod fixture;
mod identity;
mod wire;

pub(crate) fn run() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let role = args.first().and_then(|value| value.to_str()).unwrap_or("verify-unix");
    if role == "fixture-unix" {
        return fixture::run(&args[1..]);
    }
    anyhow::ensure!(role == "verify-unix", "usage: rbit-process-tree-probe verify-unix [report.json]");
    // Only this isolated probe becomes a subreaper, before any child or runtime exists.
    // The published safe API models the kernel boolean as an optional nonzero Pid.
    rustix::process::set_child_subreaper(Some(rustix::process::Pid::INIT))?;
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    runtime.block_on(controller::verify(args.get(1).map(std::path::PathBuf::from)))
}
