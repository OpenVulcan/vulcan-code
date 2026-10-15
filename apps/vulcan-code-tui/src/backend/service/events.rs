// `/events` WebSocket reader.
// `/events` WebSocket 读取线程。
//
// The service pushes `{kind, payload}` text frames. One reader thread per connection forwards them
// to the worker tagged with the connection generation, so frames from a socket that was already
// replaced are dropped by the worker. The thread ends when the socket closes or `stop` is set.
// 服务推送 `{kind, payload}` 文本帧。每个连接一个读取线程，将帧连同连接代际转发给工作线程，使
// 已被替换的套接字发来的帧由工作线程丢弃。套接字关闭或设置 `stop` 后线程结束。

use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use serde_json::Value;
use tungstenite::Message;
use vulcan_service_api::ServiceEvent;

use super::discovery::Endpoint;
use super::rpc::parse_failure;
use super::worker::Input;

/// Connect and handshake limit; the service is on the loopback interface.
/// 连接与握手上限；服务位于回环接口。
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
/// Read poll interval, so a stop request is noticed without a frame arriving.
/// 读取轮询间隔，使停止请求无需等到帧到达也能被察觉。
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Handle to a running reader; dropping it does not stop the thread, `stop` does.
/// 运行中读取线程的句柄；丢弃句柄不会停止线程，需调用 `stop`。
pub struct EventStream {
    /// Shared cancellation signal observed by the bounded socket read loop.
    /// 由带读取上限的套接字循环观察的共享取消信号。
    stop: Arc<AtomicBool>,
}

impl EventStream {
    /// Signals the event reader to stop without exposing its authenticated URL.
    /// 通知事件读取器停止，不公开其已认证 URL。
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Opens the socket synchronously (so the caller learns about failures at once) and starts the
/// reader thread.
/// 同步打开套接字（使调用方立即得知失败），然后启动读取线程。
/// Returns `io::Result<EventStream>`.
/// 返回 `io::Result<EventStream>`。
pub fn open(
    endpoint: &Endpoint,
    client_id: &str,
    generation: u64,
    sink: Sender<Input>,
) -> io::Result<EventStream> {
    // Url.
    // 传输使用的准确地址。
    let url = endpoint.events_url(client_id);
    // Authority.
    // URL 声明的服务地址。
    let authority = endpoint
        .base_url
        .split("://")
        .nth(1)
        .unwrap_or(&endpoint.base_url)
        .trim_end_matches('/');
    // Address.
    // 监听或连接地址。
    let address = authority
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no service address"))?;
    // Stream.
    // 当前传输流。
    let stream = TcpStream::connect_timeout(&address, HANDSHAKE_TIMEOUT)?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_nodelay(true)?;
    // Handshake errors can echo the request URL, which carries the token; report only the status.
    // 握手错误可能回显携带令牌的请求 URL，因此只报告状态码。
    let (mut socket, _) = tungstenite::client(url.as_str(), stream).map_err(|error| {
        io::Error::other(match error {
            tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response)) => {
                format!("event stream handshake failed: HTTP {}", response.status())
            }
            _ => "event stream handshake failed".to_string(),
        })
    })?;
    socket.get_mut().set_read_timeout(Some(POLL_INTERVAL))?;
    // Stop.
    // 事件读取停止标记。
    let stop = Arc::new(AtomicBool::new(false));
    // Flag.
    // 事件读取停止标记副本。
    let flag = stop.clone();
    thread::Builder::new()
        .name("vulcan-events".into())
        .spawn(move || {
            loop {
                if flag.load(Ordering::Relaxed) {
                    let _ = socket.close(None);
                    break;
                }
                match socket.read() {
                    Ok(Message::Text(text)) => {
                        if sink.send(frame_input(generation, text.as_str())).is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(error))
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) => {}
                    Err(_) => break,
                }
            }
            let _ = sink.send(Input::EventsClosed { generation });
        })?;
    Ok(EventStream { stop })
}

/// Parses one text frame as the shared service event; a mismatch is forwarded as a rejection.
/// 将一帧文本按共享服务事件解析；不匹配时作为拒绝转发。
/// `generation` is the connection generation that received the frame.
/// `generation` 为接收该帧的连接代际。
/// `text` is the raw frame text.
/// `text` 为原始帧文本。
/// Returns `Input`.
/// 返回 `Input`。
fn frame_input(generation: u64, text: &str) -> Input {
    match serde_json::from_str::<ServiceEvent>(text) {
        Ok(event) => Input::Service {
            generation,
            event: Box::new(event),
        },
        Err(error) => Input::Rejected {
            generation,
            // The kind is a protocol name, so it is safe to show.
            // 类别是协议名称，可以安全显示。
            kind: serde_json::from_str::<Value>(text)
                .ok()
                .and_then(|frame| frame.get("kind")?.as_str().map(str::to_string))
                .unwrap_or_default(),
            detail: parse_failure(&error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;

    /// Exercises the actual WebSocket handshake, frame envelope and closure generation.
    /// 检查实际 WebSocket 握手、帧信封及关闭代际。
    #[test]
    #[expect(
        clippy::result_large_err,
        reason = "tungstenite fixes the handshake callback error type to an HTTP response"
    )]
    /// Verifies authenticated WebSocket frames and connection generation with a synthetic loopback server.
    /// 使用合成回环服务验证已认证 WebSocket 帧及连接代际。
    fn websocket_forwards_service_frames_with_connection_generation() {
        // Listener.
        // 合成回环监听器。
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        // Endpoint.
        // 已认证传输端点。
        let endpoint = Endpoint::test_endpoint(listener.local_addr().unwrap());
        // Server.
        // 当前服务器或本地合成监听器。
        let server = thread::spawn(move || {
            // Stream.
            // 当前传输流。
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            // Socket.
            // 当前 WebSocket 连接。
            let mut socket = tungstenite::accept_hdr(
                stream,
                |request: &tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.uri().path(), "/events");
                    assert!(
                        request
                            .uri()
                            .query()
                            .is_some_and(|query| query.contains("clientId=demo-client")
                                && query.contains("token=synthetic-test-token"))
                    );
                    Ok(response)
                },
            )
            .unwrap();
            socket
                .send(Message::text(
                    serde_json::json!({"kind":"workspaceGitChanged","payload":{"projectId":"demo-project"}})
                        .to_string(),
                ))
                .unwrap();
            // Event.
            // 由共享协议构造的合成工作区事件。
            let event = ServiceEvent::WorkspaceGitChanged(Box::new(
                vulcan_service_api::WorkspaceGitChangedEvent {
                    project_id: Some("demo-project".to_string()),
                    workspace_path: "D:/demo".to_string(),
                    repository_keys: vec!["demo-repository".to_string()],
                },
            ));
            socket
                .send(Message::text(serde_json::to_string(&event).unwrap()))
                .unwrap();
            socket.close(None).unwrap();
        });
        // Sink; Current input.
        // 后台收件消息发送端；当前输入。
        let (sink, input) = mpsc::channel();
        // Stream.
        // 当前传输流。
        let stream = open(&endpoint, "demo-client", 7, sink).unwrap();
        // A frame that does not match the shared type is rejected with its kind, not dropped.
        // 不符合共享类型的帧携带类别被拒绝，而不是被丢弃。
        let frame = input.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            matches!(frame,Input::Rejected{generation:7,kind,detail} if kind == "workspaceGitChanged" && detail.contains("missing field"))
        );
        // Frame.
        // 当前事件帧。
        let frame = input.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(
            matches!(frame,Input::Service{generation:7,event} if matches!(&*event, ServiceEvent::WorkspaceGitChanged(event) if event.workspace_path == "D:/demo"))
        );
        assert!(matches!(
            input.recv_timeout(Duration::from_secs(2)).unwrap(),
            Input::EventsClosed { generation: 7 }
        ));
        stream.stop();
        server.join().unwrap();
    }
}
