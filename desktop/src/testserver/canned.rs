//! A server that gives every request the same answer, for what transom
//! itself never answers, such as a proxy's 401 in front of it.

use std::fmt::Write as _;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use url::Url;

/// Starts a server that answers every request with `status`, the
/// `headers` given, and `body`, and returns its base URL. It runs until
/// the test's runtime stops.
pub async fn answering(status: &str, headers: &[(&str, &str)], body: &str) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let mut answer = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        let _ = write!(answer, "{name}: {value}\r\n");
    }
    answer.push_str("\r\n");
    answer.push_str(body);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let answer = answer.clone();
            tokio::spawn(async move {
                // The request is read up to the end of its headers, so the
                // client is not still writing when the answer comes.
                let (read, mut write) = stream.split();
                let mut lines = BufReader::new(read).lines();
                while lines
                    .next_line()
                    .await
                    .unwrap()
                    .is_some_and(|line| !line.is_empty())
                {}
                let _ = write.write_all(answer.as_bytes()).await;
            });
        }
    });
    url
}
