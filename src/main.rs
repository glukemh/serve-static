use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroU16;
use std::thread;

fn main() {
    // allow overriding the port (default host is 127.0.0.1, default port 7878) and optionally the public dir
    let host = "127.0.0.1";

    let mut port = 7878;
    let mut pub_dir = env::current_dir().expect("Failed to get current dir");

    for arg in env::args().skip(1).take(2) {
      if let Ok(nz) = arg.parse::<NonZeroU16>() {
          port = nz.get();
      } else {
          pub_dir = arg.into();
      }
    }

    if !pub_dir.is_dir() {
        panic!("expecting directory at {}", pub_dir.display())
    }

    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr).expect("Failed to bind address");

    println!("Serving static files on http://{}/ from {}", addr, pub_dir.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let pub_dir = pub_dir.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, pub_dir) {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}

fn handle_connection(mut stream: TcpStream, pub_dir: std::path::PathBuf) -> std::io::Result<()> {
    // read request (just enough to get the request line and headers)
    let mut buffer = [0; 4096];
    let n = stream.read(&mut buffer)?;
    if n == 0 {
        return Ok(());
    }
    let req = String::from_utf8_lossy(&buffer[..n]);
    let mut lines = req.lines();
    let request_line = lines.next().unwrap_or_default();
    // Expect format: GET /path HTTP/1.1
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let mut path = parts.next().unwrap_or("/");

    println!("{} {}", stream.peer_addr().unwrap_or_else(|_| "unknown".parse().unwrap()), request_line);

    if method != "GET" {
        let response = format!(
            "HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(response.as_bytes())?;
        return Ok(());
    }

    // strip query string if present
    if let Some(pos) = path.find('?') {
        path = &path[..pos];
    }

    if path == "/" {
        path = "/index.html";
    }

    // disallow path traversal
    if path.contains("..") {
        let body = b"<h1>400 Bad Request</h1>";
        let response = format!(
            "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes())?;
        stream.write_all(body)?;
        return Ok(());
    }

    // remove leading slash and serve from current directory
    let rel_path = &path[1..];
    let fs_path = pub_dir.join(rel_path);

    match fs::read(&fs_path) {
        Ok(contents) => {
            let content_type = match fs_path.extension().and_then(|s| s.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css; charset=utf-8",
                Some("js") => "application/javascript; charset=utf-8",
                _ => "application/octet-stream",
            };
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                contents.len(),
                content_type
            );
            stream.write_all(header.as_bytes())?;
            stream.write_all(&contents)?;
        }
        Err(_) => {
            let body = format!(
                "<h1>404 Not Found</h1>\n<p>The requested file '{}' was not found.</p>",
                html_escape(rel_path)
            );
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes())?;
        }
    }

    Ok(())
}

fn html_escape(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#x27;".into(),
            _ => c.to_string(),
        })
        .collect()
}
