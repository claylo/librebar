#![allow(missing_docs)]
#![cfg(feature = "http-cookies")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use librebar::http::HttpClient;

fn read_request(stream: &mut impl Read) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return String::from_utf8(request).unwrap();
        }
    }
}

fn spawn_server(
    request_count: usize,
    responder: impl Fn(usize, &str) -> Vec<u8> + Send + 'static,
) -> (SocketAddr, mpsc::Receiver<String>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (request_tx, request_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for index in 0..request_count {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let request = read_request(&mut stream);
            let response = responder(index, &request);
            request_tx.send(request).unwrap();
            stream.write_all(&response).unwrap();
        }
    });
    (address, request_rx, server)
}

fn response(headers: &[(&str, &str)], body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    )
    .into_bytes();
    for (name, value) in headers {
        response.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(body);
    response
}

fn redirect_response(location: &str, cookie: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nSet-Cookie: {cookie}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

#[tokio::test]
async fn clients_are_stateless_without_explicit_cookie_opt_in() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response(&[("Set-Cookie", "session=secret; Path=/")], b"login")
        } else {
            response(&[], b"profile")
        }
    });
    let client = HttpClient::from_app("librebar-test", "0.1.0").unwrap();

    client
        .get(&format!("http://{address}/login"))
        .await
        .unwrap();
    client
        .get(&format!("http://{address}/profile"))
        .await
        .unwrap();

    assert!(
        !requests
            .recv()
            .unwrap()
            .to_ascii_lowercase()
            .contains("cookie:")
    );
    assert!(
        !requests
            .recv()
            .unwrap()
            .to_ascii_lowercase()
            .contains("cookie:")
    );
    assert!(client.cookie_jar().is_none());
    server.join().unwrap();
}

#[tokio::test]
async fn opted_in_client_captures_and_sends_cookies() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response(
                &[("Set-Cookie", "session=secret; Path=/; HttpOnly")],
                b"login",
            )
        } else {
            response(&[], b"profile")
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();

    client
        .get(&format!("http://{address}/login"))
        .await
        .unwrap();
    client
        .get(&format!("http://{address}/profile"))
        .await
        .unwrap();

    assert!(
        !requests
            .recv()
            .unwrap()
            .to_ascii_lowercase()
            .contains("cookie:")
    );
    let profile = requests.recv().unwrap().to_ascii_lowercase();
    assert!(profile.contains("cookie: session=secret\r\n"), "{profile}");
    assert!(client.cookie_jar().is_some());
    server.join().unwrap();
}

#[tokio::test]
async fn redirect_hops_can_set_cookies_for_the_next_request() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            redirect_response("/profile", "session=secret; Path=/")
        } else {
            response(&[], b"profile")
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();

    let result = client
        .get(&format!("http://{address}/login"))
        .await
        .unwrap();

    assert_eq!(result.status, 200);
    assert!(
        !requests
            .recv()
            .unwrap()
            .to_ascii_lowercase()
            .contains("cookie:")
    );
    let profile = requests.recv().unwrap().to_ascii_lowercase();
    assert!(profile.contains("cookie: session=secret\r\n"), "{profile}");
    server.join().unwrap();
}

#[tokio::test]
async fn cookies_are_scoped_to_the_origin_host() {
    let (address, requests, server) = spawn_server(2, |index, _| {
        if index == 0 {
            response(&[("Set-Cookie", "session=secret; Path=/")], b"login")
        } else {
            response(&[], b"other host")
        }
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();

    client
        .get(&format!("http://{address}/login"))
        .await
        .unwrap();
    client
        .get(&format!("http://localhost:{}/profile", address.port()))
        .await
        .unwrap();

    requests.recv().unwrap();
    let other_host = requests.recv().unwrap().to_ascii_lowercase();
    assert!(!other_host.contains("cookie:"), "{other_host}");
    server.join().unwrap();
}

#[tokio::test]
async fn cookie_jar_can_be_saved_and_reloaded() {
    let jar_file = tempfile::NamedTempFile::new().unwrap();
    let jar_path = jar_file.path().to_path_buf();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&jar_path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let (login_address, login_requests, login_server) = spawn_server(1, |_, _| {
        response(&[("Set-Cookie", "session=secret; Path=/")], b"login")
    });
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();
    client
        .get(&format!("http://{login_address}/login"))
        .await
        .unwrap();
    client.cookie_jar().unwrap().save_to(&jar_path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&jar_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    login_requests.recv().unwrap();
    login_server.join().unwrap();

    let (profile_address, profile_requests, profile_server) =
        spawn_server(1, |_, _| response(&[], b"profile"));
    let reloaded = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar_from(&jar_path)
        .build()
        .unwrap();
    reloaded
        .get(&format!("http://{profile_address}/profile"))
        .await
        .unwrap();

    let profile = profile_requests.recv().unwrap().to_ascii_lowercase();
    assert!(profile.contains("cookie: session=secret\r\n"), "{profile}");
    profile_server.join().unwrap();
}

#[cfg(unix)]
#[test]
fn cookie_jar_save_replaces_a_symlink_instead_of_following_it() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.txt");
    let jar_path = directory.path().join("cookies.json");
    std::fs::write(&target, b"do not overwrite").unwrap();
    symlink(&target, &jar_path).unwrap();
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();

    client.cookie_jar().unwrap().save_to(&jar_path).unwrap();

    assert_eq!(std::fs::read(&target).unwrap(), b"do not overwrite");
    assert!(
        !std::fs::symlink_metadata(&jar_path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn cookie_jar_save_refuses_to_replace_a_directory() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("cookies.json");
    std::fs::create_dir(&destination).unwrap();
    let client = HttpClient::builder("librebar-test", "0.1.0")
        .with_cookie_jar()
        .build()
        .unwrap();

    client
        .cookie_jar()
        .unwrap()
        .save_to(&destination)
        .expect_err("directory must be rejected");

    assert!(destination.is_dir());
}
