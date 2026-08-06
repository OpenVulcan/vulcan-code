//! Provider-neutral request usage accounting.
//! 供应商无关的请求用量统计。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identifies how one usage value was obtained.
/// 标识一个用量值的取得方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageValueSource {
    /// Reported directly by the provider.
    /// 由供应商直接报告。
    Reported,
    /// Derived from other reported fields.
    /// 从其他已报告字段推导。
    Derived,
    /// Estimated locally.
    /// 在本地估算。
    Estimated,
}

/// Identifies how one usage event relates to earlier events in the same physical request.
/// 标识一条用量事件与同一物理请求内早期事件的关系。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageUpdateKind {
    /// Provider reports a current cumulative snapshot.
    /// 供应商报告当前累计快照。
    #[default]
    Snapshot,
    /// Provider reports a delta that must be added exactly once.
    /// 供应商报告必须精确累加一次的增量。
    Delta,
    /// Provider reports the authoritative terminal snapshot.
    /// 供应商报告权威终态快照。
    Final,
}

/// Represents a known or unknown token count.
/// 表示一个已知或未知的 Token 数量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UsageValue {
    /// Provider did not report enough information.
    /// 供应商未报告足够信息。
    #[default]
    Unknown,
    /// Known token count with provenance.
    /// 带来源的已知 Token 数量。
    Known {
        /// Token count.
        /// Token 数量。
        value: u64,
        /// Value provenance.
        /// 数值来源。
        source: UsageValueSource,
    },
}

/// Carries one immutable effective pricing revision without floating-point values.
/// 承载一份不使用浮点数的不可变生效价格修订。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricingProfileSnapshot {
    /// Content-addressed immutable revision.
    /// 内容寻址的不可变修订号。
    pub pricing_revision: String,
    /// Stable provider identity.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Optional stable service-entry identity.
    /// 可选稳定服务入口标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_entry_id: Option<String>,
    /// Exact model identity.
    /// 精确模型标识。
    pub model_id: String,
    /// ISO currency.
    /// ISO 币种。
    pub currency: String,
    /// Effective input price per million tokens.
    /// 生效输入每百万 Token 价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_per_million: Option<String>,
    /// Effective output price per million tokens.
    /// 生效输出每百万 Token 价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_per_million: Option<String>,
    /// Effective cache-read price per million tokens.
    /// 生效缓存读取每百万 Token 价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_per_million: Option<String>,
    /// Effective cache-write price per million tokens.
    /// 生效缓存写入每百万 Token 价格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_per_million: Option<String>,
    /// Stable source scope kind.
    /// 稳定来源作用域类型。
    pub source_kind: String,
    /// Exact source identity.
    /// 精确来源标识。
    pub source_id: String,
    /// Hexadecimal SHA-256 of the frozen revision material.
    /// 冻结修订材料的十六进制 SHA-256。
    pub content_hash: String,
}

/// Carries one provider-reported billing partition without collapsing it into token totals.
/// 承载一项供应商报告的计费分区，且不会把它折叠进 Token 合计。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageBillingDetail {
    /// Provider-defined billing category from one versioned extraction profile.
    /// 由版本化提取 Profile 约束的供应商计费类别。
    pub billing_type: String,
    /// Input tokens assigned to this billing category.
    /// 归入当前计费类别的输入 Token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    /// Output tokens assigned to this billing category.
    /// 归入当前计费类别的输出 Token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Total tokens assigned to this billing category.
    /// 归入当前计费类别的 Token 合计。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    /// Text input tokens reported inside this billing category.
    /// 当前计费类别中报告的文本输入 Token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_tokens: Option<u64>,
    /// Image input tokens reported inside this billing category.
    /// 当前计费类别中报告的图片输入 Token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_tokens: Option<u64>,
    /// Reasoning output tokens reported inside this billing category.
    /// 当前计费类别中报告的推理输出 Token。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
}

/// Carries normalized usage for one provider request.
/// 承载一次供应商请求的归一化用量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestUsage {
    /// Relationship of this update to earlier usage events.
    /// 当前更新与早期用量事件的关系。
    #[serde(default)]
    pub update_kind: UsageUpdateKind,
    /// Non-classified input tokens when reported.
    /// 已报告但未进一步分类的输入 Token。
    pub input_tokens: UsageValue,
    /// Input tokens not served from a provider cache.
    /// 未由供应商缓存提供的输入 Token。
    #[serde(default)]
    pub uncached_input_tokens: UsageValue,
    /// Output tokens.
    /// 输出 Token。
    pub output_tokens: UsageValue,
    /// Reasoning output tokens.
    /// 推理输出 Token。
    pub reasoning_tokens: UsageValue,
    /// Cache-read input tokens.
    /// 缓存读取输入 Token。
    pub cache_read_tokens: UsageValue,
    /// Cache-write input tokens.
    /// 缓存写入输入 Token。
    pub cache_write_tokens: UsageValue,
    /// Five-minute cache-write input tokens.
    /// 五分钟缓存写入输入 Token。
    pub cache_write_5m_tokens: UsageValue,
    /// One-hour cache-write input tokens.
    /// 一小时缓存写入输入 Token。
    pub cache_write_1h_tokens: UsageValue,
    /// Audio input tokens.
    /// 音频输入 Token。
    pub audio_input_tokens: UsageValue,
    /// Audio output tokens.
    /// 音频输出 Token。
    pub audio_output_tokens: UsageValue,
    /// Total tokens.
    /// 总 Token。
    pub total_tokens: UsageValue,
    /// Provider-reported decimal cost string.
    /// 供应商报告的十进制费用字符串。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<String>,
    /// Locally estimated decimal cost from one frozen pricing revision.
    /// 根据一份冻结价格修订在本地估算的十进制费用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost: Option<String>,
    /// ISO currency shared by reported or estimated cost facts.
    /// 报告或估算成本事实共用的 ISO 币种。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_currency: Option<String>,
    /// Immutable pricing revision used for the local estimate.
    /// 本地估算使用的不可变价格修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_revision: Option<String>,
    /// Frozen effective pricing profile used for the local estimate.
    /// 本地估算使用的冻结生效价格 Profile。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_profile: Option<PricingProfileSnapshot>,
    /// Provider-reported billing partitions retained separately from normalized token dimensions.
    /// 与规范化 Token 维度分开保留的供应商计费分区。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub billing_details: Vec<UsageBillingDetail>,
    /// Frozen extraction profile identity when raw provider evidence supplied this update.
    /// 原始供应商证据产生当前更新时所使用的冻结提取 Profile 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_profile_id: Option<String>,
    /// Frozen extraction profile revision.
    /// 冻结的提取 Profile 修订号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction_profile_revision: Option<u64>,
    /// Exact provider JSON Pointer retained for every reported dimension.
    /// 为每个已报告维度保留的精确供应商 JSON Pointer。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_paths: BTreeMap<String, String>,
    /// Validation findings retained without replacing provider evidence.
    /// 在不替换供应商证据的情况下保留的校验结果。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_flags: Vec<String>,
    /// Provider-returned model identity when reported with usage.
    /// 与用量一同报告时的供应商返回模型标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    /// Provider-returned response identity when reported with usage.
    /// 与用量一同报告时的供应商返回响应标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

impl RequestUsage {
    /// Merges one later partial update without erasing previously known fields.
    /// 合并一条后续局部更新，同时不抹除先前已知字段。
    ///
    /// # Parameters
    /// 参数
    /// `update` contains fields reported by a later event in the same physical request.
    /// `update` 包含同一物理请求中后续事件报告的字段。
    ///
    /// # Returns
    /// 返回值
    /// This method mutates the current request usage in place.
    /// 本方法会原地修改当前请求用量。
    pub fn merge_update(&mut self, update: Self) {
        let update_kind = update.update_kind;
        merge_usage_value(&mut self.input_tokens, update.input_tokens, update_kind);
        merge_usage_value(
            &mut self.uncached_input_tokens,
            update.uncached_input_tokens,
            update_kind,
        );
        merge_usage_value(&mut self.output_tokens, update.output_tokens, update_kind);
        merge_usage_value(
            &mut self.reasoning_tokens,
            update.reasoning_tokens,
            update_kind,
        );
        merge_usage_value(
            &mut self.cache_read_tokens,
            update.cache_read_tokens,
            update_kind,
        );
        merge_usage_value(
            &mut self.cache_write_tokens,
            update.cache_write_tokens,
            update_kind,
        );
        merge_usage_value(
            &mut self.cache_write_5m_tokens,
            update.cache_write_5m_tokens,
            update_kind,
        );
        merge_usage_value(
            &mut self.cache_write_1h_tokens,
            update.cache_write_1h_tokens,
            update_kind,
        );
        merge_usage_value(
            &mut self.audio_input_tokens,
            update.audio_input_tokens,
            update_kind,
        );
        merge_usage_value(
            &mut self.audio_output_tokens,
            update.audio_output_tokens,
            update_kind,
        );
        merge_usage_value(&mut self.total_tokens, update.total_tokens, update_kind);
        if update.cost.is_some() {
            self.cost = update.cost;
        }
        if update.estimated_cost.is_some() {
            self.estimated_cost = update.estimated_cost;
        }
        if update.cost_currency.is_some() {
            self.cost_currency = update.cost_currency;
        }
        if update.pricing_revision.is_some() {
            self.pricing_revision = update.pricing_revision;
        }
        if update.pricing_profile.is_some() {
            self.pricing_profile = update.pricing_profile;
        }
        if !update.billing_details.is_empty() {
            self.billing_details = update.billing_details;
        }
        if update.extraction_profile_id.is_some() {
            self.extraction_profile_id = update.extraction_profile_id;
            self.extraction_profile_revision = update.extraction_profile_revision;
        }
        self.source_paths.extend(update.source_paths);
        self.validation_flags.extend(update.validation_flags);
        if update.response_model.is_some() {
            self.response_model = update.response_model;
        }
        if update.response_id.is_some() {
            self.response_id = update.response_id;
        }
        if matches!(update_kind, UsageUpdateKind::Final) {
            self.update_kind = UsageUpdateKind::Final;
        } else if !matches!(self.update_kind, UsageUpdateKind::Final) {
            self.update_kind = UsageUpdateKind::Snapshot;
        }
    }
}

/// Derives an uncached input count from a provider-reported total and cache-read count.
/// 根据供应商报告的输入总量与缓存读取量推导非缓存输入量。
///
/// # Parameters
/// 参数
/// `input` is the provider-reported total input and `cached` is the cache-hit subset.
/// `input` 是供应商报告的输入总量，`cached` 是其中的缓存命中子集。
///
/// # Returns
/// 返回值
/// A derived count when both values are known and consistent; otherwise unknown.
/// 两个数值均已知且关系一致时返回推导值，否则返回未知。
pub fn derive_uncached_input(input: UsageValue, cached: UsageValue) -> UsageValue {
    match (input, cached) {
        (
            UsageValue::Known {
                value: input_value, ..
            },
            UsageValue::Known {
                value: cached_value,
                ..
            },
        ) if cached_value <= input_value => UsageValue::Known {
            value: input_value - cached_value,
            source: UsageValueSource::Derived,
        },
        _ => UsageValue::Unknown,
    }
}

/// Derives one combined usage value from two disjoint provider-reported counts.
/// 根据两个互不重叠的供应商报告计数推导合计用量。
///
/// # Parameters
/// 参数
/// `left` and `right` are disjoint token counts.
/// `left` 与 `right` 是互不重叠的 Token 计数。
///
/// # Returns
/// 返回值
/// A checked derived sum when both values are known; otherwise unknown.
/// 两个数值均已知时返回经过溢出检查的推导和，否则返回未知。
pub fn derive_usage_sum(left: UsageValue, right: UsageValue) -> UsageValue {
    match (left, right) {
        (
            UsageValue::Known {
                value: left_value, ..
            },
            UsageValue::Known {
                value: right_value, ..
            },
        ) => left_value
            .checked_add(right_value)
            .map(|value| UsageValue::Known {
                value,
                source: UsageValueSource::Derived,
            })
            .unwrap_or(UsageValue::Unknown),
        _ => UsageValue::Unknown,
    }
}

/// Replaces one usage field only when a later event reports a known value.
/// 仅当后续事件报告已知值时替换一个用量字段。
///
/// # Parameters
/// 参数
/// `current` is the accumulated field and `update` is the later partial field.
/// `current` 是累计字段，`update` 是后续局部字段。
fn merge_usage_value(current: &mut UsageValue, update: UsageValue, update_kind: UsageUpdateKind) {
    if matches!(update, UsageValue::Unknown) {
        return;
    }
    if !matches!(update_kind, UsageUpdateKind::Delta) {
        *current = update;
        return;
    }
    match (*current, update) {
        (
            UsageValue::Known {
                value: current_value,
                source: current_source,
            },
            UsageValue::Known {
                value: update_value,
                source: update_source,
            },
        ) => {
            *current = current_value
                .checked_add(update_value)
                .map(|value| UsageValue::Known {
                    value,
                    source: combined_usage_source(current_source, update_source),
                })
                .unwrap_or(UsageValue::Unknown);
        }
        (UsageValue::Unknown, update) => *current = update,
        _ => {}
    }
}

/// Combines provenance when two exact delta values are accumulated.
/// 在累加两个精确增量值时合并其来源。
///
/// # Parameters
/// 参数
/// `left` and `right` are the two contributing value sources.
/// `left` 与 `right` 是两个贡献值来源。
///
/// # Returns
/// 返回值
/// The least authoritative source required to describe the sum.
/// 返回描述总和所需的最低权威来源。
fn combined_usage_source(left: UsageValueSource, right: UsageValueSource) -> UsageValueSource {
    if matches!(left, UsageValueSource::Estimated) || matches!(right, UsageValueSource::Estimated) {
        UsageValueSource::Estimated
    } else if matches!(left, UsageValueSource::Derived)
        || matches!(right, UsageValueSource::Derived)
    {
        UsageValueSource::Derived
    } else {
        UsageValueSource::Reported
    }
}

/// Carries one provider-authoritative count of an input request.
/// 承载一次由供应商权威统计的输入请求 Token 数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputTokenCount {
    /// Total input tokens counted by the provider.
    /// 供应商统计的输入 Token 总数。
    pub total_tokens: u64,
    /// Tokens attributed to cached input when the provider reports them.
    /// 供应商报告时归属于缓存输入的 Token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    /// Per-modality token counts for the full input.
    /// 完整输入按模态拆分的 Token 数。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modality_details: Vec<ModalityTokenCount>,
    /// Per-modality token counts for cached input.
    /// 缓存输入按模态拆分的 Token 数。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cached_modality_details: Vec<ModalityTokenCount>,
}

/// Carries one provider-reported modality token count without narrowing future modality names.
/// 承载一个供应商报告的模态 Token 数，并保留未来模态名称的扩展能力。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModalityTokenCount {
    /// Exact provider modality identifier.
    /// 供应商返回的精确模态标识。
    pub modality: String,
    /// Token count attributed to this modality.
    /// 归属于该模态的 Token 数。
    pub token_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies partial events retain known cache fields while adding terminal output fields.
    /// 验证局部事件在补充终态输出字段时会保留已知缓存字段。
    #[test]
    fn partial_updates_preserve_known_fields() {
        let reported = |value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        };
        let mut usage = RequestUsage {
            input_tokens: reported(1_000),
            uncached_input_tokens: reported(200),
            cache_read_tokens: reported(800),
            cache_write_tokens: reported(120),
            ..RequestUsage::default()
        };
        usage.merge_update(RequestUsage {
            output_tokens: reported(75),
            total_tokens: reported(1_075),
            ..RequestUsage::default()
        });

        assert_eq!(usage.input_tokens, reported(1_000));
        assert_eq!(usage.uncached_input_tokens, reported(200));
        assert_eq!(usage.cache_read_tokens, reported(800));
        assert_eq!(usage.cache_write_tokens, reported(120));
        assert_eq!(usage.output_tokens, reported(75));
        assert_eq!(usage.total_tokens, reported(1_075));
    }

    /// Verifies delta updates add while a final snapshot authoritatively replaces values.
    /// 验证增量更新执行累加，而终态快照会权威替换数值。
    #[test]
    fn delta_and_final_updates_have_distinct_merge_semantics() {
        let reported = |value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        };
        let mut usage = RequestUsage::default();
        usage.merge_update(RequestUsage {
            update_kind: UsageUpdateKind::Delta,
            output_tokens: reported(4),
            ..RequestUsage::default()
        });
        usage.merge_update(RequestUsage {
            update_kind: UsageUpdateKind::Delta,
            output_tokens: reported(6),
            ..RequestUsage::default()
        });
        assert_eq!(usage.output_tokens, reported(10));

        usage.merge_update(RequestUsage {
            update_kind: UsageUpdateKind::Final,
            output_tokens: reported(9),
            ..RequestUsage::default()
        });
        assert_eq!(usage.output_tokens, reported(9));
        assert_eq!(usage.update_kind, UsageUpdateKind::Final);
    }

    /// Verifies overflowing deltas become unknown instead of silently saturating.
    /// 验证溢出的增量会变为未知，而不是静默饱和。
    #[test]
    fn overflowing_delta_becomes_unknown() {
        let reported = |value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        };
        let mut usage = RequestUsage {
            output_tokens: reported(u64::MAX),
            ..RequestUsage::default()
        };

        usage.merge_update(RequestUsage {
            update_kind: UsageUpdateKind::Delta,
            output_tokens: reported(1),
            ..RequestUsage::default()
        });

        assert_eq!(usage.output_tokens, UsageValue::Unknown);
    }

    /// Verifies historical usage JSON without uncached input remains readable.
    /// 验证缺少非缓存输入字段的历史用量 JSON 仍可读取。
    #[test]
    fn historical_usage_without_uncached_input_defaults_to_unknown() {
        let usage = serde_json::from_value::<RequestUsage>(serde_json::json!({
            "input_tokens": {"state": "known", "value": 10, "source": "reported"},
            "output_tokens": {"state": "known", "value": 2, "source": "reported"},
            "reasoning_tokens": {"state": "unknown"},
            "cache_read_tokens": {"state": "unknown"},
            "cache_write_tokens": {"state": "unknown"},
            "cache_write_5m_tokens": {"state": "unknown"},
            "cache_write_1h_tokens": {"state": "unknown"},
            "audio_input_tokens": {"state": "unknown"},
            "audio_output_tokens": {"state": "unknown"},
            "total_tokens": {"state": "known", "value": 12, "source": "reported"}
        }))
        .expect("historical usage should deserialize");

        assert_eq!(usage.uncached_input_tokens, UsageValue::Unknown);
    }

    /// Verifies uncached derivation preserves explicit zero and rejects inconsistent totals.
    /// 验证非缓存推导保留显式零值并拒绝不一致的总量。
    #[test]
    fn uncached_derivation_distinguishes_zero_unknown_and_invalid_values() {
        let reported = |value| UsageValue::Known {
            value,
            source: UsageValueSource::Reported,
        };
        assert_eq!(
            derive_uncached_input(reported(100), reported(100)),
            UsageValue::Known {
                value: 0,
                source: UsageValueSource::Derived,
            }
        );
        assert_eq!(
            derive_uncached_input(reported(100), UsageValue::Unknown),
            UsageValue::Unknown
        );
        assert_eq!(
            derive_uncached_input(reported(100), reported(101)),
            UsageValue::Unknown
        );
    }
}
