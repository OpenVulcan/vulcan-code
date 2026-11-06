//! External-compatible memory token estimation and default persistence clipping.
//! 与外置兼容的记忆 Token 估算和默认持久化裁剪。

use regex::RegexSet;
use std::sync::LazyLock;

/// Classifies non-ASCII scalars using the same Unicode categories as external textutil.
/// 使用与外置 textutil 相同的 Unicode 类别划分非 ASCII 字符。
static UNICODE_BUCKETS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        r"[\p{Han}\p{Hiragana}\p{Katakana}\p{Hangul}]",
        r"[\p{Latin}&&\p{L}]",
        r"\p{Nd}",
        r"\p{White_Space}",
        r"\p{L}",
        r"\p{Ll}",
        r"\p{Lu}",
    ])
    .expect("static memory token Unicode categories")
});

/// Tracks one Latin run; flushing returns its cost and resets the run.
/// 跟踪一段连续拉丁字母；结算时返回成本并重置该段。
#[derive(Default)]
struct LatinRun {
    /// Counts letters in the current contiguous run.
    /// 统计当前连续字母段的长度。
    letters: usize,
    /// Counts Unicode lowercase-to-uppercase transitions.
    /// 统计 Unicode 小写到大写的跃迁次数。
    transitions: usize,
    /// Records whether the preceding Latin letter was lowercase.
    /// 记录前一个拉丁字母是否小写。
    previous_lower: bool,
}

impl LatinRun {
    /// Appends one letter with its category flags; produces no output.
    /// 按类别标记追加一个字母；无返回值。
    fn push(&mut self, lower: bool, upper: bool) {
        if self.previous_lower && upper {
            self.transitions += 1;
        }
        self.letters += 1;
        self.previous_lower = lower;
    }

    /// Returns the external default word cost and clears the accumulator.
    /// 返回外置默认单词成本并清空累计状态。
    fn flush(&mut self) -> f64 {
        if self.letters == 0 {
            return 0.0;
        }
        // Preserve Go's operation order: bucket grouping affects rounding near integer thresholds.
        // 保留 Go 的运算顺序：分桶累加会影响整数边界附近的舍入。
        let mut cost = 0.88;
        if self.letters > 5 {
            cost += (self.letters - 5) as f64 / 4.5;
        }
        if self.transitions > 0 {
            cost += self.transitions as f64 * 0.10;
        }
        *self = Self::default();
        cost
    }
}

/// Estimates text with external VMM's domestic profile; empty input costs zero.
/// 使用外置 VMM 国内模型配置估算文本；空输入成本为零。
/// Returns a saturating u32 count, not a provider tokenizer result or a billing quantity.
/// 返回饱和 u32 计数，并非供应商分词结果或计费数量。
pub fn estimate_memory_tokens(text: &str) -> u32 {
    // Keep CJK, Latin, digits, symbols, whitespace, and other letters in external summation order.
    // 按外置求和顺序分别保存中日韩、拉丁、数字、符号、空白及其他字母。
    let mut buckets = [0.0_f64; 6];
    // Hold one contiguous Latin run until a non-Latin scalar ends it.
    // 保存连续拉丁字母段，直到非拉丁字符结束该段。
    let mut latin = LatinRun::default();
    for character in text.chars() {
        // ASCII avoids regex overhead for common code, logs, and English inputs.
        // ASCII 绕过正则开销，覆盖常见代码、日志和英文输入。
        let bucket = if character.is_ascii() {
            if character.is_ascii_alphabetic() {
                latin.push(
                    character.is_ascii_lowercase(),
                    character.is_ascii_uppercase(),
                );
                continue;
            }
            if character.is_ascii_digit() {
                2
            } else if character.is_whitespace() {
                4
            } else {
                3
            }
        } else {
            // Encode exactly one valid scalar; category matches never inspect adjacent text.
            // 编码恰好一个合法字符；类别匹配不会读取相邻文本。
            let mut utf8 = [0; 4];
            // Preserve Go's IsLetter/Ll/Lu semantics instead of Rust's broader Alphabetic property.
            // 保留 Go 的 IsLetter/Ll/Lu 语义，不使用 Rust 更宽泛的 Alphabetic 属性。
            let classes = UNICODE_BUCKETS.matches(character.encode_utf8(&mut utf8));
            if classes.matched(0) {
                0
            } else if classes.matched(1) {
                latin.push(classes.matched(5), classes.matched(6));
                continue;
            } else if classes.matched(2) {
                2
            } else if classes.matched(3) {
                4
            } else if classes.matched(4) {
                5
            } else {
                3
            }
        };
        buckets[1] += latin.flush();
        buckets[bucket] += [1.18, 0.0, 0.50, 0.82, 0.08, 1.10][bucket];
    }
    buckets[1] += latin.flush();
    // Sum buckets before applying the external safety margin, without fused multiply-add.
    // 分桶求和后应用外置安全系数，不使用融合乘加。
    let total = buckets.into_iter().sum::<f64>();
    (total * 1.06).ceil().min(f64::from(u32::MAX)) as u32
}

/// Clips text using external VMM's default 1200-token, 1200-head/400-tail persistence policy.
/// 使用外置 VMM 默认的 1200 Token、头部 1200 字符及尾部 400 字符持久化策略裁剪文本。
/// Returns unchanged input below budget; dense text retains the external 80/40-character floor.
/// 预算内返回原文；高密度文本保留外置的头尾 80/40 字符下限。
pub fn clip_memory_text_to_default_budget(text: &str) -> String {
    clip_memory_text_to_budget(text, crate::text_policy::DEFAULT_MAX_FRAGMENT_TOKENS)
}

/// Applies the selected approximate token target with the source-compatible head/tail shrink algorithm.
/// 使用来源兼容的头尾收缩算法应用选定近似 Token 目标。
/// `max_tokens` is validated by the caller's text policy; returns intact or marked, Unicode-safe text.
/// `max_tokens` 由调用者文本策略校验；返回完整或带标记且 Unicode 安全的文本。
pub(crate) fn clip_memory_text_to_budget(text: &str, max_tokens: u32) -> String {
    if estimate_memory_tokens(text) <= max_tokens {
        return text.to_owned();
    }
    // Count Unicode scalars, not UTF-8 bytes, so clipping never splits a character.
    // 按 Unicode 字符而非 UTF-8 字节计数，避免裁断字符。
    let characters: Vec<char> = text.chars().collect();
    // Preserve the exact external initial head/tail allocation.
    // 保留与外置完全一致的初始头尾分配。
    let mut head = (max_tokens as usize).min(characters.len());
    // Tail starts after the head when dense input contains fewer than 1600 characters.
    // 高密度输入不足 1600 字符时，尾部从头部之后开始分配。
    let mut tail = (max_tokens as usize / 3).min(characters.len() - head);
    loop {
        // Keep the marker inside estimation; it is part of the persisted model-visible input.
        // 将标记纳入估算；它属于持久化后模型可见输入的一部分。
        let clipped = characters[..head].iter().collect::<String>()
            + "\n...[truncated by token budget]...\n"
            + &characters[characters.len() - tail..]
                .iter()
                .collect::<String>();
        if estimate_memory_tokens(&clipped) <= max_tokens || (head <= 80 && tail <= 40) {
            return clipped;
        }
        if head > 80 {
            head = 80.max((head as f64 * 0.8) as usize);
        }
        if tail > 40 {
            tail = 40.max((tail as f64 * 0.8) as usize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{clip_memory_text_to_default_budget, estimate_memory_tokens};

    /// Checks empty/short identity and verifies clipping preserves a dense Unicode head and tail.
    /// 检查空白及短文本恒等，并验证裁剪保留高密度 Unicode 头尾。
    #[test]
    fn default_budget_preserves_short_text_and_unicode_boundaries() {
        assert_eq!(estimate_memory_tokens(""), 0);
        assert_eq!(
            clip_memory_text_to_default_budget("短文 Hello"),
            "短文 Hello"
        );
        // Synthetic dense text exercises progressive shrinking and tail retention.
        // 合成高密度文本覆盖渐进收缩和尾部保留。
        let dense = "首".repeat(2500) + &"尾".repeat(500);
        // Retain output for independent content and budget assertions.
        // 保存输出以分别断言内容及预算。
        let clipped = clip_memory_text_to_default_budget(&dense);
        assert!(clipped.starts_with('首'));
        assert!(clipped.ends_with('尾'));
        assert!(clipped.contains("\n...[truncated by token budget]...\n"));
        assert!(estimate_memory_tokens(&clipped) <= 1200);
    }
}
