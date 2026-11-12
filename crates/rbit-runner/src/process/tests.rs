use super::*;

#[test]
fn programs_are_found_on_the_path() {
    let dir = tempfile::tempdir().unwrap();
    let tool = if cfg!(windows) { "tool.exe" } else { "tool" };
    std::fs::write(dir.path().join(tool), "").unwrap();
    let path = std::env::join_paths([dir.path().join("missing"), dir.path().to_path_buf()]).unwrap();
    let found = find_in("tool", Some(&path), Some(OsStr::new(".EXE;.CMD"))).unwrap();
    assert_eq!(found.file_name().unwrap().to_string_lossy().to_lowercase(), tool);
    assert!(find_in("other", Some(&path), None).is_none());
    assert!(find_in("", Some(&path), None).is_none());
    let direct = dir.path().join(tool);
    assert_eq!(find_in(direct.to_str().unwrap(), None, None), Some(direct));
}

#[cfg(windows)]
#[test]
fn windows_shells_are_git_for_windows_not_wsl() {
    let root = tempfile::tempdir().unwrap();
    let dir = |parts: &[&str]| {
        let dir = parts.iter().fold(root.path().to_path_buf(), |dir, part| dir.join(part));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    };
    let (system, git_cmd, git_bin, usr_bin) =
        (dir(&["Windows", "System32"]), dir(&["Git", "cmd"]), dir(&["Git", "bin"]), dir(&["Git", "usr", "bin"]));
    for file in
        [system.join("bash.exe"), git_cmd.join("git.exe"), git_bin.join("bash.exe"), git_bin.join("sh.exe"), usr_bin.join("bash.exe")]
    {
        std::fs::write(file, "").unwrap();
    }
    let path = |dirs: &[&PathBuf]| std::env::join_paths(dirs).unwrap().to_string_lossy().into_owned();
    // Git's default installation: only `Git\cmd` on `PATH`, after `System32`.
    assert_eq!(find_shell("bash", &path(&[&system, &git_cmd])), Some(git_bin.join("bash.exe")));
    assert_eq!(find_shell("sh", &path(&[&system, &git_cmd])), Some(git_bin.join("sh.exe")));
    // A bash put on `PATH` by hand still wins, as long as it is not WSL's.
    assert_eq!(find_shell("bash", &path(&[&system, &usr_bin, &git_cmd])), Some(usr_bin.join("bash.exe")));
    assert_eq!(find_shell("bash", &path(&[&system])), None);
    assert_eq!(find_shell("pwsh", &path(&[&system, &git_cmd])), None, "other shells are looked up as before");

    // A script's own `bash` is Git's too: its directory goes first.
    let default = path(&[&system, &git_cmd]);
    assert_eq!(with_git_bash_first(&default), Some(path(&[&git_bin, &system, &git_cmd])));
    assert_eq!(with_git_bash_first(&path(&[&git_bin, &system])), None, "already first");
    assert_eq!(with_git_bash_first(&path(&[&system])), None, "no Git");
}

#[tokio::test]
async fn long_lines_are_cut_between_characters() {
    let lines = |text: &'static str, max_line| async move {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
        read_lines(Box::new(text.as_bytes()), max_line, sender).await;
        let mut lines = Vec::new();
        while let Some(line) = receiver.recv().await {
            lines.push(line);
        }
        lines
    };
    assert_eq!(lines("abcdéfgh\r\nnext\nlast", 5).await, ["abcd", "éfgh", "next", "last"], "é is two bytes");
    assert_eq!(lines("ab€cd\n", 4).await, ["ab", "€c", "d"]);
    assert_eq!(lines("short\n", MAX_LINE_BYTES).await, ["short"]);
    const { assert!(MAX_LINE_BYTES > rbit_workflow::protocol::MAX_LOG_LINE_BYTES * 8, "the log cuts lines after masking them") };
}

#[cfg(unix)]
#[tokio::test]
async fn a_stopped_step_takes_down_a_background_process_that_ignores_the_polite_signal() {
    // The step's shell exits on TERM, but the background sleep ignores TERM. The group
    // must still get the forceful signal once the grace period is over.
    let mut command = command("sh");
    command.args(["-c", "(trap '' TERM; exec sleep 30) & echo $!; trap 'exit 0' TERM; wait"]);
    isolate(&mut command);
    let mut lines = Vec::new();
    let stop = tokio::time::sleep(Duration::from_secs(1));
    let exit = run_streaming(command, MAX_LINE_BYTES, stop, &Stopper::Host, |line| lines.push(line)).await.unwrap();
    assert_eq!(exit, Exit::Stopped);
    let pid = lines.first().expect("the background pid was printed").clone();
    // A killed process stays a zombie until whoever inherited it reaps it, and `kill -0`
    // still reaches a zombie: alive means listed by `ps` in a state other than `Z`.
    let running = || {
        let state = std::process::Command::new("ps").args(["-o", "stat=", "-p", pid.as_str()]).output().unwrap();
        String::from_utf8_lossy(&state.stdout).trim().chars().next().is_some_and(|state| state != 'Z')
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while running() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let alive = running();
    if alive {
        // Do not leave the sleep behind when the test fails.
        let _ = std::process::Command::new("kill").args(["-KILL", pid.as_str()]).status();
    }
    assert!(!alive, "the background sleep {pid} survived the stop");
}

#[tokio::test]
async fn output_is_captured_and_bounded() {
    let Some(git) = find_program("git") else { return };
    let output = output(&git, &["--version"], Duration::from_secs(30)).await.unwrap();
    assert!(output.success && output.stdout.starts_with("git version"), "{output:?}");
}
