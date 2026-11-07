//! Native child/helper/grandchild fixture; intentionally inherits the host's stdio.

use crate::wire::{self, Identity};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

pub(crate) fn run(args: &[OsString]) -> Result<()> {
    let role = args.first().and_then(|arg| arg.to_str()).context("fixture role missing")?;
    ensure!(["root", "helper", "grandchild", "sibling"].contains(&role), "unknown fixture role");
    let directory = PathBuf::from(args.get(1).context("fixture directory missing")?);
    let identity = Identity::capture(std::process::id())?;
    wire::write(&directory.join(format!("{role}.json")), &identity)?;
    let next = match role {
        "root" => Some("helper"),
        "helper" => Some("grandchild"),
        _ => None,
    };
    // Keep the Child object for the fixture's lifetime. Abrupt-death tests terminate
    // this process without Rust destructors, exercising OS Job containment only.
    let mut child = next
        .map(|next| {
            Command::new(std::env::current_exe()?)
                .args([OsString::from("fixture"), OsString::from(next), directory.as_os_str().into()])
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
        })
        .transpose()?;
    println!("{role} ready");
    loop {
        if role == "root" && directory.join("exit-root").exists() {
            return Ok(());
        }
        if let Some(child) = &mut child {
            let _ = child.try_wait()?;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
