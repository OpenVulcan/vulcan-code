use super::super::{
    PersistedPromptConfiguration, ProjectVulcanCodeConfigurationFile,
    load_project_configuration_file, project_configuration_file_path,
    save_project_configuration_file,
};
use super::helpers::unique_temp_dir;
use std::fs;
use vulcan_vmm::VmmConfigLayer;

/// Verifies a confirmed missing project configuration resolves to the typed default.
/// 验证已确认缺失的项目配置会解析为强类型默认值。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if absence becomes an error or a nondefault configuration.
/// 此测试无返回值；若路径缺失变成错误或非默认配置则失败。
#[test]
fn missing_project_configuration_returns_default() {
    // root is absent so the loader exercises only the confirmed-missing contract.
    // root 不存在，因此加载器只执行已确认缺失契约。
    let root = unique_temp_dir("vulcan-project-config-missing");

    // loaded is the typed default produced for the absent optional file.
    // loaded 是为缺失可选文件生成的强类型默认值。
    let loaded = load_project_configuration_file(&root).expect("missing config should load");

    assert_eq!(loaded, ProjectVulcanCodeConfigurationFile::default());
    assert!(!root.exists());
}

/// Verifies malformed project configuration diagnostics retain the exact source path.
/// 验证格式错误的项目配置诊断会保留精确来源路径。
///
/// # Parameters
/// # 参数
/// This test has no parameters.
/// 此测试没有参数。
///
/// # Returns
/// # 返回值
/// This test returns no value and fails if invalid JSON is accepted or loses its file identity.
/// 此测试无返回值；若无效 JSON 被接受或丢失文件身份则失败。
#[test]
fn invalid_project_configuration_reports_exact_path() {
    // root isolates the malformed configuration fixture from all other tests.
    // root 将格式错误的配置夹具与其他所有测试隔离。
    let root = unique_temp_dir("vulcan-project-config-invalid");
    // configuration_path is the sole file consumed by the project loader.
    // configuration_path 是项目加载器消费的唯一文件。
    let configuration_path = project_configuration_file_path(&root);
    fs::create_dir_all(
        configuration_path
            .parent()
            .expect("project configuration should have a parent"),
    )
    .expect("create project configuration directory");
    fs::write(&configuration_path, b"{invalid-json").expect("write invalid project configuration");

    // error must identify the exact malformed file instead of degrading to defaults.
    // error 必须标识精确的格式错误文件，而不是降级为默认值。
    let error = load_project_configuration_file(&root)
        .expect_err("invalid project configuration should fail");

    assert!(error.contains(&configuration_path.display().to_string()));
    fs::remove_dir_all(root).expect("remove invalid project configuration fixture");
}

/// Verifies project-local configuration files round-trip through the shared provider config helpers.
/// 验证项目本地配置文件可以通过共享 Provider 配置辅助函数完成读写往返。
#[test]
fn project_configuration_file_roundtrips_prompt_settings() {
    let root = unique_temp_dir("vulcan-project-config-roundtrip");
    let config = ProjectVulcanCodeConfigurationFile {
        prompt: Some(PersistedPromptConfiguration {
            project_prompt_enabled: false,
            project_prompt_path: String::from(".vulcan/code/local-prompt.md"),
            ..PersistedPromptConfiguration::default()
        }),
        prompt_skills: Default::default(),
        vmm: Default::default(),
    };

    save_project_configuration_file(&root, &config).expect("save project config");
    let loaded = load_project_configuration_file(&root).expect("load project config");

    assert_eq!(loaded, config);
    assert!(
        project_configuration_file_path(&root)
            .to_string_lossy()
            .ends_with(".vulcan\\code\\config.json")
            || project_configuration_file_path(&root)
                .to_string_lossy()
                .ends_with(".vulcan/code/config.json")
    );

    let _ = fs::remove_dir_all(root);
}

/// Verifies project-local VMM overrides round-trip through the shared project configuration helpers.
/// 验证项目级 VMM 覆盖配置可以通过共享项目配置辅助函数完成读写往返。
#[test]
fn project_configuration_file_roundtrips_vmm_settings() {
    let root = unique_temp_dir("vulcan-project-vmm-config-roundtrip");
    let config = ProjectVulcanCodeConfigurationFile {
        prompt: None,
        prompt_skills: Default::default(),
        vmm: VmmConfigLayer {
            enabled: Some(false),
            project_id: Some(1001),
            receive_timeout_ms: Some(30_000),
            implicit_memory_turns: Some(5),
            ..VmmConfigLayer::default()
        },
    };

    save_project_configuration_file(&root, &config).expect("save project config");
    let loaded = load_project_configuration_file(&root).expect("load project config");

    assert_eq!(loaded, config);
    assert_eq!(loaded.vmm.project_id, Some(1001));
    assert_eq!(loaded.vmm.receive_timeout_ms, Some(30_000));

    let _ = fs::remove_dir_all(root);
}
