//! Explicit VCE dimensions derived only from the selected model's declared contract.
//! 仅从所选模型声明契约推导的 VCE 明确维度。

use vulcan_ai_contract::EmbeddingCapabilities;

/// Preferred initial dimension; this is a UI suggestion, never a persisted runtime fallback.
/// 首次建议维度；仅供界面建议，绝不是持久化运行时兜底。
const PREFERRED_DIMENSION: u32 = 1024;

/// Validates a user-selected dimension against declared discrete values and inclusive bounds.
/// 根据声明的离散值与包含式边界校验用户选择的明确维度。
///
/// # Parameters
/// 参数
/// `capabilities` is the frozen model-route contract; `dimension` is the explicit saved choice.
/// `capabilities` 为冻结的模型路由契约；`dimension` 为已保存的明确选择。
///
/// # Returns
/// 返回值
/// Success for a proven supported positive dimension, otherwise a configuration error.
/// 已证明支持的正整数维度返回成功，否则返回配置错误。
pub(crate) fn validate_explicit_embedding_dimension(
    capabilities: &EmbeddingCapabilities,
    dimension: u32,
) -> Result<(), String> {
    // Bounds and the discrete set are cumulative restrictions, as in the shared AI contract.
    // 与共享 AI 契约一致，边界和离散集合共同限制可选范围。
    let (minimum, maximum) = declared_dimension_bounds(capabilities)?;
    if dimension < minimum
        || dimension > maximum
        || (!capabilities.dimensions.is_empty() && !capabilities.dimensions.contains(&dimension))
    {
        return Err(format!(
            "VCE embedding dimension {dimension} is outside the declared model contract"
        ));
    }
    Ok(())
}

/// Suggests an explicit supported dimension while preserving a still-valid user choice.
/// 建议明确受支持的维度，同时保留仍然有效的用户选择。
///
/// # Parameters
/// 参数
/// `capabilities` declares valid values; `preserved` is an existing explicit UI selection.
/// `capabilities` 声明合法取值；`preserved` 为界面中已有的明确选择。
///
/// # Returns
/// 返回值
/// The preserved choice, 1024, nearest higher value, or nearest lower value, in that order.
/// 按已有选择、1024、最近向上值、最近向下值的顺序返回建议值。
/// Unknown or contradictory declarations return an error without writing configuration.
/// 未知或矛盾的声明返回错误，不写入配置。
pub(crate) fn preferred_embedding_dimension(
    capabilities: &EmbeddingCapabilities,
    preserved: Option<u32>,
) -> Result<u32, String> {
    // A default-only declaration cannot prove which explicit dimensions the model accepts.
    // 只有默认值的声明无法证明模型接受哪些明确维度。
    let (minimum, maximum) = declared_dimension_bounds(capabilities)?;
    if let Some(dimension) = preserved
        && validate_explicit_embedding_dimension(capabilities, dimension).is_ok()
    {
        return Ok(dimension);
    }
    if capabilities.dimensions.is_empty() {
        return Ok(PREFERRED_DIMENSION.clamp(minimum, maximum));
    }
    // BTreeSet ordering makes the first upper value and last lower value deterministic.
    // BTreeSet 的顺序保证首个向上值和最后一个向下值是确定的。
    let upper = capabilities
        .dimensions
        .range(minimum..=maximum)
        .copied()
        .find(|dimension| *dimension >= PREFERRED_DIMENSION);
    if let Some(dimension) = upper {
        return Ok(dimension);
    }
    capabilities
        .dimensions
        .range(minimum..=maximum)
        .next_back()
        .copied()
        .ok_or_else(|| String::from("VCE embedding contract declares no valid dimension"))
}

/// Resolves the contract's legal positive interval without reading provider defaults.
/// 不读取供应商默认值，解析契约允许的正整数区间。
///
/// # Parameters
/// 参数
/// `capabilities` is the exact model capability declaration.
/// `capabilities` 为精确模型能力声明。
///
/// # Returns
/// 返回值
/// Inclusive bounds, or an error for unknown or malformed declarations.
/// 返回包含式边界；声明未知或格式错误时返回错误。
fn declared_dimension_bounds(capabilities: &EmbeddingCapabilities) -> Result<(u32, u32), String> {
    if capabilities.dimensions.is_empty()
        && capabilities.minimum_dimensions.is_none()
        && capabilities.maximum_dimensions.is_none()
    {
        return Err(String::from(
            "VCE requires declared explicit embedding dimensions, not a provider default",
        ));
    }
    // Missing one side means an open positive-u32 bound in the shared capability type.
    // 共享能力类型中缺失的一侧表示正 u32 范围的开放边界。
    let minimum = capabilities.minimum_dimensions.unwrap_or(1);
    // The shared contract represents dimensions as u32, so no larger value is admissible.
    // 共享契约用 u32 表示维度，因此更大的值不可准入。
    let maximum = capabilities.maximum_dimensions.unwrap_or(u32::MAX);
    if minimum == 0 || maximum < minimum || capabilities.dimensions.contains(&0) {
        return Err(String::from(
            "VCE embedding dimension declaration is invalid",
        ));
    }
    Ok((minimum, maximum))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vulcan_ai_contract::{DistanceMetric, EmbeddingNormalization};

    /// Builds a dimension-only fixture; unrelated capabilities do not affect dimension selection.
    /// 构建仅含维度的夹具；无关能力不影响维度选择。
    ///
    /// # Parameters
    /// 参数
    /// `dimensions` contains the exact declared supported values.
    /// `dimensions` 为声明支持的精确值。
    ///
    /// # Returns
    /// 返回值
    /// A capability fixture with no provider default and no additional bounds.
    /// 返回不含供应商默认值及额外边界的能力夹具。
    fn capabilities(dimensions: &[u32]) -> EmbeddingCapabilities {
        EmbeddingCapabilities {
            tasks: BTreeSet::new(),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::new(),
            encodings: BTreeSet::new(),
            dimensions: dimensions.iter().copied().collect(),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: None,
            maximum_inputs: None,
            maximum_tokens_per_input: None,
            normalization: EmbeddingNormalization::Unknown,
            distance_metric: DistanceMetric::Unknown,
        }
    }

    /// Verifies exact, upward and downward preferences independently of declaration order.
    /// 验证精确、向上和向下选择不依赖声明顺序。
    #[test]
    fn explicit_dimensions_use_1024_then_upward_then_downward() {
        assert_eq!(
            preferred_embedding_dimension(&capabilities(&[2048, 512, 1024]), None),
            Ok(1024)
        );
        assert_eq!(
            preferred_embedding_dimension(&capabilities(&[768, 3072, 1536]), None),
            Ok(1536)
        );
        assert_eq!(
            preferred_embedding_dimension(&capabilities(&[256, 768, 512]), None),
            Ok(768)
        );
        assert_eq!(
            preferred_embedding_dimension(&capabilities(&[3072]), None),
            Ok(3072)
        );
    }

    /// Verifies a saved supported dimension stays selected rather than reverting to 1024.
    /// 验证已保存且支持的维度继续选中，不回到 1024。
    #[test]
    fn explicit_dimensions_preserve_valid_choices_only() {
        // The prior choice is a deliberate user preference, not the provider's default.
        // 原选择是用户的明确偏好，不是供应商默认值。
        let contract = capabilities(&[512, 1024, 2048]);
        assert_eq!(
            preferred_embedding_dimension(&contract, Some(2048)),
            Ok(2048)
        );
        assert_eq!(
            preferred_embedding_dimension(&contract, Some(768)),
            Ok(1024)
        );
        assert!(validate_explicit_embedding_dimension(&contract, 768).is_err());
        assert!(validate_explicit_embedding_dimension(&contract, 0).is_err());
    }

    /// Verifies a provider default alone cannot establish an explicit dimension contract.
    /// 验证仅供应商默认值不能建立明确维度契约。
    #[test]
    fn explicit_dimensions_reject_default_only_metadata() {
        // Default values are not an allowed-dimension declaration.
        // 默认值不是支持维度声明。
        let mut contract = capabilities(&[]);
        contract.default_dimensions = Some(1024);
        assert!(preferred_embedding_dimension(&contract, None).is_err());
        assert!(validate_explicit_embedding_dimension(&contract, 1024).is_err());
    }

    /// Verifies continuous inclusive bounds and contradictory declarations are handled explicitly.
    /// 验证连续包含式边界和矛盾声明均被明确处理。
    #[test]
    fn explicit_dimensions_respect_ranges_and_reject_contradictions() {
        // An empty discrete set with explicit bounds is the shared continuous-range contract.
        // 空离散集合配合明确边界是共享契约定义的连续范围。
        let mut contract = capabilities(&[]);
        contract.minimum_dimensions = Some(512);
        contract.maximum_dimensions = Some(2048);
        assert_eq!(preferred_embedding_dimension(&contract, None), Ok(1024));
        contract.minimum_dimensions = Some(1536);
        assert_eq!(preferred_embedding_dimension(&contract, None), Ok(1536));
        contract.minimum_dimensions = Some(256);
        contract.maximum_dimensions = Some(768);
        assert_eq!(preferred_embedding_dimension(&contract, None), Ok(768));
        contract.minimum_dimensions = Some(2048);
        assert!(preferred_embedding_dimension(&contract, None).is_err());
        assert!(validate_explicit_embedding_dimension(&contract, 2048).is_err());
    }

    /// Verifies intersecting discrete and range declarations never suggest excluded values.
    /// 验证离散集合与区间共同约束时不会建议被排除的值。
    #[test]
    fn explicit_dimensions_intersect_discrete_values_with_bounds() {
        // Bounds intentionally exclude the preferred discrete value.
        // 边界特意排除了首选的离散值。
        let mut contract = capabilities(&[512, 1024, 1536, 2048]);
        contract.minimum_dimensions = Some(1400);
        contract.maximum_dimensions = Some(1600);
        assert_eq!(preferred_embedding_dimension(&contract, None), Ok(1536));
        assert!(validate_explicit_embedding_dimension(&contract, 1024).is_err());
        contract.minimum_dimensions = Some(1700);
        contract.maximum_dimensions = Some(1800);
        assert!(preferred_embedding_dimension(&contract, None).is_err());
    }
}
