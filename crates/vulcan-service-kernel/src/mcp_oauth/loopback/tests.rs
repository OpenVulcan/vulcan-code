//! Redirect planning and loopback callback receipt.
//! 回调地址规划与本机回环回调接收测试。

use super::*;

/// Sends one raw GET request and returns the status line of the reply.
/// 发送一个原始 GET 请求并返回响应状态行。
async fn get(address: std::net::SocketAddr, target: &str) -> String {
    let mut stream = TcpStream::connect(address).await.expect("connect");
    stream
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").as_bytes())
        .await
        .expect("write");
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.expect("read");
    reply.lines().next().unwrap_or_default().to_string()
}

#[tokio::test]
async fn dynamic_registration_gets_an_ephemeral_loopback_redirect() {
    let plan = plan_redirect(None, false).await.expect("plan");

    assert!(plan.redirect_uri.starts_with("http://127.0.0.1:"));
    assert!(plan.redirect_uri.ends_with(DEFAULT_CALLBACK_PATH));
    assert!(matches!(plan.receiver, CallbackReceiver::Loopback { .. }));
}

#[tokio::test]
async fn preregistered_client_without_redirect_is_rejected() {
    assert!(plan_redirect(None, true).await.is_err());
}

#[tokio::test]
async fn non_loopback_redirect_uses_manual_completion() {
    let plan = plan_redirect(Some("https://app.example.test/oauth/callback"), true)
        .await
        .expect("plan");

    assert_eq!(plan.redirect_uri, "https://app.example.test/oauth/callback");
    assert!(matches!(plan.receiver, CallbackReceiver::Manual));
}

#[tokio::test]
async fn configured_loopback_redirect_binds_its_port_and_path() {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let redirect = format!("http://localhost:{port}/oauth/done");

    let plan = plan_redirect(Some(&redirect), true).await.expect("plan");

    assert_eq!(plan.redirect_uri, redirect);
    let CallbackReceiver::Loopback { listener, path, .. } = plan.receiver else {
        panic!("loopback redirect must bind a receiver");
    };
    assert_eq!(listener.local_addr().unwrap().port(), port);
    assert_eq!(path, "/oauth/done");
}

#[tokio::test]
async fn receiver_skips_stray_requests_until_the_matching_state_arrives() {
    let plan = plan_redirect(None, false).await.expect("plan");
    let CallbackReceiver::Loopback {
        listener,
        path,
        origin,
    } = plan.receiver
    else {
        panic!("dynamic registration binds a loopback receiver");
    };
    let address = listener.local_addr().unwrap();
    let receiver = tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (url, mut stream) = receive_callback(&listener, &path, &origin, "expected", deadline)
            .await
            .expect("callback");
        respond(&mut stream, 200, true).await.expect("respond");
        url
    });

    assert!(get(address, "/favicon.ico").await.contains("404"));
    assert!(
        get(address, "/callback?code=c&state=other")
            .await
            .contains("400")
    );
    assert!(
        get(address, "/callback?code=c&state=expected")
            .await
            .contains("200")
    );

    let url = receiver.await.unwrap();
    assert!(url.starts_with(&format!("http://127.0.0.1:{}/callback?", address.port())));
    assert!(url.contains("code=c"));
}

#[tokio::test]
async fn receiver_times_out_at_the_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    let result = receive_callback(
        &listener,
        "/callback",
        "http://127.0.0.1:1",
        "state",
        Instant::now() + Duration::from_millis(50),
    )
    .await;

    assert!(result.is_err());
}
