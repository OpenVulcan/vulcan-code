//! Typed Noise rule compilation and deterministic local detection.
//! 强类型 Noise 规则编译与确定性本地检测。

use crate::source_regex::compile_source_regex;
use regex::Regex;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_memorymesh_core_contracts::{CoreError, ErrorCode, ErrorStage, Role};

/// Identifies the only supported rule schema and the version reported in admission audits.
/// 标识唯一支持的规则 Schema 及准入审计报告的版本。
pub const NOISE_RULE_SCHEMA_VERSION: &str = "1.0.0";

/// Reports one deterministic Noise decision.
/// 报告一次确定性 Noise 决策。
#[derive(Clone, Debug, PartialEq)]
pub struct NoiseDecision {
    /// States whether the text is classified as noise.
    /// 表示文本是否分类为噪声。
    pub is_noise: bool,
    /// Names the matched category when present.
    /// 在存在时命名匹配类别。
    pub category: Option<String>,
    /// Carries the deterministic local confidence.
    /// 携带确定性本地置信度。
    pub confidence: f64,
}

/// Applies compiled common and locale-overridden Noise categories.
/// 应用已编译的 common 与区域覆盖 Noise 类别。
#[derive(Clone)]
pub struct NoiseDetector {
    /// Freezes global gates and the default category threshold for this snapshot.
    /// 为此快照冻结全局开关与类别默认阈值。
    policy: NoisePolicy,
    /// Preserves source category order while replacing locale overrides in place.
    /// 保留来源类别顺序，同时原位替换区域覆盖。
    categories: Vec<CompiledCategory>,
}

impl std::fmt::Debug for NoiseDetector {
    /// Formats category counts without dumping pattern payloads.
    /// 格式化类别数量而不输出模式载荷。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NoiseDetector")
            .field("policy", &self.policy)
            .field("category_count", &self.categories.len())
            .finish()
    }
}

impl NoiseDetector {
    /// Compiles common categories followed by locale category overrides.
    /// 编译 common 类别以及其后的区域类别覆盖。
    ///
    /// # Errors
    /// Returns an error for malformed JSON, invalid regex, threshold, target, or duplicate category.
    /// 当 JSON、正则、阈值、目标无效或类别重复时返回错误。
    pub(crate) fn compile(
        files: &[(String, Vec<u8>)],
        policy: NoisePolicy,
    ) -> Result<Self, CoreError> {
        // Preserve first occurrence order: the first matching rule owns the diagnostic.
        // 保留首次出现顺序：首个命中规则拥有决策诊断。
        let mut categories = BTreeMap::new();
        let mut order = Vec::new();
        for (path, bytes) in files {
            // Parse directly into the closed Noise schema.
            // 直接解析为封闭 Noise Schema。
            let definition: NoiseFile = serde_json::from_slice(bytes)
                .map_err(|error| noise_error(format!("invalid Noise rule file {path}: {error}")))?;
            if definition.version != NOISE_RULE_SCHEMA_VERSION {
                return Err(noise_error(format!(
                    "unsupported Noise rule version in {path}: {}",
                    definition.version
                )));
            }
            if !path.ends_with(&format!("/{}.json", definition.language)) {
                return Err(noise_error(format!(
                    "Noise language does not match path: {path}"
                )));
            }
            // Reject duplicate names inside one file while allowing the next locale file to override common.
            // 拒绝单个文件内的重复名称，同时允许后续区域文件覆盖 common。
            let mut names_in_file = BTreeSet::new();
            for category in definition.categories {
                if !names_in_file.insert(category.name.trim().to_owned()) {
                    return Err(noise_error(format!(
                        "duplicate Noise category in {path}: {}",
                        category.name
                    )));
                }
                // Compile the entire category before replacing any existing common definition.
                // 在替换任何现有 common 定义前完整编译类别。
                let compiled =
                    CompiledCategory::compile(category, policy.default_semantic_threshold)?;
                if !categories.contains_key(&compiled.name) {
                    order.push(compiled.name.clone());
                }
                categories.insert(compiled.name.clone(), compiled);
            }
        }
        Ok(Self {
            policy,
            categories: order
                .into_iter()
                .filter_map(|name| categories.remove(&name))
                .collect(),
        })
    }

    /// Detects only regex matches; phrases require the semantic embedding path.
    /// 仅检测正则匹配；短语必须走语义嵌入链路。
    #[must_use]
    pub fn detect(&self, role: Role, text: &str) -> NoiseDecision {
        let text = text.trim();
        for category in &self.categories {
            if !self.policy.enabled || text.is_empty() || !category.targets.contains(&role) {
                continue;
            }
            // A semantic prototype is never an unconditional substring rejection rule.
            // 语义原型绝不是无条件的子串拒绝规则。
            let matched = category
                .patterns
                .iter()
                .any(|pattern| pattern.is_match(text));
            if matched {
                return NoiseDecision {
                    is_noise: true,
                    category: Some(category.name.clone()),
                    confidence: 1.0,
                };
            }
        }
        NoiseDecision {
            is_noise: false,
            category: None,
            confidence: 0.0,
        }
    }

    /// Reports the number of active categories after locale override.
    /// 报告区域覆盖后的活跃类别数量。
    #[must_use]
    pub fn category_count(&self) -> usize {
        self.categories.len()
    }

    /// Lists every role-targeted category phrase with its semantic threshold.
    /// 列出每个带角色目标的类别短语及其语义阈值。
    #[must_use]
    pub fn semantic_phrase_targets(&self, role: Role) -> Vec<NoisePhraseTarget> {
        let mut targets = Vec::new();
        if !self.policy.enabled || !self.policy.semantic_enabled {
            return targets;
        }
        for category in &self.categories {
            if !category.targets.contains(&role) {
                continue;
            }
            for phrase in &category.phrases {
                targets.push(NoisePhraseTarget {
                    category: category.name.clone(),
                    phrase: phrase.clone(),
                    threshold: category.threshold,
                });
            }
        }
        targets
    }

    /// Evaluates one semantic Noise decision from a normalized text vector and prototype rows.
    /// 基于规范化文本向量与原型行评估一次语义 Noise 决策。
    #[must_use]
    pub fn semantic_decision(
        &self,
        role: Role,
        text_vector: &[f32],
        prototypes: &[NoisePrototypeRow],
    ) -> NoiseDecision {
        let mut best: Option<(f64, String)> = None;
        for category in &self.categories {
            if !self.policy.enabled
                || !self.policy.semantic_enabled
                || !category.targets.contains(&role)
            {
                continue;
            }
            // Resolve rows by source phrase order; storage order and cached thresholds are not policy.
            // 按来源短语顺序定位行；存储顺序和缓存阈值不是策略权威。
            for phrase in &category.phrases {
                let Some(prototype) = prototypes
                    .iter()
                    .find(|row| row.category == category.name && row.phrase == *phrase)
                else {
                    continue;
                };
                let Some(similarity) = dot_similarity(text_vector, &prototype.vector) else {
                    continue;
                };
                if similarity >= category.threshold {
                    return NoiseDecision {
                        is_noise: true,
                        category: Some(category.name.clone()),
                        confidence: similarity,
                    };
                }
                if similarity > 0.0 && best.as_ref().is_none_or(|(value, _)| similarity > *value) {
                    best = Some((similarity, category.name.clone()));
                }
            }
        }
        match best {
            Some((confidence, category)) => NoiseDecision {
                is_noise: false,
                category: Some(category),
                confidence,
            },
            None => NoiseDecision {
                is_noise: false,
                category: None,
                confidence: 0.0,
            },
        }
    }
}

/// Default semantic threshold when a category does not declare a usable one.
/// 类别未声明可用阈值时的默认语义阈值。
pub const DEFAULT_SEMANTIC_THRESHOLD: f64 = 0.88;

/// Configures Core-owned Noise admission without moving decisions into Host adapters.
/// 配置 Core 自有 Noise 准入，不将决策转移到 Host 适配器。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoisePolicy {
    /// Enables both deterministic and semantic Noise admission.
    /// 启用确定性与语义 Noise 准入。
    pub enabled: bool,
    /// Enables embeddings after deterministic rules pass; ignored when Noise is disabled.
    /// 在确定性规则通过后启用嵌入；Noise 禁用时忽略此开关。
    pub semantic_enabled: bool,
    /// Supplies the threshold for zero/omitted category thresholds; zero selects 0.88.
    /// 为零值或省略的类别阈值提供默认值；零值选择 0.88。
    pub default_semantic_threshold: f64,
}

impl Default for NoisePolicy {
    /// Enables the existing Core policy with VMM's default semantic threshold.
    /// 使用 VMM 默认语义阈值启用现有 Core 策略。
    fn default() -> Self {
        Self {
            enabled: true,
            semantic_enabled: true,
            default_semantic_threshold: DEFAULT_SEMANTIC_THRESHOLD,
        }
    }
}

/// Names one category phrase eligible for semantic prototype embedding.
/// 命名一个可用于语义原型嵌入的类别短语。
#[derive(Clone, Debug, PartialEq)]
pub struct NoisePhraseTarget {
    /// Names the owning category.
    /// 命名归属的类别。
    pub category: String,
    /// Carries the source phrase text.
    /// 携带来源短语文本。
    pub phrase: String,
    /// Carries the semantic decision threshold.
    /// 携带语义决策阈值。
    pub threshold: f64,
}

/// Carries one persisted semantic prototype row for scoring.
/// 携带一条供评分使用的持久语义原型行。
#[derive(Clone, Debug, PartialEq)]
pub struct NoisePrototypeRow {
    /// Names the owning category.
    /// 命名归属的类别。
    pub category: String,
    /// Carries the source phrase text.
    /// 携带来源短语文本。
    pub phrase: String,
    /// Carries the L2-normalized prototype vector.
    /// 携带 L2 规范化的原型向量。
    pub vector: Vec<f32>,
    /// Echoes the source threshold for diagnostics; the compiled snapshot owns decisions.
    /// 回显来源阈值供诊断；编译快照拥有决策权威。
    pub threshold: f64,
}

/// Sums float32 products in float64 source order; inputs have already passed Noise normalization.
/// 按来源顺序以双精度累加单精度乘积；输入已在噪声生命周期中完成归一化。
/// Returns no score for malformed input; valid zero vectors yield zero, without cosine recomputation.
/// 畸形输入无评分；合法零向量返回零，不重新计算余弦。
fn dot_similarity(left: &[f32], right: &[f32]) -> Option<f64> {
    if left.len() != right.len()
        || left.is_empty()
        || left.iter().chain(right).any(|value| !value.is_finite())
    {
        return None;
    }
    let mut dot = 0.0_f64;
    for (left, right) in left.iter().zip(right) {
        // Materialize the source float32 product before widening; FMA and clamping change admission.
        // 先产生来源单精度乘积再扩宽；融合乘加与钳位都会改变准入。
        dot += f64::from(*left * *right);
    }
    dot.is_finite().then_some(dot)
}

/// Describes one Noise JSON resource file.
/// 描述一个 Noise JSON 资源文件。
#[derive(Deserialize)]
struct NoiseFile {
    /// Carries the language identity.
    /// 携带语言标识。
    language: String,
    /// Carries the schema version.
    /// 携带 Schema 版本。
    version: String,
    /// Carries ordered category definitions.
    /// 携带有序类别定义。
    categories: Vec<CategoryDefinition>,
}

/// Describes one Noise category definition.
/// 描述一个 Noise 类别定义。
#[derive(Deserialize)]
struct CategoryDefinition {
    /// Carries the stable category name.
    /// 携带稳定类别名称。
    name: String,
    /// Carries applicable role names.
    /// 携带适用角色名称。
    #[serde(default)]
    targets: Vec<String>,
    /// Carries the acceptance threshold.
    /// 携带接受阈值。
    #[serde(default)]
    threshold: f64,
    /// Carries regex patterns.
    /// 携带正则模式。
    #[serde(default)]
    patterns: Vec<String>,
    /// Carries semantic prototype phrases.
    /// 携带语义原型短语。
    #[serde(default)]
    phrases: Vec<String>,
}

/// Holds one compiled Noise category.
/// 保存一个已编译 Noise 类别。
#[derive(Clone)]
struct CompiledCategory {
    /// Carries the stable category name.
    /// 携带稳定类别名称。
    name: String,
    /// Carries applicable roles.
    /// 携带适用角色。
    targets: BTreeSet<Role>,
    /// Carries the configured threshold.
    /// 携带配置阈值。
    threshold: f64,
    /// Carries compiled regex patterns.
    /// 携带已编译正则模式。
    patterns: Vec<Regex>,
    /// Carries whitespace-normalized, case-preserving prototype phrases.
    /// 携带规范空白且保留大小写的原型短语。
    phrases: Vec<String>,
}

impl CompiledCategory {
    /// Compiles and validates one complete category.
    /// 编译并校验一个完整类别。
    fn compile(definition: CategoryDefinition, default_threshold: f64) -> Result<Self, CoreError> {
        if definition.name.trim().is_empty() {
            return Err(noise_error("Noise category name must not be empty"));
        }
        // Source category overrides use nonpositive values as defaults and allow positive values above one.
        // 来源类别覆盖以非正数表示默认，并允许大于一的正数；全局策略仍单独校验。
        if !definition.threshold.is_finite() {
            return Err(noise_error(format!(
                "Noise category threshold is not finite: {}",
                definition.name
            )));
        }
        // Parse the exact role vocabulary used by authoritative resources.
        // 解析权威资源使用的精确角色词汇。
        let mut targets = definition
            .targets
            .into_iter()
            .map(|target| match target.trim().to_lowercase().as_str() {
                "user" => Ok(Role::User),
                "assistant" => Ok(Role::Assistant),
                other => Err(noise_error(format!(
                    "unsupported Noise target {other} in {}",
                    definition.name
                ))),
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if targets.is_empty() {
            targets.extend([Role::User, Role::Assistant]);
        }
        // Compile case-insensitive Unicode regexes once per snapshot.
        // 每个快照仅编译一次不区分大小写的 Unicode 正则。
        let patterns = definition
            .patterns
            .iter()
            .map(|pattern| pattern.trim())
            .filter(|pattern| !pattern.is_empty())
            .map(|pattern| {
                compile_source_regex(pattern, true).map_err(|error| {
                    noise_error(format!(
                        "invalid Noise regex in {}: {error}",
                        definition.name
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Match VMM's ASCII-regex whitespace normalization without changing case.
        // 对齐 VMM 的 ASCII 正则空白归一化，不改变大小写。
        // Duplicate normalized phrases share one category/phrase cache row.
        // 规范化后的重复短语共享一条类别/短语缓存行。
        let mut seen_phrases = BTreeSet::new();
        let phrases = definition
            .phrases
            .into_iter()
            .map(|phrase| {
                phrase
                    .trim()
                    .split([' ', '\t', '\n', '\r', '\u{000c}'])
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|phrase| !phrase.is_empty() && seen_phrases.insert(phrase.clone()))
            .collect();
        Ok(Self {
            name: definition.name.trim().to_owned(),
            targets,
            threshold: if definition.threshold > 0.0 {
                definition.threshold
            } else {
                default_threshold
            },
            patterns,
            phrases,
        })
    }
}

/// Creates one configuration-stage Noise error.
/// 创建一个配置阶段 Noise 错误。
fn noise_error(diagnostic: impl Into<String>) -> CoreError {
    CoreError::new(
        ErrorCode::InvalidConfiguration,
        ErrorStage::Configuration,
        diagnostic,
    )
}
