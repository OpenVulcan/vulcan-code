use super::runtime::project_router_arguments_for_test;
use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use vulcan_provider_config::config::{
    PersistedRouterToolBinding, PersistedRouterToolConfiguration, RouterCapabilityContract,
    RouterCapabilityContractReference, RouterClientToolContract, RouterModelToolCapabilities,
    RouterParameterSchema, RouterParameterSchemaKind, RouterParameterSchemaProperty,
    RouterParameterSchemaValue, RouterToolOutputContract, RouterToolParameterMapping,
    RouterToolParameterOwner, RouterToolParameterTargetKind, RouterToolResourceInput,
};

/// Confirms that Router schema conversion retains model-facing constraints without introducing hidden target fields.
/// 确认 Router Schema 转换保留模型可见约束且不会引入隐藏目标字段。
#[test]
fn router_schema_conversion_preserves_contract_without_target_metadata() {
    let contract = image_tool_contract();
    let schema =
        router_parameter_schema_to_json_schema(&contract.parameters).expect("schema conversion");
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["prompt"]["maxLength"], 1500);
    assert_eq!(schema["properties"]["subject_references"]["type"], "array");
    assert_eq!(
        schema["properties"]["subject_references"]["items"]["format"],
        "vulcan-local-path"
    );
    let text = schema.to_string();
    assert!(!text.contains("credential"));
    assert!(!text.contains("provider_instance"));
    assert!(!text.contains("\"target\""));
}

/// Confirms recursive const and if/then/else constraints survive Router schema conversion without semantic loss.
/// 确认递归 const 与 if/then/else 约束在 Router Schema 转换后不会丢失语义。
#[test]
fn router_schema_conversion_preserves_recursive_conditional_keywords() {
    let string_literal = |value: &str| RouterParameterSchemaValue {
        string: Some(value.to_string()),
        ..RouterParameterSchemaValue::default()
    };
    let conditional = RouterParameterSchema {
        kind: Some(RouterParameterSchemaKind::Object),
        r#if: Some(Box::new(RouterParameterSchema {
            kind: Some(RouterParameterSchemaKind::Object),
            properties: vec![RouterParameterSchemaProperty {
                name: String::from("model"),
                required: true,
                schema: RouterParameterSchema {
                    kind: Some(RouterParameterSchemaKind::String),
                    r#const: Some(string_literal("image-01-live")),
                    ..RouterParameterSchema::default()
                },
            }],
            ..RouterParameterSchema::default()
        })),
        then: Some(Box::new(RouterParameterSchema {
            kind: Some(RouterParameterSchemaKind::Object),
            properties: vec![RouterParameterSchemaProperty {
                name: String::from("style"),
                required: true,
                schema: RouterParameterSchema {
                    kind: Some(RouterParameterSchemaKind::String),
                    allowed_values: vec![string_literal("anime")],
                    ..RouterParameterSchema::default()
                },
            }],
            ..RouterParameterSchema::default()
        })),
        r#else: Some(Box::new(RouterParameterSchema {
            not: Some(Box::new(RouterParameterSchema {
                kind: Some(RouterParameterSchemaKind::Object),
                properties: vec![RouterParameterSchemaProperty {
                    name: String::from("style"),
                    required: true,
                    schema: RouterParameterSchema {
                        kind: Some(RouterParameterSchemaKind::String),
                        ..RouterParameterSchema::default()
                    },
                }],
                ..RouterParameterSchema::default()
            })),
            ..RouterParameterSchema::default()
        })),
        ..RouterParameterSchema::default()
    };
    let schema = router_parameter_schema_to_json_schema(&conditional)
        .expect("recursive conditional schema conversion");
    assert_eq!(
        schema["if"]["properties"]["model"]["const"],
        "image-01-live"
    );
    assert_eq!(schema["then"]["required"], json!(["style"]));
    assert_eq!(
        schema["then"]["properties"]["style"]["enum"],
        json!(["anime"])
    );
    assert_eq!(
        schema["else"]["not"]["properties"]["style"]["type"],
        "string"
    );
}

/// Confirms that ambiguous duplicate Router tool names are not registered as an implicit account route.
/// 确认存在歧义的重复 Router 工具名称不会被注册为隐式账号路由。
#[test]
fn duplicate_router_tool_names_do_not_register_an_implicit_route() {
    let directory = tempfile::tempdir().expect("temp directory");
    let mut configuration = router_configuration("http://127.0.0.1:9");
    let mut duplicate = configuration.bindings[0].clone();
    duplicate.id = String::from("binding_2");
    duplicate.credential_id = String::from("credential_2");
    configuration.bindings.push(duplicate);
    let mut candidates = Vec::new();
    append_router_contract_tool_candidates(
        &mut candidates,
        &configuration,
        Some(directory.path().to_path_buf()),
    );
    assert!(candidates.is_empty());
}

/// Confirms that an omitted optional mapped resource does not become a false runtime requirement.
/// 确认省略可选映射资源时不会在运行时被错误提升为必填项。
#[tokio::test]
async fn optional_router_mapping_is_omitted_without_import() {
    let configuration = router_configuration("http://127.0.0.1:9");
    let projected = project_router_arguments_for_test(
        &configuration,
        json!({"prompt": "A deterministic image."}),
    )
    .await
    .expect("optional projection");
    assert_eq!(
        projected["payload"]["image_generate"]["prompt"],
        "A deterministic image."
    );
    assert!(
        projected["payload"]["image_generate"]
            .get("references")
            .is_none()
    );
}

/// Confirms one invocation imports local bytes, creates exactly one execution, polls that execution, and verifies the downloaded result.
/// 确认一次调用会导入本地字节、仅创建一个执行、轮询该执行并校验下载结果。
#[tokio::test]
async fn router_tool_invocation_creates_once_and_downloads_verified_output() {
    let output_bytes = b"router-output-image".to_vec();
    let output_digest = hex_lowercase(&Sha256::digest(&output_bytes));
    let create_count = Arc::new(AtomicUsize::new(0));
    let (base_url, server) =
        spawn_router_server(output_bytes.clone(), output_digest, create_count.clone()).await;
    let directory = tempfile::tempdir().expect("temp directory");
    std::fs::write(directory.path().join("reference.png"), b"input-image").expect("write input");
    let configuration = router_configuration(&base_url);
    let mut candidates = Vec::new();
    append_router_contract_tool_candidates(
        &mut candidates,
        &configuration,
        Some(directory.path().to_path_buf()),
    );
    assert_eq!(candidates.len(), 1);
    let result = (candidates[0].invoke)(
        json!({
            "prompt": "Draw one deterministic test image.",
            "subject_references": ["reference.png"]
        }),
        None,
    )
    .await
    .expect("router invocation");
    let paths = result.model_output["local_paths"]
        .as_array()
        .expect("local paths");
    assert_eq!(paths.len(), 1);
    let output_path = paths[0].as_str().expect("output path");
    assert_eq!(
        std::fs::read(output_path).expect("read output"),
        output_bytes
    );
    assert_eq!(create_count.load(Ordering::SeqCst), 1);
    server.await.expect("router server");
}

/// Builds one valid Router configuration with one exact credential-bound MiniMax-style image tool.
/// 构建一个包含单个精确账号绑定 MiniMax 风格图片工具的有效 Router 配置。
fn router_configuration(base_url: &str) -> PersistedRouterToolConfiguration {
    PersistedRouterToolConfiguration {
        enabled: true,
        base_url: base_url.to_string(),
        api_key: String::from("test-api-key"),
        bindings: vec![PersistedRouterToolBinding {
            id: String::from("binding_1"),
            enabled: true,
            credential_id: String::from("credential_1"),
            contract: RouterCapabilityContract {
                reference: RouterCapabilityContractReference {
                    id: String::from("contract_1"),
                    catalog_revision: 1,
                    content_hash: String::from("hash_1"),
                },
                kind: String::from("model"),
                provider_instance_id: String::from("instance_1"),
                provider_model_id: String::from("model_1"),
                provider_service_id: String::new(),
                offering_id: String::from("offering_1"),
                execution_profile_id: String::from("profile_1"),
                operation: String::from("image.generate"),
                credential_binding: String::from("explicit"),
                model_capabilities: Some(RouterModelToolCapabilities {
                    client_tool: Some(image_tool_contract()),
                }),
                service_capabilities: None,
            },
        }],
    }
}

/// Builds one closed image generation tool contract with one local resource input.
/// 构建一个包含单个本地资源输入的封闭图片生成工具合同。
fn image_tool_contract() -> RouterClientToolContract {
    RouterClientToolContract {
        name: String::from("generate_image"),
        description: String::from(
            "Generate one image with the explicitly configured Router model and account.",
        ),
        operation: String::from("image.generate"),
        parameters: RouterParameterSchema {
            kind: Some(RouterParameterSchemaKind::Object),
            properties: vec![
                RouterParameterSchemaProperty {
                    name: String::from("prompt"),
                    required: true,
                    schema: RouterParameterSchema {
                        kind: Some(RouterParameterSchemaKind::String),
                        description: String::from("Image prompt."),
                        maximum_length: Some(1500),
                        ..RouterParameterSchema::default()
                    },
                },
                RouterParameterSchemaProperty {
                    name: String::from("subject_references"),
                    required: false,
                    schema: RouterParameterSchema {
                        kind: Some(RouterParameterSchemaKind::Array),
                        items: Some(Box::new(RouterParameterSchema {
                            kind: Some(RouterParameterSchemaKind::Resource),
                            description: String::from("Workspace-local reference image path."),
                            ..RouterParameterSchema::default()
                        })),
                        maximum_items: Some(3),
                        ..RouterParameterSchema::default()
                    },
                },
            ],
            ..RouterParameterSchema::default()
        },
        mappings: vec![
            RouterToolParameterMapping {
                source_path: String::from("prompt"),
                owner: RouterToolParameterOwner::Ai,
                target_kind: RouterToolParameterTargetKind::Operation,
                target_path: String::from("payload.image_generate.prompt"),
            },
            RouterToolParameterMapping {
                source_path: String::from("subject_references"),
                owner: RouterToolParameterOwner::Ai,
                target_kind: RouterToolParameterTargetKind::ResourceImport,
                target_path: String::from("payload.image_generate.references"),
            },
        ],
        resource_inputs: vec![RouterToolResourceInput {
            source_path: String::from("subject_references"),
            kind: String::from("image"),
            role: String::from("subject_reference"),
            mime_types: vec![String::from("image/png")],
            maximum_item_bytes: 1024,
            minimum_items: 0,
            maximum_items: 3,
        }],
        output: RouterToolOutputContract {
            kind: String::from("image"),
            mime_types: vec![String::from("image/png")],
            minimum_items: 1,
            maximum_items: 1,
            auto_download: true,
            ai_result_path: String::from("local_paths"),
        },
    }
}

/// Starts one bounded mock Router that closes every response and records execution creation count.
/// 启动一个每次响应后关闭连接并记录执行创建次数的受限模拟 Router。
async fn spawn_router_server(
    output_bytes: Vec<u8>,
    output_digest: String,
    create_count: Arc<AtomicUsize>,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock router");
    let address = listener.local_addr().expect("mock router address");
    let handle = tokio::spawn(async move {
        let mut handled = 0usize;
        while handled < 4 {
            let (mut stream, _) = listener.accept().await.expect("accept request");
            let request = read_http_request(&mut stream).await;
            let first_line = request.lines().next().unwrap_or_default();
            let (content_type, body) = if first_line
                .starts_with("POST /vulcan/v1/resources/import ")
            {
                (
                    "application/json",
                    json!({
                        "id": "resource_input",
                        "kind": "image",
                        "mime_type": "image/png",
                        "size_bytes": 11,
                        "sha256": "input",
                        "state": "ready"
                    })
                    .to_string()
                    .into_bytes(),
                )
            } else if first_line.starts_with("POST /vulcan/v1/executions ") {
                create_count.fetch_add(1, Ordering::SeqCst);
                (
                    "application/json",
                    json!({
                        "execution": {
                            "id": "execution_1",
                            "status": "accepted"
                        },
                        "idempotent_replay": false
                    })
                    .to_string()
                    .into_bytes(),
                )
            } else if first_line.starts_with("GET /vulcan/v1/executions/execution_1 ") {
                (
                    "application/json",
                    json!({
                        "id": "execution_1",
                        "status": "succeeded",
                        "result": {
                            "resources": [{
                                "id": "resource_output",
                                "kind": "image",
                                "mime_type": "image/png",
                                "size_bytes": output_bytes.len(),
                                "sha256": output_digest,
                                "state": "ready"
                            }]
                        }
                    })
                    .to_string()
                    .into_bytes(),
                )
            } else if first_line.starts_with("GET /vulcan/v1/resources/resource_output/content ") {
                ("image/png", output_bytes.clone())
            } else {
                panic!("unexpected mock Router request: {first_line}");
            };
            write_http_response(&mut stream, content_type, &body).await;
            handled += 1;
        }
    });
    (format!("http://{address}"), handle)
}

/// Reads one complete HTTP/1 request including its declared body length.
/// 读取一个完整 HTTP/1 请求及其声明的正文长度。
async fn read_http_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = stream.read(&mut buffer).await.expect("read request");
        assert!(count > 0, "request closed before headers completed");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(header_end) = find_header_end(&bytes) {
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .or_else(|| line.strip_prefix("Content-Length:"))
                })
                .map(str::trim)
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if bytes.len() >= header_end + 4 + content_length {
                break;
            }
        }
    }
    String::from_utf8(bytes).expect("request utf8")
}

/// Finds the terminating CRLF sequence for one HTTP header block.
/// 查找一个 HTTP Header 块的终止 CRLF 序列。
fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Writes one bounded HTTP/1 success response and closes the connection.
/// 写入一个受限 HTTP/1 成功响应并关闭连接。
async fn write_http_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(headers.as_bytes())
        .await
        .expect("write headers");
    stream.write_all(body).await.expect("write body");
    stream.shutdown().await.expect("close response");
}

/// Converts bytes into lowercase hexadecimal for mock resource metadata.
/// 将字节转换为小写十六进制以供模拟资源元数据使用。
fn hex_lowercase(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
