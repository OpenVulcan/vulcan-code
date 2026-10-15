//! Fixed authorities for the reviewed OpenDesign device and wallet protocols.
//! 已审核 OpenDesign 设备授权与钱包协议的固定授权端。

/// Control authority shared by device authorization and wallet queries.
/// 设备授权与钱包查询共用的控制端。
pub(crate) const CONTROL_ORIGIN: &str = "https://amr-api.open-design.ai";

/// Builds a client that never forwards device or control credentials through redirects.
/// 创建绝不通过重定向转发设备凭据或控制凭据的客户端。
///
/// Returns the bounded protocol client, or a controlled initialization error.
/// 返回协议客户端，或受控的初始化错误。
pub(crate) fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| String::from("Failed to initialize the OpenDesign account client."))
}

/// Serves synthetic protocol replies and captures complete requests without external traffic.
/// 提供合成协议响应并捕获完整请求，不产生外部流量。
///
/// `replies` contains status, body, and optional response delay. Returns a loopback origin and task.
/// `replies` 包含状态、正文及可选响应延迟。返回回环源与任务。
#[cfg(test)]
pub(crate) async fn test_server(
    replies: Vec<(u16, String, std::time::Duration)>,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // listener is private to this test and is dropped when the supplied replies are exhausted.
    // listener 属于本测试，所给响应耗尽后释放。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock");
    let origin = format!("http://{}", listener.local_addr().expect("mock address"));
    let task = tokio::spawn(async move {
        // requests holds synthetic wire data for behavioral assertions.
        // requests 保存用于行为断言的合成线协议数据。
        let mut requests = Vec::new();
        for (status, body, delay) in replies {
            // Each connection serves one request, matching the response's close semantics.
            // 每条连接服务一个请求，与响应的关闭语义一致。
            let (mut stream, _) = listener.accept().await.expect("accept mock request");
            let mut bytes = Vec::new();
            loop {
                // chunk limits individual reads; the total cap catches malformed test clients.
                // chunk 限制单次读取；总上限捕获异常测试客户端。
                let mut chunk = [0_u8; 4096];
                let count = stream.read(&mut chunk).await.expect("read mock request");
                assert!(
                    count > 0 && bytes.len() < 65536,
                    "incomplete synthetic HTTP request"
                );
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    // HTTP GET legitimately omits Content-Length; POST uses the exact declared size.
                    // HTTP GET 合法省略 Content-Length；POST 使用精确声明的大小。
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                                .map(|(_, value)| {
                                    value.trim().parse::<usize>().expect("mock body length")
                                })
                        })
                        .unwrap_or(0);
                    if bytes.len() >= header_end + 4 + length {
                        break;
                    }
                }
            }
            requests.push(String::from_utf8(bytes).expect("synthetic UTF-8 request"));
            tokio::time::sleep(delay).await;
            // Redirect replies deliberately point elsewhere so the test can verify no forwarding.
            // 重定向响应故意指向其他位置，使测试能验证不转发。
            let response = format!(
                "HTTP/1.1 {status} Synthetic\r\nContent-Type: application/json\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:1/never\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        }
        requests
    });
    (origin, task)
}
