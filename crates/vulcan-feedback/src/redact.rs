//! On-device filtering applied to every feedback text before the user previews it.
//! 用户预览前对每段反馈文本执行的本机过滤。
//!
//! Order matters and is fixed: known secrets, paths, the account name, URLs, then the bundled PII rules.
//! Paths run before the account name so a home path collapses to `~` instead of exposing its parent.
//! 顺序固定且有意义：已知秘密、路径、系统用户名、URL，最后是随包 PII 规则。路径先于用户名处理，
//! 使主目录路径整体折叠为 `~`，而不是暴露其上级目录。

use crate::FeedbackError;
use regex::Regex;
use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use vulcan_memorymesh_core_config::{
    PiiScrubber, embedded_pii_scrubber, embedded_resource_identity,
};

/// Replacement counts per category; never holds original text.
/// 每个类别的替换次数；从不保存原文。
pub type RedactionHits = BTreeMap<String, u32>;

/// Category of exact known-secret replacements.
/// 已知秘密精确替换的类别。
pub const HIT_SECRET: &str = "secret";
/// Category of path replacements.
/// 路径替换的类别。
pub const HIT_PATH: &str = "path";
/// Category of account-name replacements.
/// 系统用户名替换的类别。
pub const HIT_USER: &str = "user";
/// Category of URL reductions.
/// URL 精简的类别。
pub const HIT_URL: &str = "url";

/// Host suffixes that only resolve inside a private network.
/// 只在私有网络内解析的主机后缀。
const PRIVATE_HOST_SUFFIXES: [&str; 7] = [
    ".local",
    ".lan",
    ".internal",
    ".corp",
    ".home",
    ".intranet",
    ".localhost",
];

/// One secret value known to this process, replaced exactly wherever it appears.
/// 当前进程已知的一个秘密值，出现在任何位置都精确替换。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownSecret {
    /// Public label shown in the replacement, such as an environment variable name.
    /// 替换文本中显示的公开标签，例如环境变量名。
    pub label: String,
    /// Secret value; the caller applies its own minimum length before passing it.
    /// 秘密值；调用方在传入前自行执行最小长度规则。
    pub value: String,
}

/// Everything the filter needs to know about this machine.
/// 过滤器需要了解的本机信息。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RedactorInputs {
    /// Registered secret values.
    /// 已登记的秘密值。
    pub secrets: Vec<KnownSecret>,
    /// Project roots replaced by `<workspace>`.
    /// 替换为 `<workspace>` 的项目根目录。
    pub workspace_roots: Vec<PathBuf>,
    /// Authoritative data root replaced by `<data-root>`.
    /// 替换为 `<data-root>` 的权威数据根。
    pub data_root: Option<PathBuf>,
    /// User home replaced by `~`.
    /// 替换为 `~` 的用户主目录。
    pub home_dir: Option<PathBuf>,
    /// Operating-system account name replaced by `<user>`.
    /// 替换为 `<user>` 的操作系统账户名。
    pub user_name: Option<String>,
    /// Interface locale selecting the bundled PII rule file.
    /// 选择随包 PII 规则文件的界面语言。
    pub locale: String,
}

/// Compiled filter for one preparation.
/// 一次准备过程使用的已编译过滤器。
pub struct Redactor {
    secrets: Vec<(String, String)>,
    paths: Vec<(String, &'static str)>,
    user: Option<Regex>,
    url: Regex,
    pii: PiiScrubber,
    rules_version: String,
}

impl Redactor {
    /// Compiles the filter for this machine.
    /// 为本机编译过滤器。
    ///
    /// # Parameters
    /// 参数
    /// `inputs` lists the secrets, paths, account name and locale to filter.
    /// `inputs` 列出需要过滤的秘密、路径、账户名与语言。
    ///
    /// # Returns
    /// 返回值
    /// The compiled filter, or an error when the bundled rules cannot compile.
    /// 编译好的过滤器；随包规则无法编译时返回错误。
    pub fn new(inputs: RedactorInputs) -> Result<Self, FeedbackError> {
        let mut secrets = Vec::new();
        for secret in inputs.secrets {
            if secret.value.is_empty() {
                continue;
            }
            let replacement = format!("[redacted:{}]", secret.label);
            let escaped = json_escaped(&secret.value);
            if escaped != secret.value {
                secrets.push((escaped, replacement.clone()));
            }
            secrets.push((secret.value, replacement));
        }
        secrets.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
        secrets.dedup_by(|left, right| left.0 == right.0);

        let mut paths = Vec::new();
        for root in &inputs.workspace_roots {
            push_path_variants(&mut paths, root, "<workspace>");
        }
        if let Some(root) = &inputs.data_root {
            push_path_variants(&mut paths, root, "<data-root>");
        }
        if let Some(home) = &inputs.home_dir {
            push_path_variants(&mut paths, home, "~");
        }
        paths.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
        paths.dedup_by(|left, right| left.0.eq_ignore_ascii_case(&right.0));

        let user = match inputs.user_name.as_deref().map(str::trim) {
            Some(name) if name.chars().count() >= 3 => Some(
                Regex::new(&format!(r"(?i)\b{}\b", regex::escape(name)))
                    .map_err(|error| FeedbackError::filter(error.to_string()))?,
            ),
            _ => None,
        };
        let url = Regex::new(r#"(?i)\b[a-z][a-z0-9+.\-]*://[^\s"'<>()\[\]{}\\]+"#)
            .map_err(|error| FeedbackError::filter(error.to_string()))?;
        let (pii, language) = embedded_pii_scrubber(&inputs.locale)
            .map_err(|error| FeedbackError::filter(error.to_string()))?;
        let identity = embedded_resource_identity()
            .map_err(|error| FeedbackError::filter(error.to_string()))?;
        Ok(Self {
            secrets,
            paths,
            user,
            url,
            pii,
            rules_version: format!("{}/{language}", identity.package_version),
        })
    }

    /// Identity of the bundled rule set, sent in the redaction summary.
    /// 随包规则集的标识，随过滤摘要发送。
    pub fn rules_version(&self) -> &str {
        &self.rules_version
    }

    /// Filters one text and adds what it replaced to `hits`.
    /// 过滤一段文本，并把替换内容计入 `hits`。
    ///
    /// # Parameters
    /// 参数
    /// `text` is the original text and `hits` accumulates counts across fields.
    /// `text` 是原文，`hits` 跨字段累计计数。
    ///
    /// # Returns
    /// 返回值
    /// The filtered text, or an error when the PII engine refuses the input.
    /// 过滤后的文本；PII 引擎拒绝输入时返回错误。
    pub fn redact(&self, text: &str, hits: &mut RedactionHits) -> Result<String, FeedbackError> {
        let mut output = text.to_owned();
        for (needle, replacement) in &self.secrets {
            let count = output.matches(needle.as_str()).count();
            if count > 0 {
                output = output.replace(needle.as_str(), replacement);
                add_hits(hits, HIT_SECRET, count);
            }
        }
        for (needle, placeholder) in &self.paths {
            let (replaced, count) = replace_ascii_case_insensitive(&output, needle, placeholder);
            if count > 0 {
                output = replaced;
                add_hits(hits, HIT_PATH, count);
            }
        }
        if let Some(user) = &self.user {
            let count = user.find_iter(&output).count();
            if count > 0 {
                output = user.replace_all(&output, "<user>").into_owned();
                add_hits(hits, HIT_USER, count);
            }
        }
        let mut url_changes = 0_usize;
        output = self
            .url
            .replace_all(&output, |captures: &regex::Captures<'_>| {
                let original = &captures[0];
                let reduced = reduce_url(original);
                if reduced != original {
                    url_changes += 1;
                }
                reduced
            })
            .into_owned();
        add_hits(hits, HIT_URL, url_changes);
        let scrubbed = self
            .pii
            .scrub(&output)
            .map_err(|error| FeedbackError::filter(error.to_string()))?;
        for redaction in scrubbed.redactions {
            add_hits(
                hits,
                &redaction.rule_name.to_ascii_lowercase(),
                redaction.match_count as usize,
            );
        }
        Ok(scrubbed.text)
    }
}

/// Adds `count` to one category, skipping zero so summaries list only real replacements.
/// 把 `count` 计入一个类别；零次不记录，使摘要只列出实际替换。
fn add_hits(hits: &mut RedactionHits, category: &str, count: usize) {
    if count == 0 {
        return;
    }
    let entry = hits.entry(category.to_owned()).or_insert(0);
    *entry = entry.saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
}

/// Encodes a value the way it appears inside a JSON string, without the surrounding quotes.
/// 按 JSON 字符串内部的形式编码一个值，不含两侧引号。
fn json_escaped(value: &str) -> String {
    let quoted = serde_json::Value::String(value.to_owned()).to_string();
    quoted[1..quoted.len() - 1].to_owned()
}

/// Pushes the spellings one path takes in logs and messages: native, both separators, JSON-escaped.
/// 推入一个路径在日志与消息中的各种写法：原生、两种分隔符、JSON 转义。
fn push_path_variants(
    paths: &mut Vec<(String, &'static str)>,
    path: &std::path::Path,
    placeholder: &'static str,
) {
    let text = path.to_string_lossy();
    let trimmed = text.trim_end_matches(['/', '\\']);
    if trimmed.chars().count() < 3 {
        return;
    }
    let forward = trimmed.replace('\\', "/");
    let backward = trimmed.replace('/', "\\");
    let escaped = backward.replace('\\', "\\\\");
    for variant in [trimmed.to_owned(), forward, backward, escaped] {
        paths.push((variant, placeholder));
    }
}

/// Replaces every ASCII-case-insensitive occurrence of `needle`.
/// 替换 `needle` 的每一处 ASCII 大小写不敏感出现。
///
/// ASCII lowercasing keeps byte offsets, so positions found in the lowered copy are valid in the original.
/// ASCII 小写化保持字节偏移，因此在小写副本中找到的位置对原文同样有效。
fn replace_ascii_case_insensitive(
    haystack: &str,
    needle: &str,
    replacement: &str,
) -> (String, usize) {
    let lowered_haystack = haystack.to_ascii_lowercase();
    let lowered_needle = needle.to_ascii_lowercase();
    let mut output = String::with_capacity(haystack.len());
    let mut cursor = 0;
    let mut count = 0;
    while let Some(offset) = lowered_haystack[cursor..].find(&lowered_needle) {
        let start = cursor + offset;
        output.push_str(&haystack[cursor..start]);
        output.push_str(replacement);
        cursor = start + needle.len();
        count += 1;
    }
    output.push_str(&haystack[cursor..]);
    (output, count)
}

/// Keeps scheme, host and path; drops credentials, query and fragment; hides private hosts.
/// 保留协议、主机与路径；去掉凭据、查询串与片段；隐藏私有主机。
fn reduce_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_owned();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let remainder = &rest[authority_end..];
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let path = if remainder.starts_with('/') {
        remainder.split(['?', '#']).next().unwrap_or_default()
    } else {
        ""
    };
    let host = if host_is_private(host) {
        "<host>"
    } else {
        host
    };
    format!("{scheme}://{host}{path}")
}

/// Whether a `host[:port]` only identifies something inside the user's own network.
/// `host[:port]` 是否只标识用户自身网络内的目标。
fn host_is_private(host_port: &str) -> bool {
    if host_port.starts_with('[') {
        return true;
    }
    let host = match host_port.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|value| value.is_ascii_digit()) => host,
        _ => host_port,
    };
    let host = host.to_ascii_lowercase();
    host.parse::<Ipv4Addr>().is_ok()
        || host == "localhost"
        || !host.contains('.')
        || PRIVATE_HOST_SUFFIXES
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redactor(inputs: RedactorInputs) -> Redactor {
        Redactor::new(RedactorInputs {
            locale: String::from("en-US"),
            ..inputs
        })
        .unwrap()
    }

    #[test]
    fn known_secrets_are_replaced_with_their_label_including_json_escaped_spellings() {
        let filter = redactor(RedactorInputs {
            secrets: vec![KnownSecret {
                label: String::from("MY_TOKEN"),
                value: String::from(r#"abc"def"#),
            }],
            ..RedactorInputs::default()
        });
        let mut hits = RedactionHits::new();
        let output = filter
            .redact(r#"raw abc"def and json abc\"def"#, &mut hits)
            .unwrap();
        assert_eq!(
            output,
            "raw [redacted:MY_TOKEN] and json [redacted:MY_TOKEN]"
        );
        assert_eq!(hits.get(HIT_SECRET), Some(&2));
    }

    #[test]
    fn paths_collapse_to_placeholders_in_every_spelling_and_case() {
        let filter = redactor(RedactorInputs {
            workspace_roots: vec![PathBuf::from(r"C:\Users\Ada\proj")],
            home_dir: Some(PathBuf::from(r"C:\Users\Ada")),
            user_name: Some(String::from("Ada")),
            ..RedactorInputs::default()
        });
        let mut hits = RedactionHits::new();
        let output = filter
            .redact(
                r#"a c:/users/ada/proj/src b {"p":"C:\\Users\\Ada\\.vulcan"} c ada said"#,
                &mut hits,
            )
            .unwrap();
        assert_eq!(
            output,
            r#"a <workspace>/src b {"p":"~\\.vulcan"} c <user> said"#
        );
        assert_eq!(hits.get(HIT_PATH), Some(&2));
        assert_eq!(hits.get(HIT_USER), Some(&1));
    }

    #[test]
    fn urls_lose_credentials_query_and_private_hosts() {
        let filter = redactor(RedactorInputs::default());
        let mut hits = RedactionHits::new();
        let output = filter
            .redact(
                "see https://api.example.com/v1/chat?key=1#x and http://10.0.0.2:8080/a and https://llm.corp/x",
                &mut hits,
            )
            .unwrap();
        assert_eq!(
            output,
            "see https://api.example.com/v1/chat and http://<host>/a and https://<host>/x"
        );
        assert_eq!(hits.get(HIT_URL), Some(&3));
    }

    #[test]
    fn bundled_pii_rules_run_and_report_rule_categories() {
        let filter = redactor(RedactorInputs::default());
        let mut hits = RedactionHits::new();
        let output = filter
            .redact("contact ada@example.com now", &mut hits)
            .unwrap();
        assert!(!output.contains("ada@example.com"));
        assert_eq!(hits.get("email_address"), Some(&1));
        assert!(filter.rules_version().ends_with("/en-US"));
    }

    #[test]
    fn locales_without_their_own_rule_file_fall_back_to_en_us() {
        let filter = Redactor::new(RedactorInputs {
            locale: String::from("ru-RU"),
            ..RedactorInputs::default()
        })
        .unwrap();
        assert!(filter.rules_version().ends_with("/en-US"));
    }
}
