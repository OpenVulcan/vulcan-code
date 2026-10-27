//! GitHub mirror catalog, custom mirror validation and mirror speed probes for LuaSkills.
//! LuaSkills 的 GitHub 镜像目录、自定义镜像校验与镜像测速。
//!
//! The built-in catalog, URL normalization and probe all come from the LuaSkills crate; this
//! module only reshapes them into the camelCase service contract and validates user input.
//! 内置目录、地址规范化与测速都来自 LuaSkills crate；本模块只把它们转换为 camelCase 服务契约并校验用户输入。

use luaskills::{
    LuaRuntimeGithubMirror, LuaRuntimeGithubMirrorKind, LuaRuntimeGithubMirrorProbeRequest,
    LuaRuntimeGithubMirrorProbeTarget, LuaRuntimeGithubMirrorRegion, builtin_github_mirrors,
    normalize_github_mirror_base_url, probe_github_mirrors,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use vulcan_luaskills_config::LuaSkillsCustomGithubMirror;

/// Longest accepted custom mirror identifier or label, counted in Unicode scalar values.
/// 可接受的最长自定义镜像标识或名称，按 Unicode 标量值计数。
const CUSTOM_MIRROR_TEXT_MAX_CHARS: usize = 64;

/// One built-in GitHub mirror in the service contract.
/// 服务契约中的一个内置 GitHub 镜像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsGithubMirror {
    /// Stable identifier.
    /// 稳定标识。
    pub id: String,
    /// English label; interfaces may localize known identifiers.
    /// 英文名称；界面可按已知标识本地化。
    pub label: String,
    /// `direct`, `prefix` or `luaskills_station`.
    /// `direct`、`prefix` 或 `luaskills_station`。
    pub kind: String,
    /// `china` or `global`.
    /// `china` 或 `global`。
    pub region: String,
    /// Download base URL, absent for direct GitHub.
    /// 下载基址；直连 GitHub 时缺省。
    pub github_base_url: Option<String>,
    /// API base URL, absent when the official API is used.
    /// API 基址；使用官方 API 时缺省。
    pub github_api_base_url: Option<String>,
}

/// One mirror to probe, built-in or custom.
/// 一个待测速的镜像，内置或自定义。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LuaSkillsGithubMirrorProbeTarget {
    /// Identifier echoed in the result.
    /// 结果中回显的标识。
    pub id: String,
    /// Download base URL; absent probes github.com directly.
    /// 下载基址；缺省时直接测 github.com。
    #[serde(default)]
    pub github_base_url: Option<String>,
}

/// One mirror probe result in the service contract.
/// 服务契约中的一个镜像测速结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LuaSkillsGithubMirrorProbeResult {
    /// Identifier of the probed target.
    /// 被测目标的标识。
    pub id: String,
    /// Whether usable bytes arrived.
    /// 是否收到了可用字节。
    pub ok: bool,
    /// Whether the whole probe range arrived before the deadline.
    /// 截止前是否收到了完整测速区间。
    pub complete: bool,
    /// HTTP status when a response arrived.
    /// 收到响应时的 HTTP 状态码。
    pub http_status: Option<u16>,
    /// Milliseconds until the first body byte.
    /// 收到首个正文字节前的毫秒数。
    pub ttfb_ms: Option<u64>,
    /// Measured throughput in bytes per second.
    /// 实测吞吐量，单位字节每秒。
    pub bytes_per_second: Option<u64>,
    /// English failure description.
    /// 英文失败描述。
    pub error: Option<String>,
}

/// Returns the built-in GitHub mirror catalog.
/// 返回内置 GitHub 镜像目录。
pub fn github_mirror_catalog() -> Vec<LuaSkillsGithubMirror> {
    builtin_github_mirrors()
        .into_iter()
        .map(service_mirror)
        .collect()
}

/// Probes every target in parallel and returns results in request order.
/// 并行测速全部目标，并按请求顺序返回结果。
///
/// # Errors
/// Returns an English error when the target list is empty or exceeds the LuaSkills limit.
/// 目标列表为空或超过 LuaSkills 上限时返回英文错误。
pub fn probe_mirrors(
    targets: Vec<LuaSkillsGithubMirrorProbeTarget>,
) -> Result<Vec<LuaSkillsGithubMirrorProbeResult>, String> {
    if targets.is_empty() {
        return Err(String::from("Select at least one GitHub mirror to probe."));
    }
    let request = LuaRuntimeGithubMirrorProbeRequest {
        targets: targets
            .into_iter()
            .map(|target| LuaRuntimeGithubMirrorProbeTarget {
                id: target.id,
                github_base_url: target.github_base_url,
            })
            .collect(),
        release_path: None,
        max_bytes: None,
        timeout_ms: None,
    };
    Ok(probe_github_mirrors(&request)?
        .into_iter()
        .map(|result| LuaSkillsGithubMirrorProbeResult {
            id: result.id,
            ok: result.ok,
            complete: result.complete,
            http_status: result.http_status,
            ttfb_ms: result.ttfb_ms,
            bytes_per_second: result.bytes_per_second,
            error: result.error,
        })
        .collect())
}

/// Validates user-added mirrors and returns them with normalized base URLs.
/// 校验用户添加的镜像，并返回规范化基址后的列表。
///
/// # Errors
/// Returns an English error naming the first empty, overlong, duplicate or invalid entry.
/// 返回英文错误，指出第一个为空、过长、重复或无效的条目。
pub fn validated_custom_github_mirrors(
    mirrors: &[LuaSkillsCustomGithubMirror],
) -> Result<Vec<LuaSkillsCustomGithubMirror>, String> {
    let builtin_ids = builtin_github_mirrors()
        .into_iter()
        .map(|mirror| mirror.id)
        .collect::<BTreeSet<_>>();
    let mut seen_ids = BTreeSet::new();
    mirrors
        .iter()
        .map(|mirror| {
            let id = mirror.id.trim();
            let label = mirror.label.trim();
            if id.is_empty() || label.is_empty() {
                return Err(String::from(
                    "A custom GitHub mirror needs both an identifier and a label.",
                ));
            }
            if id.chars().count() > CUSTOM_MIRROR_TEXT_MAX_CHARS
                || label.chars().count() > CUSTOM_MIRROR_TEXT_MAX_CHARS
            {
                return Err(format!(
                    "Custom GitHub mirror `{id}` has an identifier or label longer than {CUSTOM_MIRROR_TEXT_MAX_CHARS} characters."
                ));
            }
            if builtin_ids.contains(id) || !seen_ids.insert(id.to_string()) {
                return Err(format!(
                    "Custom GitHub mirror identifier `{id}` is already used."
                ));
            }
            let github_base_url = normalize_github_mirror_base_url(&mirror.github_base_url)
                .map_err(|error| format!("Custom GitHub mirror `{id}`: {error}"))?;
            Ok(LuaSkillsCustomGithubMirror {
                id: id.to_string(),
                label: label.to_string(),
                github_base_url,
            })
        })
        .collect()
}

/// Converts one LuaSkills mirror into the service contract.
/// 将一个 LuaSkills 镜像转换为服务契约。
fn service_mirror(mirror: LuaRuntimeGithubMirror) -> LuaSkillsGithubMirror {
    LuaSkillsGithubMirror {
        id: mirror.id,
        label: mirror.label,
        kind: match mirror.kind {
            LuaRuntimeGithubMirrorKind::Direct => "direct",
            LuaRuntimeGithubMirrorKind::Prefix => "prefix",
            LuaRuntimeGithubMirrorKind::LuaskillsStation => "luaskills_station",
        }
        .to_string(),
        region: match mirror.region {
            LuaRuntimeGithubMirrorRegion::China => "china",
            LuaRuntimeGithubMirrorRegion::Global => "global",
        }
        .to_string(),
        github_base_url: mirror.github_base_url,
        github_api_base_url: mirror.github_api_base_url,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one custom mirror literal for the validation tests.
    /// 为校验测试构建一个自定义镜像字面量。
    fn custom(id: &str, url: &str) -> LuaSkillsCustomGithubMirror {
        LuaSkillsCustomGithubMirror {
            id: id.to_string(),
            label: format!("{id} label"),
            github_base_url: url.to_string(),
        }
    }

    /// Verify a bare mirror origin is normalized into a GitHub prefix and kept in order.
    /// 验证裸镜像源地址会被规范化为 GitHub 前缀，并保持顺序。
    #[test]
    fn custom_mirrors_are_normalized() {
        let mirrors = validated_custom_github_mirrors(&[
            custom(" mine ", "https://mirror.example.cn"),
            custom("other", "https://other.example/gh"),
        ])
        .expect("valid custom mirrors");
        assert_eq!(mirrors[0].id, "mine");
        assert!(mirrors[0].github_base_url.ends_with("/https://github.com"));
        assert_eq!(mirrors[1].id, "other");
    }

    /// Verify duplicates, built-in identifiers and invalid URLs are refused.
    /// 验证重复标识、内置标识与无效地址会被拒绝。
    #[test]
    fn invalid_custom_mirrors_are_refused() {
        let builtin_id = github_mirror_catalog()
            .into_iter()
            .next()
            .expect("built-in mirror")
            .id;
        for mirrors in [
            vec![
                custom("same", "https://a.example"),
                custom("same", "https://b.example"),
            ],
            vec![custom(&builtin_id, "https://a.example")],
            vec![custom("bad", "ftp://a.example")],
            vec![custom("", "https://a.example")],
        ] {
            assert!(validated_custom_github_mirrors(&mirrors).is_err());
        }
    }

    /// Verify the catalog uses only the documented kind and region keys.
    /// 验证目录只使用已记录的类型与区域键。
    #[test]
    fn catalog_uses_stable_keys() {
        let catalog = github_mirror_catalog();
        assert!(!catalog.is_empty());
        for mirror in catalog {
            assert!(["direct", "prefix", "luaskills_station"].contains(&mirror.kind.as_str()));
            assert!(["china", "global"].contains(&mirror.region.as_str()));
        }
    }

    /// Verify an empty probe request is refused before any network access.
    /// 验证空测速请求会在任何网络访问前被拒绝。
    #[test]
    fn empty_probe_is_refused() {
        assert!(probe_mirrors(Vec::new()).is_err());
    }
}
