use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::thread;

fn main() {
    // only allow overriding the port (default host is 127.0.0.1, default port 7878)
    let default_host = "127.0.0.1";
    let default_port: u16 = 7878;
    let port = env::args().nth(1)
        .map(|s| s.parse::<u16>().ok())
        .flatten()
        .take_if(|p| *p > 0)
        .unwrap_or(default_port);

    let addr = format!("{}:{}", default_host, port);
    let listener = TcpListener::bind(&addr).expect("Failed to bind address");
    println!("Serving static files on http://{}/", addr);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| {
                    if let Err(e) = handle_connection(stream) {
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

fn handle_connection(mut stream: TcpStream) -> std::io::Result<()> {
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

    // normalize path
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

    // remove leading slash and serve from current directory (you can place files beside the binary)
    let rel_path = &path[1..];
    let fs_path = Path::new(rel_path);

    match fs::read(fs_path) {
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
