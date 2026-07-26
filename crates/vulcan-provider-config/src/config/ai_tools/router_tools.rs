use super::*;

/// Closed Router parameter value kind published by one immutable capability contract.
/// 由不可变能力合同发布的封闭 Router 参数值类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterParameterSchemaKind {
    /// Object value with explicitly declared properties.
    /// 具有显式属性声明的对象值。
    Object,
    /// Array value with one recursive item schema.
    /// 具有单个递归元素 Schema 的数组值。
    Array,
    /// Unicode string value.
    /// Unicode 字符串值。
    String,
    /// Signed integer value.
    /// 有符号整数值。
    Integer,
    /// Floating-point number value.
    /// 浮点数值。
    Number,
    /// Boolean value.
    /// 布尔值。
    Boolean,
    /// Explicit null value.
    /// 显式空值。
    Null,
    /// Client-local resource path that must be imported before execution.
    /// 必须在执行前导入的客户端本地资源路径。
    Resource,
}

/// One typed scalar literal from a Router parameter contract.
/// Router 参数合同中的一个类型化标量字面量。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RouterParameterSchemaValue {
    /// Optional string literal.
    /// 可选字符串字面量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string: Option<String>,
    /// Optional integer literal.
    /// 可选整数字面量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integer: Option<i64>,
    /// Optional floating-point literal.
    /// 可选浮点数字面量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<f64>,
    /// Optional boolean literal.
    /// 可选布尔字面量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boolean: Option<bool>,
    /// Whether this literal is explicit null.
    /// 此字面量是否为显式空值。
    #[serde(default, skip_serializing_if = "is_false")]
    pub null: bool,
}

/// One named object property from a Router parameter contract.
/// Router 参数合同中的一个命名对象属性。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterParameterSchemaProperty {
    /// Exact property name.
    /// 精确属性名称。
    pub name: String,
    /// Whether the caller must provide the property.
    /// 调用方是否必须提供该属性。
    pub required: bool,
    /// Complete recursive property schema.
    /// 完整递归属性 Schema。
    pub schema: RouterParameterSchema,
}

/// Closed conditional predicate operator used by Router parameter contracts.
/// Router 参数合同使用的封闭条件谓词操作符。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterParameterPredicateOperator {
    /// Matches one exact typed literal.
    /// 匹配一个精确类型化字面量。
    Equals,
    /// Rejects one exact typed literal.
    /// 排除一个精确类型化字面量。
    NotEquals,
    /// Matches when the path exists.
    /// 路径存在时匹配。
    Present,
    /// Matches when the path is absent.
    /// 路径不存在时匹配。
    Absent,
}

/// One exact dotted-path predicate from a Router parameter contract.
/// Router 参数合同中的一个精确点分路径谓词。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterParameterSchemaPredicate {
    /// Dotted path relative to the parameter root.
    /// 相对于参数根节点的点分路径。
    pub path: String,
    /// Closed comparison operator.
    /// 封闭比较操作符。
    pub operator: RouterParameterPredicateOperator,
    /// Literal required by equality operators.
    /// 相等类操作符要求的字面量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<RouterParameterSchemaValue>,
}

/// One deterministic conditional requirement from a Router parameter contract.
/// Router 参数合同中的一条确定性条件要求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterParameterSchemaCondition {
    /// Predicate that activates the requirement.
    /// 激活要求的谓词。
    #[serde(rename = "if")]
    pub predicate: RouterParameterSchemaPredicate,
    /// Dotted paths required when the predicate matches.
    /// 谓词匹配时必填的点分路径集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub require: Vec<String>,
    /// Dotted paths forbidden when the predicate matches.
    /// 谓词匹配时禁止的点分路径集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbid: Vec<String>,
}

/// One presence-triggered dependency from a Router parameter contract.
/// Router 参数合同中的一条存在触发依赖。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterParameterSchemaDependency {
    /// Exact trigger property path.
    /// 精确触发属性路径。
    pub property: String,
    /// Exact property paths required with the trigger.
    /// 与触发属性同时必填的精确属性路径。
    pub require: Vec<String>,
}

/// Complete recursive Router parameter schema used as the source of an AI tool schema.
/// 作为 AI 工具 Schema 唯一来源的完整递归 Router 参数 Schema。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RouterParameterSchema {
    /// Closed value kind; absent only for a pure union node.
    /// 封闭值类型；仅纯联合节点允许缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<RouterParameterSchemaKind>,
    /// Client-facing value semantics.
    /// 面向客户端的值语义。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Deterministically ordered object properties.
    /// 确定性排序的对象属性。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<RouterParameterSchemaProperty>,
    /// Recursive array item schema.
    /// 递归数组元素 Schema。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<RouterParameterSchema>>,
    /// Exact accepted scalar values.
    /// 精确接受的标量值。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_values: Vec<RouterParameterSchemaValue>,
    /// Sole accepted scalar value.
    /// 唯一接受的标量值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#const: Option<RouterParameterSchemaValue>,
    /// Provider-documented default scalar value.
    /// 供应商文档声明的默认标量值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<RouterParameterSchemaValue>,
    /// Provider-documented scalar examples.
    /// 供应商文档声明的标量示例。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<RouterParameterSchemaValue>,
    /// Inclusive integer lower bound.
    /// 包含式整数下限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_integer: Option<i64>,
    /// Inclusive integer upper bound.
    /// 包含式整数上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_integer: Option<i64>,
    /// Exclusive integer lower bound.
    /// 排他式整数下限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum_integer: Option<i64>,
    /// Exclusive integer upper bound.
    /// 排他式整数上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum_integer: Option<i64>,
    /// Positive integer divisibility constraint.
    /// 正整数整除约束。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple_of_integer: Option<i64>,
    /// Inclusive numeric lower bound.
    /// 包含式数值下限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_number: Option<f64>,
    /// Inclusive numeric upper bound.
    /// 包含式数值上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_number: Option<f64>,
    /// Exclusive numeric lower bound.
    /// 排他式数值下限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_minimum_number: Option<f64>,
    /// Exclusive numeric upper bound.
    /// 排他式数值上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusive_maximum_number: Option<f64>,
    /// Positive floating-point step.
    /// 正浮点步长。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple_of_number: Option<f64>,
    /// Inclusive Unicode code-point length lower bound.
    /// 包含式 Unicode 码点长度下限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_length: Option<usize>,
    /// Inclusive Unicode code-point length upper bound.
    /// 包含式 Unicode 码点长度上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_length: Option<usize>,
    /// Inclusive array length lower bound.
    /// 包含式数组长度下限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_items: Option<usize>,
    /// Inclusive array length upper bound.
    /// 包含式数组长度上限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_items: Option<usize>,
    /// Whether duplicate array values are forbidden.
    /// 是否禁止重复数组值。
    #[serde(default, skip_serializing_if = "is_false")]
    pub unique_items: bool,
    /// RE2-compatible string constraint.
    /// 兼容 RE2 的字符串约束。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pattern: String,
    /// Closed client rendering hint.
    /// 封闭客户端渲染提示。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub format: String,
    /// Exact accepted provider-owned resource roles.
    /// 精确接受的供应商资源角色。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_roles: Vec<String>,
    /// Deterministic cross-field conditions.
    /// 确定性跨字段条件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<RouterParameterSchemaCondition>,
    /// Presence-triggered cross-field dependencies.
    /// 存在触发的跨字段依赖。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependent_required: Vec<RouterParameterSchemaDependency>,
    /// Exact-one recursive alternatives.
    /// 精确一项递归候选。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub one_of: Vec<RouterParameterSchema>,
    /// At-least-one recursive alternatives.
    /// 至少一项递归候选。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<RouterParameterSchema>,
    /// All required recursive constraints.
    /// 全部必需的递归约束。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub all_of: Vec<RouterParameterSchema>,
    /// Recursive negative constraint.
    /// 递归否定约束。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<Box<RouterParameterSchema>>,
    /// Recursive conditional predicate.
    /// 递归条件谓词。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#if: Option<Box<RouterParameterSchema>>,
    /// Constraint applied when the recursive predicate matches.
    /// 递归谓词匹配时应用的约束。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<Box<RouterParameterSchema>>,
    /// Constraint applied when the recursive predicate does not match.
    /// 递归谓词不匹配时应用的约束。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#else: Option<Box<RouterParameterSchema>>,
}

/// Closed logical parameter owner published by Router.
/// Router 发布的封闭逻辑参数所有者。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterToolParameterOwner {
    /// Value generated by the AI.
    /// 由 AI 生成的值。
    Ai,
    /// Value supplied by Vulcan Code.
    /// 由 Vulcan Code 提供的值。
    Client,
    /// Value frozen in the selected binding.
    /// 在选定绑定中冻结的值。
    Binding,
    /// Value injected by Router.
    /// 由 Router 注入的值。
    Router,
}

/// Closed VCP projection destination published by Router.
/// Router 发布的封闭 VCP 投影目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterToolParameterTargetKind {
    /// Typed VCP operation field.
    /// 类型化 VCP 操作字段。
    Operation,
    /// Capability-scoped execution parameter field.
    /// 能力作用域执行参数字段。
    ExecutionParameter,
    /// Local-resource import followed by a typed VCP field assignment.
    /// 本地资源导入后赋值到类型化 VCP 字段。
    ResourceImport,
}

/// One logical-to-VCP parameter mapping published by Router.
/// Router 发布的一条逻辑参数到 VCP 的映射。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterToolParameterMapping {
    /// Exact dotted source path in the AI-visible parameter object.
    /// AI 可见参数对象中的精确点分源路径。
    pub source_path: String,
    /// Layer responsible for supplying the source value.
    /// 负责提供源值的层。
    pub owner: RouterToolParameterOwner,
    /// Closed destination family.
    /// 封闭目标类别。
    pub target_kind: RouterToolParameterTargetKind,
    /// Exact VCP or execution-parameter target path.
    /// 精确 VCP 或执行参数目标路径。
    pub target_path: String,
}

/// One local-resource input that Vulcan Code imports before execution.
/// Vulcan Code 在执行前导入的一个本地资源输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterToolResourceInput {
    /// Exact AI-visible local-path field.
    /// 精确 AI 可见本地路径字段。
    pub source_path: String,
    /// Exact Router resource family.
    /// 精确 Router 资源类别。
    pub kind: String,
    /// Exact typed operation role.
    /// 精确类型化操作角色。
    pub role: String,
    /// Accepted normalized MIME types.
    /// 接受的规范 MIME 类型。
    pub mime_types: Vec<String>,
    /// Exclusive per-file byte ceiling when positive.
    /// 为正数时表示每个文件的排他字节上限。
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub maximum_item_bytes: i64,
    /// Inclusive minimum local-path count.
    /// 包含式最小本地路径数量。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub minimum_items: usize,
    /// Inclusive maximum local-path count; zero means provider-defined.
    /// 包含式最大本地路径数量；零表示由供应商定义。
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub maximum_items: usize,
}

/// Normalized generated-resource output contract published by Router.
/// Router 发布的规范化生成资源输出合同。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterToolOutputContract {
    /// Generated Router resource family.
    /// 生成的 Router 资源类别。
    pub kind: String,
    /// Exact accepted result encodings.
    /// 精确接受的结果编码。
    pub mime_types: Vec<String>,
    /// Inclusive minimum result count.
    /// 包含式最小结果数量。
    pub minimum_items: usize,
    /// Inclusive maximum result count.
    /// 包含式最大结果数量。
    pub maximum_items: usize,
    /// Whether Vulcan Code must download result resources before returning.
    /// Vulcan Code 是否必须在返回前下载结果资源。
    pub auto_download: bool,
    /// Stable AI-visible local-path result field.
    /// 稳定 AI 可见本地路径结果字段。
    pub ai_result_path: String,
}

/// Router-owned source used to register one AI-visible tool.
/// 用于注册一个 AI 可见工具的 Router 所有事实源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterClientToolContract {
    /// Stable AI-visible tool name.
    /// 稳定 AI 可见工具名称。
    pub name: String,
    /// AI-visible operation description.
    /// AI 可见操作说明。
    pub description: String,
    /// Sole VCP operation produced by this tool.
    /// 此工具生成的唯一 VCP 操作。
    pub operation: String,
    /// Complete AI-visible logical parameter schema.
    /// 完整 AI 可见逻辑参数 Schema。
    pub parameters: RouterParameterSchema,
    /// Closed projection for each logical parameter.
    /// 每个逻辑参数的封闭投影。
    pub mappings: Vec<RouterToolParameterMapping>,
    /// Local paths imported automatically by Vulcan Code.
    /// 由 Vulcan Code 自动导入的本地路径。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_inputs: Vec<RouterToolResourceInput>,
    /// Normalized output and download behavior.
    /// 规范化输出与下载行为。
    pub output: RouterToolOutputContract,
}

/// Stable immutable reference for one Router capability contract.
/// 一个 Router 能力合同的稳定不可变引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterCapabilityContractReference {
    /// Stable contract identifier.
    /// 稳定合同标识。
    pub id: String,
    /// Atomic catalog revision that published the contract.
    /// 发布合同的原子目录修订。
    pub catalog_revision: u64,
    /// Semantic content hash.
    /// 语义内容哈希。
    pub content_hash: String,
}

/// Model capability subset required by the Vulcan Code tool bridge.
/// Vulcan Code 工具桥接所需的模型能力子集。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RouterModelToolCapabilities {
    /// Optional Router-owned AI tool contract.
    /// 可选 Router 所有 AI 工具合同。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_tool: Option<RouterClientToolContract>,
}

/// Service capability subset reserved for Router-owned AI tool contracts.
/// 为 Router 所有 AI 工具合同保留的服务能力子集。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct RouterServiceToolCapabilities {
    /// Optional Router-owned AI tool contract.
    /// 可选 Router 所有 AI 工具合同。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_tool: Option<RouterClientToolContract>,
}

/// One complete exact-target capability contract returned by Router.
/// Router 返回的一个完整精确目标能力合同。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RouterCapabilityContract {
    /// Immutable contract reference.
    /// 不可变合同引用。
    pub reference: RouterCapabilityContractReference,
    /// Closed contract variant, currently `model` or `service`.
    /// 封闭合同变体，当前为 `model` 或 `service`。
    pub kind: String,
    /// Immutable provider instance identifier.
    /// 不可变供应商实例标识。
    pub provider_instance_id: String,
    /// Exact provider model identifier for model contracts.
    /// 模型合同使用的精确供应商模型标识。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_model_id: String,
    /// Exact provider service identifier for service contracts.
    /// 服务合同使用的精确供应商服务标识。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_service_id: String,
    /// Exact model or service offering identifier.
    /// 精确模型或服务产品标识。
    pub offering_id: String,
    /// Exact execution profile identifier.
    /// 精确执行规格标识。
    pub execution_profile_id: String,
    /// Sole VCP operation accepted by the contract.
    /// 合同接受的唯一 VCP 操作。
    pub operation: String,
    /// Required caller-owned credential-binding policy.
    /// 必需的调用方账号绑定策略。
    pub credential_binding: String,
    /// Model capability branch.
    /// 模型能力分支。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_capabilities: Option<RouterModelToolCapabilities>,
    /// Service capability branch.
    /// 服务能力分支。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_capabilities: Option<RouterServiceToolCapabilities>,
}

impl RouterCapabilityContract {
    /// Returns the sole Router-owned client tool contract for this capability.
    /// 返回此能力唯一的 Router 所有客户端工具合同。
    pub fn client_tool(&self) -> Option<&RouterClientToolContract> {
        self.model_capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.client_tool.as_ref())
            .or_else(|| {
                self.service_capabilities
                    .as_ref()
                    .and_then(|capabilities| capabilities.client_tool.as_ref())
            })
    }

    /// Validates the exact target identity and explicit credential policy required by Vulcan Code.
    /// 校验 Vulcan Code 要求的精确目标身份与显式账号策略。
    pub fn validate_tool_contract(&self) -> Result<(), String> {
        if self.reference.id.trim().is_empty()
            || self.reference.content_hash.trim().is_empty()
            || self.reference.catalog_revision == 0
            || self.provider_instance_id.trim().is_empty()
            || self.offering_id.trim().is_empty()
            || self.execution_profile_id.trim().is_empty()
            || self.operation.trim().is_empty()
        {
            return Err(String::from(
                "Router capability contract identity is incomplete.",
            ));
        }
        let model = !self.provider_model_id.trim().is_empty();
        let service = !self.provider_service_id.trim().is_empty();
        if model == service || self.kind != if model { "model" } else { "service" } {
            return Err(String::from(
                "Router capability contract must identify exactly one matching model or service target.",
            ));
        }
        if self.credential_binding != "explicit" {
            return Err(String::from(
                "Router capability contract does not require one explicit credential.",
            ));
        }
        let tool = self.client_tool().ok_or_else(|| {
            String::from("Router capability contract does not publish a client tool contract.")
        })?;
        if tool.name.trim().is_empty()
            || tool.description.trim().is_empty()
            || tool.operation != self.operation
            || tool.output.ai_result_path.trim().is_empty()
        {
            return Err(String::from(
                "Router client tool contract identity or output contract is incomplete.",
            ));
        }
        Ok(())
    }
}

/// One enabled Vulcan Code tool instance frozen to a contract and exact Router credential.
/// 冻结到一个合同与精确 Router 凭据的 Vulcan Code 工具实例。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedRouterToolBinding {
    /// Stable local binding identifier.
    /// 稳定本地绑定标识。
    pub id: String,
    /// Whether this exact tool binding is enabled.
    /// 此精确工具绑定是否启用。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// Exact Router credential identifier hidden from the AI.
    /// 对 AI 隐藏的精确 Router 凭据标识。
    pub credential_id: String,
    /// Immutable Router capability contract snapshot.
    /// 不可变 Router 能力合同快照。
    pub contract: RouterCapabilityContract,
}

impl PersistedRouterToolBinding {
    /// Validates one exact binding without rewriting immutable contract content.
    /// 校验一条精确绑定且不重写不可变合同内容。
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err(String::from("Router tool binding id is required."));
        }
        if self.credential_id.trim().is_empty() {
            return Err(String::from(
                "Router tool binding requires one exact credential id.",
            ));
        }
        self.contract.validate_tool_contract()
    }
}

/// Persisted Router connection and immutable tool bindings used by Vulcan Code.
/// Vulcan Code 使用的持久化 Router 连接与不可变工具绑定。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedRouterToolConfiguration {
    /// Whether Router-derived tools participate in this Vulcan Code runtime.
    /// Router 派生工具是否参与当前 Vulcan Code 运行时。
    #[serde(default, skip_serializing_if = "is_false")]
    pub enabled: bool,
    /// Router HTTP origin without a V1 route suffix.
    /// 不包含 V1 路由后缀的 Router HTTP 源地址。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Call-plane API key or `{env:NAME}` placeholder.
    /// 调用面 API 密钥或 `{env:NAME}` 占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Enabled immutable tool bindings.
    /// 已启用的不可变工具绑定集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<PersistedRouterToolBinding>,
}

impl PersistedRouterToolConfiguration {
    /// Returns whether the Router tool configuration is absent.
    /// 返回 Router 工具配置是否为空。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Resolves the configured call-plane secret without exposing it to tool schemas.
    /// 解析已配置的调用面秘密且不将其暴露到工具 Schema。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(Some(self.api_key.as_str()), "Vulcan Router API Key")
    }

    /// Normalizes mutable connection fields while preserving immutable contract snapshots byte-for-byte.
    /// 归一化可变连接字段，同时逐字节保留不可变合同快照。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        self.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        self.api_key = self.api_key.trim().to_string();
        for binding in &mut self.bindings {
            binding.id = binding.id.trim().to_string();
            binding.credential_id = binding.credential_id.trim().to_string();
        }
        self.bindings.sort_by(|left, right| left.id.cmp(&right.id));
        self != &original
    }
}

/// Returns whether an integer value is zero for serde omission.
/// 返回整数值是否为零以供 serde 省略。
fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

/// Returns whether an unsigned size value is zero for serde omission.
/// 返回无符号大小值是否为零以供 serde 省略。
fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}
