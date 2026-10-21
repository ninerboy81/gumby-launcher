//! HTTP / HTTPS through Windows' own WinHTTP (system proxy settings, system certificates; no extra library): the
//! terror zone feed, problem reports and update downloads. Redirects are never followed by WinHTTP; the caller sees the
//! 3xx and its Location.
#![allow(non_snake_case)]

type H = *mut std::ffi::c_void;

#[cfg(windows)]
#[link(name = "winhttp")]
extern "system" {
    fn WinHttpOpen(agent: *const u16, access: u32, proxy: *const u16, bypass: *const u16, flags: u32) -> H;
    fn WinHttpConnect(session: H, server: *const u16, port: u16, reserved: u32) -> H;
    fn WinHttpOpenRequest(connect: H, verb: *const u16, object: *const u16, version: *const u16, referrer: *const u16, accept: *const *const u16, flags: u32) -> H;
    fn WinHttpSetTimeouts(h: H, resolve: i32, connect: i32, send: i32, receive: i32) -> i32;
    fn WinHttpSetOption(h: H, option: u32, buffer: *const std::ffi::c_void, len: u32) -> i32;
    fn WinHttpAddRequestHeaders(h: H, headers: *const u16, len: u32, modifiers: u32) -> i32;
    fn WinHttpSendRequest(h: H, headers: *const u16, hlen: u32, optional: *const std::ffi::c_void, olen: u32, total: u32, context: usize) -> i32;
    fn WinHttpReceiveResponse(h: H, reserved: *mut std::ffi::c_void) -> i32;
    fn WinHttpQueryHeaders(h: H, level: u32, name: *const u16, buffer: *mut std::ffi::c_void, len: *mut u32, index: *mut u32) -> i32;
    fn WinHttpQueryDataAvailable(h: H, available: *mut u32) -> i32;
    fn WinHttpReadData(h: H, buffer: *mut std::ffi::c_void, to_read: u32, read: *mut u32) -> i32;
    fn WinHttpCloseHandle(h: H) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" { fn GetLastError() -> u32; }

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

struct Handle(H);
impl Drop for Handle { fn drop(&mut self) { #[cfg(windows)] unsafe { if !self.0.is_null() { WinHttpCloseHandle(self.0); } } } }

pub struct Response { pub status: u32, pub location: Option<String>, pub body: Vec<u8> }

#[derive(Debug, Clone)]
pub struct HttpError(pub String);
impl std::fmt::Display for HttpError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { f.write_str(&self.0) } }

/// (https?, host, port, path+query) of an http(s) URL.
pub fn split_url(url: &str) -> Result<(bool, String, u16, String), HttpError> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") { (true, r) } else if let Some(r) = url.strip_prefix("http://") { (false, r) }
        else { return Err(HttpError(format!("unknown url type: {}", url))); };
    let (hostport, path) = match rest.find('/') { Some(i) => (&rest[..i], &rest[i..]), None => (rest, "/") };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h.to_string(), p.parse::<u16>().map_err(|_| HttpError("bad port".into()))?),
        _ => (hostport.to_string(), if secure { 443 } else { 80 }),
    };
    if host.is_empty() { return Err(HttpError("no host given".into())); }
    Ok((secure, host, port, path.to_string()))
}

fn err(what: &str) -> HttpError {
    let code = { #[cfg(windows)] unsafe { GetLastError() } #[cfg(not(windows))] { 0 } };
    let text = match code {
        12002 => "timed out".to_string(),
        12007 => "the server name could not be resolved".to_string(),
        12029 | 12030 => "could not connect to the server".to_string(),
        12175 => "secure connection failed".to_string(),
        c => format!("network error {}", c),
    };
    HttpError(format!("<urlopen error {} ({})>", text, what))
}

/// One request. `limit` caps the body (Err beyond it); `progress(bytes so far)` as the body arrives.
pub fn request(method: &str, url: &str, headers: &[(&str, &str)], body: Option<&[u8]>, timeout_ms: u32, limit: usize,
               mut progress: Option<&mut dyn FnMut(usize)>) -> Result<Response, HttpError> {
    #[cfg(not(windows))]
    { let _ = (method, url, headers, body, timeout_ms, limit, &mut progress); return Err(HttpError("windows only".into())); }
    #[cfg(windows)]
    unsafe {
        let (secure, host, port, path) = split_url(url)?;
        let agent = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("User-Agent")).map(|(_, v)| *v).unwrap_or("GumbyLauncher");
        let session = Handle(WinHttpOpen(wide(agent).as_ptr(), 4, std::ptr::null(), std::ptr::null(), 0));
        if session.0.is_null() { return Err(err("open")); }
        let t = timeout_ms as i32;
        WinHttpSetTimeouts(session.0, t, t, t, t);
        let connect = Handle(WinHttpConnect(session.0, wide(&host).as_ptr(), port, 0));
        if connect.0.is_null() { return Err(err("connect")); }
        let req = Handle(WinHttpOpenRequest(connect.0, wide(method).as_ptr(), wide(&path).as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
            if secure { 0x0080_0000 } else { 0 }));
        if req.0.is_null() { return Err(err("request")); }
        let never: u32 = 0;
        WinHttpSetOption(req.0, 88, &never as *const u32 as *const std::ffi::c_void, 4);
        let mut text = String::new();
        for (k, v) in headers { text.push_str(&format!("{}: {}\r\n", k, v)); }
        if !text.is_empty() {
            let w: Vec<u16> = text.encode_utf16().collect();
            WinHttpAddRequestHeaders(req.0, w.as_ptr(), w.len() as u32, 0x2000_0000 | 0x8000_0000);
        }
        let (bp, bl) = match body { Some(b) => (b.as_ptr() as *const std::ffi::c_void, b.len() as u32), None => (std::ptr::null(), 0) };
        if WinHttpSendRequest(req.0, std::ptr::null(), 0, bp, bl, bl, 0) == 0 { return Err(err("send")); }
        if WinHttpReceiveResponse(req.0, std::ptr::null_mut()) == 0 { return Err(err("receive")); }
        let mut status: u32 = 0;
        let mut len: u32 = 4;
        WinHttpQueryHeaders(req.0, 19 | 0x2000_0000, std::ptr::null(), &mut status as *mut u32 as *mut std::ffi::c_void, &mut len, std::ptr::null_mut());
        let mut location = None;
        let mut buf = vec![0u16; 4096];
        let mut blen: u32 = (buf.len() * 2) as u32;
        if WinHttpQueryHeaders(req.0, 33, std::ptr::null(), buf.as_mut_ptr() as *mut std::ffi::c_void, &mut blen, std::ptr::null_mut()) != 0 {
            location = Some(String::from_utf16_lossy(&buf[..(blen / 2) as usize]));
        }
        let mut out = Vec::new();
        loop {
            let mut avail: u32 = 0;
            if WinHttpQueryDataAvailable(req.0, &mut avail) == 0 { return Err(err("read")); }
            if avail == 0 { break; }
            let mut chunk = vec![0u8; avail.min(1 << 20) as usize];
            let mut got: u32 = 0;
            if WinHttpReadData(req.0, chunk.as_mut_ptr() as *mut std::ffi::c_void, chunk.len() as u32, &mut got) == 0 { return Err(err("read")); }
            if got == 0 { break; }
            out.extend_from_slice(&chunk[..got as usize]);
            if out.len() > limit { return Err(HttpError("download larger than allowed".into())); }
            if let Some(p) = progress.as_mut() { p(out.len()); }
        }
        Ok(Response { status, location, body: out })
    }
}

/// The error for an HTTP status that is not 2xx.
pub fn status_error(status: u32) -> HttpError {
    let reason = match status {
        400 => "Bad Request", 401 => "Unauthorized", 403 => "Forbidden", 404 => "Not Found", 413 => "Request Entity Too Large",
        429 => "Too Many Requests", 500 => "Internal Server Error", 502 => "Bad Gateway", 503 => "Service Unavailable", _ => "",
    };
    HttpError(format!("HTTP Error {}: {}", status, reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls() {
        assert_eq!(split_url("http://144.217.29.142:8190/zones").unwrap(), (false, "144.217.29.142".into(), 8190, "/zones".into()));
        assert_eq!(split_url("https://api.github.com/repos/a/b").unwrap(), (true, "api.github.com".into(), 443, "/repos/a/b".into()));
        assert_eq!(split_url("http://host").unwrap().3, "/");
        assert!(split_url("ftp://x").is_err());
    }
}
