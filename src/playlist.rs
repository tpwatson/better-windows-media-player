use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "wav", "wave", "flac", "ogg", "oga", "opus", "m4a", "aac", "wma", "aiff", "aif",
];

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "avi", "wmv", "mov", "webm", "mpg", "mpeg", "wtv", "asf", "m2ts",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "ico", "heic",
];

const FAST_AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "wave", "flac", "ogg", "oga"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    Mp3,
    Video,
    Images,
}

impl Filter {
    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "mp3" => Self::Mp3,
            "video" => Self::Video,
            "images" => Self::Images,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mp3 => "mp3",
            Self::Video => "video",
            Self::Images => "images",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Audio,
    Video,
    Image,
}

#[derive(Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub kind: Kind,
}

pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

pub fn is_audio(path: &Path) -> bool {
    extension_of(path).is_some_and(|ext| AUDIO_EXTENSIONS.contains(&ext.as_str()))
}

pub fn is_video(path: &Path) -> bool {
    extension_of(path).is_some_and(|ext| VIDEO_EXTENSIONS.contains(&ext.as_str()))
}

pub fn is_image(path: &Path) -> bool {
    extension_of(path).is_some_and(|ext| IMAGE_EXTENSIONS.contains(&ext.as_str()))
}

pub fn entry_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn badge(kind: Kind, path: &Path) -> &'static str {
    match kind {
        Kind::Folder => "Folder",
        Kind::Video => "Video",
        Kind::Image => "Image",
        Kind::Audio if extension_of(path).as_deref() == Some("mp3") => "MP3",
        Kind::Audio => "Audio",
    }
}

impl Filter {
    pub fn matches(self, path: &Path) -> bool {
        match self {
            Self::All => is_audio(path) || is_video(path) || is_image(path),
            Self::Mp3 => extension_of(path).as_deref() == Some("mp3"),
            Self::Video => is_video(path),
            Self::Images => is_image(path),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    Name,
    Newest,
}

impl Order {
    pub fn parse(value: &str) -> Self {
        if value.trim() == "newest" {
            Self::Newest
        } else {
            Self::Name
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Newest => "newest",
        }
    }
}

pub fn list_dir(dir: &Path, filter: Filter, order: Order) -> Vec<Entry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            folders.push(path);
        } else if path.is_file() && filter.matches(&path) {
            files.push(path);
        }
    }
    match order {
        Order::Name => {
            sort_paths(&mut folders);
            sort_paths(&mut files);
        }
        Order::Newest => {
            sort_by_newest(&mut folders);
            sort_by_newest(&mut files);
        }
    }
    let mut listed = Vec::with_capacity(folders.len() + files.len());
    for path in folders {
        listed.push(Entry {
            path,
            kind: Kind::Folder,
        });
    }
    for path in files {
        let kind = if is_image(&path) {
            Kind::Image
        } else if is_video(&path) {
            Kind::Video
        } else {
            Kind::Audio
        };
        listed.push(Entry { path, kind });
    }
    listed
}

pub fn prefers_fast_audio(path: &Path) -> bool {
    extension_of(path).is_some_and(|ext| FAST_AUDIO_EXTENSIONS.contains(&ext.as_str()))
}

fn modified(path: &Path) -> std::time::SystemTime {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
}

pub fn sort_by_newest(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| {
        modified(b).cmp(&modified(a)).then_with(|| {
            let ao = a.file_name().unwrap_or_default();
            let bo = b.file_name().unwrap_or_default();
            compare_natural(&ao.to_string_lossy(), &bo.to_string_lossy()).then_with(|| a.cmp(b))
        })
    });
}

pub fn sort_paths(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| {
        let ao = a.file_name().unwrap_or_default();
        let bo = b.file_name().unwrap_or_default();
        compare_natural(&ao.to_string_lossy(), &bo.to_string_lossy()).then_with(|| a.cmp(b))
    });
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (fs::canonicalize(a), fs::canonicalize(b)) {
        return a == b;
    }
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

pub fn normalize_shell_path(arg: &str) -> PathBuf {
    let mut text = arg.trim().trim_matches('"').trim().to_string();
    if text.len() > 3 && (text.ends_with('\\') || text.ends_with('/')) {
        text.pop();
    }
    let path = PathBuf::from(&text);
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|dir| dir.join(path))
            .unwrap_or_else(|_| PathBuf::from(text))
    }
}

pub fn compare_natural(a: &str, b: &str) -> Ordering {
    let mut ac = a.chars().peekable();
    let mut bc = b.chars().peekable();
    loop {
        match (ac.peek().copied(), bc.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left), Some(right)) if left.is_ascii_digit() && right.is_ascii_digit() => {
                let (an, alen) = take_digits(&mut ac);
                let (bn, blen) = take_digits(&mut bc);
                match an.cmp(&bn).then(alen.cmp(&blen)) {
                    Ordering::Equal => {}
                    order => return order,
                }
            }
            (Some(left), Some(right)) => {
                ac.next();
                bc.next();
                match left
                    .to_ascii_lowercase()
                    .cmp(&right.to_ascii_lowercase())
                {
                    Ordering::Equal => {}
                    order => return order,
                }
            }
        }
    }
}

fn take_digits(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> (u128, usize) {
    let mut value: u128 = 0;
    let mut len = 0;
    let mut overflow = false;
    while let Some(ch) = chars.peek().copied() {
        if !ch.is_ascii_digit() {
            break;
        }
        chars.next();
        len += 1;
        if !overflow {
            match value
                .checked_mul(10)
                .and_then(|next| next.checked_add(u128::from(ch as u8 - b'0')))
            {
                Some(next) => value = next,
                None => overflow = true,
            }
        }
    }
    if overflow {
        (u128::MAX, len)
    } else {
        (value, len)
    }
}

pub fn shell_extensions() -> &'static [&'static str] {
    &[
        "mp3", "wav", "flac", "ogg", "opus", "m4a", "aac", "wma", "aiff", "aif", "mp4", "m4v",
        "mkv", "avi", "wmv", "mov", "webm", "mpg", "mpeg", "wtv",
    ]
}

#[cfg(test)]
mod tests {
    use super::compare_natural;
    use std::cmp::Ordering;

    #[test]
    fn natural_order_sorts_numbers_by_value() {
        assert_eq!(
            compare_natural("track2.mp3", "track10.mp3"),
            Ordering::Less
        );
        assert_eq!(compare_natural("10", "2"), Ordering::Greater);
        assert_eq!(
            compare_natural("Clip.mp4", "clip.mp4"),
            Ordering::Equal
        );
    }

    #[test]
    fn newest_puts_the_latest_real_file_first() {
        let root = std::env::var_os("USERPROFILE").expect("USERPROFILE");
        let music = std::path::PathBuf::from(root).join("Music");
        let listed = super::list_dir(&music, super::Filter::All, super::Order::Newest);
        let files: Vec<_> = listed
            .into_iter()
            .filter(|entry| entry.kind != super::Kind::Folder)
            .map(|entry| entry.path)
            .collect();
        assert!(files.len() >= 2, "Music needs at least two real files");
        let times: Vec<_> = files.iter().map(|path| super::modified(path)).collect();
        for pair in times.windows(2) {
            assert!(pair[0] >= pair[1]);
        }
    }
}
