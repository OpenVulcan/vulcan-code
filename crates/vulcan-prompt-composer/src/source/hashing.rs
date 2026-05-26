/// Compute one stable FNV-1a hash for UTF-8 text.
/// 为 UTF-8 文本计算稳定的 FNV-1a 哈希。
pub fn stable_text_hash(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
