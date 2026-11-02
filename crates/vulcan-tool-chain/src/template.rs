//! `${{ }}` templates inside step arguments.
//! 步骤参数中的 `${{ }}` 模板。
//!
//! A string that is exactly one `${{ expr }}` keeps the expression's JSON type, so
//! `"${{ fromJSON(steps.a.output).count }}"` can yield a number. Any other string interpolates the
//! expression results as text.
//! 恰好只有一个 `${{ expr }}` 的字符串保留表达式的 JSON 类型，因此
//! `"${{ fromJSON(steps.a.output).count }}"` 可以得到数字；其他字符串按文本插值表达式结果。

use serde_json::{Map, Value};

use crate::expr::{EvalContext, Expr, evaluate, parse_expression, referenced_steps, to_text};

/// Argument tree with parsed templates.
/// 已解析模板的参数树。
#[derive(Debug, Clone)]
pub(crate) enum ArgNode {
    /// Value without any template, used verbatim.
    /// 不含模板的值，原样使用。
    Static(Value),
    /// String that is exactly one expression.
    /// 恰好为一个表达式的字符串。
    Single(Expr),
    /// String mixing text and expressions.
    /// 混合文本与表达式的字符串。
    Interpolated(Vec<Part>),
    /// Object whose fields contain templates.
    /// 字段中含模板的对象。
    Object(Vec<(String, ArgNode)>),
    /// Array whose items contain templates.
    /// 元素中含模板的数组。
    Array(Vec<ArgNode>),
}

/// One piece of an interpolated string.
/// 插值字符串中的一段。
#[derive(Debug, Clone)]
pub(crate) enum Part {
    /// Literal text.
    /// 字面文本。
    Text(String),
    /// Expression whose result is inserted as text.
    /// 结果以文本插入的表达式。
    Expr(Expr),
}

/// Parses templates in an argument value; errors carry the JSON path.
/// 解析参数值中的模板；错误携带 JSON 路径。
pub(crate) fn parse_arguments(value: &Value, path: &str, errors: &mut Vec<String>) -> ArgNode {
    match value {
        Value::String(text) => match split_template(text) {
            Ok(None) => ArgNode::Static(value.clone()),
            Ok(Some(parts)) => {
                let mut parsed = Vec::with_capacity(parts.len());
                for part in parts {
                    match part {
                        RawPart::Text(text) => parsed.push(Part::Text(text)),
                        RawPart::Expr(source) => match parse_expression(&source) {
                            Ok(expr) => parsed.push(Part::Expr(expr)),
                            Err(error) => {
                                errors.push(format!(
                                    "{path}: in `${{{{ {} }}}}`: {error}",
                                    source.trim()
                                ));
                                return ArgNode::Static(value.clone());
                            }
                        },
                    }
                }
                if let [Part::Expr(expr)] = parsed.as_slice() {
                    ArgNode::Single(expr.clone())
                } else {
                    ArgNode::Interpolated(parsed)
                }
            }
            Err(error) => {
                errors.push(format!("{path}: {error}"));
                ArgNode::Static(value.clone())
            }
        },
        Value::Object(map) => {
            let fields = map
                .iter()
                .map(|(key, item)| {
                    (
                        key.clone(),
                        parse_arguments(item, &format!("{path}.{key}"), errors),
                    )
                })
                .collect::<Vec<_>>();
            if fields
                .iter()
                .all(|(_, node)| matches!(node, ArgNode::Static(_)))
            {
                ArgNode::Static(value.clone())
            } else {
                ArgNode::Object(fields)
            }
        }
        Value::Array(items) => {
            let nodes = items
                .iter()
                .enumerate()
                .map(|(index, item)| parse_arguments(item, &format!("{path}[{index}]"), errors))
                .collect::<Vec<_>>();
            if nodes.iter().all(|node| matches!(node, ArgNode::Static(_))) {
                ArgNode::Static(value.clone())
            } else {
                ArgNode::Array(nodes)
            }
        }
        _ => ArgNode::Static(value.clone()),
    }
}

/// Unparsed piece of a template string.
/// 模板字符串中尚未解析的一段。
enum RawPart {
    Text(String),
    Expr(String),
}

/// Splits a string at `${{ … }}` markers; `None` means no template.
/// 在 `${{ … }}` 标记处切分字符串；`None` 表示不含模板。
fn split_template(text: &str) -> Result<Option<Vec<RawPart>>, String> {
    if !text.contains("${{") {
        return Ok(None);
    }
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        if start > 0 {
            parts.push(RawPart::Text(rest[..start].to_string()));
        }
        let body = &rest[start + 3..];
        let end = find_template_end(body)
            .ok_or_else(|| String::from("`${{` is missing its closing `}}`"))?;
        parts.push(RawPart::Expr(body[..end].to_string()));
        rest = &body[end + 2..];
    }
    if !rest.is_empty() {
        parts.push(RawPart::Text(rest.to_string()));
    }
    Ok(Some(parts))
}

/// Finds the closing `}}` outside single-quoted strings.
/// 在单引号字符串之外查找闭合的 `}}`。
fn find_template_end(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let mut quoted = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // A doubled quote toggles twice, so escaped quotes need no special case.
            // 连写的两个引号会切换两次，因此转义引号无需特殊处理。
            b'\'' => quoted = !quoted,
            b'}' if !quoted && bytes.get(index + 1) == Some(&b'}') => return Some(index),
            _ => {}
        }
        index += 1;
    }
    None
}

/// Collects step ids referenced anywhere in an argument tree.
/// 收集参数树中任意位置引用的步骤标识。
pub(crate) fn argument_references<'a>(node: &'a ArgNode, out: &mut Vec<&'a str>) {
    match node {
        ArgNode::Static(_) => {}
        ArgNode::Single(expr) => referenced_steps(expr, out),
        ArgNode::Interpolated(parts) => {
            for part in parts {
                if let Part::Expr(expr) = part {
                    referenced_steps(expr, out);
                }
            }
        }
        ArgNode::Object(fields) => fields
            .iter()
            .for_each(|(_, item)| argument_references(item, out)),
        ArgNode::Array(items) => items.iter().for_each(|item| argument_references(item, out)),
    }
}

/// Resolves an argument tree into concrete JSON.
/// 将参数树解析为具体 JSON。
pub(crate) fn resolve_arguments(
    node: &ArgNode,
    context: &EvalContext<'_>,
) -> Result<Value, String> {
    match node {
        ArgNode::Static(value) => Ok(value.clone()),
        ArgNode::Single(expr) => evaluate(expr, context),
        ArgNode::Interpolated(parts) => {
            let mut text = String::new();
            for part in parts {
                match part {
                    Part::Text(literal) => text.push_str(literal),
                    Part::Expr(expr) => text.push_str(&to_text(&evaluate(expr, context)?)),
                }
            }
            Ok(Value::String(text))
        }
        ArgNode::Object(fields) => {
            let mut map = Map::with_capacity(fields.len());
            for (key, item) in fields {
                map.insert(key.clone(), resolve_arguments(item, context)?);
            }
            Ok(Value::Object(map))
        }
        ArgNode::Array(items) => items
            .iter()
            .map(|item| resolve_arguments(item, context))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
    }
}
