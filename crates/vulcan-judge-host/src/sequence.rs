//! Token sequences in the exact layout the judge model was trained on.
//! 与判定模型训练布局完全一致的词元序列。
//!
//! The package declares one of two encodings. `marker-head`: `[CLS]` policy head `[SEP]`, then for
//! each criterion `[MASK]` and its text (the graph reads the three `[MASK]` positions), `[SEP]`, the
//! rendered call and a final `[SEP]`; heads depend only on the fixed option modes, so every
//! combination is built once up front and caller text can never change the instructions.
//! `decision-prompt`: the English decision prompt of the policy, tokenized whole without special
//! tokens, because byte-pair merges across the call boundary must match training.
//! 模型包声明两种编码之一。`marker-head`：`[CLS]` 策略头 `[SEP]`，然后每条判据依次为 `[MASK]` 与其
//! 文本（计算图读取三个 `[MASK]` 位置），`[SEP]`，渲染后的调用，最后 `[SEP]`；策略头只取决于固定
//! 选项档位，因此所有组合预先构建一次，调用方文本永远无法改变指令。`decision-prompt`：策略的英文
//! 判定提示，整体分词且不加特殊词元，因为跨越调用边界的字节对合并必须与训练一致。

use std::collections::{BTreeMap, HashMap};

use tokenizers::Tokenizer;
use vulcan_permission_judge::{Encoding, JudgePolicy, JudgeSettings, RuntimeConfig};

/// Smallest instruction budget the head may keep after the criteria.
/// 扣除判据后策略头至少保留的指令预算。
const MIN_INSTRUCTION_TOKENS: usize = 16;

/// Encoding result for one call.
/// 单条调用的编码结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeOutcome {
    /// Token ids of the complete sequence, which fits the model.
    /// 符合模型上限的完整序列词元标识。
    Encoded(Vec<i64>),
    /// The complete sequence exceeds the model limit.
    /// 完整序列超过模型上限。
    TooLong(usize),
}

/// Encoding-specific state.
/// 与编码相关的状态。
enum Layout {
    /// Precomputed heads by mode choice, and the separator closing each sequence.
    /// 按档位选择预先计算的策略头，以及结束每个序列的分隔符。
    MarkerHead {
        sep: i64,
        heads: HashMap<Vec<String>, Vec<i64>>,
    },
    /// The policy whose decision prompt wraps each call.
    /// 其判定提示包裹每条调用的策略。
    DecisionPrompt { policy: JudgePolicy },
}

/// Builds model input sequences for one package.
/// 为一个模型包构建模型输入序列。
pub struct Encoder {
    tokenizer: Tokenizer,
    /// Padding id used when batching.
    /// 批处理时使用的填充标识。
    pub pad: i64,
    max_tokens: usize,
    layout: Layout,
}

impl Encoder {
    /// Prepares the declared encoding; fails when the policy does not fit the trained head budget
    /// or the tokenizer lacks a token the encoding needs.
    /// 预备声明的编码；策略超出训练的策略头预算或分词器缺少编码所需词元时失败。
    pub fn new(
        mut tokenizer: Tokenizer,
        policy: &JudgePolicy,
        config: &RuntimeConfig,
    ) -> Result<Self, String> {
        tokenizer
            .with_truncation(None)
            .map_err(|error| format!("Failed to disable tokenizer truncation: {error}"))?;
        tokenizer.with_padding(None);
        let max_tokens = config.max_tokens as usize;
        match config.encoding {
            Encoding::MarkerHead {
                head_tokens,
                option_tokens,
            } => marker_head(
                tokenizer,
                policy,
                max_tokens,
                head_tokens as usize,
                option_tokens as usize,
            ),
            Encoding::DecisionPrompt { pad_token_id } => Ok(Self {
                tokenizer,
                pad: pad_token_id,
                max_tokens,
                layout: Layout::DecisionPrompt {
                    policy: policy.clone(),
                },
            }),
        }
    }

    /// Encodes one call; a sequence over the model limit is reported, never truncated.
    /// 编码一条调用；超过模型上限的序列如实报告，绝不截断。
    pub fn encode(
        &self,
        settings: &JudgeSettings,
        rendered: &str,
    ) -> Result<EncodeOutcome, String> {
        let ids = match &self.layout {
            Layout::MarkerHead { sep, heads } => {
                let head = heads
                    .get(&head_key(settings))
                    .ok_or("The settings do not match a prepared policy head.")?;
                let mut ids = head.clone();
                ids.extend(self.marker_free_tokens(rendered)?);
                ids.push(*sep);
                ids
            }
            Layout::DecisionPrompt { policy } => {
                self.tokens(&policy.decision_prompt(settings, rendered))?
            }
        };
        if ids.len() > self.max_tokens {
            return Ok(EncodeOutcome::TooLong(ids.len()));
        }
        Ok(EncodeOutcome::Encoded(ids))
    }

    /// Tokenizes text without special tokens.
    /// 不加特殊词元地分词。
    fn tokens(&self, text: &str) -> Result<Vec<i64>, String> {
        self.tokenizer
            .encode(text, false)
            .map(|encoding| encoding.get_ids().iter().map(|id| i64::from(*id)).collect())
            .map_err(|error| format!("Tokenization failed: {error}"))
    }

    /// Tokenizes text with a literal `[MASK]` turned into a space, so text cannot forge a marker.
    /// 分词时将字面 `[MASK]` 替换为空格，使文本无法伪造标记。
    fn marker_free_tokens(&self, text: &str) -> Result<Vec<i64>, String> {
        self.tokens(&text.replace("[MASK]", " "))
    }
}

/// Builds the `marker-head` encoder with every policy head prepared.
/// 构建预备好全部策略头的 `marker-head` 编码器。
fn marker_head(
    tokenizer: Tokenizer,
    policy: &JudgePolicy,
    max_tokens: usize,
    head_tokens: usize,
    option_tokens: usize,
) -> Result<Encoder, String> {
    let id = |token: &str| {
        tokenizer
            .token_to_id(token)
            .map(i64::from)
            .ok_or_else(|| format!("The tokenizer lacks the `{token}` token."))
    };
    let (cls, sep, mask, pad) = (id("[CLS]")?, id("[SEP]")?, id("[MASK]")?, id("[PAD]")?);
    let mut encoder = Encoder {
        tokenizer,
        pad,
        max_tokens,
        layout: Layout::MarkerHead {
            sep,
            heads: HashMap::new(),
        },
    };
    let mut options = Vec::with_capacity(policy.criteria.len());
    for (label, text) in &policy.criteria {
        let mut option = vec![mask];
        option.extend(
            encoder
                .marker_free_tokens(&format!(" {label}: {text}"))?
                .into_iter()
                .take(option_tokens),
        );
        options.push(option);
    }
    let used: usize = options.iter().map(Vec::len).sum();
    let budget = head_tokens
        .checked_sub(used)
        .filter(|budget| *budget >= MIN_INSTRUCTION_TOKENS)
        .ok_or("The policy criteria exceed the trained head budget.")?;
    let distinct = options
        .iter()
        .enumerate()
        .all(|(index, option)| !options[..index].contains(option));
    if options.len() != 3 || !distinct {
        return Err(String::from(
            "The policy criteria do not form three distinct choices.",
        ));
    }
    let mut heads = HashMap::new();
    for combination in mode_combinations(policy) {
        let settings = policy
            .exact_settings(&combination)
            .map_err(|error| error.to_string())?;
        let instructions = encoder.marker_free_tokens(&format!(
            "choice question: {}",
            policy.instructions(&settings)
        ))?;
        let mut tokens = vec![cls];
        tokens.extend(instructions.into_iter().take(budget));
        tokens.push(sep);
        for option in &options {
            tokens.extend(option);
        }
        tokens.push(sep);
        heads.insert(head_key(&settings), tokens);
    }
    encoder.layout = Layout::MarkerHead { sep, heads };
    Ok(encoder)
}

/// Head lookup key: the chosen modes in policy order.
/// 策略头查找键：按策略顺序排列的已选档位。
fn head_key(settings: &JudgeSettings) -> Vec<String> {
    settings.modes().values().cloned().collect()
}

/// Every complete mode choice of the policy.
/// 策略的全部完整档位选择。
fn mode_combinations(policy: &JudgePolicy) -> Vec<BTreeMap<String, String>> {
    let mut combinations = vec![BTreeMap::new()];
    for (key, option) in &policy.options {
        combinations = combinations
            .into_iter()
            .flat_map(|partial| {
                option.modes.keys().map(move |mode| {
                    let mut next = partial.clone();
                    next.insert(key.clone(), mode.clone());
                    next
                })
            })
            .collect();
    }
    combinations
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tokenizers::models::wordlevel::WordLevel;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    /// Policy with two options whose sentences reuse a small vocabulary.
    /// 两个选项的策略，其句子复用一个小词表。
    const POLICY: &str = r#"{
        "version": "test-v1",
        "core_instructions": "judge the call",
        "criteria": {"allow": "ordinary work", "review": "needs confirmation", "block": "extreme damage"},
        "options": {
            "writes": {"default": "allow", "modes": {"allow": "allow outside writes", "review": "review outside writes"}},
            "reset": {"default": "confirm", "modes": {"confirm": "review project reset", "allow": "allow project reset"}}
        }
    }"#;

    /// Mask token id of the test vocabulary.
    /// 测试词表的掩码词元标识。
    const MASK: i64 = 4;

    /// Word-level tokenizer over the test vocabulary; unknown words map to `[UNK]`.
    /// 基于测试词表的词级分词器；未知词映射为 `[UNK]`。
    pub(crate) fn tokenizer() -> Tokenizer {
        let words = [
            "[PAD]",
            "[UNK]",
            "[CLS]",
            "[SEP]",
            "[MASK]",
            "choice",
            "question",
            ":",
            "judge",
            "the",
            "call",
            "allow",
            "review",
            "block",
            "ordinary",
            "work",
            "needs",
            "confirmation",
            "extreme",
            "damage",
            "outside",
            "writes",
            "project",
            "reset",
            "Tool",
            "Arguments",
        ];
        let vocab: HashMap<String, u32> = words
            .iter()
            .enumerate()
            .map(|(index, word)| (word.to_string(), index as u32))
            .collect();
        let model = WordLevel::builder()
            .vocab(vocab.into_iter().collect())
            .unk_token("[UNK]".into())
            .build()
            .unwrap();
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace {}));
        tokenizer
    }

    pub(crate) fn policy() -> JudgePolicy {
        serde_json::from_str(POLICY).unwrap()
    }

    /// Runtime config with the given limit and encoding.
    /// 带给定上限与编码的运行配置。
    pub(crate) fn config_with(max_tokens: u32, encoding: serde_json::Value) -> RuntimeConfig {
        serde_json::from_value(serde_json::json!({
            "family": "test", "max_tokens": max_tokens, "max_request_items": 4, "micro_batch": 2,
            "padded_batch_tokens": 100, "allow_margin": 1.0, "labels": ["allow", "review", "block"],
            "input_fields": vulcan_permission_judge::package::INPUT_FIELDS,
            "output_field": vulcan_permission_judge::package::OUTPUT_FIELD,
            "model_file": "graph.onnx", "precision": "fp32", "devices": ["cpu"],
            "gpu_memory_limit_mib": 1, "arena_trim_above_tokens": 512, "encoding": encoding
        }))
        .unwrap()
    }

    /// Marker-head runtime config with the given limit.
    /// 带给定上限的 marker-head 运行配置。
    pub(crate) fn config(max_tokens: u32) -> RuntimeConfig {
        config_with(
            max_tokens,
            serde_json::json!({"kind": "marker-head", "head_tokens": 40, "option_tokens": 3}),
        )
    }

    fn encoded(outcome: EncodeOutcome) -> Vec<i64> {
        match outcome {
            EncodeOutcome::Encoded(ids) => ids,
            EncodeOutcome::TooLong(length) => panic!("the call fits the limit, got {length}"),
        }
    }

    #[test]
    fn marker_heads_hold_three_masks_and_modes_select_their_head() {
        let policy = policy();
        let encoder = Encoder::new(tokenizer(), &policy, &config(200)).unwrap();
        let Layout::MarkerHead { heads, .. } = &encoder.layout else {
            panic!("the config declares a marker head");
        };
        assert_eq!(heads.len(), 4);
        let settings = policy.resolve_settings(&BTreeMap::new()).unwrap();
        let ids = encoded(
            encoder
                .encode(&settings, "Tool: shell [MASK] Arguments: {}")
                .unwrap(),
        );
        assert_eq!(ids[0], 2, "sequences start with [CLS]");
        assert_eq!(ids.last(), Some(&3), "sequences end with [SEP]");
        assert_eq!(
            ids.iter().filter(|id| **id == MASK).count(),
            3,
            "caller text cannot add a marker"
        );

        let other = policy
            .resolve_settings(&BTreeMap::from([("reset".into(), "allow".into())]))
            .unwrap();
        let changed = encoded(encoder.encode(&other, "Tool: shell").unwrap());
        let first_mask = |ids: &[i64]| ids.iter().position(|id| *id == MASK).unwrap();
        assert_ne!(
            changed[..first_mask(&changed)],
            ids[..first_mask(&ids)],
            "a different mode changes the instructions"
        );
    }

    #[test]
    fn decision_prompts_tokenize_the_policy_prompt_and_pad_with_the_declared_id() {
        let policy = policy();
        let config = config_with(
            200,
            serde_json::json!({"kind": "decision-prompt", "pad_token_id": 0}),
        );
        let encoder = Encoder::new(tokenizer(), &policy, &config).unwrap();
        assert_eq!(encoder.pad, 0);
        let settings = policy.resolve_settings(&BTreeMap::new()).unwrap();
        let ids = encoded(encoder.encode(&settings, "Tool: shell").unwrap());
        let expected = encoder
            .tokens(&policy.decision_prompt(&settings, "Tool: shell"))
            .unwrap();
        assert_eq!(ids, expected);
        assert!(!ids.contains(&2) && !ids.contains(&3), "no special tokens");
    }

    #[test]
    fn long_calls_are_reported_and_oversized_criteria_refused() {
        let policy = policy();
        let encoder = Encoder::new(tokenizer(), &policy, &config(40)).unwrap();
        let settings = policy.resolve_settings(&BTreeMap::new()).unwrap();
        let long = "work ".repeat(60);
        assert!(matches!(
            encoder.encode(&settings, &long).unwrap(),
            EncodeOutcome::TooLong(length) if length > 40
        ));
        let tight = config_with(
            200,
            serde_json::json!({"kind": "marker-head", "head_tokens": 20, "option_tokens": 3}),
        );
        assert!(Encoder::new(tokenizer(), &policy, &tight).is_err());
    }
}
