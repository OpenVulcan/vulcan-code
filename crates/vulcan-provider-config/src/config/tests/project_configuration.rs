use super::super::{
    PersistedPromptConfiguration, ProjectVulcanCodeConfigurationFile,
    load_project_configuration_file, project_configuration_file_path,
    save_project_configuration_file,
};
use super::helpers::unique_temp_dir;
use std::fs;
use vulcan_vmm::VmmConfigLayer;

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
