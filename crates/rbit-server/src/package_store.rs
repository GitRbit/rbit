//! Explicit host-owned provisioning; no database, network, shell or automatic repair.
//!
//! The configured data directory and its ancestors must be trusted and stable.
//! These path-based checks do not defeat an actor able to replace ancestors or
//! mutate host-owned paths concurrently. Run as the intended service account.
//! A successful check verifies current permissions, not previous crash durability.

use std::path::{Path, PathBuf};

#[cfg(unix)]
use anyhow::Context as _;
use anyhow::bail;

use crate::cli::PackageStoreCommand;

pub async fn run(data_dir: PathBuf, command: PackageStoreCommand) -> anyhow::Result<()> {
    let provision = matches!(command, PackageStoreCommand::Provision);
    let created = tokio::task::spawn_blocking(move || inspect_or_provision(&data_dir, provision)).await??;
    if provision {
        println!("Package store ready; {created} directories created and synchronized.");
    } else {
        println!(
            "Package store directory checks passed. Ancestor trust/stability and prior provisioning durability remain host preconditions."
        );
    }
    Ok(())
}

#[cfg(unix)]
fn inspect_or_provision(data_dir: &Path, provision: bool) -> anyhow::Result<u8> {
    use std::{
        fs,
        os::unix::fs::{DirBuilderExt as _, PermissionsExt as _},
    };

    let data = fs::symlink_metadata(data_dir).context("configured data directory is unavailable")?;
    if !data.is_dir() || data.file_type().is_symlink() || data.permissions().mode() & 0o022 != 0 {
        bail!("configured data directory must be a trusted nonsymlink directory without group/world write permissions");
    }
    let mut parent = data_dir.to_path_buf();
    let mut created = 0;
    for component in ["plugins", "packages"] {
        let path = parent.join(component);
        let new = match fs::symlink_metadata(&path) {
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && provision => {
                // Nonrecursive and create-only: an existing object is never repaired.
                let mut builder = fs::DirBuilder::new();
                builder.mode(0o700);
                builder.create(&path).context("cannot create private package directory")?;
                true
            }
            Err(_) => bail!("package directory is unavailable; provision explicitly if missing"),
        };
        let metadata = fs::symlink_metadata(&path).context("cannot inspect package directory")?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o7777 != 0o700 {
            bail!(
                "package directory must be nonsymlink with exact mode 0700; permissions were not repaired (a restrictive umask can prevent usable creation)"
            );
        }
        if provision {
            // Also synchronize existing parents on explicit provision. This makes
            // a retry after interrupted mkdir/fsync useful without changing modes.
            fs::File::open(&path).and_then(|directory| directory.sync_all()).context("package directory synchronization failed")?;
            fs::File::open(&parent).and_then(|directory| directory.sync_all()).context("package parent synchronization failed")?;
        }
        if new {
            created += 1;
        }
        parent = path;
    }
    Ok(created)
}

#[cfg(not(unix))]
fn inspect_or_provision(_: &Path, _: bool) -> anyhow::Result<u8> {
    bail!(
        "package-store verification/provisioning is unsupported on this platform; Windows requires externally secured ACLs and durable host provisioning"
    )
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        fs,
        os::unix::fs::{PermissionsExt as _, symlink},
    };

    use super::*;

    fn private(path: &Path) {
        fs::create_dir(path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn check_is_read_only_and_provision_is_private_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        private(&data);
        fs::write(data.join("rbit.db"), b"not a database").unwrap();
        assert!(inspect_or_provision(&data, false).is_err());
        assert!(!data.join("plugins").exists());
        assert_eq!(inspect_or_provision(&data, true).unwrap(), 2);
        for path in [data.join("plugins"), data.join("plugins/packages")] {
            assert_eq!(fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777, 0o700);
        }
        let object = data.join("plugins/packages/retained.rbit-plugin");
        fs::write(&object, b"retained immutable bytes").unwrap();
        assert_eq!(inspect_or_provision(&data, false).unwrap(), 0);
        assert_eq!(inspect_or_provision(&data, true).unwrap(), 0);
        assert_eq!(fs::read(object).unwrap(), b"retained immutable bytes");
        assert_eq!(fs::read(data.join("rbit.db")).unwrap(), b"not a database");
        assert!(!data.join("secret.key").exists());
        assert!(!data.join("rbit.redb").exists());
    }

    #[test]
    fn missing_or_writable_data_directory_is_never_created_or_repaired() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        assert!(inspect_or_provision(&data, true).is_err());
        assert!(!data.exists());
        private(&data);
        fs::set_permissions(&data, fs::Permissions::from_mode(0o770)).unwrap();
        for provision in [false, true] {
            assert!(inspect_or_provision(&data, provision).is_err());
            assert_eq!(fs::metadata(&data).unwrap().permissions().mode() & 0o777, 0o770);
            assert!(!data.join("plugins").exists());
        }
    }

    #[test]
    fn unsafe_existing_components_are_never_repaired_or_populated() {
        for component in ["plugins", "packages"] {
            for (file, mode) in [(false, 0o750), (false, 0o500), (false, 0o000), (true, 0o600)] {
                let temp = tempfile::tempdir().unwrap();
                let data = temp.path().join("data");
                private(&data);
                let unsafe_path = if component == "packages" {
                    private(&data.join("plugins"));
                    data.join("plugins/packages")
                } else {
                    data.join("plugins")
                };
                if file {
                    fs::write(&unsafe_path, b"do not replace").unwrap();
                } else {
                    private(&unsafe_path);
                    fs::set_permissions(&unsafe_path, fs::Permissions::from_mode(mode)).unwrap();
                }
                for provision in [false, true] {
                    assert!(inspect_or_provision(&data, provision).is_err());
                    if file {
                        assert_eq!(fs::read(&unsafe_path).unwrap(), b"do not replace");
                    } else {
                        assert_eq!(fs::metadata(&unsafe_path).unwrap().permissions().mode() & 0o777, mode);
                    }
                }
                if !file {
                    fs::set_permissions(&unsafe_path, fs::Permissions::from_mode(0o700)).unwrap();
                    assert_eq!(fs::read_dir(&unsafe_path).unwrap().count(), 0);
                }
            }
        }
    }

    #[test]
    fn final_component_links_are_rejected_without_following_or_populating_targets() {
        for component in ["data", "plugins", "packages"] {
            let temp = tempfile::tempdir().unwrap();
            let data = temp.path().join("data");
            let outside = temp.path().join("outside");
            private(&outside);
            if component != "data" {
                private(&data);
            }
            if component == "packages" {
                private(&data.join("plugins"));
            }
            let link = match component {
                "data" => data.clone(),
                "plugins" => data.join("plugins"),
                _ => data.join("plugins/packages"),
            };
            symlink(&outside, &link).unwrap();
            for provision in [false, true] {
                assert!(inspect_or_provision(&data, provision).is_err());
                assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
                assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
            }
        }
    }
}
