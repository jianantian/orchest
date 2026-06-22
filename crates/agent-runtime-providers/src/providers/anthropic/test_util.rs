use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn serve_sse_once(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test server should bind");
    let address = listener.local_addr().expect("test server should have local address");

    tokio::spawn(async move {
        let (mut socket, _) =
            listener.accept().await.expect("test server should accept one request");
        let mut request = vec![0; 8192];
        let _ = socket.read(&mut request).await.expect("test server should read request");
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.expect("test server should write response");
    });

    format!("http://{address}")
}

pub async fn serve_sse_once_capture(
    body: &'static str,
) -> (String, tokio::sync::oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test server should bind");
    let address = listener.local_addr().expect("test server should have local address");

    let (capture_tx, capture_rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        let (mut socket, _) =
            listener.accept().await.expect("test server should accept one request");
        let mut request = vec![0; 16384];
        let n = socket.read(&mut request).await.expect("test server should read request");
        let req_str = String::from_utf8_lossy(&request[..n]).to_string();
        let _ = capture_tx.send(req_str);

        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.expect("test server should write response");
    });

    (format!("http://{address}"), capture_rx)
}

pub async fn serve_status(status: u16, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test server should bind");
    let address = listener.local_addr().expect("test server should have local address");

    tokio::spawn(async move {
        let (mut socket, _) =
            listener.accept().await.expect("test server should accept one request");
        let mut request = vec![0; 8192];
        let _ = socket.read(&mut request).await.expect("test server should read request");
        let response = format!(
            "HTTP/1.1 {status} Error\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.expect("test server should write response");
    });

    format!("http://{address}")
}

pub async fn serve_partial_sse(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test server should bind");
    let address = listener.local_addr().expect("test server should have local address");

    tokio::spawn(async move {
        let (mut socket, _) =
            listener.accept().await.expect("test server should accept one request");
        let mut request = vec![0; 8192];
        let _ = socket.read(&mut request).await.expect("test server should read request");
        // No content-length, just write and drop the connection
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.expect("test server should write response");
    });

    format!("http://{address}")
}
