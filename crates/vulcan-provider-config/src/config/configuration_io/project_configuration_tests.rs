use super::{
    ProjectVulcanCodeConfigurationFile, load_project_configuration_file,
    project_configuration_file_path, save_project_configuration_file,
};
use std::fs;
use vulcan_vmm::VmmConfigLayer;

/// Verifies a confirmed missing project configuration resolves to the typed default.
/// 验证已确认缺失的项目配置会解析为强类型默认值。
#[test]
fn missing_project_configuration_returns_default() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("absent");

    let loaded = load_project_configuration_file(&root).expect("missing config should load");

    assert_eq!(loaded, ProjectVulcanCodeConfigurationFile::default());
    assert!(!root.exists());
}

/// Verifies malformed project configuration diagnostics retain the exact source path.
/// 验证格式错误的项目配置诊断会保留精确来源路径。
#[test]
fn invalid_project_configuration_reports_exact_path() {
    let temp = tempfile::tempdir().expect("temp dir");
    let configuration_path = project_configuration_file_path(temp.path());
    fs::create_dir_all(
        configuration_path
            .parent()
            .expect("project configuration should have a parent"),
    )
    .expect("create project configuration directory");
    fs::write(&configuration_path, b"{invalid-json").expect("write invalid project configuration");

    let error = load_project_configuration_file(temp.path())
        .expect_err("invalid project configuration should fail");

    assert!(error.contains(&configuration_path.display().to_string()));
}

/// Verifies project-local configuration files round-trip at `.vulcan/code/config.json`.
/// 验证项目本地配置文件在 `.vulcan/code/config.json` 完成读写往返。
#[test]
fn project_configuration_file_roundtrips_default_settings() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config = ProjectVulcanCodeConfigurationFile::default();

    save_project_configuration_file(temp.path(), &config).expect("save project config");
    let loaded = load_project_configuration_file(temp.path()).expect("load project config");

    assert_eq!(loaded, config);
    assert_eq!(
        project_configuration_file_path(temp.path()),
        temp.path().join(".vulcan").join("code").join("config.json")
    );
}

/// Verifies project-local VMM overrides round-trip through the project configuration helpers.
/// 验证项目级 VMM 覆盖配置可以通过项目配置辅助函数完成读写往返。
#[test]
fn project_configuration_file_roundtrips_vmm_settings() {
    let temp = tempfile::tempdir().expect("temp dir");
    let config = ProjectVulcanCodeConfigurationFile {
        vmm: VmmConfigLayer {
            enabled: Some(false),
            project_id: Some(1001),
            receive_timeout_ms: Some(30_000),
            implicit_memory_turns: Some(5),
            ..VmmConfigLayer::default()
        },
        ..ProjectVulcanCodeConfigurationFile::default()
    };

    save_project_configuration_file(temp.path(), &config).expect("save project config");
    let loaded = load_project_configuration_file(temp.path()).expect("load project config");

    assert_eq!(loaded, config);
}
