//! Preserves source Unicode simple-fold equality in review-query construction without allocating regular expressions.
//! 本文件属于引擎查询层，为评审查询保留来源Unicode简单折叠相等语义，不构造动态正则。

/// Supplies the 30 Unicode17 orbit representatives that differ from source simple lowercase.
/// 提供Unicode17中不同于来源简单小写的30个轨道代表；由VMM全标量标准库测试生成，完整摘要在下方复核。
const FOLD_OVERRIDES: &[(char, char)] = &[
    ('\u{130}', '\u{130}'),
    ('\u{17f}', '\u{73}'),
    ('\u{399}', '\u{345}'),
    ('\u{39c}', '\u{b5}'),
    ('\u{3a3}', '\u{3c2}'),
    ('\u{3b9}', '\u{345}'),
    ('\u{3bc}', '\u{b5}'),
    ('\u{3c3}', '\u{3c2}'),
    ('\u{3d0}', '\u{3b2}'),
    ('\u{3d1}', '\u{3b8}'),
    ('\u{3d5}', '\u{3c6}'),
    ('\u{3d6}', '\u{3c0}'),
    ('\u{3f0}', '\u{3ba}'),
    ('\u{3f1}', '\u{3c1}'),
    ('\u{3f5}', '\u{3b5}'),
    ('\u{1c80}', '\u{432}'),
    ('\u{1c81}', '\u{434}'),
    ('\u{1c82}', '\u{43e}'),
    ('\u{1c83}', '\u{441}'),
    ('\u{1c84}', '\u{442}'),
    ('\u{1c85}', '\u{442}'),
    ('\u{1c86}', '\u{44a}'),
    ('\u{1c87}', '\u{463}'),
    ('\u{1e9b}', '\u{1e61}'),
    ('\u{1fbe}', '\u{345}'),
    ('\u{1fd3}', '\u{390}'),
    ('\u{1fe3}', '\u{3b0}'),
    ('\u{a64a}', '\u{1c88}'),
    ('\u{a64b}', '\u{1c88}'),
    ('\u{fb06}', '\u{fb05}'),
];

/// Returns the source simple-fold representative for one valid scalar, checking exceptions before lowercase loses identity.
/// 为一个合法标量返回来源简单折叠代表；先查例外再小写，避免带点I的独立身份被普通i吞并。
fn representative(value: char) -> char {
    FOLD_OVERRIDES
        .binary_search_by_key(&value, |entry| entry.0)
        .map_or_else(
            |_| value.to_lowercase().next().unwrap_or(value),
            |index| FOLD_OVERRIDES[index].1,
        )
}

/// Compares UTF-8 strings by the source's simple-fold equivalence, without full case expansion or contextual lowercasing.
/// 按来源简单折叠等价关系比较两个UTF-8字符串，返回相等与否；不进行完整大小写展开或上下文小写转换。
// Preserve crate-only visibility if this private implementation module is reorganized later.
// 即使后续重组私有实现模块，也保持仅crate内部可见的边界。
#[allow(clippy::redundant_pub_crate)]
pub(crate) fn simple_fold_equal(left: &str, right: &str) -> bool {
    if left.is_ascii() && right.is_ascii() {
        return left.eq_ignore_ascii_case(right);
    }
    left.chars()
        .map(representative)
        .eq(right.chars().map(representative))
}

#[cfg(test)]
mod tests {
    use super::{FOLD_OVERRIDES, representative, simple_fold_equal};
    use sha2::{Digest, Sha256};

    /// Verifies every scalar's real lowercase helper and fold mapping against independently generated source bytes.
    /// 对照独立生成的来源字节，验证全部标量经过真实小写助手及折叠映射后的摘要；工具链数据漂移必须明确失败。
    #[test]
    fn all_scalars_match_source_unicode17() {
        assert_eq!(char::UNICODE_VERSION, (17, 0, 0));
        assert!(FOLD_OVERRIDES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        let mut digest = Sha256::new();
        for value in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
            let lowered = crate::algorithms::simple_lowercase(&value.to_string());
            assert_eq!(lowered.chars().count(), 1);
            let lower = lowered.chars().next().unwrap_or(value);
            digest.update(u32::from(value).to_le_bytes());
            digest.update(u32::from(lower).to_le_bytes());
            digest.update(u32::from(representative(value)).to_le_bytes());
        }
        assert_eq!(
            format!("{:x}", digest.finalize()),
            "9b38b54fcb7670d91fc7aac701a6ef54bba5c6afa47c85cb6e5bd2e4028405e0"
        );
    }

    /// Exercises expansions, exceptional orbits and unequal lengths through the actual comparison entry.
    /// 通过真实比较入口验证完整展开不等价、例外轨道和长度不同；不以小写相等替代折叠语义。
    #[test]
    fn source_orbits_do_not_expand_or_merge_dotted_i() {
        for (left, right) in [("ſ", "S"), ("Σ", "ς"), ("ᾀ", "ᾈ"), ("µ", "Μ"), ("ẞ", "ß")]
        {
            assert!(simple_fold_equal(left, right));
        }
        for (left, right) in [("İ", "i"), ("ı", "I"), ("ß", "ss"), ("Σ", "ςς")] {
            assert!(!simple_fold_equal(left, right));
        }
        assert!(simple_fold_equal("ASCII", "ascii"));
        assert!(!simple_fold_equal("ASCII", "asci"));
    }
}
