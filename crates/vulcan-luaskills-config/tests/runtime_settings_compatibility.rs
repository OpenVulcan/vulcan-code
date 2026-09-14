//! Cross-version guarantees for the user-owned LuaSkills runtime settings document.
//! 用户自有 LuaSkills 运行时设置文档的跨版本保证。
//!
//! The fixture below is a frozen schema 1 document that is deliberately never regenerated from
//! `LuaSkillsRuntimeSettings::default()`. It is the migration gate for the capacity policy change
//! frozen in `docs/plan/20260914-01-LUASKILLS_CONCURRENCY_AND_VM_DEFAULTS.md`: because user files
//! are now preserved across syncs and upgrades, any later field addition or schema bump must keep
//! loading this document with every user value intact, or update it through an explicit migration.
//! 下述夹具是一份冻结的 schema 1 文档，刻意不由 `LuaSkillsRuntimeSettings::default()` 重新生成。
//! 它是在上述计划中冻结的容量策略变更的迁移门槛：由于用户文件现在会跨同步与升级被保留，
//! 后续任何新增字段或 schema 升版都必须仍能加载它并保持全部用户值不变，否则必须走显式迁移。

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use vulcan_luaskills_config::{
    LuaSkillsRuntimeSettings, load_runtime_settings, runtime_settings_file_path,
};

/// Returns the absolute path of the frozen schema 1 user document.
/// 返回冻结的 schema 1 用户文档的绝对路径。
fn frozen_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("luaskills-runtime-setting.schema1.user.json")
}

/// Installs the frozen fixture as the user document of one isolated config directory.
/// 将冻结夹具安装为某个隔离配置目录的用户文档。
///
/// Returns the temporary directory and the exact bytes that must stay untouched.
/// 返回临时目录，以及必须保持不变的精确字节。
fn install_frozen_fixture() -> (TempDir, Vec<u8>) {
    let directory = TempDir::new().expect("create isolated config directory");
    let bytes = fs::read(frozen_fixture_path()).expect("read frozen schema 1 fixture");
    fs::write(runtime_settings_file_path(directory.path()), &bytes)
        .expect("install frozen schema 1 fixture");
    (directory, bytes)
}

/// Returns the current on-disk bytes of one installed user settings document.
/// 返回某个已安装用户设置文档的当前磁盘字节。
fn current_document_bytes(directory: &TempDir) -> Vec<u8> {
    fs::read(runtime_settings_file_path(directory.path())).expect("read current settings document")
}

/// Verifies that a rejected document keeps its exact bytes instead of being repaired or replaced.
/// 验证被拒绝的文档保持精确字节，而不被修复或替换。
fn assert_document_is_rejected_without_rewrite(mutate: impl FnOnce(&mut serde_json::Value)) {
    let (directory, _) = install_frozen_fixture();
    let path = runtime_settings_file_path(directory.path());
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read installed fixture"))
            .expect("decode installed fixture");
    mutate(&mut document);
    let damaged = serde_json::to_vec_pretty(&document).expect("encode damaged document");
    fs::write(&path, &damaged).expect("write damaged document");

    assert!(
        load_runtime_settings(directory.path()).is_err(),
        "damaged document must be rejected instead of silently decoded"
    );
    assert_eq!(
        fs::read(&path).expect("read damaged document"),
        damaged,
        "rejected document must never be repaired, rewritten, or replaced by compiled defaults"
    );
}

/// Verify that the frozen schema 1 user document still loads with every user value preserved.
/// 验证冻结的 schema 1 用户文档仍能加载，且全部用户值保持不变。
#[test]
fn frozen_schema1_document_preserves_every_user_value() {
    let (directory, original_bytes) = install_frozen_fixture();
    let settings = load_runtime_settings(directory.path()).expect("load frozen schema 1 document");

    assert_eq!(settings.schema_version, 1);

    // Pool values stay exactly as the user's file declared them; new host defaults must never leak in.
    // 池取值与用户文件声明完全一致，新的宿主默认值绝不渗入。
    assert_eq!(settings.vm_pool.min_size, 1);
    assert_eq!(settings.vm_pool.max_size, 2);
    assert_eq!(settings.vm_pool.idle_ttl_secs, 30);
    assert_eq!(settings.run_lua_pool.min_size, 1);
    assert_eq!(settings.run_lua_pool.max_size, 4);
    assert_eq!(settings.run_lua_pool.idle_ttl_secs, 60);
    assert_eq!(settings.tool_cache.max_entries, 500);
    assert_eq!(settings.tool_cache.default_ttl_secs, 600);
    assert_eq!(settings.tool_cache.max_ttl_secs, 1_200);

    // Capability choices the user disabled must never be re-enabled by a fallback path.
    // 用户已关闭的能力选择绝不能被任何回退路径重新打开。
    assert!(!settings.network.allow_downloads);
    assert!(settings.network.enable_private_url_skill_install);
    assert_eq!(
        settings.network.github_base_url.as_deref(),
        Some("https://github.example.invalid")
    );
    assert_eq!(
        settings.network.private_skill_source_allowlist,
        vec![String::from("https://skills.example.invalid")]
    );
    assert_eq!(
        settings.capabilities.default_text_encoding.as_deref(),
        Some("system")
    );
    assert!(!settings.capabilities.enable_managed_io_compat);
    assert!(!settings.ai_tools.enable_run_lua);
    assert!(!settings.ai_tools.enable_run_lua_session);

    assert_eq!(
        settings
            .managed_runtime
            .worker_pool_max_size_per_environment,
        2
    );
    assert_eq!(settings.managed_runtime.worker_idle_ttl_secs, 120);
    assert_eq!(
        settings.managed_runtime.persistent_session_limit_per_engine,
        64
    );
    assert_eq!(
        settings
            .managed_runtime
            .persistent_session_default_buffer_limit_bytes_per_stream,
        524_288
    );
    assert_eq!(
        settings.managed_runtime.invoke_default_timeout_ms,
        Some(90_000)
    );

    // Loading is read-only, so the user document keeps its exact bytes.
    // 加载是只读操作，因此用户文档保持精确字节。
    assert_eq!(current_document_bytes(&directory), original_bytes);
}

/// Verify that repeated loads never accumulate rewrites of the user document.
/// 验证重复加载绝不会累积改写用户文档。
#[test]
fn repeated_loads_leave_the_user_document_byte_identical() {
    let (directory, original_bytes) = install_frozen_fixture();
    for _ in 0..3 {
        let settings = load_runtime_settings(directory.path()).expect("reload frozen document");
        assert_eq!(settings.vm_pool.max_size, 2);
        assert_eq!(current_document_bytes(&directory), original_bytes);
    }
}

/// Verify that a missing document alone selects the compiled defaults without creating a file.
/// 验证只有文档缺失时才选用编译默认值，且不会创建文件。
#[test]
fn absent_document_selects_compiled_defaults_without_writing() {
    let directory = TempDir::new().expect("create isolated config directory");
    let settings =
        load_runtime_settings(directory.path()).expect("load defaults for absent document");

    assert_eq!(settings, LuaSkillsRuntimeSettings::default());
    assert!(
        !runtime_settings_file_path(directory.path()).exists(),
        "loading defaults must not create or seed a user document"
    );
}

/// Verify the compiled defaults carry the capacity policy frozen by the plan.
/// 验证编译默认值携带计划冻结的容量策略。
#[test]
fn compiled_defaults_match_the_frozen_capacity_policy() {
    let defaults = LuaSkillsRuntimeSettings::default();

    assert_eq!(defaults.vm_pool.min_size, 4);
    assert_eq!(defaults.vm_pool.max_size, 8);
    assert_eq!(defaults.vm_pool.idle_ttl_secs, 30);
    assert_eq!(defaults.run_lua_pool.min_size, 1);
    assert_eq!(defaults.run_lua_pool.max_size, 2);
    assert_eq!(defaults.run_lua_pool.idle_ttl_secs, 60);

    // New defaults must differ from the frozen user document, otherwise the preservation test above
    // could pass without proving anything.
    // 新默认值必须不同于冻结用户文档，否则上述保留性测试即使通过也证明不了任何事。
    let (directory, _) = install_frozen_fixture();
    let frozen = load_runtime_settings(directory.path()).expect("load frozen document");
    assert_ne!(defaults.vm_pool, frozen.vm_pool);
    assert_ne!(defaults.run_lua_pool, frozen.run_lua_pool);
}

/// Verify that a removed required field is rejected without rewriting the document.
/// 验证删除必填字段会被拒绝且不改写文档。
#[test]
fn missing_required_field_is_rejected_without_rewrite() {
    assert_document_is_rejected_without_rewrite(|document| {
        document
            .as_object_mut()
            .expect("settings document is a JSON object")
            .remove("vmPool");
    });
}

/// Verify that a newer unknown schema version is rejected without rewriting the document.
/// 验证更新的未知 schema 版本会被拒绝且不改写文档。
#[test]
fn unknown_future_schema_version_is_rejected_without_rewrite() {
    assert_document_is_rejected_without_rewrite(|document| {
        document["schemaVersion"] = serde_json::Value::from(2);
    });
}

/// Verify validation failures name the exact preserved user document for startup diagnostics.
/// 验证校验失败会指出被保留用户文档的精确路径，供启动诊断使用。
#[test]
fn invalid_schema_error_names_the_exact_user_document() {
    let (directory, _) = install_frozen_fixture();
    let path = runtime_settings_file_path(directory.path());
    let mut document: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read frozen settings"))
            .expect("decode frozen settings");
    document["schemaVersion"] = serde_json::Value::from(2);
    fs::write(
        &path,
        serde_json::to_vec(&document).expect("encode invalid settings"),
    )
    .expect("write invalid settings");

    let error = load_runtime_settings(directory.path()).expect_err("schema 2 must be rejected");
    assert!(error.starts_with("invalid LuaSkills runtime settings `"));
    assert!(error.contains(&path.display().to_string()));
    assert!(error.contains("unsupported LuaSkills runtime settings schema: 2"));
}

/// Verify that truncated JSON is rejected without rewriting the document.
/// 验证被截断的 JSON 会被拒绝且不改写文档。
#[test]
fn corrupt_document_is_rejected_without_rewrite() {
    let (directory, _) = install_frozen_fixture();
    let path = runtime_settings_file_path(directory.path());
    let corrupt = b"{ \"schemaVersion\": 1, ".to_vec();
    fs::write(&path, &corrupt).expect("write corrupt document");

    assert!(load_runtime_settings(directory.path()).is_err());
    assert_eq!(
        fs::read(&path).expect("read corrupt document"),
        corrupt,
        "corrupt document must never be repaired or replaced by compiled defaults"
    );
}
