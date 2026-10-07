//! Loopback integration coverage for the real HTTP execution and wire-capture lifecycle.
//! 真实 HTTP 执行与线路捕获生命周期的环回集成覆盖。

use super::super::ProviderWireRequestTraceDispatch;
use super::super::request_headers::VULCAN_CODE_USER_AGENT;
use super::super::test_support::{
    minimal_request, pinned_openrouter_route, read_test_http_request,
};
use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use vulcan_ai_contract::ReasoningEffort;
use vulcan_llm_core::{OPENAI_RESPONSES_PROFILE_ID, ProtocolRegistry};

/// Verifies the captured API body is the exact JSON body received by the HTTP server.
/// 验证捕获的 API 正文就是 HTTP 服务器实际收到的精确 JSON 正文。
#[tokio::test]
async fn http_wire_request_capture_matches_dispatched_api_body() {
    // listener accepts the one physical HTTP request emitted by the execution client.
    // listener 接收执行客户端发出的唯一真实 HTTP 请求。
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    // address replaces the provider base URL with the bounded loopback endpoint.
    // address 使用有界环回端点替代 Provider 基础 URL。
    let address = listener.local_addr().expect("address should resolve");
    // server records the body received over the socket and returns a valid Responses stream.
    // server 记录 Socket 实际收到的正文，并返回有效的 Responses 流。
    let server = tokio::spawn(async move {
        // stream owns the accepted HTTP connection for this single test dispatch.
        // stream 持有本次单一测试分派接受的 HTTP 连接。
        let (mut stream, _) = listener.accept().await.expect("HTTP client should connect");
        // request retains the complete HTTP request so the body boundary remains verifiable.
        // request 保留完整 HTTP 请求，以便验证正文边界。
        let request = read_test_http_request(&mut stream).await;
        // body is the JSON value the loopback server actually received from reqwest.
        // body 是环回服务器从 reqwest 实际收到的 JSON 值。
        let (_, body) = request
            .split_once("\r\n\r\n")
            .expect("request should contain a body boundary");
        let body: Value = serde_json::from_str(body).expect("request body should be JSON");
        // responseBody completes the normalized stream without adding unrelated behavior.
        // responseBody 完成规范化响应流，不引入无关行为。
        let response_body = [
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-capture\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"message-capture\",\"delta\":\"OK\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"response-capture\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
        ]
        .concat();
        // response is the complete bounded HTTP response sent back to the client.
        // response 是回传给客户端的完整有界 HTTP 响应。
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("HTTP response should send");
        body
    });

    // client uses the production encoder and HTTP transport under test.
    // client 使用被测的生产编码器与 HTTP 传输。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // route binds Responses directly to the loopback server and forbids protocol fallback.
    // route 将 Responses 直接绑定到环回服务器，并禁止协议回退。
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    // secret supplies the same credential projection used by ordinary execution.
    // secret 提供常规执行使用的同一凭据投影。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    // request explicitly disables reasoning so the final API field is independently asserted.
    // request 显式关闭推理，以便独立断言最终 API 字段。
    let mut request = minimal_request();
    request.reasoning_effort = Some(ReasoningEffort::None);
    // requestCapture transfers the exact encoded request and responseCapture retains pre-parser bytes.
    // requestCapture 传递精确编码请求；responseCapture 保留解析前字节。
    let (request_capture_sender, request_capture_receiver) =
        oneshot::channel::<ProviderWireRequestTraceDispatch>();
    let (response_capture_sender, response_capture_receiver) = oneshot::channel();
    // requestCaptureTask acknowledges the same durable barrier used by the runtime writer.
    // requestCaptureTask 确认与运行时写入器相同的可靠屏障。
    let request_capture_task = tokio::spawn(async move {
        let dispatch = request_capture_receiver
            .await
            .expect("transport should publish its exact encoded request");
        let _ = dispatch.persistence_result.send(Ok(()));
        dispatch.request
    });
    // response is the normalized stream produced by the real execution path.
    // response 是真实执行路径产生的规范化响应流。
    let mut response = client.execute_with_wire_api_trace_capture(
        route,
        request,
        secret,
        String::from("execution-wire-capture"),
        request_capture_sender,
        response_capture_sender,
    );
    // completed proves the mock response reached the declared terminal protocol event.
    // completed 证明模拟响应到达已声明的协议终态事件。
    let mut completed = false;
    while let Some(event) = response.next().await {
        completed = matches!(
            event.expect("captured request should succeed"),
            AiStreamEvent::ResponseCompleted
        );
        if completed {
            break;
        }
    }
    // Dropping immediately after the terminal event mirrors the production adapter consumer.
    // 在终态事件后立即析构，模拟生产适配器调用方的真实行为。
    drop(response);
    // captured is the object cloned immediately before the HTTP dispatch call.
    // captured 是紧邻 HTTP 分派调用之前克隆的请求对象。
    let captured = request_capture_task
        .await
        .expect("request capture task should finish");
    // capturedResponse is the raw HTTP payload recorded before SSE framing.
    // capturedResponse 是在 SSE 分帧之前记录的原始 HTTP 载荷。
    let captured_response = response_capture_receiver
        .await
        .expect("transport should publish its raw response");
    // dispatched is the JSON body independently observed at the socket boundary.
    // dispatched 是在 Socket 边界独立观察到的 JSON 正文。
    let dispatched = server.await.expect("server should finish");

    assert!(completed);
    assert_eq!(captured.body, dispatched);
    assert_eq!(captured.body["reasoning"]["effort"], "none");
    assert_eq!(captured.transport, "http_sse");
    assert_eq!(captured.method, "POST");
    assert_eq!(captured.path, "/responses");
    assert_eq!(
        captured.headers.get("authorization").map(String::as_str),
        Some("<redacted>")
    );
    assert_eq!(
        captured.headers.get("user-agent").map(String::as_str),
        Some(VULCAN_CODE_USER_AGENT)
    );
    assert_eq!(
        captured
            .headers
            .get("x-openrouter-categories")
            .map(String::as_str),
        Some("programming-app")
    );
    assert!(!captured.headers.values().any(|value| value == "secret"));
    assert_eq!(captured_response.transport, "http_sse");
    assert_eq!(captured_response.http_status, Some(200));
    assert_eq!(captured_response.outcome, "response_completed");
    assert!(!captured_response.body_truncated);
    assert!(!captured_response.chunk_index_truncated);
    assert!(!captured_response.chunks.is_empty());
    assert!(captured_response.chunks.iter().all(|chunk| {
        chunk.observed_at_unix_ms > 0 && chunk.sha256.len() == 64 && chunk.length > 0
    }));
    assert_eq!(
        captured_response.body_sha256,
        hex::encode(Sha256::digest(captured_response.body.as_slice()))
    );
    assert_eq!(
        captured_response.received_bytes,
        u64::try_from(captured_response.body.len()).expect("response length should fit")
    );
    let raw_response =
        String::from_utf8(captured_response.body).expect("SSE response should be UTF-8");
    assert!(raw_response.contains("response.output_text.delta"));
    assert!(raw_response.contains("response.completed"));
}

/// Verifies a failed trace writer does not replace the Provider business result.
/// 验证追踪写入器失败不会覆盖 Provider 业务结果。
#[tokio::test]
async fn http_trace_persistence_failure_does_not_prevent_network_dispatch() {
    // listener observes the business request that must continue after the rejected trace write.
    // listener 观察追踪写入被拒绝后仍必须继续的业务请求。
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");
    // server returns one valid Responses stream after confirming the physical dispatch.
    // server 在确认真实分派后返回一份合法的 Responses 流。
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("HTTP client should connect");
        let request = read_test_http_request(&mut stream).await;
        let response_body = [
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-trace-failure\"}}\n\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"message-trace-failure\",\"delta\":\"OK\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"response-trace-failure\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
        ]
        .concat();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("HTTP response should send");
        request
    });
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let (request_capture_sender, request_capture_receiver) =
        oneshot::channel::<ProviderWireRequestTraceDispatch>();
    let (response_capture_sender, response_capture_receiver) = oneshot::channel();
    // barrierTask returns the same writer failure shape produced by durable trace persistence.
    // barrierTask 返回与可靠追踪持久化相同形态的写入器失败。
    let barrier_task = tokio::spawn(async move {
        let dispatch = request_capture_receiver
            .await
            .expect("transport should publish request before dispatch");
        dispatch
            .persistence_result
            .send(Err(String::from("simulated trace writer failure")))
            .expect("transport should await the barrier");
    });
    let mut response = client.execute_with_wire_api_trace_capture(
        route,
        minimal_request(),
        secret,
        String::from("execution-trace-barrier-failure"),
        request_capture_sender,
        response_capture_sender,
    );
    // events prove the Provider response remains authoritative despite the diagnostic failure.
    // events 证明即使诊断写入失败，Provider 返回仍保持权威。
    let mut events = Vec::new();
    while let Some(event) = response.next().await {
        events.push(event.expect("business response should remain successful"));
    }
    barrier_task.await.expect("barrier task should finish");
    let dispatched_request = server.await.expect("server should finish");
    assert!(dispatched_request.contains("POST "));
    assert!(!events.is_empty());
    let captured_response = response_capture_receiver
        .await
        .expect("response guard should publish the received business response");
    assert!(captured_response.received_bytes > 0);
    assert!(!captured_response.body.is_empty());
    assert_eq!(captured_response.outcome, "response_completed");
}

/// Verifies a post-capture connection failure is classified as dispatch instead of cancellation.
/// 验证请求捕获后的连接失败会归类为分派失败，而不是取消。
#[tokio::test]
async fn http_wire_response_capture_classifies_dispatch_failure() {
    // closedAddress reserves and then releases one loopback port so connection refusal is deterministic.
    // closedAddress 先保留再释放一个环回端口，使连接拒绝具备确定性。
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let closed_address = listener.local_addr().expect("address should resolve");
    drop(listener);

    // client and route exercise the production request capture before the failed HTTP dispatch.
    // client 与 route 在 HTTP 分派失败前走过生产请求捕获链路。
    let client = ProviderExecutionClient::new(
        reqwest::Client::builder()
            .no_proxy()
            .build()
            .expect("direct test client should build"),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{closed_address}"));
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    let (request_capture_sender, request_capture_receiver) =
        oneshot::channel::<ProviderWireRequestTraceDispatch>();
    let (response_capture_sender, response_capture_receiver) = oneshot::channel();
    let request_capture_task = tokio::spawn(async move {
        let dispatch = request_capture_receiver
            .await
            .expect("transport should publish request before dispatch");
        let _ = dispatch.persistence_result.send(Ok(()));
    });

    // response must fail through the Provider stream while its diagnostic guard still publishes.
    // response 必须通过 Provider 流失败，同时诊断守卫仍会发布捕获结果。
    let mut response = client.execute_with_wire_api_trace_capture(
        route,
        minimal_request(),
        secret,
        String::from("execution-wire-dispatch-failure"),
        request_capture_sender,
        response_capture_sender,
    );
    let failure = response
        .next()
        .await
        .expect("dispatch should resolve")
        .expect_err("closed port must reject the request");
    drop(response);
    request_capture_task
        .await
        .expect("request capture task should finish");
    let captured_response = response_capture_receiver
        .await
        .expect("response guard should publish dispatch failure");

    assert!(failure.error.retryable);
    assert_eq!(captured_response.failure_stage.as_deref(), Some("dispatch"));
    assert_eq!(captured_response.received_bytes, 0);
    assert!(captured_response.body.is_empty());
    assert_ne!(
        captured_response.failure_stage.as_deref(),
        Some("cancelled")
    );
}

/// Verifies protocol decoding failures still publish the exact pre-parser HTTP response.
/// 验证协议解码失败时仍会发布进入解析器前的精确 HTTP 响应。
#[tokio::test]
async fn http_wire_response_capture_survives_protocol_failure() {
    // listener accepts the one physical HTTP request emitted by the execution client.
    // listener 接收执行客户端发出的唯一真实 HTTP 请求。
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    // address replaces the provider base URL with the bounded loopback endpoint.
    // address 使用有界环回端点替代 Provider 基础 URL。
    let address = listener.local_addr().expect("address should resolve");
    // malformedResponseBody is valid SSE framing whose data payload is invalid JSON.
    // malformedResponseBody 使用合法 SSE 分帧承载非法 JSON 数据载荷。
    let malformed_response_body = String::from("data: {not-json}\n\n");
    // expectedResponseBody independently retains the bytes the capture must publish.
    // expectedResponseBody 独立保留捕获结果必须发布的字节。
    let expected_response_body = malformed_response_body.clone();
    // server returns the malformed payload only after consuming the dispatched request.
    // server 在消费已分派请求后返回格式错误的载荷。
    let server = tokio::spawn(async move {
        // stream owns the accepted HTTP connection for this single test dispatch.
        // stream 持有本次单一测试分派接受的 HTTP 连接。
        let (mut stream, _) = listener.accept().await.expect("HTTP client should connect");
        // request confirms the production HTTP client completed its outbound dispatch.
        // request 用于确认生产 HTTP 客户端已经完成出站分派。
        let _request = read_test_http_request(&mut stream).await;
        // response is a complete HTTP success whose SSE payload must fail protocol decoding.
        // response 是完整 HTTP 成功响应，但其 SSE 载荷必须在协议解码时失败。
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            malformed_response_body.len(),
            malformed_response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("HTTP response should send");
    });

    // client uses the production encoder, frame parser, and Responses decoder under test.
    // client 使用被测的生产编码器、分帧解析器与 Responses 解码器。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // route binds Responses directly to the loopback server and forbids protocol fallback.
    // route 将 Responses 直接绑定到环回服务器，并禁止协议回退。
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    // secret supplies the same credential projection used by ordinary execution.
    // secret 提供常规执行使用的同一凭据投影。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    // requestCapture confirms the request existed before the response failed decoding.
    // requestCapture 用于确认响应解码失败前请求已经存在。
    let (request_capture_sender, request_capture_receiver) =
        oneshot::channel::<ProviderWireRequestTraceDispatch>();
    // responseCapture must survive every error return after raw bytes are observed.
    // responseCapture 必须在观察到原始字节后的所有错误返回路径中存活。
    let (response_capture_sender, response_capture_receiver) = oneshot::channel();
    // requestCaptureTask releases dispatch after modeling successful durable persistence.
    // requestCaptureTask 在模拟可靠持久化成功后放行分派。
    let request_capture_task = tokio::spawn(async move {
        let dispatch = request_capture_receiver
            .await
            .expect("transport should publish its exact encoded request");
        let _ = dispatch.persistence_result.send(Ok(()));
        dispatch.request
    });
    // response is the normalized stream expected to terminate with a protocol failure.
    // response 是预期以协议失败终止的规范化流。
    let mut response = client.execute_with_wire_api_trace_capture(
        route,
        minimal_request(),
        secret,
        String::from("execution-wire-response-failure"),
        request_capture_sender,
        response_capture_sender,
    );
    // failureObserved proves malformed Provider JSON crossed the public stream error boundary.
    // failureObserved 证明格式错误的 Provider JSON 已经过公开流错误边界。
    let mut failure_observed = false;
    while let Some(event) = response.next().await {
        failure_observed |= event.is_err();
    }
    // capturedRequest proves this was a post-dispatch response failure, not an encode failure.
    // capturedRequest 证明这是分派后的响应失败，而不是编码失败。
    let _captured_request = request_capture_task
        .await
        .expect("request capture task should finish");
    // capturedResponse is delivered by the guard even though the decoder returned an error.
    // capturedResponse 即使解码器返回错误，也会由守卫发布。
    let captured_response = response_capture_receiver
        .await
        .expect("transport should preserve the failed raw response");
    server.await.expect("server should finish");

    assert!(failure_observed);
    assert_eq!(captured_response.transport, "http_sse");
    assert_eq!(captured_response.http_status, Some(200));
    assert_eq!(
        captured_response.content_type.as_deref(),
        Some("text/event-stream")
    );
    assert_eq!(captured_response.body, expected_response_body.into_bytes());
    assert_eq!(
        captured_response.received_bytes,
        u64::try_from(captured_response.body.len()).expect("response length should fit")
    );
    assert_eq!(captured_response.outcome, "stream_failed_or_cancelled");
    assert_eq!(
        captured_response.failure_stage.as_deref(),
        Some("response_projection")
    );
    assert!(
        captured_response
            .failure_message
            .as_deref()
            .is_some_and(|message| message.contains("JSON"))
    );
    assert!(!captured_response.body_truncated);
    assert!(!captured_response.chunk_index_truncated);
}

/// Verifies a decoded tool-argument abort remains captured when its consumer stops immediately.
/// 验证调用方在工具参数中止后立即停止消费时，原始响应仍会被捕获。
#[tokio::test]
async fn http_wire_response_capture_survives_tool_argument_abort() {
    // listener accepts the one physical HTTP request emitted by the execution client.
    // listener 接收执行客户端发出的唯一真实 HTTP 请求。
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    // address replaces the provider base URL with the bounded loopback endpoint.
    // address 使用有界环回端点替代 Provider 基础 URL。
    let address = listener.local_addr().expect("address should resolve");
    // abortedResponseBody uses a complete Responses lifecycle whose authoritative file_edit
    // output item contains incomplete arguments.
    // abortedResponseBody 使用完整 Responses 生命周期，但其权威 file_edit 输出项包含不完整参数。
    let aborted_response_body = [
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-tool-abort\"}}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"item-tool-abort\",\"call_id\":\"call-tool-abort\",\"name\":\"file_edit\",\"arguments\":\"\"}}\n\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"item-tool-abort\",\"output_index\":0,\"call_id\":\"call-tool-abort\",\"delta\":\"{\\\"operations\\\":[\"}\n\n",
        "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"item-tool-abort\",\"output_index\":0,\"call_id\":\"call-tool-abort\",\"arguments\":\"{\\\"operations\\\":[\"}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"item-tool-abort\",\"call_id\":\"call-tool-abort\",\"name\":\"file_edit\",\"arguments\":\"{\\\"operations\\\":[\"}}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"response-tool-abort\",\"usage\":{}}}\n\n",
    ]
    .concat();
    // expectedResponseBody independently retains the bytes the capture must publish.
    // expectedResponseBody 独立保留捕获结果必须发布的字节。
    let expected_response_body = aborted_response_body.clone();
    // server returns the argument-abort stream only after consuming the dispatched request.
    // server 在消费已分派请求后返回工具参数中止流。
    let server = tokio::spawn(async move {
        // stream owns the accepted HTTP connection for this single test dispatch.
        // stream 持有本次单一测试分派接受的 HTTP 连接。
        let (mut stream, _) = listener.accept().await.expect("HTTP client should connect");
        // request confirms the production HTTP client completed its outbound dispatch.
        // request 用于确认生产 HTTP 客户端已经完成出站分派。
        let _request = read_test_http_request(&mut stream).await;
        // response is a complete HTTP and Responses success whose tool arguments remain invalid.
        // response 是完整 HTTP 与 Responses 成功响应，但其中的工具参数保持非法。
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            aborted_response_body.len(),
            aborted_response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("HTTP response should send");
    });

    // client uses the production Responses decoder under the same HTTP transport as the failure.
    // client 在与故障相同的 HTTP 传输下使用生产 Responses 解码器。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    // route binds Responses directly to the loopback server and forbids protocol fallback.
    // route 将 Responses 直接绑定到环回服务器，并禁止协议回退。
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    // secret supplies the same credential projection used by ordinary execution.
    // secret 提供常规执行使用的同一凭据投影。
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");
    // requestCapture confirms the request existed before the tool arguments were aborted.
    // requestCapture 用于确认工具参数中止前请求已经存在。
    let (request_capture_sender, request_capture_receiver) =
        oneshot::channel::<ProviderWireRequestTraceDispatch>();
    // responseCapture must publish when the higher layer stops at ToolCallAborted.
    // responseCapture 必须在上层遇到 ToolCallAborted 停止时发布。
    let (response_capture_sender, response_capture_receiver) = oneshot::channel();
    // requestCaptureTask releases dispatch after modeling successful durable persistence.
    // requestCaptureTask 在模拟可靠持久化成功后放行分派。
    let request_capture_task = tokio::spawn(async move {
        let dispatch = request_capture_receiver
            .await
            .expect("transport should publish its exact encoded request");
        let _ = dispatch.persistence_result.send(Ok(()));
        dispatch.request
    });
    // response is consumed only through the same semantic abort boundary used by the adapter.
    // response 仅消费到适配器使用的同一语义中止边界。
    let mut response = client.execute_with_wire_api_trace_capture(
        route,
        minimal_request(),
        secret,
        String::from("execution-wire-tool-abort"),
        request_capture_sender,
        response_capture_sender,
    );
    // abortObserved proves the valid outer stream decoded into an invalid inner tool call.
    // abortObserved 证明合法外层流被解码为无效的内层工具调用。
    let mut abort_observed = false;
    while let Some(event) = response.next().await {
        let event = event.expect("outer SSE protocol should remain valid");
        if matches!(event, AiStreamEvent::ToolCallAborted { .. }) {
            abort_observed = true;
            break;
        }
    }
    // Dropping the suspended stream models the adapter returning on the tool-call abort.
    // 丢弃暂停的流用于模拟适配器在工具调用中止时直接返回。
    drop(response);
    // capturedRequest proves this was a post-dispatch semantic failure.
    // capturedRequest 证明这是分派后的语义失败。
    let _captured_request = request_capture_task
        .await
        .expect("request capture task should finish");
    // capturedResponse must arrive from the guard after the suspended stream is dropped.
    // capturedResponse 必须在暂停流被丢弃后由守卫发布。
    let captured_response = response_capture_receiver
        .await
        .expect("transport should preserve the tool-abort response");
    server.await.expect("server should finish");

    assert!(abort_observed);
    assert_eq!(captured_response.http_status, Some(200));
    assert_eq!(captured_response.body, expected_response_body.into_bytes());
    assert_eq!(captured_response.outcome, "stream_failed_or_cancelled");
    // abortDiagnostic must survive even though no response-level terminal event was consumed.
    // abortDiagnostic 即使未消费响应级终态事件也必须保留。
    let abort_diagnostic = captured_response
        .normalized_tool_call_abort
        .as_deref()
        .expect("tool-abort diagnostic should be captured");
    assert!(abort_diagnostic.contains("before valid JSON was complete"));
    assert!(abort_diagnostic.contains("line 1 column"));
    assert_eq!(
        captured_response.failure_stage.as_deref(),
        Some("tool_arguments_json")
    );
    assert_eq!(
        captured_response.failure_message.as_deref(),
        Some(abort_diagnostic)
    );
    assert!(!captured_response.body_truncated);
}

/// Verifies a terminal Responses failure never touches the Chat or Anthropic endpoints:
/// every physical request stays on the bound protocol (plan §13.5.5).
/// 验证 Responses 终态失败绝不触达 Chat 或 Anthropic 端点：每个真实请求都停留在绑定协议上（方案 §13.5.5）。
#[tokio::test]
async fn responses_failure_never_touches_other_protocol_endpoints() {
    use std::sync::Mutex as StdMutex;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener should bind");
    let address = listener.local_addr().expect("address should resolve");
    let observed_paths = std::sync::Arc::new(StdMutex::new(Vec::<String>::new()));
    let server_paths = std::sync::Arc::clone(&observed_paths);
    // client_done fires once the execution stream is dropped; the client awaits every response before
    // yielding its terminal error, so any request it made is already pending or accepted by then.
    // client_done 在执行流被丢弃后触发；客户端在产生终态错误前等待每个响应，
    // 因此其发出的任何请求届时都已在等待接受或已被接受。
    let (client_done, mut client_finished) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        // Accept up to four requests to detect any cross-protocol attempt; each returns a
        // terminal 400 so the client must never follow up on another protocol endpoint.
        // 最多接受四个请求以探测任何跨协议尝试；每个都返回终态 400，
        // 客户端绝不能在另一协议端点上继续跟进。
        for _ in 0..4 {
            // A pending connection is accepted before the finish signal is honoured.
            // 先接受已挂起的连接，再响应结束信号。
            let accept_result = tokio::select! {
                biased;
                accepted = listener.accept() => accepted,
                _ = &mut client_finished => break,
            };
            let Ok((mut stream, _)) = accept_result else {
                break;
            };
            let request = read_test_http_request(&mut stream).await;
            let path = request
                .lines()
                .next()
                .unwrap_or_default()
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            server_paths.lock().expect("path lock").push(path);
            let response = "HTTP/1.1 400 Bad Request
content-type: application/json
content-length: 2
connection: close

{}";
            stream
                .write_all(response.as_bytes())
                .await
                .expect("400 response should send");
        }
    });

    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
    );
    let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
    route.protocol_route.base_url_override = Some(format!("http://{address}"));
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
        .expect("test key should validate")
        .execution_secret()
        .expect("test key should resolve");

    let mut stream = client.execute(
        route,
        minimal_request(),
        secret,
        String::from("execution-responses-400"),
    );
    let mut terminal_seen = false;
    while let Some(event) = stream.next().await {
        if event.is_err() {
            terminal_seen = true;
            break;
        }
    }
    drop(stream);
    client_done.send(()).expect("mock server is still listening");
    server.await.expect("mock server should finish");

    assert!(
        terminal_seen,
        "the 400 must terminate the execution attempt"
    );
    let paths = observed_paths.lock().expect("path lock").clone();
    assert!(
        !paths.is_empty(),
        "the bound protocol endpoint must have been hit"
    );
    assert!(
        paths.iter().all(|path| path.starts_with("/responses")),
        "no request may leave the bound responses protocol: {paths:?}"
    );
}
