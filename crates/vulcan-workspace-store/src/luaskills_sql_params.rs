//! Parser for the LuaSkills host-callback SQL parameter encoding.
//! LuaSkills 宿主回调 SQL 参数编码的解析器。
//!
//! When LuaSkills runs its providers in host-callback mode it forwards the parameters a skill supplied
//! verbatim, in an encoding the engine's own provider defines rather than one this host invented. The
//! encoding was read from the pinned `luaskills 0.5.5` source (`providers/sqlite.rs`) and is reproduced
//! here so that every host path produces the same answer a skill observes.
//! 当 LuaSkills 以宿主回调模式运行其 provider 时，它把技能提供的参数原样转发，编码由引擎自身的 provider 定义，
//! 而非本宿主发明。该编码读取自锁定的 `luaskills 0.5.5` 源码（`providers/sqlite.rs`）并在此重现，使宿主答案不会
//! 与技能实际观察到的宿主答案产生差异。
//!
//! The encoding has two forms, and a value is either one:
//! 该编码有两种形式，而一个值必属其一：
//!
//! - A plain JSON scalar: `null`, a boolean, a number, or a string. A number becomes an `i64` when it fits,
//!   an unsigned integer that fits becomes an `i64`, and anything else becomes a float.
//! - 普通 JSON 标量：`null`、布尔、数字或字符串。数字在可容纳时成为 `i64`，可容纳的无符号整数成为 `i64`，
//!   其余成为浮点数。
//! - An object carrying a `kind` key, whose value is one of `null`, `bool`, `int64`, `float64`, `string` or
//!   `bytes`. The kind is compared after trimming and lowercasing.
//! - 携带 `kind` 键的对象，其值为 `null`、`bool`、`int64`、`float64`、`string` 或 `bytes` 之一。`kind` 在去除
//!   首尾空白并转为小写后比较。
//!
//! Two rejection rules matter as much as the accepted forms, because a host that accepts more than the
//! engine does would let a skill depend on something the other modes refuse:
//! 两条拒绝规则与接受的形式同样重要，因为接受得比引擎更多的宿主会让技能依赖于其他模式所拒绝的东西：
//!
//! - A `params_json` key is refused outright rather than parsed.
//! - `params_json` 键被直接拒绝而非解析。
//! - A batch's `items` must be a non-empty array of arrays, so a zero-row batch is an error rather than a
//!   silent no-op.
//! - 批次的 `items` 必须是非空数组的数组，因此零行批次是错误而非静默的空操作。

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde_json::{Map, Value};
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Parses one request's single-statement parameter array.
/// 解析单个请求的单语句参数数组。
///
/// # Parameters / 参数
/// `input` is the callback request's `input` object.
/// `input` 是回调请求的 `input` 对象。
///
/// # Returns / 返回值
/// Returns the parameters in engine form; an absent `params` yields an empty vector, exactly as the engine
/// treats it.
/// 返回引擎形式的参数；`params` 缺失时返回空向量，与引擎的处理完全一致。
pub fn parse_params(input: &Value) -> Result<Vec<SqliteValue>, String> {
    if input.get("params_json").is_some() {
        return Err(String::from(
            "params_json is no longer supported; use params",
        ));
    }
    let Some(params) = input.get("params") else {
        return Ok(Vec::new());
    };
    let array = params
        .as_array()
        .ok_or_else(|| String::from("params must be an array"))?;
    array
        .iter()
        .enumerate()
        .map(|(index, item)| parse_param(item, &format!("params[{index}]")))
        .collect()
}

/// Parses one request's batch parameter matrix.
/// 解析单个请求的批次参数矩阵。
///
/// # Parameters / 参数
/// `input` is the callback request's `input` object.
/// `input` 是回调请求的 `input` 对象。
///
/// # Returns / 返回值
/// Returns one engine-form parameter row per input row, or an explicit error when `items` is missing, not an
/// array, empty, or contains a non-array row.
/// 每个输入行返回一个引擎形式的参数行；当 `items` 缺失、非数组、为空或含非数组行时返回明确错误。
pub fn parse_batch(input: &Value) -> Result<Vec<Vec<SqliteValue>>, String> {
    let items = input
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("items must be an array of arrays"))?;
    if items.is_empty() {
        return Err(String::from("items must not be empty"));
    }
    items
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let row_items = row
                .as_array()
                .ok_or_else(|| format!("items[{row_index}] must be an array"))?;
            row_items
                .iter()
                .enumerate()
                .map(|(column_index, item)| {
                    parse_param(item, &format!("items[{row_index}][{column_index}]"))
                })
                .collect()
        })
        .collect()
}

/// Parses one parameter in either of the two accepted forms.
/// 以两种可接受形式之一解析单个参数。
///
/// # Parameters / 参数
/// `value` is the parameter value and `field_name` names it in diagnostics.
/// `value` 是参数值，`field_name` 在诊断中命名它。
///
/// # Returns / 返回值
/// Returns the engine value, or an explicit error naming the offending field.
/// 返回引擎值，或命名出错字段的明确错误。
fn parse_param(value: &Value, field_name: &str) -> Result<SqliteValue, String> {
    match value {
        Value::Object(object) if object.contains_key("kind") => {
            parse_typed_param(object, field_name)
        }
        other => parse_scalar_param(other, field_name),
    }
}

/// Parses one plain JSON scalar, following the engine's numeric widening rules exactly.
/// 解析一个普通 JSON 标量，精确遵循引擎的数值拓宽规则。
///
/// # Parameters / 参数
/// `value` is the scalar and `field_name` names it in diagnostics.
/// `value` 是标量，`field_name` 在诊断中命名它。
///
/// # Returns / 返回值
/// Returns the engine value, or an explicit error for a non-scalar or an unrepresentable number.
/// 返回引擎值；对标量之外的值或无法表示的数值返回明确错误。
fn parse_scalar_param(value: &Value, field_name: &str) -> Result<SqliteValue, String> {
    match value {
        Value::Null => Ok(SqliteValue::Null),
        Value::Bool(flag) => Ok(SqliteValue::Integer(i64::from(*flag))),
        Value::Number(number) => {
            if let Some(int_value) = number.as_i64() {
                return Ok(SqliteValue::Integer(int_value));
            }
            if let Some(unsigned) = number.as_u64() {
                let converted = i64::try_from(unsigned).map_err(|_| {
                    format!("{field_name} contains an unsigned integer larger than i64")
                })?;
                return Ok(SqliteValue::Integer(converted));
            }
            if let Some(float_value) = number.as_f64() {
                return Ok(SqliteValue::Real(float_value));
            }
            Err(format!(
                "{field_name} contains an unsupported numeric value"
            ))
        }
        Value::String(text) => Ok(SqliteValue::Text(text.clone())),
        _ => Err(format!("{field_name} must contain only scalar values")),
    }
}

/// Parses one `kind`-tagged object.
/// 解析一个带 `kind` 标记的对象。
///
/// # Parameters / 参数
/// `object` is the tagged object and `field_name` names it in diagnostics.
/// `object` 是标记对象，`field_name` 在诊断中命名它。
///
/// # Returns / 返回值
/// Returns the engine value, or an explicit error. The `bytes` kind is refused with a specific message
/// because base64 decoding is not yet available on this crate's dependency surface; that is a recorded gap,
/// not a deliberate restriction, and it must be closed before the host-callback mode is enabled.
/// 返回引擎值或明确错误。`bytes` kind 以特定消息被拒绝，因为本 crate 的依赖面尚无 base64 解码；这是已记录的
/// 缺口而非刻意的限制，必须在启用宿主回调模式前补齐。
fn parse_typed_param(object: &Map<String, Value>, field_name: &str) -> Result<SqliteValue, String> {
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field_name}.kind is required for typed parameters"))?;
    match kind.trim().to_ascii_lowercase().as_str() {
        "null" => Ok(SqliteValue::Null),
        "bool" => object
            .get("value")
            .and_then(Value::as_bool)
            .map(|flag| SqliteValue::Integer(i64::from(flag)))
            .ok_or_else(|| format!("{field_name}.value must be a bool")),
        "int64" => object
            .get("value")
            .and_then(Value::as_i64)
            .map(SqliteValue::Integer)
            .ok_or_else(|| format!("{field_name}.value must be an int64")),
        "float64" => object
            .get("value")
            .and_then(Value::as_f64)
            .map(SqliteValue::Real)
            .ok_or_else(|| format!("{field_name}.value must be a float64")),
        "string" => object
            .get("value")
            .and_then(Value::as_str)
            .map(|text| SqliteValue::Text(text.to_string()))
            .ok_or_else(|| format!("{field_name}.value must be a string")),
        "bytes" => parse_bytes_param(object, field_name),
        other => Err(format!(
            "{field_name}.kind `{other}` is not a known parameter kind"
        )),
    }
}

/// Parses the typed `bytes` kind, which the engine accepts in either of two encodings.
/// 解析 typed `bytes` 种类，引擎以两种编码之一接受它。
///
/// A `base64` string is decoded as standard base64; otherwise `value` must be an array of unsigned integers
/// each fitting in a byte. Both are accepted because the engine accepts both, and a host that accepted fewer
/// would break a skill that works in the other modes.
/// `base64` 字符串按标准 base64 解码；否则 `value` 必须是每个都能容纳进一个字节的无符号整数数组。二者都被接受
/// 是因为引擎二者都接受，而接受更少的宿主会破坏在其他模式下可用的技能。
///
/// # Parameters / 参数
/// `object` is the tagged object and `field_name` names it in diagnostics.
/// `object` 是标记对象，`field_name` 在诊断中命名它。
///
/// # Returns / 返回值
/// Returns the blob value, or an explicit error naming the offending field or index.
/// 返回 blob 值，或命名出错字段或下标的明确错误。
fn parse_bytes_param(object: &Map<String, Value>, field_name: &str) -> Result<SqliteValue, String> {
    if let Some(encoded) = object.get("base64").and_then(Value::as_str) {
        let decoded = BASE64_STANDARD
            .decode(encoded)
            .map_err(|error| format!("{field_name}.base64 is invalid: {error}"))?;
        return Ok(SqliteValue::Blob(decoded));
    }
    let array = object
        .get("value")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{field_name}.value must be a byte array or provide base64"))?;
    let mut bytes = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let byte = item
            .as_u64()
            .ok_or_else(|| format!("{field_name}.value[{index}] must be an unsigned integer"))?;
        let converted =
            u8::try_from(byte).map_err(|_| format!("{field_name}.value[{index}] exceeds u8"))?;
        bytes.push(converted);
    }
    Ok(SqliteValue::Blob(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies an absent `params` is empty and a `params_json` key is refused.
    /// 验证 `params` 缺失时为空，而 `params_json` 键被拒绝。
    #[test]
    fn absent_params_are_empty_and_params_json_is_refused() {
        assert!(parse_params(&json!({})).expect("absent params").is_empty());
        let error = parse_params(&json!({ "params_json": "[1]" }))
            .expect_err("params_json must be refused");
        assert!(error.contains("params_json"), "unexpected error: {error}");
        assert!(parse_params(&json!({ "params": 7 })).is_err());
    }

    /// Verifies every accepted scalar form maps to the engine's widening rules.
    /// 验证每种可接受的标量形式都映射到引擎的拓宽规则。
    #[test]
    fn scalar_params_follow_the_engine_widening_rules() {
        let parsed = parse_params(&json!({
            "params": [null, true, false, 7, -7, 9_007_199_254_740_993u64, 1.5, "text"]
        }))
        .expect("parse scalar params");

        assert_eq!(parsed[0], SqliteValue::Null);
        assert_eq!(parsed[1], SqliteValue::Integer(1));
        assert_eq!(parsed[2], SqliteValue::Integer(0));
        assert_eq!(parsed[3], SqliteValue::Integer(7));
        assert_eq!(parsed[4], SqliteValue::Integer(-7));
        // An unsigned integer that fits in i64 stays an integer even when f64 could not hold it exactly; the
        // engine widens to f64 only when neither integer reading succeeds.
        // 可容纳进 i64 的无符号整数即使 f64 无法精确表示也保持为整数；引擎仅在两种整数读法都失败时才拓宽为 f64。
        assert_eq!(parsed[5], SqliteValue::Integer(9_007_199_254_740_993));
        assert_eq!(parsed[6], SqliteValue::Real(1.5));
        assert_eq!(parsed[7], SqliteValue::Text(String::from("text")));

        let overflow = parse_params(&json!({ "params": [u64::MAX] }))
            .expect_err("an unsigned integer larger than i64 must be refused");
        assert!(
            overflow.contains("params[0]"),
            "unexpected error: {overflow}"
        );
    }

    /// Verifies every accepted typed kind maps to the engine form.
    /// 验证每种可接受的 typed kind 都映射到引擎形式。
    #[test]
    fn typed_params_map_by_kind() {
        let parsed = parse_params(&json!({
            "params": [
                { "kind": "null" },
                { "kind": " Bool ", "value": true },
                { "kind": "INT64", "value": 12 },
                { "kind": "float64", "value": 2.5 },
                { "kind": "string", "value": "value" }
            ]
        }))
        .expect("parse typed params");

        assert_eq!(parsed[0], SqliteValue::Null);
        assert_eq!(parsed[1], SqliteValue::Integer(1));
        assert_eq!(parsed[2], SqliteValue::Integer(12));
        assert_eq!(parsed[3], SqliteValue::Real(2.5));
        assert_eq!(parsed[4], SqliteValue::Text(String::from("value")));
    }

    /// Verifies malformed typed parameters are refused with the field named.
    /// 验证格式错误的 typed 参数会被拒绝并命名该字段。
    #[test]
    fn malformed_typed_params_name_the_field() {
        let error = parse_params(&json!({ "params": [{ "kind": "int64", "value": "12" }] }))
            .expect_err("a string value for int64 must be refused");
        assert!(error.contains("params[0]"), "unexpected error: {error}");

        let unknown = parse_params(&json!({ "params": [{ "kind": "timestamp" }] }))
            .expect_err("an unknown kind must be refused");
        assert!(unknown.contains("timestamp"), "unexpected error: {unknown}");

        // The typed `bytes` kind is supported in both encodings the engine accepts.
        // typed `bytes` 种类以引擎接受的两种编码均受支持。
        let base64_bytes = parse_params(&json!({
            "params": [{ "kind": "bytes", "base64": "AAEC" }]
        }))
        .expect("decode a base64 blob");
        assert_eq!(base64_bytes[0], SqliteValue::Blob(vec![0, 1, 2]));

        let array_bytes = parse_params(&json!({
            "params": [{ "kind": "bytes", "value": [255, 0, 7] }]
        }))
        .expect("decode a byte array");
        assert_eq!(array_bytes[0], SqliteValue::Blob(vec![255, 0, 7]));

        let bad_base64 = parse_params(&json!({
            "params": [{ "kind": "bytes", "base64": "not base64!" }]
        }))
        .expect_err("invalid base64 must be refused");
        assert!(
            bad_base64.contains("base64"),
            "unexpected error: {bad_base64}"
        );

        let out_of_range = parse_params(&json!({
            "params": [{ "kind": "bytes", "value": [256] }]
        }))
        .expect_err("a value beyond u8 must be refused");
        assert!(
            out_of_range.contains("value[0]"),
            "unexpected error: {out_of_range}"
        );
    }

    /// Verifies the batch matrix rules, including the zero-row rejection.
    /// 验证批次矩阵规则，含零行拒绝。
    #[test]
    fn batch_params_require_a_non_empty_matrix() {
        let parsed = parse_batch(&json!({ "items": [[1, "a"], [2, null]] })).expect("parse batch");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0][1], SqliteValue::Text(String::from("a")));
        assert_eq!(parsed[1][1], SqliteValue::Null);

        assert!(
            parse_batch(&json!({ "items": [] })).is_err(),
            "empty items must be refused"
        );
        assert!(
            parse_batch(&json!({ "items": [7] })).is_err(),
            "a non-array row must be refused"
        );
        assert!(
            parse_batch(&json!({})).is_err(),
            "absent items must be refused"
        );
    }
}
