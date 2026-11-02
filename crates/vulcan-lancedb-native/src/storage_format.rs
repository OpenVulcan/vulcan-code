//! The v1 Lance data-format baseline: pinning, verification and blob prohibition.
//! v1 Lance 数据格式基线：固定、校验与 Blob 禁止。
//!
//! The plan fixes v1 tables at `LanceFileVersion::V2_1` and forbids the `V2_2` upgrade that
//! `lancedb` performs automatically when a schema carries a `lance.blob.v2` column. This module
//! owns both halves of that requirement in one place so neither can drift.
//!
//! 方案把 v1 表固定为 `LanceFileVersion::V2_1`，并禁止当 Schema 带有 `lance.blob.v2` 列时
//! `lancedb` 自动执行的 `V2_2` 升级。本模块在一处同时承担该要求的两半，使二者都不会漂移。
//!
//! Why the option is applied at connect time / 为何该选项在连接时施加:
//!
//! `lancedb 0.38.0`'s `CreateTableBuilder` exposes **no** `data_storage_version` method. The only
//! reachable knob on a listing database is the `new_table_data_storage_version` connection option,
//! read by `ListingDatabaseOptions::parse_from_map` and applied as the **lowest**-precedence input
//! of three:
//!
//! `lancedb 0.38.0` 的 `CreateTableBuilder` **没有** `data_storage_version` 方法。listing 数据库上
//! 唯一可达的旋钮是连接选项 `new_table_data_storage_version`，由
//! `ListingDatabaseOptions::parse_from_map` 读取，并作为三个输入中**优先级最低**者应用：
//!
//! ```text
//! overrides.data_storage_version          (request level)
//!   .or(write_params.data_storage_version)  (table level)
//!   .or(new_table_config.data_storage_version)  (connection level)
//! ```
//!
//! Because a higher-precedence request or write parameter could therefore override the connection
//! option, setting the option alone is **not** a guarantee. [`verify_table_format`] closes that gap
//! by reading the created table's resolved storage format back from its manifest, turning "we
//! asked for `V2_1`" into "the table on disk is `V2_1`". A failure to verify is an error, never a
//! silent pass.
//!
//! 因此更高优先级的请求或写入参数可以覆盖连接选项，仅设置该选项**并不**构成保证。
//! [`verify_table_format`] 通过在建表后从 manifest 读回该表**实际解析出的**存储格式来弥补这一缺口，
//! 把"我们请求了 `V2_1`"变成"磁盘上的表就是 `V2_1`"。校验失败是错误，绝不静默通过。
//!
//! Why the version is compared as text / 为何以文本比较版本:
//!
//! Both `LanceFileVersion` and `ConcreteFileVersion` are defined in `lance_file::version`, a crate
//! that `lancedb` does not re-export, so neither type can be named without taking a direct
//! dependency on a transitive crate and risking version skew. The resolved format is therefore
//! compared through its `Debug` rendering. This is deliberately a strict equality check against one
//! expected string: if a future `lancedb` renames the variant, the check fails loudly rather than
//! silently accepting a format change.
//!
//! `LanceFileVersion` 与 `ConcreteFileVersion` 都定义在 `lance_file::version` 中，而 `lancedb` 未
//! 重导出该 crate，因此在不直接依赖传递 crate（并承担版本偏移风险）的前提下无法命名这两个类型。
//! 因此解析出的格式通过其 `Debug` 渲染来比较。此处刻意采用对单个期望字符串的严格相等检查：若未来
//! `lancedb` 重命名该变体，检查会**响亮失败**，而不是静默接受一次格式变化。

use arrow_schema::Schema;
use lancedb::Table;

use crate::error::{EngineResult, LanceDbEngineError};

/// Connection option read by `ListingDatabaseOptions::parse_from_map`.
/// 由 `ListingDatabaseOptions::parse_from_map` 读取的连接选项。
///
/// Taken from `lancedb`'s own public constant rather than duplicated as a literal, so a rename in
/// the dependency surfaces as a compile error instead of a silently ignored option.
///
/// 取自 `lancedb` 自身的公开常量而非复制字面量，使依赖中的重命名表现为编译错误，而不是被静默忽略的选项。
pub const PINNED_STORAGE_VERSION_OPTION: &str =
    lancedb::database::listing::OPT_NEW_TABLE_STORAGE_VERSION;

/// The pinned v1 storage version, in `LanceFileVersion`'s `FromStr` spelling.
/// 固定的 v1 存储版本，采用 `LanceFileVersion` 的 `FromStr` 拼写。
///
/// `"2.1"` is the concrete form. The aliases `"stable"` and `"next"` both resolve at storage time
/// to whatever the dependency currently considers current, which is exactly the drift this pin
/// exists to prevent, so neither is ever used here.
///
/// `"2.1"` 是具体形式。别名 `"stable"` 与 `"next"` 都会在存储时解析为依赖当时认为的当前版本，而这
/// 正是本固定项要防止的漂移，因此此处绝不使用它们。
pub const PINNED_STORAGE_VERSION_VALUE: &str = "2.1";

/// `Debug` rendering of the resolved [`ConcreteFileVersion::V2_1`] this crate requires.
/// 本 crate 所要求的已解析版本（`ConcreteFileVersion::V2_1`）的 `Debug` 渲染。
///
/// [`ConcreteFileVersion::V2_1`]: https://docs.rs/lance-file
pub const PINNED_RESOLVED_FORMAT_DEBUG: &str = "V2_1";

/// Schema metadata key whose presence forces `lancedb` to upgrade a table to `V2_2`.
/// Schema 元数据键；其存在会迫使 `lancedb` 把表升级到 `V2_2`。
pub const BLOB_V2_METADATA_KEY: &str = "lance.blob.v2";

/// Returns the refusal reason when a schema would force the forbidden `V2_2` upgrade.
/// 当某个 Schema 会迫使执行被禁止的 `V2_2` 升级时，返回拒绝理由。
///
/// `lancedb`'s `ensure_blob_storage_version` force-bumps the format when a schema carries the
/// blob metadata key, so this is checked against both schema-level and field-level metadata before
/// any table is created. Returning `Option` rather than a `bool` keeps the diagnostic specific.
///
/// 当 Schema 带有 Blob 元数据键时，`lancedb` 的 `ensure_blob_storage_version` 会强制提升格式，
/// 因此在创建任何表之前，这里会同时检查 Schema 级与字段级元数据。返回 `Option` 而非 `bool`
/// 使诊断信息更具体。
pub fn blob_v2_refusal_reason(schema: &Schema) -> Option<String> {
    if schema.metadata().contains_key(BLOB_V2_METADATA_KEY) {
        return Some(format!(
            "schema metadata contains the reserved key '{BLOB_V2_METADATA_KEY}', which would force the table to Lance {V2_2_ALIAS}",
        ));
    }

    for field in schema.fields() {
        if field.metadata().contains_key(BLOB_V2_METADATA_KEY) {
            return Some(format!(
                "field '{}' metadata contains the reserved key '{BLOB_V2_METADATA_KEY}', which would force the table to Lance {V2_2_ALIAS}",
                field.name(),
            ));
        }
    }

    None
}

/// Name of the forbidden upgraded format, used only in diagnostics.
/// 被禁止的升级格式名称，仅用于诊断。
const V2_2_ALIAS: &str = "V2_2";

/// Reads back the created table's resolved storage format and requires it to be the pinned one.
/// 读回已创建表的实际存储格式，并要求其为固定版本。
///
/// A table that is not a native Lance table (for example a namespace-backed remote table) exposes
/// no manifest, so there is nothing this process can verify; that case is reported as
/// [`LanceDbEngineError::Internal`] rather than accepted, because the v1 baseline must not be
/// assumed to hold on a path that cannot prove it.
///
/// 非原生 Lance 表（例如由 namespace 支撑的远端表）不暴露 manifest，因此本进程无从校验；该情形按
/// [`LanceDbEngineError::Internal`] 上报而非接受，因为 v1 基线不应在无法证明它的路径上被假定成立。
pub async fn verify_table_format(table: &Table, table_name: &str) -> EngineResult<()> {
    let Some(native_table) = table.as_native() else {
        return Err(LanceDbEngineError::internal(format!(
            "cannot verify the Lance storage format of table '{table_name}' because it does not expose a native manifest",
        )));
    };

    let manifest = native_table.manifest().await.map_err(|error| {
        LanceDbEngineError::internal(format!(
            "failed to read the manifest of table '{table_name}' while verifying its storage format: {error}",
        ))
    })?;

    let resolved = format!("{:?}", manifest.data_storage_format.lance_file_format());
    if resolved != PINNED_RESOLVED_FORMAT_DEBUG {
        return Err(LanceDbEngineError::schema_not_allowed(format!(
            "table '{table_name}' resolved to Lance file format {resolved}, but v1 requires {PINNED_RESOLVED_FORMAT_DEBUG}",
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        BLOB_V2_METADATA_KEY, PINNED_RESOLVED_FORMAT_DEBUG, PINNED_STORAGE_VERSION_OPTION,
        PINNED_STORAGE_VERSION_VALUE, blob_v2_refusal_reason,
    };
    use arrow_schema::{DataType, Field, Schema};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Verifies the pin uses a concrete version spelling rather than a drifting alias.
    /// 验证固定项使用具体版本拼写而非漂移别名。
    #[test]
    fn pinned_version_is_concrete_not_an_alias() {
        assert_eq!(PINNED_STORAGE_VERSION_VALUE, "2.1");
        assert_ne!(PINNED_STORAGE_VERSION_VALUE, "stable");
        assert_ne!(PINNED_STORAGE_VERSION_VALUE, "next");
        assert_eq!(PINNED_RESOLVED_FORMAT_DEBUG, "V2_1");
    }

    /// Verifies the option key comes from `lancedb` rather than being a local copy.
    /// 验证选项键取自 `lancedb` 而非本地副本。
    #[test]
    fn option_key_matches_the_dependency_constant() {
        assert_eq!(
            PINNED_STORAGE_VERSION_OPTION,
            lancedb::database::listing::OPT_NEW_TABLE_STORAGE_VERSION
        );
    }

    /// Verifies a blob metadata key at schema level is refused with an explanatory reason.
    /// 验证 Schema 级的 Blob 元数据键被拒绝并给出可解释的理由。
    #[test]
    fn schema_level_blob_metadata_is_refused() {
        let mut metadata = HashMap::new();
        metadata.insert(BLOB_V2_METADATA_KEY.to_string(), "1".to_string());
        let schema = Schema::new_with_metadata(Vec::<Field>::new(), metadata);

        let reason =
            blob_v2_refusal_reason(&schema).expect("schema-level blob metadata is refused");
        assert!(reason.contains(BLOB_V2_METADATA_KEY));
        assert!(reason.contains("schema metadata"));
    }

    /// Verifies a blob metadata key on a single field is refused and names that field.
    /// 验证单个字段上的 Blob 元数据键被拒绝并指出该字段。
    #[test]
    fn field_level_blob_metadata_is_refused_and_names_the_field() {
        let mut field_metadata = HashMap::new();
        field_metadata.insert(BLOB_V2_METADATA_KEY.to_string(), "1".to_string());
        let schema = Schema::new(vec![
            Field::new("payload", DataType::Utf8, true).with_metadata(field_metadata),
        ]);

        let reason = blob_v2_refusal_reason(&schema).expect("field-level blob metadata is refused");
        assert!(
            reason.contains("payload"),
            "reason must name the field: {reason}"
        );
    }

    /// Verifies an ordinary schema with unrelated metadata is accepted.
    /// 验证带无关元数据的普通 Schema 被接受。
    #[test]
    fn ordinary_schema_is_accepted() {
        let mut metadata = HashMap::new();
        metadata.insert("vulcan.table.kind".to_string(), "documents".to_string());
        let schema = Schema::new_with_metadata(
            vec![Arc::new(Field::new("id", DataType::Utf8, false))],
            metadata,
        );

        assert_eq!(blob_v2_refusal_reason(&schema), None);
    }

    /// Verifies a vector-shaped schema with no blob key is accepted, since a float32 fixed-size
    /// list is the only nested column the engine permits.
    /// 验证无 Blob 键的向量形态 Schema 被接受，因为 float32 定长列表是引擎唯一允许的嵌套列。
    #[test]
    fn vector_schema_is_accepted() {
        let schema = Schema::new(vec![Field::new(
            "vector",
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 3),
            false,
        )]);

        assert_eq!(blob_v2_refusal_reason(&schema), None);
    }
}
