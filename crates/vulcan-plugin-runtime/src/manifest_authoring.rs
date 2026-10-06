//! Explicit manifest authoring; runtime startup never invokes these transformations.
//! 显式清单编写工具；运行时启动绝不调用这些转换。

use crate::{
    VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION, VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION,
    VulcanPluginManifest, VulcanPluginReusableToolManifest, VulcanPluginRuntimeRequirements,
    VulcanPluginSettingsLayout, VulcanPluginToolExecutionKind, VulcanPluginToolExecutionManifest,
    VulcanPluginUiMountKind, VulcanPluginUiMountManifest,
};
use schemars::{JsonSchema, Schema, generate::SchemaSettings};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

/// Remove an absent-only field's synthetic null default from schema; Serde still owns omission handling.
/// 从 schema 中移除仅可省略字段的合成空默认值；省略处理仍由 Serde 拥有。
/// Object schemas may carry defaults; boolean schemas have no default member to remove.
/// 对象 Schema 可携带默认值；布尔 Schema 没有需要移除的默认成员。
pub(crate) fn remove_schema_default(schema: &mut Schema) {
    if let Some(object) = schema.as_object_mut() {
        object.remove("default");
    }
}

/// Generate all complete and fragment artifacts from actual Rust input types, returning canonical UTF-8 bytes.
/// 从真实 Rust 输入类型生成完整及片段产物，返回规范 UTF-8 字节。
/// Version rules additionally mirror the catalog's explicit v1/v2 admission gates; live policy remains runtime-owned.
/// 版本规则额外对应目录的明确一／二版入场门控；实时策略仍由运行时拥有。
pub fn generated_schemas() -> Result<BTreeMap<&'static str, Vec<u8>>, String> {
    // The generator version and draft are pinned; Deserialize is explicitly the input contract.
    // 生成器版本及规范草案均固定；Deserialize 明确作为输入契约。
    let mut manifest = input_schema::<VulcanPluginManifest>()?;
    manifest["properties"]["schema_version"]["enum"] = json!([
        VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION,
        VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION
    ]);
    manifest["allOf"] = json!([
        {
            "if":{"properties":{"schema_version":{"const":VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION}}},
            "then":{
                "not":{"required":["runtime_requirements"]},
                "properties":{"lua_runtime":{"properties":{"tools":{"items":{"properties":{
                    "execution":{"not":{"required":["reusable"]}}
                }}}}}}
            }
        },
        {
            "if":{"properties":{"schema_version":{"const":VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION}}},
            "then":{
                "if":{"required":["lua_runtime"],"properties":{"lua_runtime":{"type":"object"}}},
                "then":{"required":["runtime_requirements"]},
                "else":{"not":{"required":["runtime_requirements"]}}
            }
        }
    ]);
    // Schemars uses the type-owned definition name, so refactoring an unrelated field cannot select another node.
    // Schemars 使用类型拥有的定义名，因此无关字段重构不能选中其他节点。
    let execution_name = VulcanPluginToolExecutionManifest::schema_name();
    let execution = manifest["$defs"]
        .get_mut(execution_name.as_ref())
        .ok_or_else(|| "Generated manifest lost the tool execution definition.".to_string())?;
    execution["allOf"] = json!([{
        "if":{"required":["reusable"]},
        "then":{"properties":{"kind":{"const": serde_json::to_value(
            // Select the real Serde spelling rather than copying a second enum table.
            // 选择真实 Serde 拼写，而非复制第二份枚举表。
            VulcanPluginToolExecutionKind::Stateless
        ).map_err(|error| error.to_string())?}}}
    }]);
    // Mount kinds mirror the catalog gate: modules need an export, settings layouts forbid one.
    // 挂载类型对应目录门控：模块必须声明导出，设置版面禁止声明导出。
    let mount_name = VulcanPluginUiMountManifest::schema_name();
    let mount = manifest["$defs"]
        .get_mut(mount_name.as_ref())
        .ok_or_else(|| "Generated manifest lost the UI mount definition.".to_string())?;
    let settings_kind = VulcanPluginUiMountKind::Settings.key();
    mount["allOf"] = json!([{
        "if":{"required":["kind"],"properties":{"kind":{"const":settings_kind}}},
        "then":{"not":{"required":["export"]}},
        "else":{"required":["export"]}
    }]);
    // One artifact set replaces both historical hand-written fragments as well as the full manifest.
    // 单个产物集合同时替换两份历史手写片段并提供完整清单。
    let schemas = [
        ("plugin-manifest.schema.json", manifest),
        (
            "plugin-runtime-requirements.schema.json",
            input_schema::<VulcanPluginRuntimeRequirements>()?,
        ),
        (
            "plugin-tool-reuse.schema.json",
            input_schema::<VulcanPluginReusableToolManifest>()?,
        ),
        (
            "plugin-settings-layout.schema.json",
            input_schema::<VulcanPluginSettingsLayout>()?,
        ),
    ];
    schemas
        .into_iter()
        .map(|(name, schema)| {
            let mut bytes =
                serde_json::to_vec_pretty(&schema).map_err(|error| error.to_string())?;
            bytes.push(b'\n');
            Ok((name, bytes))
        })
        .collect()
}

/// Check or explicitly regenerate the canonical artifacts under directory; check never writes.
/// 在 directory 下检查或显式重新生成规范产物；check 模式绝不写入。
/// Returns the first concrete drift or I/O error; each written file uses synchronized sibling publication.
/// 返回首个具体漂移或读写错误；每份写入文件均使用已同步同级文件发布。
pub fn sync_schemas(directory: &Path, check: bool) -> Result<(), String> {
    // Names come exclusively from the generator; callers cannot turn this into an arbitrary file replacement.
    // 名称完全来自生成器；调用方不能将此操作变成任意文件替换。
    for (name, bytes) in generated_schemas()? {
        let path = std::path::absolute(directory.join(name)).map_err(|error| error.to_string())?;
        if check {
            let actual =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            if actual != bytes {
                return Err(format!("Generated schema is stale: {}", path.display()));
            }
        } else {
            crate::atomic_file::write_file_atomically(&path, &bytes)?;
        }
    }
    Ok(())
}

/// Generate an offline Draft 2020-12 schema for input type T and return its JSON object.
/// 为输入类型 T 生成离线 Draft 2020-12 Schema 并返回其 JSON 对象。
fn input_schema<T: JsonSchema>() -> Result<Value, String> {
    let schema = SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<T>();
    serde_json::to_value(schema).map_err(|error| error.to_string())
}

/// Decode and semantically validate source bytes without reading files, starting runtimes or granting permissions.
/// 解码并语义校验 source 字节，不读取文件、启动运行时或授予权限。
/// package_id is the exact containing directory identity; success returns the production manifest type.
/// package_id 为所属目录的精确身份；成功时返回生产清单类型。
pub fn validate_manifest(source: &[u8], package_id: &str) -> Result<VulcanPluginManifest, String> {
    let manifest: VulcanPluginManifest = serde_yaml::from_slice(source)
        .map_err(|error| format!("Invalid plugin manifest: {error}"))?;
    // Generic YAML mappings reject duplicate keys before any JSON map could collapse them.
    // 通用 YAML 映射在任何 JSON 映射可能折叠键之前拒绝重复键。
    decode_authoring_value(source)?;
    crate::catalog::validate_manifest_identity(&manifest, package_id)?;
    Ok(manifest)
}

/// Validate package's unambiguous manifest and all production package resources without creating a runtime.
/// 校验 package 的无歧义清单及全部生产插件包资源，不创建运行时。
/// Returns the exact production descriptor; static success does not assert callback availability or host budget admission.
/// 返回精确生产描述符；静态成功不代表回调可用或宿主预算接纳。
pub fn validate_package(package: &Path) -> Result<crate::VulcanPluginDescriptor, String> {
    // Resolve the caller-selected package once; its directory name owns the manifest identity.
    // 对调用方选择的插件包解析一次；其目录名称拥有清单身份。
    let package = package.canonicalize().map_err(|error| error.to_string())?;
    let package_id = package
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Package directory identity is not valid UTF-8.".to_string())?;
    let source = std::fs::read(package.join("plugin.yaml")).map_err(|error| error.to_string())?;
    validate_manifest(&source, package_id)?;
    let descriptor = crate::inspect_user_plugin_package(&package)?;
    // A manifest edited during inspection cannot be reported as the same checked candidate.
    // 检查期间被编辑的清单不能被报告为同一已检查候选。
    if std::fs::read(package.join("plugin.yaml")).map_err(|error| error.to_string())? != source {
        return Err("Plugin manifest changed during authoring validation.".into());
    }
    Ok(descriptor)
}

/// Convert one valid legacy source to a new v2 YAML image using explicit requirements for Lua packages.
/// 使用 Lua 插件明确提供的 requirements，将一份有效旧版 source 转换为新的第二版 YAML 内容。
/// package_id authenticates the declared identity; non-Lua packages reject requirements. No input file is modified.
/// package_id 校验声明身份；非 Lua 插件拒绝 requirements。不修改任何输入文件。
pub fn migrate_manifest(
    source: &[u8],
    package_id: &str,
    requirements: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    // Typed parsing happens before generic projection so duplicate struct fields cannot disappear in a map.
    // 在通用投影前进行类型化解析，使重复结构字段不能在映射中消失。
    let original = validate_manifest(source, package_id)?;
    if original.schema_version != VULCAN_PLUGIN_LEGACY_MANIFEST_SCHEMA_VERSION {
        return Err("Migration requires an exact version-one manifest.".into());
    }
    let required = match (original.has_lua_runtime(), requirements) {
        (true, Some(bytes)) => {
            let _: VulcanPluginRuntimeRequirements = serde_yaml::from_slice(bytes)
                .map_err(|error| format!("Invalid explicit runtime requirements: {error}"))?;
            Some(decode_authoring_value(bytes)?)
        }
        (false, None) => None,
        (true, None) => {
            return Err("Lua migration requires an explicit runtime-requirements file.".into());
        }
        (false, Some(_)) => {
            return Err("Non-Lua migration does not accept runtime requirements.".into());
        }
    };
    // Only these two version-owned fields change; omitted defaults, permissions and execution topology stay intact.
    // 仅修改这两个版本拥有字段；省略的默认值、权限及执行拓扑保持原样。
    let mut value = decode_authoring_value(source)?;
    value["schema_version"] = json!(VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION);
    if let Some(required) = required {
        value["runtime_requirements"] = required;
    }
    let migrated = serde_yaml::to_string(&value)
        .map_err(|error| error.to_string())?
        .into_bytes();
    validate_manifest(&migrated, package_id)?;
    Ok(migrated)
}

/// Decode source as an unambiguous YAML document, then project its JSON-compatible value.
/// 将 source 解码为无歧义 YAML 文档，再投影为兼容 JSON 的值。
/// Duplicate keys, non-string mapping keys and YAML tags are explicit authoring errors.
/// 重复键、非字符串映射键及 YAML 标签均为明确编写错误。
fn decode_authoring_value(source: &[u8]) -> Result<Value, String> {
    let yaml: serde_yaml::Value = serde_yaml::from_slice(source)
        .map_err(|error| format!("Ambiguous or invalid YAML: {error}"))?;
    validate_yaml_mapping_keys(&yaml)?;
    serde_json::to_value(yaml).map_err(|error| error.to_string())
}

/// Recursively validate each YAML value's mapping keys before JSON serialization can stringify them.
/// 在 JSON 序列化可能将键字符串化前，递归校验每个 YAML value 的映射键。
/// Returns an error for non-string keys or tags; sequences and ordinary scalars preserve their exact values.
/// 非字符串键或标签返回错误；序列及普通标量保留精确值。
fn validate_yaml_mapping_keys(value: &serde_yaml::Value) -> Result<(), String> {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                if !matches!(key, serde_yaml::Value::String(_)) {
                    return Err("Manifest authoring requires string mapping keys.".into());
                }
                validate_yaml_mapping_keys(value)?;
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for item in items {
                validate_yaml_mapping_keys(item)?;
            }
        }
        serde_yaml::Value::Tagged(_) => {
            return Err("Manifest authoring does not accept YAML tags.".into());
        }
        _ => {}
    }
    Ok(())
}

/// Publish bytes to a new output path after writing and syncing a sibling temporary file.
/// 将 bytes 写入并同步同级临时文件后，发布到新的 output 路径。
/// Refuses every existing destination, including the source, links and hard links; parent must already exist.
/// 拒绝所有已存在目标，包含源文件、链接及硬链接；父目录必须已经存在。
pub fn write_new_manifest(output: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    // Absolute resolution is lexical; publication never overwrites an existing final path.
    // 绝对路径解析仅处理路径语义；最终路径发布绝不覆盖已有文件。
    let output = std::path::absolute(output).map_err(|error| error.to_string())?;
    let parent = output
        .parent()
        .ok_or_else(|| "Output has no parent directory.".to_string())?;
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    staged
        .write_all(bytes)
        .and_then(|_| staged.as_file().sync_all())
        .map_err(|error| error.to_string())?;
    staged
        .persist_noclobber(&output)
        .map_err(|error| format!("Cannot publish new manifest: {error}"))?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("Manifest published but directory sync failed: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests;
