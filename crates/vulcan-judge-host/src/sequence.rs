//! Token sequences in the exact layout the judge model was trained on.
//! 与判定模型训练布局完全一致的词元序列。
//!
//! Layout: `[CLS]` policy head `[SEP]`, then for each criterion `[MASK]` and its text (the `[MASK]`
//! positions are the choice markers), `[SEP]`, the rendered call and a final `[SEP]`. Heads depend
//! only on the fixed option modes, so every combination is built once up front and caller text can
//! never change the instructions.
//! 布局：`[CLS]` 策略头 `[SEP]`，然后每条判据依次为 `[MASK]` 与其文本（`[MASK]` 位置即选项标记），
//! `[SEP]`，渲染后的调用，最后 `[SEP]`。策略头只取决于固定选项档位，因此所有组合预先构建一次，
//! 调用方文本永远无法改变指令。

use std::collections::{BTreeMap, HashMap};

use tokenizers::Tokenizer;
use vulcan_permission_judge::{JudgePolicy, JudgeSettings, RuntimeConfig};

/// Smallest instruction budget the head may keep after the criteria.
/// 扣除判据后策略头至少保留的指令预算。
const MIN_INSTRUCTION_TOKENS: usize = 16;

/// One encoded call.
/// 一条已编码调用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    /// Token ids of the complete sequence.
    /// 完整序列的词元标识。
    pub ids: Vec<i64>,
    /// Positions of the three choice markers.
    /// 三个选项标记的位置。
    pub markers: [i64; 3],
}

/// Encoding result for one call.
/// 单条调用的编码结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeOutcome {
    /// The call fits the model.
    /// 调用符合模型上限。
    Encoded(Encoded),
    /// The complete sequence exceeds the model limit.
    /// 完整序列超过模型上限。
    TooLong(usize),
}

/// A precomputed policy head with its marker positions.
/// 预先计算的策略头及其标记位置。
#[derive(Debug, Clone)]
struct Head {
    tokens: Vec<i64>,
    markers: [i64; 3],
}

/// Builds model input sequences for one package.
/// 为一个模型包构建模型输入序列。
pub struct Encoder {
    tokenizer: Tokenizer,
    sep: i64,
    /// Padding id used when batching.
    /// 批处理时使用的填充标识。
    pub pad: i64,
    max_tokens: usize,
    heads: HashMap<Vec<String>, Head>,
}

impl Encoder {
    /// Prepares every policy head; fails when the policy does not fit the trained head budget.
    /// 预备所有策略头；策略超出训练的策略头预算时失败。
    pub fn new(
        mut tokenizer: Tokenizer,
        policy: &JudgePolicy,
        config: &RuntimeConfig,
    ) -> Result<Self, String> {
        tokenizer
            .with_truncation(None)
            .map_err(|error| format!("Failed to disable tokenizer truncation: {error}"))?;
        tokenizer.with_padding(None);
        let id = |token: &str| {
            tokenizer
                .token_to_id(token)
                .map(i64::from)
                .ok_or_else(|| format!("The tokenizer lacks the `{token}` token."))
        };
        let (cls, sep, mask, pad) = (id("[CLS]")?, id("[SEP]")?, id("[MASK]")?, id("[PAD]")?);
        let mut encoder = Self {
            tokenizer,
            sep,
            pad,
            max_tokens: config.max_tokens as usize,
            heads: HashMap::new(),
        };
        let mut options = Vec::with_capacity(policy.criteria.len());
        for (label, text) in &policy.criteria {
            let mut option = vec![mask];
            option.extend(
                encoder
                    .tokens(&format!(" {label}: {text}"))?
                    .into_iter()
                    .take(config.option_tokens as usize),
            );
            options.push(option);
        }
        let option_tokens: usize = options.iter().map(Vec::len).sum();
        let budget = (config.head_tokens as usize)
            .checked_sub(option_tokens)
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
        for combination in mode_combinations(policy) {
            let settings = policy
                .exact_settings(&combination)
                .map_err(|error| error.to_string())?;
            let instructions = encoder.tokens(&format!(
                "choice question: {}",
                policy.instructions(&settings)
            ))?;
            let mut tokens = vec![cls];
            tokens.extend(instructions.into_iter().take(budget));
            tokens.push(sep);
            let mut markers = [0; 3];
            for (marker, option) in markers.iter_mut().zip(&options) {
                *marker = tokens.len() as i64;
                tokens.extend(option);
            }
            tokens.push(sep);
            encoder
                .heads
                .insert(head_key(&settings), Head { tokens, markers });
        }
        Ok(encoder)
    }

    /// Encodes one call; a sequence over the model limit is reported, never truncated.
    /// 编码一条调用；超过模型上限的序列如实报告，绝不截断。
    pub fn encode(
        &self,
        settings: &JudgeSettings,
        rendered: &str,
    ) -> Result<EncodeOutcome, String> {
        let head = self
            .heads
            .get(&head_key(settings))
            .ok_or("The settings do not match a prepared policy head.")?;
        let mut ids = head.tokens.clone();
        ids.extend(self.tokens(rendered)?);
        ids.push(self.sep);
        if ids.len() > self.max_tokens {
            return Ok(EncodeOutcome::TooLong(ids.len()));
        }
        Ok(EncodeOutcome::Encoded(Encoded {
            ids,
            markers: head.markers,
        }))
    }

    /// Tokenizes text without special tokens; a literal `[MASK]` becomes a space so text cannot
    /// forge a marker.
    /// 不加特殊词元地分词；字面 `[MASK]` 替换为空格，使文本无法伪造标记。
    fn tokens(&self, text: &str) -> Result<Vec<i64>, String> {
        self.tokenizer
            .encode(text.replace("[MASK]", " "), false)
            .map(|encoding| encoding.get_ids().iter().map(|id| i64::from(*id)).collect())
            .map_err(|error| format!("Tokenization failed: {error}"))
    }
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

    pub(crate) fn config(max_tokens: u32) -> RuntimeConfig {
        serde_json::from_value(serde_json::json!({
            "max_tokens": max_tokens, "head_tokens": 40, "option_tokens": 3, "max_request_items": 4,
            "micro_batch": 2, "allow_margin": 1.0, "labels": ["allow", "review", "block"],
            "input_fields": vulcan_permission_judge::package::INPUT_FIELDS,
            "model_file": "graph.onnx", "precision": "fp32", "gpu_memory_limit_mib": 1,
            "arena_trim_above_tokens": 512
        }))
        .unwrap()
    }

    #[test]
    fn markers_point_at_masks_and_modes_select_their_head() {
        let policy = policy();
        let encoder = Encoder::new(tokenizer(), &policy, &config(200)).unwrap();
        assert_eq!(encoder.heads.len(), 4);
        let mask = 4;
        let settings = policy.resolve_settings(&BTreeMap::new()).unwrap();
        let EncodeOutcome::Encoded(encoded) = encoder
            .encode(&settings, "Tool: shell [MASK] Arguments: {}")
            .unwrap()
        else {
            panic!("the call fits the limit");
        };
        assert_eq!(encoded.ids[0], 2, "sequences start with [CLS]");
        assert_eq!(encoded.ids.last(), Some(&3), "sequences end with [SEP]");
        for marker in encoded.markers {
            assert_eq!(encoded.ids[marker as usize], mask);
        }
        assert_eq!(
            encoded.ids.iter().filter(|id| **id == mask).count(),
            3,
            "caller text cannot add a marker"
        );

        let other = policy
            .resolve_settings(&BTreeMap::from([("reset".into(), "allow".into())]))
            .unwrap();
        let EncodeOutcome::Encoded(changed) = encoder.encode(&other, "Tool: shell").unwrap() else {
            panic!("the call fits the limit");
        };
        assert_ne!(
            changed.ids[..changed.markers[0] as usize],
            encoded.ids[..encoded.markers[0] as usize],
            "a different mode changes the instructions"
        );
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
        let mut tight = config(200);
        tight.head_tokens = 20;
        assert!(Encoder::new(tokenizer(), &policy, &tight).is_err());
    }
}
