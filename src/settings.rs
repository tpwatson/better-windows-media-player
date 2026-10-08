use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Settings {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub volume: f32,
    pub speed: f32,
    pub loop_playlist: bool,
    pub auto_next: bool,
    pub topmost: bool,
    pub opacity: u8,
    pub filter: String,
    pub order: String,
    pub phone: bool,
    pub phone_token: String,
    pub phone_port: u16,
    pub list_px: i32,
    pub placed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            x: 80,
            y: 80,
            width: 980,
            height: 640,
            volume: 1.0,
            speed: 1.0,
            loop_playlist: false,
            auto_next: true,
            topmost: false,
            opacity: 255,
            filter: "all".into(),
            order: "newest".into(),
            phone: false,
            phone_token: String::new(),
            phone_port: 47832,
            list_px: 0,
            placed: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut settings = Self::default();
        let Some(path) = settings_path() else {
            return settings;
        };
        let Ok(text) = fs::read_to_string(path) else {
            return settings;
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "x" => settings.x = value.trim().parse().unwrap_or(settings.x),
                "y" => settings.y = value.trim().parse().unwrap_or(settings.y),
                "w" => settings.width = value.trim().parse().unwrap_or(settings.width),
                "h" => settings.height = value.trim().parse().unwrap_or(settings.height),
                "volume" => {
                    settings.volume = value
                        .trim()
                        .parse::<f32>()
                        .unwrap_or(settings.volume)
                        .clamp(0.0, 1.0);
                }
                "speed" => {
                    settings.speed = value
                        .trim()
                        .parse::<f32>()
                        .unwrap_or(settings.speed)
                        .clamp(0.25, 4.0);
                }
                "loop" => settings.loop_playlist = value.trim() == "1",
                "auto" => settings.auto_next = value.trim() == "1",
                "top" => settings.topmost = value.trim() == "1",
                "opacity" => {
                    settings.opacity = value
                        .trim()
                        .parse::<u8>()
                        .unwrap_or(settings.opacity)
                        .clamp(115, 255);
                }
                "filter" => settings.filter = value.trim().to_string(),
                "order" => settings.order = value.trim().to_string(),
                "phone" => settings.phone = value.trim() == "1",
                "phone_token" => {
                    let token = value.trim();
                    if token.len() >= 16 && token.chars().all(|ch| ch.is_ascii_hexdigit()) {
                        settings.phone_token = token.to_string();
                    }
                }
                "phone_port" => {
                    settings.phone_port = value.trim().parse().unwrap_or(settings.phone_port);
                }
                "list" => {
                    settings.list_px = value.trim().parse::<i32>().unwrap_or(0).max(0);
                }
                _ => {}
            }
        }
        if settings.width >= 480 && settings.height >= 320 {
            settings.placed = true;
        } else {
            settings.width = 980;
            settings.height = 640;
        }
        settings
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let text = format!(
            "x={}\ny={}\nw={}\nh={}\nvolume={:.3}\nspeed={:.2}\nloop={}\nauto={}\ntop={}\nopacity={}\nfilter={}\norder={}\nphone={}\nphone_token={}\nphone_port={}\nlist={}\n",
            self.x,
            self.y,
            self.width.max(480),
            self.height.max(320),
            self.volume.clamp(0.0, 1.0),
            self.speed.clamp(0.25, 4.0),
            if self.loop_playlist { 1 } else { 0 },
            if self.auto_next { 1 } else { 0 },
            if self.topmost { 1 } else { 0 },
            self.opacity.clamp(115, 255),
            self.filter.trim(),
            if self.order.trim() == "newest" { "newest" } else { "name" },
            if self.phone { 1 } else { 0 },
            self.phone_token,
            self.phone_port,
            self.list_px.max(0)
        );
        let _ = fs::write(path, text);
    }
}

fn settings_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("BetterMediaPlayer")
            .join("window.txt"),
    )
}
