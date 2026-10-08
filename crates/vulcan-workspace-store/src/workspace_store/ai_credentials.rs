//! In-process relational persistence for public AI credential material.
//! 公共 AI 凭据材料的进程内关系化持久层。

use super::*;

/// Stores one complete relational AI credential row without provider-domain interpretation.
/// 保存一条完整关系化 AI 凭据记录，不在存储层解释供应商领域语义。
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct AiCredentialSecretRecord {
    /// Opaque reference retained by the public AI connection graph.
    /// 公共 AI 连接图持有的不透明引用。
    pub secret_reference: String,
    /// Version of the strongly typed credential envelope.
    /// 强类型凭据封装版本。
    pub format_version: i64,
    /// Monotonic compare-and-replace revision.
    /// 单调比较替换修订号。
    pub revision: i64,
    /// Closed credential material discriminator.
    /// 封闭凭据材料类别标识。
    pub material_kind: String,
    /// Optional API-key value.
    /// 可选 API Key 值。
    pub secret_value: Option<String>,
    /// Optional bearer or OAuth access token.
    /// 可选 Bearer 或 OAuth Access Token。
    pub access_token: Option<String>,
    /// Optional OAuth refresh token.
    /// 可选 OAuth Refresh Token。
    pub refresh_token: Option<String>,
    /// Optional OAuth issue time in Unix milliseconds.
    /// 可选 OAuth Unix 毫秒签发时间。
    pub issued_at_unix_ms: Option<i64>,
    /// Optional OAuth expiration time in Unix milliseconds.
    /// 可选 OAuth Unix 毫秒过期时间。
    pub expires_at_unix_ms: Option<i64>,
    /// Canonical space-separated OAuth scope field.
    /// 规范空格分隔 OAuth Scope 字段。
    pub scope_text: String,
    /// Optional stable provider account subject.
    /// 可选稳定供应商账号 Subject。
    pub account_subject: Option<String>,
    /// Optional closed host-authorization discriminator.
    /// 可选封闭宿主授权类型标识。
    pub authorization_kind: Option<String>,
    /// Optional provider-visible account user name.
    /// 可选供应商可见账号用户名。
    pub provider_username: Option<String>,
    /// Optional provider-visible credential label.
    /// 可选供应商可见凭据标签。
    pub credential_label: Option<String>,
    /// Optional verified account email.
    /// 可选已验证账号邮箱。
    pub email: Option<String>,
    /// Optional OpenID identity token.
    /// 可选 OpenID 身份 Token。
    pub id_token: Option<String>,
    /// Optional provider account identifier.
    /// 可选供应商账号标识。
    pub account_id: Option<String>,
    /// Optional OAuth client identifier.
    /// 可选 OAuth Client 标识。
    pub oauth_client_id: Option<String>,
    /// Optional OAuth client secret.
    /// 可选 OAuth Client Secret。
    pub oauth_client_secret: Option<String>,
    /// Optional provider project identifier.
    /// 可选供应商项目标识。
    pub project_id: Option<String>,
    /// Optional quota project identifier.
    /// 可选配额项目标识。
    pub quota_project_id: Option<String>,
    /// Optional provider region.
    /// 可选供应商区域。
    pub provider_region: Option<String>,
    /// Optional provider resource URL.
    /// 可选供应商资源 URL。
    pub resource_url: Option<String>,
    /// Optional provider-visible device identifier.
    /// 可选供应商可见设备标识。
    pub provider_device_id: Option<String>,
    /// Row creation time in Unix milliseconds.
    /// 记录 Unix 毫秒创建时间。
    pub created_at_unix_ms: i64,
    /// Row update time in Unix milliseconds.
    /// 记录 Unix 毫秒更新时间。
    pub updated_at_unix_ms: i64,
}

/// Carries one exact mutation identity returned by SQLite.
/// 承载 SQLite 返回的一项精确变更身份。
#[derive(Debug, Clone, Deserialize)]
struct AiCredentialMutationRow {
    /// Mutated opaque secret reference.
    /// 已变更的不透明秘密引用。
    secret_reference: String,
}

/// Loads one relational credential record by its exact opaque reference.
/// 按精确不透明引用加载一条关系化凭据记录。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application SQLite binding and `secret_reference` is the exact lookup key.
/// `host_runtime` 持有应用 SQLite 绑定，`secret_reference` 是精确查找键。
///
/// # Returns
/// 返回值
/// One stored record, clean absence, or a controlled database error.
/// 返回一条已存记录、干净缺失或受控数据库错误。
pub async fn load_ai_credential_secret_record(
    host_runtime: &HostRuntimeHandle,
    secret_reference: &str,
) -> Result<Option<AiCredentialSecretRecord>, String> {
    let secret_reference = validate_secret_reference(secret_reference)?;
    let result =
        local_sql::query_json_async(
            &host_runtime.local,
            LogicalDatabase::AppState,
            r#"
SELECT
    secret_reference, format_version, revision, material_kind, secret_value,
    access_token, refresh_token, issued_at_unix_ms, expires_at_unix_ms, scope_text,
    account_subject, authorization_kind, provider_username, credential_label, email, id_token, account_id,
    oauth_client_id, oauth_client_secret,
    project_id, quota_project_id, provider_region, resource_url, provider_device_id,
    created_at_unix_ms, updated_at_unix_ms
FROM ai_credential_secrets
WHERE secret_reference = ?1
LIMIT 1;
"#,
            vec![SqliteValue::Text(secret_reference.to_string())],
        ).await
        .map_err(|error| format!("查询 AI 凭据记录失败：{error}"))?;
    let mut rows = decode_query_rows::<AiCredentialSecretRecord>(&result.json_data)?;
    if rows.len() > 1 {
        return Err(String::from("AI 凭据查询返回了重复主键记录。"));
    }
    Ok(rows.pop())
}

/// Creates one revision-one relational credential record without overwriting an existing reference.
/// 创建一条修订号为一的关系化凭据记录且不覆盖现有引用。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application SQLite binding and `record` is the complete candidate row.
/// `host_runtime` 持有应用 SQLite 绑定，`record` 是完整候选记录。
///
/// # Returns
/// 返回值
/// Success only after SQLite inserts the exact reference.
/// 仅在 SQLite 插入精确引用后返回成功。
pub async fn create_ai_credential_secret_record(
    host_runtime: &HostRuntimeHandle,
    record: &AiCredentialSecretRecord,
) -> Result<(), String> {
    validate_record(record)?;
    if record.revision != 1 {
        return Err(String::from("新 AI 凭据记录必须从修订号一开始。"));
    }
    insert_ai_credential_secret_record(host_runtime, record, "创建").await
}

/// Restores one previously deleted relational credential record with its exact revision.
/// 使用精确修订号恢复一条此前删除的关系化凭据记录。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application SQLite binding and `record` is the exact rollback snapshot.
/// `host_runtime` 持有应用 SQLite 绑定，`record` 是精确回滚快照。
///
/// # Returns
/// 返回值
/// Success only when the reference is absent and the snapshot is inserted.
/// 仅在引用缺失且快照成功插入时返回成功。
pub async fn restore_ai_credential_secret_record(
    host_runtime: &HostRuntimeHandle,
    record: &AiCredentialSecretRecord,
) -> Result<(), String> {
    validate_record(record)?;
    insert_ai_credential_secret_record(host_runtime, record, "恢复").await
}

/// Replaces one relational credential only when its stored revision matches the caller expectation.
/// 仅在已存修订号匹配调用方预期时替换关系化凭据。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application SQLite binding, `expected_revision` is the current revision,
/// and `record` is the complete next revision.
/// `host_runtime` 持有应用 SQLite 绑定，`expected_revision` 是当前修订号，`record` 是完整下一修订版。
///
/// # Returns
/// 返回值
/// Success only after exactly one compare-and-replace update.
/// 仅在精确完成一次比较替换更新后返回成功。
pub async fn replace_ai_credential_secret_record(
    host_runtime: &HostRuntimeHandle,
    expected_revision: i64,
    record: &AiCredentialSecretRecord,
) -> Result<(), String> {
    validate_record(record)?;
    let expected_next = expected_revision
        .checked_add(1)
        .ok_or_else(|| String::from("AI 凭据修订号已耗尽。"))?;
    if record.revision != expected_next {
        return Err(String::from("替换 AI 凭据必须精确递增一个修订号。"));
    }
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
UPDATE ai_credential_secrets
SET format_version = ?1,
    revision = ?2,
    material_kind = ?3,
    secret_value = ?4,
    access_token = ?5,
    refresh_token = ?6,
    issued_at_unix_ms = ?7,
    expires_at_unix_ms = ?8,
    scope_text = ?9,
    account_subject = ?10,
    authorization_kind = ?11,
    provider_username = ?12,
    credential_label = ?13,
    email = ?14,
    id_token = ?15,
    account_id = ?16,
    oauth_client_id = ?17,
    oauth_client_secret = ?18,
    project_id = ?19,
    quota_project_id = ?20,
    provider_region = ?21,
    resource_url = ?22,
    provider_device_id = ?23,
    updated_at_unix_ms = ?24
WHERE secret_reference = ?25 AND revision = ?26
RETURNING secret_reference;
"#,
        update_params(record, expected_revision),
    )
    .await
    .map_err(|error| format!("替换 AI 凭据记录失败：{error}"))?;
    require_single_mutation_row(&result.json_data, record.secret_reference.as_str(), "替换")
}

/// Deletes one relational credential record idempotently.
/// 幂等删除一条关系化凭据记录。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application SQLite binding and `secret_reference` is the exact deletion key.
/// `host_runtime` 持有应用 SQLite 绑定，`secret_reference` 是精确删除键。
///
/// # Returns
/// 返回值
/// Success when the row is absent or deleted.
/// 记录缺失或已删除时返回成功。
pub async fn delete_ai_credential_secret_record(
    host_runtime: &HostRuntimeHandle,
    secret_reference: &str,
) -> Result<(), String> {
    let secret_reference = validate_secret_reference(secret_reference)?;
    local_sql::execute_batch_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        "DELETE FROM ai_credential_secrets WHERE secret_reference = ?1;",
        vec![vec![SqliteValue::Text(secret_reference.to_string())]],
    )
    .await
    .map_err(|error| format!("删除 AI 凭据记录失败：{error}"))?;
    Ok(())
}

/// Inserts one complete credential row for create or rollback restoration.
/// 为创建或回滚恢复插入一条完整凭据记录。
async fn insert_ai_credential_secret_record(
    host_runtime: &HostRuntimeHandle,
    record: &AiCredentialSecretRecord,
    operation: &str,
) -> Result<(), String> {
    let result =
        local_sql::query_json_async(
            &host_runtime.local,
            LogicalDatabase::AppState,
            r#"
INSERT INTO ai_credential_secrets (
    secret_reference, format_version, revision, material_kind, secret_value,
    access_token, refresh_token, issued_at_unix_ms, expires_at_unix_ms, scope_text,
    account_subject, authorization_kind, provider_username, credential_label, email, id_token, account_id,
    oauth_client_id, oauth_client_secret, project_id, quota_project_id, provider_region,
    resource_url, provider_device_id,
    created_at_unix_ms, updated_at_unix_ms
) VALUES (
    ?1, ?2, ?3, ?4, ?5,
    ?6, ?7, ?8, ?9, ?10,
    ?11, ?12, ?13, ?14, ?15, ?16,
    ?17, ?18, ?19, ?20, ?21, ?22, ?23,
    ?24, ?25, ?26
)
RETURNING secret_reference;
"#,
            insert_params(record),
        ).await
        .map_err(|error| format!("{operation} AI 凭据记录失败：{error}"))?;
    require_single_mutation_row(
        &result.json_data,
        record.secret_reference.as_str(),
        operation,
    )
}

/// Builds typed SQLite values for one complete insert.
/// 为一次完整插入构建类型化 SQLite 参数。
fn insert_params(record: &AiCredentialSecretRecord) -> Vec<SqliteValue> {
    vec![
        SqliteValue::Text(record.secret_reference.clone()),
        SqliteValue::Integer(record.format_version),
        SqliteValue::Integer(record.revision),
        SqliteValue::Text(record.material_kind.clone()),
        optional_sqlite_string(&record.secret_value),
        optional_sqlite_string(&record.access_token),
        optional_sqlite_string(&record.refresh_token),
        optional_sqlite_i64(record.issued_at_unix_ms),
        optional_sqlite_i64(record.expires_at_unix_ms),
        SqliteValue::Text(record.scope_text.clone()),
        optional_sqlite_string(&record.account_subject),
        optional_sqlite_string(&record.authorization_kind),
        optional_sqlite_string(&record.provider_username),
        optional_sqlite_string(&record.credential_label),
        optional_sqlite_string(&record.email),
        optional_sqlite_string(&record.id_token),
        optional_sqlite_string(&record.account_id),
        optional_sqlite_string(&record.oauth_client_id),
        optional_sqlite_string(&record.oauth_client_secret),
        optional_sqlite_string(&record.project_id),
        optional_sqlite_string(&record.quota_project_id),
        optional_sqlite_string(&record.provider_region),
        optional_sqlite_string(&record.resource_url),
        optional_sqlite_string(&record.provider_device_id),
        SqliteValue::Integer(record.created_at_unix_ms),
        SqliteValue::Integer(record.updated_at_unix_ms),
    ]
}

/// Builds typed SQLite values for one complete compare-and-replace update.
/// 为一次完整比较替换更新构建类型化 SQLite 参数。
fn update_params(record: &AiCredentialSecretRecord, expected_revision: i64) -> Vec<SqliteValue> {
    let mut params = insert_params(record);
    params.remove(0);
    params.remove(23);
    params.push(SqliteValue::Text(record.secret_reference.clone()));
    params.push(SqliteValue::Integer(expected_revision));
    params
}

/// Requires one exact mutation identity from a SQLite RETURNING result.
/// 要求 SQLite RETURNING 结果包含一个精确变更身份。
fn require_single_mutation_row(
    json_data: &str,
    secret_reference: &str,
    operation: &str,
) -> Result<(), String> {
    let rows = decode_query_rows::<AiCredentialMutationRow>(json_data)?;
    match rows.as_slice() {
        [row] if row.secret_reference == secret_reference => Ok(()),
        [] => Err(format!(
            "AI 凭据 `{secret_reference}` 在{operation}期间发生并发变化。"
        )),
        _ => Err(format!(
            "AI 凭据 `{secret_reference}` 的{operation}结果不唯一。"
        )),
    }
}

/// Validates storage-level row invariants without interpreting provider-specific secrets.
/// 校验存储层记录不变量且不解释供应商专属秘密。
fn validate_record(record: &AiCredentialSecretRecord) -> Result<(), String> {
    validate_secret_reference(record.secret_reference.as_str())?;
    if record.format_version <= 0 || record.revision <= 0 {
        return Err(String::from("AI 凭据版本和修订号必须为正数。"));
    }
    if record.material_kind.trim().is_empty() {
        return Err(String::from("AI 凭据材料类别不能为空。"));
    }
    if record.created_at_unix_ms < 0 || record.updated_at_unix_ms < 0 {
        return Err(String::from("AI 凭据记录时间不能为负数。"));
    }
    Ok(())
}

/// Validates one opaque secret reference before it reaches SQL.
/// 在不透明秘密引用进入 SQL 前校验该引用。
fn validate_secret_reference(secret_reference: &str) -> Result<&str, String> {
    let normalized = secret_reference.trim();
    if normalized.is_empty() {
        return Err(String::from("AI 凭据秘密引用不能为空。"));
    }
    Ok(normalized)
}

/// Converts one optional text field into a typed SQLite scalar.
/// 将一个可选文本字段转换为类型化 SQLite 标量。
fn optional_sqlite_string(value: &Option<String>) -> SqliteValue {
    value
        .as_ref()
        .map(|value| SqliteValue::Text(value.clone()))
        .unwrap_or(SqliteValue::Null)
}

/// Converts one optional integer field into a typed SQLite scalar.
/// 将一个可选整数字段转换为类型化 SQLite 标量。
fn optional_sqlite_i64(value: Option<i64>) -> SqliteValue {
    value.map(SqliteValue::Integer).unwrap_or(SqliteValue::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that update parameters preserve the SQL placeholder order.
    /// 验证更新参数保持 SQL 占位符顺序。
    #[test]
    fn update_parameters_place_reference_and_expected_revision_last() {
        let record = AiCredentialSecretRecord {
            secret_reference: String::from("secret-1"),
            format_version: 1,
            revision: 2,
            material_kind: String::from("static_bearer"),
            secret_value: None,
            access_token: Some(String::from("token")),
            refresh_token: None,
            issued_at_unix_ms: None,
            expires_at_unix_ms: None,
            scope_text: String::new(),
            account_subject: None,
            authorization_kind: None,
            provider_username: None,
            credential_label: None,
            email: None,
            id_token: None,
            account_id: None,
            oauth_client_id: None,
            oauth_client_secret: None,
            project_id: None,
            quota_project_id: None,
            provider_region: None,
            resource_url: None,
            provider_device_id: None,
            created_at_unix_ms: 10,
            updated_at_unix_ms: 20,
        };

        let params = update_params(&record, 1);

        assert_eq!(params.len(), 26);
        assert_eq!(params[23], SqliteValue::Integer(20));
        assert_eq!(params[24], SqliteValue::Text(String::from("secret-1")));
        assert_eq!(params[25], SqliteValue::Integer(1));
    }
}
