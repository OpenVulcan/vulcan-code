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

/// Verifies that the canonical user skill root lives below the Vulcan Code data directory.
/// 验证规范用户技能根目录位于 Vulcan Code 数据目录下。
#[test]
fn default_user_skill_root_uses_vulcan_code_directory() {
    assert!(default_user_skill_root().ends_with(Path::new(".vulcan/code/skills")));
}

/// Verifies the packaged RunLua guide exists in the program system-skill source and separates both tool families.
/// 验证打包 RunLua 指引存在于程序系统 Skill 源目录，并区分两类工具。
#[test]
fn packaged_runlua_skill_requires_native_skill_lookup() {
    let skill_file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("assets")
        .join("system")
        .join("runlua")
        .join(SKILL_FILE_NAME);
    let content = fs::read_to_string(skill_file).expect("packaged RunLua Skill should be readable");
    assert!(content.contains("{\"name\":\"runlua\"}"));
    assert!(content.contains("The tool id is `skill`, singular."));
    assert!(content.contains("Do not call `luaskill`"));
    assert!(content.contains("LuaSkills: `0.5.3`"));
}

/// Builds one complete user prompt skill fixture for policy and rendering tests.
/// 为策略与渲染测试构建一个完整用户提示词技能夹具。
fn prompt_skill_fixture(name: &str, description: &str) -> PromptSkill {
    PromptSkill {
        name: name.to_string(),
        description: description.to_string(),
        license: None,
        compatibility: None,
        metadata: BTreeMap::new(),
        skill_file: PathBuf::from(name).join(SKILL_FILE_NAME),
        base_dir: PathBuf::from(name),
        scope: PromptSkillScope::User,
        interface: PromptSkillInterfaceMetadata::default(),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("uncategorized"),
            tags: Vec::new(),
        },
        origin: PromptSkillOriginMetadata {
            kind: String::from("unknown"),
            ..PromptSkillOriginMetadata::default()
        },
        resources: PromptSkillResourceSummary::default(),
        body: String::new(),
    }
}

/// Builds the explicit host-assigned provenance used by managed package tests.
/// 构建受管技能包测试使用的明确宿主分配来源信息。
fn managed_test_origin() -> PromptSkillOriginMetadata {
    PromptSkillOriginMetadata {
        kind: String::from("local-created"),
        label: Some(String::from("User-created")),
        managed: false,
        ..PromptSkillOriginMetadata::default()
    }
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
    assert_eq!(skill.interface, PromptSkillInterfaceMetadata::default());
}

/// Verifies package loading freezes deterministic relative paths for model-facing detail output.
/// 验证技能包加载会为面向模型的详情输出冻结确定的相对路径。
#[test]
fn freezes_prompt_skill_resource_paths_during_loading() {
    // root isolates one real package containing a nested script and reference.
    // root 隔离一个包含嵌套脚本与参考资料的真实技能包。
    let root = temp_dir();
    let skill_dir = root.join("resource-helper");
    fs::create_dir_all(skill_dir.join("scripts/nested"))
        .expect("nested scripts directory should create");
    fs::create_dir_all(skill_dir.join("references")).expect("references directory should create");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: resource-helper\ndescription: Use for resource tests.\n---\n\n# Resource helper",
    )
    .expect("skill instructions should write");
    fs::write(skill_dir.join("scripts/nested/run.ps1"), "Write-Output ok")
        .expect("script should write");
    fs::write(skill_dir.join("references/guide.md"), "# Guide").expect("reference should write");

    let skill = parse_prompt_skill_file(&skill_dir.join(SKILL_FILE_NAME))
        .expect("resource-bearing skill should load");

    assert_eq!(skill.resources.scripts, 1);
    assert_eq!(skill.resources.references, 1);
    assert_eq!(skill.resources.assets, 0);
    assert_eq!(
        skill.resources.sampled_files,
        vec![
            PathBuf::from("references/guide.md"),
            PathBuf::from("scripts/nested/run.ps1"),
        ]
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies Unix symbolic-link resources invalidate the package instead of being followed.
/// 验证 Unix 符号链接资源会使技能包失效而不会被跟随。
#[cfg(unix)]
#[test]
fn rejects_symbolic_link_resources_during_loading() {
    use std::os::unix::fs::symlink;

    // root isolates the package and an outside file that must never enter its snapshot.
    // root 隔离技能包及绝不能进入其快照的外部文件。
    let root = temp_dir();
    let skill_dir = root.join("linked-helper");
    fs::create_dir_all(skill_dir.join("scripts")).expect("scripts directory should create");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: linked-helper\ndescription: Use for link tests.\n---\n\n# Linked helper",
    )
    .expect("skill instructions should write");
    let outside = root.join("outside.sh");
    fs::write(&outside, "echo outside").expect("outside file should write");
    symlink(&outside, skill_dir.join("scripts/run.sh")).expect("resource symlink should create");

    let error = parse_prompt_skill_file(&skill_dir.join(SKILL_FILE_NAME))
        .expect_err("linked resource must invalidate the package");

    assert!(error.message.contains("Symbolic links"));
    let _ = fs::remove_dir_all(root);
}

/// Verifies Windows junction resources invalidate the package instead of being traversed.
/// 验证 Windows 联接点资源会使技能包失效而不会被遍历。
#[cfg(windows)]
#[test]
fn rejects_windows_reparse_point_resources_during_loading() {
    // root isolates the package and an outside directory behind a junction.
    // root 隔离技能包及联接点背后的外部目录。
    let root = temp_dir();
    let skill_dir = root.join("linked-helper");
    let outside = root.join("outside-scripts");
    fs::create_dir_all(&skill_dir).expect("skill directory should create");
    fs::create_dir_all(&outside).expect("outside directory should create");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: linked-helper\ndescription: Use for link tests.\n---\n\n# Linked helper",
    )
    .expect("skill instructions should write");
    fs::write(outside.join("run.ps1"), "Write-Output outside")
        .expect("outside script should write");
    let junction = skill_dir.join("scripts");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .status()
        .expect("junction command should run");
    assert!(status.success(), "junction should be created for the test");

    let error = parse_prompt_skill_file(&skill_dir.join(SKILL_FILE_NAME))
        .expect_err("junction resource must invalidate the package");

    assert!(error.message.contains("reparse points"));
    fs::remove_dir(&junction).expect("junction should be removed without touching its target");
    let _ = fs::remove_dir_all(root);
}

/// Verifies package identity whitespace is rejected instead of being silently normalized.
/// 验证技能包身份空白会被拒绝而不是被静默规范化。
#[test]
fn rejects_noncanonical_prompt_skill_identity_whitespace() {
    // path matches the canonical directory name so only frontmatter whitespace is under test.
    // path 匹配规范目录名称，因此仅测试 frontmatter 空白。
    let path = PathBuf::from("demo/SKILL.md");
    let error = parse_prompt_skill_text(
        &path,
        "---\nname: ' demo '\ndescription: Use for demo tasks.\n---\n\n# Demo",
    )
    .expect_err("surrounding identity whitespace must be rejected");

    assert!(
        error
            .message
            .contains("cannot contain surrounding whitespace")
    );
}

/// Verifies managed deletion cannot target a normalized alias of a real package.
/// 验证受管删除不能通过真实技能包的规范化别名定位目标。
#[test]
fn rejects_noncanonical_managed_delete_name() {
    // root isolates the user package that must remain untouched.
    // root 隔离必须保持不变的用户技能包。
    let root = temp_dir();
    let skill_dir = root.join("demo");
    fs::create_dir_all(&skill_dir).expect("skill directory should create");

    let error = delete_user_prompt_skill_package(&root, " demo ")
        .expect_err("noncanonical deletion identity must be rejected");

    assert!(error.contains("cannot contain surrounding whitespace"));
    assert!(skill_dir.is_dir());
    let _ = fs::remove_dir_all(root);
}

/// Verifies managed deletion rejects a Unix symlink package without touching its target.
/// 验证受管删除会拒绝 Unix 符号链接技能包且不触碰其目标。
#[cfg(unix)]
#[test]
fn rejects_linked_managed_delete_target() {
    use std::os::unix::fs::symlink;

    // root contains only a linked package name while protected owns the real sentinel.
    // root 仅包含链接形式的技能包名，protected 持有真实哨兵文件。
    let root = temp_dir();
    let protected = temp_dir();
    fs::write(protected.join("sentinel.txt"), "keep").expect("sentinel should write");
    symlink(&protected, root.join("demo")).expect("package symlink should create");

    let error = delete_user_prompt_skill_package(&root, "demo")
        .expect_err("linked package deletion must be rejected");

    assert!(error.contains("cannot be a symbolic link"));
    assert_eq!(
        fs::read_to_string(protected.join("sentinel.txt")).expect("sentinel should remain"),
        "keep"
    );
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(protected);
}

/// Verifies managed deletion rejects a Windows junction package without touching its target.
/// 验证受管删除会拒绝 Windows 联接点技能包且不触碰其目标。
#[cfg(windows)]
#[test]
fn rejects_linked_managed_delete_target() {
    // root contains only a junction package name while protected owns the real sentinel.
    // root 仅包含联接点形式的技能包名，protected 持有真实哨兵文件。
    let root = temp_dir();
    let protected = temp_dir();
    fs::write(protected.join("sentinel.txt"), "keep").expect("sentinel should write");
    let linked_package = root.join("demo");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&linked_package)
        .arg(&protected)
        .status()
        .expect("junction command should run");
    assert!(status.success(), "junction should be created for the test");

    let error = delete_user_prompt_skill_package(&root, "demo")
        .expect_err("junction package deletion must be rejected");

    assert!(error.contains("Windows reparse point"));
    assert_eq!(
        fs::read_to_string(protected.join("sentinel.txt")).expect("sentinel should remain"),
        "keep"
    );
    fs::remove_dir(&linked_package)
        .expect("junction should be removed without touching its target");
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(protected);
}

/// Verifies managed writes reject traversal identities before creating anything outside the root.
/// 验证受管写入会在用户根目录外创建任何内容之前拒绝路径穿越标识。
#[test]
fn rejects_managed_write_path_traversal() {
    // root isolates the authorized user package boundary.
    // root 隔离获授权的用户技能包边界。
    let root = temp_dir();
    // sibling is the exact unauthorized path a naive join would target.
    // sibling 是朴素路径拼接会命中的精确未授权路径。
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .expect("temporary root should have a UTF-8 file name");
    let sibling_name = format!("{root_name}-escaped");
    let sibling = root
        .parent()
        .expect("temporary root should have a parent")
        .join(&sibling_name);
    // input deliberately attempts to escape through the package identity.
    // input 故意尝试通过技能包标识逃逸根目录。
    let input = PromptSkillPackageInput {
        name: format!("../{sibling_name}"),
        description: String::from("Use when traversal protection is tested."),
        body: String::from("# Traversal"),
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };

    let error = save_user_prompt_skill_package(&root, input)
        .expect_err("path traversal identity must be rejected");

    assert!(error.contains("lowercase ASCII letters"));
    assert!(!sibling.exists());
    let _ = fs::remove_dir_all(root);
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
        "schema_version: 3\ninterface:\n  display_name: \"Vulcan Demo\"\n  short_description: \"Vulcan description\"\n  emoji: \"🧭\"\n  color: \"#2563EB\"\ncatalog:\n  category_id: \"general\"\norigin:\n  kind: \"local-created\"\n",
    )
    .expect("vulcan metadata should be written");

    let catalog = load_prompt_skills_from_user_root(root);
    let interface = &catalog.skills[0].interface;
    assert_eq!(interface.display_name.as_deref(), Some("Vulcan Demo"));
    assert_eq!(
        interface.short_description.as_deref(),
        Some("Vulcan description")
    );
    assert_eq!(interface.emoji.as_deref(), Some("🧭"));
    assert_eq!(interface.color.as_deref(), Some("#2563EB"));
}

/// Verifies the canonical frontmatter name must equal the skill package directory name.
/// 验证规范 frontmatter 名称必须等于技能包目录名称。
#[test]
fn rejects_skill_name_that_differs_from_package_directory() {
    // root isolates one mismatched external package scan.
    // root 隔离一个名称不匹配的外部技能包扫描。
    let root = temp_dir();
    // skill_dir deliberately differs from the canonical frontmatter name.
    // skill_dir 刻意与规范 frontmatter 名称不同。
    let skill_dir = root.join("directory-name");
    fs::create_dir_all(&skill_dir).expect("skill directory should create");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: frontmatter-name\ndescription: Use when package identity is validated.\n---\n# Identity",
    )
    .expect("skill manifest should write");

    // catalog is the strict scan result that must reject the mismatched package.
    // catalog 是必须拒绝名称不匹配技能包的严格扫描结果。
    let catalog = load_prompt_skills_from_user_root(root.clone());

    assert!(catalog.skills.is_empty());
    assert!(
        catalog
            .errors
            .iter()
            .any(|error| { error.message.contains("must match parent directory") }),
        "unexpected diagnostics: {:?}",
        catalog.errors
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies invalid current-schema locale metadata excludes the package with an explicit error.
/// 验证无效当前结构语言元数据会通过明确错误排除技能包。
#[test]
fn rejects_invalid_vulcan_locale_metadata() {
    // root isolates one invalid package scan.
    // root 隔离一次无效技能包扫描。
    let root = temp_dir();
    // skill_dir owns both the valid standard manifest and invalid Vulcan extension.
    // skill_dir 同时拥有有效标准清单与无效 Vulcan 扩展。
    let skill_dir = root.join("invalid-locale");
    fs::create_dir_all(&skill_dir).expect("skill directory should create");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: invalid-locale\ndescription: Use when invalid locale validation is tested.\n---\n# Invalid",
    )
    .expect("skill manifest should write");
    fs::write(
        skill_dir.join(VULCAN_SKILL_METADATA_FILE_NAME),
        "schema_version: 3\ninterface:\n  locales:\n    zh_CN:\n      display_name: Invalid\ncatalog:\n  category_id: general\norigin:\n  kind: local-created\n",
    )
    .expect("invalid metadata should write");
    // catalog is the authoritative strict scan result.
    // catalog 是权威严格扫描结果。
    let catalog = load_prompt_skills_from_user_root(root.clone());

    assert!(catalog.skills.is_empty());
    assert_eq!(catalog.errors.len(), 1);
    assert!(catalog.errors[0].message.contains("BCP 47"));
    let _ = fs::remove_dir_all(root);
}

/// Verifies an obsolete Vulcan metadata schema is diagnosed instead of compatibility-loaded.
/// 验证过时 Vulcan 元数据结构会被诊断而不会通过兼容方式加载。
#[test]
fn rejects_obsolete_vulcan_metadata_schema() {
    // root isolates one obsolete extension document scan.
    // root 隔离一次过时扩展文档扫描。
    let root = temp_dir();
    // skill_dir pairs a valid standard manifest with the explicitly unsupported schema version.
    // skill_dir 将有效标准清单与明确不支持的结构版本配对。
    let skill_dir = root.join("obsolete-schema");
    fs::create_dir_all(&skill_dir).expect("skill directory should create");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: obsolete-schema\ndescription: Use when obsolete schema rejection is tested.\n---\n# Obsolete",
    )
    .expect("skill manifest should write");
    fs::write(
        skill_dir.join(VULCAN_SKILL_METADATA_FILE_NAME),
        "schema_version: 1\ninterface: {}\ncatalog:\n  category_id: general\norigin:\n  kind: local-created\n",
    )
    .expect("obsolete metadata should write");
    // catalog is the strict scan result that must exclude obsolete metadata.
    // catalog 是必须排除过时元数据的严格扫描结果。
    let catalog = load_prompt_skills_from_user_root(root.clone());

    assert!(catalog.skills.is_empty());
    assert_eq!(catalog.errors.len(), 1);
    assert!(
        catalog.errors[0]
            .message
            .contains("schema_version must equal 3")
    );
    let _ = fs::remove_dir_all(root);
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
    assert_eq!(catalog.records.len(), 2);
    assert_eq!(catalog.skills[0].scope, PromptSkillScope::User);
    assert_eq!(catalog.skills[0].body, "# User");
    assert_eq!(
        catalog
            .source_record("system:demo")
            .expect("system source record should resolve")
            .scope,
        PromptSkillScope::System
    );
    assert_eq!(
        catalog
            .source_record("user:demo")
            .expect("user source record should resolve")
            .scope,
        PromptSkillScope::User
    );
    assert!(catalog.source_record("package:demo").is_none());
}

/// Verifies bundled system skills, provenance metadata, emoji presentation, and idempotent installation.
/// 验证内置系统技能、来源元数据、Emoji 展示与幂等安装行为。
#[test]
fn installs_embedded_system_prompt_skills_with_marker_skip() {
    let root = temp_dir().join(SYSTEM_SKILL_DIR_NAME);
    let first = install_embedded_system_prompt_skills_to_root(root.clone())
        .expect("system skills should install");
    assert!(first.installed);
    assert!(root.join("skill-creator").join(SKILL_FILE_NAME).is_file());
    let code_humanizer_file = root.join("code-humanizer").join(SKILL_FILE_NAME);
    assert!(code_humanizer_file.is_file());
    let code_humanizer =
        parse_prompt_skill_file_with_scope(&code_humanizer_file, PromptSkillScope::System)
            .expect("code-humanizer metadata should load");
    assert_eq!(
        code_humanizer.origin.url.as_deref(),
        Some("https://github.com/LeonardNJU/code-humanizer")
    );
    assert_eq!(code_humanizer.interface.emoji.as_deref(), Some("🧹"));
    assert_eq!(code_humanizer.resources.assets, 0);

    let second = install_embedded_system_prompt_skills_to_root(root)
        .expect("system skills should skip unchanged install");
    assert!(!second.installed);
    assert_eq!(first.fingerprint, second.fingerprint);
}

/// Verifies a Unix symlink destination cannot redirect managed system-skill replacement.
/// 验证 Unix 符号链接目标无法重定向受管系统技能替换。
#[cfg(unix)]
#[test]
fn rejects_symbolic_link_system_skill_install_root() {
    use std::os::unix::fs::symlink;

    // root isolates a protected target and the linked destination presented to the installer.
    // root 隔离受保护目标及提交给安装器的链接目标。
    let root = temp_dir();
    let protected = root.join("protected");
    fs::create_dir_all(&protected).expect("protected directory should create");
    fs::write(protected.join("sentinel.txt"), "keep").expect("sentinel should write");
    let linked_root = root.join("system-link");
    symlink(&protected, &linked_root).expect("system root symlink should create");

    let error = install_embedded_system_prompt_skills_to_root(linked_root)
        .expect_err("linked system root must be rejected");

    assert!(error.message.contains("cannot be a symbolic link"));
    assert_eq!(
        fs::read_to_string(protected.join("sentinel.txt")).expect("sentinel should remain"),
        "keep"
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies a Windows junction destination cannot redirect managed system-skill replacement.
/// 验证 Windows 联接点目标无法重定向受管系统技能替换。
#[cfg(windows)]
#[test]
fn rejects_windows_reparse_point_system_skill_install_root() {
    // root isolates a protected target and the junction presented to the installer.
    // root 隔离受保护目标及提交给安装器的联接点。
    let root = temp_dir();
    let protected = root.join("protected");
    fs::create_dir_all(&protected).expect("protected directory should create");
    fs::write(protected.join("sentinel.txt"), "keep").expect("sentinel should write");
    let linked_root = root.join("system-link");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&linked_root)
        .arg(&protected)
        .status()
        .expect("junction command should run");
    assert!(status.success(), "junction should be created for the test");

    let error = install_embedded_system_prompt_skills_to_root(linked_root.clone())
        .expect_err("junction system root must be rejected");

    assert!(error.message.contains("Windows reparse point"));
    assert_eq!(
        fs::read_to_string(protected.join("sentinel.txt")).expect("sentinel should remain"),
        "keep"
    );
    fs::remove_dir(&linked_root).expect("junction should be removed without touching its target");
    let _ = fs::remove_dir_all(root);
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
        skills: vec![prompt_skill_fixture("demo", "Use for demo tasks.")],
        records: Vec::new(),
        categories: default_prompt_skill_categories(),
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

/// Verifies committed plugin skills ignore user-configurable skill state layers.
/// 验证已提交插件技能忽略用户可配置的技能状态层级。
#[test]
fn plugin_skill_state_is_owned_only_by_plugin_lifecycle() {
    let mut skill = prompt_skill_fixture("content-plugin:review-helper", "Review changes.");
    skill.scope = PromptSkillScope::Plugin;
    skill.catalog.category_id = String::from(PLUGIN_CATEGORY_ID);
    let mut catalog = load_prompt_skills_from_roots(PathBuf::new(), PathBuf::new());
    catalog
        .merge_plugin_skills([skill])
        .expect("valid plugin skill should merge");
    let mut global = PromptSkillsGlobalConfig {
        default_skill_state: PromptSkillState::Disabled,
        ..PromptSkillsGlobalConfig::default()
    };
    global.skill_overrides.insert(
        String::from("content-plugin:review-helper"),
        PromptSkillState::Disabled,
    );
    let project = PromptSkillsProjectConfig {
        skill_overrides: BTreeMap::from([(
            String::from("content-plugin:review-helper"),
            PromptSkillState::Disabled,
        )]),
    };
    let session = PromptSkillsSessionConfig {
        skill_overrides: BTreeMap::from([(
            String::from("content-plugin:review-helper"),
            PromptSkillState::Disabled,
        )]),
    };

    let resolved = resolve_prompt_skills(&catalog, &global, &project, &session);

    assert_eq!(resolved.len(), 1);
    assert_eq!(resolved[0].state, PromptSkillState::Enabled);
    assert_eq!(resolved[0].source, PromptSkillStateSource::Plugin);
}

/// Verifies a failed plugin-skill batch leaves the target catalog unchanged.
/// 验证失败的插件技能批次保持目标目录不变。
#[test]
fn plugin_skill_merge_is_transactional() {
    let mut valid = prompt_skill_fixture("first-plugin:review-helper", "Review changes.");
    valid.scope = PromptSkillScope::Plugin;
    valid.catalog.category_id = String::from(PLUGIN_CATEGORY_ID);
    let mut invalid = valid.clone();
    invalid.name = String::from("invalid-without-namespace");
    let mut catalog = load_prompt_skills_from_roots(PathBuf::new(), PathBuf::new());

    let result = catalog.merge_plugin_skills([valid, invalid]);

    assert!(result.is_err());
    assert!(catalog.skills.is_empty());
    assert!(catalog.records.is_empty());
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
            skill: prompt_skill_fixture("enabled", "Use for enabled tasks."),
            state: PromptSkillState::Enabled,
            source: PromptSkillStateSource::GlobalDefault,
        },
        ResolvedPromptSkill {
            skill: prompt_skill_fixture("disabled", "Use for disabled tasks."),
            state: PromptSkillState::Disabled,
            source: PromptSkillStateSource::Session,
        },
    ];
    let rendered = render_prompt_skills_instructions(&skills).expect("instructions expected");
    assert!(rendered.contains("enabled"));
    assert!(!rendered.contains("disabled"));
    assert!(rendered.contains("<skills_instructions>"));
}

/// Verifies managed package writes commit strict standard and Vulcan metadata together.
/// 验证受管技能包写入会共同提交严格标准元数据与 Vulcan 元数据。
#[test]
fn saves_strict_user_prompt_skill_package() {
    // root isolates the complete user catalog transaction.
    // root 隔离完整用户目录事务。
    let root = temp_dir();
    // input contains one complete current-schema package document.
    // input 包含一个完整当前结构技能包文档。
    let input = PromptSkillPackageInput {
        name: String::from("review-helper"),
        description: String::from("Use when a focused review workflow is requested."),
        body: String::from("# Review\n\nInspect the selected files."),
        interface: PromptSkillInterfaceMetadata {
            display_name: Some(String::from("Review Helper")),
            locales: BTreeMap::from([(
                String::from("zh-CN"),
                PromptSkillLocaleMetadata {
                    display_name: Some(String::from("评审助手")),
                    short_description: Some(String::from("执行聚焦代码评审")),
                },
            )]),
            ..PromptSkillInterfaceMetadata::default()
        },
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("quality"),
            tags: vec![String::from("review")],
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    // saved is the parsed committed package returned by the transaction.
    // saved 是事务返回的已解析提交技能包。
    let saved =
        save_user_prompt_skill_package(&root, input).expect("strict user package should save");

    assert_eq!(saved.name, "review-helper");
    assert_eq!(saved.scope, PromptSkillScope::User);
    assert_eq!(saved.catalog.category_id, "quality");
    assert_eq!(saved.origin.kind, "local-created");
    assert_eq!(
        saved.interface.locales["zh-CN"].display_name.as_deref(),
        Some("评审助手")
    );
    assert!(root.join("review-helper").join(SKILL_FILE_NAME).is_file());
    assert!(
        root.join("review-helper")
            .join(VULCAN_SKILL_METADATA_FILE_NAME)
            .is_file()
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies renaming preserves package resources and removes the previous directory.
/// 验证重命名会保留技能包资源并移除原目录。
#[test]
fn renames_user_prompt_skill_package_with_resources() {
    // root isolates the rename transaction and resource tree.
    // root 隔离重命名事务与资源树。
    let root = temp_dir();
    // initial creates the original managed package.
    // initial 创建原始受管技能包。
    let initial = PromptSkillPackageInput {
        name: String::from("old-helper"),
        description: String::from("Use when the old helper is requested."),
        body: String::from("# Old"),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("general"),
            tags: Vec::new(),
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    save_user_prompt_skill_package(&root, initial).expect("initial package should save");
    fs::create_dir_all(root.join("old-helper/assets")).expect("asset directory should create");
    fs::write(root.join("old-helper/assets/example.txt"), "resource").expect("asset should write");
    // renamed explicitly identifies the only original package source.
    // renamed 明确标识唯一原始技能包来源。
    let renamed = PromptSkillPackageInput {
        original_name: Some(String::from("old-helper")),
        name: String::from("new-helper"),
        description: String::from("Use when the renamed helper is requested."),
        body: String::from("# New"),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("general"),
            tags: Vec::new(),
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    save_user_prompt_skill_package(&root, renamed).expect("renamed package should save");

    assert!(!root.join("old-helper").exists());
    assert_eq!(
        fs::read_to_string(root.join("new-helper/assets/example.txt"))
            .expect("resource should remain readable"),
        "resource"
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies copying from an exact source preserves resources without mutating the source package.
/// 验证从精确来源复制会保留资源且不修改来源技能包。
#[test]
fn copies_prompt_skill_package_resources_to_new_identity() {
    // root isolates both source and copied packages.
    // root 隔离来源与复制后的两个技能包。
    let root = temp_dir();
    // source creates the original package and its bundled script.
    // source 创建原始技能包及其附带脚本。
    let source = PromptSkillPackageInput {
        name: String::from("source-helper"),
        description: String::from("Use when the source helper is requested."),
        body: String::from("# Source"),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("general"),
            tags: Vec::new(),
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    save_user_prompt_skill_package(&root, source).expect("source package should save");
    fs::create_dir_all(root.join("source-helper/scripts")).expect("script directory should create");
    fs::write(
        root.join("source-helper/scripts/run.ps1"),
        "Write-Output copied",
    )
    .expect("source script should write");
    // copied uses the authoritative source directory only as the resource seed.
    // copied 仅将权威来源目录用作资源初始化来源。
    let copied = PromptSkillPackageInput {
        resource_source_dir: Some(root.join("source-helper")),
        name: String::from("copied-helper"),
        description: String::from("Use when the copied helper is requested."),
        body: String::from("# Copied"),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("general"),
            tags: Vec::new(),
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    save_user_prompt_skill_package(&root, copied).expect("copied package should save");

    assert!(root.join("source-helper/SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(root.join("copied-helper/scripts/run.ps1"))
            .expect("copied script should remain readable"),
        "Write-Output copied"
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies user categories and explicit assignments round-trip through the host catalog.
/// 验证用户分类与显式归属可通过宿主目录往返加载。
#[test]
fn persists_user_category_and_skill_assignment() {
    // root isolates the host catalog and user package.
    // root 隔离宿主目录与用户技能包。
    let root = temp_dir();
    // input creates the effective skill assigned later by host metadata.
    // input 创建稍后通过宿主元数据归类的有效技能。
    let input = PromptSkillPackageInput {
        name: String::from("catalog-helper"),
        description: String::from("Use when catalog management is requested."),
        body: String::from("# Catalog"),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("general"),
            tags: Vec::new(),
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    save_user_prompt_skill_package(&root, input).expect("user package should save");
    // category is one complete user-owned logical directory entry.
    // category 是一个完整用户拥有的逻辑目录条目。
    let category = PromptSkillCategory {
        category_id: String::from("team-workflows"),
        name: String::from("Team Workflows"),
        emoji: String::from("🧭"),
        color: String::from("#0F766E"),
        order: 120,
        locales: BTreeMap::from([(
            String::from("zh-CN"),
            PromptSkillCategoryLocale {
                name: String::from("团队流程"),
                description: None,
            },
        )]),
        ..PromptSkillCategory::default()
    };
    save_user_prompt_skill_category(&root, category).expect("user category should save");
    save_user_prompt_skill_catalog_assignment(
        Path::new(""),
        &root,
        None,
        "catalog-helper",
        "team-workflows",
        vec![String::from("team"), String::from("workflow")],
    )
    .expect("category assignment should save");
    // catalog is the authoritative reloaded source-aware result.
    // catalog 是权威重新加载的来源感知结果。
    let catalog = load_prompt_skills_from_roots(PathBuf::new(), root.clone());

    assert!(
        catalog
            .categories
            .iter()
            .any(|item| item.category_id == "team-workflows")
    );
    assert_eq!(
        catalog
            .skill("catalog-helper")
            .expect("assigned catalog helper skill should remain available")
            .catalog
            .category_id,
        "team-workflows"
    );
    assert_eq!(
        catalog
            .skill("catalog-helper")
            .expect("assigned catalog helper skill should remain available")
            .catalog
            .tags,
        vec![String::from("team"), String::from("workflow")]
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies category deletion moves assigned skills to the immutable unassigned category.
/// 验证分类删除会将已归属技能迁移到不可变的未分配分类。
#[test]
fn deleting_user_category_moves_skills_to_unassigned() {
    // root isolates category assignment and deletion transitions.
    // root 隔离分类归属与删除状态转换。
    let root = temp_dir();
    // category is the user-owned logical category under test.
    // category 是待测试的用户拥有逻辑分类。
    let category = PromptSkillCategory {
        category_id: String::from("temporary-workflows"),
        name: String::from("Temporary Workflows"),
        emoji: String::from("🧪"),
        color: String::from("#7C3AED"),
        order: 150,
        locales: BTreeMap::from([(
            String::from("zh-CN"),
            PromptSkillCategoryLocale {
                name: String::from("临时流程"),
                description: None,
            },
        )]),
        ..PromptSkillCategory::default()
    };
    save_user_prompt_skill_category(&root, category).expect("user category should save");
    // assigned creates one skill whose logical assignment must be migrated.
    // assigned 创建一个必须迁移逻辑归属的技能。
    let assigned = PromptSkillPackageInput {
        name: String::from("temporary-helper"),
        description: String::from("Use when the temporary workflow is requested."),
        body: String::from("# Temporary"),
        catalog: PromptSkillCatalogMetadata {
            category_id: String::from("temporary-workflows"),
            tags: Vec::new(),
        },
        origin: managed_test_origin(),
        ..PromptSkillPackageInput::default()
    };
    save_user_prompt_skill_package(&root, assigned).expect("assigned package should save");

    delete_user_prompt_skill_category(Path::new(""), &root, "temporary-workflows")
        .expect("user category should delete and migrate assignments");
    // catalog is the exact post-delete view used to verify both transitions.
    // catalog 是用于验证两个状态转换的精确删除后目录。
    let catalog = load_prompt_skills_from_roots(PathBuf::new(), root.clone());
    assert!(
        !catalog
            .categories
            .iter()
            .any(|item| item.category_id == "temporary-workflows")
    );
    assert_eq!(
        catalog
            .skill("temporary-helper")
            .expect("skill should remain")
            .catalog
            .category_id,
        "uncategorized"
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies plugin skills retain package metadata while receiving canonical names and the immutable plugin category.
/// 验证插件技能保留包元数据，同时获得规范名称与不可变插件分类。
#[test]
fn plugin_skill_loader_namespaces_and_reclassifies_records() {
    // root owns one conventional plugin skills directory.
    // root 持有一个约定式插件技能目录。
    let root = temp_dir();
    let skill_dir = root.join("review-helper");
    fs::create_dir_all(&skill_dir).expect("plugin skill directory should be created");
    fs::write(
        skill_dir.join(SKILL_FILE_NAME),
        "---\nname: review-helper\ndescription: Review one change.\n---\n\n# Review\nUse the repository context.",
    )
    .expect("plugin skill should be written");
    fs::write(
        skill_dir.join(VULCAN_SKILL_METADATA_FILE_NAME),
        "schema_version: 3\ncatalog:\n  category_id: engineering\norigin:\n  kind: github\n  url: https://github.com/example/content-plugin\n",
    )
    .expect("plugin metadata should be written");

    let skills = load_plugin_prompt_skills("content-plugin", "Content Plugin", "1.2.3", &root)
        .expect("plugin skills should load");
    let skill = skills.first().expect("one plugin skill should exist");

    assert_eq!(skill.name, "content-plugin:review-helper");
    assert_eq!(skill.local_name(), "review-helper");
    assert_eq!(skill.plugin_id(), Some("content-plugin"));
    assert_eq!(skill.scope, PromptSkillScope::Plugin);
    assert_eq!(skill.catalog.category_id, PLUGIN_CATEGORY_ID);
    assert_eq!(skill.origin.kind, "github");
    assert_eq!(skill.origin.label, None);
    assert_eq!(skill.origin.version.as_deref(), Some("1.2.3"));
    assert_eq!(
        skill.origin.url.as_deref(),
        Some("https://github.com/example/content-plugin")
    );
    let _ = fs::remove_dir_all(root);
}

/// Verifies the host-reserved plugin category cannot be created, deleted, or assigned manually.
/// 验证宿主保留的插件分类不能被手工创建、删除或分配。
#[test]
fn plugin_category_rejects_user_mutations() {
    let root = temp_dir();
    let category = PromptSkillCategory {
        category_id: String::from(PLUGIN_CATEGORY_ID),
        name: String::from("Shadow Plugins"),
        ..PromptSkillCategory::default()
    };

    assert!(save_user_prompt_skill_category(&root, category).is_err());
    assert!(delete_user_prompt_skill_category(Path::new(""), &root, PLUGIN_CATEGORY_ID).is_err());
    assert!(
        save_user_prompt_skill_catalog_assignment(
            Path::new(""),
            &root,
            None,
            "ordinary-skill",
            PLUGIN_CATEGORY_ID,
            Vec::new(),
        )
        .is_err()
    );
    let _ = fs::remove_dir_all(root);
}
