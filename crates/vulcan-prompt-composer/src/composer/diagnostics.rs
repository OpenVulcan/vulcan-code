use crate::cache::report::PromptCacheReport;

/// One cache event recorded during composition.
/// 组合过程中记录的一次缓存事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCacheEvent {
    /// Cache scope label.
    /// 缓存作用域标签。
    pub scope: String,
    /// Cache key.
    /// 缓存键。
    pub cache_key: String,
    /// Whether the event was a hit.
    /// 此事件是否为命中。
    pub hit: bool,
}

/// Diagnostics returned by one composition run.
/// 一次组合执行返回的诊断信息。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptComposerDiagnostics {
    /// Selected strategy identifier.
    /// 选中的策略标识。
    pub strategy_id: String,
    /// Selected personality identifier when any.
    /// 选中的人格标识。
    pub personality_id: Option<String>,
    /// Cache report counters.
    /// 缓存统计计数。
    pub cache_report: PromptCacheReport,
    /// Detailed cache events.
    /// 详细缓存事件。
    pub cache_events: Vec<PromptCacheEvent>,
}

impl PromptComposerDiagnostics {
    /// Record one node-cache event.
    /// 记录一次节点缓存事件。
    pub fn record_node_cache(&mut self, cache_key: &str, hit: bool) {
        if hit {
            self.cache_report.node_hits += 1;
        } else {
            self.cache_report.node_misses += 1;
        }
        self.cache_events.push(PromptCacheEvent {
            scope: String::from("node"),
            cache_key: cache_key.to_string(),
            hit,
        });
    }

    /// Record one bundle-cache event.
    /// 记录一次组合缓存事件。
    pub fn record_bundle_cache(&mut self, cache_key: &str, hit: bool) {
        if hit {
            self.cache_report.bundle_hits += 1;
        } else {
            self.cache_report.bundle_misses += 1;
        }
        self.cache_events.push(PromptCacheEvent {
            scope: String::from("bundle"),
            cache_key: cache_key.to_string(),
            hit,
        });
    }
}
