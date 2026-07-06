use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Creates one isolated temporary directory for prompt skill tests.
/// 为提示词技能测试创建一个隔离临时目录。
fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("vulcan-prompt-skills-{suffix}"));
    fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

#[test]
fn parses_skill_frontmatter_and_body() {
    let path = PathBuf::from("demo/SKILL.md");
    let skill = parse_prompt_skill_text(
        &path,
        "---\nname: demo\ndescription: Use for demo tasks.\n---\n\n# Demo\nBody",
    )
    .expect("skill should parse");
    assert_eq!(skill.name, "demo");
    assert_eq!(skill.description, "Use for demo tasks.");
    assert_eq!(skill.body, "# Demo\nBody");
    assert!(skill.interface.is_none());
}

#[test]
fn loads_only_user_root_skill_directories() {
    let root = temp_dir();
    let skill_dir = root.join("demo");
    let system_dir = root.join(SYSTEM_SKILL_DIR_NAME);
    fs::create_dir_all(&skill_dir).expect("skill dir should be created");
    fs::create_dir_all(&system_dir).expect("system dir should be created");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: demo\ndescription: Use for demo tasks.\n---\n\n# Demo",
    )
    .expect("skill file should be written");
    fs::write(
        system_dir.join(SKILL_FILE_NAME),
        "---\nname: invalid\ndescription: Should be ignored.\n---\n\n# Invalid",
    )
    .expect("system root file should be written");
    fs::write(root.join(SKILL_FILE_NAME), "ignored").expect("root file should be written");

    let catalog = load_prompt_skills_from_user_root(root);
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].name, "demo");
    assert_eq!(catalog.skills[0].scope, PromptSkillScope::User);
    assert!(catalog.errors.is_empty());
}

#[test]
fn loads_root_vulcan_interface_metadata() {
    let root = temp_dir();
    let skill_dir = root.join("demo");
    fs::create_dir_all(skill_dir.join("assets")).expect("assets dir should be created");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: demo\ndescription: Use for demo tasks.\n---\n\n# Demo",
    )
    .expect("skill file should be written");
    fs::write(
        skill_dir.join(VULCAN_SKILL_METADATA_FILE_NAME),
        "interface:\n  display_name: \"Vulcan Demo\"\n  short_description: \"Vulcan description\"\n  icon_small: \"./assets/demo.svg\"\n",
    )
    .expect("vulcan metadata should be written");

    let catalog = load_prompt_skills_from_user_root(root);
    let interface = catalog.skills[0]
        .interface
        .as_ref()
        .expect("interface metadata should load");
    assert_eq!(interface.display_name.as_deref(), Some("Vulcan Demo"));
    assert_eq!(
        interface.short_description.as_deref(),
        Some("Vulcan description")
    );
    assert_eq!(
        interface.icon_small.as_deref(),
        Some(Path::new("./assets/demo.svg"))
    );
}

#[test]
fn loads_user_skill_over_system_skill_with_same_name() {
    let root = temp_dir();
    let system_root = root.join(SYSTEM_SKILL_DIR_NAME);
    let system_skill_dir = system_root.join("demo");
    let user_skill_dir = root.join("demo");
    fs::create_dir_all(&system_skill_dir).expect("system skill dir should be created");
    fs::create_dir_all(&user_skill_dir).expect("user skill dir should be created");
    fs::write(
        system_skill_dir.join(SKILL_FILE_NAME),
        "---\nname: demo\ndescription: System demo.\n---\n\n# System",
    )
    .expect("system skill should be written");
    fs::write(
        user_skill_dir.join(SKILL_FILE_NAME),
        "---\nname: demo\ndescription: User demo.\n---\n\n# User",
    )
    .expect("user skill should be written");

    let catalog = load_prompt_skills_from_roots(system_root, root);
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].scope, PromptSkillScope::User);
    assert_eq!(catalog.skills[0].body, "# User");
}

#[test]
fn installs_embedded_system_prompt_skills_with_marker_skip() {
    let root = temp_dir().join(SYSTEM_SKILL_DIR_NAME);
    let first = install_embedded_system_prompt_skills_to_root(root.clone())
        .expect("system skills should install");
    assert!(first.installed);
    assert!(root.join("skill-creator").join(SKILL_FILE_NAME).is_file());

    let second = install_embedded_system_prompt_skills_to_root(root)
        .expect("system skills should skip unchanged install");
    assert!(!second.installed);
    assert_eq!(first.fingerprint, second.fingerprint);
}

#[test]
fn installs_system_prompt_skills_from_packaged_source() {
    let root = temp_dir();
    let source_root = root.join("packaged").join("prompt-skills").join("system");
    let source_skill_dir = source_root.join("packaged-skill");
    fs::create_dir_all(source_skill_dir.join("scripts"))
        .expect("source skill dir should be created");
    fs::write(
        source_skill_dir.join(SKILL_FILE_NAME),
        "---\nname: packaged-skill\ndescription: Use for packaged tests.\n---\n\n# Packaged",
    )
    .expect("source skill should be written");
    fs::write(source_skill_dir.join("scripts").join("helper.txt"), "ok")
        .expect("source resource should be written");

    let system_root = root.join(SYSTEM_SKILL_DIR_NAME);
    let first = install_system_prompt_skills_from_source(source_root.clone(), system_root.clone())
        .expect("packaged system skills should install");
    assert!(first.installed);
    assert_eq!(first.skill_names, vec![String::from("packaged-skill")]);
    assert!(
        system_root
            .join("packaged-skill")
            .join("scripts")
            .join("helper.txt")
            .is_file()
    );

    let second = install_system_prompt_skills_from_source(source_root, system_root)
        .expect("unchanged packaged system skills should skip");
    assert!(!second.installed);
    assert_eq!(first.fingerprint, second.fingerprint);
}

#[test]
fn resolves_session_over_project_over_global() {
    let catalog = PromptSkillCatalog {
        user_root: PathBuf::from("."),
        system_root: PathBuf::from("./.system"),
        skills: vec![PromptSkill {
            name: String::from("demo"),
            description: String::from("Use for demo tasks."),
            skill_file: PathBuf::from("demo/SKILL.md"),
            base_dir: PathBuf::from("demo"),
            scope: PromptSkillScope::User,
            interface: None,
            body: String::from("# Demo"),
        }],
        errors: Vec::new(),
    };
    let mut global = PromptSkillsGlobalConfig::default();
    global
        .skill_overrides
        .insert(String::from("demo"), PromptSkillState::Disabled);
    let mut project = PromptSkillsProjectConfig::default();
    project
        .skill_overrides
        .insert(String::from("demo"), PromptSkillState::Enabled);
    let mut session = PromptSkillsSessionConfig::default();
    session
        .skill_overrides
        .insert(String::from("demo"), PromptSkillState::Disabled);

    let resolved = resolve_prompt_skills(&catalog, &global, &project, &session);
    assert_eq!(resolved[0].state, PromptSkillState::Disabled);
    assert_eq!(resolved[0].source, PromptSkillStateSource::Session);
}

/// Verifies prompt skill descriptions collapse repeated whitespace for compact instructions.
/// 验证提示词技能描述会折叠重复空白以生成紧凑指令。
#[test]
fn collapse_whitespace_compacts_skill_descriptions() {
    assert_eq!(
        collapse_whitespace("  Use\n\tfor   compact   skill descriptions.  "),
        "Use for compact skill descriptions."
    );
    assert_eq!(collapse_whitespace(" \n\t "), "");
}

#[test]
fn renders_only_enabled_skills() {
    let skills = vec![
        ResolvedPromptSkill {
            skill: PromptSkill {
                name: String::from("enabled"),
                description: String::from("Use for enabled tasks."),
                skill_file: PathBuf::from("enabled/SKILL.md"),
                base_dir: PathBuf::from("enabled"),
                scope: PromptSkillScope::User,
                interface: None,
                body: String::new(),
            },
            state: PromptSkillState::Enabled,
            source: PromptSkillStateSource::GlobalDefault,
        },
        ResolvedPromptSkill {
            skill: PromptSkill {
                name: String::from("disabled"),
                description: String::from("Use for disabled tasks."),
                skill_file: PathBuf::from("disabled/SKILL.md"),
                base_dir: PathBuf::from("disabled"),
                scope: PromptSkillScope::User,
                interface: None,
                body: String::new(),
            },
            state: PromptSkillState::Disabled,
            source: PromptSkillStateSource::Session,
        },
    ];
    let rendered = render_prompt_skills_instructions(&skills).expect("instructions expected");
    assert!(rendered.contains("enabled"));
    assert!(!rendered.contains("disabled"));
    assert!(rendered.contains("<skills_instructions>"));
}
