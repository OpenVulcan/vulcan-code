//! Normalization and hashing helpers shared across context-injection modules.
//! 上下文注入模块共享的规范化与哈希辅助函数。

/// Normalizes one tag name to the stable limited form accepted by the renderer.
/// 将标签名规范化为渲染器接受的稳定受限形式。
pub(crate) fn normalize_tag(tag: &str) -> String {
    tag.trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect()
}

/// Normalizes one provider-visible item content value.
/// 规范化一条 Provider 可见内容值。
pub(crate) fn normalize_content(content: &str) -> String {
    content.trim().replace("\r\n", "\n").replace('\r', "\n")
}

/// Normalizes one renderer id to a stable lookup key.
/// 将渲染器标识规范化为稳定查询键。
pub(crate) fn normalize_renderer_id(id: &str) -> String {
    id.trim().to_ascii_lowercase()
}

/// Builds a stable short content hash without adding an extra dependency.
/// 在不增加额外依赖的情况下构建稳定短内容哈希。
pub(crate) fn stable_content_hash(content: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
