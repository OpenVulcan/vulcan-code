//! Real activation files prove complete configuration selection, limits and strict recovery boundaries.
//! 实际激活文件证明完整配置选择、限制及严格恢复边界。

use super::*;
use serde_json::{Value, json};

/// Build an MCP package with an optional real configuration declaration; return its exact staged package and credentials.
/// 构建可选声明真实配置的 MCP 插件包；返回其精确已暂存插件包及凭据。
pub(super) fn fixture(
    root: &Path,
    configuration: bool,
) -> (
    VulcanPluginGenerationStore,
    PinnedVulcanPluginGeneration,
    VulcanPluginCredentialRevision,
) {
    // Package contents are isolated from every installed plugin.
    // 插件包内容与所有已安装插件隔离。
    let source = root.join("source/configured-plugin");
    fs::create_dir_all(&source).expect("create configured source");
    // Both variants declare the same harmless remote contribution; the configured variant owns a real schema.
    // 两个变体声明相同无害远端贡献；配置变体持有真实模式。
    let declaration = if configuration {
        "permissions: [network_https, config]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\n"
    } else {
        "permissions: [network_https]\n"
    };
    fs::write(source.join("plugin.yaml"), format!(
        "schema_version: 1\nid: configured-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Configuration fixture\n  capability: Selected configuration\n{declaration}mcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {{source: literal, value: 'https://example.test/mcp'}}\n"
    )).expect("write configured manifest");
    if configuration {
        fs::write(source.join("schema.json"), r#"{"type":["object","null"],"properties":{"label":{"type":"string"},"padding":{"type":"string"}},"required":["label"],"additionalProperties":false}"#)
            .expect("write exact configuration schema");
        fs::write(source.join("defaults.json"), r#"{"label":"default"}"#)
            .expect("write configuration defaults");
    }
    // Production inspection derives permissions, schema declaration and byte identity from this one package.
    // 生产检查从此单个插件包派生权限、模式声明及字节身份。
    let store = VulcanPluginGenerationStore::open(root).expect("open configuration store");
    // Actual package ownership survives every configuration-only revision in the test.
    // 实际插件包所有权在测试的每个纯配置修订期间持续保留。
    let package = prepare(&store, &source);
    // This package declares no secrets, but fourth-version activation still requires a genuine staged credential identity.
    // 此插件包不声明秘密，但第四版激活仍要求确实已暂存的凭据身份。
    let credentials = VulcanPluginCredentialRevision::allocate();
    super::credentials::stage_empty_credentials(&store, &package, &credentials);
    (store, package, credentials)
}

/// Wrap the complete `value` in an explicit document selection, preserving JSON null as a real document.
/// 将完整 `value` 包装为显式文档选择，保留 JSON 空值作为实际文档。
fn document(value: Value) -> VulcanPluginConfigurationSelection {
    VulcanPluginConfigurationSelection::Document { value }
}

/// Verify atomic same-package configuration revisions, stale predecessor rejection and tombstone retention.
/// 验证同包配置修订原子性、旧前驱拒绝及停用记录保留。
#[test]
fn configuration_selection_publishes_complete_values_and_preserves_old_observations() {
    // Real filesystem observations are independent of the store object's Rust lifetime.
    // 实际文件系统观察独立于存储对象的 Rust 生命周期。
    let temp = tempfile::tempdir().expect("create configured activation fixture");
    // All authorities come from the actual isolated package.
    // 全部权威来自实际隔离插件包。
    let (store, package, credentials) = fixture(temp.path(), true);
    // A user document must never appear in Debug output.
    // 用户文档绝不能出现在 Debug 输出中。
    let first = document(json!({"label":"fixture-private-first"}));
    // The first choice publishes package, approval, credential identity and configuration in one record.
    // 第一次选择在同一记录发布插件包、批准、凭据身份及配置。
    let original = store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials.clone(),
            first.clone(),
        )
        .expect("publish configured selection");
    assert_eq!(original.schema_version, ACTIVATION_RECORD_VERSION);
    assert!(!format!("{original:?}").contains("fixture-private-first"));
    // Pinning must verify the configuration against the same exact physical package.
    // 固定必须按相同精确物理插件包校验配置。
    let pinned = store
        .pin_active("configured-plugin")
        .expect("pin original configured selection");
    // A same-package successor changes only the explicit selected configuration.
    // 同包后继只改变显式选定配置。
    let second = document(json!({"label":"second"}));
    store
        .activate_configured(
            &package,
            1,
            package.required_permissions.clone(),
            credentials.clone(),
            second.clone(),
        )
        .expect("publish next configuration revision");
    assert_eq!(
        pinned
            .activation
            .as_ref()
            .expect("retain original observation")
            .configuration(),
        &first
    );
    assert_eq!(
        store
            .activation("configured-plugin")
            .expect("read current selection")
            .expect("current activation")
            .configuration(),
        &second
    );
    assert_eq!(
        store
            .activate_configured(
                &package,
                1,
                package.required_permissions.clone(),
                credentials.clone(),
                first
            )
            .expect_err("stale predecessor must reject")
            .code,
        VulcanPluginStoreErrorCode::Conflict
    );
    // Invalid values cannot replace the already selected valid document.
    // 非法值不能替换已经选定的合法文档。
    assert!(
        store
            .activate_configured(
                &package,
                2,
                package.required_permissions.clone(),
                credentials.clone(),
                document(json!({"label":5}))
            )
            .is_err()
    );
    assert!(
        store
            .activate_configured(
                &package,
                2,
                package.required_permissions.clone(),
                credentials,
                VulcanPluginConfigurationSelection::NotDeclared {}
            )
            .is_err()
    );
    // Explicit removal keeps historical configuration without granting execution.
    // 显式移除保留历史配置，但不授予执行权限。
    let removed = store
        .deactivate("configured-plugin", 2)
        .expect("remove configured selection");
    assert_eq!(removed.configuration(), &second);
    assert!(
        store
            .pin_active("configured-plugin")
            .expect("pin tombstone")
            .generation
            .is_none()
    );
    drop(store);
    assert_eq!(
        VulcanPluginGenerationStore::open(temp.path())
            .expect("reopen configured store")
            .activation("configured-plugin")
            .expect("read durable tombstone")
            .expect("tombstone remains")
            .configuration(),
        &second
    );
}

/// Reject cross-version fields, malformed authority tags, missing values and schema-invalid selected documents.
/// 拒绝跨版本字段、损坏权威标签、缺失值及不符合模式的选定文档。
#[test]
fn configuration_selection_rejects_corruption_without_repair_or_value_diagnostics() {
    // Preserve exact valid bytes between each independently injected defect.
    // 在每个独立注入缺陷之间保留精确合法字节。
    let temp = tempfile::tempdir().expect("create corrupt configuration fixture");
    // The immutable package supplies a strict object-or-null schema.
    // 不可变插件包提供严格对象或空值模式。
    let (store, package, credentials) = fixture(temp.path(), true);
    store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials,
            document(json!({"label":"ok"})),
        )
        .expect("publish corruption baseline");
    // The path is derived from the established exact storage layout.
    // 路径从已建立的精确存储布局派生。
    let path = store.root.join("active/configured-plugin.json");
    // Retain bytes for verifying that rejected reads never rewrite them.
    // 保留字节以验证被拒绝读取绝不重写它们。
    let baseline = fs::read_to_string(&path).expect("read original activation bytes");
    for replacement in [
        json!(null),
        json!({"kind":"unknown-private-value"}),
        json!({"kind":"document"}),
        json!({"kind":"document","value":{"label":"ok"},"unknown":true}),
    ] {
        // Each mutation targets only the declared configuration authority field.
        // 每次变更仅针对已声明配置权威字段。
        let mut corrupt: Value =
            serde_json::from_str(&baseline).expect("decode original selection");
        corrupt["configuration"] = replacement;
        // Exact input remains available for manual recovery after the failed read.
        // 读取失败后，精确输入仍可供手动恢复。
        let bytes = serde_json::to_vec(&corrupt).expect("encode corrupt selection");
        fs::write(&path, &bytes).expect("inject configuration corruption");
        assert!(store.pin_active("configured-plugin").is_err());
        assert_eq!(fs::read(&path).expect("inspect rejected bytes"), bytes);
    }
    // An explicit document null is valid under this fixture schema, unlike a missing selection field.
    // 此夹具模式下显式文档空值合法，区别于选择字段缺失。
    let mut corrupt: Value =
        serde_json::from_str(&baseline).expect("decode field mutation baseline");
    corrupt
        .as_object_mut()
        .expect("activation object")
        .remove("configuration");
    fs::write(
        &path,
        serde_json::to_vec(&corrupt).expect("encode missing field"),
    )
    .expect("inject missing field");
    assert!(store.activation("configured-plugin").is_err());
    corrupt = serde_json::from_str(&baseline).expect("restore mutation baseline");
    corrupt["schema_version"] = json!(3);
    fs::write(
        &path,
        serde_json::to_vec(&corrupt).expect("encode cross-version field"),
    )
    .expect("inject cross-version authority");
    assert!(store.activation("configured-plugin").is_err());
    fs::write(
        &path,
        baseline.replace(
            "\"configuration\":",
            "\"configuration\":{\"kind\":\"not_declared\"},\"configuration\":",
        ),
    )
    .expect("inject actual duplicate selection key");
    assert!(store.activation("configured-plugin").is_err());
    corrupt = serde_json::from_str(&baseline).expect("restore schema mutation baseline");
    corrupt["configuration"]["value"] = json!({"label":123,"private":"must-not-leak"});
    fs::write(
        &path,
        serde_json::to_vec(&corrupt).expect("encode invalid schema value"),
    )
    .expect("inject invalid configuration schema value");
    assert!(
        !store
            .pin_active("configured-plugin")
            .expect_err("schema-invalid document must reject")
            .to_string()
            .contains("must-not-leak")
    );
    fs::write(&path, baseline).expect("restore exact valid activation");
    assert!(store.pin_active("configured-plugin").is_ok());
}

/// Distinguish explicit JSON null from absent declaration and enforce exact byte budgets without changing prior selections.
/// 区分显式 JSON 空值与无声明，并执行精确字节预算而不改变此前选择。
#[test]
fn configuration_selection_enforces_exact_limits_and_explicit_null_semantics() {
    assert!(
        serde_json::from_value::<VulcanPluginConfigurationSelection>(
            json!({"kind":"not_declared","unexpected":"fixture-private"})
        )
        .is_err()
    );
    // The same package permits null and bounded complete documents.
    // 相同插件包允许空值及有界完整文档。
    let temp = tempfile::tempdir().expect("create configuration limit fixture");
    // The schema owns meaning while the store owns encoded byte bounds.
    // 模式持有含义，而存储持有编码字节边界。
    let (store, package, credentials) = fixture(temp.path(), true);
    store
        .activate_configured(
            &package,
            0,
            package.required_permissions.clone(),
            credentials.clone(),
            document(Value::Null),
        )
        .expect("publish explicit schema-valid null");
    assert_eq!(
        store
            .pin_active("configured-plugin")
            .expect("pin explicit null")
            .activation
            .expect("null selection exists")
            .configuration()
            .value(),
        Some(&Value::Null)
    );
    // Derive fixture overhead from actual canonical encoding instead of duplicating production size assumptions.
    // 从实际规范编码派生夹具开销，不复制生产大小假设。
    let empty = json!({"label":"ok","padding":""});
    // This exact-sized value must fit in the larger activation envelope.
    // 此精确大小值必须可放入较大的激活封套。
    let padding = MAX_PLUGIN_CONFIGURATION_BYTES as usize
        - serde_json::to_vec(&empty)
            .expect("encode empty limit fixture")
            .len();
    store
        .activate_configured(
            &package,
            1,
            package.required_permissions.clone(),
            credentials.clone(),
            document(json!({"label":"ok","padding":"x".repeat(padding)})),
        )
        .expect("publish exact configuration limit");
    assert!(store.pin_active("configured-plugin").is_ok());
    // Preserve the exact committed large record before rejecting the next over-limit attempt.
    // 拒绝下一次超限尝试前，保留精确已提交大记录。
    let path = store.root.join("active/configured-plugin.json");
    // Byte equality proves no partial replacement occurred after validation failed.
    // 字节相等证明校验失败后未发生部分替换。
    let original = fs::read(&path).expect("read exact-limit record");
    assert!(
        store
            .activate_configured(
                &package,
                2,
                package.required_permissions.clone(),
                credentials,
                document(json!({"label":"ok","padding":"x".repeat(padding + 1)}))
            )
            .is_err()
    );
    assert_eq!(
        fs::read(&path).expect("read record after rejected oversize"),
        original
    );
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open oversized record fixture")
        .set_len(MAX_ACTIVATION_BYTES + 1)
        .expect("grow record beyond actual read budget");
    assert!(store.activation("configured-plugin").is_err());
    fs::write(&path, original).expect("restore original large record");
    // A separate undeclared package must explicitly select NotDeclared, never a document.
    // 另一个未声明配置插件包必须显式选择 NotDeclared，绝不选择文档。
    let other = tempfile::tempdir().expect("create no-configuration fixture");
    // Physical package identity prevents importing the preceding configured selection.
    // 物理插件包身份防止引入此前已配置选择。
    let (plain_store, plain, plain_credentials) = fixture(other.path(), false);
    assert!(
        plain_store
            .activate_configured(
                &plain,
                0,
                plain.required_permissions.clone(),
                plain_credentials.clone(),
                document(Value::Null)
            )
            .is_err()
    );
    let plain_path = plain_store.root.join("active/configured-plugin.json");
    plain_store
        .activate_configured(
            &plain,
            0,
            plain.required_permissions.clone(),
            plain_credentials,
            VulcanPluginConfigurationSelection::NotDeclared {},
        )
        .expect("publish explicit undeclared configuration");
    assert!(plain_store.pin_active("configured-plugin").is_ok());
    // Persist an actual unknown field in the no-declaration branch, not merely a standalone decoder fixture.
    // 在无声明分支持久写入实际未知字段，而非仅使用独立解码器夹具。
    let original_plain = fs::read(&plain_path).expect("read explicit undeclared selection");
    // This mutation must be rejected before a managed catalog can consume the record.
    // 此变更必须在受管目录消费记录前被拒绝。
    let mut invalid_plain: Value =
        serde_json::from_slice(&original_plain).expect("decode undeclared selection");
    invalid_plain["configuration"]["unexpected"] = json!("fixture-private");
    // Keep exact invalid bytes to prove ordinary inspection performs no silent repair.
    // 保留精确非法字节，证明普通检查不执行静默修复。
    let invalid_bytes =
        serde_json::to_vec(&invalid_plain).expect("encode unknown undeclared field");
    fs::write(&plain_path, &invalid_bytes).expect("persist unknown undeclared field");
    assert!(plain_store.activation("configured-plugin").is_err());
    assert_eq!(
        fs::read(&plain_path).expect("inspect rejected no-declaration record"),
        invalid_bytes
    );
    fs::write(&plain_path, original_plain).expect("restore original undeclared selection");
}
