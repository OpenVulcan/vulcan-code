use super::*;

/// Encodes a command line the way Windows passes it.
/// 按 Windows 传递的方式编码命令行。
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Lays out an install root whose current version provides both executables.
/// 布置一个安装根目录，其当前版本提供两个可执行文件。
fn installed(root: &Path, current_path: &str) -> PathBuf {
    let version_dir = root.join("versions").join("0.1.3");
    std::fs::create_dir_all(version_dir.join("bin")).expect("bin");
    std::fs::create_dir_all(version_dir.join("desktop")).expect("desktop");
    std::fs::write(version_dir.join("bin").join("vulcan-tui.exe"), b"").expect("tui");
    std::fs::write(version_dir.join("desktop").join("app.exe"), b"").expect("desktop");
    std::fs::write(
        version_dir.join(RUNTIME_LAYOUT_FILE),
        r#"{"desktopExecutable":"desktop/app.exe","tuiExecutable":"bin/vulcan-tui.exe"}"#,
    )
    .expect("layout");
    let current = serde_json::json!({ "version": "0.1.3", "path": current_path });
    std::fs::write(root.join(CURRENT_FILE), current.to_string()).expect("current");
    version_dir
}

#[test]
fn the_program_name_is_removed_and_the_arguments_are_kept_verbatim() {
    let cases = [
        (r#""C:\Program Files\x.exe" --a "b c""#, r#"--a "b c""#),
        (r#"x.exe   -- "q\"r""#, r#"-- "q\"r""#),
        (r#""x.exe"--flag"#, "--flag"),
        ("x.exe\t\tvalue", "value"),
        ("x.exe", ""),
        (r#""x.exe"#, ""),
        ("", ""),
    ];
    for (line, tail) in cases {
        assert_eq!(command_line_tail(&wide(line)), wide(tail), "{line}");
    }
}

#[test]
fn each_program_finds_the_root_from_its_own_location() {
    let root = PathBuf::from("app");
    assert_eq!(
        install_root(Role::Desktop, &root.join("VulcanCode.exe")),
        Ok(root.clone())
    );
    assert_eq!(
        install_root(Role::Terminal, &root.join("cli").join("vulcan-tui.exe")),
        Ok(root)
    );
    assert!(install_root(Role::Terminal, Path::new("vulcan-tui.exe")).is_err());
}

#[test]
fn each_role_starts_its_executable_from_the_current_version() {
    let temp = tempfile::tempdir().expect("temp dir");
    let separator = std::path::MAIN_SEPARATOR;
    let version_dir = installed(temp.path(), &format!("versions{separator}0.1.3"));
    assert_eq!(
        resolve_target(temp.path(), Role::Desktop),
        Ok(version_dir.join("desktop/app.exe"))
    );
    assert_eq!(
        resolve_target(temp.path(), Role::Terminal),
        Ok(version_dir.join("bin/vulcan-tui.exe"))
    );
}

#[test]
fn a_current_path_that_leaves_the_root_is_refused() {
    let temp = tempfile::tempdir().expect("temp dir");
    let outside = temp.path().join("elsewhere");
    installed(temp.path(), "../versions/0.1.3");
    assert!(resolve_target(temp.path(), Role::Desktop).is_err());
    installed(temp.path(), &outside.display().to_string());
    assert!(resolve_target(temp.path(), Role::Desktop).is_err());
}

#[test]
fn a_missing_pointer_layout_or_executable_is_an_error() {
    let temp = tempfile::tempdir().expect("temp dir");
    assert!(resolve_target(temp.path(), Role::Desktop).is_err());
    let separator = std::path::MAIN_SEPARATOR;
    let version_dir = installed(temp.path(), &format!("versions{separator}0.1.3"));
    std::fs::remove_file(version_dir.join("bin").join("vulcan-tui.exe")).expect("remove tui");
    assert!(resolve_target(temp.path(), Role::Terminal).is_err());
    assert!(resolve_target(temp.path(), Role::Desktop).is_ok());
    std::fs::write(version_dir.join(RUNTIME_LAYOUT_FILE), "{}").expect("layout");
    assert!(resolve_target(temp.path(), Role::Desktop).is_err());
}

/// Installs 0.1.3 and 0.1.4, makes 0.1.4 current and puts it on trial.
/// 安装 0.1.3 与 0.1.4，将 0.1.4 设为当前版本并进入试运行。
fn on_trial(root: &Path, attempts: u64) {
    let separator = std::path::MAIN_SEPARATOR;
    installed(root, &format!("versions{separator}0.1.3"));
    std::fs::create_dir_all(root.join("versions").join("0.1.4")).expect("new version");
    let current =
        serde_json::json!({ "version": "0.1.4", "path": format!("versions{separator}0.1.4") });
    std::fs::write(root.join(CURRENT_FILE), current.to_string()).expect("current");
    std::fs::create_dir_all(root.join("updates")).expect("updates");
    let trial = serde_json::json!({
        "version": "0.1.4", "previous": "0.1.3", "attempts": attempts, "max_attempts": 2
    });
    std::fs::write(
        root.join("updates").join("startup-trial.json"),
        trial.to_string(),
    )
    .expect("trial");
}

/// Reads one JSON file of the install root.
/// 读取安装根目录中的一个 JSON 文件。
fn json_at(path: PathBuf) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json")
}

#[test]
fn launches_without_a_trial_change_nothing() {
    let temp = tempfile::tempdir().expect("temp dir");
    installed(temp.path(), "versions/0.1.3");
    assert_eq!(trial::advance(temp.path()), Ok(trial::TrialOutcome::Idle));
    assert!(!temp.path().join("updates").exists());
}

#[test]
fn a_version_on_trial_gets_its_attempts_then_the_previous_version_returns() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    on_trial(root, 0);
    for expected in [1, 2] {
        assert_eq!(trial::advance(root), Ok(trial::TrialOutcome::Counted));
        let trial = json_at(root.join("updates").join("startup-trial.json"));
        assert_eq!(trial["attempts"], expected);
        assert_eq!(json_at(root.join(CURRENT_FILE))["version"], "0.1.4");
    }
    assert_eq!(trial::advance(root), Ok(trial::TrialOutcome::RolledBack));
    assert_eq!(json_at(root.join(CURRENT_FILE))["version"], "0.1.3");
    assert_eq!(
        json_at(root.join("updates").join("rollback.json")),
        serde_json::json!({ "failed": "0.1.4", "restored": "0.1.3" })
    );
    assert!(!root.join("updates").join("startup-trial.json").exists());
    assert!(resolve_target(root, Role::Desktop).is_ok());
}

#[test]
fn a_trial_without_its_previous_version_is_an_error() {
    let temp = tempfile::tempdir().expect("temp dir");
    on_trial(temp.path(), 2);
    std::fs::remove_dir_all(temp.path().join("versions").join("0.1.3")).expect("remove previous");
    assert!(trial::advance(temp.path()).is_err());
    assert_eq!(json_at(temp.path().join(CURRENT_FILE))["version"], "0.1.4");
}

#[test]
fn a_trial_of_a_version_that_is_no_longer_current_is_dropped() {
    let temp = tempfile::tempdir().expect("temp dir");
    on_trial(temp.path(), 2);
    installed(temp.path(), "versions/0.1.3");
    assert_eq!(trial::advance(temp.path()), Ok(trial::TrialOutcome::Idle));
    assert!(
        !temp
            .path()
            .join("updates")
            .join("startup-trial.json")
            .exists()
    );
    assert!(!temp.path().join("updates").join("rollback.json").exists());
}

#[cfg(windows)]
#[test]
fn a_running_core_of_either_version_defers_the_trial() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path();
    on_trial(root, 2);
    let previous = root.join("versions").join("0.1.3");
    std::fs::write(
        previous.join(RUNTIME_LAYOUT_FILE),
        r#"{"desktopExecutable":"desktop/app.exe","tuiExecutable":"bin/vulcan-tui.exe","serviceExecutable":"bin/vulcan-code-service.exe"}"#,
    )
    .expect("layout");
    // A copy of a long-running system program stands in for the previous version's resident Core.
    // 用一个长时间运行的系统程序副本代替上一版本常驻的 Core。
    let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot"));
    let service = previous.join("bin").join("vulcan-code-service.exe");
    std::fs::copy(system.join("System32").join("ping.exe"), &service).expect("copy");
    let mut core = std::process::Command::new(&service)
        .args(["-n", "60", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("start");
    let deferred = trial::advance(root);
    core.kill().expect("stop");
    core.wait().expect("wait");
    assert_eq!(deferred, Ok(trial::TrialOutcome::CoreRunning));
    assert_eq!(json_at(root.join(CURRENT_FILE))["version"], "0.1.4");
    assert_eq!(
        json_at(root.join("updates").join("startup-trial.json"))["attempts"],
        2
    );
    // Once no Core runs, the used-up trial ends as before.
    // 没有 Core 运行后，已用尽的试运行照常结束。
    assert_eq!(trial::advance(root), Ok(trial::TrialOutcome::RolledBack));
}
