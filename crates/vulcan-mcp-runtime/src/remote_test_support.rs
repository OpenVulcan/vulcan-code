//! Real loopback connections and operating-system locks witness remote transport ownership.
//! 真实回环连接及操作系统锁见证远程传输所有权。

use crate::{McpResourceRelease, McpTransportResources};
use std::{fs::OpenOptions, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};

/// One bounded observation budget for remote resource fixtures.
/// 一项远程资源夹具共用的有界观察预算。
pub(crate) const OBSERVE: Duration = Duration::from_secs(5);

/// A real HTTP response stays open until the test releases or drops its server.
/// 一条真实 HTTP 响应保持打开，直至测试释放或销毁服务器。
pub(crate) struct HeldHttpServer {
    /// Exact loopback endpoint allocated by the operating system.
    /// 操作系统分配的精确回环端点。
    pub(crate) uri: String,
    /// The request arrival signal contains the actual first HTTP line.
    /// 请求到达信号包含实际 HTTP 首行。
    request: Option<oneshot::Receiver<String>>,
    /// A retained sender holds the response open without sleeping.
    /// 保留发送端，无需休眠即可保持响应打开。
    _hold: oneshot::Sender<()>,
    /// Original socket owner; dropping the fixture cancels this exact task.
    /// 原套接字所有者；销毁夹具取消此精确任务。
    task: JoinHandle<()>,
}

impl HeldHttpServer {
    /// Starts one actual listener with an optional immediate response; absent response holds headers.
    /// 启动真实监听器并可立即响应；无响应则持续阻挡响应头。
    /// Response bytes are static fixture data; returns an endpoint and arrival/ownership controls.
    /// response 字节为静态夹具数据；返回端点及到达、所有权控制。
    pub(crate) async fn start(response: Option<&'static [u8]>) -> Self {
        // Binding port zero prevents fixed-port conflicts with other host tests.
        // 绑定零端口，避免与其他宿主测试固定端口冲突。
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture listener");
        // The URI comes only from the successfully bound listener.
        // URI 仅来自成功绑定的监听器。
        let uri = format!(
            "http://{}/mcp",
            listener.local_addr().expect("fixture address")
        );
        // Arrival and response lifetime have separate controls.
        // 到达及响应生命周期使用独立控制。
        let (request_tx, request) = oneshot::channel();
        let (hold, resume) = oneshot::channel();
        // This original task retains the actual response socket while the client is exercised.
        // 检验客户端期间，此原任务保留实际响应套接字。
        let task = tokio::spawn(async move {
            // Accept exactly one declared test connection.
            // 仅接受一条声明的测试连接。
            let (mut socket, _) = listener.accept().await.expect("fixture accept");
            // Read the actual header delimiter before declaring a request has arrived.
            // 宣告请求到达前读取实际请求头分隔符。
            let mut headers = Vec::new();
            let mut byte = [0_u8; 1];
            while !headers.ends_with(b"\r\n\r\n") {
                assert!(
                    headers.len() < 8192,
                    "fixture request header is unexpectedly large"
                );
                socket
                    .read_exact(&mut byte)
                    .await
                    .expect("fixture request headers");
                headers.push(byte[0]);
            }
            // Complete the real request body so response disposal cannot mask an incomplete POST.
            // 完整读取真实请求体，防止响应销毁掩盖未完成 POST。
            let header_text = String::from_utf8(headers).expect("fixture ASCII headers");
            let content_length = header_text
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().expect("fixture body length"))
                })
                .unwrap_or(0);
            let mut body = vec![0; content_length];
            socket
                .read_exact(&mut body)
                .await
                .expect("fixture request body");
            request_tx
                .send(header_text.lines().next().expect("request line").to_owned())
                .expect("request observer");
            if let Some(response) = response {
                socket.write_all(response).await.expect("fixture response");
            }
            let _ = resume.await;
            drop(socket);
        });
        Self {
            uri,
            request: Some(request),
            _hold: hold,
            task,
        }
    }

    /// Waits for the actual HTTP request rather than guessing with a delay.
    /// 等待实际 HTTP 请求，不以延迟猜测。
    /// No inputs; returns its first request line or fails within the shared observation budget.
    /// 无输入；返回请求首行，或在共享观察预算内失败。
    pub(crate) async fn request_line(&mut self) -> String {
        tokio::time::timeout(
            OBSERVE,
            self.request.take().expect("one request observation"),
        )
        .await
        .expect("request arrival deadline")
        .expect("request arrived")
    }
}

impl Drop for HeldHttpServer {
    /// Cancels only this fixture's original socket owner, including after a failing assertion.
    /// 仅取消此夹具原套接字所有者，断言失败后同样如此。
    /// No inputs or return value; no task survives fixture disposal.
    /// 无输入或返回值；任务不能继续保留夹具资源。
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Creates a real exclusive host file lease with an independent competing witness.
/// 创建真实排他宿主文件租约及独立竞争见证。
/// No inputs; returns the witness, exact transferred owner and original non-owning receipt.
/// 无输入；返回见证、精确移交所有者及原非拥有回执。
pub(crate) fn host_lease() -> (
    tempfile::NamedTempFile,
    McpTransportResources,
    McpResourceRelease,
) {
    // The witness remains outside every transport task.
    // 见证保留在每个传输任务之外。
    let witness = tempfile::NamedTempFile::new().expect("remote host witness");
    // A separately opened handle exposes premature release through an actual OS lock conflict.
    // 单独打开的句柄通过真实操作系统锁冲突暴露提前释放。
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .open(witness.path())
        .expect("remote host lease");
    lease.lock().expect("exclusive remote lease");
    let (resources, release) = McpTransportResources::new(lease);
    (witness, resources, release)
}
