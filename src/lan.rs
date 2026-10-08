use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::process::CommandExt;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::playlist::{self, Kind, Order};

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
}

struct Share {
    token: String,
    pc_dir: Option<PathBuf>,
    phone_dir: Option<PathBuf>,
    playing: Option<PathBuf>,
    nav: u64,
    order: Order,
}

pub struct Server {
    pub url: String,
    pub port: u16,
    pub qr: Vec<bool>,
    pub qr_width: usize,
    pub firewall_ok: bool,
    stop: Arc<AtomicBool>,
    jobs: Arc<Mutex<Vec<u32>>>,
    share: Arc<Mutex<Share>>,
}

impl Server {
    pub fn stop(self) {
        self.shutdown();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Server {
    fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], self.port)),
            Duration::from_millis(300),
        );
        if let Ok(mut jobs) = self.jobs.lock() {
            for pid in jobs.drain(..) {
                let _ = Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/T", "/F"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(0x0800_0000)
                    .status();
            }
        }
    }

    pub fn publish(&self, dir: Option<PathBuf>, playing: Option<PathBuf>) {
        if let Ok(mut share) = self.share.lock() {
            share.pc_dir = dir;
            share.playing = playing;
        }
    }
}

pub fn new_token() -> String {
    let mut bytes = [0u8; 16];
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            0x0000_0002,
        )
    };
    if status != 0 {
        let ticks = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&ticks.to_le_bytes());
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn qr_modules(text: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let mut modules = Vec::with_capacity(width * width);
    for y in 0..width {
        for x in 0..width {
            modules.push(code[(x, y)] == qrcode::Color::Dark);
        }
    }
    Some((width, modules))
}

pub fn start(token: String, preferred_port: u16) -> Result<Server, String> {
    let listener = bind_listener(preferred_port).map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let ip = lan_ipv4().ok_or("This PC does not have a Wi-Fi address yet")?;
    let url = format!("http://{ip}:{port}/s/{token}");
    let (qr_width, qr) = qr_modules(&url).unwrap_or((0, Vec::new()));
    let firewall_ok = allow_private_port(port);
    let stop = Arc::new(AtomicBool::new(false));
    let jobs = Arc::new(Mutex::new(Vec::new()));
    let share = Arc::new(Mutex::new(Share {
        token,
        pc_dir: None,
        phone_dir: None,
        playing: None,
        nav: 0,
        order: Order::Newest,
    }));
    let stop_thread = stop.clone();
    let jobs_thread = jobs.clone();
    let share_thread = share.clone();
    thread::spawn(move || accept_loop(listener, stop_thread, jobs_thread, share_thread));
    Ok(Server {
        url,
        port,
        qr,
        qr_width,
        firewall_ok,
        stop,
        jobs,
        share,
    })
}

fn bind_listener(preferred: u16) -> std::io::Result<TcpListener> {
    if preferred != 0 {
        if let Ok(listener) = TcpListener::bind(("0.0.0.0", preferred)) {
            return Ok(listener);
        }
    }
    if preferred != 47832 {
        if let Ok(listener) = TcpListener::bind(("0.0.0.0", 47832)) {
            return Ok(listener);
        }
    }
    TcpListener::bind(("0.0.0.0", 0))
}

fn lan_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(addr) => {
            let ip = *addr.ip();
            if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
                None
            } else {
                Some(ip)
            }
        }
        SocketAddr::V6(_) => None,
    }
}

fn allow_private_port(port: u16) -> bool {
    let name = "Better Media Player Stream";
    let _ = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", &format!("name={name}")])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .status();
    Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name={name}"),
            "dir=in",
            "action=allow",
            "protocol=TCP",
            &format!("localport={port}"),
            "profile=private",
            "remoteip=localsubnet",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn accept_loop(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    jobs: Arc<Mutex<Vec<u32>>>,
    share: Arc<Mutex<Share>>,
) {
    let _ = listener.set_nonblocking(false);
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let jobs = jobs.clone();
                let share = share.clone();
                let stop = stop.clone();
                thread::spawn(move || handle(stream, &share, &jobs, &stop));
            }
            Err(_) => break,
        }
    }
}

fn handle(mut stream: TcpStream, share: &Mutex<Share>, jobs: &Mutex<Vec<u32>>, stop: &AtomicBool) {
    if stop.load(Ordering::Relaxed) {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(8)));
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(Some(Duration::from_secs(120)));
    if let Some((content_type, body)) = favicon(&request.path) {
        let _ = write_status(&mut stream, 200, content_type, body);
        return;
    }
    let reply = {
        let Ok(mut share) = share.lock() else {
            return;
        };
        if let Some(order) = order_query(&request.path) {
            share.order = order;
        }
        let route = match route_of(&request.path, &share.token) {
            Some(route) => route,
            None => {
                drop(share);
                let _ = write_status(&mut stream, 404, "text/plain; charset=utf-8", b"Not found");
                return;
            }
        };
        match route {
            Route::Page => Reply::Page,
            Route::List => Reply::Json(list_body(&share)),
            Route::Up => {
                move_up(&mut share);
                Reply::Json(list_body(&share))
            }
            Route::Open(index) => {
                open_index(&mut share, index);
                Reply::Json(list_body(&share))
            }
            Route::Next => {
                move_next(&mut share);
                Reply::Json(list_body(&share))
            }
            Route::Media(index) => match file_at(&share, index) {
                Some(item) => Reply::File { item, audio: false, head: request.head },
                None => Reply::Missing,
            },
            Route::Audio(index) => match file_at(&share, index) {
                Some(item) => Reply::File { item, audio: true, head: request.head },
                None => Reply::Missing,
            },
        }
    };
    match reply {
        Reply::Page => {
            let _ = write_status(&mut stream, 200, "text/html; charset=utf-8", page().as_bytes());
        }
        Reply::Json(body) => {
            let _ = write_status(&mut stream, 200, "application/json; charset=utf-8", body.as_bytes());
        }
        Reply::Missing => {
            let _ = write_status(&mut stream, 404, "text/plain; charset=utf-8", b"Not found");
        }
        Reply::File { item, audio, head } => {
            if head {
                let _ = write_head_only(&mut stream, &item, audio);
            } else {
                serve_item(&mut stream, &item, audio, request.range, jobs);
            }
        }
    }
}

enum Route {
    Page,
    List,
    Up,
    Open(usize),
    Next,
    Media(usize),
    Audio(usize),
}

enum Reply {
    Page,
    Json(String),
    Missing,
    File { item: Item, audio: bool, head: bool },
}

fn favicon(path: &str) -> Option<(&'static str, &'static [u8])> {
    match path.split('?').next().unwrap_or(path) {
        "/favicon.ico" => Some(("image/x-icon", include_bytes!("../assets/app.ico"))),
        "/favicon.png" => Some(("image/png", include_bytes!("../assets/favicon.png"))),
        _ => None,
    }
}

fn route_of(path: &str, token: &str) -> Option<Route> {
    let path = path.split('?').next().unwrap_or(path);
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    if parts.next() != Some("s") {
        return None;
    }
    let given = parts.next()?;
    if given != token {
        return None;
    }
    match parts.next() {
        None => Some(Route::Page),
        Some("list") if parts.next().is_none() => Some(Route::List),
        Some("up") if parts.next().is_none() => Some(Route::Up),
        Some("next") if parts.next().is_none() => Some(Route::Next),
        Some("open") => parts.next()?.parse().ok().filter(|_| parts.next().is_none()).map(Route::Open),
        Some("m") => parts.next()?.parse().ok().filter(|_| parts.next().is_none()).map(Route::Media),
        Some("a") => parts.next()?.parse().ok().filter(|_| parts.next().is_none()).map(Route::Audio),
        _ => None,
    }
}

struct Request {
    head: bool,
    path: String,
    range: Option<(u64, Option<u64>)>,
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 2048];
    while buf.len() < 16_384 {
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|mark| mark == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let start = lines.next()?;
    let mut words = start.split_whitespace();
    let method = words.next()?;
    if method != "GET" && method != "HEAD" {
        return None;
    }
    let path = decode_path(words.next()?);
    let mut range = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("range") {
            range = parse_range(value.trim());
        }
    }
    Some(Request {
        head: method == "HEAD",
        path,
        range,
    })
}

fn decode_path(raw: &str) -> String {
    let mut out = String::new();
    let bytes = raw.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""), 16) {
                out.push(byte as char);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn parse_range(value: &str) -> Option<(u64, Option<u64>)> {
    let value = value.strip_prefix("bytes=")?;
    let spec = value.split(',').next()?.trim();
    let (start, end) = spec.split_once('-')?;
    if start.is_empty() {
        return None;
    }
    let start = start.parse().ok()?;
    let end = if end.is_empty() { None } else { Some(end.parse().ok()?) };
    Some((start, end))
}

fn serve_item(
    stream: &mut TcpStream,
    item: &Item,
    audio_only: bool,
    range: Option<(u64, Option<u64>)>,
    jobs: &Mutex<Vec<u32>>,
) {
    if audio_only {
        if item.kind == Kind::Audio {
            if let Some(kind) = direct_type(&item.path) {
                let _ = send_file(stream, &item.path, kind, range);
                return;
            }
        }
        if item.kind == Kind::Image {
            let _ = write_status(stream, 415, "text/plain; charset=utf-8", b"Images have no audio track");
            return;
        }
        let _ = transcode_audio(stream, &item.path, range, jobs);
        return;
    }
    if let Some(kind) = direct_type(&item.path) {
        let _ = send_file(stream, &item.path, kind, range);
        return;
    }
    let _ = transcode(stream, &item.path, false, jobs);
}

fn direct_type(path: &Path) -> Option<&'static str> {
    match playlist::extension_of(path).as_deref() {
        Some("mp4" | "m4v") => Some("video/mp4"),
        Some("webm") => Some("video/webm"),
        Some("mp3") => Some("audio/mpeg"),
        Some("m4a" | "aac") => Some("audio/mp4"),
        Some("wav" | "wave") => Some("audio/wav"),
        Some("ogg" | "oga" | "opus") => Some("audio/ogg"),
        Some("flac") => Some("audio/flac"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("png") => Some("image/png"),
        Some("gif") => Some("image/gif"),
        Some("bmp") => Some("image/bmp"),
        Some("webp") => Some("image/webp"),
        _ => None,
    }
}

fn send_file(
    stream: &mut TcpStream,
    path: &Path,
    content_type: &str,
    range: Option<(u64, Option<u64>)>,
) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len == 0 {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nAccept-Ranges: bytes\r\nContent-Length: 0\r\nCache-Control: private\r\nConnection: close\r\n\r\n"
        );
        return stream.write_all(header.as_bytes());
    }
    let (status, start, end) = match range {
        Some((start, end)) if start >= len => {
            let header = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(header.as_bytes())?;
            return Ok(());
        }
        Some((start, end)) => {
            let end = end.unwrap_or(len - 1).min(len - 1);
            if end < start {
                (416, 0, 0)
            } else {
                (206, start, end)
            }
        }
        None => (200, 0, len.saturating_sub(1)),
    };
    if status == 416 || len == 0 {
        let header = format!(
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{len}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return stream.write_all(header.as_bytes());
    }
    let count = end - start + 1;
    let header = if status == 206 {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Type: {content_type}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{len}\r\nContent-Length: {count}\r\nCache-Control: private\r\nConnection: close\r\n\r\n"
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nCache-Control: private\r\nConnection: close\r\n\r\n"
        )
    };
    stream.write_all(header.as_bytes())?;
    file.seek(SeekFrom::Start(start))?;
    let mut remaining = count;
    let mut buf = [0u8; 64 * 1024];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = file.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        stream.write_all(&buf[..n])?;
        remaining -= n as u64;
    }
    Ok(())
}

fn write_head_only(stream: &mut TcpStream, item: &Item, audio_only: bool) -> std::io::Result<()> {
    if audio_only && item.kind != Kind::Audio {
        if let Some(len) = audio_size(&item.path) {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nCache-Control: private\r\nConnection: close\r\n\r\n"
            );
            return stream.write_all(header.as_bytes());
        }
        let header = "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n";
        return stream.write_all(header.as_bytes());
    }
    if let Some(kind) = direct_type(&item.path) {
        let len = item.bytes;
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
        );
        return stream.write_all(header.as_bytes());
    }
    let header = "HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n";
    stream.write_all(header.as_bytes())
}

const AUDIO_BYTES_PER_SEC: f64 = 16_000.0;

fn audio_size(path: &Path) -> Option<u64> {
    let secs = duration_secs(path)?;
    Some((secs * AUDIO_BYTES_PER_SEC).round().max(1.0) as u64)
}

fn duration_secs(path: &Path) -> Option<f64> {
    let meta = std::fs::metadata(path).ok()?;
    let len = meta.len();
    let cache = duration_cache();
    if let Ok(cache) = cache.lock() {
        if let Some((cached_len, secs)) = cache.get(path) {
            if *cached_len == len && *secs > 0.0 {
                return Some(*secs);
            }
        }
    }
    let secs = probe_duration(path)?;
    if !secs.is_finite() || secs <= 0.0 {
        return None;
    }
    if let Ok(mut cache) = cache.lock() {
        cache.insert(path.to_path_buf(), (len, secs));
    }
    Some(secs)
}

fn duration_cache() -> &'static Mutex<HashMap<PathBuf, (u64, f64)>> {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<PathBuf, (u64, f64)>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn probe_duration(path: &Path) -> Option<f64> {
    let probed = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    if let Ok(secs) = String::from_utf8_lossy(&probed.stdout).trim().parse::<f64>() {
        if secs.is_finite() && secs > 0.0 {
            return Some(secs);
        }
    }
    let listed = Command::new("ffmpeg")
        .args(["-hide_banner", "-i"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&listed.stderr);
    let stamp = text.split("Duration:").nth(1)?.trim().split([',', ' ']).next()?;
    parse_stamp(stamp)
}

fn parse_stamp(stamp: &str) -> Option<f64> {
    let mut parts = stamp.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn transcode_audio(
    stream: &mut TcpStream,
    path: &Path,
    range: Option<(u64, Option<u64>)>,
    jobs: &Mutex<Vec<u32>>,
) -> std::io::Result<()> {
    let Some(total) = audio_size(path) else {
        return transcode(stream, path, true, jobs);
    };
    let (status, start, end) = match range {
        Some((start, _)) if start >= total => {
            let header = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(header.as_bytes())?;
            return Ok(());
        }
        Some((start, end)) => {
            let end = end.unwrap_or(total - 1).min(total - 1);
            if end < start {
                let header = format!(
                    "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{total}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                stream.write_all(header.as_bytes())?;
                return Ok(());
            }
            (206, start, end)
        }
        None => (200, 0, total - 1),
    };
    let count = end - start + 1;
    let start_sec = start as f64 / AUDIO_BYTES_PER_SEC;
    let header = if status == 206 {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {start}-{end}/{total}\r\nContent-Length: {count}\r\nCache-Control: private\r\nConnection: close\r\n\r\n"
        )
    } else {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nAccept-Ranges: bytes\r\nContent-Length: {total}\r\nCache-Control: private\r\nConnection: close\r\n\r\n"
        )
    };
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-ss")
        .arg(format!("{start_sec:.3}"))
        .arg("-i")
        .arg(path)
        .args([
            "-vn",
            "-ac",
            "2",
            "-ar",
            "44100",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "128k",
            "-write_xing",
            "0",
            "-id3v2_version",
            "0",
            "-f",
            "mp3",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            let _ = write_status(
                stream,
                500,
                "text/plain; charset=utf-8",
                b"ffmpeg is not available to convert this file",
            );
            return Ok(());
        }
    };
    if stream.write_all(header.as_bytes()).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(());
    }
    if let Ok(mut jobs) = jobs.lock() {
        jobs.push(child.id());
    }
    let result = pipe_limited(&mut child, stream, count);
    let pid = child.id();
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(mut jobs) = jobs.lock() {
        jobs.retain(|job| *job != pid);
    }
    result
}

fn transcode(stream: &mut TcpStream, path: &Path, audio_only: bool, jobs: &Mutex<Vec<u32>>) -> std::io::Result<()> {
    let mut command = Command::new("ffmpeg");
    command
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-i")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000);
    if audio_only {
        command.args(["-vn", "-c:a", "libmp3lame", "-b:a", "128k", "-f", "mp3", "pipe:1"]);
    } else {
        command.args([
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
            "-movflags",
            "frag_keyframe+empty_moov+default_base_moof",
            "-f",
            "mp4",
            "pipe:1",
        ]);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            let _ = write_status(
                stream,
                500,
                "text/plain; charset=utf-8",
                b"ffmpeg is not available to convert this file",
            );
            return Ok(());
        }
    };
    let content_type = if audio_only { "audio/mpeg" } else { "video/mp4" };
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n"
    );
    if stream.write_all(header.as_bytes()).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Ok(());
    }
    if let Ok(mut jobs) = jobs.lock() {
        jobs.push(child.id());
    }
    let result = pipe_child(&mut child, stream);
    let pid = child.id();
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(mut jobs) = jobs.lock() {
        jobs.retain(|job| *job != pid);
    }
    result
}

fn pipe_child(child: &mut Child, stream: &mut TcpStream) -> std::io::Result<()> {
    pipe_limited(child, stream, u64::MAX)
}

fn pipe_limited(child: &mut Child, stream: &mut TcpStream, limit: u64) -> std::io::Result<()> {
    let Some(mut output) = child.stdout.take() else {
        return Ok(());
    };
    let mut remaining = limit;
    let mut buf = [0u8; 64 * 1024];
    while remaining > 0 {
        let want = remaining.min(buf.len() as u64) as usize;
        let n = output.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        if stream.write_all(&buf[..n]).is_err() {
            break;
        }
        remaining -= n as u64;
    }
    Ok(())
}

fn write_status(stream: &mut TcpStream, code: u16, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        404 => "Not Found",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        500 => "Server Error",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {code} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)
}

fn active_dir(share: &Share) -> Option<PathBuf> {
    share.phone_dir.clone().or_else(|| share.pc_dir.clone())
}

fn order_query(path: &str) -> Option<Order> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("order=") {
            return Some(Order::parse(value));
        }
    }
    None
}

fn items_in(dir: &Path, order: Order) -> Vec<Item> {
    playlist::list_dir(dir, playlist::Filter::All, order)
        .into_iter()
        .map(|entry| {
            let bytes = if entry.kind == Kind::Folder {
                0
            } else {
                std::fs::metadata(&entry.path).map(|meta| meta.len()).unwrap_or(0)
            };
            Item {
                name: playlist::entry_name(&entry.path),
                path: entry.path,
                kind: entry.kind,
                bytes,
            }
        })
        .collect()
}

fn file_at(share: &Share, index: usize) -> Option<Item> {
    let dir = active_dir(share)?;
    let item = items_in(&dir, share.order).into_iter().nth(index)?;
    if item.kind == Kind::Folder {
        None
    } else {
        Some(item)
    }
}

fn move_up(share: &mut Share) {
    let Some(dir) = active_dir(share) else {
        return;
    };
    let Some(parent) = dir.parent() else {
        return;
    };
    if parent.as_os_str().is_empty() {
        return;
    }
    share.phone_dir = Some(parent.to_path_buf());
    share.nav += 1;
}

fn open_index(share: &mut Share, index: usize) {
    let Some(dir) = active_dir(share) else {
        return;
    };
    let Some(item) = items_in(&dir, share.order).into_iter().nth(index) else {
        return;
    };
    if item.kind == Kind::Folder && item.path.is_dir() {
        share.phone_dir = Some(item.path);
        share.nav += 1;
    }
}

fn move_next(share: &mut Share) {
    let Some(dir) = active_dir(share) else {
        return;
    };
    if let Some(next) = next_module(&dir, share.order) {
        share.phone_dir = Some(next);
        share.nav += 1;
    }
}

fn next_module(start: &Path, order: Order) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..4 {
        let parent = dir.parent()?;
        if parent.as_os_str().is_empty() {
            return None;
        }
        let siblings = items_in(parent, order);
        let name = dir.file_name()?;
        let pos = siblings.iter().position(|item| item.path.file_name() == Some(name))?;
        for item in siblings.iter().skip(pos + 1) {
            if item.kind != Kind::Folder {
                continue;
            }
            if let Some(found) = first_playable_dir(&item.path, 3, order) {
                return Some(found);
            }
        }
        dir = parent.to_path_buf();
    }
    None
}

fn first_playable_dir(dir: &Path, depth: u8, order: Order) -> Option<PathBuf> {
    let items = items_in(dir, order);
    if items.iter().any(|item| item.kind != Kind::Folder) {
        return Some(dir.to_path_buf());
    }
    if depth == 0 {
        return None;
    }
    for item in items.iter().filter(|item| item.kind == Kind::Folder) {
        if let Some(found) = first_playable_dir(&item.path, depth - 1, order) {
            return Some(found);
        }
    }
    None
}

fn list_body(share: &Share) -> String {
    let dir = active_dir(share);
    let items = dir.as_ref().map(|dir| items_in(dir, share.order)).unwrap_or_default();
    let parent = dir
        .as_ref()
        .and_then(|dir| dir.parent())
        .is_some_and(|parent| !parent.as_os_str().is_empty());
    let label = dir.as_ref().map(|dir| dir.display().to_string()).unwrap_or_default();
    let current = share.playing.as_ref().and_then(|playing| {
        items.iter().position(|item| item.kind != Kind::Folder && playlist::paths_equal(&item.path, playing))
    });
    list_json(&label, parent, &items, current, share.nav)
}

fn list_json(dir: &str, parent: bool, items: &[Item], current: Option<usize>, nav: u64) -> String {
    let parent = if parent { "true" } else { "false" };
    let mut body = format!(
        "{{\"dir\":\"{}\",\"parent\":{parent},\"gen\":{nav},\"index\":{},\"items\":[",
        json_escape(dir),
        current.map(|index| index.to_string()).unwrap_or_else(|| "null".into())
    );
    for (index, item) in items.iter().enumerate() {
        if index > 0 {
            body.push(',');
        }
        let kind = match item.kind {
            Kind::Folder => "folder",
            Kind::Video => "video",
            Kind::Image => "image",
            Kind::Audio => "audio",
        };
        body.push_str(&format!(
            "{{\"name\":\"{}\",\"kind\":\"{kind}\",\"bytes\":{}}}",
            json_escape(&item.name),
            item.bytes
        ));
    }
    body.push_str("]}");
    body
}

fn json_escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

fn page() -> &'static str {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Better Media Player</title>
<link rel="icon" href="/favicon.ico" sizes="any">
<link rel="icon" type="image/png" href="/favicon.png">
<style>
body{margin:0;background:#16161a;color:#ececf0;font:16px/1.4 Segoe UI,sans-serif}
header{padding:12px 16px 0}
#stage video,#stage audio,#stage img{width:100%;max-height:46vh;background:#000;display:block}
label{display:flex;gap:10px;align-items:center;padding:12px 16px}
#place{display:flex;gap:8px;align-items:flex-start;padding:0 16px 8px}
#up,#sort{background:#2a3142;color:#ececf0;border:0;border-radius:8px;padding:12px 16px;font:inherit;flex:0 0 auto}
#sort.on{background:#3d6b4f}
#folder{color:#bec2cc;word-break:break-word;padding-top:10px}
#now{min-height:1.4em;padding:8px 0;color:#bec2cc}
button.row{display:block;width:100%;text-align:left;background:#16161a;color:#ececf0;border:0;border-bottom:1px solid #2a2a30;padding:16px;font:inherit}
button.row.folder{color:#b7c7ea}
button.row.on{background:#263658}
</style>
</head>
<body>
<header>
<div id="now">Leave Better Media Player open on the PC.</div>
<div id="stage"></div>
</header>
<label><input id="audio" type="checkbox"> Audio only, for listening away from the screen</label>
<div id="place"><button id="up" type="button" hidden>Up</button><button id="sort" type="button">Newest</button><div id="folder"></div></div>
<div id="list"></div>
<script>
const root = location.pathname.replace(/\/$/, "");
const audioBox = document.getElementById("audio");
audioBox.checked = localStorage.getItem("bmp-audio") === "1";
audioBox.onchange = () => { localStorage.setItem("bmp-audio", audioBox.checked ? "1" : "0"); if (current >= 0) choose(current, true); };
const up = document.getElementById("up");
up.onclick = () => load("/up");
const sortBtn = document.getElementById("sort");
let order = localStorage.getItem("bmp-sort") === "name" ? "name" : "newest";
function paintSort() {
  sortBtn.textContent = "Newest";
  sortBtn.className = order === "newest" ? "on" : "";
}
paintSort();
sortBtn.onclick = () => {
  order = order === "newest" ? "name" : "newest";
  localStorage.setItem("bmp-sort", order);
  paintSort();
  listGen++;
  current = -1;
  load("/list");
};
let items = [];
let current = -1;
let folder = "";
let canUp = false;
let listGen = 0;
let navSeq = 0;
let seen = -1;
function nextFile(index) {
  for (let i = index + 1; i < items.length; i++) if (items[i].kind === "audio" || items[i].kind === "video") return i;
  return -1;
}
function choose(index, autoplay) {
  const item = items[index];
  if (!item || item.kind === "folder") return;
  current = index;
  document.getElementById("now").textContent = item.name;
  const audio = audioBox.checked || item.kind === "audio";
  const stage = document.getElementById("stage");
  stage.innerHTML = "";
  let el;
  if (item.kind === "image" && !audioBox.checked) el = document.createElement("img");
  else if (audio) el = document.createElement("audio");
  else el = document.createElement("video");
  if (el.tagName !== "IMG") { el.controls = true; el.playsInline = true; el.preload = "metadata"; }
  const key = "bmp-pos:" + folder + ":" + item.name + ":" + item.bytes;
  el.src = root + (audio && item.kind !== "image" ? "/a/" : "/m/") + index;
  const gen = listGen;
  el.addEventListener("loadedmetadata", () => {
    const t = parseFloat(localStorage.getItem(key) || "0");
    if (t > 1 && isFinite(el.duration) && t < el.duration - 2) el.currentTime = t;
  });
  el.addEventListener("timeupdate", () => {
    if (el.currentTime > 1) localStorage.setItem(key, String(el.currentTime));
  });
  el.addEventListener("ended", () => {
    if (gen !== listGen) return;
    const next = nextFile(index);
    if (next >= 0) choose(next, true);
    else goNextFolder();
  });
  stage.appendChild(el);
  if (autoplay && el.play) el.play().catch(() => {});
  paint();
}
function paint() {
  up.hidden = !canUp;
  document.getElementById("folder").textContent = folder;
  const list = document.getElementById("list");
  list.innerHTML = "";
  items.forEach((item, index) => {
    const row = document.createElement("button");
    row.className = "row" + (item.kind === "folder" ? " folder" : "") + (index === current ? " on" : "");
    row.textContent = item.kind === "folder" ? item.name + "  ›" : item.name;
    row.onclick = () => { if (item.kind === "folder") load("/open/" + index); else choose(index, true); };
    list.appendChild(row);
  });
}
async function load(path) {
  const seq = ++navSeq;
  const response = await fetch(root + path + (path.includes("?") ? "&" : "?") + "order=" + order, { cache: "no-store" });
  const data = await response.json();
  if (data.gen < seen) return null;
  const newer = data.gen > seen;
  seen = data.gen;
  if (!newer && seq !== navSeq) return null;
  const same = data.dir === folder && !!data.parent === canUp && data.items.length === items.length && data.items.every((item, index) => items[index] && items[index].name === item.name && items[index].bytes === item.bytes && items[index].kind === item.kind);
  const dirChanged = data.dir !== folder;
  folder = data.dir || "";
  canUp = !!data.parent;
  if (same) return data;
  if (dirChanged) { listGen++; current = -1; }
  items = data.items;
  paint();
  const stage = document.getElementById("stage");
  if (current < 0 && !stage.firstChild && data.index !== null && items[data.index] && items[data.index].kind !== "folder") choose(data.index, false);
  return data;
}
async function goNextFolder() {
  const before = folder;
  const data = await load("/next");
  if (!data || data.dir === before) return;
  const first = items.findIndex((item) => item.kind === "audio" || item.kind === "video");
  if (first >= 0) choose(first, true);
}
load("/list").catch(() => { document.getElementById("now").textContent = "Could not load the playlist."; });
setInterval(() => load("/list").catch(() => {}), 3000);
</script>
</body>
</html>"#
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(algorithm: *mut core::ffi::c_void, buffer: *mut u8, length: u32, flags: u32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_parses_hours_minutes_seconds() {
        let secs = parse_stamp("00:01:30.5").unwrap();
        assert!((secs - 90.5).abs() < 0.01);
    }

    #[test]
    fn range_header_parses_start_and_end() {
        assert_eq!(parse_range("bytes=0-1"), Some((0, Some(1))));
        assert_eq!(parse_range("bytes=40-"), Some((40, None)));
        assert_eq!(parse_range("bytes=0-1, 8-9"), Some((0, Some(1))));
    }

    #[test]
    fn serves_a_real_file_with_ranges_and_hides_it_without_the_token() {
        let wav = find_real("wav").expect("a real wav in Music");
        let name = wav
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let server = start("abc123abc123abcd".into(), 0).unwrap();
        let dir = wav.parent().expect("the file has a folder").to_path_buf();
        server.publish(Some(dir), Some(wav.clone()));
        let port = server.port;
        let page = http(port, "GET", "/s/abc123abc123abcd");
        assert!(page.starts_with("HTTP/1.1 200"));
        assert!(page.contains("Audio only"));
        assert!(page.contains("id=\"sort\""));
        assert!(page.contains("rel=\"icon\""));
        let mut icon_stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        icon_stream
            .write_all(b"GET /favicon.ico HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .unwrap();
        let mut icon = Vec::new();
        icon_stream.read_to_end(&mut icon).unwrap();
        let split = icon.windows(4).position(|mark| mark == b"\r\n\r\n").unwrap();
        let header = String::from_utf8_lossy(&icon[..split]);
        assert!(header.starts_with("HTTP/1.1 200"), "{header}");
        assert!(header.contains("image/x-icon"));
        assert!(icon[split + 4..].starts_with(&[0, 0, 1, 0]));
        let list = http(port, "GET", "/s/abc123abc123abcd/list");
        assert!(list.contains(&name));
        let hidden = http(port, "GET", "/s/wrong-token-value/list");
        assert!(hidden.starts_with("HTTP/1.1 404"));
        assert!(!hidden.contains(&name));
        let wav_index = item_index(&list, &name);
        let partial = http(port, "GET", &format!("/s/abc123abc123abcd/m/{wav_index}\r\nRange: bytes=0-3"));
        assert!(partial.starts_with("HTTP/1.1 206"));
        assert!(partial.contains("audio/wav"));
        assert!(partial.contains("Content-Range: bytes 0-3/"));
        if let Some(video) = find_real("mp4") {
            let video_name = video.file_name().unwrap().to_string_lossy().into_owned();
            let video_dir = video.parent().expect("the video has a folder").to_path_buf();
            server.publish(Some(video_dir), Some(video.clone()));
            let listed = http(port, "GET", "/s/abc123abc123abcd/list");
            let video_index = item_index(&listed, &video_name);
            let head = http(port, "GET", &format!("/s/abc123abc123abcd/m/{video_index}\r\nRange: bytes=0-15"));
            assert!(head.starts_with("HTTP/1.1 206"));
            assert!(head.contains("video/mp4"));
            let audio = http_header(port, "GET", &format!("/s/abc123abc123abcd/a/{video_index}\r\nRange: bytes=0-15"));
            assert!(audio.starts_with("HTTP/1.1 206"), "{audio}");
            assert!(audio.contains("audio/mpeg"));
            assert!(audio.contains("Accept-Ranges: bytes"));
            let total = audio
                .split("Content-Range: bytes 0-15/")
                .nth(1)
                .and_then(|rest| rest.split("\r\n").next())
                .and_then(|rest| rest.parse::<u64>().ok())
                .unwrap_or(0);
            if total > 32_000 {
                let mid = total / 2;
                let jumped = http_header(
                    port,
                    "GET",
                    &format!("/s/abc123abc123abcd/a/{video_index}\r\nRange: bytes={mid}-{mid}"),
                );
                assert!(jumped.starts_with("HTTP/1.1 206"), "{jumped}");
                assert!(jumped.contains(&format!("Content-Range: bytes {mid}-{mid}/")));
            }
        }
        server.stop();
    }

    #[test]
    fn phone_page_preview() {
        if std::env::var("BMP_PREVIEW").is_err() {
            return;
        }
        let wav = find_real("wav").expect("a real wav in Music");
        let server = start("abc123abc123abcd".into(), 47991).unwrap();
        let dir = wav.parent().expect("the file has a folder").to_path_buf();
        server.publish(Some(dir), Some(wav));
        eprintln!("PREVIEW http://127.0.0.1:{}/s/abc123abc123abcd", server.port);
        thread::sleep(Duration::from_secs(45));
        server.stop();
    }

    #[test]
    fn phone_moves_between_real_folders() {
        let wav = find_real("wav").expect("a real wav in Music");
        let name = wav.file_name().unwrap().to_string_lossy().into_owned();
        let dir = wav.parent().expect("the file has a folder").to_path_buf();
        let folder_name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let server = start("abc123abc123abcd".into(), 0).unwrap();
        server.publish(Some(dir), Some(wav));
        let port = server.port;
        let list = http(port, "GET", "/s/abc123abc123abcd/list");
        assert!(list.contains(&format!("\"name\":\"{}\"", json_escape(&name))), "{list}");
        assert!(list.contains("\"parent\":true"), "{list}");
        let up = http(port, "GET", "/s/abc123abc123abcd/up");
        assert!(up.contains(&format!("\"kind\":\"folder\"")), "{up}");
        assert!(up.contains(&format!("\"name\":\"{}\"", json_escape(&folder_name))), "{up}");
        let hidden = http(port, "GET", "/s/wrong-token-value/up");
        assert!(hidden.starts_with("HTTP/1.1 404"));
        assert!(!hidden.contains(&folder_name));
        let index = item_index(&up, &folder_name);
        let back = http(port, "GET", &format!("/s/abc123abc123abcd/open/{index}"));
        assert!(back.contains(&format!("\"name\":\"{}\"", json_escape(&name))), "{back}");
        let opened = http(port, "GET", "/s/abc123abc123abcd/list");
        assert!(opened.contains(&format!("\"name\":\"{}\"", json_escape(&name))));
        server.stop();
    }

    #[test]
    fn phone_newest_lists_real_files_latest_first() {
        let wav = find_real("wav").expect("a real wav in Music");
        let dir = wav.parent().expect("the file has a folder").to_path_buf();
        let server = start("abc123abc123abcd".into(), 0).unwrap();
        server.publish(Some(dir.clone()), None);
        let port = server.port;
        let list = http(port, "GET", "/s/abc123abc123abcd/list?order=newest");
        assert!(list.contains("id=\"sort\"") || list.contains("\"items\":"), "{list}");
        let mut times = Vec::new();
        for name in file_names(&list) {
            let path = dir.join(&name);
            if let Ok(time) = std::fs::metadata(&path).and_then(|meta| meta.modified()) {
                times.push(time);
            }
        }
        assert!(times.len() >= 2, "{list}");
        for pair in times.windows(2) {
            assert!(pair[0] >= pair[1]);
        }
        server.stop();
    }

    fn file_names(body: &str) -> Vec<String> {
        let items = body.split("\"items\":").nth(1).unwrap_or("");
        let mut names = Vec::new();
        let mut rest = items;
        while let Some(pos) = rest.find("{\"name\":\"") {
            rest = &rest[pos + 9..];
            let Some(end) = rest.find('"') else { break };
            let name = rest[..end].replace("\\\\", "\\").replace("\\\"", "\"");
            let Some(kind_end) = rest.find('}') else { break };
            if !rest[..kind_end].contains("\"kind\":\"folder\"") {
                names.push(name);
            }
            rest = &rest[end..];
        }
        names
    }

    fn item_index(body: &str, name: &str) -> usize {
        let items = body.split("\"items\":").nth(1).unwrap_or(body);
        let needle = format!("\"name\":\"{}\"", json_escape(name));
        let pos = items.find(&needle).unwrap_or_else(|| panic!("{name} missing from {body}"));
        items[..pos].matches("\"name\":").count()
    }

    fn find_real(ext: &str) -> Option<std::path::PathBuf> {
        let root = std::env::var_os("USERPROFILE")?;
        let music = PathBuf::from(root).join("Music");
        let mut stack = vec![(music, 0u8)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && depth < 3 {
                    stack.push((path, depth + 1));
                } else if path
                    .extension()
                    .and_then(|item| item.to_str())
                    .is_some_and(|item| item.eq_ignore_ascii_case(ext))
                {
                    return Some(path);
                }
            }
        }
        None
    }

    fn http_header(port: u16, method: &str, target: &str) -> String {
        let mut parts = target.split("\r\n");
        let path = parts.next().unwrap();
        let extra = parts.next().unwrap_or("");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{extra}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 512];
        while !buf.windows(4).any(|mark| mark == b"\r\n\r\n") && buf.len() < 8192 {
            let n = stream.read(&mut tmp).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
        }
        let end = buf.windows(4).position(|mark| mark == b"\r\n\r\n").unwrap_or(buf.len());
        String::from_utf8_lossy(&buf[..end]).into_owned()
    }

    fn http(port: u16, method: &str, target: &str) -> String {
        let mut parts = target.split("\r\n");
        let path = parts.next().unwrap();
        let extra = parts.next().unwrap_or("");
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{extra}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).unwrap();
        let mut body = String::new();
        let _ = stream.read_to_string(&mut body);
        body
    }
}
