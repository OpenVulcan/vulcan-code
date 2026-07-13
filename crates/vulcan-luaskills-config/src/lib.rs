//! Vulcan-owned LuaSkills runtime settings stored under the output configuration directory.
//! 存储在 output 配置目录下、由 Vulcan 持有的 LuaSkills 运行时设置。

mod atomic_file;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// File name of the Vulcan-owned LuaSkills runtime settings document.
/// Vulcan 持有的 LuaSkills 运行时设置文档文件名。
pub const LUASKILLS_RUNTIME_SETTINGS_FILE_NAME: &str = "luaskills-runtime-setting.json";

/// Current schema version of the LuaSkills runtime settings document.
/// LuaSkills 运行时设置文档的当前 schema 版本。
pub const LUASKILLS_RUNTIME_SETTINGS_SCHEMA_VERSION: u32 = 1;

/// Pool sizing and idle-retirement settings for one Lua VM pool.
/// 单个 Lua VM 池的容量与空闲回收设置。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaVmPoolSettings {
    /// Minimum number of warm Lua VMs retained by the pool.
    /// 池内保留的最小预热 Lua VM 数量。
    pub min_size: usize,
    /// Maximum number of Lua VMs allowed in the pool.
    /// 池内允许存在的最大 Lua VM 数量。
    pub max_size: usize,
    /// Idle seconds before one excess Lua VM may be retired.
    /// 多余 Lua VM 可被回收前的空闲秒数。
    pub idle_ttl_secs: u64,
}

impl LuaVmPoolSettings {
    /// Validate one pool without silently rewriting user intent.
    /// 校验单个池配置，且不静默改写用户意图。
    ///
    /// Returns unit for a usable pool or a field-qualified error.
    /// 池配置可用时返回空值，否则返回带字段名的错误。
    pub fn validate(&self, field_name: &str) -> Result<(), String> {
        if self.min_size == 0 {
            return Err(format!("{field_name}.minSize must be greater than zero"));
        }
        if self.max_size < self.min_size {
            return Err(format!(
                "{field_name}.maxSize must be greater than or equal to minSize"
            ));
        }
        if self.idle_ttl_secs == 0 {
            return Err(format!(
                "{field_name}.idleTtlSecs must be greater than zero"
            ));
        }
        Ok(())
    }
}

/// Shared transient tool-cache capacity and expiration settings.
/// 共享临时工具缓存的容量与过期设置。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsToolCacheSettings {
    /// Maximum process-wide cached entry count.
    /// 进程级缓存最大条目数。
    pub max_entries: usize,
    /// Default cache entry lifetime in seconds.
    /// 缓存条目的默认存活秒数。
    pub default_ttl_secs: u64,
    /// Maximum accepted cache entry lifetime in seconds.
    /// 缓存条目允许的最大存活秒数。
    pub max_ttl_secs: u64,
}

impl LuaSkillsToolCacheSettings {
    /// Validate cache capacity and TTL ordering.
    /// 校验缓存容量与 TTL 顺序。
    ///
    /// Returns unit for a usable policy or a stable validation error.
    /// 策略可用时返回空值，否则返回稳定校验错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.max_entries == 0 {
            return Err("toolCache.maxEntries must be greater than zero".to_string());
        }
        if self.default_ttl_secs == 0 {
            return Err("toolCache.defaultTtlSecs must be greater than zero".to_string());
        }
        if self.max_ttl_secs < self.default_ttl_secs {
            return Err(
                "toolCache.maxTtlSecs must be greater than or equal to defaultTtlSecs".to_string(),
            );
        }
        Ok(())
    }
}

/// Network download and source-routing settings consumed during engine creation.
/// 引擎创建期间消费的网络下载与来源路由设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsNetworkSettings {
    /// Whether LuaSkills may download skills and dependencies.
    /// LuaSkills 是否允许下载技能与依赖。
    pub allow_downloads: bool,
    /// Optional GitHub website base URL override.
    /// 可选 GitHub 网站基址覆盖。
    pub github_base_url: Option<String>,
    /// Optional GitHub API base URL override.
    /// 可选 GitHub API 基址覆盖。
    pub github_api_base_url: Option<String>,
    /// Optional official LuaSkills Hub base URL override.
    /// 可选 LuaSkills 官方 Hub 基址覆盖。
    pub official_skill_hub_base_url: Option<String>,
    /// Whether trusted system operations may install private URL manifests.
    /// 可信系统操作是否允许安装私有 URL 清单。
    pub enable_private_url_skill_install: bool,
    /// Allowed URL prefixes for private skill manifests.
    /// 私有技能清单允许的 URL 前缀。
    pub private_skill_source_allowlist: Vec<String>,
}

impl LuaSkillsNetworkSettings {
    /// Normalize optional endpoints and deduplicate the private source allowlist.
    /// 规范化可选端点并去重私有来源允许列表。
    ///
    /// Returns a normalized copy without changing the receiver.
    /// 返回规范化副本且不修改接收者。
    pub fn normalized(&self) -> Self {
        Self {
            allow_downloads: self.allow_downloads,
            github_base_url: normalized_optional_text(self.github_base_url.as_deref()),
            github_api_base_url: normalized_optional_text(self.github_api_base_url.as_deref()),
            official_skill_hub_base_url: normalized_optional_text(
                self.official_skill_hub_base_url.as_deref(),
            ),
            enable_private_url_skill_install: self.enable_private_url_skill_install,
            private_skill_source_allowlist: self
                .private_skill_source_allowlist
                .iter()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(String::from)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        }
    }
}

/// Host capability and text-encoding settings captured by one LuaSkills engine.
/// 单个 LuaSkills 引擎捕获的宿主能力与文本编码设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsCapabilitySettings {
    /// Optional default encoding label for managed IO and process APIs.
    /// 托管 IO 与进程 API 使用的可选默认编码标签。
    pub default_text_encoding: Option<String>,
    /// Whether Lua may invoke the host skill-management bridge.
    /// Lua 是否可调用宿主技能管理桥接。
    pub enable_skill_management_bridge: bool,
    /// Whether runtime sessions replace Lua's native global IO table with managed IO.
    /// 运行时会话是否使用托管 IO 替换 Lua 原生全局 IO 表。
    pub enable_managed_io_compat: bool,
}

impl LuaSkillsCapabilitySettings {
    /// Normalize and validate the optional runtime text encoding.
    /// 规范化并校验可选运行时文本编码。
    ///
    /// Returns a normalized copy or a stable unsupported-encoding error.
    /// 返回规范化副本，或返回稳定的不支持编码错误。
    pub fn normalized(&self) -> Result<Self, String> {
        let default_text_encoding = normalized_optional_text(self.default_text_encoding.as_deref())
            .map(|value| value.to_ascii_lowercase());
        if let Some(encoding) = default_text_encoding.as_deref()
            && !matches!(
                encoding,
                "utf-8" | "system" | "oem" | "gbk" | "gb18030" | "latin1" | "base64"
            )
        {
            return Err(format!(
                "capabilities.defaultTextEncoding uses unsupported value `{encoding}`"
            ));
        }
        Ok(Self {
            default_text_encoding,
            enable_skill_management_bridge: self.enable_skill_management_bridge,
            enable_managed_io_compat: self.enable_managed_io_compat,
        })
    }
}

/// Resource limits for managed Python and Node workers and persistent sessions.
/// 受管 Python、Node Worker 与持久会话使用的资源限制。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsManagedRuntimeSettings {
    /// Maximum live workers retained for one exact environment and package owner.
    /// 单个精确环境与包所有者允许保留的最大活动 Worker 数量。
    pub worker_pool_max_size_per_environment: usize,
    /// Idle seconds after which an unused managed worker may be retired.
    /// 未使用的受管 Worker 可被回收前的空闲秒数。
    pub worker_idle_ttl_secs: u64,
    /// Maximum launching or active persistent sessions owned by one engine.
    /// 单个引擎允许持有的启动中或活动持久会话最大数量。
    pub persistent_session_limit_per_engine: usize,
    /// Default retained byte limit for each persistent-session output stream.
    /// 每个持久会话输出流默认保留的字节上限。
    pub persistent_session_default_buffer_limit_bytes_per_stream: usize,
    /// Default invoke timeout in milliseconds; absent means unlimited.
    /// 默认调用超时毫秒数；缺失表示无限制。
    pub invoke_default_timeout_ms: Option<u64>,
}

impl Default for LuaSkillsManagedRuntimeSettings {
    /// Return the stable LuaSkills 0.5.2 managed-runtime defaults.
    /// 返回 LuaSkills 0.5.2 的稳定受管运行时默认值。
    fn default() -> Self {
        Self {
            worker_pool_max_size_per_environment: 4,
            worker_idle_ttl_secs: 60,
            persistent_session_limit_per_engine: 256,
            persistent_session_default_buffer_limit_bytes_per_stream: 1024 * 1024,
            invoke_default_timeout_ms: None,
        }
    }
}

impl LuaSkillsManagedRuntimeSettings {
    /// Validate every configured managed-runtime capacity and timeout boundary.
    /// 校验每个已配置的受管运行时容量与超时边界。
    ///
    /// Returns unit for a usable policy or a field-qualified validation error.
    /// 策略可用时返回空值，否则返回带字段名的校验错误。
    pub fn validate(&self) -> Result<(), String> {
        if self.worker_pool_max_size_per_environment == 0 {
            return Err(
                "managedRuntime.workerPoolMaxSizePerEnvironment must be greater than zero"
                    .to_string(),
            );
        }
        if self.worker_idle_ttl_secs == 0 {
            return Err("managedRuntime.workerIdleTtlSecs must be greater than zero".to_string());
        }
        if self.persistent_session_limit_per_engine == 0 {
            return Err(
                "managedRuntime.persistentSessionLimitPerEngine must be greater than zero"
                    .to_string(),
            );
        }
        if self.persistent_session_default_buffer_limit_bytes_per_stream == 0 {
            return Err(
                "managedRuntime.persistentSessionDefaultBufferLimitBytesPerStream must be greater than zero"
                    .to_string(),
            );
        }
        if self.invoke_default_timeout_ms == Some(0) {
            return Err(
                "managedRuntime.invokeDefaultTimeoutMs must be greater than zero when configured"
                    .to_string(),
            );
        }
        Ok(())
    }
}

/// Complete Vulcan-owned LuaSkills engine startup settings.
/// 完整的 Vulcan 自有 LuaSkills 引擎启动设置。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsRuntimeSettings {
    /// Settings document schema version.
    /// 设置文档 schema 版本。
    pub schema_version: u32,
    /// Reusable VM pool used by ordinary Skill calls.
    /// 普通 Skill 调用使用的可复用 VM 池。
    pub vm_pool: LuaVmPoolSettings,
    /// Dedicated isolated VM pool used by `vulcan.runtime.lua.exec`.
    /// `vulcan.runtime.lua.exec` 使用的独立隔离 VM 池。
    pub run_lua_pool: LuaVmPoolSettings,
    /// Shared transient tool-cache policy.
    /// 共享临时工具缓存策略。
    pub tool_cache: LuaSkillsToolCacheSettings,
    /// Network and source-routing policy.
    /// 网络与来源路由策略。
    pub network: LuaSkillsNetworkSettings,
    /// Host capabilities and default text encoding.
    /// 宿主能力与默认文本编码。
    pub capabilities: LuaSkillsCapabilitySettings,
    /// Managed Python and Node worker and persistent-session policy.
    /// 受管 Python、Node Worker 与持久会话策略。
    #[serde(default)]
    pub managed_runtime: LuaSkillsManagedRuntimeSettings,
}

impl Default for LuaSkillsRuntimeSettings {
    /// Return the Vulcan Code defaults matching the currently embedded LuaSkills integration.
    /// 返回与当前内嵌 LuaSkills 集成一致的 Vulcan Code 默认值。
    fn default() -> Self {
        Self {
            schema_version: LUASKILLS_RUNTIME_SETTINGS_SCHEMA_VERSION,
            vm_pool: LuaVmPoolSettings {
                min_size: 1,
                max_size: 2,
                idle_ttl_secs: 30,
            },
            run_lua_pool: LuaVmPoolSettings {
                min_size: 1,
                max_size: 4,
                idle_ttl_secs: 60,
            },
            tool_cache: LuaSkillsToolCacheSettings {
                max_entries: 1_000,
                default_ttl_secs: 30 * 60,
                max_ttl_secs: 30 * 60,
            },
            network: LuaSkillsNetworkSettings {
                allow_downloads: true,
                github_base_url: None,
                github_api_base_url: None,
                official_skill_hub_base_url: None,
                enable_private_url_skill_install: false,
                private_skill_source_allowlist: Vec::new(),
            },
            capabilities: LuaSkillsCapabilitySettings {
                default_text_encoding: None,
                enable_skill_management_bridge: false,
                enable_managed_io_compat: true,
            },
            managed_runtime: LuaSkillsManagedRuntimeSettings::default(),
        }
    }
}

impl LuaSkillsRuntimeSettings {
    /// Normalize and validate one complete settings document.
    /// 规范化并校验一份完整设置文档。
    ///
    /// Returns a normalized copy or a field-qualified validation error.
    /// 返回规范化副本，或返回带字段名的校验错误。
    pub fn normalized(&self) -> Result<Self, String> {
        if self.schema_version != LUASKILLS_RUNTIME_SETTINGS_SCHEMA_VERSION {
            return Err(format!(
                "unsupported LuaSkills runtime settings schema: {}",
                self.schema_version
            ));
        }
        self.vm_pool.validate("vmPool")?;
        self.run_lua_pool.validate("runLuaPool")?;
        self.tool_cache.validate()?;
        self.managed_runtime.validate()?;
        Ok(Self {
            schema_version: self.schema_version,
            vm_pool: self.vm_pool,
            run_lua_pool: self.run_lua_pool,
            tool_cache: self.tool_cache,
            network: self.network.normalized(),
            capabilities: self.capabilities.normalized()?,
            managed_runtime: self.managed_runtime,
        })
    }
}

/// Return the LuaSkills runtime settings file path below one output configuration directory.
/// 返回单个 output 配置目录下的 LuaSkills 运行时设置文件路径。
///
/// `config_directory` is the exact Vulcan `output/config` directory selected by the host.
/// `config_directory` 是宿主选定的精确 Vulcan `output/config` 目录。
pub fn runtime_settings_file_path(config_directory: &Path) -> PathBuf {
    config_directory.join(LUASKILLS_RUNTIME_SETTINGS_FILE_NAME)
}

/// Load and validate Vulcan-owned settings, falling back only when the file is absent.
/// 加载并校验 Vulcan 自有设置，仅在文件不存在时回退默认值。
///
/// Returns the normalized saved settings or the built-in defaults.
/// 返回规范化后的已保存设置或内建默认值。
pub fn load_runtime_settings(config_directory: &Path) -> Result<LuaSkillsRuntimeSettings, String> {
    let path = runtime_settings_file_path(config_directory);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LuaSkillsRuntimeSettings::default());
        }
        Err(error) => {
            return Err(format!(
                "failed to read LuaSkills runtime settings `{}`: {error}",
                path.display()
            ));
        }
    };
    let settings = serde_json::from_str::<LuaSkillsRuntimeSettings>(&text).map_err(|error| {
        format!(
            "failed to parse LuaSkills runtime settings `{}`: {error}",
            path.display()
        )
    })?;
    settings.normalized()
}

/// Validate and atomically save one Vulcan-owned settings document.
/// 校验并原子保存一份 Vulcan 自有设置文档。
///
/// Returns the normalized settings actually written to disk.
/// 返回实际写入磁盘的规范化设置。
pub fn save_runtime_settings(
    config_directory: &Path,
    settings: &LuaSkillsRuntimeSettings,
) -> Result<LuaSkillsRuntimeSettings, String> {
    let normalized = settings.normalized()?;
    let mut contents = serde_json::to_vec_pretty(&normalized)
        .map_err(|error| format!("failed to serialize LuaSkills runtime settings: {error}"))?;
    contents.push(b'\n');
    atomic_file::write_file_atomically(&runtime_settings_file_path(config_directory), &contents)?;
    Ok(normalized)
}

/// Trim one optional free-form text value and discard blank strings.
/// 修剪一个可选自由文本值并丢弃空字符串。
fn normalized_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the repository template synchronized into output matches the compiled defaults.
    /// 验证同步到 output 的仓库模板与编译默认值一致。
    #[test]
    fn repository_template_matches_compiled_defaults() {
        let template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("config")
            .join(LUASKILLS_RUNTIME_SETTINGS_FILE_NAME);
        let template_text = fs::read_to_string(&template_path).expect("read settings template");
        let template = serde_json::from_str::<LuaSkillsRuntimeSettings>(&template_text)
            .expect("parse settings template")
            .normalized()
            .expect("validate settings template");
        assert_eq!(template, LuaSkillsRuntimeSettings::default());
    }

    /// Verify missing settings use the exact embedded defaults.
    /// 验证设置缺失时使用精确的内嵌默认值。
    #[test]
    fn missing_settings_use_defaults() {
        let root = tempfile::tempdir().expect("create temporary runtime root");
        assert_eq!(
            load_runtime_settings(root.path()).expect("load default settings"),
            LuaSkillsRuntimeSettings::default()
        );
    }

    /// Verify saved settings round-trip through the Vulcan-owned path.
    /// 验证已保存设置可通过 Vulcan 自有路径往返读取。
    #[test]
    fn settings_round_trip_through_runtime_root() {
        let root = tempfile::tempdir().expect("create temporary runtime root");
        let mut settings = LuaSkillsRuntimeSettings::default();
        settings.vm_pool.max_size = 8;
        settings.network.github_base_url = Some(" https://github.example ".to_string());
        let saved = save_runtime_settings(root.path(), &settings).expect("save settings");
        assert_eq!(saved.vm_pool.max_size, 8);
        assert_eq!(
            saved.network.github_base_url.as_deref(),
            Some("https://github.example")
        );
        // replaced verifies Windows and POSIX both publish over an existing settings file.
        // replaced 验证 Windows 与 POSIX 都能覆盖发布已有设置文件。
        let mut replaced = saved.clone();
        replaced.vm_pool.max_size = 10;
        let replaced = save_runtime_settings(root.path(), &replaced).expect("replace settings");
        assert_eq!(
            load_runtime_settings(root.path()).expect("reload settings"),
            replaced
        );
    }

    /// Verify invalid pool ordering is rejected instead of being silently normalized.
    /// 验证非法池顺序会被拒绝而非静默归一化。
    #[test]
    fn invalid_pool_order_is_rejected() {
        let mut settings = LuaSkillsRuntimeSettings::default();
        settings.vm_pool.min_size = 4;
        settings.vm_pool.max_size = 2;
        let error = settings
            .normalized()
            .expect_err("reject invalid pool ordering");
        assert!(error.contains("vmPool.maxSize"));
    }

    /// Verify a zero managed-runtime capacity is rejected before engine startup.
    /// 验证受管运行时容量为零时会在引擎启动前被拒绝。
    #[test]
    fn invalid_managed_runtime_capacity_is_rejected() {
        let mut settings = LuaSkillsRuntimeSettings::default();
        settings
            .managed_runtime
            .worker_pool_max_size_per_environment = 0;
        let error = settings
            .normalized()
            .expect_err("reject zero managed-runtime worker capacity");
        assert!(error.contains("managedRuntime.workerPoolMaxSizePerEnvironment"));
    }

    /// Verify schema-one files created before 0.5.2 receive stable managed-runtime defaults.
    /// 验证 0.5.2 之前创建的 schema-one 文件会获得稳定受管运行时默认值。
    #[test]
    fn legacy_settings_without_managed_runtime_use_stable_defaults() {
        let mut value = serde_json::to_value(LuaSkillsRuntimeSettings::default())
            .expect("serialize default settings");
        value
            .as_object_mut()
            .expect("settings must serialize as an object")
            .remove("managedRuntime");
        let parsed = serde_json::from_value::<LuaSkillsRuntimeSettings>(value)
            .expect("parse legacy settings without managed runtime");
        assert_eq!(
            parsed.managed_runtime,
            LuaSkillsManagedRuntimeSettings::default()
        );
    }
}
