//! Lossless source partitioning against the host's serialized request budget.
//! 按宿主序列化请求预算对来源进行无损分段。

use super::*;

/// Checks both initial and repair request sizes; returns false when either exceeds the caller budget.
/// 检查初始请求与修复请求大小；任一超过调用方预算时返回 false。
pub(super) fn knowledge_display_request_fits(
    request: &ResponsesExecutionRequest,
) -> Result<bool, String> {
    // Measure the same full JSON representation as configured inference, including escaped content.
    // 与配置推理入口测量同一份完整 JSON，包含转义后的内容。
    let repair = knowledge_display_schema_repair_request(request.clone())?;
    for candidate in [request, &repair] {
        // encoded includes metadata, schema, messages and the repair suffix.
        // encoded 包含元数据、Schema、消息及修复后缀。
        let encoded = serde_json::to_vec(candidate).map_err(|error| error.to_string())?;
        if encoded.len() > KNOWLEDGE_DISPLAY_INPUT_BYTE_LIMIT {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Builds ordered requests for one source without dropping any UTF-8 bytes; errors if metadata alone cannot fit.
/// 为单份来源构建有序请求，不丢弃任何 UTF-8 字节；仅元数据就超限时返回错误。
fn knowledge_display_source_requests(
    settings: &KnowledgeLibrarySettings,
    source: &KnowledgeDocumentContent,
) -> Result<Vec<ResponsesExecutionRequest>, String> {
    // complete preserves the existing single-request identity when the entire document fits.
    // complete 在整篇文档可容纳时保留原有单次请求身份。
    let complete = knowledge_display_request(
        settings,
        &source.document.id,
        &source.document.relative_path,
        &source.document.title,
        &source.content,
    )?;
    if knowledge_display_request_fits(&complete)? {
        return Ok(vec![complete]);
    }
    // requests preserves source order, while start is the first unconsumed byte.
    // requests 保留来源顺序，start 指向首个尚未消费的字节。
    let mut requests = Vec::new();
    let mut start = 0;
    while start < source.content.len() {
        // part_id gives each segment an independent execution and schema-repair identity.
        // part_id 为每段提供独立的执行与 Schema 修复身份。
        let part_id = format!("{}-part-{start}", source.document.id);
        // boundaries bounds the search to at most one request's raw bytes and never splits a code point.
        // boundaries 将搜索限制为最多一次请求的原文字节，且绝不切断字符。
        let boundaries = source.content[start..]
            .char_indices()
            .map(|(offset, _)| start + offset)
            .take_while(|end| *end - start <= KNOWLEDGE_DISPLAY_INPUT_BYTE_LIMIT)
            .chain(
                (source.content.len() - start <= KNOWLEDGE_DISPLAY_INPUT_BYTE_LIMIT)
                    .then_some(source.content.len()),
            )
            .collect::<Vec<_>>();
        // low/high perform an exact serialized-size binary search over valid UTF-8 boundaries.
        // low/high 在合法 UTF-8 边界上按精确序列化大小执行二分搜索。
        let mut low = 0;
        let mut high = boundaries.len();
        while low + 1 < high {
            // middle selects the next candidate prefix, including all host overhead.
            // middle 选择下一候选前缀，测量包含全部宿主开销。
            let middle = low + (high - low) / 2;
            // candidate contains the complete contract for this prefix.
            // candidate 包含该前缀的完整请求契约。
            let candidate = knowledge_display_request(
                settings,
                &part_id,
                &source.document.relative_path,
                &source.document.title,
                &source.content[start..boundaries[middle]],
            )?;
            if knowledge_display_request_fits(&candidate)? {
                low = middle;
            } else {
                high = middle;
            }
        }
        // end prefers a nearby complete line while retaining every byte for the next request.
        // end 优先选择附近完整行，并将剩余每个字节留给下一请求。
        let mut end = boundaries[low];
        if end == start {
            return Err(format!(
                "整理 {} 的请求元数据过大，无法在 {} 字节限额内容纳正文",
                source.document.relative_path, KNOWLEDGE_DISPLAY_INPUT_BYTE_LIMIT
            ));
        }
        if end < source.content.len()
            && let Some(newline) = source.content[start..end].rfind('\n')
            && newline >= (end - start) / 2
        {
            end = start + newline + 1;
        }
        requests.push(knowledge_display_request(
            settings,
            &part_id,
            &source.document.relative_path,
            &source.document.title,
            &source.content[start..end],
        )?);
        start = end;
    }
    if requests.is_empty() {
        return Err(String::from("整理请求元数据超过调用方字节限额"));
    }
    Ok(requests)
}

/// Curates all bounded segments for source under job_id and returns one draft only after every segment succeeds.
/// 在 job_id 下整理 source 的全部有界分段，仅在每段成功后返回一份草稿。
/// port executes under capability; settings supplies the route; reporter records each part's progress and cancellation.
/// port 在 capability 授权下执行；settings 提供路由；reporter 记录各分段进度及取消状态。
pub(super) async fn execute_knowledge_display_source(
    port: &dyn HostAiPort,
    capability: &HostAiCapabilityHandle,
    job_id: &str,
    settings: &KnowledgeLibrarySettings,
    source: &KnowledgeDocumentContent,
    reporter: &KnowledgeOperationReporter,
) -> Result<KnowledgeDisplayDraft, String> {
    // requests is budgeted before any provider work begins.
    // requests 在任何供应商工作开始前完成预算检查。
    let requests = knowledge_display_source_requests(settings, source)?;
    // total allows progress to identify the current part of a large document.
    // total 让进度能够指出长文档的当前分段。
    let total = requests.len();
    // drafts retains source order and is never published partially.
    // drafts 保留来源顺序，绝不发布未完成的文档。
    let mut drafts = Vec::with_capacity(total);
    for (index, request) in requests.into_iter().enumerate() {
        if reporter.runtime.operation_is_cancelled(job_id).await? {
            return Err(String::from("资料库显示库整理操作已取消"));
        }
        // label makes segmented work identifiable without exposing host IDs to the model.
        // label 使分段工作可识别，同时不向模型暴露宿主标识。
        let label = if total == 1 {
            source.document.relative_path.clone()
        } else {
            format!("{}（{}/{total}）", source.document.relative_path, index + 1)
        };
        drafts.push(
            execute_knowledge_display_draft(
                port,
                capability,
                knowledge_ai_call_context(job_id, &request.request_id),
                request,
                &label,
                reporter,
            )
            .await?,
        );
    }
    merge_knowledge_display_drafts(drafts)
}

/// Joins ordered segment drafts, retaining all Markdown and symbols; returns an error for no segments.
/// 合并有序分段草稿，保留全部 Markdown 与符号；没有分段时返回错误。
fn merge_knowledge_display_drafts(
    drafts: Vec<KnowledgeDisplayDraft>,
) -> Result<KnowledgeDisplayDraft, String> {
    // parts consumes drafts in original source order.
    // parts 按原始来源顺序消费草稿。
    let mut parts = drafts.into_iter();
    // merged uses the first segment's title and accumulates subsequent factual content.
    // merged 使用第一段标题，并累积后续事实内容。
    let mut merged = parts
        .next()
        .ok_or_else(|| String::from("资料库整理分段结果为空"))?;
    for draft in parts {
        merged.summary.push_str("\n\n");
        merged.summary.push_str(&draft.summary);
        merged.markdown.push_str("\n\n");
        merged.markdown.push_str(&draft.markdown);
        merged.embedding_text.push_str("\n\n");
        merged.embedding_text.push_str(&draft.embedding_text);
        merged.keywords.extend(draft.keywords);
        merged.symbols.extend(draft.symbols);
    }
    // seen removes duplicate metadata while keeping first-occurrence order.
    // seen 去除重复元数据，同时保持首次出现顺序。
    let mut seen = BTreeSet::new();
    merged.keywords.retain(|word| seen.insert(word.clone()));
    seen.clear();
    merged.symbols.retain(|symbol| seen.insert(symbol.clone()));
    normalize_knowledge_display_draft(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises Chinese, JSON escapes and a source larger than the retired character cap without loss.
    /// 使用中文、JSON 转义及超过旧字符上限的来源，验证分段无损。
    #[test]
    fn source_partition_preserves_all_content_and_reserves_repair_budget() {
        // content deliberately exceeds one request with multibyte and escaped characters.
        // content 使用多字节与转义字符，刻意超过单次请求限额。
        let content = "中文😀\"\\\t\u{0001}\n".repeat(35_000);
        // source supplies real request metadata and the complete original body.
        // source 提供真实请求元数据和完整原始正文。
        let mut source = super::super::tests::test_knowledge_display_source("large", content.len());
        source.content = content.clone();
        // requests must all pass the same JSON-byte oracle as the inference entrypoint.
        // requests 必须全部通过与推理入口相同的 JSON 字节判定。
        let requests =
            knowledge_display_source_requests(&KnowledgeLibrarySettings::default(), &source)
                .unwrap();
        assert!(requests.len() > 1);
        // recovered reconstructs exactly what was sent, proving no silent truncation or overlap.
        // recovered 重建实际发送内容，证明没有静默截断或重叠。
        let mut recovered = String::new();
        // identities ensures segments cannot collide in the execution registry.
        // identities 确保分段在执行注册表中不会冲突。
        let mut identities = BTreeSet::new();
        for request in requests {
            assert!(identities.insert(request.request_id.clone()));
            assert!(
                serde_json::to_vec(&request).unwrap().len() <= KNOWLEDGE_DISPLAY_INPUT_BYTE_LIMIT
            );
            assert!(
                serde_json::to_vec(
                    &knowledge_display_schema_repair_request(request.clone()).unwrap()
                )
                .unwrap()
                .len()
                    <= KNOWLEDGE_DISPLAY_INPUT_BYTE_LIMIT
            );
            let ContentItem::Text { text } = &request.messages[1].content[0] else {
                panic!("expected text")
            };
            recovered.push_str(
                text.split_once("<untrusted_markdown>\n")
                    .unwrap()
                    .1
                    .strip_suffix("\n</untrusted_markdown>")
                    .unwrap(),
            );
        }
        assert_eq!(recovered, content);
    }

    /// Validates realistic enum catalogs in both schemas and confirms merge retains tail symbols.
    /// 验证两种 Schema 接受真实枚举目录，并确认合并保留尾部符号。
    #[test]
    fn schema_and_merge_preserve_large_symbol_catalogs() {
        // value models a keyboard API with more than 64 constants.
        // value 模拟包含超过 64 个常量的键盘 API。
        let value = json!({"title":"keyboard", "summary":"keys", "keywords":[], "symbols":(0..150).map(|index| format!("KEY_{index}")).collect::<Vec<_>>(), "markdown":"first", "embedding_text":"keyboard keys"});
        assert!(jsonschema::is_valid(
            &knowledge_display_output_schema(),
            &value
        ));
        // item adds the batch marker without changing document fields.
        // item 添加批次标记且不改变文档字段。
        let mut item = value.clone();
        item["source_marker"] = json!("source-1");
        assert!(jsonschema::is_valid(
            &knowledge_display_batch_output_schema(),
            &json!({"documents":[item]})
        ));
        // second supplies another segment to exercise ordered merging and deduplication.
        // second 提供另一段，用于验证顺序合并与去重。
        let mut second = value.clone();
        second["markdown"] = json!("last");
        let merged = merge_knowledge_display_drafts(vec![
            parse_knowledge_display_draft(value).unwrap(),
            parse_knowledge_display_draft(second).unwrap(),
        ])
        .unwrap();
        assert_eq!(merged.symbols.len(), 150);
        assert_eq!(merged.symbols.last().unwrap(), "KEY_149");
        assert_eq!(merged.markdown, "first\n\nlast");
    }
}
