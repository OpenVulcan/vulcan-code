//! Cross-check generated authoring contracts, explicit migration and real package inspection.
//! 交叉校验生成的编写契约、显式迁移及真实插件包检查。

use super::*;

/// Return a complete legacy Lua declaration; the stateless tool retains its original execution semantics.
/// 返回完整旧版 Lua 声明；无状态工具保留原执行语义。
fn legacy() -> Value {
    json!({
        "schema_version": VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION,
        "id":"author-test","version":"1.0.0","default_enabled":true,
        "display":{"name":"Author test","capability":"Test explicit migration."},
        "execution":{"dependencies":{"manifest":"dependencies.yaml","install_mode":"bundled"}},
        "lua_runtime":{
            "bundle":"plugin.lua","invoke_function":"dispatch","shutdown_function":"shutdown",
            "max_active_sessions":2,
            "tools":[{"id":"author_call","group":"author","description":"Run the authoring fixture.",
                "parameters_file":"parameters.json",
                "execution":{"kind":"stateless","operation":"call","timeout_ms":1000}}]
        }
    })
}

/// Return explicit requirements for the linked core; migration itself never manufactures these choices.
/// 返回针对链接核心的显式要求；迁移自身绝不制造这些选择。
fn requirements() -> Value {
    json!({
        "protocol_version":luaskills::ffi_embedded::embedded_core_description().protocol_version,
        "backend":crate::runtime::VULCAN_PLUGIN_EXECUTION_BACKEND,
        "core_capabilities":[],"capacities":[]
    })
}

/// Encode value as author-owned UTF-8 YAML, preserving field omission in the generic value.
/// 将 value 编码为作者拥有的 UTF-8 YAML，保留通用值中的字段省略。
fn bytes(value: &Value) -> Vec<u8> {
    serde_yaml::to_string(value)
        .expect("fixture YAML")
        .into_bytes()
}

/// Compile the complete generated schema through the production offline JSON Schema dependency.
/// 通过生产离线 JSON Schema 依赖编译完整生成 Schema。
fn validator() -> jsonschema::Validator {
    let artifacts = generated_schemas().expect("generated artifact set");
    let schema: Value = serde_json::from_slice(&artifacts["plugin-manifest.schema.json"])
        .expect("complete schema JSON");
    jsonschema::validator_for(&schema).expect("offline complete schema")
}

/// Full schemas must cover actual shipped packages and stay byte-for-byte synchronized with generation.
/// 完整 Schema 必须覆盖实际随附插件包，并与生成结果逐字节同步。
#[test]
fn manifest_authoring_schema_matches_all_shipped_packages_and_artifacts() {
    // Paths derive from the crate source identity, never the process working directory.
    // 路径从 crate 源码身份派生，绝不依赖进程工作目录。
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    sync_schemas(&workspace.join("docs/contracts"), true).expect("committed generated schemas");
    let validator = validator();
    let mut checked = 0;
    for entry in std::fs::read_dir(workspace.join("plugins")).expect("shipped plugins") {
        let path = entry.expect("package entry").path();
        if !path.is_dir() {
            continue;
        }
        let source = std::fs::read(path.join("plugin.yaml")).expect("shipped manifest");
        let value: Value = serde_yaml::from_slice(&source).expect("shipped YAML");
        assert!(
            validator.is_valid(&value),
            "{}: {:?}",
            path.display(),
            validator
                .iter_errors(&value)
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
        );
        validate_manifest(
            &source,
            path.file_name()
                .expect("package directory name")
                .to_str()
                .expect("UTF-8 package identity"),
        )
        .expect("same Rust semantics");
        checked += 1;
    }
    assert!(
        checked > 0,
        "Expected shipped plugin manifests; source layout may have changed."
    );
}

/// Version gates, explicit nulls, tagged MCP variants and nested unknown fields match actual decoding boundaries.
/// 版本门控、显式空值、带标签 MCP 变体及嵌套未知字段与实际解码边界一致。
#[test]
fn manifest_authoring_schema_rejects_invalid_structure_and_version_combinations() {
    let validator = validator();
    let base = legacy();
    assert!(validator.is_valid(&base));
    let migrated: Value = serde_yaml::from_slice(
        &migrate_manifest(&bytes(&base), "author-test", Some(&bytes(&requirements())))
            .expect("valid authoring fixture"),
    )
    .expect("valid authoring fixture");
    assert!(validator.is_valid(&migrated));
    // A valid opt-in contract is the control for both legacy and session-kind rejection cases.
    // 有效主动复用契约作为旧版及会话类别拒绝用例的对照。
    let mut reusable = migrated.clone();
    let tool = reusable["lua_runtime"]["tools"]
        .as_array_mut()
        .expect("declared tool list")
        .iter_mut()
        .find(|tool| tool["id"] == "author_call")
        .expect("named reusable tool");
    tool["execution"]["reusable"] = json!({
        "before_call":"before","after_call":"after","reuse_check":"check",
        "ordering":"key_serial","max_resident_vms":1,"max_running_calls":1,"idle_ttl_ms":1000,"max_uses":4
    });
    assert!(validator.is_valid(&reusable));
    validate_manifest(&bytes(&reusable), "author-test").expect("real reusable declaration");
    let mut invalid = Vec::new();
    // Historical fragment constraints must also survive in the complete generated manifest.
    // 历史片段约束也必须保留在完整生成清单中。
    for (field, value) in [
        ("max_resident_vms", json!(0)),
        ("max_running_calls", json!(0)),
        ("max_running_calls", json!(2)),
        ("idle_ttl_ms", json!(0)),
        ("max_uses", json!(0)),
        ("before_call", json!("bad.name")),
        ("after_call", json!("")),
        ("reuse_check", json!("check\n")),
    ] {
        // Find the real tool identity so fixture reordering cannot invalidate the assertion.
        // 查找真实工具身份，避免夹具重排使断言失效。
        let mut candidate = reusable.clone();
        let tool = candidate["lua_runtime"]["tools"]
            .as_array_mut()
            .expect("declared tool list")
            .iter_mut()
            .find(|tool| tool["id"] == "author_call")
            .expect("named reusable tool");
        tool["execution"]["reusable"][field] = value;
        invalid.push(candidate);
    }
    // Empty and duplicate capability declarations must not silently weaken the contract.
    // 空能力和重复能力声明不得静默弱化契约。
    for (field, value) in [
        ("protocol_version", json!(0)),
        ("protocol_version", json!(u64::from(u32::MAX) + 1)),
        ("core_capabilities", json!([""])),
        ("core_capabilities", json!(["duplicate", "duplicate"])),
        (
            "host_capabilities",
            json!([{"name":"","version":"1.0.0","execution":"native","scope":"invocation"}]),
        ),
        (
            "host_capabilities",
            json!([{"name":"host.test","version":"","execution":"native","scope":"invocation"}]),
        ),
    ] {
        let mut candidate = migrated.clone();
        candidate["runtime_requirements"][field] = value;
        invalid.push(candidate);
    }
    // A valid capacity declaration proves each individual zero is rejected for the intended reason.
    // 合法容量声明证明每个零值因预期规则被拒绝。
    let mut capacity_control = migrated.clone();
    capacity_control["runtime_requirements"]["capacities"] = json!([{
        "purpose": crate::VulcanPluginCapacityPurpose::ToolCall,
        "reserved_resident_vms":0,"resident_vms":1,"running_calls":1,"queued_calls":1,"queued_bytes":1
    }]);
    assert!(validator.is_valid(&capacity_control));
    validate_manifest(&bytes(&capacity_control), "author-test").expect("valid capacity control");
    for field in [
        "resident_vms",
        "running_calls",
        "queued_calls",
        "queued_bytes",
    ] {
        let mut candidate = capacity_control.clone();
        let capacity = candidate["runtime_requirements"]["capacities"]
            .as_array_mut()
            .expect("declared capacities")
            .iter_mut()
            .find(|capacity| {
                capacity["purpose"] == json!(crate::VulcanPluginCapacityPurpose::ToolCall)
            })
            .expect("tool-call purpose");
        capacity[field] = json!(0);
        invalid.push(candidate);
    }

    let mut legacy_reuse = reusable.clone();
    legacy_reuse["schema_version"] = json!(VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION);
    legacy_reuse
        .as_object_mut()
        .expect("valid authoring fixture")
        .remove("runtime_requirements");
    invalid.push(legacy_reuse);
    let tool = reusable["lua_runtime"]["tools"]
        .as_array_mut()
        .expect("declared tool list")
        .iter_mut()
        .find(|tool| tool["id"] == "author_call")
        .expect("named session tool");
    tool["execution"]["kind"] = json!("session_call");
    invalid.push(reusable);
    // Every mutation targets a real named field; array positions are found by the stable tool identity below.
    // 每个变更均针对真实具名字段；下文按稳定工具身份查找数组位置。
    for (field, value) in [
        ("schema_version", json!(99)),
        ("unknown", json!(true)),
        ("runtime_requirements", Value::Null),
    ] {
        let mut candidate = base.clone();
        candidate[field] = value;
        invalid.push(candidate);
    }
    let mut old_with_requirements = base.clone();
    old_with_requirements["runtime_requirements"] = requirements();
    invalid.push(old_with_requirements);
    let mut missing_requirements = base.clone();
    missing_requirements["schema_version"] = json!(VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION);
    invalid.push(missing_requirements);
    let mut non_lua = migrated.clone();
    non_lua
        .as_object_mut()
        .expect("manifest object")
        .remove("lua_runtime");
    invalid.push(non_lua);
    let mut wrong_variant = migrated.clone();
    wrong_variant["mcp"] = json!({"servers":[{"id":"remote","name":"Remote","transport":{
        "type":"http","url":{"source":"literal","value":"https://example.com"},"launcher":{"type":"node_entry","entry_id":"bad"}
    }}]});
    invalid.push(wrong_variant);
    let mut null_reuse = migrated.clone();
    let tool = null_reuse["lua_runtime"]["tools"]
        .as_array_mut()
        .expect("declared tool list")
        .iter_mut()
        .find(|tool| tool["id"] == "author_call")
        .expect("named tool");
    tool["execution"]["reusable"] = Value::Null;
    invalid.push(null_reuse);
    for candidate in invalid {
        assert!(
            !validator.is_valid(&candidate),
            "schema accepted {candidate}"
        );
        assert!(
            validate_manifest(&bytes(&candidate), "author-test").is_err(),
            "Rust accepted {candidate}"
        );
    }
}

/// Migration preserves every author-owned field and rejects ambiguity, unknown versions and implicit requirements.
/// 迁移保留每个作者拥有字段，并拒绝歧义、未知版本及隐式要求。
#[test]
fn manifest_authoring_migration_preserves_authority_and_rejects_duplicates() {
    let source = bytes(&legacy());
    let required = bytes(&requirements());
    let migrated =
        migrate_manifest(&source, "author-test", Some(&required)).expect("explicit migration");
    let mut expected = legacy();
    expected["schema_version"] = json!(VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION);
    expected["runtime_requirements"] = requirements();
    assert_eq!(
        serde_yaml::from_slice::<Value>(&migrated).expect("valid authoring fixture"),
        expected
    );
    assert!(migrate_manifest(&source, "author-test", None).is_err());
    assert!(migrate_manifest(&source, "another-package", Some(&required)).is_err());
    assert!(migrate_manifest(&migrated, "author-test", Some(&required)).is_err());
    for suffix in [
        "schema_version: 1\n",
        "id: author-test\n",
        "unknown: true\n",
        "locales:\n  en: first.json\n  en: second.json\n",
    ] {
        let mut ambiguous = source.clone();
        ambiguous.extend_from_slice(suffix.as_bytes());
        assert!(migrate_manifest(&ambiguous, "author-test", Some(&required)).is_err());
    }
    let mut duplicated_requirement = required.clone();
    duplicated_requirement.extend_from_slice(b"backend: in_process\n");
    assert!(migrate_manifest(&source, "author-test", Some(&duplicated_requirement)).is_err());
    let mut content = legacy();
    content
        .as_object_mut()
        .expect("manifest object")
        .remove("lua_runtime");
    content
        .as_object_mut()
        .expect("manifest object")
        .remove("execution");
    assert!(migrate_manifest(&bytes(&content), "author-test", Some(&required)).is_err());
    let content_migrated =
        migrate_manifest(&bytes(&content), "author-test", None).expect("valid authoring fixture");
    assert!(
        validate_manifest(&content_migrated, "author-test")
            .expect("valid authoring fixture")
            .runtime_requirements
            .is_none()
    );
}

/// Generated files detect drift and new-manifest publication cannot overwrite an original, alias or competing output.
/// 生成文件可检测漂移，新清单发布不能覆盖原件、别名或竞争目标。
#[test]
fn manifest_authoring_file_publication_preserves_existing_bytes() {
    let root = tempfile::tempdir().expect("valid authoring fixture");
    sync_schemas(root.path(), false).expect("generate artifacts");
    sync_schemas(root.path(), true).expect("verify artifacts");
    let schema = root.path().join("plugin-manifest.schema.json");
    std::fs::write(&schema, b"stale").expect("valid authoring fixture");
    assert!(sync_schemas(root.path(), true).is_err());
    assert_eq!(
        std::fs::read(&schema).expect("valid authoring fixture"),
        b"stale"
    );
    let source = root.path().join("plugin.yaml");
    std::fs::write(&source, b"original").expect("valid authoring fixture");
    let alias = root.path().join("alias.yaml");
    std::fs::hard_link(&source, &alias).expect("valid authoring fixture");
    for existing in [&source, &alias] {
        assert!(write_new_manifest(existing, b"replacement").is_err());
        assert_eq!(
            std::fs::read(existing).expect("valid authoring fixture"),
            b"original"
        );
    }
    let output = root.path().join("new.yaml");
    // Two actual writers compete; exactly one image wins and the loser cannot truncate it.
    // 两个真实写入者竞争；恰好一份内容成功，失败者不能截断它。
    let handles = [b"first".as_slice(), b"second".as_slice()]
        .into_iter()
        .map(|bytes| {
            let output = output.clone();
            std::thread::spawn(move || write_new_manifest(&output, bytes))
        })
        .collect::<Vec<_>>();
    let successes = handles
        .into_iter()
        .map(|handle| handle.join().expect("publication writer completes").is_ok())
        .filter(|success| *success)
        .count();
    assert_eq!(successes, 1);
    let contents = std::fs::read(&output).expect("valid authoring fixture");
    assert!(contents == b"first" || contents == b"second");
}

/// An explicitly migrated real package still passes the same catalog and resource checks as a legacy package.
/// 显式迁移后的真实插件包仍通过与旧版插件相同的目录及资源检查。
#[test]
fn manifest_authoring_migrated_package_passes_production_inspection() {
    let root = tempfile::tempdir().expect("valid authoring fixture");
    let package = root.path().join("author-test");
    std::fs::create_dir(&package).expect("valid authoring fixture");
    std::fs::write(package.join("dependencies.yaml"), "dependencies: []\n")
        .expect("valid authoring fixture");
    std::fs::write(
        package.join("plugin.lua"),
        "return {dispatch=function() return '{}' end,shutdown=function() return '{}' end}",
    )
    .expect("valid authoring fixture");
    std::fs::write(
        package.join("parameters.json"),
        r#"{"type":"object","properties":{}}"#,
    )
    .expect("valid authoring fixture");
    let manifest = package.join("plugin.yaml");
    let original = bytes(&legacy());
    std::fs::write(&manifest, &original).expect("valid authoring fixture");
    validate_package(&package).expect("legacy real package");
    let migrated = migrate_manifest(&original, "author-test", Some(&bytes(&requirements())))
        .expect("valid authoring fixture");
    let separate = root.path().join("proposed.yaml");
    write_new_manifest(&separate, &migrated).expect("valid authoring fixture");
    assert_eq!(
        std::fs::read(&manifest).expect("valid authoring fixture"),
        original
    );
    // Only this test fixture explicitly installs the reviewed candidate; the migration API never changes the source.
    // 仅此测试夹具显式安装已检查候选；迁移接口绝不改变源文件。
    std::fs::write(&manifest, migrated).expect("valid authoring fixture");
    validate_package(&package).expect("migrated real package");
    // A successful static inspection still rejects duplicate dynamic mapping keys on the actual file path.
    // 成功静态检查后，实际文件路径仍拒绝动态映射重复键。
    let mut duplicate = std::fs::read(&manifest).expect("valid authoring fixture");
    duplicate.extend_from_slice(b"locales:\n  en: one.json\n  en: two.json\n");
    std::fs::write(&manifest, duplicate).expect("valid authoring fixture");
    assert!(validate_package(&package).is_err());
}
