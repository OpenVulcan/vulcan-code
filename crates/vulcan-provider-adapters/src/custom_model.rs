//! User-owned custom model declarations and safe request projections.
//! 用户自有自定义模型声明与安全请求投影。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_ai_contract::{ModelCapabilities, ModelParameterDefault, ReasoningEffort};

/// Lists request-root fields reserved for protocol and host ownership.
/// 列出由协议与宿主持有的请求根字段。
const RESERVED_REQUEST_ROOTS: &[&str] = &[
    "model",
    "messages",
    "input",
    "instructions",
    "system",
    "tools",
    "tool_choice",
    "stream",
    "stream_options",
];

/// Defines one validated mutation applied to a standard protocol request.
/// 定义应用到标准协议请求的一项已校验变更。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CustomRequestMutation {
    /// Exact JSON object paths and values to set.
    /// 要设置的精确 JSON 对象路径和值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, Value>,
    /// Exact JSON object paths to delete.
    /// 要删除的精确 JSON 对象路径。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delete: Vec<String>,
}

/// Defines the closed projection contract attached to one custom model.
/// 定义附加到一个自定义模型的封闭投影契约。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CustomModelRequestProjection {
    /// Canonical reasoning levels to exact request mutations.
    /// 规范思考档位到精确请求变更的映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub reasoning: BTreeMap<ReasoningEffort, CustomRequestMutation>,
    /// Registered model parameter identifiers to exact request paths.
    /// 已登记模型参数标识到精确请求路径的映射。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_parameter_paths: BTreeMap<String, String>,
    /// Fields added only when the standard protocol body omitted the path.
    /// 仅在标准协议请求体缺少路径时添加的字段。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<String, Value>,
    /// Fields that intentionally replace non-reserved standard fields.
    /// 有意替换非保留标准字段的字段。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub overrides: BTreeMap<String, Value>,
    /// Non-reserved standard fields removed after protocol encoding.
    /// 协议编码后删除的非保留标准字段。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delete: Vec<String>,
}

/// Stores one user-owned model without duplicating provider or credential state.
/// 保存一个不复制供应商或凭据状态的用户自有模型。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustomModelDefinition {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    pub id: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    pub display_name: String,
    /// Whether this model is available to workbench selection.
    /// 此模型是否可供工作台选择。
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Explicit provider-neutral capability declaration.
    /// 显式供应商无关能力声明。
    pub capabilities: ModelCapabilities,
    /// Optional safe post-protocol request projection.
    /// 可选的安全协议后请求投影。
    #[serde(default)]
    pub request_projection: CustomModelRequestProjection,
}

impl CustomModelDefinition {
    /// Validates identifiers, capabilities, and every declared request path.
    /// 校验标识、能力以及每一条已声明请求路径。
    ///
    /// # Returns
    /// 返回值
    /// Success or one actionable pre-save validation message.
    /// 返回成功或一条可操作的保存前校验消息。
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err(String::from("Custom model identifier is required."));
        }
        if self.display_name.trim().is_empty() {
            return Err(String::from("Custom model display name is required."));
        }
        if self.capabilities.context_limit == Some(0) {
            return Err(String::from(
                "Custom model context limit must be greater than zero.",
            ));
        }
        if self.capabilities.output_limit == Some(0) {
            return Err(String::from(
                "Custom model output limit must be greater than zero.",
            ));
        }
        self.validate_model_parameter_definitions()?;
        for effort in self.request_projection.reasoning.keys() {
            if !self.capabilities.reasoning_efforts.contains(effort) {
                return Err(format!(
                    "Reasoning projection `{}` is not declared by the model capability.",
                    effort.as_str()
                ));
            }
        }
        for parameter_id in self.request_projection.model_parameter_paths.keys() {
            if !self
                .capabilities
                .model_parameters
                .iter()
                .any(|definition| definition.id == *parameter_id)
            {
                return Err(format!(
                    "Request projection references undeclared model parameter `{parameter_id}`."
                ));
            }
        }
        for path in self.request_projection.all_paths() {
            validate_request_path(path)?;
        }
        self.request_projection.validate_path_ownership()?;
        Ok(())
    }

    /// Validates every custom parameter definition before it can enter persisted configuration.
    /// 在每个自定义参数定义进入持久化配置前进行校验。
    ///
    /// # Returns
    /// 返回值
    /// Success only when identifiers, defaults, dependencies, conflicts, and request paths agree.
    /// 仅当标识、默认值、依赖、冲突与请求路径一致时返回成功。
    fn validate_model_parameter_definitions(&self) -> Result<(), String> {
        let mut parameter_ids = BTreeSet::new();
        for definition in &self.capabilities.model_parameters {
            if definition.id.trim().is_empty() {
                return Err(String::from(
                    "Custom model parameter identifier is required.",
                ));
            }
            if definition.display_name.trim().is_empty() {
                return Err(format!(
                    "Custom model parameter `{}` requires a display name.",
                    definition.id
                ));
            }
            if !parameter_ids.insert(definition.id.as_str()) {
                return Err(format!(
                    "Custom model parameter `{}` is declared more than once.",
                    definition.id
                ));
            }
            if let ModelParameterDefault::ExplicitValue { value } = &definition.default {
                definition.validate_value(value)?;
            }
        }
        for definition in &self.capabilities.model_parameters {
            if !self
                .request_projection
                .model_parameter_paths
                .contains_key(definition.id.as_str())
            {
                return Err(format!(
                    "Custom model parameter `{}` requires an exact request path.",
                    definition.id
                ));
            }
            let dependencies = definition
                .dependent_required
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            let conflicts = definition
                .conflicts
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if dependencies.len() != definition.dependent_required.len() {
                return Err(format!(
                    "Custom model parameter `{}` contains duplicate dependencies.",
                    definition.id
                ));
            }
            if conflicts.len() != definition.conflicts.len() {
                return Err(format!(
                    "Custom model parameter `{}` contains duplicate conflicts.",
                    definition.id
                ));
            }
            for related_id in dependencies.union(&conflicts) {
                if *related_id == definition.id {
                    return Err(format!(
                        "Custom model parameter `{}` cannot depend on or conflict with itself.",
                        definition.id
                    ));
                }
                if !parameter_ids.contains(related_id) {
                    return Err(format!(
                        "Custom model parameter `{}` references undeclared parameter `{related_id}`.",
                        definition.id
                    ));
                }
            }
            if let Some(related_id) = dependencies.intersection(&conflicts).next() {
                return Err(format!(
                    "Custom model parameter `{}` both requires and conflicts with `{related_id}`.",
                    definition.id
                ));
            }
        }
        Ok(())
    }
}

impl CustomModelRequestProjection {
    /// Iterates every request path declared by this projection.
    /// 迭代此投影声明的每一条请求路径。
    fn all_paths(&self) -> impl Iterator<Item = &str> {
        self.defaults
            .keys()
            .chain(self.overrides.keys())
            .chain(self.delete.iter())
            .chain(self.model_parameter_paths.values())
            .chain(
                self.reasoning
                    .values()
                    .flat_map(|mutation| mutation.set.keys().chain(mutation.delete.iter())),
            )
            .map(String::as_str)
    }

    /// Rejects ambiguous exact-path ownership across mutually exclusive projection rules.
    /// 拒绝互斥投影规则之间含义不明确的相同精确路径归属。
    fn validate_path_ownership(&self) -> Result<(), String> {
        let mut owned_paths = BTreeMap::<&str, &str>::new();
        register_unique_paths(
            &mut owned_paths,
            self.defaults.keys().map(String::as_str),
            "defaults",
        )?;
        register_unique_paths(
            &mut owned_paths,
            self.model_parameter_paths.values().map(String::as_str),
            "model_parameter_paths",
        )?;
        register_unique_paths(
            &mut owned_paths,
            self.delete.iter().map(String::as_str),
            "delete",
        )?;
        register_unique_paths(
            &mut owned_paths,
            self.overrides.keys().map(String::as_str),
            "overrides",
        )?;

        for (effort, mutation) in &self.reasoning {
            // Each effort starts from the global ownership set so different mutually exclusive
            // efforts may reuse one wire path while one effort cannot collide with global rules.
            // 每个档位从全局归属集合开始，因此互斥档位可复用线级路径，单个档位不能与全局规则冲突。
            let mut effort_paths = owned_paths.clone();
            register_unique_paths(
                &mut effort_paths,
                mutation.set.keys().map(String::as_str),
                "reasoning.set",
            )
            .map_err(|error| {
                format!(
                    "Reasoning projection `{}` is invalid: {error}",
                    effort.as_str()
                )
            })?;
            register_unique_paths(
                &mut effort_paths,
                mutation.delete.iter().map(String::as_str),
                "reasoning.delete",
            )
            .map_err(|error| {
                format!(
                    "Reasoning projection `{}` is invalid: {error}",
                    effort.as_str()
                )
            })?;
        }
        Ok(())
    }
}

/// Registers exact paths for one mutually exclusive projection section.
/// 为一个互斥投影区段登记精确路径。
fn register_unique_paths<'a>(
    owned_paths: &mut BTreeMap<&'a str, &'static str>,
    paths: impl Iterator<Item = &'a str>,
    owner: &'static str,
) -> Result<(), String> {
    for path in paths {
        if let Some((previous_path, previous_owner)) = owned_paths
            .iter()
            .find(|(previous_path, _)| request_paths_overlap(previous_path, path))
        {
            return Err(format!(
                "Custom request path `{path}` conflicts with `{previous_path}` declared by `{previous_owner}` and `{owner}` rules."
            ));
        }
        owned_paths.insert(path, owner);
    }
    Ok(())
}

/// Reports whether two dot-separated request paths are equal or structurally nested.
/// 报告两条点分隔请求路径是否相同或具有结构嵌套关系。
///
/// # Parameters
/// 参数
/// `left` and `right` are validated exact object paths.
/// `left` 与 `right` 是已校验的精确对象路径。
///
/// # Returns
/// 返回值
/// True when assigning either path can change the parent shape required by the other.
/// 当写入任一路径可能改变另一路径所需的父级结构时返回真。
fn request_paths_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('.'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

/// Returns the default visible state for a newly added custom model.
/// 返回新添加自定义模型的默认可见状态。
fn default_enabled() -> bool {
    true
}

/// Validates one dot-separated request path against syntax and ownership rules.
/// 根据语法与所有权规则校验一条点分隔请求路径。
fn validate_request_path(path: &str) -> Result<(), String> {
    let segments = path.split('.').collect::<Vec<_>>();
    if segments.is_empty()
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
    {
        return Err(format!(
            "Custom request path `{path}` must contain dot-separated alphanumeric object keys."
        ));
    }
    if RESERVED_REQUEST_ROOTS.contains(&segments[0]) {
        return Err(format!(
            "Custom request path `{path}` targets host-owned root `{}`.",
            segments[0]
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies host-owned request roots cannot be changed by custom providers.
    /// 验证自定义供应商不能修改宿主持有的请求根字段。
    #[test]
    fn custom_projection_rejects_host_owned_roots() {
        let error = validate_request_path("messages.0.content").expect_err("must reject");
        assert!(error.contains("host-owned root"));
    }

    /// Verifies nested provider-owned object fields remain available.
    /// 验证嵌套的供应商自有对象字段仍然可用。
    #[test]
    fn custom_projection_accepts_safe_nested_paths() {
        validate_request_path("reasoning.budget_tokens").expect("safe path");
    }

    /// Verifies fixed projection sections cannot silently override each other by declaration order.
    /// 验证固定投影区段不能通过声明顺序静默互相覆盖。
    #[test]
    fn custom_projection_rejects_mutually_exclusive_exact_paths() {
        let projection = CustomModelRequestProjection {
            defaults: BTreeMap::from([(
                String::from("vendor.mode"),
                Value::String(String::from("auto")),
            )]),
            delete: vec![String::from("vendor.mode")],
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("must reject conflict");

        assert!(error.contains("defaults"));
        assert!(error.contains("delete"));
        assert!(error.contains("vendor.mode"));
    }

    /// Verifies one reasoning level cannot both set and delete the same request path.
    /// 验证单个思考档位不能同时设置并删除同一请求路径。
    #[test]
    fn custom_projection_rejects_reasoning_set_delete_conflict() {
        let projection = CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                CustomRequestMutation {
                    set: BTreeMap::from([(String::from("thinking.mode"), Value::Bool(true))]),
                    delete: vec![String::from("thinking.mode")],
                },
            )]),
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("must reject conflict");

        assert!(error.contains("reasoning.set"));
        assert!(error.contains("reasoning.delete"));
        assert!(error.contains("thinking.mode"));
    }

    /// Verifies parent and child paths cannot create an order-dependent request shape.
    /// 验证父路径与子路径不能创建依赖执行顺序的请求结构。
    #[test]
    fn custom_projection_rejects_structurally_overlapping_paths() {
        let projection = CustomModelRequestProjection {
            defaults: BTreeMap::from([(
                String::from("vendor"),
                Value::String(String::from("scalar")),
            )]),
            overrides: BTreeMap::from([(
                String::from("vendor.mode"),
                Value::String(String::from("auto")),
            )]),
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("nested paths must be rejected");

        assert!(error.contains("vendor"));
        assert!(error.contains("vendor.mode"));
    }

    /// Verifies one reasoning mutation cannot collide with a global projection path.
    /// 验证单个思考变更不能与全局投影路径冲突。
    #[test]
    fn custom_projection_rejects_reasoning_and_global_path_conflicts() {
        let projection = CustomModelRequestProjection {
            defaults: BTreeMap::from([(
                String::from("thinking.mode"),
                Value::String(String::from("disabled")),
            )]),
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("thinking"),
                        Value::Object(Default::default()),
                    )]),
                    delete: Vec::new(),
                },
            )]),
            ..CustomModelRequestProjection::default()
        };

        let error = projection
            .validate_path_ownership()
            .expect_err("reasoning path must not overlap global path");

        assert!(error.contains("Reasoning projection `high`"));
        assert!(error.contains("thinking.mode"));
        assert!(error.contains("thinking"));
    }

    /// Verifies every user-declared parameter has one explicit safe wire path.
    /// 验证每个用户声明参数都有一条显式安全的线级路径。
    #[test]
    fn custom_model_parameter_requires_request_path() {
        let model = CustomModelDefinition {
            id: String::from("custom-model"),
            display_name: String::from("Custom Model"),
            enabled: true,
            capabilities: ModelCapabilities {
                model_parameters: vec![vulcan_ai_contract::ModelParameterDefinition {
                    id: String::from("vendor.flag"),
                    display_name: String::from("Vendor flag"),
                    description: String::from("Controls one verified vendor flag."),
                    schema: vulcan_ai_contract::ModelParameterSchema::Boolean,
                    default: ModelParameterDefault::ProviderDefault,
                    protocol_ids: Vec::new(),
                    model_ids: Vec::new(),
                    dependent_required: Vec::new(),
                    conflicts: Vec::new(),
                    evidence_revision: String::from("user.custom-model.v1"),
                }],
                ..ModelCapabilities::default()
            },
            request_projection: CustomModelRequestProjection::default(),
        };

        let error = model
            .validate()
            .expect_err("parameter without a path must be rejected");

        assert!(error.contains("requires an exact request path"));
    }

    /// Verifies unknown custom projection fields fail closed instead of being silently ignored.
    /// 验证未知自定义投影字段会封闭失败而不是被静默忽略。
    #[test]
    fn custom_projection_rejects_unknown_serialized_fields() {
        let error = serde_json::from_value::<CustomModelRequestProjection>(serde_json::json!({
            "headers": {"authorization": "unsafe"}
        }))
        .expect_err("unknown projection fields must be rejected");

        assert!(error.to_string().contains("unknown field"));
    }
}
