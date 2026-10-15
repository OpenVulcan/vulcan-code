// Loopback-only HTTP fixtures shared by transport and worker regression tests.
// 传输与工作线程回归测试共用的仅回环 HTTP 夹具。

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::discovery::Endpoint;
use super::rpc::RpcClient;

/// One parsed synthetic POST, retained only by a test for protocol assertions.
/// 单次解析后的合成 POST，仅由测试保留以检查协议。
pub(super) struct ObservedRequest {
    /// Request line including its exact RPC endpoint.
    /// 包含准确 RPC 端点的请求行。
    pub request_line: String,
    /// Lowercase HTTP field names and their synthetic values.
    /// 小写 HTTP 字段名及其合成值。
    pub headers: BTreeMap<String, String>,
    /// Exact JSON envelope received from the production client.
    /// 从实际客户端收到的准确 JSON 信封。
    pub body: Value,
}

/// A bounded local server; tests join the thread after consuming every expected request.
/// 有界本地服务；测试消费全部预期请求后等待线程结束。
pub(super) struct RpcServer {
    /// Production transport aimed at this synthetic listener.
    /// 指向此合成监听器的实际传输客户端。
    pub client: RpcClient,
    /// Channel containing exact observed requests.
    /// 保存准确观察请求的通道。
    pub requests: Receiver<ObservedRequest>,
    /// Server thread with a finite accept/read deadline.
    /// 具有有限接受与读取期限的服务线程。
    pub thread: JoinHandle<()>,
}

/// Parses a single Content-Length JSON request from stream and returns its verified envelope.
/// 从 stream 解析单次 Content-Length JSON 请求，返回已检查的信封。
fn read_request(stream: &mut TcpStream) -> ObservedRequest {
    // A failed client must end the test promptly rather than stall the suite.
    // 客户端失败时应及时结束测试，避免阻塞整个测试套件。
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // Bytes.
    // 完整字节缓冲。
    let mut bytes = Vec::new();
    // Buffer.
    // 临时字节读取缓冲。
    let mut buffer = [0; 4096];
    // Header end.
    // HTTP 请求头结束位置。
    let header_end = loop {
        // Count.
        // 当前计数。
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "client closed before headers");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    // Head.
    // HTTP 请求头文本。
    let head = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    // Lines.
    // 当前文本行。
    let mut lines = head.lines();
    // Request line.
    // 准确 HTTP 请求行。
    let request_line = lines.next().unwrap().to_string();
    // Headers.
    // 请求头名称及值。
    let headers: BTreeMap<String, String> = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            // Name; Value.
            // 当前名称；当前协议值。
            let (name, value) = line.split_once(':').unwrap();
            (name.to_ascii_lowercase(), value.trim().to_string())
        })
        .collect();
    // Length.
    // 声明的正文长度。
    let length: usize = headers["content-length"].parse().unwrap();
    while bytes.len() < header_end + length {
        // Count.
        // 当前计数。
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "client closed before body");
        bytes.extend_from_slice(&buffer[..count]);
    }
    ObservedRequest {
        request_line,
        headers,
        body: serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap(),
    }
}

impl RpcServer {
    /// Accepts count requests and responds with handler's envelopes using the real HTTP client.
    /// 接受 count 次请求，使用实际 HTTP 客户端并返回 handler 提供的信封。
    pub fn start(
        count: usize,
        mut handler: impl FnMut(&ObservedRequest) -> Value + Send + 'static,
    ) -> Self {
        // Listener.
        // 合成回环监听器。
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        // Endpoint.
        // 已认证传输端点。
        let endpoint = Endpoint::test_endpoint(listener.local_addr().unwrap());
        // Client.
        // RPC 传输客户端。
        let client = RpcClient::new(endpoint);
        listener.set_nonblocking(true).unwrap();
        // Sender; Requests.
        // 测试请求观察发送端；测试请求观察接收端。
        let (sender, requests) = mpsc::channel();
        // Thread.
        // 本地合成服务线程。
        let thread = thread::spawn(move || {
            // Deadline.
            // 测试或连接的有限截止时间。
            let deadline = Instant::now() + Duration::from_secs(10);
            for _ in 0..count {
                // Stream.
                // 当前传输流。
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                Instant::now() < deadline,
                                "synthetic server accept deadline"
                            );
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("synthetic server: {error}"),
                    }
                };
                // Exact protocol request.
                // 协议请求文档。
                let request = read_request(&mut stream);
                // Response.
                // 响应信封。
                let response = handler(&request).to_string();
                sender.send(request).unwrap();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
                stream.flush().unwrap();
            }
        });
        Self {
            client,
            requests,
            thread,
        }
    }
}
