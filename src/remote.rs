use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

pub const WM_URL_READY: u32 = WM_APP + 12;

#[derive(Clone)]
pub struct MediaLink {
    pub title: String,
    pub stream: String,
    pub page: String,
}

static RESULT: Mutex<Option<(u64, Result<MediaLink, String>)>> = Mutex::new(None);
static YTDLP: OnceLock<Option<String>> = OnceLock::new();

pub fn resolve_later(hwnd: isize, generation: u64, text: String) {
    thread::spawn(move || {
        let resolved = resolve(&text);
        if let Ok(mut slot) = RESULT.lock() {
            *slot = Some((generation, resolved));
        }
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_URL_READY,
                WPARAM(generation as usize),
                LPARAM(0),
            );
        }
    });
}

pub fn take_result() -> Option<(u64, Result<MediaLink, String>)> {
    RESULT.lock().ok().and_then(|mut slot| slot.take())
}

pub fn resolve(input: &str) -> Result<MediaLink, String> {
    let mut text = input.trim().to_string();
    if text.is_empty() {
        return Err("Paste a media URL first".into());
    }
    if text.starts_with("www.") {
        text = format!("https://{text}");
    }
    if !(text.starts_with("http://") || text.starts_with("https://")) {
        return Err("Paste a full http or https link".into());
    }
    if is_direct_media(&text) {
        return Ok(MediaLink {
            title: title_from_url(&text),
            stream: text.clone(),
            page: text,
        });
    }
    youtube(&text)
}

fn is_direct_media(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "mp3" | "wav"
            | "flac"
            | "ogg"
            | "m4a"
            | "aac"
            | "mp4"
            | "m4v"
            | "mkv"
            | "webm"
            | "mov"
            | "avi"
            | "wmv"
            | "mpg"
            | "mpeg"
    )
}

fn title_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(url)
        .to_string()
}

fn is_youtube(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("youtube.com/") || lower.contains("youtu.be/") || lower.contains("music.youtube.com/")
}

fn youtube(url: &str) -> Result<MediaLink, String> {
    let Some(program) = ytdlp_program() else {
        return Err(
            "YouTube and other web pages need yt-dlp. Direct media file URLs still play."
                .into(),
        );
    };
    let output = Command::new(&program)
        .args([
            "--no-playlist",
            "--no-warnings",
            "--no-progress",
            "--extractor-args",
            "youtube:player_client=android",
            "-f",
            "b/18/best",
            "-e",
            "-g",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let mut err = String::new();
        let _ = output.stderr.as_slice().read_to_string(&mut err);
        let err = err.trim();
        if err.is_empty() {
            return Err("Could not open that link".into());
        }
        let mut line = err.lines().next().unwrap_or(err).trim().to_string();
        if line.len() > 240 {
            line.truncate(240);
        }
        return Err(line);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let stream = lines
        .iter()
        .rev()
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .ok_or_else(|| "The link did not resolve to a playable stream".to_string())?;
    let title = lines
        .iter()
        .find(|line| !line.starts_with("http://") && !line.starts_with("https://"))
        .map(|line| (*line).to_string())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| {
            if is_youtube(url) {
                "YouTube".into()
            } else {
                title_from_url(url)
            }
        });
    Ok(MediaLink {
        title,
        stream: (*stream).to_string(),
        page: url.to_string(),
    })
}

fn ytdlp_program() -> Option<String> {
    YTDLP
        .get_or_init(|| {
            for name in ["yt-dlp", "yt-dlp.exe"] {
                let ok = Command::new(name)
                    .arg("--version")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .creation_flags(0x0800_0000)
                    .status()
                    .map(|status| status.success())
                    .unwrap_or(false);
                if ok {
                    return Some(name.to_string());
                }
            }
            None
        })
        .clone()
}
