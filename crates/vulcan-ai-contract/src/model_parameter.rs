//! Strongly typed model-specific parameter contracts.
//! 强类型模型独立参数契约。

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

/// Describes how an unset model parameter behaves.
/// 描述一个未设置模型参数的行为。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum ModelParameterDefault {
    /// Omits the wire field.
    /// 省略 Wire 字段。
    Omit {},
    /// Follows the provider default without injecting a value.
    /// 遵循供应商默认值且不注入数值。
    ProviderDefault {},
    /// Uses one explicit registered value.
    /// 使用一个显式注册值。
    ExplicitValue {
        /// Default parameter value.
        /// 默认参数值。
        value: ModelParameterValue,
    },
    /// Requires the user or adapter to provide a value.
    /// 要求用户或适配器提供数值。
    RequiredValue {},
}

/// Defines one closed model-parameter value schema.
/// 定义一个封闭的模型参数值 Schema。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "type", rename_all = "snake_case")]
pub enum ModelParameterSchema {
    /// Boolean value.
    /// 布尔值。
    Boolean {},
    /// Bounded integer value.
    /// 有界整数值。
    Integer {
        /// Optional inclusive minimum.
        /// 可选包含式最小值。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<i64>,
        /// Optional inclusive maximum.
        /// 可选包含式最大值。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<i64>,
        /// Optional positive step.
        /// 可选正步长。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<u64>,
    },
    /// Decimal string value.
    /// 十进制字符串值。
    Decimal {
        /// Optional inclusive minimum decimal string.
        /// 可选包含式最小十进制字符串。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum: Option<String>,
        /// Optional inclusive maximum decimal string.
        /// 可选包含式最大十进制字符串。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum: Option<String>,
        /// Optional exclusive minimum decimal string.
        /// 可选排他式最小十进制字符串。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclusive_minimum: Option<String>,
        /// Optional exclusive maximum decimal string.
        /// 可选排他式最大十进制字符串。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exclusive_maximum: Option<String>,
    },
    /// Bounded string value.
    /// 有界字符串值。
    String {
        /// Optional minimum character count.
        /// 可选最小字符数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum_length: Option<usize>,
        /// Optional maximum character count.
        /// 可选最大字符数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum_length: Option<usize>,
    },
    /// Closed enumeration.
    /// 封闭枚举。
    Enum {
        /// Allowed values.
        /// 允许值。
        values: Vec<String>,
    },
    /// Bounded homogeneous array.
    /// 有界同构数组。
    Array {
        /// Item schema.
        /// 元素 Schema。
        items: Box<ModelParameterSchema>,
        /// Optional minimum item count.
        /// 可选最小元素数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        minimum_items: Option<usize>,
        /// Optional maximum item count.
        /// 可选最大元素数。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum_items: Option<usize>,
    },
    /// Closed object with registered fields.
    /// 带已注册字段的封闭对象。
    Object {
        /// Registered property schemas.
        /// 已注册属性 Schema。
        properties: BTreeMap<String, ModelParameterSchema>,
        /// Required property names.
        /// 必填属性名称。
        #[serde(default)]
        required: Vec<String>,
    },
}

/// Carries one strongly typed model parameter value.
/// 承载一个强类型模型参数值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    tag = "type",
    content = "value",
    rename_all = "snake_case"
)]
pub enum ModelParameterValue {
    /// Boolean value.
    /// 布尔值。
    Boolean(bool),
    /// Integer value.
    /// 整数值。
    Integer(i64),
    /// Decimal string value.
    /// 十进制字符串值。
    Decimal(String),
    /// String or enum value.
    /// 字符串或枚举值。
    String(String),
    /// Array value.
    /// 数组值。
    Array(Vec<ModelParameterValue>),
    /// Closed object value.
    /// 封闭对象值。
    Object(BTreeMap<String, ModelParameterValue>),
}

/// Defines one adapter-registered model-specific parameter.
/// 定义一个由适配器注册的模型独立参数。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelParameterDefinition {
    /// Stable namespaced parameter identifier.
    /// 稳定的命名空间参数标识。
    pub id: String,
    /// User-visible name.
    /// 用户可见名称。
    pub display_name: String,
    /// User-visible description.
    /// 用户可见说明。
    pub description: String,
    /// Closed value schema.
    /// 封闭值 Schema。
    pub schema: ModelParameterSchema,
    /// Default behavior.
    /// 默认行为。
    pub default: ModelParameterDefault,
    /// Applicable protocol profile identifiers.
    /// 适用协议 Profile 标识。
    #[serde(default)]
    pub protocol_ids: Vec<String>,
    /// Applicable exact model identifiers.
    /// 适用的精确模型标识。
    #[serde(default)]
    pub model_ids: Vec<String>,
    /// Required sibling parameter identifiers.
    /// 必需的同级参数标识。
    #[serde(default)]
    pub dependent_required: Vec<String>,
    /// Conflicting sibling parameter identifiers.
    /// 冲突的同级参数标识。
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Evidence revision.
    /// 证据修订号。
    pub evidence_revision: String,
}

impl ModelParameterDefinition {
    /// Validates the complete parameter declaration before it becomes runtime authority.
    /// 在参数声明成为运行时权威前校验其完整定义。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` for a closed, internally consistent declaration; otherwise a controlled message.
    /// 对封闭且内部一致的声明返回 `Ok(())`，否则返回受控消息。
    pub fn validate_definition(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err(String::from("Model parameter identifier is required."));
        }
        if self.display_name.trim().is_empty() {
            return Err(format!(
                "Model parameter `{}` requires a display name.",
                self.id
            ));
        }
        if self.description.trim().is_empty() {
            return Err(format!(
                "Model parameter `{}` requires a description.",
                self.id
            ));
        }
        if self.evidence_revision.trim().is_empty() {
            return Err(format!(
                "Model parameter `{}` requires an evidence revision.",
                self.id
            ));
        }
        validate_unique_non_empty_identifiers(
            self.protocol_ids.as_slice(),
            self.id.as_str(),
            "protocol",
        )?;
        validate_unique_non_empty_identifiers(
            self.model_ids.as_slice(),
            self.id.as_str(),
            "model",
        )?;
        validate_unique_non_empty_identifiers(
            self.dependent_required.as_slice(),
            self.id.as_str(),
            "dependency",
        )?;
        validate_unique_non_empty_identifiers(
            self.conflicts.as_slice(),
            self.id.as_str(),
            "conflict",
        )?;
        validate_schema_definition(&self.schema, self.id.as_str())?;
        if let ModelParameterDefault::ExplicitValue { value } = &self.default {
            self.validate_value(value)?;
        }
        Ok(())
    }

    /// Validates one value against this definition.
    /// 根据当前定义校验一个值。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the user-selected parameter value.
    /// `value` 是用户选择的参数值。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when valid, otherwise a controlled validation message.
    /// 有效时返回 `Ok(())`，否则返回受控校验消息。
    pub fn validate_value(&self, value: &ModelParameterValue) -> Result<(), String> {
        validate_schema_value(&self.schema, value, self.id.as_str())
    }
}

/// Validates one complete effective model-parameter definition set and its relationships.
/// 校验一组完整生效模型参数定义及其关系。
///
/// # Parameters
/// 参数
/// `definitions` is the post-inheritance parameter set for one exact model/protocol authority.
/// `definitions` 是一个精确模型/协议权威在继承后的参数集合。
///
/// # Returns
/// 返回值
/// Success only when identifiers are unique and every dependency/conflict reference is coherent.
/// 仅当标识唯一且每个依赖/冲突引用均一致时返回成功。
pub fn validate_model_parameter_definition_set(
    definitions: &[ModelParameterDefinition],
) -> Result<(), String> {
    let mut by_id = BTreeMap::new();
    for definition in definitions {
        definition.validate_definition()?;
        if by_id.insert(definition.id.as_str(), definition).is_some() {
            return Err(format!(
                "Model parameter `{}` is declared more than once.",
                definition.id
            ));
        }
    }
    for definition in definitions {
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
        for related_id in dependencies.union(&conflicts) {
            if *related_id == definition.id {
                return Err(format!(
                    "Model parameter `{}` cannot depend on or conflict with itself.",
                    definition.id
                ));
            }
            if !by_id.contains_key(related_id) {
                return Err(format!(
                    "Model parameter `{}` references undeclared parameter `{related_id}`.",
                    definition.id
                ));
            }
        }
        if let Some(related_id) = dependencies.intersection(&conflicts).next() {
            return Err(format!(
                "Model parameter `{}` both requires and conflicts with `{related_id}`.",
                definition.id
            ));
        }
        for dependency_id in dependencies {
            let Some(dependency) = by_id.get(dependency_id) else {
                return Err(format!(
                    "Model parameter `{}` references undeclared dependency `{dependency_id}`.",
                    definition.id
                ));
            };
            validate_dependency_scope(definition, dependency)?;
        }
    }
    Ok(())
}

/// Validates that one dependency is available everywhere its dependent parameter is available.
/// 校验一个依赖项在依赖它的参数可用之处均同样可用。
///
/// # Parameters
/// 参数
/// `definition` is the dependent parameter and `dependency` is its declared prerequisite.
/// `definition` 是依赖方参数，`dependency` 是其声明的前置参数。
///
/// # Returns
/// 返回值
/// Success when protocol and model allowlists of the dependency cover the dependent scope.
/// 当依赖项的协议和模型白名单覆盖依赖方作用域时返回成功。
fn validate_dependency_scope(
    definition: &ModelParameterDefinition,
    dependency: &ModelParameterDefinition,
) -> Result<(), String> {
    if !scope_allowlist_covers(&dependency.protocol_ids, &definition.protocol_ids) {
        return Err(format!(
            "Model parameter `{}` depends on `{}`, whose protocol scope does not cover it.",
            definition.id, dependency.id
        ));
    }
    if !scope_allowlist_covers(&dependency.model_ids, &definition.model_ids) {
        return Err(format!(
            "Model parameter `{}` depends on `{}`, whose model scope does not cover it.",
            definition.id, dependency.id
        ));
    }
    Ok(())
}

/// Reports whether one empty-means-all allowlist covers another allowlist.
/// 报告一个“空集合表示全部”的白名单是否覆盖另一个白名单。
///
/// # Parameters
/// 参数
/// `authority` is the prerequisite scope and `dependent` is the scope requiring coverage.
/// `authority` 是前置项作用域，`dependent` 是要求被覆盖的作用域。
///
/// # Returns
/// 返回值
/// `true` when every dependent identity is admitted by the authority.
/// 当权威允许每个依赖方身份时返回 `true`。
fn scope_allowlist_covers(authority: &[String], dependent: &[String]) -> bool {
    authority.is_empty()
        || (!dependent.is_empty()
            && dependent
                .iter()
                .all(|identity| authority.contains(identity)))
}

/// Validates one identifier list without guessing or silently collapsing duplicates.
/// 校验一个标识列表，不猜测也不静默折叠重复项。
///
/// # Parameters
/// 参数
/// `values` is the declared list, `parameter_id` identifies its owner, and `kind` names the list.
/// `values` 是声明列表，`parameter_id` 标识其所有者，`kind` 命名该列表。
///
/// # Returns
/// 返回值
/// Success only when every identifier is non-empty and unique.
/// 仅当每个标识均非空且唯一时返回成功。
fn validate_unique_non_empty_identifiers(
    values: &[String],
    parameter_id: &str,
    kind: &str,
) -> Result<(), String> {
    let mut unique = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() {
            return Err(format!(
                "Model parameter `{parameter_id}` contains an empty {kind} identifier."
            ));
        }
        if !unique.insert(value.as_str()) {
            return Err(format!(
                "Model parameter `{parameter_id}` contains duplicate {kind} identifier `{value}`."
            ));
        }
    }
    Ok(())
}

/// Validates one recursive model-parameter schema as a closed non-empty value domain.
/// 将一个递归模型参数 Schema 校验为封闭且非空的值域。
///
/// # Parameters
/// 参数
/// `schema` is the declared value schema and `path` identifies the current recursive location.
/// `schema` 是声明的值 Schema，`path` 标识当前递归位置。
///
/// # Returns
/// 返回值
/// Success only when bounds, collections, and required-property references are consistent.
/// 仅当边界、集合及必填属性引用保持一致时返回成功。
fn validate_schema_definition(schema: &ModelParameterSchema, path: &str) -> Result<(), String> {
    match schema {
        ModelParameterSchema::Boolean {} => Ok(()),
        ModelParameterSchema::Integer {
            minimum,
            maximum,
            step,
        } => {
            if minimum
                .zip(*maximum)
                .is_some_and(|(minimum, maximum)| minimum > maximum)
            {
                return Err(format!(
                    "Model parameter schema `{path}` has a minimum above its maximum."
                ));
            }
            if step.is_some_and(|step| step == 0) {
                return Err(format!(
                    "Model parameter schema `{path}` has an invalid zero step."
                ));
            }
            Ok(())
        }
        ModelParameterSchema::Decimal {
            minimum,
            maximum,
            exclusive_minimum,
            exclusive_maximum,
        } => {
            for (label, bound) in [
                ("minimum", minimum),
                ("maximum", maximum),
                ("exclusive minimum", exclusive_minimum),
                ("exclusive maximum", exclusive_maximum),
            ] {
                if let Some(bound) = bound {
                    validate_decimal_bound(bound, path, label)?;
                }
            }
            validate_decimal_interval(minimum, maximum, false, false, path)?;
            validate_decimal_interval(minimum, exclusive_maximum, false, true, path)?;
            validate_decimal_interval(exclusive_minimum, maximum, true, false, path)?;
            validate_decimal_interval(exclusive_minimum, exclusive_maximum, true, true, path)
        }
        ModelParameterSchema::String {
            minimum_length,
            maximum_length,
        } => {
            if minimum_length
                .zip(*maximum_length)
                .is_some_and(|(minimum, maximum)| minimum > maximum)
            {
                return Err(format!(
                    "Model parameter schema `{path}` has a minimum length above its maximum length."
                ));
            }
            Ok(())
        }
        ModelParameterSchema::Enum { values } => {
            if values.is_empty() {
                return Err(format!(
                    "Model parameter schema `{path}` requires at least one enum value."
                ));
            }
            let unique = values.iter().collect::<BTreeSet<_>>();
            if unique.len() != values.len() {
                return Err(format!(
                    "Model parameter schema `{path}` contains duplicate enum values."
                ));
            }
            Ok(())
        }
        ModelParameterSchema::Array {
            items,
            minimum_items,
            maximum_items,
        } => {
            if minimum_items
                .zip(*maximum_items)
                .is_some_and(|(minimum, maximum)| minimum > maximum)
            {
                return Err(format!(
                    "Model parameter schema `{path}` has a minimum item count above its maximum item count."
                ));
            }
            validate_schema_definition(items, format!("{path}[]").as_str())
        }
        ModelParameterSchema::Object {
            properties,
            required,
        } => {
            let required_names = required.iter().collect::<BTreeSet<_>>();
            if required_names.len() != required.len() {
                return Err(format!(
                    "Model parameter schema `{path}` contains duplicate required properties."
                ));
            }
            for name in required {
                if !properties.contains_key(name) {
                    return Err(format!(
                        "Model parameter schema `{path}` requires undeclared property `{name}`."
                    ));
                }
            }
            for (name, property_schema) in properties {
                if name.is_empty() {
                    return Err(format!(
                        "Model parameter schema `{path}` contains an empty property name."
                    ));
                }
                validate_schema_definition(property_schema, format!("{path}.{name}").as_str())?;
            }
            Ok(())
        }
    }
}

/// Validates whether two declared decimal endpoints leave a non-empty interval.
/// 校验两个已声明十进制端点是否保留非空区间。
///
/// # Parameters
/// 参数
/// `lower` and `upper` are optional bounds, exclusivity flags describe their openness, and `path`
/// identifies the owning schema.
/// `lower` 与 `upper` 是可选边界，排他标志描述其开闭性，`path` 标识所属 Schema。
///
/// # Returns
/// 返回值
/// Success when either endpoint is absent or the resulting interval is non-empty.
/// 任一端点缺失或所得区间非空时返回成功。
fn validate_decimal_interval(
    lower: &Option<String>,
    upper: &Option<String>,
    lower_exclusive: bool,
    upper_exclusive: bool,
    path: &str,
) -> Result<(), String> {
    let (Some(lower), Some(upper)) = (lower, upper) else {
        return Ok(());
    };
    let ordering = compare_decimal_lexemes(lower, upper);
    if ordering == Ordering::Greater
        || (ordering == Ordering::Equal && (lower_exclusive || upper_exclusive))
    {
        return Err(format!(
            "Model parameter schema `{path}` declares an empty decimal interval."
        ));
    }
    Ok(())
}

/// Validates one model parameter value recursively.
/// 递归校验一个模型参数值。
fn validate_schema_value(
    schema: &ModelParameterSchema,
    value: &ModelParameterValue,
    path: &str,
) -> Result<(), String> {
    match (schema, value) {
        (ModelParameterSchema::Boolean {}, ModelParameterValue::Boolean(_)) => Ok(()),
        (
            ModelParameterSchema::Integer {
                minimum,
                maximum,
                step,
            },
            ModelParameterValue::Integer(integer),
        ) => {
            if minimum.is_some_and(|minimum| *integer < minimum) {
                return Err(format!("Parameter `{path}` is below its minimum."));
            }
            if maximum.is_some_and(|maximum| *integer > maximum) {
                return Err(format!("Parameter `{path}` is above its maximum."));
            }
            if let Some(step) = step {
                if *step == 0 {
                    return Err(format!("Parameter `{path}` has an invalid zero step."));
                }
                let origin = minimum.unwrap_or(0);
                let difference = i128::from(*integer) - i128::from(origin);
                if difference.rem_euclid(i128::from(*step)) != 0 {
                    return Err(format!("Parameter `{path}` does not match its step."));
                }
            }
            Ok(())
        }
        (
            ModelParameterSchema::Decimal {
                minimum,
                maximum,
                exclusive_minimum,
                exclusive_maximum,
            },
            ModelParameterValue::Decimal(decimal),
        ) => {
            if !validate_decimal_lexeme(decimal) {
                return Err(format!("Parameter `{path}` is not a valid decimal string."));
            }
            if let Some(minimum) = minimum {
                validate_decimal_bound(minimum, path, "minimum")?;
                if compare_decimal_lexemes(decimal, minimum) == Ordering::Less {
                    return Err(format!("Parameter `{path}` is below its minimum."));
                }
            }
            if let Some(maximum) = maximum {
                validate_decimal_bound(maximum, path, "maximum")?;
                if compare_decimal_lexemes(decimal, maximum) == Ordering::Greater {
                    return Err(format!("Parameter `{path}` is above its maximum."));
                }
            }
            if let Some(minimum) = exclusive_minimum {
                validate_decimal_bound(minimum, path, "exclusive minimum")?;
                if compare_decimal_lexemes(decimal, minimum) != Ordering::Greater {
                    return Err(format!(
                        "Parameter `{path}` is not above its exclusive minimum."
                    ));
                }
            }
            if let Some(maximum) = exclusive_maximum {
                validate_decimal_bound(maximum, path, "exclusive maximum")?;
                if compare_decimal_lexemes(decimal, maximum) != Ordering::Less {
                    return Err(format!(
                        "Parameter `{path}` is not below its exclusive maximum."
                    ));
                }
            }
            Ok(())
        }
        (
            ModelParameterSchema::String {
                minimum_length,
                maximum_length,
            },
            ModelParameterValue::String(string),
        ) => {
            let character_count = string.chars().count();
            if minimum_length.is_some_and(|minimum| character_count < minimum) {
                return Err(format!("Parameter `{path}` is shorter than its minimum."));
            }
            if maximum_length.is_some_and(|maximum| character_count > maximum) {
                return Err(format!("Parameter `{path}` is longer than its maximum."));
            }
            Ok(())
        }
        (ModelParameterSchema::Enum { values }, ModelParameterValue::String(string)) => values
            .contains(string)
            .then_some(())
            .ok_or_else(|| format!("Parameter `{path}` is not one of its registered values.")),
        (
            ModelParameterSchema::Array {
                items,
                minimum_items,
                maximum_items,
            },
            ModelParameterValue::Array(values),
        ) => {
            if minimum_items.is_some_and(|minimum| values.len() < minimum) {
                return Err(format!("Parameter `{path}` has too few items."));
            }
            if maximum_items.is_some_and(|maximum| values.len() > maximum) {
                return Err(format!("Parameter `{path}` has too many items."));
            }
            for (index, item) in values.iter().enumerate() {
                validate_schema_value(items, item, format!("{path}[{index}]").as_str())?;
            }
            Ok(())
        }
        (
            ModelParameterSchema::Object {
                properties,
                required,
            },
            ModelParameterValue::Object(values),
        ) => {
            for required_name in required {
                if !values.contains_key(required_name) {
                    return Err(format!(
                        "Parameter `{path}` is missing required field `{required_name}`."
                    ));
                }
            }
            for (name, item) in values {
                let property_schema = properties.get(name).ok_or_else(|| {
                    format!("Parameter `{path}` contains unregistered field `{name}`.")
                })?;
                validate_schema_value(property_schema, item, format!("{path}.{name}").as_str())?;
            }
            Ok(())
        }
        _ => Err(format!("Parameter `{path}` has the wrong value type.")),
    }
}

/// Validates a finite base-ten decimal without binary floating-point conversion.
/// 在不转换为二进制浮点数的情况下校验有限十进制定点数。
///
/// # Parameters
/// 参数
/// `value` is the exact decimal text supplied by the caller.
/// `value` 是调用方提供的精确十进制文本。
///
/// # Returns
/// 返回值
/// `true` when the text is an optional sign followed by digits and at most one decimal point.
/// 当文本由可选符号、数字以及至多一个小数点组成时返回 `true`。
fn validate_decimal_lexeme(value: &str) -> bool {
    let unsigned = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    if unsigned.is_empty() {
        return false;
    }
    let mut decimal_point_seen = false;
    let mut digit_count = 0usize;
    for character in unsigned.chars() {
        if character.is_ascii_digit() {
            digit_count += 1;
            continue;
        }
        if character == '.' && !decimal_point_seen {
            decimal_point_seen = true;
            continue;
        }
        return false;
    }
    digit_count > 0
}

/// Validates one adapter-registered decimal bound before using it.
/// 在使用前校验一个由适配器注册的十进制边界。
///
/// # Parameters
/// 参数
/// `value` is the exact bound, `path` identifies the parameter, and `kind` names the bound.
/// `value` 是精确边界，`path` 标识参数，`kind` 命名边界类型。
///
/// # Returns
/// 返回值
/// `Ok(())` for a valid bound or a controlled adapter-definition error.
/// 边界有效时返回 `Ok(())`，否则返回受控的适配器定义错误。
fn validate_decimal_bound(value: &str, path: &str, kind: &str) -> Result<(), String> {
    validate_decimal_lexeme(value)
        .then_some(())
        .ok_or_else(|| format!("Parameter `{path}` has an invalid registered decimal {kind}."))
}

/// Compares two validated decimal lexemes without precision loss.
/// 在不损失精度的情况下比较两个已校验十进制词法值。
///
/// # Parameters
/// 参数
/// `left` and `right` are finite base-ten decimal strings accepted by the lexical validator.
/// `left` 与 `right` 是词法校验器接受的有限十进制字符串。
///
/// # Returns
/// 返回值
/// The exact numeric ordering.
/// 返回精确数值顺序。
fn compare_decimal_lexemes(left: &str, right: &str) -> Ordering {
    let left = normalized_decimal(left);
    let right = normalized_decimal(right);
    if left.negative != right.negative {
        return if left.negative {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let magnitude = left
        .integer
        .len()
        .cmp(&right.integer.len())
        .then_with(|| left.integer.cmp(right.integer))
        .then_with(|| {
            let width = left.fraction.len().max(right.fraction.len());
            left.fraction
                .bytes()
                .chain(std::iter::repeat(b'0'))
                .take(width)
                .cmp(
                    right
                        .fraction
                        .bytes()
                        .chain(std::iter::repeat(b'0'))
                        .take(width),
                )
        });
    if left.negative {
        magnitude.reverse()
    } else {
        magnitude
    }
}

/// Owns one normalized decimal split used by exact comparison.
/// 保存用于精确比较的规范化十进制拆分。
struct NormalizedDecimal<'a> {
    /// Whether the numeric value is negative.
    /// 数值是否为负数。
    negative: bool,
    /// Integer digits without redundant leading zeroes.
    /// 不含冗余前导零的整数数字。
    integer: &'a str,
    /// Fraction digits without redundant trailing zeroes.
    /// 不含冗余尾随零的小数数字。
    fraction: &'a str,
}

/// Normalizes one validated decimal without allocating.
/// 在不分配内存的情况下规范化一个已校验十进制值。
///
/// # Parameters
/// 参数
/// `value` is a finite validated decimal lexeme.
/// `value` 是一个已校验的有限十进制词法值。
///
/// # Returns
/// 返回值
/// A sign, integer, and fraction view suitable for exact comparison.
/// 返回适合精确比较的符号、整数和小数视图。
fn normalized_decimal(value: &str) -> NormalizedDecimal<'_> {
    let has_negative_sign = value.starts_with('-');
    let unsigned = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    let (integer, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let integer = integer.trim_start_matches('0');
    let integer = if integer.is_empty() { "0" } else { integer };
    let fraction = fraction.trim_end_matches('0');
    let negative = has_negative_sign && !(integer == "0" && fraction.is_empty());
    NormalizedDecimal {
        negative,
        integer,
        fraction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one intrinsically valid parameter definition for focused schema tests.
    /// 为聚焦 Schema 测试构建一个内部有效的参数定义。
    ///
    /// # Parameters
    /// 参数
    /// `id` is the stable parameter identifier and `schema` is the value contract under test.
    /// `id` 是稳定参数标识，`schema` 是待测值契约。
    ///
    /// # Returns
    /// 返回值
    /// A definition with no relationship or applicability restrictions.
    /// 返回不含关系或适用范围限制的定义。
    fn parameter_definition(id: &str, schema: ModelParameterSchema) -> ModelParameterDefinition {
        ModelParameterDefinition {
            id: id.to_string(),
            display_name: id.to_string(),
            description: String::from("Test parameter definition."),
            schema,
            default: ModelParameterDefault::Omit {},
            protocol_ids: Vec::new(),
            model_ids: Vec::new(),
            dependent_required: Vec::new(),
            conflicts: Vec::new(),
            evidence_revision: String::from("test.v1"),
        }
    }

    /// Verifies serialized parameter definitions reject unknown top-level fields.
    /// 验证序列化参数定义拒绝未知顶层字段。
    #[test]
    fn parameter_definition_rejects_unknown_fields() {
        let document = serde_json::json!({
            "id": "example.temperature",
            "display_name": "Temperature",
            "description": "Sampling temperature.",
            "schema": { "type": "decimal" },
            "default": { "type": "omit" },
            "evidence_revision": "test.v1",
            "unexpected": true
        });

        assert!(serde_json::from_value::<ModelParameterDefinition>(document).is_err());
    }

    /// Verifies serialized parameter schemas reject unknown variant fields.
    /// 验证序列化参数 Schema 拒绝未知变体字段。
    #[test]
    fn parameter_schema_rejects_unknown_fields() {
        let document = serde_json::json!({
            "type": "integer",
            "minimum": 0,
            "unexpected": true
        });

        assert!(serde_json::from_value::<ModelParameterSchema>(document).is_err());
    }

    /// Verifies serialized parameter defaults reject unknown variant fields.
    /// 验证序列化参数默认值拒绝未知变体字段。
    #[test]
    fn parameter_default_rejects_unknown_fields() {
        let document = serde_json::json!({
            "type": "explicit_value",
            "value": { "type": "boolean", "value": true },
            "unexpected": true
        });

        assert!(serde_json::from_value::<ModelParameterDefault>(document).is_err());
    }

    /// Verifies malformed ranges, enum domains, and object requirements fail before activation.
    /// 验证畸形范围、枚举值域及对象必填声明在激活前失败。
    #[test]
    fn malformed_parameter_schemas_are_rejected() {
        let schemas = [
            ModelParameterSchema::Integer {
                minimum: Some(2),
                maximum: Some(1),
                step: None,
            },
            ModelParameterSchema::Integer {
                minimum: None,
                maximum: None,
                step: Some(0),
            },
            ModelParameterSchema::Decimal {
                minimum: Some(String::from("2")),
                maximum: Some(String::from("1")),
                exclusive_minimum: None,
                exclusive_maximum: None,
            },
            ModelParameterSchema::String {
                minimum_length: Some(2),
                maximum_length: Some(1),
            },
            ModelParameterSchema::Enum { values: Vec::new() },
            ModelParameterSchema::Array {
                items: Box::new(ModelParameterSchema::Boolean {}),
                minimum_items: Some(2),
                maximum_items: Some(1),
            },
            ModelParameterSchema::Object {
                properties: BTreeMap::new(),
                required: vec![String::from("missing")],
            },
        ];

        for schema in schemas {
            assert!(
                parameter_definition("invalid", schema)
                    .validate_definition()
                    .is_err()
            );
        }
    }

    /// Verifies dependency scopes cover every protocol and model where the dependent is exposed.
    /// 验证依赖项作用域覆盖依赖方公开到的每个协议与模型。
    #[test]
    fn dependency_scope_must_cover_dependent_scope() {
        let mut dependent = parameter_definition("dependent", ModelParameterSchema::Boolean {});
        dependent.protocol_ids = vec![String::from("responses"), String::from("anthropic")];
        dependent.dependent_required = vec![String::from("prerequisite")];
        let mut prerequisite =
            parameter_definition("prerequisite", ModelParameterSchema::Boolean {});
        prerequisite.protocol_ids = vec![String::from("responses")];

        assert!(
            validate_model_parameter_definition_set(&[dependent, prerequisite]).is_err(),
            "a narrower prerequisite must not leave the dependent unusable on another protocol"
        );
    }

    /// Verifies closed objects reject unregistered fields.
    /// 验证封闭对象拒绝未注册字段。
    #[test]
    fn closed_object_rejects_unregistered_fields() {
        let definition = ModelParameterDefinition {
            id: String::from("example.config"),
            display_name: String::from("Example"),
            description: String::from("Example parameter."),
            schema: ModelParameterSchema::Object {
                properties: BTreeMap::from([(
                    String::from("enabled"),
                    ModelParameterSchema::Boolean {},
                )]),
                required: vec![String::from("enabled")],
            },
            default: ModelParameterDefault::Omit {},
            protocol_ids: Vec::new(),
            model_ids: Vec::new(),
            dependent_required: Vec::new(),
            conflicts: Vec::new(),
            evidence_revision: String::from("test"),
        };
        let value = ModelParameterValue::Object(BTreeMap::from([
            (String::from("enabled"), ModelParameterValue::Boolean(true)),
            (String::from("unknown"), ModelParameterValue::Boolean(true)),
        ]));

        assert!(definition.validate_value(&value).is_err());
    }

    /// Verifies decimal bounds retain arbitrary base-ten precision.
    /// 验证十进制边界保留任意十进制精度。
    #[test]
    fn decimal_bounds_are_compared_without_binary_floating_point() {
        let definition = ModelParameterDefinition {
            id: String::from("example.decimal"),
            display_name: String::from("Decimal"),
            description: String::from("Exact decimal parameter."),
            schema: ModelParameterSchema::Decimal {
                minimum: Some(String::from("-0.1000000000000000000000000000001")),
                maximum: Some(String::from("1.0000000000000000000000000000001")),
                exclusive_minimum: None,
                exclusive_maximum: None,
            },
            default: ModelParameterDefault::Omit {},
            protocol_ids: Vec::new(),
            model_ids: Vec::new(),
            dependent_required: Vec::new(),
            conflicts: Vec::new(),
            evidence_revision: String::from("test"),
        };

        assert!(
            definition
                .validate_value(&ModelParameterValue::Decimal(String::from(
                    "1.0000000000000000000000000000001"
                )))
                .is_ok()
        );
        assert!(
            definition
                .validate_value(&ModelParameterValue::Decimal(String::from(
                    "1.0000000000000000000000000000002"
                )))
                .is_err()
        );
    }
}
