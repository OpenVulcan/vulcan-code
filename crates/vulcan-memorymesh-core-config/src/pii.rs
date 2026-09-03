//! Typed PII rule compilation and deterministic redaction.
//! 强类型 PII 规则编译与确定性脱敏。

use crate::{pii_condition::Condition, source_regex::compile_source_regex};
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_memorymesh_core_contracts::{CoreError, ErrorCode, ErrorStage, MAX_DOMAIN_TEXT_BYTES};

/// Reports one applied PII redaction.
/// 报告一条已应用的 PII 脱敏。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiiRedaction {
    /// Names the rule that matched.
    /// 命名匹配的规则。
    pub rule_name: String,
    /// Counts matches replaced by this rule.
    /// 统计该规则替换的匹配数。
    pub match_count: u32,
}

/// Reports scrubbed text and stable redaction metadata.
/// 报告已清洗文本与稳定脱敏元数据。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiiScrubResult {
    /// Carries scrubbed text.
    /// 携带已清洗文本。
    pub text: String,
    /// Carries applied rules in execution order.
    /// 按执行顺序携带已应用规则。
    pub redactions: Vec<PiiRedaction>,
}

/// Applies compiled common and locale-specific PII rules.
/// 应用已编译的 common 与特定区域 PII 规则。
#[derive(Clone)]
pub struct PiiScrubber {
    /// Carries compiled rules in stable execution order.
    /// 以稳定执行顺序携带已编译规则。
    rules: Vec<CompiledRule>,
    /// Carries field names excluded by the authoritative schema.
    /// 携带权威 Schema 排除的字段名。
    excludes: BTreeSet<String>,
}

impl std::fmt::Debug for PiiScrubber {
    /// Formats rule counts without leaking rule payloads.
    /// 格式化规则数量而不泄漏规则载荷。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PiiScrubber")
            .field("rule_count", &self.rules.len())
            .field("exclude_count", &self.excludes.len())
            .finish()
    }
}

impl PiiScrubber {
    /// Compiles ordered common and locale rule files.
    /// 编译有序的 common 与区域规则文件。
    ///
    /// # Errors
    /// Returns an error for malformed JSON, invalid regex, duplicate names, or unsupported conditions.
    /// 当 JSON 格式错误、正则无效、名称重复或条件不受支持时返回错误。
    pub(crate) fn compile(files: &[(String, Vec<u8>)]) -> Result<Self, CoreError> {
        // Replace locale rules in the common rule's original position, matching VMM precedence.
        // 在通用规则的原位置替换区域规则，对齐 VMM 优先级。
        let mut positions = BTreeMap::new();
        // Preserve file and rule order because sequential redaction is part of the contract.
        // 保持文件与规则顺序，因为顺序脱敏是契约的一部分。
        let mut rules = Vec::new();
        // Merge excludes as a set because field exclusion has no precedence ordering.
        // 将排除字段合并为集合，因为字段排除没有优先级顺序。
        let mut excludes = BTreeSet::new();
        for (path, bytes) in files {
            // Parse directly into the closed rule schema.
            // 直接解析为封闭规则 Schema。
            let definition: RuleFile = serde_json::from_slice(bytes)
                .map_err(|error| pii_error(format!("invalid PII rule file {path}: {error}")))?;
            if definition.version != "2.0.0" {
                return Err(pii_error(format!(
                    "unsupported PII rule version in {path}: {}",
                    definition.version
                )));
            }
            if !path.ends_with(&format!("/{}.json", definition.language)) {
                return Err(pii_error(format!(
                    "PII language does not match path: {path}"
                )));
            }
            let mut names = BTreeSet::new();
            for definition in definition.rules {
                let name = definition.name.trim().to_owned();
                if name.is_empty() || !names.insert(name.clone()) {
                    return Err(pii_error(format!(
                        "empty or duplicate PII rule name in {path}: {}",
                        definition.name
                    )));
                }
                // Compile Rust regex once per immutable snapshot.
                // 每个不可变快照仅编译一次 Rust 正则。
                let pattern =
                    compile_source_regex(&definition.pattern, false).map_err(|error| {
                        pii_error(format!("invalid PII regex {}: {error}", definition.name))
                    })?;
                // Compile the bounded builtin/operator vocabulary of the authoritative evaluator.
                // 编译权威求值器的有界内建函数与操作符词汇。
                let condition =
                    Condition::parse(definition.condition.as_deref(), pattern.captures_len())?;
                let replacement =
                    compile_replacement(&definition.replacement, pattern.captures_len())?;
                let compiled = CompiledRule {
                    name: name.clone(),
                    pattern,
                    replacement,
                    condition,
                };
                if let Some(index) = positions.get(&name) {
                    rules[*index] = compiled;
                } else {
                    positions.insert(name, rules.len());
                    rules.push(compiled);
                }
            }
            excludes.extend(definition.excludes);
        }
        Ok(Self { rules, excludes })
    }

    /// Scrubs PII from one arbitrary UTF-8 text value.
    /// 从一个任意 UTF-8 文本值中清洗 PII。
    ///
    /// # Errors
    /// Rejects oversized input or any intermediate expansion beyond the domain text bound.
    /// 拒绝超限输入或任何超过领域文本边界的中间展开。
    pub fn scrub(&self, text: &str) -> Result<PiiScrubResult, CoreError> {
        if text.len() > MAX_DOMAIN_TEXT_BYTES {
            return Err(CoreError::new(
                ErrorCode::InvalidArgument,
                ErrorStage::Validation,
                "PII input exceeds domain text capacity",
            ));
        }
        // VMM leaves empty source text empty even when an ad-hoc regex can match zero bytes.
        // 即使临时正则可以匹配零字节，VMM 仍保持空来源文本为空。
        if text.is_empty() {
            return Ok(PiiScrubResult {
                text: String::new(),
                redactions: Vec::new(),
            });
        }
        // Carry the output through each rule because authoritative rule order is observable.
        // 让输出依次通过每条规则，因为权威规则顺序是可观察契约。
        let mut output = text.to_owned();
        // Record only rules that actually replaced content.
        // 仅记录实际替换内容的规则。
        let mut redactions = Vec::new();
        for rule in &self.rules {
            // Avoid allocating a full-size copy for the common no-match path.
            // 对常见的无命中路径避免分配完整大小副本。
            let mut occurrences = rule.pattern.captures_iter(&output).peekable();
            if occurrences.peek().is_none() {
                continue;
            }
            // Count accepted matches separately from regex candidates rejected by a condition.
            // 分别统计被接受的匹配与被条件拒绝的正则候选。
            let mut match_count = 0_u32;
            // Expand only numeric references; Rust's named-dollar syntax is not the VMM contract.
            // 仅展开数字引用；Rust 的命名美元语法不是 VMM 契约。
            let mut replaced = String::with_capacity(output.len());
            let mut cursor = 0;
            for captures in occurrences {
                let matched = captures.get(0).ok_or_else(|| {
                    CoreError::new(
                        ErrorCode::Internal,
                        ErrorStage::Command,
                        "PII matcher omitted its complete capture",
                    )
                })?;
                append_bounded(&mut replaced, &output[cursor..matched.start()])?;
                if rule.condition.evaluate(&captures) {
                    match_count = match_count.saturating_add(1);
                    for part in &rule.replacement {
                        match part {
                            ReplacementPart::Literal(text) => append_bounded(&mut replaced, text)?,
                            ReplacementPart::Capture(index) => {
                                if let Some(group) = captures.get(*index) {
                                    append_bounded(&mut replaced, group.as_str())?;
                                }
                            }
                        }
                    }
                } else {
                    append_bounded(&mut replaced, matched.as_str())?;
                }
                cursor = matched.end();
            }
            append_bounded(&mut replaced, &output[cursor..])?;
            if match_count > 0 {
                output = replaced;
                redactions.push(PiiRedaction {
                    rule_name: rule.name.clone(),
                    match_count,
                });
            }
        }
        Ok(PiiScrubResult {
            text: output,
            redactions,
        })
    }

    /// Reports whether one structured input field is excluded from PII processing.
    /// 报告一个结构化输入字段是否排除在 PII 处理之外。
    #[must_use]
    pub fn is_excluded_field(&self, field: &str) -> bool {
        self.excludes.contains(field)
    }

    /// Reports the number of compiled rules.
    /// 报告已编译规则数量。
    #[must_use]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

/// Checks each literal/capture segment before allocating expanded output; never returns raw fallback text.
/// 在分配展开输出前检查每个字面或捕获片段；绝不返回原文兜底。
fn append_bounded(output: &mut String, value: &str) -> Result<(), CoreError> {
    if value.len() > MAX_DOMAIN_TEXT_BYTES.saturating_sub(output.len()) {
        return Err(CoreError::new(
            ErrorCode::InvalidConfiguration,
            ErrorStage::Command,
            "PII replacement expands beyond domain text capacity",
        ));
    }
    output.push_str(value);
    Ok(())
}

/// Describes one PII JSON resource file.
/// 描述一个 PII JSON 资源文件。
#[derive(Deserialize)]
struct RuleFile {
    /// Carries the language identity.
    /// 携带语言标识。
    language: String,
    /// Carries the schema version.
    /// 携带 Schema 版本。
    version: String,
    /// Carries ordered rule definitions.
    /// 携带有序规则定义。
    rules: Vec<RuleDefinition>,
    /// Carries structured field exclusions.
    /// 携带结构化字段排除项。
    #[serde(default)]
    excludes: Vec<String>,
}

/// Describes one PII JSON rule.
/// 描述一条 PII JSON 规则。
#[derive(Deserialize)]
struct RuleDefinition {
    /// Carries the stable rule name.
    /// 携带稳定规则名称。
    name: String,
    /// Carries the regex pattern.
    /// 携带正则模式。
    pattern: String,
    /// Carries the capture-aware replacement.
    /// 携带可感知捕获组的替换文本。
    replacement: String,
    /// Carries an optional whitelisted validator expression.
    /// 携带可选的白名单校验表达式。
    condition: Option<String>,
}

/// Holds one compiled PII rule.
/// 保存一条已编译 PII 规则。
#[derive(Clone)]
struct CompiledRule {
    /// Carries the stable rule name.
    /// 携带稳定规则名称。
    name: String,
    /// Carries the compiled regex.
    /// 携带已编译正则。
    pattern: Regex,
    /// Carries validated literal and numeric capture segments.
    /// 携带已验证的字面值和数字捕获片段。
    replacement: Vec<ReplacementPart>,
    /// Carries the compiled validator.
    /// 携带已编译校验器。
    condition: Condition,
}

/// Keeps replacement grammar explicit and independent from regex-crate expansion extensions.
/// 保持替换语法显式化，并独立于 regex crate 的展开扩展。
#[derive(Clone)]
enum ReplacementPart {
    /// Preserves literal bytes, including non-numeric dollar forms.
    /// 保留字面字节，包括非数字美元形式。
    Literal(String),
    /// Expands a validated numbered group; an unmatched optional group contributes nothing.
    /// 展开已验证的编号捕获；未命中的可选捕获不贡献内容。
    Capture(usize),
}

/// Compiles VMM's dollar-digit grammar and rejects overflow or nonexistent capture references.
/// 编译 VMM 的美元数字语法，并拒绝溢出或不存在的捕获引用。
fn compile_replacement(
    text: &str,
    capture_count: usize,
) -> Result<Vec<ReplacementPart>, CoreError> {
    let bytes = text.as_bytes();
    let mut parts = Vec::new();
    let mut cursor = 0;
    let mut literal_start = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'$'
            || bytes
                .get(cursor + 1)
                .is_none_or(|byte| !byte.is_ascii_digit())
        {
            cursor += 1;
            continue;
        }
        if literal_start < cursor {
            parts.push(ReplacementPart::Literal(
                text[literal_start..cursor].to_owned(),
            ));
        }
        cursor += 1;
        let mut index = 0_usize;
        while let Some(byte) = bytes.get(cursor).filter(|byte| byte.is_ascii_digit()) {
            index = index
                .checked_mul(10)
                .and_then(|value| value.checked_add(usize::from(*byte - b'0')))
                .ok_or_else(|| pii_error("PII replacement capture index overflow"))?;
            cursor += 1;
        }
        if index >= capture_count {
            return Err(pii_error("PII replacement references an undefined capture"));
        }
        parts.push(ReplacementPart::Capture(index));
        literal_start = cursor;
    }
    if literal_start < text.len() {
        parts.push(ReplacementPart::Literal(text[literal_start..].to_owned()));
    }
    Ok(parts)
}

/// Creates one configuration-stage PII error.
/// 创建一个配置阶段 PII 错误。
fn pii_error(diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidConfiguration,
        ErrorStage::Configuration,
        diagnostic,
    )
}
