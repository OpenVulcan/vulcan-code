//! Behavioral checks for source ownership, fence parsing and execution budgets.
//! 源码归属、围栏解析和执行预算的行为校验。

use super::*;
use crate::v1::{ApiCallId, AssistantTextEvent, Seq, UserInputEvent, UserInputSource};

/// Build a completed synthetic assistant event from `text`; no live provider or storage is used.
/// 用 `text` 构造已完成的合成助手事件；不使用真实供应商或存储。
fn assistant_event(text: impl Into<String>) -> ConversationEvent {
    ConversationEvent {
        seq: Seq::try_new(3).unwrap(),
        turn_id: None,
        created_at_ms: 1,
        updated_at_ms: 2,
        event_revision: 1,
        visibility: EventVisibility::Provider,
        event: EventBody::AssistantText(AssistantTextEvent {
            api_call_id: ApiCallId::try_new(2).unwrap(),
            text: text.into(),
            status: AssistantSemanticStatus::Completed,
            provider_source: None,
        }),
    }
}

/// Wrap `source` in one valid dedicated fence, returning its exact Markdown text.
/// 将 `source` 包装进一个合法专用围栏，返回精确 Markdown 正文。
fn page(source: &str) -> String {
    format!("```vulcan-html {{\"version\":1,\"title\":\"页面\"}}\n{source}\n```\n")
}

/// Annotation publication requires physical evidence; event revisions are not document frontiers.
/// 标注发布要求物理证据；事件修订不是文档前沿。
#[test]
fn html_answers_event_projection_requires_the_exact_durable_source() {
    let mut event = assistant_event(page("<p>ready</p>"));
    event.event_revision = 5;
    assert!(HtmlAnswerEventProjection::from_event("s", &event, false).is_none());
    let projection = HtmlAnswerEventProjection::from_event("s", &event, true).unwrap();
    assert_eq!(projection.seq, event.seq);
    assert_eq!(projection.event_revision, event.event_revision);
    assert!(matches!(
        projection.answers[0].availability,
        HtmlAnswerAvailability::Ready { .. }
    ));
    let EventBody::AssistantText(assistant) = &mut event.event else {
        unreachable!()
    };
    assistant.status = AssistantSemanticStatus::Streaming;
    assert!(HtmlAnswerEventProjection::from_event("s", &event, true).is_none());
    let example = assistant_event("Use a vulcan-html fence when needed.");
    assert!(
        HtmlAnswerEventProjection::from_event("s", &example, true)
            .unwrap()
            .answers
            .is_empty()
    );
    assert!(
        HtmlAnswerEventProjection::from_event("s", &assistant_event("ordinary answer"), true)
            .is_none()
    );
}

/// Borrow the identity of a ready descriptor; fail with the actual unexpected state.
/// 借用可打开描述的身份；状态异常时用实际状态报告失败。
fn identity(descriptor: &HtmlAnswerDescriptor) -> &str {
    let HtmlAnswerAvailability::Ready { artifact_id, .. } = &descriptor.availability else {
        panic!("Expected an executable HTML answer, got {descriptor:?}");
    };
    artifact_id
}

/// Assert one descriptor's stable rejection category without matching localized wording.
/// 断言一个描述的稳定拒绝分类，不匹配本地化文案。
fn assert_rejected(descriptor: &HtmlAnswerDescriptor, reason: HtmlAnswerUnavailableReason) {
    assert_eq!(
        descriptor.availability,
        HtmlAnswerAvailability::Unavailable { reason }
    );
}

/// Original UTF-8/UTF-16 intervals and HTML bytes agree after non-BMP text and CRLF lines.
/// 非 BMP 文字和 CRLF 行之后，原始 UTF-8、UTF-16 区间及 HTML 字节一致。
#[test]
fn html_answers_source_ranges_preserve_unicode_and_crlf() {
    let source = "<!doctype html>\r\n<p>🐉 中文</p>\r\n";
    let text = format!(
        "前言😀\r\n\r\n````vulcan-html {{\"version\":1,\"title\":\"图😀\"}}\r\n{source}`````\r\n\r\n尾声"
    );
    let event = assistant_event(&text);
    let descriptors = project_html_answers("session-a", &event, true);
    assert_eq!(descriptors.len(), 1);
    let descriptor = &descriptors[0];
    let HtmlAnswerAvailability::Ready { source: range, .. } = &descriptor.availability else {
        panic!("Expected a completed source range: {descriptor:?}");
    };
    assert_eq!(&text[range.byte_start..range.byte_end], source);
    assert_eq!(
        range.utf16_start,
        text[..range.byte_start].encode_utf16().count()
    );
    assert_eq!(
        range.utf16_end,
        text[..range.byte_end].encode_utf16().count()
    );
    assert_eq!(
        descriptor.fence.utf16_start,
        text[..descriptor.fence.byte_start].encode_utf16().count()
    );
    assert_eq!(
        descriptor.fence.utf16_end,
        text[..descriptor.fence.byte_end].encode_utf16().count()
    );
    assert_eq!(identity(descriptor).len(), Sha256::output_size() * 2);
}

/// CommonMark nesting, ordinary languages and tilde fences never acquire execution authority.
/// CommonMark 嵌套、普通语言和波浪号围栏绝不获得执行权威。
#[test]
fn html_answers_only_top_level_backtick_protocol_is_recognized() {
    let nested = page("<p>example</p>");
    let quoted = nested
        .lines()
        .map(|line| format!("> {line}\n"))
        .collect::<String>();
    let listed = nested
        .lines()
        .map(|line| format!("  {line}\n"))
        .collect::<String>();
    let footnote = nested
        .lines()
        .map(|line| format!("    {line}\n"))
        .collect::<String>();
    let text = format!(
        "{quoted}\n- item\n\n{listed}\nreference[^a]\n\n[^a]: footnote\n\n{footnote}\n````text\n{nested}````\n\n```html\n<p>ordinary</p>\n```\n\n~~~vulcan-html {{\"version\":1,\"title\":\"tilde\"}}\n<p>tilde</p>\n~~~\n\n```vulcan-html-extra {{}}\nignored\n```\n"
    );
    assert!(project_html_answers("s", &assistant_event(text), true).is_empty());
    assert_eq!(
        project_html_answers("s", &assistant_event(format!("{nested}\n{nested}")), true).len(),
        2
    );
}

/// EOF recovery, undersized closers and indented closing text cannot produce ready pages.
/// 文件结尾恢复、过短闭合标记和缩进闭合文本不能产生可打开页面。
#[test]
fn html_answers_require_an_explicit_compatible_closer() {
    for tail in ["", "```", "```` trailing", "    ````"] {
        let text = format!("````vulcan-html {{\"version\":1,\"title\":\"x\"}}\n<p>x</p>\n{tail}");
        let descriptors = project_html_answers("s", &assistant_event(text), true);
        assert_eq!(descriptors.len(), 1);
        assert_rejected(&descriptors[0], HtmlAnswerUnavailableReason::UnclosedFence);
    }
    for closer in ["````", "`````   ", "   ````\t", "````\r\n"] {
        let text = format!("````vulcan-html {{\"version\":1,\"title\":\"x\"}}\n<p>x</p>\n{closer}");
        let descriptors = project_html_answers("s", &assistant_event(text), true);
        identity(&descriptors[0]);
    }
    let indented = "  ```vulcan-html {\"version\":1,\"title\":\"x\"}\n  <p>x</p>\n  ```\n";
    let descriptors = project_html_answers("s", &assistant_event(indented), true);
    assert_eq!(descriptors.len(), 1);
    identity(&descriptors[0]);
}

/// Strict metadata rejects unsupported, missing, duplicate, foreign and wrongly typed fields.
/// 严格元信息拒绝不支持、缺失、重复、额外和类型错误的字段。
#[test]
fn html_answers_metadata_is_strict_and_title_is_bounded() {
    for metadata in [
        "",
        "[]",
        "{}",
        "null",
        "{\"version\":1}",
        "{\"version\":1,\"title\":\"x\",\"id\":\"forged\"}",
        "{\"version\":1,\"version\":1,\"title\":\"x\"}",
        "{\"version\":1,\"title\":\"x\",\"title\":\"y\"}",
        "{\"version\":\"1\",\"title\":\"x\"}",
        "{\"version\":1,\"title\":123}",
    ] {
        let descriptors = project_html_answers(
            "s",
            &assistant_event(format!("```vulcan-html {metadata}\n<p>x</p>\n```")),
            true,
        );
        assert_rejected(
            &descriptors[0],
            HtmlAnswerUnavailableReason::InvalidMetadata,
        );
        assert_eq!(descriptors[0].title, None);
    }
    assert_eq!(
        validate_metadata("{\"version\":2,\"title\":\"x\"}").err(),
        Some(HtmlAnswerUnavailableReason::UnsupportedVersion)
    );
    for title in [
        String::new(),
        "  ".into(),
        "line\nbreak".into(),
        "x".repeat(HTML_ANSWER_MAX_TITLE_CHARS + 1),
    ] {
        let metadata = serde_json::json!({"version": HTML_ANSWER_VERSION, "title": title});
        assert_eq!(
            validate_metadata(&metadata.to_string()).err(),
            Some(HtmlAnswerUnavailableReason::InvalidTitle)
        );
    }
    let metadata = serde_json::json!({"version": HTML_ANSWER_VERSION, "title": "🐉".repeat(HTML_ANSWER_MAX_TITLE_CHARS)});
    assert!(validate_metadata(&metadata.to_string()).is_ok());
}

/// Only successfully persisted terminal assistant content can own an executable result.
/// 只有成功持久化的终态助手正文才能拥有可执行结果。
#[test]
fn html_answers_reject_uncommitted_interrupted_and_non_assistant_sources() {
    let mut event = assistant_event(page("<p>x</p>"));
    let descriptors = project_html_answers("s", &event, false);
    assert_rejected(&descriptors[0], HtmlAnswerUnavailableReason::NotPersisted);
    let EventBody::AssistantText(assistant) = &mut event.event else {
        unreachable!()
    };
    assistant.status = AssistantSemanticStatus::Streaming;
    assert!(project_html_answers("s", &event, true).is_empty());
    let EventBody::AssistantText(assistant) = &mut event.event else {
        unreachable!()
    };
    assistant.status = AssistantSemanticStatus::Interrupted;
    assert_rejected(
        &project_html_answers("s", &event, true)[0],
        HtmlAnswerUnavailableReason::Interrupted,
    );
    event.event = EventBody::UserInput(UserInputEvent {
        client_action_id: crate::v1::ClientActionId::try_new("html-answer-source-test").unwrap(),
        input_id: crate::v1::PendingInputId::try_new(1).unwrap(),
        input_source: UserInputSource::Root,
        text: page("<p>user example</p>"),
        resource_ids: Vec::new(),
        parameters: None,
    });
    assert!(project_html_answers("s", &event, true).is_empty());
}

/// The same content is stable on replay but differs across ownership, ordinal and revisions.
/// 相同内容重放时身份稳定，但归属、序号和内容修订不同会改变身份。
#[test]
fn html_answers_identity_binds_session_event_api_ordinal_and_content() {
    let mut event = assistant_event(format!("{}\n{}", page("<p>x</p>"), page("<p>x</p>")));
    let original = project_html_answers("session-a", &event, true);
    assert_ne!(identity(&original[0]), identity(&original[1]));
    assert_eq!(original, project_html_answers("session-a", &event, true));
    assert_ne!(
        identity(&original[0]),
        identity(&project_html_answers("session-b", &event, true)[0])
    );
    event.seq = Seq::try_new(event.seq.value() + 1).unwrap();
    assert_ne!(
        identity(&original[0]),
        identity(&project_html_answers("session-a", &event, true)[0])
    );
    event.seq = Seq::try_new(3).unwrap();
    let EventBody::AssistantText(assistant) = &mut event.event else {
        unreachable!()
    };
    assistant.api_call_id = ApiCallId::try_new(20).unwrap();
    assert_ne!(
        identity(&original[0]),
        identity(&project_html_answers("session-a", &event, true)[0])
    );
    let EventBody::AssistantText(assistant) = &mut event.event else {
        unreachable!()
    };
    assistant.api_call_id = ApiCallId::try_new(2).unwrap();
    assistant.text = page("<p>replacement</p>");
    assert_ne!(
        identity(&original[0]),
        identity(&project_html_answers("session-a", &event, true)[0])
    );
}

/// Page count and byte budgets reject only execution, while preserving all source descriptors.
/// 页面数量和字节预算仅拒绝执行，仍保留全部源码描述。
#[test]
fn html_answers_execution_budgets_have_exact_inclusive_boundaries() {
    let max_source = "x".repeat(HTML_ANSWER_MAX_PAGE_BYTES - 1);
    let max_page = page(&max_source);
    identity(&project_html_answers("s", &assistant_event(&max_page), true)[0]);
    let too_large = project_html_answers(
        "s",
        &assistant_event(page(&"x".repeat(HTML_ANSWER_MAX_PAGE_BYTES))),
        true,
    );
    assert_rejected(&too_large[0], HtmlAnswerUnavailableReason::PageTooLarge);
    let full_count = page("<p>small</p>").repeat(HTML_ANSWER_MAX_PAGES + 1);
    let descriptors = project_html_answers("s", &assistant_event(full_count), true);
    for descriptor in descriptors.iter().take(HTML_ANSWER_MAX_PAGES) {
        identity(descriptor);
    }
    assert_rejected(
        descriptors.last().unwrap(),
        HtmlAnswerUnavailableReason::TooManyPages,
    );
    let total = format!("{}{}{}", max_page, max_page, page("x"));
    let descriptors = project_html_answers("s", &assistant_event(total), true);
    identity(&descriptors[0]);
    identity(&descriptors[1]);
    assert_rejected(&descriptors[2], HtmlAnswerUnavailableReason::TotalTooLarge);
    assert_rejected(
        &project_html_answers("s", &assistant_event(page(" \t")), true)[0],
        HtmlAnswerUnavailableReason::EmptySource,
    );
}
