//! Bounded immutable history pages and exact historical document windows.
//! 有界不可变历史页及精确历史文档窗口。

use super::*;

/// Validates `start`/`end` inclusive read options using the shared line cap; returns their concrete range.
/// 使用共享行上限验证包含边界的 `start`、`end` 读取选项；返回具体范围。
pub(super) fn read_range(
    start: Option<usize>,
    end: Option<usize>,
) -> Result<(usize, usize), String> {
    // Omitted ranges are declared pagination defaults, never alternate document selectors.
    // 省略范围是已声明分页默认值，绝非其他文档选择器。
    let start = start.unwrap_or(1);
    // Saturation keeps integer extremes invalid without wraparound.
    // 饱和计算使极端整数保持无效，避免回绕。
    let end = end.unwrap_or_else(|| start.saturating_add(MAX_KNOWLEDGE_READ_LINES - 1));
    if start == 0 || end < start || end - start >= MAX_KNOWLEDGE_READ_LINES {
        return Err("ERR_KNOWLEDGE_READ_RANGE".into());
    }
    Ok((start, end))
}

/// Projects `document` and its exact `text` to inclusive `start`/`end`; returns metadata and continuation.
/// 将 `document` 及其精确 `text` 投影至包含边界的 `start`、`end`；返回元数据及续读标识。
pub(super) fn document_window(
    document: KnowledgeDocumentSummary,
    text: &str,
    start: usize,
    end: usize,
) -> Result<KnowledgeDocumentContent, String> {
    // Historical and current reads share one newline and empty-body authority.
    // 历史及当前读取共享同一换行及空正文权威。
    let total = document.line_count;
    if start > total.max(1) {
        return Err("ERR_KNOWLEDGE_READ_RANGE".into());
    }
    // The window ends at the actual stored body rather than inventing empty trailing lines.
    // 窗口止于实际存储正文，绝不虚构尾部空行。
    let end = end.min(total);
    Ok(KnowledgeDocumentContent {
        document,
        content: crate::knowledge_base::knowledge_body_lines(text)
            .skip(start - 1)
            .take(end.saturating_sub(start - 1))
            .collect(),
        start_line: start,
        end_line: end,
        next_line: (end < total).then_some(end + 1),
    })
}

impl KnowledgeBaseRuntime {
    /// Reads `seq` items for `library_id` at immutable `offset` with bounded `limit`.
    /// 以不可变 `offset` 及有界 `limit` 读取 `library_id` 中 `seq` 的条目。
    /// Returns one consistent summary/image page; no current entity is substituted for a historical image.
    /// 返回一致摘要及镜像页；绝不使用当前实体替代历史镜像。
    pub async fn get_change_page(
        &self,
        library_id: &str,
        seq: u64,
        offset: u64,
        limit: u32,
    ) -> Result<KnowledgeChangeDetail, String> {
        self.require_active_library(library_id).await?;
        if limit == 0 || limit > MAX_KNOWLEDGE_PAGE_SIZE {
            return Err("ERR_KNOWLEDGE_HISTORY_RANGE".into());
        }
        // Summary and image iteration share the same immutable change snapshot.
        // 摘要及镜像迭代共享同一不可变变更快照。
        let (_, binding) = self.ensure_library_binding(library_id).await?;
        local_sql::with_read_snapshot_async(&self.host_runtime.local, self.resolve_library_binding(binding)?, move |connection| {
            // One immutable summary anchors every returned image.
            // 一个不可变摘要锚定每个返回镜像。
            let change = connection.query_row(&format!("SELECT {CHANGE_COLUMNS} FROM kb_changes c WHERE c.seq=?;"), [sql_integer(seq)?], change_row).optional().map_err(|error|error.to_string())?.ok_or("ERR_KNOWLEDGE_CHANGE_NOT_FOUND")?;
            // The indexed query fetches one extra row only to detect continuation.
            // 有索引查询仅多取一行以检测续读。
            let mut statement = connection.prepare("SELECT entity,entity_key,before_json,after_json FROM kb_change_items WHERE change_seq=? ORDER BY ordinal LIMIT ? OFFSET ?;").map_err(|error|error.to_string())?;
            // Rows remain lazy so the byte cap can stop before materializing a full large page.
            // 行保持惰性，使字节上限可在物化完整大页之前停止。
            let rows = statement.query_map(params![sql_integer(seq)?,i64::from(limit)+1,sql_integer(offset)?], |row| {
                // Absence is a declared create/delete image side, never a current-row fallback.
                // 缺失为声明的创建或删除镜像侧别，绝非当前行兜底。
                let before = row.get::<_,Option<String>>(2)?.map(|text|serde_json::from_str(&text)).transpose().map_err(|error|rusqlite::Error::FromSqlConversionFailure(2,rusqlite::types::Type::Text,Box::new(error)))?;
                // Decode the exact stored post-image independently of the pre-image.
                // 独立于前镜像解码精确存储后镜像。
                let after = row.get::<_,Option<String>>(3)?.map(|text|serde_json::from_str(&text)).transpose().map_err(|error|rusqlite::Error::FromSqlConversionFailure(3,rusqlite::types::Type::Text,Box::new(error)))?;
                Ok(KnowledgeChangeItem { entity:row.get(0)?, entity_key:row.get(1)?, before, after })
            }).map_err(|error|error.to_string())?;
            // Accumulate only the bounded response items.
            // 仅累计有界响应条目。
            let mut items = Vec::new();
            // Serialized item bytes share one aggregate response budget.
            // 序列化条目字节共享一个累计响应预算。
            let mut bytes = 0usize;
            // Continuation counts immutable item ordinals, independent of later writes.
            // 续读计算不可变条目序号，独立于后续写入。
            let mut next_offset = None;
            for row in rows {
                // Surface corrupt image rows instead of skipping history.
                // 暴露损坏镜像行，不跳过历史。
                let item = row.map_err(|error|error.to_string())?;
                // Measure the actual serialized payload, including escaped metadata.
                // 测量实际序列化载荷，包含转义元数据。
                let size = serde_json::to_vec(&item).map_err(|error|error.to_string())?.len();
                if size > MAX_KNOWLEDGE_HISTORY_BYTES { return Err("ERR_KNOWLEDGE_HISTORY_ITEM_LIMIT".into()); }
                if items.len() >= limit as usize || size > MAX_KNOWLEDGE_HISTORY_BYTES.saturating_sub(bytes) {
                    next_offset = Some(offset.checked_add(items.len() as u64).ok_or("ERR_KNOWLEDGE_INTEGER_RANGE")?);
                    break;
                }
                bytes += size;
                items.push(item);
            }
            Ok(KnowledgeChangeDetail { change, items, next_offset })
        }).await
    }

    /// Reads the exact `side` of `document_id` in `seq`, with a bounded inclusive line window.
    /// 读取 `seq` 中 `document_id` 的精确 `side`，并使用有界包含边界行窗口。
    /// `library_id` owns the history; missing images fail explicitly instead of reading current content.
    /// `library_id` 拥有历史；镜像缺失时明确失败，不读取当前内容。
    pub async fn read_change_document(
        &self,
        library_id: &str,
        seq: u64,
        document_id: &str,
        side: KnowledgeChangeSide,
        start_line: Option<usize>,
        end_line: Option<usize>,
    ) -> Result<KnowledgeDocumentContent, String> {
        self.require_active_library(library_id).await?;
        // Range admission precedes body loading for both current and historical reads.
        // 当前与历史读取均在加载正文前执行范围准入。
        let (start, end) = read_range(start_line, end_line)?;
        // Only the typed side enum can choose a column; no caller text reaches SQL syntax.
        // 仅强类型侧别枚举可选择列；调用方文本不会进入 SQL 语法。
        let column = match side {
            KnowledgeChangeSide::Before => "before_json",
            KnowledgeChangeSide::After => "after_json",
        };
        // The immutable entity identity survives moves and current deletion.
        // 不可变实体身份在移动及当前删除后仍保持。
        let id = document_id.to_string();
        // A single snapshot keeps row image and referenced blob consistent.
        // 单个快照使行镜像及引用正文块保持一致。
        let (_, binding) = self.ensure_library_binding(library_id).await?;
        local_sql::with_read_snapshot_async(&self.host_runtime.local, self.resolve_library_binding(binding)?, move |connection| {
            // The exact change/entity/side is the sole source of this historical document.
            // 精确变更、实体及侧别为该历史文档唯一来源。
            let image = connection.query_row(&format!("SELECT {column} FROM kb_change_items WHERE change_seq=? AND entity='document' AND entity_key=?;"), params![sql_integer(seq)?,id], |row|row.get::<_,Option<String>>(0)).optional().map_err(|error|error.to_string())?.ok_or("ERR_KNOWLEDGE_CHANGE_ITEM_NOT_FOUND")?.ok_or("ERR_KNOWLEDGE_CHANGE_SIDE_EMPTY")?;
            // Decode the complete immutable row image without consulting current metadata.
            // 解码完整不可变行镜像，不查询当前元数据。
            let document: KnowledgeDocumentSummary = serde_json::from_str(&image).map_err(|error|error.to_string())?;
            // Content-addressed body lookup returns the bytes referenced by that image.
            // 内容寻址正文查找返回该镜像引用的字节。
            let text = body(connection, &document.body_sha256)?;
            document_window(document, &text, start, end)
        }).await
    }
}
