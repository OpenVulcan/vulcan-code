/// Build one stable node cache key.
/// 构建稳定的节点缓存键。
pub fn node_cache_key(namespace: &str, fingerprint: u64) -> String {
    format!("node:{namespace}:{fingerprint:016x}")
}

/// Build one stable bundle cache key.
/// 构建稳定的组合缓存键。
pub fn bundle_cache_key(namespace: &str, fingerprints: &[u64]) -> String {
    let suffix = fingerprints
        .iter()
        .map(|fingerprint| format!("{fingerprint:016x}"))
        .collect::<Vec<_>>()
        .join("-");
    format!("bundle:{namespace}:{suffix}")
}
