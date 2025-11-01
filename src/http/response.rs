use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Create a new HTTP response
    pub fn new(status_code: u16) -> Self {
        let status_text = status_text_for_code(status_code);
        
        let mut headers = HashMap::new();
        headers.insert("Server".to_string(), "RustKernelEngine/0.1".to_string());
        headers.insert("Connection".to_string(), "keep-alive".to_string());
        
        Self {
            status_code,
            status_text,
            headers,
            body: Vec::new(),
        }
    }

    /// Create 200 OK response
    pub fn ok() -> Self {
        Self::new(200)
    }

    /// Create 404 Not Found response
    pub fn not_found() -> Self {
        let mut resp = Self::new(404);
        resp.set_body(b"404 Not Found");
        resp
    }

    /// Create 500 Internal Server Error response
    pub fn internal_error() -> Self {
        let mut resp = Self::new(500);
        resp.set_body(b"500 Internal Server Error");
        resp
    }

    /// Create 400 Bad Request response
    pub fn bad_request() -> Self {
        let mut resp = Self::new(400);
        resp.set_body(b"400 Bad Request");
        resp
    }

    /// Set response body
    pub fn set_body(&mut self, body: &[u8]) {
        self.body = body.to_vec();
        self.headers.insert(
            "Content-Length".to_string(),
            self.body.len().to_string(),
        );
    }

    /// Set JSON body
    pub fn json<T: serde::Serialize>(mut self, data: &T) -> Self {
        if let Ok(json) = serde_json::to_vec(data) {
            self.body = json;
            self.headers.insert("Content-Type".to_string(), "application/json".to_string());
            self.headers.insert("Content-Length".to_string(), self.body.len().to_string());
        }
        self
    }

    /// Set HTML body
    pub fn html(mut self, html: &str) -> Self {
        self.body = html.as_bytes().to_vec();
        self.headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
        self.headers.insert("Content-Length".to_string(), self.body.len().to_string());
        self
    }

    /// Set text body
    pub fn text(mut self, text: &str) -> Self {
        self.body = text.as_bytes().to_vec();
        self.headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        self.headers.insert("Content-Length".to_string(), self.body.len().to_string());
        self
    }

    /// Add a header
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    /// Convert response to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut response = Vec::new();
        
        // Status line
        let status_line = format!(
            "HTTP/1.1 {} {}\r\n",
            self.status_code, self.status_text
        );
        response.extend_from_slice(status_line.as_bytes());
        
        // Headers
        for (name, value) in &self.headers {
            let header = format!("{}: {}\r\n", name, value);
            response.extend_from_slice(header.as_bytes());
        }
        
        // Empty line
        response.extend_from_slice(b"\r\n");
        
        // Body
        response.extend_from_slice(&self.body);
        
        response
    }

    /// Get response size in bytes
    pub fn size(&self) -> usize {
        self.to_bytes().len()
    }
}

impl fmt::Display for HttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HTTP/1.1 {} {} ({} bytes)",
            self.status_code,
            self.status_text,
            self.body.len()
        )
    }
}

fn status_text_for_code(code: u16) -> String {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        414 => "URI Too Long",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "Unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ok_response() {
        let resp = HttpResponse::ok().text("Hello, World!");
        let bytes = resp.to_bytes();
        let text = String::from_utf8_lossy(&bytes);
        
        assert!(text.contains("HTTP/1.1 200 OK"));
        assert!(text.contains("Hello, World!"));
    }

    #[test]
    fn test_json_response() {
        use serde_json::json;
        
        let data = json!({
            "status": "success",
            "data": [1, 2, 3]
        });
        
        let resp = HttpResponse::ok().json(&data);
        let bytes = resp.to_bytes();
        let text = String::from_utf8_lossy(&bytes);
        
        assert!(text.contains("application/json"));
        assert!(text.contains("success"));
    }
}
