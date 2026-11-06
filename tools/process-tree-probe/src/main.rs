//! Development-only native fixtures; no platform dependency enters the server graph.

#[cfg(any(windows, test))]
mod contract;
#[cfg(windows)]
mod fixture;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod wire;

fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        let role = args.first().and_then(|value| value.to_str()).unwrap_or("verify");
        if role == "fixture" {
            return fixture::run(&args[1..]);
        }
        if role == "parent" || role == "verify" {
            // Establish inherited containment before creating a runtime or spawning.
            windows::root::install()?;
        }
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
        runtime.block_on(async {
            match role {
                "verify" => windows::controller::verify(args.get(1).map(std::path::PathBuf::from)).await,
                "parent" => windows::parent::run(&args[1..]).await,
                _ => anyhow::bail!("usage: rbit-process-tree-probe verify [report.json]"),
            }
        })
    }
    #[cfg(target_os = "linux")]
    {
        linux::run()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    anyhow::bail!("native proof supports Windows and Linux only")
}
