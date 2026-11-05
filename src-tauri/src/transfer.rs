//! Short-lived, explicitly started LAN transfer. Only the selected asset is exposed.
use crate::capture::{self, MediaAsset};
use serde::Serialize;
use std::{
    io::{Read, Write},
    net::{IpAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
const MAX_UPLOAD: usize = 40 * 1024 * 1024;
const LIFETIME: u64 = 600;
#[derive(Default)]
pub struct TransferState(pub Mutex<Option<Session>>);
pub struct Session {
    active: Arc<AtomicBool>,
    info: TransferInfo,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInfo {
    pub urls: Vec<String>,
    pub active_index: usize,
    pub qr_svg: String,
    pub expires_at: u64,
    pub asset_id: Option<String>,
}

fn header(name: &str, value: &str) -> String {
    format!("{name}: {value}\r\n")
}
fn respond(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    bytes: &[u8],
) -> std::io::Result<()> {
    write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n{}\r\n",bytes.len(),header("Content-Security-Policy","default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src 'self' blob:; media-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'"))?;
    stream.write_all(bytes)
}
struct Request {
    method: String,
    path: String,
    length: usize,
    origin: Option<String>,
    host: String,
}
fn parse(stream: &mut TcpStream) -> Result<Request, String> {
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while !raw.ends_with(b"\r\n\r\n") {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("请求头读取超时")?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|e| e.to_string())?;
        if raw.len() >= 16 * 1024 {
            return Err("请求头过大".into());
        }
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        raw.push(byte[0]);
    }
    let text = String::from_utf8(raw).map_err(|_| "请求头无效")?;
    let mut lines = text.split("\r\n");
    let first: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    if first.len() != 3 || first[2] != "HTTP/1.1" {
        return Err("无效的 HTTP 请求".into());
    }
    let mut req = Request {
        method: first[0].into(),
        path: first[1].into(),
        length: 0,
        origin: None,
        host: String::new(),
    };
    let mut seen_length = false;
    for line in lines.filter(|s| !s.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("请求头无效")?;
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "content-length" => {
                if seen_length {
                    return Err("重复长度".into());
                }
                seen_length = true;
                req.length = value.parse().map_err(|_| "长度无效")?;
            }
            "transfer-encoding" => return Err("不支持分块上传".into()),
            "origin" => {
                if req.origin.is_some() {
                    return Err("重复 Origin".into());
                }
                req.origin = Some(value.into());
            }
            "host" => {
                if !req.host.is_empty() {
                    return Err("重复 Host".into());
                }
                req.host = value.into();
            }
            _ => {}
        }
    }
    if req.length > MAX_UPLOAD {
        return Err("文件不能超过 40 MB".into());
    }
    Ok(req)
}
fn serve(
    mut stream: TcpStream,
    app: AppHandle,
    prefix: String,
    selected: Option<MediaAsset>,
    active: Arc<AtomicBool>,
    hosts: Vec<String>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let req = match parse(&mut stream) {
        Ok(v) => v,
        Err(e) => {
            let _ = respond(
                &mut stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                e.as_bytes(),
            );
            return;
        }
    };
    if !active.load(Ordering::SeqCst)
        || !hosts.contains(&req.host)
        || req
            .origin
            .as_ref()
            .is_some_and(|o| o != &format!("http://{}", req.host))
    {
        let _ = respond(
            &mut stream,
            "403 Forbidden",
            "text/plain",
            b"Session unavailable",
        );
        return;
    }
    let route = req.path.strip_prefix(&prefix).unwrap_or("invalid");
    match (req.method.as_str(), route) {
        ("GET", "") | ("GET", "/") => {
            let page = include_str!("../../src/capture/transfer.html").replace(
                "__DOWNLOAD__",
                if selected.is_some() { "" } else { "hidden" },
            );
            let _ = respond(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                page.as_bytes(),
            );
        }
        ("GET", "/download") => {
            if let Some(item) = selected {
                if let Ok(mut file) = std::fs::File::open(&item.path) {
                    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                    let mp4 = item.kind == "video"
                        && std::path::Path::new(&item.path)
                            .extension()
                            .is_some_and(|s| s == "mp4");
                    let ext = if item.kind == "image" {
                        "png"
                    } else if mp4 {
                        "mp4"
                    } else {
                        "mov"
                    };
                    let mime = if item.kind == "image" {
                        "image/png"
                    } else if mp4 {
                        "video/mp4"
                    } else {
                        "video/quicktime"
                    };
                    let _=write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {size}\r\nContent-Disposition: attachment; filename=\"PasteNext.{ext}\"\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n");
                    let mut buffer = [0u8; 65536];
                    while active.load(Ordering::SeqCst) {
                        match file.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                if stream.write_all(&buffer[..n]).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    return;
                }
            }
            let _ = respond(
                &mut stream,
                "404 Not Found",
                "text/plain",
                b"No shared file",
            );
        }
        ("POST", "/upload") => {
            if req.length == 0 {
                let _ = respond(&mut stream, "400 Bad Request", "text/plain", b"Empty image");
                return;
            }
            let deadline = Instant::now() + Duration::from_secs(120);
            let mut bytes = vec![0u8; req.length];
            for chunk in bytes.chunks_mut(65536) {
                let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                    return;
                };
                let _ = stream.set_read_timeout(Some(remaining.min(Duration::from_secs(10))));
                if !active.load(Ordering::SeqCst) || stream.read_exact(chunk).is_err() {
                    return;
                }
            }
            if !active.load(Ordering::SeqCst) {
                return;
            }
            match capture::import_image_bytes(&app, &bytes, "手机传图") {
                Ok(item) => {
                    let _ = app.emit("transfer-received", &item);
                    let _ = respond(&mut stream, "200 OK", "application/json", b"{\"ok\":true}");
                }
                Err(e) => {
                    let _ = respond(
                        &mut stream,
                        "400 Bad Request",
                        "text/plain; charset=utf-8",
                        e.as_bytes(),
                    );
                }
            }
        }
        _ => {
            let _ = respond(&mut stream, "404 Not Found", "text/plain", b"Not found");
        }
    }
}

#[tauri::command]
pub fn transfer_start(app: AppHandle, id: Option<String>) -> Result<TransferInfo, String> {
    let selected = id
        .as_deref()
        .map(|id| capture::asset(&app, id))
        .transpose()?;
    let mut interfaces = if_addrs::get_if_addrs().map_err(|e| e.to_string())?;
    interfaces.sort_by_key(|i| {
        if i.name.starts_with("en") || i.name.starts_with("eth") || i.name.starts_with("wl") {
            0
        } else {
            1
        }
    });
    let mut addresses = Vec::new();
    for interface in interfaces {
        if let IpAddr::V4(ip) = interface.ip() {
            if ip.is_private() && !ip.is_loopback() && !addresses.contains(&ip) {
                addresses.push(ip);
            }
        }
    }
    if addresses.is_empty() {
        return Err("未找到局域网地址，请将电脑和手机连接到同一 Wi-Fi".into());
    }
    let listener = TcpListener::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let token = capture::new_id();
    let prefix = format!("/s/{token}");
    let hosts: Vec<_> = addresses.iter().map(|ip| format!("{ip}:{port}")).collect();
    let urls: Vec<_> = hosts
        .iter()
        .map(|host| format!("http://{host}{prefix}/"))
        .collect();
    let qr = qrcode::QrCode::new(urls[0].as_bytes())
        .map_err(|e| e.to_string())?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .build();
    let active = Arc::new(AtomicBool::new(true));
    let flag = active.clone();
    let info = TransferInfo {
        urls,
        active_index: 0,
        qr_svg: qr,
        expires_at: crate::license::now_ms() as u64 + LIFETIME * 1000,
        asset_id: id,
    };
    *app.state::<TransferState>().0.lock().unwrap() = Some(Session {
        active,
        info: info.clone(),
    });
    std::thread::spawn(move || {
        let start = Instant::now();
        let connections = Arc::new(AtomicUsize::new(0));
        while flag.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(LIFETIME) {
            match listener.accept() {
                Ok((stream, _)) => {
                    if connections.load(Ordering::SeqCst) >= 3 {
                        drop(stream);
                        continue;
                    }
                    connections.fetch_add(1, Ordering::SeqCst);
                    let (a, p, s, f, h, c) = (
                        app.clone(),
                        prefix.clone(),
                        selected.clone(),
                        flag.clone(),
                        hosts.clone(),
                        connections.clone(),
                    );
                    std::thread::spawn(move || {
                        serve(stream, a, p, s, f, h);
                        c.fetch_sub(1, Ordering::SeqCst);
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(_) => break,
            }
        }
        flag.store(false, Ordering::SeqCst);
        let _ = app.emit("transfer-ended", ());
    });
    Ok(info)
}
#[tauri::command]
pub fn transfer_stop(app: AppHandle) {
    app.state::<TransferState>().0.lock().unwrap().take();
}
#[tauri::command]
pub fn transfer_status(app: AppHandle) -> Option<TransferInfo> {
    app.state::<TransferState>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .filter(|s| s.active.load(Ordering::SeqCst))
        .map(|s| s.info.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(raw: &[u8]) -> Result<Request, String> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.write_all(raw).unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        parse(&mut server)
    }
    #[test]
    fn accepts_bounded_image_upload() {
        let r=request(b"POST /s/token/upload HTTP/1.1\r\nHost: 192.168.1.2:1234\r\nContent-Length: 12\r\n\r\n").unwrap();
        assert_eq!(r.length, 12);
    }
    #[test]
    fn rejects_ambiguous_lengths_and_chunked() {
        for raw in [
            b"POST / HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\n".as_slice(),
            b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
        ] {
            assert!(request(raw).is_err());
        }
    }
    #[test]
    fn rejects_oversized_upload_before_allocation() {
        assert!(request(b"POST / HTTP/1.1\r\nContent-Length: 999999999\r\n\r\n").is_err());
    }
}

#[tauri::command]
pub fn transfer_address(app: AppHandle, index: usize) -> Result<TransferInfo, String> {
    let state = app.state::<TransferState>();
    let mut current = state.0.lock().unwrap();
    let session = current
        .as_mut()
        .filter(|s| s.active.load(Ordering::SeqCst))
        .ok_or("分享已结束，请重新开启")?;
    let url = session.info.urls.get(index).ok_or("网卡地址无效")?;
    session.info.qr_svg = qrcode::QrCode::new(url.as_bytes())
        .map_err(|e| e.to_string())?
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .build();
    session.info.active_index = index;
    Ok(session.info.clone())
}
