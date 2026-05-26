/// Cache hit/miss counters collected during one composition run.
/// 一次组合执行期间收集的缓存命中与未命中计数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptCacheReport {
    /// Node-cache hit count.
    /// 节点缓存命中次数。
    pub node_hits: usize,
    /// Node-cache miss count.
    /// 节点缓存未命中次数。
    pub node_misses: usize,
    /// Bundle-cache hit count.
    /// 组合缓存命中次数。
    pub bundle_hits: usize,
    /// Bundle-cache miss count.
    /// 组合缓存未命中次数。
    pub bundle_misses: usize,
}
