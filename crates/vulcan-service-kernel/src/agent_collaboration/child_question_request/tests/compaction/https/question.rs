//! Chat fixture responding to the actual tagged question through the advertised reply tool.
//! 通过已暴露回复工具回答实际带标签问题的 Chat 夹具。

use super::*;

/// Reads the latest captured request and emits its tool reply or final SSE response on `stream`.
/// 读取最新捕获请求，在 `stream` 输出其工具回复或最终 SSE 响应。
/// Requires two ordered requests in `requests`; no question identity is injected by the test caller.
/// 要求 `requests` 中为两个有序请求；测试调用方不注入问题身份。
pub(super) async fn respond(
    stream: &mut tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
    requests: &Mutex<Vec<serde_json::Value>>,
) {
    // Own the request clone before awaiting any socket write.
    // 在等待套接字写入前持有请求副本。
    let (count, request) = {
        let captured = requests.lock().unwrap();
        (captured.len(), captured.last().unwrap().clone())
    };
    assert!(
        count <= 2,
        "auxiliary reply must not restart the business loop"
    );
    let messages = request["messages"].as_array().unwrap();
    let (delta, reason) = if count == 1 {
        // Chat permits strings and native content arrays; inspect both declared text representations.
        // Chat 允许字符串及原生内容数组；检查两种声明的文本表示。
        let question = messages
            .iter()
            .filter(|message| message["role"] == "user")
            .map(chat_text)
            .find(|body| body.contains("kind=\"question\""))
            .expect("the actual question must reach the wire as tagged User content");
        let question_id = question
            .split_once("question_id=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        assert!(!question_id.is_empty() && !question_id.contains('&'));
        assert!(
            request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| { tool["function"]["name"] == "reply_question" }),
            "the reply tool must be advertised to the actual model"
        );
        let arguments = serde_json::json!({
            "question_id": question_id,
            "answer": "Continue the owned child work; the parent goal remains paused.",
            "unable_to_answer": false,
        })
        .to_string();
        (
            serde_json::json!({"role":"assistant","tool_calls":[{
                "index":0,"id":"qa-reply-call","type":"function",
                "function":{"name":"reply_question","arguments":arguments}
            }]}),
            "tool_calls",
        )
    } else {
        // The model's next request must contain the actual successful reply result.
        // 模型下一请求必须包含实际成功回复结果。
        let result = messages
            .iter()
            .find(|message| message["role"] == "tool" && message["tool_call_id"] == "qa-reply-call")
            .expect("real reply execution must produce a paired tool result");
        assert!(result["content"].as_str().unwrap().contains("answered"));
        (
            serde_json::json!({"role":"assistant","content":"Answer delivered."}),
            "stop",
        )
    };
    let chunk = serde_json::json!({
        "id":"question-response","object":"chat.completion.chunk","model":"qwen3.5-flash",
        "choices":[{"index":0,"delta":delta,"finish_reason":null}]
    });
    let terminal = serde_json::json!({
        "id":"question-response","object":"chat.completion.chunk","model":"qwen3.5-flash",
        "choices":[{"index":0,"delta":{},"finish_reason":reason}],
        "usage":{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120}
    });
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {chunk}\n\ndata: {terminal}\n\ndata: [DONE]\n\n"
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    stream.shutdown().await.unwrap();
}

/// Extracts the text of a declared Chat `message`; rejects nontext blocks in this text-only fixture.
/// 提取声明的 Chat `message` 文本；在此纯文本夹具中拒绝非文本块。
fn chat_text(message: &serde_json::Value) -> String {
    match &message["content"] {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|part| {
                assert_eq!(part["type"], "text");
                part["text"].as_str().expect("text block content")
            })
            .collect(),
        other => panic!("unexpected Chat content type: {other:?}"),
    }
}
