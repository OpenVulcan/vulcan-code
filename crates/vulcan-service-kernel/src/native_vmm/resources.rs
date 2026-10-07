//! Native VMM embedded-resource catalog, user overrides, and persisted composition settings.
//! 原生 VMM 内置资源目录、用户覆盖与持久化组合设置。

#[path = "runtime_policy.rs"]
mod runtime_policy;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use vulcan_memorymesh_core::{
    ConfigBundle, MAX_RESOURCE_FILE_BYTES, MemoryMeshCore, NoisePolicy, ResourceLayer,
    embedded_resource_paths, embedded_resource_text,
};
use vulcan_service_api::{
    VmmNativeResourceCatalogSummary, VmmNativeResourceFileContent, VmmNativeResourceFileSummary,
    VmmNativeResourceSettingsSummary,
};

/// Persisted schema version for native VMM resource selections.
/// 原生 VMM 资源选择的持久化 Schema 版本。
const RESOURCE_SETTINGS_SCHEMA_VERSION: u32 = 1;
/// Stable configuration registration replaced by each validated resource generation.
/// 每次通过校验的资源代际都会替换的稳定配置注册名。
pub(super) const NATIVE_VMM_CONFIG_REGISTRATION_NAME: &str = "vmm-native-host";
/// Maximum accepted settings document size.
/// 可接受的设置文档最大字节数。
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

/// Stores the closed native VMM resource-selection document.
/// 保存封闭的原生 VMM 资源选择文档。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedResourceSettings {
    /// Exact persisted schema version.
    /// 准确的持久化 Schema 版本。
    schema_version: u32,
    /// Selected complete prompt bundle.
    /// 选择的完整提示词 Bundle。
    prompt_bundle: String,
    /// Selected PII locale combined after common rules.
    /// 在公共规则后组合的 PII 区域。
    pii_language: String,
    /// Selected Noise locale combined after common rules.
    /// 在公共规则后组合的噪音区域。
    noise_language: String,
    /// Whether Noise admission is enabled.
    /// 是否启用噪音准入。
    noise_enabled: bool,
    /// Whether semantic Noise matching is enabled.
    /// 是否启用语义噪音匹配。
    noise_semantic_enabled: bool,
    /// Default semantic Noise threshold.
    /// 默认语义噪音阈值。
    noise_semantic_threshold: f64,
}

impl Default for PersistedResourceSettings {
    /// Uses the copied Core's documented built-in defaults.
    /// 使用复制后 Core 已记录的内置默认值。
    fn default() -> Self {
        let bundle = ConfigBundle::default();
        Self {
            schema_version: RESOURCE_SETTINGS_SCHEMA_VERSION,
            prompt_bundle: bundle.prompt_bundle,
            pii_language: bundle.pii_language,
            noise_language: bundle.noise_language,
            noise_enabled: bundle.noise_policy.enabled,
            noise_semantic_enabled: bundle.noise_policy.semantic_enabled,
            noise_semantic_threshold: bundle.noise_policy.default_semantic_threshold,
        }
    }
}

impl From<PersistedResourceSettings> for VmmNativeResourceSettingsSummary {
    /// Projects the persisted settings into the browser-facing contract.
    /// 将持久化设置投影为面向浏览器的契约。
    fn from(value: PersistedResourceSettings) -> Self {
        Self {
            prompt_bundle: value.prompt_bundle,
            pii_language: value.pii_language,
            noise_language: value.noise_language,
            noise_enabled: value.noise_enabled,
            noise_semantic_enabled: value.noise_semantic_enabled,
            noise_semantic_threshold: value.noise_semantic_threshold,
        }
    }
}

impl From<VmmNativeResourceSettingsSummary> for PersistedResourceSettings {
    /// Adds the closed schema version to one validated browser-facing selection.
    /// 为一份已校验的浏览器选择补充封闭 Schema 版本。
    fn from(value: VmmNativeResourceSettingsSummary) -> Self {
        Self {
            schema_version: RESOURCE_SETTINGS_SCHEMA_VERSION,
            prompt_bundle: value.prompt_bundle,
            pii_language: value.pii_language,
            noise_language: value.noise_language,
            noise_enabled: value.noise_enabled,
            noise_semantic_enabled: value.noise_semantic_enabled,
            noise_semantic_threshold: value.noise_semantic_threshold,
        }
    }
}

/// Owns the exact user resource directory below the host-owned VMM data root.
/// 持有宿主 VMM 数据根目录下准确的用户资源目录。
#[derive(Clone, Debug)]
pub(super) struct NativeVmmResourceStore {
    /// Stable `.vulcan/code/vmm/config` directory.
    /// 稳定的 `.vulcan/code/vmm/config` 目录。
    config_root: PathBuf,
}

impl NativeVmmResourceStore {
    /// Creates one resource store without materializing the override directory.
    /// 创建资源存储，但不提前物化覆盖目录。
    #[must_use]
    pub(super) fn new(vmm_data_root: &Path) -> Self {
        Self {
            config_root: vmm_data_root.join("config"),
        }
    }

    /// Lists embedded resources, available selectors, and physical overrides.
    /// 列出内置资源、可用选择项与物理覆盖。
    ///
    /// # Errors
    /// Returns an error when the embedded catalog, settings, or an override file is invalid.
    /// 内置目录、设置或覆盖文件无效时返回错误。
    pub(super) fn catalog(&self) -> Result<VmmNativeResourceCatalogSummary, String> {
        let paths = embedded_paths()?;
        let settings = self.load_settings()?;
        validate_settings(&settings, &paths)?;
        let mut prompt_bundles = BTreeSet::new();
        let mut prompt_scenes = BTreeSet::new();
        let mut pii_languages = BTreeSet::new();
        let mut noise_languages = BTreeSet::new();
        let mut files = Vec::with_capacity(paths.len());
        for path in &paths {
            let descriptor = describe_resource(path)?;
            match descriptor.group.as_str() {
                "prompt" => {
                    prompt_bundles.insert(descriptor.selector.clone());
                    prompt_scenes.insert(descriptor.file_name.clone());
                }
                "pii" if descriptor.selector != "common" => {
                    pii_languages.insert(descriptor.selector.clone());
                }
                "noise" if descriptor.selector != "common" => {
                    noise_languages.insert(descriptor.selector.clone());
                }
                "pii" | "noise" => {}
                _ => return Err(format!("不支持的原生 VMM 资源分组：{path}")),
            }
            files.push(VmmNativeResourceFileSummary {
                path: path.clone(),
                group: descriptor.group,
                selector: descriptor.selector,
                file_name: descriptor.file_name,
                high_risk: is_prompt_path(path),
                has_user_override: self.override_exists(path)?,
            });
        }
        Ok(VmmNativeResourceCatalogSummary {
            override_root: vulcan_runtime_assets::native_host_path_text_from_path(
                &self.config_root,
            ),
            settings: settings.into(),
            prompt_bundles: prompt_bundles.into_iter().collect(),
            prompt_scenes: prompt_scenes.into_iter().collect(),
            pii_languages: pii_languages.into_iter().collect(),
            noise_languages: noise_languages.into_iter().collect(),
            files,
        })
    }

    /// Reads immutable built-in text and the exact effective user-over-built-in result.
    /// 读取不可变内置原文与准确的用户层优先生效结果。
    ///
    /// # Errors
    /// Returns an error for an unknown path, unsafe entry, unreadable UTF-8, or invalid file size.
    /// 路径未知、条目不安全、UTF-8 不可读或文件大小无效时返回错误。
    pub(super) fn read_file(&self, path: &str) -> Result<VmmNativeResourceFileContent, String> {
        ensure_catalog_path(path)?;
        let built_in_text = embedded_resource_text(path).map_err(|error| error.to_string())?;
        let user_override_text = self.read_override(path)?;
        let effective_text = user_override_text
            .as_ref()
            .cloned()
            .unwrap_or_else(|| built_in_text.clone());
        Ok(VmmNativeResourceFileContent {
            path: path.to_owned(),
            built_in_text,
            effective_text,
            has_user_override: user_override_text.is_some(),
            user_override_text,
            high_risk: is_prompt_path(path),
        })
    }

    /// Builds the current persisted composition for one exact vector space.
    /// 为一个准确向量空间构建当前持久化组合。
    ///
    /// # Errors
    /// Returns an error when settings or any user resource cannot be loaded and validated.
    /// 设置或任一用户资源无法加载和校验时返回错误。
    pub(super) fn current_bundle(
        &self,
        vector_space_fingerprint: String,
        embedding_dimension: u32,
    ) -> Result<ConfigBundle, String> {
        let paths = embedded_paths()?;
        let settings = self.load_settings()?;
        validate_settings(&settings, &paths)?;
        let user_layer = self.load_user_layer(&paths)?;
        self.compose_bundle(
            settings,
            user_layer,
            vector_space_fingerprint,
            embedding_dimension,
        )
    }

    /// Validates, persists, and activates new common composition selections.
    /// 校验、持久化并激活新的公共组合选择。
    ///
    /// # Errors
    /// Returns an error before activation when the selection or resulting Core snapshot is invalid.
    /// 选择或生成的 Core 快照无效时在激活前返回错误。
    pub(super) fn update_settings(
        &self,
        core: &MemoryMeshCore,
        settings: VmmNativeResourceSettingsSummary,
    ) -> Result<VmmNativeResourceCatalogSummary, String> {
        let paths = embedded_paths()?;
        let settings = PersistedResourceSettings::from(settings);
        validate_settings(&settings, &paths)?;
        let user_layer = self.load_user_layer(&paths)?;
        let active = core.active_snapshot().map_err(|error| error.to_string())?;
        let candidate = self.compose_bundle(
            settings.clone(),
            user_layer,
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        )?;
        validate_candidate_bundle(candidate)?;
        self.persist_settings(&settings)?;
        self.activate_current(core)?;
        self.catalog()
    }

    /// Validates one exact override, persists it, and activates a fresh immutable snapshot.
    /// 校验一份准确覆盖、持久化并激活新的不可变快照。
    ///
    /// # Errors
    /// Returns an error for an unknown path, missing edit confirmations, invalid content, or I/O failure.
    /// 路径未知、编辑确认缺失、内容无效或 I/O 失败时返回错误。
    pub(super) fn write_file(
        &self,
        core: &MemoryMeshCore,
        path: &str,
        content: String,
        confirmed_purpose: bool,
        confirmed_modification: bool,
    ) -> Result<VmmNativeResourceFileContent, String> {
        ensure_catalog_path(path)?;
        if !confirmed_purpose || !confirmed_modification {
            return Err(String::from(
                "原生 VMM 规则覆盖必须先确认文件用途与修改风险，并在保存时再次确认。",
            ));
        }
        let paths = embedded_paths()?;
        let settings = self.load_settings()?;
        let mut user_layer = self.load_user_layer(&paths)?;
        let mut prompt_bundle_files = Vec::new();
        if let Some(bundle_name) = prompt_bundle_from_path(path) {
            prompt_bundle_files = prompt_bundle_paths(&paths, bundle_name);
            for bundle_path in &prompt_bundle_files {
                if !user_layer.files.contains_key(bundle_path) {
                    let built_in = embedded_resource_text(bundle_path)
                        .map_err(|error| error.to_string())?
                        .into_bytes();
                    user_layer
                        .insert(bundle_path.clone(), built_in)
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        user_layer.files.remove(path);
        user_layer
            .insert(path.to_owned(), content.into_bytes())
            .map_err(|error| error.to_string())?;
        let mut validation_settings = settings;
        select_resource_for_validation(&mut validation_settings, path)?;
        let active = core.active_snapshot().map_err(|error| error.to_string())?;
        validate_candidate_bundle(self.compose_bundle(
            validation_settings,
            user_layer.clone(),
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        )?)?;
        if prompt_bundle_files.is_empty() {
            let bytes = user_layer
                .files
                .get(path)
                .ok_or_else(|| format!("待写入资源没有进入用户层：{path}"))?;
            self.persist_override(path, bytes)?;
        } else {
            self.persist_prompt_bundle(path, &prompt_bundle_files, &user_layer)?;
        }
        self.activate_current(core)?;
        self.read_file(path)
    }

    /// Restores one file's effective built-in text without creating a partial prompt layer.
    /// 恢复单个文件的内置生效文本，且不产生不完整提示词层。
    ///
    /// # Errors
    /// Returns an error for an unknown path, invalid neighboring overrides, or I/O failure.
    /// 路径未知、相邻覆盖无效或 I/O 失败时返回错误。
    pub(super) fn reset_file(
        &self,
        core: &MemoryMeshCore,
        path: &str,
    ) -> Result<VmmNativeResourceFileContent, String> {
        ensure_catalog_path(path)?;
        let paths = embedded_paths()?;
        let settings = self.load_settings()?;
        let mut user_layer = self.load_user_layer(&paths)?;
        if !user_layer.files.contains_key(path) {
            return self.read_file(path);
        }
        let mut remove_prompt_bundle = None;
        let mut write_built_in = false;
        if let Some(bundle_name) = prompt_bundle_from_path(path) {
            let bundle_paths = prompt_bundle_paths(&paths, bundle_name);
            let has_other_custom_text = bundle_paths.iter().any(|bundle_path| {
                bundle_path != path
                    && user_layer.files.get(bundle_path).is_some_and(|bytes| {
                        embedded_resource_text(bundle_path)
                            .map(|built_in| built_in.as_bytes() != bytes.as_slice())
                            .unwrap_or(true)
                    })
            });
            if has_other_custom_text {
                user_layer.files.remove(path);
                user_layer
                    .insert(
                        path.to_owned(),
                        embedded_resource_text(path)
                            .map_err(|error| error.to_string())?
                            .into_bytes(),
                    )
                    .map_err(|error| error.to_string())?;
                write_built_in = true;
            } else {
                for bundle_path in &bundle_paths {
                    user_layer.files.remove(bundle_path);
                }
                remove_prompt_bundle = Some(bundle_name.to_owned());
            }
        } else {
            user_layer.files.remove(path);
        }
        let mut validation_settings = settings;
        select_resource_for_validation(&mut validation_settings, path)?;
        let active = core.active_snapshot().map_err(|error| error.to_string())?;
        validate_candidate_bundle(self.compose_bundle(
            validation_settings,
            user_layer,
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        )?)?;
        if let Some(bundle_name) = remove_prompt_bundle {
            self.remove_prompt_bundle(&bundle_name)?;
        } else if write_built_in {
            self.persist_override(
                path,
                embedded_resource_text(path)
                    .map_err(|error| error.to_string())?
                    .as_bytes(),
            )?;
        } else {
            self.remove_override(path)?;
        }
        self.activate_current(core)?;
        self.read_file(path)
    }

    /// Activates the exact persisted user layer using the current vector-space identity.
    /// 使用当前向量空间身份激活准确的持久化用户层。
    fn activate_current(&self, core: &MemoryMeshCore) -> Result<(), String> {
        let active = core.active_snapshot().map_err(|error| error.to_string())?;
        let bundle = self.current_bundle(
            active.vector_space_fingerprint.clone(),
            active.embedding_dimension,
        )?;
        let snapshot = core
            .register_config(bundle)
            .map_err(|error| error.to_string())?;
        core.activate_config(snapshot.name.as_str())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Loads settings or returns Core defaults when no file exists.
    /// 加载设置；文件不存在时返回 Core 默认值。
    fn load_settings(&self) -> Result<PersistedResourceSettings, String> {
        let path = self.config_root.join("settings.json");
        let Some(bytes) = read_optional_regular_file(&self.config_root, &path, MAX_SETTINGS_BYTES)?
        else {
            return Ok(PersistedResourceSettings::default());
        };
        let settings: PersistedResourceSettings = serde_json::from_slice(&bytes)
            .map_err(|error| format!("解析原生 VMM 资源设置失败：{error}"))?;
        if settings.schema_version != RESOURCE_SETTINGS_SCHEMA_VERSION {
            return Err(format!(
                "不支持的原生 VMM 资源设置版本：{}",
                settings.schema_version
            ));
        }
        Ok(settings)
    }

    /// Loads only exact manifest-declared user files into one Core layer.
    /// 仅将清单明确声明的用户文件加载到 Core 层。
    fn load_user_layer(&self, paths: &[String]) -> Result<ResourceLayer, String> {
        let mut layer = ResourceLayer::default();
        for path in paths {
            let override_path = self.override_path(path);
            if let Some(bytes) = read_optional_regular_file(
                &self.config_root,
                &override_path,
                MAX_RESOURCE_FILE_BYTES as u64,
            )? {
                layer
                    .insert(path.clone(), bytes)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(layer)
    }

    /// Reads one optional UTF-8 override.
    /// 读取一份可选 UTF-8 覆盖。
    fn read_override(&self, path: &str) -> Result<Option<String>, String> {
        let Some(bytes) = read_optional_regular_file(
            &self.config_root,
            &self.override_path(path),
            MAX_RESOURCE_FILE_BYTES as u64,
        )?
        else {
            return Ok(None);
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|error| format!("原生 VMM 用户覆盖不是有效 UTF-8 `{path}`：{error}"))
    }

    /// Reports whether one exact physical override exists.
    /// 报告一份准确物理覆盖是否存在。
    fn override_exists(&self, path: &str) -> Result<bool, String> {
        read_optional_regular_file(&self.config_root, &self.override_path(path), 0)
            .map(|value| value.is_some())
    }

    /// Persists the complete settings document with same-directory atomic replacement.
    /// 使用同目录原子替换持久化完整设置文档。
    fn persist_settings(&self, settings: &PersistedResourceSettings) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(settings)
            .map_err(|error| format!("序列化原生 VMM 资源设置失败：{error}"))?;
        persist_file_atomically(
            &self.config_root,
            &self.config_root.join("settings.json"),
            &bytes,
        )
    }

    /// Persists one exact resource override with same-directory atomic replacement.
    /// 使用同目录原子替换持久化一份准确资源覆盖。
    fn persist_override(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        persist_file_atomically(&self.config_root, &self.override_path(path), bytes)
    }

    /// Materializes a complete prompt bundle on first edit and replaces one file thereafter.
    /// 首次编辑时物化完整提示词 Bundle，之后替换单个文件。
    fn persist_prompt_bundle(
        &self,
        changed_path: &str,
        bundle_paths: &[String],
        user_layer: &ResourceLayer,
    ) -> Result<(), String> {
        let existing_count = bundle_paths
            .iter()
            .filter(|path| self.override_path(path).exists())
            .count();
        if existing_count == 0 {
            let bundle_name = prompt_bundle_from_path(
                bundle_paths
                    .first()
                    .ok_or_else(|| String::from("提示词 Bundle 不包含场景文件"))?,
            )
            .ok_or_else(|| String::from("提示词 Bundle 路径无效"))?;
            let prompts_root = self.config_root.join("prompts");
            ensure_safe_directory(&self.config_root, &prompts_root)?;
            let staging = tempfile::Builder::new()
                .prefix(&format!(".{bundle_name}.stage-"))
                .tempdir_in(&prompts_root)
                .map_err(|error| format!("创建提示词 Bundle 暂存目录失败：{error}"))?;
            for path in bundle_paths {
                let file_name = Path::new(path)
                    .file_name()
                    .ok_or_else(|| format!("提示词资源缺少文件名：{path}"))?;
                let bytes = user_layer
                    .files
                    .get(path)
                    .ok_or_else(|| format!("提示词 Bundle 缺少用户层文件：{path}"))?;
                let mut file = fs::File::create(staging.path().join(file_name))
                    .map_err(|error| format!("创建提示词暂存文件失败 `{path}`：{error}"))?;
                file.write_all(bytes)
                    .map_err(|error| format!("写入提示词暂存文件失败 `{path}`：{error}"))?;
                file.sync_all()
                    .map_err(|error| format!("同步提示词暂存文件失败 `{path}`：{error}"))?;
            }
            let destination = prompts_root.join(bundle_name);
            let staging_path = staging.keep();
            fs::rename(&staging_path, &destination)
                .map_err(|error| format!("发布完整提示词 Bundle 失败：{error}"))?;
            return Ok(());
        }
        if existing_count != bundle_paths.len() {
            return Err(String::from(
                "用户提示词 Bundle 不完整；请修复或移除该 Bundle 目录后重新编辑。",
            ));
        }
        let bytes = user_layer
            .files
            .get(changed_path)
            .ok_or_else(|| format!("提示词 Bundle 缺少用户层文件：{changed_path}"))?;
        self.persist_override(changed_path, bytes)
    }

    /// Removes one exact non-prompt override while refusing links and non-files.
    /// 删除一份准确的非提示词覆盖，同时拒绝链接与非文件条目。
    fn remove_override(&self, path: &str) -> Result<(), String> {
        let target = self.override_path(path);
        match fs::symlink_metadata(&target) {
            Ok(metadata)
                if metadata.file_type().is_file() && !metadata_is_link_or_reparse(&metadata) =>
            {
                ensure_existing_safe_directory_chain(
                    &self.config_root,
                    target
                        .parent()
                        .ok_or_else(|| format!("原生 VMM 用户覆盖缺少父目录：{path}"))?,
                )?;
                ensure_canonical_path_within_root(&self.config_root, &target)?;
                fs::remove_file(&target)
                    .map_err(|error| format!("删除原生 VMM 用户覆盖失败 `{path}`：{error}"))
            }
            Ok(_) => Err(format!("拒绝删除非普通文件的原生 VMM 用户覆盖：{path}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("检查原生 VMM 用户覆盖失败 `{path}`：{error}")),
        }
    }

    /// Removes a complete prompt override bundle through an exact validated directory target.
    /// 通过准确且已校验的目录目标删除完整提示词覆盖 Bundle。
    fn remove_prompt_bundle(&self, bundle_name: &str) -> Result<(), String> {
        let prompts_root = self.config_root.join("prompts");
        let target = prompts_root.join(bundle_name);
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| format!("检查提示词 Bundle 目录失败：{error}"))?;
        if !metadata.file_type().is_dir() || metadata_is_link_or_reparse(&metadata) {
            return Err(String::from("拒绝删除非普通目录的提示词 Bundle。"));
        }
        ensure_existing_safe_directory_chain(&self.config_root, &prompts_root)?;
        let (canonical_root, canonical_target) =
            canonical_root_and_target(&self.config_root, &target)?;
        if canonical_target == canonical_root {
            return Err(String::from("拒绝删除配置根目录之外的提示词 Bundle。"));
        }
        let tombstone =
            prompts_root.join(format!(".{bundle_name}.removed-{}", uuid::Uuid::new_v4()));
        fs::rename(&target, &tombstone)
            .map_err(|error| format!("隔离待删除提示词 Bundle 失败：{error}"))?;
        fs::remove_dir_all(&tombstone)
            .map_err(|error| format!("删除已隔离提示词 Bundle 失败：{error}"))
    }

    /// Resolves one manifest path below the exact configuration root.
    /// 将一条清单路径解析到准确配置根目录下。
    fn override_path(&self, path: &str) -> PathBuf {
        path.split('/')
            .fold(self.config_root.clone(), |current, segment| {
                current.join(segment)
            })
    }
}

/// Describes one manifest path without guessing alternate layouts.
/// 描述一条清单路径，不猜测替代布局。
struct ResourceDescriptor {
    /// Browser-facing resource group.
    /// 面向浏览器的资源分组。
    group: String,
    /// Bundle, locale, or common selector.
    /// Bundle、区域或 common 选择项。
    selector: String,
    /// Exact final file name.
    /// 准确的末级文件名。
    file_name: String,
}

/// Returns the sorted authoritative embedded file paths.
/// 返回排序后的权威内置文件路径。
fn embedded_paths() -> Result<Vec<String>, String> {
    let mut paths = embedded_resource_paths().map_err(|error| error.to_string())?;
    paths.sort();
    Ok(paths)
}

/// Validates one exact path against the embedded authority catalog.
/// 根据内置权威目录校验一条准确路径。
fn ensure_catalog_path(path: &str) -> Result<(), String> {
    if embedded_paths()?.iter().any(|candidate| candidate == path) {
        return Ok(());
    }
    Err(format!("未知的原生 VMM 资源路径：{path}"))
}

/// Parses one exact manifest resource path into its stable UI dimensions.
/// 将一条准确清单资源路径解析为稳定界面维度。
fn describe_resource(path: &str) -> Result<ResourceDescriptor, String> {
    let segments = path.split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        ["prompts", bundle, file_name] => Ok(ResourceDescriptor {
            group: String::from("prompt"),
            selector: (*bundle).to_owned(),
            file_name: (*file_name).to_owned(),
        }),
        ["pii_rules", file_name] => Ok(ResourceDescriptor {
            group: String::from("pii"),
            selector: file_name.trim_end_matches(".json").to_owned(),
            file_name: (*file_name).to_owned(),
        }),
        ["noise_rules", file_name] => Ok(ResourceDescriptor {
            group: String::from("noise"),
            selector: file_name.trim_end_matches(".json").to_owned(),
            file_name: (*file_name).to_owned(),
        }),
        _ => Err(format!("不支持的原生 VMM 资源路径：{path}")),
    }
}

/// Returns whether one path is a high-risk prompt source.
/// 返回一条路径是否为高风险提示词源文件。
fn is_prompt_path(path: &str) -> bool {
    path.starts_with("prompts/")
}

/// Extracts the exact prompt bundle selector from one prompt path.
/// 从一条提示词路径提取准确 Bundle 选择项。
fn prompt_bundle_from_path(path: &str) -> Option<&str> {
    let mut segments = path.split('/');
    (segments.next() == Some("prompts"))
        .then(|| segments.next())
        .flatten()
}

/// Lists every manifest scene belonging to one exact prompt bundle.
/// 列出属于一个准确提示词 Bundle 的全部清单场景。
fn prompt_bundle_paths(paths: &[String], bundle_name: &str) -> Vec<String> {
    let prefix = format!("prompts/{bundle_name}/");
    paths
        .iter()
        .filter(|path| path.starts_with(&prefix))
        .cloned()
        .collect()
}

/// Validates closed selectors and Noise policy values against the embedded catalog.
/// 根据内置目录校验封闭选择项与 Noise 策略值。
fn validate_settings(settings: &PersistedResourceSettings, paths: &[String]) -> Result<(), String> {
    let prompt_prefix = format!("prompts/{}/", settings.prompt_bundle);
    if !paths.iter().any(|path| path.starts_with(&prompt_prefix)) {
        return Err(format!("未知的提示词 Bundle：{}", settings.prompt_bundle));
    }
    if !paths
        .iter()
        .any(|path| path == &format!("pii_rules/{}.json", settings.pii_language))
    {
        return Err(format!("未知的 PII 区域：{}", settings.pii_language));
    }
    if !paths
        .iter()
        .any(|path| path == &format!("noise_rules/{}.json", settings.noise_language))
    {
        return Err(format!("未知的噪音区域：{}", settings.noise_language));
    }
    if !settings.noise_semantic_threshold.is_finite()
        || settings.noise_semantic_threshold <= 0.0
        || settings.noise_semantic_threshold > 1.0
    {
        return Err(String::from(
            "噪音语义阈值必须是大于 0 且不超过 1 的有限数值。",
        ));
    }
    Ok(())
}

/// Selects the edited resource for validation even when it is not currently active.
/// 即使资源当前未激活，也选择被编辑资源执行校验。
fn select_resource_for_validation(
    settings: &mut PersistedResourceSettings,
    path: &str,
) -> Result<(), String> {
    let descriptor = describe_resource(path)?;
    match descriptor.group.as_str() {
        "prompt" => settings.prompt_bundle = descriptor.selector,
        "pii" if descriptor.selector != "common" => settings.pii_language = descriptor.selector,
        "noise" if descriptor.selector != "common" => {
            settings.noise_language = descriptor.selector;
        }
        "pii" | "noise" => {}
        _ => return Err(format!("无法校验原生 VMM 资源：{path}")),
    }
    Ok(())
}

/// Builds one Core configuration from exact selections and user-layer bytes.
/// 从准确选择与用户层字节构建一份 Core 配置。
fn config_bundle(
    settings: PersistedResourceSettings,
    user_layer: ResourceLayer,
    vector_space_fingerprint: String,
    embedding_dimension: u32,
) -> ConfigBundle {
    ConfigBundle {
        noise_policy: NoisePolicy {
            enabled: settings.noise_enabled,
            semantic_enabled: settings.noise_semantic_enabled,
            default_semantic_threshold: settings.noise_semantic_threshold,
        },
        name: NATIVE_VMM_CONFIG_REGISTRATION_NAME.to_owned(),
        prompt_bundle: settings.prompt_bundle,
        pii_language: settings.pii_language,
        noise_language: settings.noise_language,
        vector_space_fingerprint,
        embedding_dimension,
        user_layer,
        memory_policy: vulcan_memorymesh_core::MemoryPolicy {
            retrieval_policy: vulcan_memorymesh_core::RetrievalPolicy {
                lexical_contract: super::lexical::score_contract(),
                ..vulcan_memorymesh_core::RetrievalPolicy::v1()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Compiles one candidate bundle in an isolated Core before any durable or live-state mutation.
/// 在任何持久化或活动状态变更前，于隔离 Core 中编译一份候选 Bundle。
fn validate_candidate_bundle(bundle: ConfigBundle) -> Result<(), String> {
    let detached_core = MemoryMeshCore::new().map_err(|error| error.to_string())?;
    detached_core
        .register_config(bundle)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Reads an optional bounded regular file while refusing links and special entries.
/// 读取可选的有界普通文件，同时拒绝链接与特殊条目。
fn read_optional_regular_file(
    config_root: &Path,
    path: &Path,
    max_bytes: u64,
) -> Result<Option<Vec<u8>>, String> {
    let root_metadata = match fs::symlink_metadata(config_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "检查原生 VMM 配置根目录失败 `{}`：{error}",
                config_root.display()
            ));
        }
    };
    if !root_metadata.file_type().is_dir() || metadata_is_link_or_reparse(&root_metadata) {
        return Err(String::from("原生 VMM 配置根目录不是安全的普通目录。"));
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "检查原生 VMM 配置文件失败 `{}`：{error}",
                path.display()
            ));
        }
    };
    if !metadata.file_type().is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(format!("原生 VMM 配置路径不是普通文件：{}", path.display()));
    }
    ensure_existing_safe_directory_chain(
        config_root,
        path.parent()
            .ok_or_else(|| format!("原生 VMM 配置文件缺少父目录：{}", path.display()))?,
    )?;
    ensure_canonical_path_within_root(config_root, path)?;
    if max_bytes > 0 && metadata.len() > max_bytes {
        return Err(format!("原生 VMM 配置文件超过大小限制：{}", path.display()));
    }
    if max_bytes == 0 {
        return Ok(Some(Vec::new()));
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| format!("读取原生 VMM 配置文件失败 `{}`：{error}", path.display()))
}

/// Publishes one complete file using a synchronized same-directory temporary file.
/// 使用已同步的同目录临时文件发布一份完整文件。
fn persist_file_atomically(config_root: &Path, path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("原生 VMM 配置文件缺少父目录：{}", path.display()))?;
    ensure_safe_directory(config_root, parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if metadata.file_type().is_file() && !metadata_is_link_or_reparse(&metadata) =>
        {
            ensure_canonical_path_within_root(config_root, path)?;
        }
        Ok(_) => {
            return Err(format!(
                "拒绝覆盖非普通原生 VMM 配置文件：{}",
                path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "检查原生 VMM 配置目标失败 `{}`：{error}",
                path.display()
            ));
        }
    }
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("创建原生 VMM 配置暂存文件失败：{error}"))?;
    temporary
        .write_all(bytes)
        .map_err(|error| format!("写入原生 VMM 配置暂存文件失败：{error}"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|error| format!("同步原生 VMM 配置暂存文件失败：{error}"))?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| format!("发布原生 VMM 配置文件失败：{}", error.error))
}

/// Creates and validates one exact directory below the non-linked configuration root.
/// 创建并校验非链接配置根目录下的一个准确目录。
fn ensure_safe_directory(config_root: &Path, directory: &Path) -> Result<(), String> {
    fs::create_dir_all(config_root)
        .map_err(|error| format!("创建原生 VMM 配置根目录失败：{error}"))?;
    let root_metadata = fs::symlink_metadata(config_root)
        .map_err(|error| format!("检查原生 VMM 配置根目录失败：{error}"))?;
    if !root_metadata.file_type().is_dir() || metadata_is_link_or_reparse(&root_metadata) {
        return Err(String::from("原生 VMM 配置根目录不是安全的普通目录。"));
    }
    let relative = directory
        .strip_prefix(config_root)
        .map_err(|_| String::from("原生 VMM 配置子目录不在配置根目录内。"))?;
    let mut current = config_root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(String::from("原生 VMM 配置子目录包含非法路径片段。"));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata)
                if metadata.file_type().is_dir() && !metadata_is_link_or_reparse(&metadata) => {}
            Ok(_) => {
                return Err(format!(
                    "原生 VMM 配置子目录不是安全的普通目录：{}",
                    current.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| format!("创建原生 VMM 配置子目录失败：{error}"))?;
            }
            Err(error) => {
                return Err(format!(
                    "检查原生 VMM 配置子目录失败 `{}`：{error}",
                    current.display()
                ));
            }
        }
    }
    let canonical_root = fs::canonicalize(config_root)
        .map_err(|error| format!("解析原生 VMM 配置根目录失败：{error}"))?;
    let canonical_directory = fs::canonicalize(directory)
        .map_err(|error| format!("解析原生 VMM 配置子目录失败：{error}"))?;
    if !canonical_directory.starts_with(&canonical_root) {
        return Err(String::from("原生 VMM 配置子目录越过了配置根目录。"));
    }
    Ok(())
}

/// Verifies every existing directory from the configuration root to one exact descendant.
/// 验证从配置根目录到一个准确后代目录之间的每个既有目录。
fn ensure_existing_safe_directory_chain(
    config_root: &Path,
    directory: &Path,
) -> Result<(), String> {
    let relative = directory
        .strip_prefix(config_root)
        .map_err(|_| String::from("原生 VMM 配置目录不在配置根目录内。"))?;
    let mut current = config_root.to_path_buf();
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            let std::path::Component::Normal(segment) = component else {
                return Err(String::from("原生 VMM 配置目录包含非法路径片段。"));
            };
            current.push(segment);
        }
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            format!("检查原生 VMM 配置目录失败 `{}`：{error}", current.display())
        })?;
        if !metadata.file_type().is_dir() || metadata_is_link_or_reparse(&metadata) {
            return Err(format!(
                "原生 VMM 配置目录不是安全的普通目录：{}",
                current.display()
            ));
        }
    }
    Ok(())
}

/// Resolves one root and target pair and rejects any canonical path escape.
/// 解析一组根目录与目标路径，并拒绝任何规范路径越界。
fn canonical_root_and_target(
    config_root: &Path,
    target: &Path,
) -> Result<(PathBuf, PathBuf), String> {
    let canonical_root = fs::canonicalize(config_root)
        .map_err(|error| format!("解析原生 VMM 配置根目录失败：{error}"))?;
    let canonical_target = fs::canonicalize(target)
        .map_err(|error| format!("解析原生 VMM 配置目标失败 `{}`：{error}", target.display()))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(format!(
            "原生 VMM 配置目标越过了配置根目录：{}",
            target.display()
        ));
    }
    Ok((canonical_root, canonical_target))
}

/// Rejects one existing target unless its canonical path remains under the configuration root.
/// 拒绝规范路径未保持在配置根目录内的既有目标。
fn ensure_canonical_path_within_root(config_root: &Path, target: &Path) -> Result<(), String> {
    canonical_root_and_target(config_root, target).map(|_| ())
}

/// Reports whether metadata represents a symbolic link or Windows reparse point.
/// 报告元数据是否表示符号链接或 Windows 重解析点。
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the browser catalog exposes both prompt bundles and locale composition choices.
    /// 验证浏览器目录同时暴露提示词 Bundle 与区域组合选择。
    #[test]
    fn catalog_exposes_embedded_composition_and_override_root() {
        let temporary = tempfile::tempdir().expect("create VMM resource test root");
        let store = NativeVmmResourceStore::new(temporary.path());
        let catalog = store.catalog().expect("load embedded resource catalog");
        assert_eq!(catalog.prompt_bundles, vec!["default_cn", "default_en"]);
        assert!(catalog.pii_languages.contains(&String::from("zh-CN")));
        assert_eq!(catalog.noise_languages, vec!["zh-CN"]);
        assert!(catalog.override_root.ends_with("config"));
    }

    /// Verifies first prompt editing creates the whole five-scene user bundle.
    /// 验证首次提示词编辑会创建完整的五场景用户 Bundle。
    #[test]
    fn first_prompt_edit_materializes_complete_bundle() {
        let temporary = tempfile::tempdir().expect("create VMM resource test root");
        let store = NativeVmmResourceStore::new(temporary.path());
        let core = MemoryMeshCore::new().expect("create Core");
        store
            .write_file(
                &core,
                "prompts/default_cn/precheck_l1_main.md",
                String::from("custom precheck prompt"),
                true,
                true,
            )
            .expect("write complete prompt bundle");
        let paths = embedded_paths().expect("load paths");
        let bundle_paths = prompt_bundle_paths(&paths, "default_cn");
        assert_eq!(bundle_paths.len(), 5);
        assert!(
            bundle_paths
                .iter()
                .all(|path| store.override_path(path).is_file())
        );
    }

    /// Verifies every resource write is rejected unless both edit confirmations are explicit.
    /// 验证每份资源写入在两项编辑确认未同时明确时都会被拒绝。
    #[test]
    fn resource_edit_requires_both_confirmations() {
        let temporary = tempfile::tempdir().expect("create VMM resource test root");
        let store = NativeVmmResourceStore::new(temporary.path());
        let core = MemoryMeshCore::new().expect("create Core");
        let error = store
            .write_file(
                &core,
                "prompts/default_cn/precheck_l1_main.md",
                String::from("custom precheck prompt"),
                true,
                false,
            )
            .expect_err("missing second confirmation must fail");
        assert!(error.contains("再次确认"));

        let pii_error = store
            .write_file(
                &core,
                "pii_rules/zh-CN.json",
                String::from("{}"),
                false,
                true,
            )
            .expect_err("PII edit without unlock confirmation must fail");
        assert!(pii_error.contains("确认文件用途"));
    }

    /// Verifies oversized override files are rejected before their contents are read into memory.
    /// 验证超限覆盖文件会在内容读入内存前被拒绝。
    #[test]
    fn oversized_override_is_rejected_before_reading() {
        let temporary = tempfile::tempdir().expect("create VMM resource test root");
        let store = NativeVmmResourceStore::new(temporary.path());
        let path = "noise_rules/zh-CN.json";
        let override_path = store.override_path(path);
        fs::create_dir_all(override_path.parent().expect("override parent"))
            .expect("create override parent");
        let file = fs::File::create(&override_path).expect("create oversized override");
        file.set_len(MAX_RESOURCE_FILE_BYTES as u64 + 1)
            .expect("extend oversized override");

        let error = store
            .read_file(path)
            .expect_err("oversized override must fail before reading");
        assert!(error.contains("超过大小限制"), "unexpected error: {error}");
    }

    /// Verifies an existing non-directory configuration root is rejected during reads.
    /// 验证读取阶段会拒绝已存在但并非目录的配置根路径。
    #[test]
    fn non_directory_config_root_is_rejected() {
        let temporary = tempfile::tempdir().expect("create VMM resource test root");
        let store = NativeVmmResourceStore::new(temporary.path());
        fs::write(&store.config_root, b"not-a-directory").expect("write invalid config root");

        let error = store
            .catalog()
            .expect_err("non-directory config root must fail");
        assert!(error.contains("不是安全的普通目录"));
    }
}
