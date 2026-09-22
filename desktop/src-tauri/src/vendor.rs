//! Serves the ChatGPT app's own renderer bundle.
//!
//! The Codex Micro settings page inside the app is a React route whose component
//! reads the app's settings store, and that store is initialised by the app's
//! boot graph — so the page cannot be lifted out on its own. Instead the whole
//! renderer bundle is served here and a window points at
//! `/settings/codex-micro`, which is the same route the app itself uses.
//!
//! The bundle belongs to OpenAI, so `scripts/extract-vendor-webview.mjs` copies
//! it out of the installed app into `desktop/vendor-webview/` (gitignored)
//! rather than shipping it in the repository.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};

/// The renderer entry point inside the extracted bundle.
const INDEX: &str = "index.html";

/// Where the extracted bundle lives, in order of preference.
pub fn root() -> Option<PathBuf> {
    let candidates = [
        // development: the repository checkout
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vendor-webview/webview"),
        // installed: next to the executable
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|dir| dir.join("vendor-webview").join("webview")))
            .unwrap_or_default(),
    ];
    candidates.into_iter().find(|dir| dir.join(INDEX).is_file())
}

/// Bind a loopback port and serve `root` forever. Returns the bound port.
pub fn serve(root: PathBuf) -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            std::thread::spawn(move || {
                let _ = handle(stream, &root);
            });
        }
    });
    Ok(port)
}

fn handle(mut stream: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let target = request.split_whitespace().nth(1).unwrap_or("/");
    let path = target.split(['?', '#']).next().unwrap_or("/").trim_start_matches('/');
    let mut file = root.join(percent_decode(path));
    let is_index = path.is_empty() || path == INDEX;
    if !file.is_file() {
        // The app is a single page app, but its index.html uses relative asset
        // paths and a build-time <base> placeholder. Routes fall back to
        // index.html; anything that looks like a file is a genuine 404.
        if is_index || !path.rsplit('/').next().unwrap_or("").contains('.') {
            file = root.join(INDEX);
        }
    }

    let body = if file.file_name().and_then(|n| n.to_str()) == Some(INDEX) {
        // fill in the placeholders the real build replaces
        std::fs::read_to_string(&file).map(|html| {
            html.replace("<!-- PROD_BASE_TAG_HERE -->", "<base href=\"/\" />")
                .replace("<!-- PROD_BUILD_TAG_HERE -->", "codex-micro-adapter")
                .into_bytes()
        })
    } else {
        std::fs::read(&file)
    };

    match body {
        Ok(body) => {
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                mime_for(&file),
                body.len()
            );
            stream.write_all(head.as_bytes())?;
            stream.write_all(&body)?;
        }
        Err(_) => {
            let body = b"not found";
            let head = format!("HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            stream.write_all(head.as_bytes())?;
            stream.write_all(body)?;
        }
    }
    stream.flush()
}

/// `%20` and friends, which the bundle's chunk names never use but its routes might.
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&path[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "webp" => "image/webp",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "txt" | "md" => "text/plain; charset=utf-8",
        "bin" | "dat" | "pak" => "application/octet-stream",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_escaped_paths() {
        assert_eq!(percent_decode("assets/a%20b.js"), "assets/a b.js");
        assert_eq!(percent_decode("assets/plain.js"), "assets/plain.js");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn maps_mime_types() {
        assert_eq!(mime_for(Path::new("x/index.html")), "text/html; charset=utf-8");
        assert_eq!(mime_for(Path::new("x/app.js")), "text/javascript; charset=utf-8");
        assert_eq!(mime_for(Path::new("x/a.wasm")), "application/wasm");
    }

    #[test]
    fn fills_in_the_build_placeholders() {
        let dir = std::env::temp_dir().join("codex-micro-vendor-base-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(INDEX), "<html><!-- PROD_BASE_TAG_HERE --><body></body></html>").unwrap();
        let port = serve(dir.clone()).unwrap();

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"GET /settings/codex-micro HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut body = String::new();
        std::io::Read::read_to_string(&mut stream, &mut body).unwrap();
        assert!(body.contains("<base href=\"/\" />"), "base tag replaced: {body}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serves_index_for_unknown_routes() {
        let dir = std::env::temp_dir().join("codex-micro-vendor-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(INDEX), b"<html>ok</html>").unwrap();
        let port = serve(dir.clone()).unwrap();

        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"GET /settings/codex-micro HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut response = String::new();
        BufReader::new(stream).read_line(&mut response).unwrap();
        assert!(response.contains("200 OK"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
