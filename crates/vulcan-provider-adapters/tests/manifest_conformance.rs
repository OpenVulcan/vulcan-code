//! Provider-manifest conformance and anti-escape checks over every bundled manifest.
//! 对全部内置清单的一致性（是否符合标准）与越界（是否超脱）检测。
//!
//! Every check in this file is data driven: it iterates the bundled manifest directory and
//! never names a provider, a model, or a count. Adding or removing a provider or a model
//! therefore never requires editing this file; a genuine contract change only requires the
//! shared standard below to change.
//! 本文件的每一处检测都是数据驱动的：遍历内置清单目录，不指名任何供应商、模型或数量。
//! 因此增删供应商或模型都不需要修改本文件；只有共享标准本身变化时才需要修改。

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use vulcan_provider_adapters::ManifestOwnership;

/// Returns the runtime-assets provider-manifest root.
/// 返回 runtime-assets 的供应商清单根目录。
fn provider_manifests_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vulcan-runtime-assets")
        .join("resources")
        .join("provider-manifests")
}

/// Returns the repository root that owns both distributable and non-distributable files.
/// 返回同时拥有可分发与不可分发文件的仓库根目录。
fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Reads every `*.json` document in one directory, keyed by file stem.
/// 读取一个目录下的所有 `*.json` 文档，按文件名前缀索引。
fn read_json_directory(directory: &Path) -> BTreeMap<String, String> {
    let mut documents = BTreeMap::new();
    let entries = std::fs::read_dir(directory)
        .unwrap_or_else(|_| panic!("missing directory at {}", directory.display()));
    for entry in entries {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("json file stem")
            .to_string();
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("read {}", path.display()));
        documents.insert(stem, text);
    }
    assert!(
        !documents.is_empty(),
        "bundled manifest directory {} must not be empty",
        directory.display()
    );
    documents
}

/// Parses one bundled manifest document.
/// 解析一份内置清单文档。
fn parse_manifest(provider_id: &str, text: &str) -> Value {
    serde_json::from_str(text)
        .unwrap_or_else(|error| panic!("manifest `{provider_id}` must parse: {error}"))
}

/// Collects one string key from every object of an optional array field.
/// 从可选数组字段的每个对象中收集一个字符串键。
fn object_key(document: &Value, array_field: &str, key: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    let Some(items) = document[array_field].as_array() else {
        return values;
    };
    for item in items {
        let Some(value) = item[key].as_str() else {
            panic!("`{array_field}` entry without string `{key}`");
        };
        values.insert(value.to_string());
    }
    values
}

/// Collects every string value nested anywhere inside one document.
/// 收集文档中任意深度嵌套的全部字符串值。
fn collect_strings(value: &Value, collected: &mut Vec<String>) {
    match value {
        Value::String(text) => collected.push(text.clone()),
        Value::Array(items) => {
            for item in items {
                collect_strings(item, collected);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_strings(item, collected);
            }
        }
        _ => {}
    }
}

/// Collects every array stored under the given key anywhere inside one document.
/// 收集文档中任意深度、键名给定的全部数组。
fn collect_arrays<'a>(value: &'a Value, key: &str, collected: &mut Vec<&'a Vec<Value>>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_arrays(item, key, collected);
            }
        }
        Value::Object(map) => {
            for (name, item) in map {
                if name == key {
                    if let Some(items) = item.as_array() {
                        collected.push(items);
                    }
                }
                collect_arrays(item, key, collected);
            }
        }
        _ => {}
    }
}

/// Asserts every bundled manifest parses and compiles under system ownership.
/// 断言每份内置清单都能解析并以系统归属编译。
#[test]
fn every_bundled_manifest_compiles() {
    for (provider_id, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        parse_manifest(&provider_id, &text);
        vulcan_provider_adapters::compile_manifest_text(&text, ManifestOwnership::System)
            .unwrap_or_else(|error| panic!("manifest `{provider_id}` must compile: {error}"));
    }
}

/// Asserts each manifest identity matches its file name and declares a usable revision.
/// 断言每份清单的身份与文件名一致，并声明可用的修订号与发布时间。
#[test]
fn every_bundled_manifest_keeps_its_own_identity() {
    let mut schema_versions = BTreeSet::new();
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        assert_eq!(
            document["provider_id"].as_str(),
            Some(stem.as_str()),
            "manifest `{stem}` must declare the provider identity of its own file name"
        );
        let schema_version = document["schema_version"]
            .as_str()
            .unwrap_or_else(|| panic!("manifest `{stem}` must declare a schema version"));
        schema_versions.insert(schema_version.to_string());
        assert!(
            document["revision"].as_u64().is_some_and(|value| value > 0),
            "manifest `{stem}` must declare a positive revision"
        );
        assert!(
            document["published_at"]
                .as_str()
                .is_some_and(|value| !value.is_empty()),
            "manifest `{stem}` must declare a publication timestamp"
        );
    }
    assert_eq!(
        schema_versions.len(),
        1,
        "every bundled manifest must share one schema version, found {schema_versions:?}"
    );
}

/// Asserts every declaration identifier is unique inside its own manifest.
/// 断言每份清单内部各声明标识唯一。
#[test]
fn every_bundled_manifest_declares_unique_identifiers() {
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        for (array_field, key) in [
            ("services", "service_entry_id"),
            ("tool_targets", "target_id"),
            ("operations", "operation_id"),
            ("model_rules", "rule_id"),
            ("usage_profiles", "profile_id"),
            ("request_profiles", "profile_id"),
            ("response_profiles", "profile_id"),
            ("upload_channels", "upload_channel_id"),
        ] {
            let identifiers = object_key(&document, array_field, key);
            let declared = document[array_field]
                .as_array()
                .map(|items| items.len())
                .unwrap_or(0);
            assert_eq!(
                identifiers.len(),
                declared,
                "manifest `{stem}` must declare unique `{key}` values in `{array_field}`"
            );
        }
    }
}

/// Asserts every target resolves the service entry and credentials it names.
/// 断言每个目标引用的服务入口与凭据都在同一清单内声明。
#[test]
fn every_bundled_target_resolves_its_entry_and_credentials() {
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        let mut entry_credentials = BTreeMap::new();
        for service in document["services"].as_array().expect("services array") {
            let entry_id = service["service_entry_id"]
                .as_str()
                .expect("service entry identifier");
            let credentials = service["credential_profiles"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            item["credential_profile_id"].as_str().map(str::to_string)
                        })
                        .collect::<BTreeSet<_>>()
                })
                .unwrap_or_default();
            entry_credentials.insert(entry_id.to_string(), credentials);
        }
        for target in document["tool_targets"].as_array().expect("tool_targets array") {
            let target_id = target["target_id"].as_str().expect("target identifier");
            let entry_id = target["service_entry_id"]
                .as_str()
                .unwrap_or_else(|| panic!("target `{target_id}` must name a service entry"));
            let declared = entry_credentials.get(entry_id).unwrap_or_else(|| {
                panic!("target `{target_id}` names undeclared service entry `{entry_id}`")
            });
            for credential in target["credential_profile_ids"]
                .as_array()
                .expect("credential_profile_ids array")
            {
                let credential = credential.as_str().expect("credential identifier");
                assert!(
                    declared.contains(credential),
                    "target `{target_id}` names credential `{credential}` absent from entry `{entry_id}`"
                );
            }
        }
    }
}

/// Asserts target and operation references cover each other exactly.
/// 断言目标与 operation 之间的引用双向完整。
#[test]
fn every_bundled_target_and_operation_reference_each_other() {
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        let targets = object_key(&document, "tool_targets", "target_id");
        let mut referenced = BTreeSet::new();
        for operation in document["operations"].as_array().expect("operations array") {
            let operation_id = operation["operation_id"].as_str().expect("operation id");
            for reference in operation["tool_target_refs"]
                .as_array()
                .expect("tool_target_refs array")
            {
                let reference = reference.as_str().expect("target reference");
                assert!(
                    targets.contains(reference),
                    "operation `{operation_id}` references undeclared target `{reference}`"
                );
                referenced.insert(reference.to_string());
            }
        }
        for target in &targets {
            assert!(
                referenced.contains(target),
                "target `{target}` in manifest `{stem}` is not reachable from any operation"
            );
        }
    }
}

/// Asserts no bundled manifest references another provider.
/// 断言没有任何内置清单引用另一个供应商：供应商配置必须自包含，即使内容相同也应各自声明。
#[test]
fn no_bundled_manifest_references_another_provider() {
    let manifests = read_json_directory(&provider_manifests_root().join("manifests"));
    let provider_ids = manifests.keys().cloned().collect::<BTreeSet<_>>();
    for (stem, text) in &manifests {
        let document = parse_manifest(stem, text);
        let mut strings = Vec::new();
        collect_strings(&document, &mut strings);
        for value in &strings {
            for other in &provider_ids {
                if other == stem {
                    continue;
                }
                assert!(
                    !value.starts_with(&format!("{other}::")),
                    "manifest `{stem}` must not reference provider `{other}` (found `{value}`)"
                );
                assert_ne!(
                    value, other,
                    "manifest `{stem}` must not bind another provider identity `{other}`"
                );
            }
        }
    }
}

/// Asserts evidence references stay auditable from a fresh clone.
/// 断言证据引用在全新克隆中仍可审计：必须指向可分发路径并真实存在，或使用标识符形态。
#[test]
fn every_evidence_reference_is_distributable_or_an_identifier() {
    let root = repository_root();
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        let mut reference_arrays = Vec::new();
        collect_arrays(&document, "evidence_refs", &mut reference_arrays);
        for references in reference_arrays {
            for reference in references {
                let reference = reference.as_str().expect("evidence reference string");
                assert!(
                    !reference.is_empty(),
                    "manifest `{stem}` must not declare an empty evidence reference"
                );
                if !reference.contains('/') {
                    continue;
                }
                let path = reference.split('#').next().unwrap_or(reference);
                assert!(
                    !path.starts_with("docs/") || path.starts_with("docs/contracts/"),
                    "manifest `{stem}` evidence `{path}` is not distributed: only docs/contracts/ ships"
                );
                assert!(
                    root.join(path).exists(),
                    "manifest `{stem}` evidence `{path}` does not exist in the repository"
                );
            }
        }
        for rule in document["model_rules"].as_array().expect("model_rules array") {
            if let Some(revision) = rule["capabilities"]["evidence_revision"].as_str() {
                assert!(
                    !revision.contains('/'),
                    "manifest `{stem}` evidence revision `{revision}` must stay an identifier, not a path"
                );
            }
        }
    }
}

/// Asserts every production target is covered by exactly one executable fixture.
/// 断言每个生产目标恰由一个可执行 fixture 覆盖。
#[test]
fn every_production_target_is_covered_by_one_fixture() {
    let manifests = read_json_directory(&provider_manifests_root().join("manifests"));
    let mut coverage: BTreeMap<String, usize> = BTreeMap::new();
    for (fixture_id, text) in read_json_directory(&provider_manifests_root().join("fixtures")) {
        let fixture = parse_manifest(&fixture_id, &text);
        let Some(references) = fixture["production_target_refs"].as_array() else {
            continue;
        };
        for reference in references {
            let reference = reference.as_str().expect("production target reference");
            *coverage.entry(reference.to_string()).or_insert(0) += 1;
        }
    }
    for (stem, text) in &manifests {
        let document = parse_manifest(stem, text);
        for target in document["tool_targets"].as_array().expect("tool_targets array") {
            let target_id = target["target_id"].as_str().expect("target identifier");
            let reference = format!("{stem}::{target_id}");
            let covered = coverage.get(&reference).copied().unwrap_or(0);
            assert_eq!(
                covered, 1,
                "production target `{reference}` must be covered by exactly one executable fixture"
            );
        }
    }
}

/// Asserts every availability gate names a model the same manifest declares.
/// 断言每个可用性门控指名的模型都由同一清单自身声明，不得把门控指向别处的身份空间。
#[test]
fn every_availability_gate_stays_inside_its_own_manifest() {
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        let mut declared = BTreeSet::new();
        for rule in document["model_rules"].as_array().expect("model_rules array") {
            if let Some(value) = rule["matcher"]["value"].as_str() {
                declared.insert(value.to_string());
            }
        }
        for service in document["services"].as_array().expect("services array") {
            let Some(models) = service["model_catalog"]["models"].as_array() else {
                continue;
            };
            for model in models {
                if let Some(model_id) = model["model_id"].as_str() {
                    declared.insert(model_id.to_string());
                }
            }
        }
        for target in document["tool_targets"].as_array().expect("tool_targets array") {
            let target_id = target["target_id"].as_str().expect("target identifier");
            let Some(gates) = target["availability_model_ids_any"].as_array() else {
                continue;
            };
            for gate in gates {
                let gate = gate.as_str().expect("availability model identifier");
                assert!(
                    declared.contains(gate),
                    "target `{target_id}` in manifest `{stem}` gates on undeclared model `{gate}`"
                );
            }
        }
    }
}

/// Returns the common model catalog document shipped beside this crate.
/// 返回与本 crate 一同发布的通用模型目录文档。
fn common_model_catalog() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("common_model_capabilities.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("read {}", path.display()));
    serde_json::from_str(&text).expect("common model catalog must parse")
}

/// Returns every exact model identity our provider manifests configure.
/// 返回我们所有供应商清单配置的精确模型身份。
fn configured_model_identities() -> BTreeSet<String> {
    let mut identities = BTreeSet::new();
    for (stem, text) in read_json_directory(&provider_manifests_root().join("manifests")) {
        let document = parse_manifest(&stem, &text);
        for rule in document["model_rules"].as_array().expect("model_rules array") {
            // Only exact matchers name a model identity; prefix matchers are family selectors.
            // 只有 exact 匹配器才指向一个模型身份；prefix 匹配器是族选择器。
            if rule["matcher"]["kind"].as_str() != Some("exact") {
                continue;
            }
            let value = rule["matcher"]["value"]
                .as_str()
                .expect("exact matcher value")
                .to_lowercase();
            identities.insert(value);
        }
    }
    assert!(
        !identities.is_empty(),
        "bundled manifests must configure at least one exact model identity"
    );
    identities
}

/// Returns every catalog lookup key, including declared aliases.
/// 返回目录的全部查询键，含已声明的别名。
fn catalog_lookup_keys(catalog: &Value) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for model in catalog["models"].as_array().expect("catalog models array") {
        keys.insert(model["key"].as_str().expect("catalog key").to_lowercase());
        for alias in model["aliases"].as_array().expect("catalog aliases array") {
            keys.insert(alias.as_str().expect("catalog alias").to_lowercase());
        }
    }
    keys
}

/// Asserts the common model catalog resolves every model our manifests configure.
/// 断言通用目录能解析我们清单配置的每一个模型。
///
/// This is the machine-checked form of "one shared catalog for every model we configured": adding
/// a provider or a model fails this check until the catalog is regenerated, and regenerating it
/// never requires editing this file.
/// 这是"为我们配置过的每个模型维护一份共享目录"的机器化形式：增删供应商或模型都会让本检测
/// 失败直到目录被重新生成，而重新生成目录永远不需要修改本文件。
#[test]
fn every_configured_model_is_resolvable_in_the_common_catalog() {
    let lookup = catalog_lookup_keys(&common_model_catalog());
    for identity in configured_model_identities() {
        let tail = identity.rsplit('/').next().unwrap_or(identity.as_str());
        assert!(
            lookup.contains(&identity) || lookup.contains(tail),
            "configured model `{identity}` must be resolvable in the common model catalog"
        );
    }
}

/// Asserts the common model catalog never becomes a pull of an upstream model list.
/// 断言通用目录绝不会变成上游模型清单的拉取结果。
///
/// Upstream sources are cross references for context and modality metadata only: every catalog
/// record must still name a model our own manifests configure.
/// 上游来源只作为上下文与模态元数据的交叉参照：每条目录记录仍必须指向我们清单配置的模型。
#[test]
fn the_common_catalog_contains_only_configured_models() {
    let identities = configured_model_identities();
    let tails = identities
        .iter()
        .map(|identity| identity.rsplit('/').next().unwrap_or(identity.as_str()))
        .collect::<BTreeSet<_>>();
    let catalog = common_model_catalog();
    for model in catalog["models"].as_array().expect("catalog models array") {
        let key = model["key"].as_str().expect("catalog key").to_lowercase();
        let tail = key.rsplit('/').next().unwrap_or(key.as_str()).to_string();
        assert!(
            identities.contains(&key) || tails.contains(tail.as_str()),
            "common model catalog record `{key}` is not a model any bundled manifest configures"
        );
    }
}
