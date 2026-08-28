use super::*;

/// Inserts or refreshes one active project row in the application index.
/// 在应用索引中插入或刷新一条活动项目记录。
///
/// # Parameters / 参数
///
/// - `controller`: Controller runtime owning the application SQLite binding.
///   `controller`：持有应用 SQLite 绑定的 Controller 运行时。
/// - `project`: Complete active project projection to persist.
///   `project`：需要持久化的完整活动项目投影。
///
/// # Returns / 返回值
///
/// Success after the exact project row is committed, or an explicit storage error.
/// 精确项目记录提交后返回成功，否则返回显式存储错误。
async fn upsert_active_project(
    controller: &ControllerRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    // now is shared by created_at and updated_at for a newly inserted project row.
    // now 由新插入项目行的 created_at 与 updated_at 共享。
    let now = current_unix_millis_text();
    // sort_order uses a checked conversion so project order cannot truncate on wide usize hosts.
    // sort_order 使用受检查转换，避免项目顺序在宽 usize 宿主上发生截断。
    let sort_order = i64::try_from(project.order)
        .map_err(|_| String::from("写入活动项目失败：项目顺序超出 SQLite 整数范围。"))?;
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO projects (
    id,
    name,
    root_path,
    pinned,
    archived,
    sort_order,
    icon_pack,
    icon_key,
    codebase_memory_enabled,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    root_path = excluded.root_path,
    pinned = excluded.pinned,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    codebase_memory_enabled = excluded.codebase_memory_enabled,
    updated_at = excluded.updated_at;
"#,
            vec![vec![
                ControllerSqliteValue::String(project.id.clone()),
                ControllerSqliteValue::String(project.title.clone()),
                ControllerSqliteValue::String(project.path.clone()),
                ControllerSqliteValue::Int64(i64::from(project.pinned)),
                ControllerSqliteValue::Int64(0),
                ControllerSqliteValue::Int64(sort_order),
                optional_sqlite_string(&project.icon_pack),
                optional_sqlite_string(&project.icon_key),
                ControllerSqliteValue::Int64(i64::from(project.codebase_memory_enabled)),
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(now),
            ]],
        )
        .await
        .map_err(|error| format!("写入活动项目失败：{error}"))?;

    Ok(())
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
        return Err(format!("{action_description}失败：项目标识不能为空。"));
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
            "{action_description}失败：活动项目 `{project_id}` 不存在或状态已改变。"
        )),
        [row] => Err(format!(
            "{action_description}失败：SQLite 返回意外项目标识 `{}`，预期 `{project_id}`。",
            row.id
        )),
        _ => Err(format!(
            "{action_description}失败：SQLite 返回 {} 条项目身份，预期恰好一条。",
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
        validate_project_mutation_id(project_id, "更新项目布局")?;
        if !seen_project_ids.insert(project_id.clone()) {
            return Err(format!(
                "更新项目布局失败：项目标识 `{project_id}` 在布局请求中重复。"
            ));
        }
        // sort_order uses a checked conversion so an oversized layout cannot silently truncate.
        // sort_order 使用受检查转换，避免超大布局被静默截断。
        let sort_order = i64::try_from(index)
            .map_err(|_| String::from("更新项目布局失败：项目数量超出 SQLite 顺序范围。"))?;
        // expected_pinned differs only for the explicitly requested pin transition.
        // expected_pinned 仅对显式请求的置顶转换项目取相反值。
        let expected_pinned = match pin_transition {
            Some((transition_project_id, next_pinned)) if transition_project_id == project_id => {
                if next_pinned != pinned {
                    return Err(format!(
                        "更新项目布局失败：置顶转换项目 `{project_id}` 位于错误分区。"
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
            .ok_or_else(|| String::from("更新项目布局失败：项目数量溢出。"))?,
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
        return Err(String::from("更新项目布局失败：完整项目布局不能为空。"));
    }
    if let Some((transition_project_id, _next_pinned)) = pin_transition
        && !seen_project_ids.contains(transition_project_id)
    {
        return Err(format!(
            "更新项目布局失败：置顶转换项目 `{transition_project_id}` 不在完整布局中。"
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
        return Err(String::from("更新项目布局失败：完整项目布局不能为空。"));
    }
    // value_rows assigns four stable parameters to every requested project transition.
    // value_rows 为每个请求的项目转换分配四个稳定参数。
    let mut value_rows = Vec::with_capacity(entry_count);
    for index in 0..entry_count {
        // parameter_offset is checked before deriving the one-based SQLite parameter indices.
        // parameter_offset 在派生 SQLite 一基参数索引前执行溢出检查。
        let parameter_offset = index
            .checked_mul(4)
            .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
        let id_parameter = parameter_offset
            .checked_add(1)
            .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
        let expected_pinned_parameter = parameter_offset
            .checked_add(2)
            .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
        let pinned_parameter = parameter_offset
            .checked_add(3)
            .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
        let sort_order_parameter = parameter_offset
            .checked_add(4)
            .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
        value_rows.push(format!(
            "(?{id_parameter}, ?{expected_pinned_parameter}, ?{pinned_parameter}, ?{sort_order_parameter})"
        ));
    }
    // updated_at_parameter follows all per-project values in the typed parameter vector.
    // updated_at_parameter 紧跟类型化参数向量中的全部逐项目值。
    let updated_at_parameter = entry_count
        .checked_mul(4)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
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
            "{action_description}失败：活动项目集合或置顶状态已改变，SQLite 返回 {} 行，预期 {} 行。",
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
    // returned_project_ids rejects duplicate or fabricated controller response rows.
    // returned_project_ids 用于拒绝控制器响应中的重复或伪造行。
    let mut returned_project_ids = BTreeSet::new();
    for row in rows {
        let Some((expected_pinned, expected_sort_order)) =
            expected_layout.get(row.id.as_str()).copied()
        else {
            return Err(format!(
                "{action_description}失败：SQLite 返回未请求的项目标识 `{}`。",
                row.id
            ));
        };
        if !returned_project_ids.insert(row.id.as_str()) {
            return Err(format!(
                "{action_description}失败：SQLite 重复返回项目标识 `{}`。",
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
                    "{action_description}失败：项目 `{}` 返回非法置顶标记 `{value}`。",
                    row.id
                ));
            }
        };
        if returned_pinned != expected_pinned || row.sort_order != expected_sort_order {
            return Err(format!(
                "{action_description}失败：项目 `{}` 返回布局 pinned={}, sort_order={}，预期 pinned={}, sort_order={}。",
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
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
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
    controller: ControllerRuntimeHandle,
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
        .ok_or_else(|| String::from("更新项目布局失败：SQLite 参数数量溢出。"))?;
    // parameters follow the exact numbered-placeholder order generated above.
    // parameters 严格遵循上面生成的编号占位符顺序。
    let mut parameters = Vec::with_capacity(parameter_capacity);
    for entry in entries {
        parameters.push(ControllerSqliteValue::String(entry.id.clone()));
        parameters.push(ControllerSqliteValue::Int64(i64::from(
            entry.expected_pinned,
        )));
        parameters.push(ControllerSqliteValue::Int64(i64::from(entry.pinned)));
        parameters.push(ControllerSqliteValue::Int64(entry.sort_order));
    }
    parameters.push(ControllerSqliteValue::String(current_unix_millis_text()));
    // result proves the full guarded layout statement updated every requested active project.
    // result 证明完整受保护布局语句更新了每个请求的活动项目。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            &statement,
            parameters,
        )
        .await
        .map_err(|error| format!("{action_description}失败：{error}"))?;
    // rows retain the authoritative result state instead of trusting write completion alone.
    // rows 保留权威结果状态，而不是只相信写入调用完成。
    let rows = decode_query_rows::<ProjectLayoutMutationRow>(&result.json_data)?;
    validate_project_layout_mutation_rows(entries, &rows, action_description)
}

/// Persists one newly visible project row without rewriting unrelated workspace rows.
/// 持久化单个新出现的可见项目行，不重写无关工作区行。
pub async fn persist_new_project(
    controller: ControllerRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    upsert_active_project(&controller, project).await
}

/// Updates the archived flag for one project row without changing its metadata.
/// 只更新单个项目行的归档标记，不改变其元数据。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
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
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    expected_archived: bool,
    archived: bool,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "更新项目归档状态")?;
    if expected_archived == archived {
        return Err(String::from(
            "更新项目归档状态失败：预期状态与结果状态不能相同。",
        ));
    }
    // result proves the exact expected lifecycle transition changed one project row.
    // result 证明精确的预期生命周期转换变更了一条项目行。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
UPDATE projects
SET archived = ?1, updated_at = ?2
WHERE id = ?3 AND archived = ?4
RETURNING id;
"#,
            vec![
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
                ControllerSqliteValue::Int64(i64::from(expected_archived)),
            ],
        )
        .await
        .map_err(|error| format!("更新项目归档状态失败：{error}"))?;
    // rows must contain the one project whose lifecycle predicate matched.
    // rows 必须包含生命周期谓词命中的唯一项目。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "更新项目归档状态")
}

/// Deletes one active project row while preserving archived session directories.
/// 删除单个活动项目记录，同时保留归档会话目录。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
/// `project_id` is the exact active project identity to delete.
/// `project_id` 是需要删除的精确活动项目身份。
///
/// # Returns
/// 返回值
/// Returns success only after SQLite returns the deleted active identity.
/// 仅在 SQLite 返回已删除活动身份后成功。
pub(super) async fn delete_active_project_row(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "移除项目记录")?;
    // result proves the active project row still existed at deletion time.
    // result 证明活动项目行在删除时仍然存在。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "DELETE FROM projects WHERE id = ?1 AND archived = 0 RETURNING id;",
            vec![ControllerSqliteValue::String(project_id.to_string())],
        )
        .await
        .map_err(|error| format!("移除项目记录失败：{error}"))?;
    // rows must contain the one active identity deleted by SQLite.
    // rows 必须包含 SQLite 删除的唯一活动身份。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "移除项目记录")
}

/// Reactivates one exact archived project row after every child V1 session has been restored.
/// 在每个子 V1 会话均已恢复后重新激活一条精确的归档项目行。
///
/// # Parameters / 参数
///
/// - `controller`: Authoritative APP SQLite runtime.
///   `controller`：权威 APP SQLite 运行时。
/// - `project_id`: Exact archived project identity.
///   `project_id`：精确的归档项目身份。
///
/// # Returns / 返回值
///
/// Success only when the guarded archived-to-active transition returns the exact row.
/// 仅在受保护的归档到活动转换返回精确行时成功。
pub async fn restore_archived_project_row_v1(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    update_project_archive_flag(controller, project_id, true, false).await
}

/// Deletes one exact active project row after every child V1 session has been archived.
/// 在每个子 V1 会话均已归档后删除一条精确的活动项目行。
///
/// # Parameters / 参数
///
/// - `controller`: Authoritative APP SQLite runtime.
///   `controller`：权威 APP SQLite 运行时。
/// - `project_id`: Exact active project identity.
///   `project_id`：精确的活动项目身份。
///
/// # Returns / 返回值
///
/// Success only when SQLite deletes and returns the exact active row.
/// 仅在 SQLite 删除并返回精确活动行时成功。
pub async fn delete_active_project_row_v1(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    delete_active_project_row(controller, project_id).await
}

/// Atomically persists one pin transition and both complete project-section orders.
/// 原子持久化一次置顶转换以及两个完整项目分区的顺序。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
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
    controller: ControllerRuntimeHandle,
    project_id: &str,
    pinned: bool,
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "切换项目置顶状态")?;
    // entries model one pin transition plus the complete resulting active-project layout.
    // entries 表示一次置顶转换以及完整的结果活动项目布局。
    let entries = project_layout_mutation_entries(
        pinned_project_ids,
        regular_project_ids,
        Some((project_id, pinned)),
    )?;
    persist_project_layout_mutation(controller, &entries, "切换项目置顶状态").await
}

/// Persists one active project's display name without touching its sessions.
/// 持久化单个活动项目的显示名称，不触碰其会话数据。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
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
    controller: ControllerRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<(), String> {
    let normalized_name = resolve_project_rename_target(&controller, project_id, name).await?;

    // result proves the normalized name was committed to the exact active project row.
    // result 证明规范化名称已提交到精确活动项目行。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            r#"
UPDATE projects
SET name = ?1, updated_at = ?2
WHERE id = ?3 AND archived = 0
RETURNING id;
"#,
            vec![
                ControllerSqliteValue::String(normalized_name.to_string()),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ],
        )
        .await
        .map_err(|error| format!("重命名项目失败：{error}"))?;
    // rows must contain the exact active identity renamed by SQLite.
    // rows 必须包含 SQLite 重命名的精确活动身份。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "重命名项目")
}

/// Persists the host-managed Codebase Memory enablement for one exact active project.
/// 为一个精确活动项目持久化宿主管理的 Codebase Memory 启用状态。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
/// `project_id` is the exact active project identity to update.
/// `project_id` 是需要更新的精确活动项目身份。
/// `enabled` is the resulting project-owned enablement state.
/// `enabled` 是项目实体最终持有的启用状态。
///
/// # Returns
/// 返回值
/// Returns success only after the guarded update returns the exact active project identity.
/// 仅在受保护更新返回精确活动项目身份后成功。
pub async fn persist_project_codebase_memory_enabled(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    enabled: bool,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "更新 Codebase Memory 启用状态")?;
    // result proves that the setting belongs to an existing active project, never to a path alias.
    // result 证明该设置属于现有活动项目，而不是某个路径别名。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            r#"
UPDATE projects
SET codebase_memory_enabled = ?1, updated_at = ?2
WHERE id = ?3 AND archived = 0
RETURNING id;
"#,
            vec![
                ControllerSqliteValue::Int64(i64::from(enabled)),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ],
        )
        .await
        .map_err(|error| format!("更新 Codebase Memory 启用状态失败：{error}"))?;
    // rows must contain the one active project whose project-owned setting changed.
    // rows 必须包含项目自有设置发生变化的唯一活动项目。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "更新 Codebase Memory 启用状态")
}

/// Persists one active project's complete icon override pair.
/// 持久化单个活动项目的完整图标覆盖对。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
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
    controller: ControllerRuntimeHandle,
    project_id: &str,
    icon_pack: Option<&str>,
    icon_key: Option<&str>,
) -> Result<(), String> {
    validate_project_mutation_id(project_id, "更新项目图标")?;
    // normalized_icon_pair preserves the invariant that icon pack and key are both present or both absent.
    // normalized_icon_pair 保持图标集合与图标键同时存在或同时缺失的不变量。
    let (normalized_icon_pack, normalized_icon_key) =
        normalize_project_icon_pair(icon_pack, icon_key)?;
    // result proves the icon pair was committed to one exact active row.
    // result 证明图标对已提交到一个精确活动行。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            r#"
UPDATE projects
SET icon_pack = ?1, icon_key = ?2, updated_at = ?3
WHERE id = ?4 AND archived = 0
RETURNING id;
"#,
            vec![
                optional_sqlite_string(&normalized_icon_pack),
                optional_sqlite_string(&normalized_icon_key),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ],
        )
        .await
        .map_err(|error| format!("更新项目图标失败：{error}"))?;
    // rows must contain the exact active identity whose icon pair changed.
    // rows 必须包含图标对发生变化的精确活动身份。
    let rows = decode_query_rows::<ProjectMutationIdentityRow>(&result.json_data)?;
    validate_project_mutation_identity_rows(project_id, &rows, "更新项目图标")
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
                return Err(String::from("更新项目图标失败：图标集合与图标键不能为空。"));
            }
            Ok((
                Some(normalized_icon_pack.to_string()),
                Some(normalized_icon_key.to_string()),
            ))
        }
        _ => Err(String::from(
            "更新项目图标失败：图标集合与图标键必须同时提供或同时清除。",
        )),
    }
}

/// Resolves an empty project rename into the owning folder name.
/// 将空项目重命名解析为所属文件夹名称。
pub(super) async fn resolve_project_rename_target(
    controller: &ControllerRuntimeHandle,
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

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT root_path FROM projects WHERE id = ?1 AND archived = 0 LIMIT 1;",
            vec![ControllerSqliteValue::String(project_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询项目路径失败：{error}"))?;
    let row = decode_query_rows::<ProjectPathRow>(&result.json_data)?
        .into_iter()
        .next()
        .ok_or_else(|| String::from("没有找到要重命名的项目。"))?;
    let fallback_name = project_folder_name_from_path_text(&row.root_path)
        .ok_or_else(|| String::from("项目路径无法生成默认名称。"))?;
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
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
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
    controller: ControllerRuntimeHandle,
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
) -> Result<(), String> {
    // entries require every active project's current pin state and resulting section order.
    // entries 要求包含每个活动项目的当前置顶状态与结果分区顺序。
    let entries = project_layout_mutation_entries(pinned_project_ids, regular_project_ids, None)?;
    persist_project_layout_mutation(controller, &entries, "重排项目").await
}

/// Persists the collapsed task-project identifiers as workspace-level UI state.
/// 将已折叠任务项目标识作为工作台级 UI 状态持久化。
pub async fn persist_collapsed_task_project_ids(
    controller: ControllerRuntimeHandle,
    collapsed_project_ids: &[String],
) -> Result<(), String> {
    let value = serde_json::to_string(collapsed_project_ids)
        .map_err(|error| format!("序列化工作台项目展开状态失败：{error}"))?;
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            r#"
INSERT INTO app_metadata (key, value)
VALUES (?1, ?2)
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
"#,
            vec![vec![
                ControllerSqliteValue::String(COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string()),
                ControllerSqliteValue::String(value),
            ]],
        )
        .await
        .map_err(|error| format!("写入工作台项目展开状态失败：{error}"))?;

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

    /// Verifies guarded single-row mutations require one exact returned identity.
    /// 验证受保护单行变更必须返回一个精确身份。
    #[test]
    fn project_mutation_identity_rows_require_exact_target() {
        // matching_rows model the only valid guarded mutation result.
        // matching_rows 模拟唯一有效的受保护变更结果。
        let matching_rows = vec![ProjectMutationIdentityRow {
            id: String::from("project-a"),
        }];
        // mismatched_rows model a corrupted or unrelated controller response.
        // mismatched_rows 模拟损坏或无关的控制器响应。
        let mismatched_rows = vec![ProjectMutationIdentityRow {
            id: String::from("project-b"),
        }];

        assert!(
            validate_project_mutation_identity_rows("project-a", &matching_rows, "测试项目变更")
                .is_ok()
        );
        assert!(
            validate_project_mutation_identity_rows("project-a", &[], "测试项目变更")
                .expect_err("missing mutation row should fail")
                .contains("状态已改变")
        );
        assert!(
            validate_project_mutation_identity_rows("project-a", &mismatched_rows, "测试项目变更",)
                .expect_err("mismatched mutation identity should fail")
                .contains("意外项目标识")
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
            .contains("重复")
        );
        assert!(
            project_layout_mutation_entries(&pinned_ids, &regular_ids, Some(("project-b", false)),)
                .expect_err("transition target in wrong section should fail")
                .contains("错误分区")
        );
        assert!(
            project_layout_mutation_entries(
                &pinned_ids,
                &regular_ids,
                Some(("missing-project", true)),
            )
            .expect_err("missing transition target should fail")
            .contains("不在完整布局")
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
            validate_project_layout_mutation_rows(&entries, &matching_rows, "测试布局").is_ok()
        );
        assert!(
            validate_project_layout_mutation_rows(&entries, &matching_rows[..2], "测试布局")
                .expect_err("partial returned layout should fail")
                .contains("状态已改变")
        );
        assert!(
            validate_project_layout_mutation_rows(&entries, &invalid_flag_rows, "测试布局")
                .expect_err("invalid pinned flag should fail")
                .contains("非法置顶标记")
        );
        assert!(
            validate_project_layout_mutation_rows(&entries, &wrong_order_rows, "测试布局")
                .expect_err("wrong returned order should fail")
                .contains("返回布局")
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
                .contains("不能为空")
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
