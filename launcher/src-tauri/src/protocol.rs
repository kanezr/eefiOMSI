//! `omsi://localhost/…` (`http://omsi.localhost/…` on Windows): the content's pictures the page
//! shows - a map's `picture.jpg` from the OMSI folder, the content folder or an archive used in
//! place (`content/<path>`). Nothing else on the disk can be asked for.

use tauri::http::{header, Request, Response, StatusCode};
use tauri::{Runtime, UriSchemeContext, UriSchemeResponder};

pub fn handle<R: Runtime>(_ctx: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let path = request.uri().path().to_string();
    // (reading a picture out of a 5 GB archive must not hold the window's thread)
    std::thread::spawn(move || responder.respond(answer(&path)));
}

fn answer(path: &str) -> Response<Vec<u8>> {
    let path = percent_decode(path.trim_start_matches('/'));
    let found = if let Some(rel) = path.strip_prefix("content/") {
        omsi_launcher_api::content_picture(rel)
    } else {
        None
    };
    match found {
        Some(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime(&path))
            .header(header::CACHE_CONTROL, "max-age=300")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(bytes)
            .unwrap_or_default(),
        None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()).unwrap_or_default(),
    }
}

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_are_read() {
        assert_eq!(super::percent_decode("maps/Berlin%20Spandau/picture.jpg"), "maps/Berlin Spandau/picture.jpg");
        assert_eq!(super::percent_decode("a%"), "a%");
    }
}
