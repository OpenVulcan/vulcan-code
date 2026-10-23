//! Model-readable profiles retain exact schemas without publishing host audit metadata.
//! 模型可读说明保留精确 Schema，不发布宿主审核元数据。

use serde_json::Value;
use vulcan_provider_adapters::{ProviderToolExtensionProfile, ProviderToolResolutionMode};
use vulcan_tools_manager::result_markdown::{fenced_code, inline_code};

/// Renders one callable model profile, preserving every declared calling constraint.
/// 渲染单个可调用模型说明，保留全部已声明调用约束。
/// Inputs are the public identity, display name, typed profile and exact argument schema.
/// 输入为公开身份、展示名、强类型说明与精确参数 Schema。
/// Returns shared Markdown or an explicit schema serialization error.
/// 返回共享 Markdown 或明确的 Schema 序列化错误。
pub(in crate::provider_tool_bridge) fn model(
    id: &str,
    name: &str,
    profile: &ProviderToolExtensionProfile,
    schema: &Value,
) -> Result<String, String> {
    // The display name stays plain text so the labeled `model` value is the only code span to copy.
    // 展示名保持纯文本，使带标签的 `model` 值成为唯一可复制的代码片段。
    let mut output = format!(
        "## {name}\n\n`model` (copy verbatim, including the provider prefix): {}\n\n{}\n",
        inline_code(id),
        profile.summary
    );
    for (title, items) in [
        ("Characteristics", &profile.characteristics),
        ("Input modes", &profile.input_modes),
        ("Constraints", &profile.constraints),
        ("Limitations", &profile.limitations),
    ] {
        if !items.is_empty() {
            output.push_str(&format!("\n{title}:\n"));
            for item in items {
                output.push_str(&format!("- {item}\n"));
            }
        }
    }
    if let Some(policy) = &profile.resolution_policy {
        let mode = match policy.mode {
            ProviderToolResolutionMode::FixedPresets => "fixed presets only",
            ProviderToolResolutionMode::BoundedCustom => "custom dimensions within declared bounds",
            ProviderToolResolutionMode::ProviderManaged => "provider managed",
        };
        output.push_str(&format!("\nResolution: {mode}.\n"));
        for preset in &policy.presets {
            output.push_str(&format!("- Preset {}", inline_code(&preset.value)));
            if let Some(width) = preset.width {
                output.push_str(&format!("; width {width} px"));
            }
            if let Some(height) = preset.height {
                output.push_str(&format!("; height {height} px"));
            }
            output.push('\n');
        }
        list(&mut output, "Aspect ratios", &policy.aspect_ratios);
        if let Some(bounds) = &policy.custom_bounds {
            output.push_str(&format!(
                "- Inclusive width: {}–{} px; height: {}–{} px; dimension step: {}.\n",
                bounds.minimum_width,
                bounds.maximum_width,
                bounds.minimum_height,
                bounds.maximum_height,
                bounds.step
            ));
            if let Some(min) = bounds.minimum_pixels {
                output.push_str(&format!("- Minimum total pixels: {min}.\n"));
            }
            if let Some(max) = bounds.maximum_pixels {
                output.push_str(&format!("- Maximum total pixels: {max}.\n"));
            }
        }
    }
    if let Some(policy) = &profile.duration_policy {
        output.push_str(&format!(
            "\nDuration: {}–{} seconds inclusive.\n",
            policy.minimum_seconds, policy.maximum_seconds
        ));
        if let Some(default) = policy.default_seconds {
            output.push_str(&format!("- Default: {default} seconds.\n"));
        }
        list(
            &mut output,
            "Allowed durations (seconds)",
            &policy.presets_seconds,
        );
    }
    if let Some(policy) = &profile.format_policy {
        list(&mut output, "Input formats", &policy.input_formats);
        list(&mut output, "Output formats", &policy.output_formats);
        list(&mut output, "Sample rates (Hz)", &policy.sample_rates_hz);
        list(&mut output, "Channel counts", &policy.channel_counts);
    }
    // Schemas are executable calling contracts; no subset or guessed prose may replace them.
    // Schema 是可执行调用契约，不能用子集或猜测性说明替换。
    let schema = serde_json::to_string_pretty(schema)
        .map_err(|error| format!("Failed to encode model argument schema: {error}"))?;
    output.push_str(&format!(
        "\nExact `args_schema`:\n\n{}",
        fenced_code(&schema)
    ));
    Ok(output)
}

/// Appends a nonempty list of exact scalar policy values to shared Markdown.
/// 将非空的精确标量策略值列表追加到共享 Markdown。
/// The output buffer is mutated; empty policies produce no placeholder.
/// 修改输出缓冲；空策略不产生占位内容。
fn list<T: ToString>(output: &mut String, title: &str, values: &[T]) {
    if !values.is_empty() {
        output.push_str(&format!(
            "\n{title}: {}.\n",
            values
                .iter()
                .map(|value| inline_code(&value.to_string()))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}
