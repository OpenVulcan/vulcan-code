//! Strict sidecar schemas, discovery, parsing, and atomic YAML persistence.
//! 严格 sidecar schema、发现、解析与原子 YAML 持久化。

use super::capability_policy::{
    SubagentPromptSkillPolicySidecar, SubagentToolExposureSidecar, SubagentToolPolicySidecar,
};
use super::directory_scan::catalog_directory_entry;
use super::path_rules::{
    JSON_EXTENSION, YAML_EXTENSION, YML_EXTENSION, path_has_json_extension,
    path_has_sidecar_extension,
};
use super::{CATEGORY_METADATA_FILE_NAME, SubagentCatalogLoadError};
use crate::atomic_file::write_file_atomically;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use vulcan_session_core::{
    ContextCompactionPolicyRequest, ExecutionRuntimePolicyRequest, ProviderModelSnapshot,
};

/// Current schema version accepted for definition and category sidecars.
/// 定义与分类 sidecar 当前接受的 schema 版本。
pub(super) const SUBAGENT_SIDECAR_SCHEMA_VERSION: u32 = 1;

/// Definition sidecar upstream or license source metadata.
/// 定义 sidecar 上游或协议来源元数据。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentOriginSidecar {
    /// Source kind such as github, system, or other.
    /// 来源类型，例如 github、system 或 other。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) kind: Option<String>,
    /// Optional human-facing source label.
    /// 可选的人类可读来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) label: Option<String>,
    /// Optional icon key used by WebUI.
    /// WebUI 使用的可选图标键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) icon: Option<String>,
    /// Optional source URL.
    /// 可选来源链接。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) url: Option<String>,
}

/// Definition sidecar localized metadata.
/// 定义 sidecar 本地化元数据。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentDefinitionLocaleSidecar {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    /// Optional localized description.
    /// 可选本地化说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
}

/// Sidecar metadata for one subagent definition.
/// 单个子代理定义的 sidecar 元数据。
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentDefinitionSidecar {
    /// Required schema version validated before this sidecar is applied.
    /// 应用该 sidecar 前必须校验的必需 schema 版本。
    pub(super) schema_version: u32,
    /// Optional display name.
    /// 可选显示名称。
    #[serde(default)]
    pub(super) name: Option<String>,
    /// Optional description.
    /// 可选说明。
    #[serde(default)]
    pub(super) description: Option<String>,
    /// Optional display emoji.
    /// 可选展示图标。
    #[serde(default)]
    pub(super) emoji: Option<String>,
    /// Optional display color.
    /// 可选展示颜色。
    #[serde(default)]
    pub(super) color: Option<String>,
    /// Optional short tone or persona hint.
    /// 可选简短语气或人格提示。
    #[serde(default)]
    pub(super) vibe: Option<String>,
    /// Optional localized display metadata.
    /// 可选本地化展示元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) locales: Option<BTreeMap<String, SubagentDefinitionLocaleSidecar>>,
    /// Optional upstream or license source metadata.
    /// 可选上游或协议来源元数据。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) source: Option<SubagentOriginSidecar>,
    /// Optional enabled state.
    /// 可选启用状态。
    #[serde(default)]
    pub(super) enabled: Option<bool>,
    /// Optional appended system prompt.
    /// 可选追加系统提示词。
    #[serde(default)]
    pub(super) system_prompt_append: Option<String>,
    /// Provider and model override snapshot.
    /// 供应商与模型覆盖快照。
    #[serde(default)]
    pub(super) provider_model: ProviderModelSnapshot,
    /// Family-specific tool exposure payload.
    /// 分族工具可见性载荷。
    #[serde(default, skip_serializing_if = "SubagentToolExposureSidecar::is_empty")]
    pub(super) tool_exposure: SubagentToolExposureSidecar,
    /// Unified tool policy.
    /// 统一工具策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) tool_policy: Option<SubagentToolPolicySidecar>,
    /// Unified prompt skill policy.
    /// 统一提示词技能策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) prompt_skill_policy: Option<SubagentPromptSkillPolicySidecar>,
    /// Context compaction override snapshot.
    /// 上下文压缩覆盖快照。
    #[serde(default)]
    pub(super) context_compaction: ContextCompactionPolicyRequest,
    /// Runtime policy override request.
    /// 运行时策略覆盖请求。
    #[serde(default)]
    pub(super) runtime_policy: ExecutionRuntimePolicyRequest,
    /// Optional maximum nested task depth.
    /// 可选最大嵌套任务深度。
    #[serde(default)]
    pub(super) max_depth: Option<u16>,
    /// Optional user-facing package id.
    /// 可选的用户可见包标识。
    #[serde(default, rename = "packageId")]
    pub(super) package_id: Option<String>,
    /// Optional remote source URL.
    /// 可选远程来源 URL。
    #[serde(default)]
    pub(super) remote_url: Option<String>,
    /// Optional system definition id overridden by this metadata.
    /// 当前元数据覆盖的可选系统定义标识。
    #[serde(default)]
    pub(super) override_of: Option<String>,
    /// Optional source hash.
    /// 可选来源哈希。
    #[serde(default)]
    pub(super) source_hash: Option<String>,
    /// Optional source version.
    /// 可选来源版本。
    #[serde(default)]
    pub(super) version: Option<String>,
    /// Optional remote ETag.
    /// 可选远程 ETag。
    #[serde(default)]
    pub(super) etag: Option<String>,
    /// Optional remote Last-Modified header.
    /// 可选远程 Last-Modified 头。
    #[serde(default)]
    pub(super) last_modified: Option<String>,
    /// Optional update timestamp.
    /// 可选更新时间戳。
    #[serde(default)]
    pub(super) updated_at: Option<String>,
    /// Optional extension payload.
    /// 可选扩展载荷。
    #[serde(default)]
    pub(super) extra: Option<Value>,
}

/// Category sidecar metadata.
/// 分类 sidecar 元数据。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentCategorySidecar {
    /// Required schema version validated before this category metadata is applied.
    /// 应用分类元数据前必须校验的必需 schema 版本。
    pub(super) schema_version: u32,
    /// Optional display name.
    /// 可选显示名称。
    #[serde(default)]
    pub(super) name: Option<String>,
    /// Optional description.
    /// 可选说明。
    #[serde(default)]
    pub(super) description: Option<String>,
    /// Optional display emoji.
    /// 可选展示图标。
    #[serde(default)]
    pub(super) emoji: Option<String>,
    /// Optional display color.
    /// 可选展示颜色。
    #[serde(default)]
    pub(super) color: Option<String>,
    /// Optional display order.
    /// 可选展示顺序。
    #[serde(default)]
    pub(super) order: Option<i32>,
    /// Optional localized display metadata.
    /// 可选本地化展示元数据。
    #[serde(default)]
    pub(super) locales: BTreeMap<String, SubagentCategoryLocaleSidecar>,
}

/// Category sidecar localized metadata.
/// 分类 sidecar 本地化元数据。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SubagentCategoryLocaleSidecar {
    /// Optional localized display name.
    /// 可选本地化显示名称。
    #[serde(default)]
    pub(super) name: Option<String>,
    /// Optional localized description.
    /// 可选本地化说明。
    #[serde(default)]
    pub(super) description: Option<String>,
}

/// Serializes and atomically publishes one YAML sidecar document.
/// 序列化并原子发布一个 YAML sidecar 文档。
///
/// # Parameters
/// 参数
/// `path` is the final YAML path that receives the complete serialized document.
/// `path` 是接收完整序列化文档的最终 YAML 路径。
/// `sidecar` is the fully constructed protocol value serialized before filesystem mutation.
/// `sidecar` 是在修改文件系统前完成构造并等待序列化的协议值。
/// `artifact_kind` identifies the sidecar type in diagnostics.
/// `artifact_kind` 在诊断中标识 sidecar 类型。
///
/// # Returns
/// 返回值
/// The published final path, or a path-aware serialization or persistence error.
/// 返回已发布的最终路径，或包含路径的序列化或持久化错误。
pub(super) fn write_yaml_sidecar_atomically<T>(
    path: &Path,
    sidecar: &T,
    artifact_kind: &str,
) -> Result<PathBuf, SubagentCatalogLoadError>
where
    T: Serialize,
{
    // yaml is complete in memory before staging or replacing any filesystem entry.
    // yaml 在暂存或替换任何文件系统目录项前已经在内存中完整生成。
    let yaml = serde_yaml::to_string(sidecar).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to serialize {artifact_kind}: {error}"),
    })?;
    write_file_atomically(path, yaml.as_bytes(), artifact_kind).map_err(|message| {
        SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message,
        }
    })?;
    Ok(path.to_path_buf())
}

/// Reads one definition sidecar from YAML, YML, or JSON.
/// 从 YAML、YML 或 JSON 读取单个定义 sidecar。
///
/// # Parameters
/// 参数
/// `path` identifies the definition sidecar file.
/// `path` 标识定义 sidecar 文件。
///
/// # Returns
/// 返回值
/// A strict current-version DTO, or a path-aware read, parse, or version error.
/// 返回严格的当前版本 DTO，或包含路径的读取、解析或版本错误。
pub(super) fn read_definition_sidecar(
    path: &Path,
) -> Result<SubagentDefinitionSidecar, SubagentCatalogLoadError> {
    let sidecar: SubagentDefinitionSidecar =
        read_sidecar_document(path, "subagent definition sidecar")?;
    validate_sidecar_schema_version(path, "subagent definition sidecar", sidecar.schema_version)?;
    Ok(sidecar)
}

/// Reads one category sidecar from YAML, YML, or JSON.
/// 从 YAML、YML 或 JSON 读取单个分类 sidecar。
///
/// # Parameters
/// 参数
/// `path` identifies the category sidecar file.
/// `path` 标识分类 sidecar 文件。
///
/// # Returns
/// 返回值
/// A strict current-version DTO, or a path-aware read, parse, or version error.
/// 返回严格的当前版本 DTO，或包含路径的读取、解析或版本错误。
pub(super) fn read_category_sidecar(
    path: &Path,
) -> Result<SubagentCategorySidecar, SubagentCatalogLoadError> {
    let sidecar: SubagentCategorySidecar =
        read_sidecar_document(path, "subagent category sidecar")?;
    validate_sidecar_schema_version(path, "subagent category sidecar", sidecar.schema_version)?;
    Ok(sidecar)
}

/// Reads and deserializes one sidecar document according to its file extension.
/// 根据文件扩展名读取并反序列化一个 sidecar 文档。
///
/// # Parameters
/// 参数
/// `path` identifies the YAML or JSON sidecar file to read.
/// `path` 标识要读取的 YAML 或 JSON sidecar 文件。
/// `artifact_label` supplies precise error context for the owning sidecar protocol.
/// `artifact_label` 为所属 sidecar 协议提供精确的错误上下文。
///
/// # Returns
/// 返回值
/// The deserialized sidecar value, or a path-aware catalog load error.
/// 返回反序列化后的 sidecar 值，或包含路径信息的目录加载错误。
fn read_sidecar_document<T>(
    path: &Path,
    artifact_label: &str,
) -> Result<T, SubagentCatalogLoadError>
where
    T: DeserializeOwned,
{
    let raw = fs::read_to_string(path).map_err(|error| SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!("Failed to read {artifact_label}: {error}"),
    })?;
    if path_has_json_extension(path) {
        serde_json::from_str(&raw).map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to parse {artifact_label} JSON: {error}"),
        })
    } else {
        serde_yaml::from_str(&raw).map_err(|error| SubagentCatalogLoadError {
            path: path.to_path_buf(),
            message: format!("Failed to parse {artifact_label} YAML: {error}"),
        })
    }
}

/// Rejects sidecars whose declared schema version is not the current protocol version.
/// 拒绝声明版本不是当前协议版本的 sidecar。
///
/// # Parameters
/// 参数
/// `path` identifies the sidecar file for path-aware diagnostics.
/// `path` 标识 sidecar 文件，以生成包含路径的诊断。
/// `artifact_label` describes whether the document is a definition or category sidecar.
/// `artifact_label` 描述该文档是定义 sidecar 还是分类 sidecar。
/// `schema_version` is the required version declared by the document.
/// `schema_version` 是文档声明的必需版本。
///
/// # Returns
/// 返回值
/// `Ok(())` for the current version, otherwise an explicit unsupported-version error.
/// 当前版本返回 `Ok(())`，否则返回明确的不支持版本错误。
fn validate_sidecar_schema_version(
    path: &Path,
    artifact_label: &str,
    schema_version: u32,
) -> Result<(), SubagentCatalogLoadError> {
    if schema_version == SUBAGENT_SIDECAR_SCHEMA_VERSION {
        return Ok(());
    }
    Err(SubagentCatalogLoadError {
        path: path.to_path_buf(),
        message: format!(
            "Unsupported {artifact_label} schema version {schema_version}; expected {SUBAGENT_SIDECAR_SCHEMA_VERSION}."
        ),
    })
}

/// Returns the first matching sidecar path for one Markdown definition.
/// 返回单个 Markdown 定义的第一个匹配 sidecar 路径。
///
/// # Parameters
/// 参数
/// `definition_file` is the Markdown path whose sibling metadata should be resolved.
/// `definition_file` 是需要解析同级元数据的 Markdown 路径。
///
/// # Returns
/// 返回值
/// The first existing YAML, YML, or JSON sibling according to protocol precedence.
/// 按协议优先级返回第一个存在的 YAML、YML 或 JSON 同级文件。
pub(super) fn find_definition_sidecar(definition_file: &Path) -> Option<PathBuf> {
    let base = definition_file.with_extension("");
    [YAML_EXTENSION, YML_EXTENSION, JSON_EXTENSION]
        .iter()
        .map(|extension| base.with_extension(extension))
        .find(|path| path_is_regular_file_without_following(path))
}

/// Returns the first matching category sidecar in one directory.
/// 返回某个目录中的第一个匹配分类 sidecar。
///
/// # Parameters
/// 参数
/// `directory` is the category directory whose metadata should be resolved.
/// `directory` 是需要解析元数据的分类目录。
///
/// # Returns
/// 返回值
/// The first existing category YAML, YML, or JSON path according to protocol precedence.
/// 按协议优先级返回第一个存在的分类 YAML、YML 或 JSON 路径。
pub(super) fn find_category_sidecar(directory: &Path) -> Option<PathBuf> {
    [YAML_EXTENSION, YML_EXTENSION, JSON_EXTENSION]
        .iter()
        .map(|extension| directory.join(format!("{CATEGORY_METADATA_FILE_NAME}.{extension}")))
        .find(|path| path_is_regular_file_without_following(path))
}

/// Returns whether one sidecar candidate is a real regular file without following links.
/// 返回一个 sidecar 候选是否为不跟随链接的真实普通文件。
///
/// # Parameters
/// 参数
/// `path` is the exact sidecar candidate to inspect.
/// `path` 是需要检查的精确 sidecar 候选路径。
///
/// # Returns
/// 返回值
/// `true` only for a directly stored regular file; links, directories, and absent paths are false.
/// 仅对直接存储的普通文件返回 `true`；链接、目录和缺失路径均返回 `false`。
fn path_is_regular_file_without_following(path: &Path) -> bool {
    matches!(
        fs::symlink_metadata(path),
        Ok(metadata) if metadata.file_type().is_file()
    )
}

/// Recursively collects definition override sidecars from one root.
/// 从一个根目录递归收集定义覆盖 sidecar。
///
/// # Parameters
/// 参数
/// `root` is the current directory to scan recursively.
/// `root` 是当前递归扫描目录。
/// `sidecars` receives every matching non-category sidecar path.
/// `sidecars` 接收每个匹配的非分类 sidecar 路径。
/// `errors` receives directory traversal failures without hiding successful siblings.
/// `errors` 接收目录遍历失败，同时不隐藏成功的同级项。
///
/// # Returns
/// 返回值
/// This in-place collector has no return value; paths and errors are appended to their outputs.
/// 该原地收集器没有返回值；路径与错误会追加到对应输出。
pub(super) fn collect_sidecar_files(
    root: &Path,
    sidecars: &mut Vec<PathBuf>,
    errors: &mut Vec<SubagentCatalogLoadError>,
) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(SubagentCatalogLoadError {
                path: root.to_path_buf(),
                message: format!("Failed to read override sidecar directory: {error}"),
            });
            return;
        }
    };
    for entry_result in entries {
        // entry follows the shared non-following type and symbolic-link policy.
        // entry 遵循共享的不跟随类型与符号链接策略。
        let Some((entry, file_type)) =
            catalog_directory_entry(entry_result, root, "subagent override", errors)
        else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_sidecar_files(&path, sidecars, errors);
        } else if file_type.is_file()
            && path_has_sidecar_extension(&path)
            && path.file_stem().and_then(|stem| stem.to_str()) != Some(CATEGORY_METADATA_FILE_NAME)
        {
            sidecars.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies legacy policy maps and global fields are rejected by the current sidecar schema.
    /// 验证当前 sidecar schema 会拒绝旧版策略映射与全局字段。
    #[test]
    fn rejects_legacy_policy_shapes() {
        assert!(
            serde_yaml::from_str::<SubagentDefinitionSidecar>(
                "schemaVersion: 1\ntoolPolicy:\n  shell_command: allow\n"
            )
            .is_err()
        );
        assert!(
            serde_yaml::from_str::<SubagentDefinitionSidecar>(
                "schemaVersion: 1\npromptSkillPolicy:\n  skill-creator: deny\n"
            )
            .is_err()
        );
        assert!(
            serde_yaml::from_str::<SubagentDefinitionSidecar>(
                "schemaVersion: 1\ntoolExposure:\n  allowed_tools:\n    - shell_command\n"
            )
            .is_err()
        );
        assert!(
            serde_yaml::from_str::<SubagentDefinitionSidecar>(
                "schemaVersion: 1\ntoolExposure:\n  policy:\n    mode: allow_only\n    selectors:\n      - shell_command\n"
            )
            .is_err()
        );
    }

    /// Verifies sidecar readers require version one and reject undeclared protocol fields.
    /// 验证 sidecar 读取器要求版本一并拒绝协议未声明字段。
    #[test]
    fn sidecar_readers_enforce_current_schema_and_known_fields() {
        // root owns isolated definition and category files used to exercise the actual read path.
        // root 持有用于执行真实读取路径的隔离定义与分类文件。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-sidecar-schema-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("sidecar schema test root");
        // definition_path represents one definition sidecar consumed by the production reader.
        // definition_path 表示由生产读取器消费的单个定义 sidecar。
        let definition_path = root.join("reviewer.yaml");
        fs::write(&definition_path, "name: Reviewer\n").expect("missing-version definition");
        let error = read_definition_sidecar(&definition_path)
            .expect_err("definition sidecar without schemaVersion must fail");
        assert!(error.message.contains("missing field"));
        assert!(error.message.contains("schemaVersion"));

        fs::write(&definition_path, "schemaVersion: 2\nname: Reviewer\n")
            .expect("unsupported-version definition");
        let error = read_definition_sidecar(&definition_path)
            .expect_err("unsupported definition sidecar version must fail");
        assert_eq!(
            error.message,
            "Unsupported subagent definition sidecar schema version 2; expected 1."
        );

        fs::write(&definition_path, "schemaVersion: 1\nunknownField: true\n")
            .expect("unknown-field definition");
        let error = read_definition_sidecar(&definition_path)
            .expect_err("unknown definition sidecar field must fail");
        assert!(error.message.contains("unknown field `unknownField`"));

        fs::write(&definition_path, "schemaVersion: 1\nname: Reviewer\n")
            .expect("current-version definition");
        assert_eq!(
            read_definition_sidecar(&definition_path)
                .expect("current definition sidecar must load")
                .schema_version,
            SUBAGENT_SIDECAR_SCHEMA_VERSION
        );

        // json_definition_path exercises the JSON branch with the same mandatory version contract.
        // json_definition_path 使用相同的强制版本契约执行 JSON 分支。
        let json_definition_path = root.join("reviewer.json");
        fs::write(
            &json_definition_path,
            r#"{"schemaVersion":1,"name":"Reviewer"}"#,
        )
        .expect("current-version JSON definition");
        assert_eq!(
            read_definition_sidecar(&json_definition_path)
                .expect("current JSON definition sidecar must load")
                .schema_version,
            SUBAGENT_SIDECAR_SCHEMA_VERSION
        );

        // category_path represents one category sidecar consumed by the production reader.
        // category_path 表示由生产读取器消费的单个分类 sidecar。
        let category_path = root.join("_category.yaml");
        fs::write(&category_path, "schemaVersion: 0\nname: Engineering\n")
            .expect("unsupported-version category");
        let error = read_category_sidecar(&category_path)
            .expect_err("unsupported category sidecar version must fail");
        assert_eq!(
            error.message,
            "Unsupported subagent category sidecar schema version 0; expected 1."
        );

        fs::write(&category_path, "schemaVersion: 1\nlegacyCategory: true\n")
            .expect("unknown-field category");
        let error = read_category_sidecar(&category_path)
            .expect_err("unknown category sidecar field must fail");
        assert!(error.message.contains("unknown field `legacyCategory`"));

        fs::write(&category_path, "schemaVersion: 1\nname: Engineering\n")
            .expect("current-version category");
        assert_eq!(
            read_category_sidecar(&category_path)
                .expect("current category sidecar must load")
                .schema_version,
            SUBAGENT_SIDECAR_SCHEMA_VERSION
        );

        fs::remove_dir_all(root).expect("sidecar schema test root should be removable");
    }

    /// Verifies definition sidecar discovery never follows a symbolic-link candidate.
    /// 验证定义 sidecar 发现绝不会跟随符号链接候选。
    #[cfg(unix)]
    #[test]
    fn definition_sidecar_discovery_ignores_symbolic_link_candidate() {
        use std::os::unix::fs::symlink;

        // root owns a Markdown path, a real metadata target, and its sidecar link.
        // root 持有 Markdown 路径、真实元数据目标及其 sidecar 链接。
        let root = std::env::temp_dir().join(format!(
            "vulcan-subagent-sidecar-link-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("sidecar link test root");
        // definition_path supplies the base name used by sidecar discovery.
        // definition_path 提供 sidecar 发现使用的基础名称。
        let definition_path = root.join("reviewer.md");
        fs::write(&definition_path, "Review.").expect("definition file");
        // metadata_target is a real file that must not become trusted through a link.
        // metadata_target 是不得通过链接获得信任的真实文件。
        let metadata_target = root.join("external.yaml");
        fs::write(&metadata_target, "schemaVersion: 1\n").expect("metadata target");
        // sidecar_link occupies the conventional sidecar path but is not a regular file itself.
        // sidecar_link 占用规范 sidecar 路径，但自身不是普通文件。
        let sidecar_link = root.join("reviewer.yaml");
        symlink(&metadata_target, &sidecar_link).expect("sidecar symlink");

        assert!(find_definition_sidecar(&definition_path).is_none());

        fs::remove_file(&sidecar_link).expect("sidecar link should be removable");
        fs::remove_dir_all(root).expect("sidecar link test root should be removable");
    }

    /// Verifies override discovery retains the operating-system reason for an unreadable root.
    /// 验证覆盖发现会保留无法读取根路径的操作系统原因。
    #[test]
    fn collect_sidecar_files_reports_underlying_directory_error() {
        // root_file is a real file passed where the collector requires a directory.
        // root_file 是在收集器要求目录的位置传入的真实文件。
        let root_file = std::env::temp_dir().join(format!(
            "vulcan-subagent-sidecar-collector-error-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after unix epoch")
                .as_nanos()
        ));
        fs::write(&root_file, b"not a directory").expect("collector error fixture");
        // sidecars receives discovered paths and must remain empty after the root failure.
        // sidecars 接收发现的路径，并且在根路径失败后必须保持为空。
        let mut sidecars = Vec::new();
        // errors receives the path-aware failure including the operating-system reason.
        // errors 接收包含操作系统原因和路径信息的失败。
        let mut errors = Vec::new();

        collect_sidecar_files(&root_file, &mut sidecars, &mut errors);

        fs::remove_file(&root_file).expect("collector error fixture should be removable");
        assert!(sidecars.is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, root_file);
        assert!(
            errors[0]
                .message
                .starts_with("Failed to read override sidecar directory: ")
        );
    }
}
