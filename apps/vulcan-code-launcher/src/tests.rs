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
