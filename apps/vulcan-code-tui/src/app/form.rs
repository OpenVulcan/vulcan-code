// Multi-field input using the existing prompt overlay.
// 使用现有输入浮层收集多字段输入。

use serde_json::Value;

use super::App;
use super::action::{PromptDialog, PromptThen, UiAction, perform};
use crate::backend::Command;
use crate::model::NoticeSeverity;
use crate::service_api::ServiceCall;
use crate::tr;

/// One field of a confirmed WebUI request, displayed as a separate prompt.
/// 已确认 WebUI 请求中的一个字段，显示为独立输入框。
#[derive(Debug, Clone)]
pub struct FormField {
    /// Localized field title shown above the prompt.
    /// 显示在输入框上方的本地化字段标题。
    pub title: String,
    /// Exact pointer into the request schema, including declared array indexes.
    /// 请求结构中的准确指针，包含已声明的数组下标。
    pub pointer: String,
    /// Secret input is masked and is discarded with the transient form.
    /// 机密输入被遮蔽，并随临时表单释放。
    pub secret: bool,
    /// Whether an empty input is accepted by this request field.
    /// 此请求字段是否接受空输入。
    pub optional: bool,
    /// Converts visible input to the exact request field type; invalid input stays in the form.
    /// 将可见输入转换为准确请求字段类型；无效输入保留在表单中。
    pub parse: fn(&str) -> Option<Value>,
    /// Checks the parsed field against the frozen request, such as a ready-file whitelist.
    /// 按冻结请求检查已解析字段，例如就绪文件允许列表。
    pub validate: fn(&Value, &Value) -> bool,
}

/// Sequential request input; `save` converts the completed document to one service call.
/// 顺序收集请求输入；`save` 将完整文档转换为一次服务调用。
#[derive(Clone)]
pub struct Form {
    /// Request initialized with the exact fields defined by the upstream API.
    /// 按上游 API 定义的精确字段初始化的请求。
    pub request: Value,
    /// Ordered field definitions selected by the request builder.
    /// 请求构建器选出的有序字段定义。
    pub fields: Vec<FormField>,
    /// Zero-based current field; the field count marks final dispatch.
    /// 从零开始的当前字段；达到字段数时执行最终发送。
    pub index: usize,
    /// Exact API builder or reviewed method metadata used after all fields pass.
    /// 全部字段通过后使用的准确 API 构建器或已审查方法元数据。
    pub save: FormSubmit,
}

/// Builds a form call or fills a previously reviewed call without changing its method.
/// 构建表单调用，或填充已审查调用而不改变其方法。
#[derive(Debug, Clone)]
pub enum FormSubmit {
    /// Converts the completed request with its statically selected API builder.
    /// 使用静态选择的 API 构建器转换完整请求。
    Build(fn(Value) -> ServiceCall),
    /// Stores method metadata; the only copy of entered secrets remains in Form::request.
    /// 保存方法元数据；输入密钥仅在 Form::request 中保留一份。
    Prepared(ServiceCall),
}

impl std::fmt::Debug for Form {
    /// Omits request contents because intermediate input may contain a secret.
    /// 省略请求内容，因为中间输入可能包含机密。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Form")
            .field("index", &self.index)
            .finish()
    }
}

impl Form {
    /// Opens the current field, or dispatches the completed request through `perform`.
    /// 打开当前字段，或通过 `perform` 发送已经完成的请求。
    pub fn open(self, app: &mut App) {
        if let Some(field) = self.fields.get(self.index).cloned() {
            // Only non-secret defaults are echoed into the visible prompt.
            // 仅非机密默认值回显到可见输入框。
            let input = if field.secret {
                String::new()
            } else {
                match self.request.pointer(&field.pointer) {
                    Some(Value::String(value)) => value.clone(),
                    Some(Value::Bool(value)) => value.to_string(),
                    Some(Value::Number(value)) => value.to_string(),
                    Some(Value::Array(values)) => values
                        .iter()
                        .map(|value| {
                            if let Some(value) = value.as_str() {
                                value.to_string()
                            } else {
                                format!(
                                    "{}={}",
                                    value["key"].as_str().unwrap_or_default(),
                                    value["value"].as_str().unwrap_or_default()
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                    _ => String::new(),
                }
            };
            perform(
                app,
                UiAction::Prompt(Box::new(PromptDialog::new(
                    field.title.clone(),
                    tr!(
                        "tui.form.step",
                        current = self.index + 1,
                        total = self.fields.len()
                    ),
                    input,
                    PromptThen::Form(Box::new(self)),
                    field.secret,
                ))),
            );
        } else {
            // Move the transient request into one send, including installation-only secrets.
            // 将临时请求移入一次发送，包括仅安装时使用的密钥。
            let call = match self.save {
                FormSubmit::Build(build) => build(self.request),
                FormSubmit::Prepared(mut call) => {
                    call.params = self.request;
                    call
                }
            };
            perform(app, UiAction::Send(Command::Call(call)));
        }
    }

    /// Stores `input` at the current verified pointer and advances; invalid schemas report error.
    /// 在当前已确认的指针保存 `input` 并前进；无效结构明确报告错误。
    pub fn accept(mut self, app: &mut App, input: String) {
        // Field.
        // 准确字段定义。
        let field = &self.fields[self.index];
        // Parsed.
        // 通过解析的输入值。
        let Some(parsed) =
            (field.parse)(&input).filter(|parsed| (field.validate)(&self.request, parsed))
        else {
            self.open(app);
            if let Some(super::overlay::Overlay::Prompt(dialog)) = &mut app.overlay {
                dialog.input = input;
                dialog.error = Some(tr!("tui.form.invalid"));
            }
            return;
        };
        match self.request.pointer_mut(&field.pointer) {
            Some(value) => *value = parsed,
            None => {
                app.toast(NoticeSeverity::Error, tr!("tui.form.invalid"));
                return;
            }
        }
        self.index += 1;
        self.open(app);
    }
}
