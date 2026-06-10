//! Provider-neutral cache hint policy helpers.
//! Provider 中立的缓存提示策略辅助逻辑。

/// Maximum cache breakpoints accepted by Anthropic Messages.
/// Anthropic Messages 接受的最大缓存断点数量。
pub(crate) const ANTHROPIC_MAX_CACHE_BREAKPOINTS: usize = 4;

/// Provider cache-hint support profile used by adapter-specific lowering.
/// 供适配器特定转换使用的 Provider 缓存提示支持配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderCacheHintSupport {
    /// Provider does not support cache hints.
    /// Provider 不支持缓存提示。
    Unsupported,
    /// Provider supports Anthropic-style ephemeral cache-control breakpoints.
    /// Provider 支持 Anthropic 风格的 ephemeral cache-control 断点。
    AnthropicEphemeral { max_breakpoints: usize },
}

/// Provider-neutral target where one cache hint may be attached.
/// 可附加单个缓存提示的 Provider 中立目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderCacheHintTarget {
    /// Stable system prompt or system block.
    /// 稳定的系统提示词或系统块。
    System,
    /// Latest user-authored prompt block.
    /// 最新用户输入提示块。
    LastUser,
    /// Latest tool definition block.
    /// 最新工具定义块。
    LastToolDefinition,
    /// Latest tool-result block.
    /// 最新工具结果块。
    LastToolResult,
}

/// One provider-neutral cache breakpoint candidate.
/// 单个 Provider 中立缓存断点候选。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderCacheBreakpoint {
    /// Target block that should receive the cache hint if selected.
    /// 若被选中则应接收缓存提示的目标块。
    pub target: ProviderCacheHintTarget,
    /// Larger priority values are selected first when provider limits apply.
    /// 当 Provider 存在数量限制时，优先选择较大的优先级值。
    pub priority: u8,
}

/// Result of lowering provider-neutral cache hints for one adapter.
/// 单个适配器转换 Provider 中立缓存提示后的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderCachePolicyLowering {
    /// Selected cache breakpoints that the provider can represent.
    /// Provider 可以表达的已选缓存断点。
    pub selected: Vec<ProviderCacheBreakpoint>,
    /// Diagnostic reason emitted when hints had to be ignored or truncated.
    /// 缓存提示被忽略或截断时发出的诊断原因。
    pub diagnostic: Option<String>,
}

/// Selects cache breakpoints according to provider support and stable priority ordering.
/// 根据 Provider 支持能力与稳定优先级顺序选择缓存断点。
pub(crate) fn lower_provider_cache_policy(
    candidates: &[ProviderCacheBreakpoint],
    support: ProviderCacheHintSupport,
) -> ProviderCachePolicyLowering {
    match support {
        ProviderCacheHintSupport::Unsupported => ProviderCachePolicyLowering {
            selected: Vec::new(),
            diagnostic: (!candidates.is_empty()).then(|| String::from("cache_hints_unsupported")),
        },
        ProviderCacheHintSupport::AnthropicEphemeral { max_breakpoints } => {
            let mut selected = candidates.to_vec();
            selected.sort_by(|left, right| {
                right.priority.cmp(&left.priority).then_with(|| {
                    cache_target_order(left.target).cmp(&cache_target_order(right.target))
                })
            });
            let truncated = selected.len() > max_breakpoints;
            selected.truncate(max_breakpoints);
            ProviderCachePolicyLowering {
                selected,
                diagnostic: truncated.then(|| String::from("cache_hints_truncated")),
            }
        }
    }
}

/// Returns the stable tie-break order for equal-priority cache targets.
/// 返回同优先级缓存目标的稳定决胜顺序。
fn cache_target_order(target: ProviderCacheHintTarget) -> u8 {
    match target {
        ProviderCacheHintTarget::System => 0,
        ProviderCacheHintTarget::LastToolDefinition => 1,
        ProviderCacheHintTarget::LastToolResult => 2,
        ProviderCacheHintTarget::LastUser => 3,
    }
}

/// Builds the default automatic cache candidates used by providers that support cache hints.
/// 构建支持缓存提示的 Provider 使用的默认自动缓存候选。
pub(crate) fn default_provider_cache_candidates() -> Vec<ProviderCacheBreakpoint> {
    vec![
        ProviderCacheBreakpoint {
            target: ProviderCacheHintTarget::System,
            priority: 100,
        },
        ProviderCacheBreakpoint {
            target: ProviderCacheHintTarget::LastToolResult,
            priority: 80,
        },
        ProviderCacheBreakpoint {
            target: ProviderCacheHintTarget::LastToolDefinition,
            priority: 90,
        },
        ProviderCacheBreakpoint {
            target: ProviderCacheHintTarget::LastUser,
            priority: 60,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        ANTHROPIC_MAX_CACHE_BREAKPOINTS, ProviderCacheBreakpoint, ProviderCacheHintSupport,
        ProviderCacheHintTarget, default_provider_cache_candidates, lower_provider_cache_policy,
    };

    /// Verifies unsupported providers ignore cache hints with a stable diagnostic reason.
    /// 验证不支持缓存提示的 Provider 会忽略提示并返回稳定诊断原因。
    #[test]
    fn unsupported_provider_drops_cache_hints_with_diagnostic() {
        let lowered = lower_provider_cache_policy(
            &default_provider_cache_candidates(),
            ProviderCacheHintSupport::Unsupported,
        );

        assert!(lowered.selected.is_empty());
        assert_eq!(
            lowered.diagnostic.as_deref(),
            Some("cache_hints_unsupported")
        );
    }

    /// Verifies Anthropic cache lowering keeps the highest-priority candidates within the provider limit.
    /// 验证 Anthropic 缓存转换会在 Provider 限制内保留最高优先级候选。
    #[test]
    fn anthropic_cache_lowering_respects_breakpoint_limit() {
        let candidates = vec![
            ProviderCacheBreakpoint {
                target: ProviderCacheHintTarget::LastUser,
                priority: 1,
            },
            ProviderCacheBreakpoint {
                target: ProviderCacheHintTarget::System,
                priority: 5,
            },
            ProviderCacheBreakpoint {
                target: ProviderCacheHintTarget::LastToolResult,
                priority: 3,
            },
            ProviderCacheBreakpoint {
                target: ProviderCacheHintTarget::LastUser,
                priority: 9,
            },
            ProviderCacheBreakpoint {
                target: ProviderCacheHintTarget::System,
                priority: 7,
            },
        ];

        let lowered = lower_provider_cache_policy(
            &candidates,
            ProviderCacheHintSupport::AnthropicEphemeral {
                max_breakpoints: ANTHROPIC_MAX_CACHE_BREAKPOINTS,
            },
        );

        assert_eq!(lowered.selected.len(), ANTHROPIC_MAX_CACHE_BREAKPOINTS);
        assert_eq!(lowered.diagnostic.as_deref(), Some("cache_hints_truncated"));
        assert_eq!(lowered.selected[0].priority, 9);
        assert_eq!(lowered.selected[1].priority, 7);
    }
}
