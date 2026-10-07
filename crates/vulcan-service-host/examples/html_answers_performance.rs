//! Reproducible local parser measurements using the production Core projector and frozen budgets.
//! 使用正式 Core 投影器及冻结预算的可复现本地解析测量。
use std::{hint::black_box, time::Instant};
use vulcan_session_core::v1::{
    ApiCallId, AssistantSemanticStatus, AssistantTextEvent, ConversationEvent, EventBody,
    EventVisibility, Seq,
    html_answers::{
        HTML_ANSWER_MAX_PAGE_BYTES, HTML_ANSWER_MAX_PAGES, HTML_ANSWER_MAX_TOTAL_BYTES,
        project_html_answers,
    },
};

/// Build one complete provider event for `sizes`; returns canonical source without executing it.
/// 为 `sizes` 构建一个完整 Provider 事件；返回权威源码，不执行页面。
fn event(sizes: &[usize]) -> ConversationEvent {
    let text = sizes
        .iter()
        .map(|size| {
            format!(
                "```vulcan-html {{\"version\":1,\"title\":\"Performance\"}}\n{}\n```\n",
                "x".repeat(size - 1)
            )
        })
        .collect();
    ConversationEvent {
        seq: Seq::try_new(1).expect("positive fixture event sequence"),
        turn_id: None,
        created_at_ms: 1,
        updated_at_ms: 2,
        event_revision: 1,
        visibility: EventVisibility::Provider,
        event: EventBody::AssistantText(AssistantTextEvent {
            api_call_id: ApiCallId::try_new(1).expect("positive fixture API identity"),
            status: AssistantSemanticStatus::Completed,
            provider_source: None,
            text,
        }),
    }
}

/// Warm three times, measure twenty times and return median/P95 in milliseconds.
/// 预热三次、测量二十次，返回毫秒单位的中位数及 P95。
fn measure(event: &ConversationEvent) -> (f64, f64) {
    for _ in 0..3 {
        black_box(project_html_answers("synthetic", black_box(event), true));
    }
    let mut samples: Vec<f64> = (0..20)
        .map(|_| {
            let start = Instant::now();
            black_box(project_html_answers("synthetic", black_box(event), true));
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    ((samples[9] + samples[10]) / 2.0, samples[18])
}

/// Report all fixed fixtures and fail when the predeclared linear-growth budget is exceeded.
/// 报告全部固定夹具，超过预先声明的线性增长预算时失败。
fn main() {
    let fixtures = [
        vec![1024],
        vec![64 * 1024],
        vec![HTML_ANSWER_MAX_PAGE_BYTES],
        vec![HTML_ANSWER_MAX_TOTAL_BYTES / HTML_ANSWER_MAX_PAGES; HTML_ANSWER_MAX_PAGES],
    ];
    let results: Vec<_> = fixtures.iter().map(|sizes| {
        let source = event(sizes);
        assert_eq!(project_html_answers("synthetic", &source, true).len(), sizes.len());
        let (median, p95) = measure(&source);
        serde_json::json!({"bytes": sizes.iter().sum::<usize>(), "pages": sizes.len(), "medianMs": median, "p95Ms": p95})
    }).collect();
    let small = results
        .iter()
        .find(|result| result["bytes"] == 64 * 1024 && result["pages"] == 1)
        .expect("64 KiB fixture")["p95Ms"]
        .as_f64()
        .expect("numeric timing");
    let large = results
        .iter()
        .find(|result| result["bytes"] == HTML_ANSWER_MAX_PAGE_BYTES && result["pages"] == 1)
        .expect("maximum page fixture")["p95Ms"]
        .as_f64()
        .expect("numeric timing");
    let budget = small * (HTML_ANSWER_MAX_PAGE_BYTES as f64 / (64.0 * 1024.0)) * 1.5 + 1.0;
    println!(
        "{}",
        serde_json::json!({"warmup":3,"samples":20,"fixtures":results,"growthBudgetMs":budget,"passed":large <= budget})
    );
    assert!(large <= budget, "frozen parser growth budget exceeded");
}
