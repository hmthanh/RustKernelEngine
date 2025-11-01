use anyhow::{anyhow, Result};
use httparse;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub version: u8,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Parse HTTP request from buffer
    pub fn parse(buf: &[u8]) -> Result<Self> {
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        let status = req
            .parse(buf)
            .map_err(|e| anyhow!("Failed to parse HTTP request: {:?}", e))?;

        if status.is_partial() {
            return Err(anyhow!("Incomplete HTTP request"));
        }

        let method = req
            .method
            .ok_or_else(|| anyhow!("Missing method"))?
            .to_string();
        let path = req
            .path
            .ok_or_else(|| anyhow!("Missing path"))?
            .to_string();
        let version = req.version.ok_or_else(|| anyhow!("Missing version"))?;

        let mut header_map = HashMap::new();
        for header in req.headers {
            if !header.name.is_empty() {
                let name = header.name.to_lowercase();
                let value = String::from_utf8_lossy(header.value).to_string();
                header_map.insert(name, value);
            }
        }

        // Calculate body offset
        let body_offset = status.unwrap();
        let body = if body_offset < buf.len() {
            buf[body_offset..].to_vec()
        } else {
            Vec::new()
        };

        Ok(HttpRequest {
            method,
            path,
            version,
            headers: header_map,
            body,
        })
    }

    /// Get header value by name (case-insensitive)
    pub fn get_header(&self, name: &str) -> Option<&String> {
        self.headers.get(&name.to_lowercase())
    }

    /// Check if this is a keep-alive connection
    pub fn is_keep_alive(&self) -> bool {
        if let Some(connection) = self.get_header("connection") {
            connection.to_lowercase() == "keep-alive"
        } else {
            // HTTP/1.1 defaults to keep-alive
            self.version >= 1
        }
    }

    /// Get content length from headers
    pub fn content_length(&self) -> Option<usize> {
        self.get_header("content-length")
            .and_then(|v| v.parse().ok())
    }

    /// Check if request has body
    pub fn has_body(&self) -> bool {
        self.content_length().unwrap_or(0) > 0 || !self.body.is_empty()
    }

    /// Parse query parameters from path
    pub fn query_params(&self) -> HashMap<String, String> {
        let mut params = HashMap::new();
        
        if let Some(query_start) = self.path.find('?') {
            let query = &self.path[query_start + 1..];
            for pair in query.split('&') {
                if let Some(eq_pos) = pair.find('=') {
                    let key = &pair[..eq_pos];
                    let value = &pair[eq_pos + 1..];
                    params.insert(
                        urlencoding::decode(key).unwrap_or_default().to_string(),
                        urlencoding::decode(value).unwrap_or_default().to_string(),
                    );
                }
            }
        }
        
        params
    }

    /// Get the path without query parameters
    pub fn clean_path(&self) -> &str {
        if let Some(query_start) = self.path.find('?') {
            &self.path[..query_start]
        } else {
            &self.path
        }
    }
}

/// Simple URL decoding helper (inline implementation to avoid dependency)
mod urlencoding {
    use std::borrow::Cow;

    pub fn decode(s: &str) -> Result<Cow<str>, std::fmt::Error> {
        let mut result = String::new();
        let mut chars = s.chars();
        
        while let Some(ch) = chars.next() {
            match ch {
                '%' => {
                    let mut hex = String::new();
                    hex.push(chars.next().ok_or(std::fmt::Error)?);
                    hex.push(chars.next().ok_or(std::fmt::Error)?);
                    
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        result.push(byte as char);
                    } else {
                        return Err(std::fmt::Error);
                    }
                }
                '+' => result.push(' '),
                _ => result.push(ch),
            }
        }
        
        if result == s {
            Ok(Cow::Borrowed(s))
        } else {
            Ok(Cow::Owned(result))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_request() {
        let raw = b"GET /index.html HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let req = HttpRequest::parse(raw).unwrap();
        
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/index.html");
        assert_eq!(req.get_header("host"), Some(&"example.com".to_string()));
    }

    #[test]
    fn test_parse_with_query() {
        let raw = b"GET /api/data?key=value&foo=bar HTTP/1.1\r\n\r\n";
        let req = HttpRequest::parse(raw).unwrap();
        
        let params = req.query_params();
        assert_eq!(params.get("key"), Some(&"value".to_string()));
        assert_eq!(params.get("foo"), Some(&"bar".to_string()));
    }
}
