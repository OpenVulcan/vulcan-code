use super::*;

/// RTK routes enabled by default in `selected` mode.
/// `selected` 模式下默认启用的 RTK 路由。
pub const DEFAULT_RTK_ENABLED_ROUTES: &[&str] = &[
    "cargo", "find", "git", "go", "grep", "jest", "ls", "mypy", "npm", "npx", "pnpm", "pytest",
    "read", "ruff", "tree", "tsc", "vitest", "wc",
];

/// Further RTK routes offered for selection; they are off by default in `selected` mode.
/// The list follows the upstream RTK release locked in `scripts/build-inputs.lock.json` `tools.rtk`.
/// 可供选择的其余 RTK 路由；在 `selected` 模式下默认关闭。
/// 该列表跟随 `scripts/build-inputs.lock.json` `tools.rtk` 锁定的上游 RTK 发行版。
pub const RTK_EXTENDED_ROUTES: &[&str] = &[
    "ansible-playbook",
    "aws",
    "brew",
    "bundle",
    "composer",
    "curl",
    "df",
    "diff",
    "docker",
    "dotnet",
    "du",
    "fail2ban-client",
    "gcloud",
    "gh",
    "glab",
    "golangci-lint",
    "gradlew",
    "gt",
    "hadolint",
    "helm",
    "iptables",
    "kubectl",
    "lint",
    "liquibase",
    "make",
    "markdownlint",
    "mix",
    "mvn",
    "next",
    "oc",
    "ping",
    "pio",
    "pip",
    "playwright",
    "poetry",
    "pre-commit",
    "prettier",
    "prisma",
    "ps",
    "psql",
    "pulumi",
    "quarto",
    "rake",
    "rg",
    "rspec",
    "rsync",
    "rubocop",
    "shellcheck",
    "shopify",
    "sops",
    "swift",
    "systemctl",
    "terraform",
    "tofu",
    "trunk",
    "uv",
    "wget",
    "yamllint",
];

/// Largest number of entries one RTK route list may hold.
/// 单个 RTK 路由列表允许的最大条目数。
pub const MAX_RTK_ROUTE_ENTRIES: usize = 256;

/// Longest accepted RTK route name, in bytes.
/// 允许的最长 RTK 路由名称（按字节计）。
pub const MAX_RTK_ROUTE_NAME_BYTES: usize = 64;

/// Selects which RTK routes may replace a model-issued shell command.
/// 选择哪些 RTK 路由可以替换模型发出的 shell 命令。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RtkRouteMode {
    /// No rewrite is applied.
    /// 不应用任何改写。
    Disabled,
    /// Only routes listed in `enabledRoutes` are applied.
    /// 只应用 `enabledRoutes` 中列出的路由。
    #[default]
    Selected,
    /// Every route except those listed in `disabledRoutes` is applied.
    /// 应用除 `disabledRoutes` 所列之外的全部路由。
    All,
}

/// Returns the default `selected`-mode route list.
/// 返回默认的 `selected` 模式路由列表。
fn default_rtk_enabled_routes() -> Vec<String> {
    DEFAULT_RTK_ENABLED_ROUTES
        .iter()
        .map(|route| (*route).to_string())
        .collect()
}

/// Returns `true` for serde fields whose default is enabled.
/// 为默认开启的 serde 字段返回 `true`。
fn default_true() -> bool {
    true
}

/// Persisted host-native RTK command-router settings.
/// 持久化的宿主原生 RTK 命令路由设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedRtkConfiguration {
    /// Whether model-issued shell commands are routed through RTK.
    /// 是否将模型发出的 shell 命令经 RTK 路由。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Which routes may apply.
    /// 哪些路由可以生效。
    #[serde(default)]
    pub route_mode: RtkRouteMode,
    /// Allowlist used in `selected` mode.
    /// `selected` 模式使用的允许列表。
    #[serde(default = "default_rtk_enabled_routes")]
    pub enabled_routes: Vec<String>,
    /// Denylist used in `all` mode.
    /// `all` 模式使用的拒绝列表。
    #[serde(default)]
    pub disabled_routes: Vec<String>,
    /// Whether RTK telemetry is disabled for host-launched processes.
    /// 是否为宿主启动的进程关闭 RTK 遥测。
    #[serde(default = "default_true")]
    pub telemetry_disabled: bool,
    /// Whether Git commit-summary context is compressed through RTK.
    /// 是否经 RTK 压缩 Git 提交总结上下文。
    #[serde(default = "default_true")]
    pub git_commit_context_enabled: bool,
}

impl Default for PersistedRtkConfiguration {
    /// Builds the default RTK configuration.
    /// 构建默认 RTK 配置。
    fn default() -> Self {
        Self {
            enabled: true,
            route_mode: RtkRouteMode::default(),
            enabled_routes: default_rtk_enabled_routes(),
            disabled_routes: Vec::new(),
            telemetry_disabled: true,
            git_commit_context_enabled: true,
        }
    }
}

impl PersistedRtkConfiguration {
    /// Returns whether every RTK setting is still at its default value.
    /// 返回全部 RTK 设置是否仍处于默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Returns whether shell-command rewriting can apply at all.
    /// 返回 shell 命令改写是否可能生效。
    pub fn routing_enabled(&self) -> bool {
        self.enabled && self.route_mode != RtkRouteMode::Disabled
    }

    /// Returns whether every route of one rewrite is allowed; an empty route set is never allowed.
    /// 返回一次改写的全部路由是否均被允许；空路由集合永不允许。
    ///
    /// # Parameters
    /// 参数
    /// `routes` are the RTK subcommands referenced by the rewritten command.
    /// `routes` 是改写后命令引用的 RTK 子命令。
    pub fn allows_routes(&self, routes: &[String]) -> bool {
        if routes.is_empty() {
            return false;
        }
        let listed = |list: &[String], route: &str| {
            list.iter()
                .any(|entry| entry.trim().eq_ignore_ascii_case(route))
        };
        match self.route_mode {
            RtkRouteMode::Disabled => false,
            RtkRouteMode::Selected => routes
                .iter()
                .all(|route| listed(&self.enabled_routes, route)),
            RtkRouteMode::All => !routes
                .iter()
                .any(|route| listed(&self.disabled_routes, route)),
        }
    }

    /// Validates and normalizes both route lists before the settings are saved.
    /// 保存设置前校验并规范化两个路由列表。
    ///
    /// # Returns
    /// 返回值
    /// The settings with each list lowercased, de-duplicated, and sorted, or an English reason
    /// naming the first invalid entry.
    /// 每个列表转为小写、去重并排序后的设置；遇到无效条目时返回指明首个无效条目的英文原因。
    pub fn normalized(mut self) -> Result<Self, String> {
        self.enabled_routes = normalize_rtk_routes(self.enabled_routes, "enabledRoutes")?;
        self.disabled_routes = normalize_rtk_routes(self.disabled_routes, "disabledRoutes")?;
        Ok(self)
    }
}

/// Validates one route list: ASCII letters, digits, `_` and `-`, bounded in length and count.
/// 校验一个路由列表：仅 ASCII 字母、数字、`_` 与 `-`，长度与数量均有上限。
fn normalize_rtk_routes(routes: Vec<String>, field: &str) -> Result<Vec<String>, String> {
    if routes.len() > MAX_RTK_ROUTE_ENTRIES {
        return Err(format!(
            "RTK `{field}` holds {} entries; at most {MAX_RTK_ROUTE_ENTRIES} are allowed.",
            routes.len()
        ));
    }
    let mut normalized = Vec::with_capacity(routes.len());
    for route in routes {
        let route = route.trim().to_ascii_lowercase();
        let valid = !route.is_empty()
            && route.len() <= MAX_RTK_ROUTE_NAME_BYTES
            && route
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
        if !valid {
            return Err(format!(
                "RTK `{field}` contains the invalid route `{route}`."
            ));
        }
        normalized.push(route);
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Missing fields take their defaults while explicit fields override only themselves.
    /// 缺失字段取默认值，显式字段只覆盖自身。
    #[test]
    fn rtk_configuration_defaults_round_trip() {
        let parsed: PersistedRtkConfiguration =
            serde_json::from_str("{}").expect("empty RTK configuration");
        assert!(parsed.is_default());
        let explicit: PersistedRtkConfiguration =
            serde_json::from_str(r#"{"routeMode":"all","telemetryDisabled":false}"#)
                .expect("explicit RTK configuration");
        assert_eq!(explicit.route_mode, RtkRouteMode::All);
        assert!(!explicit.telemetry_disabled);
        assert!(explicit.enabled);
    }

    /// Each list applies only in its own mode and route names compare case-insensitively.
    /// 每个列表只在自身模式生效，路由名称不区分大小写比较。
    #[test]
    fn rtk_route_policy_follows_mode() {
        let routes = |names: &[&str]| {
            names
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>()
        };
        let mut config = PersistedRtkConfiguration {
            enabled_routes: routes(&["git"]),
            disabled_routes: routes(&["docker"]),
            ..PersistedRtkConfiguration::default()
        };
        assert!(config.allows_routes(&routes(&["GIT"])));
        assert!(!config.allows_routes(&routes(&["git", "cargo"])));
        assert!(!config.allows_routes(&[]));
        config.route_mode = RtkRouteMode::All;
        assert!(config.allows_routes(&routes(&["git", "cargo"])));
        assert!(!config.allows_routes(&routes(&["git", "docker"])));
        config.route_mode = RtkRouteMode::Disabled;
        assert!(!config.allows_routes(&routes(&["git"])));
        assert!(!config.routing_enabled());
    }

    /// Saved route lists are lowercased, de-duplicated and sorted; malformed names are rejected.
    /// 保存的路由列表转小写、去重并排序；格式错误的名称被拒绝。
    #[test]
    fn rtk_route_lists_are_normalized_before_saving() {
        let config = PersistedRtkConfiguration {
            enabled_routes: vec![
                String::from(" Git "),
                String::from("cargo"),
                String::from("git"),
            ],
            ..PersistedRtkConfiguration::default()
        }
        .normalized()
        .expect("valid routes");
        assert_eq!(
            config.enabled_routes,
            vec![String::from("cargo"), String::from("git")]
        );
        for invalid in ["", "git status", "rtk;ls", "\u{8def}"] {
            let config = PersistedRtkConfiguration {
                disabled_routes: vec![String::from(invalid)],
                ..PersistedRtkConfiguration::default()
            };
            assert!(config.normalized().is_err(), "`{invalid}` must be rejected");
        }
        let oversized = PersistedRtkConfiguration {
            enabled_routes: (0..=MAX_RTK_ROUTE_ENTRIES)
                .map(|index| format!("r{index}"))
                .collect(),
            ..PersistedRtkConfiguration::default()
        };
        assert!(oversized.normalized().is_err());
    }

    /// The curated and extended route catalogs never overlap.
    /// 精选与扩展路由目录互不重叠。
    #[test]
    fn rtk_route_catalogs_are_disjoint() {
        assert!(
            RTK_EXTENDED_ROUTES
                .iter()
                .all(|route| !DEFAULT_RTK_ENABLED_ROUTES.contains(route))
        );
    }
}
