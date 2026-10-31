use super::*;

/// Inserts or refreshes one active project row in the application index.
/// 在应用索引中插入或刷新一条活动项目记录。
///
/// # Parameters / 参数
///
/// - `host_runtime`: Host runtime owning the application SQLite binding.
///   `host_runtime`：持有应用 SQLite 绑定的宿主运行时。
/// - `project`: Complete active project projection to persist.
///   `project`：需要持久化的完整活动项目投影。
///
/// # Returns / 返回值
///
/// Success after the project commits successfully, or an explicit error.
/// 项目提交成功后返回成功，否则返回显式错误。
async fn upsert_active_project(
    host_runtime: &HostRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    // now is shared by created_at and updated_at for a newly inserted project row.
    // now 由新插入项目行的 created_at 与 updated_at 共享。
    let now = current_unix_millis_text();
    // sort_order uses a checked conversion so project order cannot truncate on wide usize hosts.
    // sort_order 使用受检查转换，避免项目顺序在宽 usize 宿主上发生截断。
    let sort_order = i64::try_from(project.order)
        .map_err(|_| String::from("Failed to write the active project: the project order exceeds the SQLite integer range."))?;
    // Preserve the existing typed field order, including SQL NULL for absent icon overrides.
    // 保持既有强类型字段顺序，包括缺失图标覆盖对应的 SQL NULL。
    let project_values = project_persistence_sql_values(&[
        SqliteValue::Text(project.id.clone()),
        SqliteValue::Text(project.title.clone()),
        SqliteValue::Text(project.path.clone()),
        SqliteValue::Integer(i64::from(project.pinned)),
        SqliteValue::Integer(0),
        SqliteValue::Integer(sort_order),
        optional_sqlite_string(&project.icon_pack),
        optional_sqlite_string(&project.icon_key),
        SqliteValue::Text(now.clone()),
        SqliteValue::Text(now),
    ])?;
    // Client 0.2.3 binds parameters to one statement only; this script owns the entire transaction.
    // 客户端 0.2.3 仅向单条语句绑定参数；此脚本持有整个事务。
    // Keep BEGIN and COMMIT on the same backend-owned connection, never across separate RPCs.
    // 将 BEGIN 和 COMMIT 保留在同一后端连接内，绝不跨多个远程请求拆分。
    let statement = format!(
        r#"
BEGIN IMMEDIATE;
INSERT INTO projects (
    id,
    name,
    root_path,
    pinned,
    archived,
    sort_order,
    icon_pack,
    icon_key,
    created_at,
    updated_at
) VALUES ({project_values})
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    root_path = excluded.root_path,
    pinned = excluded.pinned,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
COMMIT;
"#,
    );
    local_sql::execute_script_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        statement.as_str(),
        vec![],
    )
    .await
    .map_err(|error| format!("Failed to write the active project transaction: {error}"))?;

    Ok(())
}

/// Encode the exact scalar subset produced by the project parameter builder.
/// 编码项目参数构建器产生的精确标量子集。
/// `parameters` contains ordered integers, text, or NULL; returns SQL values or an explicit type error.
/// `parameters` 包含有序整数、文本或 NULL；返回 SQL 值，或明确的类型错误。
fn project_persistence_sql_values(parameters: &[SqliteValue]) -> Result<String, String> {
    parameters
        .iter()
        .map(|parameter| match parameter {
            SqliteValue::Integer(value) => Ok(value.to_string()),
            SqliteValue::Null => Ok(String::from("NULL")),
            // Escape quotes before introducing literal NUL concatenation; source text never becomes SQL syntax.
            // 先转义引号，再引入字面空字符拼接；源文本绝不成为 SQL 语法。
            // Preserve text as text regardless of the database encoding.
            // 无论数据库编码如何，都将文本保留为文本类型。
            SqliteValue::Text(value) => Ok(format!(
                "'{}'",
                value.replace('\'', "''").replace('\0', "' || char(0) || '")
            )),
            SqliteValue::Real(_) | SqliteValue::Blob(_) => Err(String::from(
                "Project persistence parameters can only be integers, text or NULL.",
            )),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|values| values.join(", "))
}

/// Identity row returned by one guarded project mutation.
/// 单次受保护项目变更返回的身份行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ProjectMutationIdentityRow {
    /// Exact project identifier changed by SQLite.
    /// SQLite 实际变更的精确项目标识。
    id: String,
}

/// Complete project-layout row returned by one guarded layout mutation.
/// 单次受保护布局变更返回的完整项目布局行。
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ProjectLayoutMutationRow {
    /// Exact project identifier changed by SQLite.
    /// SQLite 实际变更的精确项目标识。
    id: String,
    /// Persisted pinned flag returned as a strict SQLite integer boolean.
    /// 以严格 SQLite 整数布尔值返回的持久化置顶标记。
    pinned: i64,
    /// Persisted zero-based order inside the resulting project section.
    /// 结果项目分区内持久化的零基顺序。
    sort_order: i64,
}

/// One expected-to-result project layout transition submitted to SQLite.
/// 提交给 SQLite 的单个“预期状态到结果状态”项目布局转换。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectLayoutMutationEntry {
    /// Exact active project identifier covered by the full-layout mutation.
    /// 完整布局变更覆盖的精确活动项目标识。
    id: String,
    /// Pinned state that must still exist before the mutation starts.
    /// 变更开始前必须仍然存在的置顶状态。
    expected_pinned: bool,
    /// Pinned state committed by the mutation.
    /// 本次变更提交的置顶状态。
    pinned: bool,
    /// Zero-based order committed inside the resulting section.
    /// 在结果分区内提交的零基顺序。
    sort_order: i64,
}

/// Validates one project identifier before it enters a persistence predicate.
/// 在项目标识进入持久化谓词前进行校验。
///
/// # Parameters
/// 参数
/// `project_id` is the exact project identity supplied by the caller.
/// `project_id` 是调用方提供的精确项目身份。
/// `action_description` describes the mutation in user-facing diagnostics.
/// `action_description` 描述用户可见诊断中的变更动作。
///
/// # Returns
/// 返回值
/// Returns success for a non-blank identity or an explicit validation error.
/// 对非空白身份返回成功，否则返回显式校验错误。
fn validate_project_mutation_id(project_id: &str, action_description: &str) -> Result<(), String> {
    if project_id.trim().is_empty() {
        return Err(format!(
            "Failed to {action_description}: the project identifier must not be empty."
        ));
    }
    Ok(())
}

/// Requires one guarded mutation to return exactly the requested project identity.
/// 要求单次受保护变更精确返回所请求的项目身份。
///
/// # Parameters
/// 参数
/// `project_id` is the exact project identity submitted to SQLite.
/// `project_id` 是提交给 SQLite 的精确项目身份。
/// `rows` contains identities returned by `RETURNING`.
/// `rows` 包含 `RETURNING` 返回的身份。
/// `action_description` describes the mutation in user-facing diagnostics.
/// `action_description` 描述用户可见诊断中的变更动作。
///
/// # Returns
/// 返回值
/// Returns success only for one matching identity; missing, extra, or mismatched rows fail explicitly.
/// 仅在存在一条匹配身份时返回成功；缺失、多余或不匹配的行都会显式失败。
fn validate_project_mutation_identity_rows(
    project_id: &str,
    rows: &[ProjectMutationIdentityRow],
    action_description: &str,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, action_description)?;
    match rows {
        [row] if row.id == project_id => Ok(()),
        [] => Err(format!(
            "Failed to {action_description}: active project `{project_id}` does not exist or its state changed."
        )),
        [row] => Err(format!(
            "Failed to {action_description}: SQLite returned unexpected project identifier `{}`, expected `{project_id}`.",
            row.id
        )),
        _ => Err(format!(
            "Failed to {action_description}: SQLite returned {} project identities, expected exactly one.",
            rows.len()
        )),
    }
}

/// Appends one complete project section to a guarded layout mutation request.
/// 将一个完整项目分区追加到受保护布局变更请求。
///
/// # Parameters
/// 参数
/// `entries` receives validated transition entries in persisted section order.
/// `entries` 按持久化分区顺序接收已校验的转换条目。
/// `seen_project_ids` rejects duplicate identities across both sections.
/// `seen_project_ids` 用于拒绝两个分区之间重复的身份。
/// `project_ids` is the complete ordered identity list for this section.
/// `project_ids` 是该分区完整的有序身份列表。
/// `pinned` is the resulting pinned state shared by the section.
/// `pinned` 是该分区共享的结果置顶状态。
/// `pin_transition` optionally identifies the one project whose previous pinned state must be opposite.
/// `pin_transition` 可选标识唯一一个先前置顶状态必须相反的项目。
///
/// # Returns
/// 返回值
/// Returns success after all section entries are appended, or a strict identity/order error.
/// 全部分区条目追加后返回成功，否则返回严格身份或顺序错误。
fn append_project_layout_section(
    entries: &mut Vec<ProjectLayoutMutationEntry>,
    seen_project_ids: &mut BTreeSet<String>,
    project_ids: &[String],
    pinned: bool,
    pin_transition: Option<(&str, bool)>,
) -> Result<(), String> {
    for (index, project_id) in project_ids.iter().enumerate() {
        validate_project_mutation_id(project_id, "update the project layout")?;
        if !seen_project_ids.insert(project_id.clone()) {
            return Err(format!(
                "Failed to update the project layout: project identifier `{project_id}` is duplicated in the layout request."
            ));
        }
        // sort_order uses a checked conversion so an oversized layout cannot silently truncate.
        // sort_order 使用受检查转换，避免超大布局被静默截断。
        let sort_order = i64::try_from(index)
            .map_err(|_| String::from("Failed to update the project layout: the project count exceeds the SQLite order range."))?;
        // expected_pinned differs only for the explicitly requested pin transition.
        // expected_pinned 仅对显式请求的置顶转换项目取相反值。
        let expected_pinned = match pin_transition {
            Some((transition_project_id, next_pinned)) if transition_project_id == project_id => {
                if next_pinned != pinned {
                    return Err(format!(
                        "Failed to update the project layout: pin-transition project `{project_id}` is in the wrong section."
                    ));
                }
                !next_pinned
            }
            _ => pinned,
        };
        entries.push(ProjectLayoutMutationEntry {
            id: project_id.clone(),
            expected_pinned,
            pinned,
            sort_order,
        });
    }
    Ok(())
}

/// Builds one full active-project layout mutation with optional pin transition semantics.
/// 构建一个带可选置顶转换语义的完整活动项目布局变更。
///
/// # Parameters
/// 参数
/// `pinned_project_ids` is the complete resulting pinned-section order.
/// `pinned_project_ids` 是完整的结果置顶分区顺序。
/// `regular_project_ids` is the complete resulting regular-section order.
/// `regular_project_ids` 是完整的结果普通分区顺序。
/// `pin_transition` optionally identifies one project and its resulting pinned state.
/// `pin_transition` 可选标识一个项目及其结果置顶状态。
///
/// # Returns
/// 返回值
/// Returns the validated full-layout transition entries or a strict request error.
/// 返回已校验的完整布局转换条目，或严格请求错误。
fn project_layout_mutation_entries(
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
    pin_transition: Option<(&str, bool)>,
) -> Result<Vec<ProjectLayoutMutationEntry>, String> {
    // entries retain pinned-section then regular-section order for deterministic parameters.
    // entries 以置顶分区后接普通分区的顺序保留条目，确保参数确定性。
    let mut entries = Vec::with_capacity(
        pinned_project_ids
            .len()
            .checked_add(regular_project_ids.len())
            .ok_or_else(|| {
                String::from("Failed to update the project layout: the project count overflowed.")
            })?,
    );
    // seen_project_ids proves the request contains one unique identity per active project.
    // seen_project_ids 证明请求为每个活动项目只包含一个唯一身份。
    let mut seen_project_ids = BTreeSet::new();
    append_project_layout_section(
        &mut entries,
        &mut seen_project_ids,
        pinned_project_ids,
        true,
        pin_transition,
    )?;
    append_project_layout_section(
        &mut entries,
        &mut seen_project_ids,
        regular_project_ids,
        false,
        pin_transition,
    )?;
    if entries.is_empty() {
        return Err(String::from(
            "Failed to update the project layout: the complete project layout must not be empty.",
        ));
    }
    if let Some((transition_project_id, _next_pinned)) = pin_transition
        && !seen_project_ids.contains(transition_project_id)
    {
        return Err(format!(
            "Failed to update the project layout: pin-transition project `{transition_project_id}` is not in the complete layout."
        ));
    }
    Ok(entries)
}

/// Builds the single guarded SQLite statement for a complete project-layout mutation.
/// 为完整项目布局变更构建单条受保护 SQLite 语句。
///
/// # Parameters
/// 参数
/// `entry_count` is the exact number of active projects represented by the request.
/// `entry_count` 是请求表示的精确活动项目数量。
///
/// # Returns
/// 返回值
/// Returns a numbered-parameter statement that updates every row or no row.
/// 返回使用编号参数、只会更新全部行或完全不更新的语句。
fn project_layout_mutation_statement(entry_count: usize) -> Result<String, String> {
    if entry_count == 0 {
        return Err(String::from(
            "Failed to update the project layout: the complete project layout must not be empty.",
        ));
    }
    // value_rows assigns four stable parameters to every requested project transition.
    // value_rows 为每个请求的项目转换分配四个稳定参数。
    let mut value_rows = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        // parameter_offset is checked before deriving the one-based SQLite parameter indices.
        // parameter_offset 在派生 SQLite 一基参数索引前执行溢出检查。
        let parameter_offset = index.checked_mul(4).ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
        let id_parameter = parameter_offset.checked_add(1).ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
        let expected_pinned_parameter = parameter_offset.checked_add(2).ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
        let pinned_parameter = parameter_offset.checked_add(3).ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
        let sort_order_parameter = parameter_offset.checked_add(4).ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
        value_rows.push(format!(
            "(?{id_parameter}, ?{expected_pinned_parameter}, ?{pinned_parameter}, ?{sort_order_parameter})"
        ));
    }
    // updated_at_parameter follows all per-project values in the typed parameter vector.
    // updated_at_parameter 紧跟类型化参数向量中的全部逐项目值。
    let updated_at_parameter = entry_count
        .checked_mul(4)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
    Ok(format!(
        r#"
WITH requested(id, expected_pinned, pinned, sort_order) AS (
    VALUES {}
),
current_active(id, pinned) AS MATERIALIZED (
    SELECT id, pinned
    FROM projects
    WHERE archived = 0
),
layout_guard(valid) AS (
    SELECT 1
    WHERE (SELECT COUNT(*) FROM current_active) = (SELECT COUNT(*) FROM requested)
      AND (
          SELECT COUNT(*)
          FROM current_active
          INNER JOIN requested
              ON requested.id = current_active.id
             AND requested.expected_pinned = current_active.pinned
      ) = (SELECT COUNT(*) FROM requested)
)
UPDATE projects
SET pinned = (
        SELECT requested.pinned
        FROM requested
        WHERE requested.id = projects.id
    ),
    sort_order = (
        SELECT requested.sort_order
        FROM requested
        WHERE requested.id = projects.id
    ),
    updated_at = ?{updated_at_parameter}
WHERE archived = 0
  AND id IN (SELECT id FROM requested)
  AND EXISTS (SELECT 1 FROM layout_guard)
RETURNING id, pinned, sort_order;
"#,
        value_rows.join(", ")
    ))
}

/// Validates every row returned by a guarded full-layout mutation.
/// 校验受保护完整布局变更返回的每一行。
///
/// # Parameters
/// 参数
/// `entries` contains the complete requested result layout.
/// `entries` 包含完整的请求结果布局。
/// `rows` contains the actual SQLite `RETURNING` rows.
/// `rows` 包含 SQLite 实际返回的 `RETURNING` 行。
/// `action_description` describes the mutation in user-facing diagnostics.
/// `action_description` 描述用户可见诊断中的变更动作。
///
/// # Returns
/// 返回值
/// Returns success only when every requested identity, pin state, and order is returned exactly once.
/// 仅在每个请求身份、置顶状态与顺序都恰好返回一次时成功。
fn validate_project_layout_mutation_rows(
    entries: &[ProjectLayoutMutationEntry],
    rows: &[ProjectLayoutMutationRow],
    action_description: &str,
) -> Result<(), String> {
    if rows.len() != entries.len() {
        return Err(format!(
            "Failed to {action_description}: the active project set or pin state changed; SQLite returned {} rows, expected {} rows.",
            rows.len(),
            entries.len()
        ));
    }
    // expected_layout maps every requested identity to its exact resulting state.
    // expected_layout 将每个请求身份映射到其精确结果状态。
    let expected_layout = entries
        .iter()
        .map(|entry| (entry.id.as_str(), (entry.pinned, entry.sort_order)))
        .collect::<BTreeMap<_, _>>();
    // returned_project_ids rejects duplicate or fabricated engine response rows.
    // returned_project_ids 用于拒绝 SQLite 结果中的重复或伪造行。
    let mut returned_project_ids = BTreeSet::new();
    for row in rows {
        let Some((expected_pinned, expected_sort_order)) =
            expected_layout.get(row.id.as_str()).copied()
        else {
            return Err(format!(
                "Failed to {action_description}: SQLite returned unrequested project identifier `{}`.",
                row.id
            ));
        };
        if !returned_project_ids.insert(row.id.as_str()) {
            return Err(format!(
                "Failed to {action_description}: SQLite returned project identifier `{}` more than once.",
                row.id
            ));
        }
        // returned_pinned rejects non-boolean persisted states instead of coercing them.
        // returned_pinned 拒绝非布尔持久化状态，而不是进行强制转换。
        let returned_pinned = match row.pinned {
            0 => false,
            1 => true,
            value => {
                return Err(format!(
                    "Failed to {action_description}: project `{}` returned an invalid pinned flag `{value}`.",
                    row.id
                ));
            }
        };
        if returned_pinned != expected_pinned || row.sort_order != expected_sort_order {
            return Err(format!(
                "Failed to {action_description}: project `{}` returned layout pinned={}, sort_order={}, expected pinned={}, sort_order={}.",
                row.id,
                row.pinned,
                row.sort_order,
                i64::from(expected_pinned),
                expected_sort_order
            ));
        }
    }
    Ok(())
}

/// Persists one complete active-project layout through a single guarded SQLite statement.
/// 通过单条受保护 SQLite 语句持久化完整活动项目布局。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `entries` contains every active project with expected and resulting layout states.
/// `entries` 包含每个活动项目的预期与结果布局状态。
/// `action_description` describes the mutation in user-facing diagnostics.
/// `action_description` 描述用户可见诊断中的变更动作。
///
/// # Returns
/// 返回值
/// Returns success only after SQLite atomically returns the complete requested result layout.
/// 仅在 SQLite 原子返回完整请求结果布局后成功。
async fn persist_project_layout_mutation(
    host_runtime: HostRuntimeHandle,
    entries: &[ProjectLayoutMutationEntry],
    action_description: &str,
) -> Result<(), String> {
    // statement embeds only generated numbered placeholders; all project data remains typed parameters.
    // statement 仅嵌入生成的编号占位符；全部项目数据仍通过类型化参数传递。
    let statement = project_layout_mutation_statement(entries.len())?;
    // parameter_capacity covers four fields per project plus one shared update timestamp.
    // parameter_capacity 覆盖每个项目四个字段以及一个共享更新时间戳。
    let parameter_capacity = entries
        .len()
        .checked_mul(4)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| {
            String::from(
                "Failed to update the project layout: the SQLite parameter count overflowed.",
            )
        })?;
    // parameters follow the exact numbered-placeholder order generated above.
    // parameters 严格遵循上面生成的编号占位符顺序。
    let mut parameters = Vec::with_capacity(parameter_capacity);
    for entry in entries {
        parameters.push(SqliteValue::Text(entry.id.clone()));
        parameters.push(SqliteValue::Integer(i64::from(entry.expected_pinned)));
        parameters.push(SqliteValue::Integer(i64::from(entry.pinned)));
        parameters.push(SqliteValue::Integer(entry.sort_order));
    }
    parameters.push(SqliteValue::Text(current_unix_millis_text()));
    // result proves the full guarded layout statement updated every requested active project.
    // result 证明完整受保护布局语句更新了每个请求的活动项目。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        &statement,
        parameters.clone(),
    )
    .await
    .map_err(|error| format!("Failed to {action_description}: {error}"))?;
    // rows retain the authoritative result state instead of trusting write completion alone.
    // rows 保留权威结果状态，而不是只相信写入调用完成。
    let rows = decode_query_rows::<ProjectLayoutMutationRow>(&result.json_data)?;
    validate_project_layout_mutation_rows(entries, &rows, action_description)
}

/// Persists one newly visible project row without rewriting unrelated workspace rows.
/// 持久化单个新出现的可见项目行，不重写无关工作区行。
pub async fn persist_new_project(
    host_runtime: HostRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    upsert_active_project(&host_runtime, project).await
}

/// Updates the archived flag for one project row without changing its metadata.
/// 只更新单个项目行的归档标记，不改变其元数据。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `project_id` is the exact project identity to transition.
/// `project_id` 是需要转换的精确项目身份。
/// `expected_archived` is the lifecycle state that must still exist before the update.
/// `expected_archived` 是更新前必须仍然存在的生命周期状态。
/// `archived` is the distinct lifecycle state committed by the update.
/// `archived` 是更新提交的不同生命周期状态。
///
/// # Returns
/// 返回值
/// Returns success only after one exact expected-state row is returned by SQLite.
/// 仅在 SQLite 返回一条精确预期状态行后成功。
pub(super) async fn update_project_archive_flag(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
    expected_archived: bool,
    archived: bool,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "update the project archive state")?;
    if expected_archived == archived {
        return Err(String::from(
            "Failed to update the project archive state: the expected state and the result state must differ.",
        ));
    }
    // result proves the exact expected lifecycle transition changed one project row.
    // result 证明精确的预期生命周期转换变更了一条项目行。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
UPDATE projects
SET archived = ?1, updated_at = ?2
WHERE id = ?3 AND archived = ?4
RETURNING id;
"#,
        vec![
            SqliteValue::Integer(i64::from(archived)),
            SqliteValue::Text(current_unix_millis_text()),
            SqliteValue::Text(project_id.to_string()),
            SqliteValue::Integer(i64::from(expected_archived)),
        ],
    )
    .await
    .map_err(|error| format!("Failed to update the project archive state: {error}"))?;
    // rows must contain the one project whose lifecycle predicate matched.
    // rows 必须包含生命周期谓词命中的唯一项目。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "update the project archive state")
}

/// Deletes one active project row while preserving archived session directories.
/// 删除单个活动项目记录，同时保留归档会话目录。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `project_id` is the exact active project identity to delete.
/// `project_id` 是需要删除的精确活动项目身份。
///
/// # Returns
/// 返回值
/// Returns success only after SQLite returns the deleted active identity.
/// 仅在 SQLite 返回已删除活动身份后成功。
pub(super) async fn delete_active_project_row(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "remove the project record")?;
    // result proves the active project row still existed at deletion time.
    // result 证明活动项目行在删除时仍然存在。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        "DELETE FROM projects WHERE id = ?1 AND archived = 0 RETURNING id;",
        vec![SqliteValue::Text(project_id.to_string())],
    )
    .await
    .map_err(|error| format!("Failed to remove the project record: {error}"))?;
    // rows must contain the one active identity deleted by SQLite.
    // rows 必须包含 SQLite 删除的唯一活动身份。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "remove the project record")
}

/// Reactivates one exact archived project row after every child V1 session has been restored.
/// 在每个子 V1 会话均已恢复后重新激活一条精确的归档项目行。
///
/// # Parameters / 参数
///
/// - `host_runtime`: Authoritative APP SQLite runtime.
///   `host_runtime`：权威 APP SQLite 运行时。
/// - `project_id`: Exact archived project identity.
///   `project_id`：精确的归档项目身份。
///
/// # Returns / 返回值
///
/// Success only when the guarded archived-to-active transition returns the exact row.
/// 仅在受保护的归档到活动转换返回精确行时成功。
pub async fn restore_archived_project_row_v1(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    update_project_archive_flag(host_runtime, project_id, true, false).await
}

/// Deletes one exact active project row after every child V1 session has been archived.
/// 在每个子 V1 会话均已归档后删除一条精确的活动项目行。
///
/// # Parameters / 参数
///
/// - `host_runtime`: Authoritative APP SQLite runtime.
///   `host_runtime`：权威 APP SQLite 运行时。
/// - `project_id`: Exact active project identity.
///   `project_id`：精确的活动项目身份。
///
/// # Returns / 返回值
///
/// Success only when SQLite deletes and returns the exact active row.
/// 仅在 SQLite 删除并返回精确活动行时成功。
pub async fn delete_active_project_row_v1(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    delete_active_project_row(host_runtime, project_id).await
}

/// Atomically persists one pin transition and both complete project-section orders.
/// 原子持久化一次置顶转换以及两个完整项目分区的顺序。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `project_id` is the one project expected to cross section boundaries.
/// `project_id` 是预期跨越分区边界的唯一项目。
/// `pinned` is that project's resulting pinned state.
/// `pinned` 是该项目的结果置顶状态。
/// `pinned_project_ids` is the complete resulting pinned-section order.
/// `pinned_project_ids` 是完整的结果置顶分区顺序。
/// `regular_project_ids` is the complete resulting regular-section order.
/// `regular_project_ids` 是完整的结果普通分区顺序。
///
/// # Returns
/// 返回值
/// Returns success only after one atomic full-layout transition returns every active project.
/// 仅在一次原子完整布局转换返回每个活动项目后成功。
pub async fn persist_project_pinned_and_order_update(
    host_runtime: HostRuntimeHandle,
    project_id: &str,
    pinned: bool,
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "toggle the project pin state")?;
    // entries model one pin transition plus the complete resulting active-project layout.
    // entries 表示一次置顶转换以及完整的结果活动项目布局。
    let entries = project_layout_mutation_entries(
        pinned_project_ids,
        regular_project_ids,
        Some((project_id, pinned)),
    )?;
    persist_project_layout_mutation(host_runtime, &entries, "toggle the project pin state").await
}

/// Persists one active project's display name without touching its sessions.
/// 持久化单个活动项目的显示名称，不触碰其会话数据。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `project_id` is the exact active project identity to rename.
/// `project_id` 是需要重命名的精确活动项目身份。
/// `name` is the requested display name or an empty reset request.
/// `name` 是请求的显示名称或空名称还原请求。
///
/// # Returns
/// 返回值
/// Returns success only after the normalized name is returned for one exact active identity.
/// 仅在规范化名称对应的一条精确活动身份返回后成功。
pub async fn rename_project(
    host_runtime: HostRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<(), String> {
    let normalized_name = resolve_project_rename_target(&host_runtime, project_id, name).await?;

    // result proves the normalized name was committed to the exact active project row.
    // result 证明规范化名称已提交到精确活动项目行。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
UPDATE projects
SET name = ?1, updated_at = ?2
WHERE id = ?3 AND archived = 0
RETURNING id;
"#,
        vec![
            SqliteValue::Text(normalized_name.to_string()),
            SqliteValue::Text(current_unix_millis_text()),
            SqliteValue::Text(project_id.to_string()),
        ],
    )
    .await
    .map_err(|error| format!("Failed to rename the project: {error}"))?;
    // rows must contain the exact active identity renamed by SQLite.
    // rows 必须包含 SQLite 重命名的精确活动身份。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "rename the project")
}

/// Persists one active project's complete icon override pair.
/// 持久化单个活动项目的完整图标覆盖对。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `project_id` is the exact active project identity to update.
/// `project_id` 是需要更新的精确活动项目身份。
/// `icon_pack` is the optional non-blank icon collection key.
/// `icon_pack` 是可选的非空白图标集合键。
/// `icon_key` is the optional non-blank icon identity paired with `icon_pack`.
/// `icon_key` 是与 `icon_pack` 配对的可选非空白图标身份。
///
/// # Returns
/// 返回值
/// Returns success only after one exact active project returns from the guarded update.
/// 仅在受保护更新返回一个精确活动项目后成功。
pub async fn persist_project_icon_update(
    host_runtime: HostRuntimeHandle,
    project_id: &str,
    icon_pack: Option<&str>,
    icon_key: Option<&str>,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "update the project icon")?;
    // normalized_icon_pair preserves the invariant that icon pack and key are both present or both absent.
    // normalized_icon_pair 保持图标集合与图标键同时存在或同时缺失的不变量。
    let (normalized_icon_pack, normalized_icon_key) =
        normalize_project_icon_pair(icon_pack, icon_key)?;
    // result proves the icon pair was committed to one exact active row.
    // result 证明图标对已提交到一个精确活动行。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
UPDATE projects
SET icon_pack = ?1, icon_key = ?2, updated_at = ?3
WHERE id = ?4 AND archived = 0
RETURNING id;
"#,
        vec![
            optional_sqlite_string(&normalized_icon_pack),
            optional_sqlite_string(&normalized_icon_key),
            SqliteValue::Text(current_unix_millis_text()),
            SqliteValue::Text(project_id.to_string()),
        ],
    )
    .await
    .map_err(|error| format!("Failed to update the project icon: {error}"))?;
    // rows must contain the exact active identity whose icon pair changed.
    // rows 必须包含图标对发生变化的精确活动身份。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "update the project icon")
}

/// Stores how one active project orders its sessions in the sidebar.
/// 保存一个活动项目在侧栏中的会话排序方式。
///
/// # Parameters
/// 参数
/// `project_id` is the exact active project; `mode` is the chosen sort mode, stored explicitly even when it
/// equals the default so the user's choice survives a later default change.
/// `project_id` 为精确的活动项目；`mode` 为所选排序方式。即使等于默认值也显式存储，使用户的选择不受日后默认值变化影响。
///
/// # Returns
/// 返回值
/// Returns success only after the preference is written for one exact active project.
/// 仅在偏好写入一个精确活动项目后返回成功。
pub async fn persist_project_session_sort_mode(
    host_runtime: HostRuntimeHandle,
    project_id: &str,
    mode: crate::model::ProjectSessionSortMode,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "set the session sort mode")?;
    // result proves the preference row belongs to one exact active project.
    // result 证明偏好行属于一个精确的活动项目。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
INSERT INTO project_session_preferences (project_id, session_sort_mode)
SELECT id, ?2 FROM projects WHERE id = ?1 AND archived = 0
ON CONFLICT(project_id) DO UPDATE SET session_sort_mode = excluded.session_sort_mode
RETURNING project_id AS id;
"#,
        vec![
            SqliteValue::Text(project_id.to_string()),
            SqliteValue::Text(mode.as_str().to_string()),
        ],
    )
    .await
    .map_err(|error| format!("Failed to set the session sort mode: {error}"))?;
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "set the session sort mode")
}

/// Normalizes and validates one optional project icon pair.
/// 规范化并校验一组可选项目图标对。
///
/// # Parameters
/// 参数
/// `icon_pack` is the optional icon collection key.
/// `icon_pack` 是可选图标集合键。
/// `icon_key` is the optional icon identity.
/// `icon_key` 是可选图标身份。
///
/// # Returns
/// 返回值
/// Returns either two normalized values, two absent values, or an explicit pair validation error.
/// 返回两个规范化值、两个缺失值，或显式的配对校验错误。
fn normalize_project_icon_pair(
    icon_pack: Option<&str>,
    icon_key: Option<&str>,
) -> Result<(Option<String>, Option<String>), String> {
    match (icon_pack, icon_key) {
        (None, None) => Ok((None, None)),
        (Some(icon_pack), Some(icon_key)) => {
            // normalized_icon_pack rejects whitespace-only icon collection keys.
            // normalized_icon_pack 拒绝仅含空白的图标集合键。
            let normalized_icon_pack = icon_pack.trim();
            // normalized_icon_key rejects whitespace-only icon identities.
            // normalized_icon_key 拒绝仅含空白的图标身份。
            let normalized_icon_key = icon_key.trim();
            if normalized_icon_pack.is_empty() || normalized_icon_key.is_empty() {
                return Err(String::from(
                    "Failed to update the project icon: the icon set and icon key must not be empty.",
                ));
            }
            Ok((
                Some(normalized_icon_pack.to_string()),
                Some(normalized_icon_key.to_string()),
            ))
        }
        _ => Err(String::from(
            "Failed to update the project icon: the icon set and icon key must be provided or cleared together.",
        )),
    }
}

/// Resolves an empty project rename into the owning folder name.
/// 将空项目重命名解析为所属文件夹名称。
pub(super) async fn resolve_project_rename_target(
    host_runtime: &HostRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<String, String> {
    let normalized_name = name.trim();
    if !normalized_name.is_empty() && !is_path_separator_only_project_name(normalized_name) {
        return Ok(normalized_name.to_string());
    }

    /// Raw project path row decoded for empty-name reset.
    /// 为项目空名还原解码的原始项目路径行。
    #[derive(Debug, Clone, Deserialize)]
    struct ProjectPathRow {
        /// Absolute project root path stored in SQLite.
        /// SQLite 中存储的项目绝对根路径。
        root_path: String,
    }

    let result = local_sql::query_json(
        host_runtime.local.as_ref(),
        LogicalDatabase::AppState,
        "SELECT root_path FROM projects WHERE id = ?1 AND archived = 0 LIMIT 1;",
        &[SqliteValue::Text(project_id.to_string())],
    )
    .map_err(|error| format!("Failed to query the project path: {error}"))?;
    let row = decode_query_rows::<ProjectPathRow>(&result.json_data)?
        .into_iter()
        .next()
        .ok_or_else(|| String::from("The project to rename was not found."))?;
    let fallback_name = project_folder_name_from_path_text(&row.root_path)
        .ok_or_else(|| String::from("A default name cannot be derived from the project path."))?;
    Ok(fallback_name)
}

/// Detects invalid slash-only project display names and treats them as reset requests.
/// 检测仅由斜杠组成的无效项目显示名称，并将其视为还原请求。
pub(super) fn is_path_separator_only_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| matches!(character, '\\' | '/'))
}

/// Extracts the final folder segment from a Windows or Unix path string.
/// 从 Windows 或 Unix 路径字符串中提取最后一级文件夹名称。
pub(super) fn project_folder_name_from_path_text(path: &str) -> Option<String> {
    path.split(['\\', '/'])
        .rfind(|segment| !segment.trim().is_empty())
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
}

/// Persists both complete active project sections without changing their pinned membership.
/// 在不改变置顶成员关系的前提下持久化两个完整活动项目分区。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the authoritative SQLite binding.
/// `host_runtime` 持有权威 SQLite 绑定。
/// `pinned_project_ids` is the complete resulting pinned-section order.
/// `pinned_project_ids` 是完整的结果置顶分区顺序。
/// `regular_project_ids` is the complete resulting regular-section order.
/// `regular_project_ids` 是完整的结果普通分区顺序。
///
/// # Returns
/// 返回值
/// Returns success only after one atomic expected-state update returns every active project.
/// 仅在一次原子预期状态更新返回每个活动项目后成功。
pub async fn persist_project_order_update(
    host_runtime: HostRuntimeHandle,
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
) -> Result<(), String> {
    // entries require every active project's current pin state and resulting section order.
    // entries 要求包含每个活动项目的当前置顶状态与结果分区顺序。
    let entries = project_layout_mutation_entries(pinned_project_ids, regular_project_ids, None)?;
    persist_project_layout_mutation(host_runtime, &entries, "reorder projects").await
}

/// Persists the collapsed task-project identifiers as workspace-level UI state.
/// 将已折叠任务项目标识作为工作台级 UI 状态持久化。
pub async fn persist_collapsed_task_project_ids(
    host_runtime: HostRuntimeHandle,
    collapsed_project_ids: &[String],
) -> Result<(), String> {
    let value = serde_json::to_string(collapsed_project_ids).map_err(|error| {
        format!("Failed to serialize the workbench project expansion state: {error}")
    })?;
    local_sql::execute_batch_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
INSERT INTO app_metadata (key, value)
VALUES (?1, ?2)
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
"#,
        vec![vec![
            SqliteValue::Text(COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string()),
            SqliteValue::Text(value),
        ]],
    )
    .await
    .map_err(|error| format!("Failed to write the workbench project expansion state: {error}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds owned project identities for project-layout helper tests.
    /// 为项目布局辅助测试构建拥有所有权的项目身份。
    ///
    /// # Parameters
    /// 参数
    /// `project_ids` contains borrowed identity literals in display order.
    /// `project_ids` 包含按展示顺序排列的借用身份字面量。
    ///
    /// # Returns
    /// 返回值
    /// Returns owned identity strings in the same order.
    /// 返回保持相同顺序的拥有所有权身份字符串。
    fn owned_project_ids(project_ids: &[&str]) -> Vec<String> {
        project_ids
            .iter()
            .map(|project_id| (*project_id).to_string())
            .collect()
    }

    /// Verify project SQL encoding preserves text boundaries and rejects unsupported scalar kinds.
    /// 验证项目 SQL 编码保留文本边界，并拒绝不支持的标量种类。
    /// No inputs or output; checks the exact encoder used by the atomic creation request.
    /// 无输入或输出；检查原子创建请求使用的精确编码器。
    #[test]
    fn project_persistence_values_preserve_exact_scalar_types() {
        assert_eq!(
            project_persistence_sql_values(&[
                SqliteValue::Integer(i64::MIN),
                SqliteValue::Integer(i64::MAX),
                SqliteValue::Null,
                SqliteValue::Text(String::from("中文'\0;--")),
                SqliteValue::Text(String::new()),
            ])
            .expect("project scalar subset"),
            "-9223372036854775808, 9223372036854775807, NULL, '中文''' || char(0) || ';--', ''",
        );
        // These variants are not produced by either project parameter builder. The engine has no
        // boolean variant at all, so a boolean can only reach storage as an integer and there is
        // nothing left to refuse.
        // 两个项目参数构建器都不会产生这些变体。引擎根本没有布尔变体，因此布尔值只能作为整数进入存储，
        // 已没有需要拒绝的东西。
        for unsupported in [SqliteValue::Real(1.5), SqliteValue::Blob(vec![1])] {
            assert!(project_persistence_sql_values(&[unsupported]).is_err());
        }
    }

    /// Verifies guarded single-row mutations require one exact returned identity.
    /// 验证受保护单行变更必须返回一个精确身份。
    #[test]
    fn project_mutation_identity_rows_require_exact_target() {
        // matching_rows model the only valid guarded mutation result.
        // matching_rows 模拟唯一有效的受保护变更结果。
        let matching_rows = vec![ProjectMutationIdentityRow {
            id: String::from("project-a"),
        }];
        // mismatched_rows model a corrupted or unrelated engine response.
        // mismatched_rows 模拟损坏或无关的 SQLite 结果。
        let mismatched_rows = vec![ProjectMutationIdentityRow {
            id: String::from("project-b"),
        }];

        assert!(
            validate_project_mutation_identity_rows(
                "project-a",
                &matching_rows,
                "test project mutation"
            )
            .is_ok()
        );
        assert!(
            validate_project_mutation_identity_rows("project-a", &[], "test project mutation")
                .expect_err("missing mutation row should fail")
                .contains("state changed")
        );
        assert!(
            validate_project_mutation_identity_rows(
                "project-a",
                &mismatched_rows,
                "test project mutation",
            )
            .expect_err("mismatched mutation identity should fail")
            .contains("unexpected project identifier")
        );
    }

    /// Verifies full-layout requests reject duplicate and inconsistent transition identities.
    /// 验证完整布局请求会拒绝重复及转换不一致的身份。
    #[test]
    fn project_layout_entries_require_unique_sections_and_exact_transition() {
        // pinned_ids and regular_ids model the complete resulting active layout.
        // pinned_ids 与 regular_ids 模拟完整的结果活动布局。
        let pinned_ids = owned_project_ids(&["project-a", "project-b"]);
        let regular_ids = owned_project_ids(&["project-c"]);
        // entries model project-b moving from regular to the resulting pinned section.
        // entries 模拟 project-b 从普通分区移动到结果置顶分区。
        let entries =
            project_layout_mutation_entries(&pinned_ids, &regular_ids, Some(("project-b", true)))
                .expect("complete pin transition layout should validate");

        assert_eq!(3, entries.len());
        assert_eq!("project-b", entries[1].id);
        assert!(!entries[1].expected_pinned);
        assert!(entries[1].pinned);
        assert_eq!(1, entries[1].sort_order);
        assert!(
            project_layout_mutation_entries(
                &owned_project_ids(&["project-a"]),
                &owned_project_ids(&["project-a"]),
                None,
            )
            .expect_err("duplicate project identity should fail")
            .contains("duplicated")
        );
        assert!(
            project_layout_mutation_entries(&pinned_ids, &regular_ids, Some(("project-b", false)),)
                .expect_err("transition target in wrong section should fail")
                .contains("wrong section")
        );
        assert!(
            project_layout_mutation_entries(
                &pinned_ids,
                &regular_ids,
                Some(("missing-project", true)),
            )
            .expect_err("missing transition target should fail")
            .contains("not in the complete layout")
        );
    }

    /// Verifies returned layout rows must exactly reproduce every requested result state.
    /// 验证返回的布局行必须精确复现每个请求结果状态。
    #[test]
    fn project_layout_rows_require_exact_complete_result() {
        // entries contain one unchanged pinned row and one pin transition result.
        // entries 包含一个未改变的置顶行以及一个置顶转换结果。
        let entries = project_layout_mutation_entries(
            &owned_project_ids(&["project-a", "project-b"]),
            &owned_project_ids(&["project-c"]),
            Some(("project-b", true)),
        )
        .expect("test layout should validate");
        // matching_rows deliberately use a different response order to prove identity-based validation.
        // matching_rows 有意使用不同响应顺序，以证明校验按身份进行。
        let matching_rows = vec![
            ProjectLayoutMutationRow {
                id: String::from("project-c"),
                pinned: 0,
                sort_order: 0,
            },
            ProjectLayoutMutationRow {
                id: String::from("project-a"),
                pinned: 1,
                sort_order: 0,
            },
            ProjectLayoutMutationRow {
                id: String::from("project-b"),
                pinned: 1,
                sort_order: 1,
            },
        ];
        // invalid_flag_rows model a corrupted non-boolean pinned state.
        // invalid_flag_rows 模拟损坏的非布尔置顶状态。
        let mut invalid_flag_rows = matching_rows.clone();
        invalid_flag_rows[0].pinned = 2;
        // wrong_order_rows model an incomplete or conflicting sort result.
        // wrong_order_rows 模拟不完整或冲突的排序结果。
        let mut wrong_order_rows = matching_rows.clone();
        wrong_order_rows[2].sort_order = 0;

        assert!(
            validate_project_layout_mutation_rows(&entries, &matching_rows, "test layout").is_ok()
        );
        assert!(
            validate_project_layout_mutation_rows(&entries, &matching_rows[..2], "test layout")
                .expect_err("partial returned layout should fail")
                .contains("state changed")
        );
        assert!(
            validate_project_layout_mutation_rows(&entries, &invalid_flag_rows, "test layout")
                .expect_err("invalid pinned flag should fail")
                .contains("invalid pinned flag")
        );
        assert!(
            validate_project_layout_mutation_rows(&entries, &wrong_order_rows, "test layout")
                .expect_err("wrong returned order should fail")
                .contains("returned layout")
        );
    }

    /// Verifies the generated layout statement guards the full active set and expected pin states.
    /// 验证生成的布局语句会保护完整活动集合与预期置顶状态。
    #[test]
    fn project_layout_statement_guards_complete_expected_active_set() {
        // statement represents three projects and therefore places updated_at at parameter thirteen.
        // statement 表示三个项目，因此将 updated_at 放在第十三个参数。
        let statement =
            project_layout_mutation_statement(3).expect("non-empty layout statement should build");

        assert!(statement.contains("current_active(id, pinned) AS MATERIALIZED"));
        assert!(statement.contains("requested.expected_pinned = current_active.pinned"));
        assert!(statement.contains("(SELECT COUNT(*) FROM current_active)"));
        assert!(statement.contains("updated_at = ?13"));
        assert!(statement.contains("RETURNING id, pinned, sort_order"));
        assert!(
            project_layout_mutation_statement(0)
                .expect_err("empty layout statement should fail")
                .contains("must not be empty")
        );
    }

    /// Verifies project icon persistence requires a complete non-blank pair or a complete reset.
    /// 验证项目图标持久化要求完整非空白对或完整清除。
    #[test]
    fn project_icon_pair_requires_complete_non_blank_values() {
        assert_eq!(
            (Some(String::from("lucide")), Some(String::from("folder"))),
            normalize_project_icon_pair(Some(" lucide "), Some(" folder "))
                .expect("complete icon pair should normalize")
        );
        assert_eq!(
            (None, None),
            normalize_project_icon_pair(None, None).expect("complete icon reset should validate")
        );
        assert!(normalize_project_icon_pair(Some("lucide"), None).is_err());
        assert!(normalize_project_icon_pair(Some("   "), Some("folder")).is_err());
    }
}
