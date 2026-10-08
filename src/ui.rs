use std::time::Duration;
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, GetStockObject,
    GetTextExtentPoint32W, GetTextMetricsW, IntersectClipRect, Polygon, RestoreDC, SaveDC,
    SelectObject, SetBkMode, SetStretchBltMode, SetTextColor, StretchBlt, BLACK_BRUSH, BLACK_PEN,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, COLORONCOLOR, DEFAULT_CHARSET, DEFAULT_PITCH,
    DRAW_TEXT_FORMAT, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_PATH_ELLIPSIS, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, DT_WORDBREAK, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, HBITMAP, HBRUSH, HDC, HFONT,
    OUT_DEFAULT_PRECIS, SRCCOPY, TEXTMETRICW, TRANSPARENT, WHITE_BRUSH,
};
use crate::playlist::Filter;
use windows_core::PCWSTR;

pub fn dp(px: i32, dpi: i32) -> i32 {
    px * dpi.max(96) / 96
}

pub fn rgb(r: u32, g: u32, b: u32) -> COLORREF {
    COLORREF(r | (g << 8) | (b << 16))
}

pub struct Gfx {
    pub dpi: i32,
    pub bg: HBRUSH,
    pub stage: HBRUSH,
    pub transport: HBRUSH,
    pub button: HBRUSH,
    pub button_hot: HBRUSH,
    pub accent: HBRUSH,
    pub row: HBRUSH,
    pub seek: HBRUSH,
    pub font: HFONT,
    pub font_bold: HFONT,
    pub font_small: HFONT,
}

impl Gfx {
    pub fn new(dpi: i32) -> Self {
        unsafe {
            Self {
                dpi,
                bg: CreateSolidBrush(rgb(22, 22, 26)),
                stage: CreateSolidBrush(rgb(12, 12, 14)),
                transport: CreateSolidBrush(rgb(32, 32, 36)),
                button: CreateSolidBrush(rgb(50, 50, 56)),
                button_hot: CreateSolidBrush(rgb(68, 68, 76)),
                accent: CreateSolidBrush(rgb(64, 128, 255)),
                row: CreateSolidBrush(rgb(38, 54, 88)),
                seek: CreateSolidBrush(rgb(58, 58, 66)),
                font: font(dp(15, dpi), FW_NORMAL, w("Segoe UI")),
                font_bold: font(dp(22, dpi), FW_SEMIBOLD, w("Segoe UI")),
                font_small: font(dp(12, dpi), FW_NORMAL, w("Segoe UI")),
            }
        }
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.bg.into());
            let _ = DeleteObject(self.stage.into());
            let _ = DeleteObject(self.transport.into());
            let _ = DeleteObject(self.button.into());
            let _ = DeleteObject(self.button_hot.into());
            let _ = DeleteObject(self.accent.into());
            let _ = DeleteObject(self.row.into());
            let _ = DeleteObject(self.seek.into());
            let _ = DeleteObject(self.font.into());
            let _ = DeleteObject(self.font_bold.into());
            let _ = DeleteObject(self.font_small.into());
        }
    }
}

fn font(height: i32, weight: windows::Win32::Graphics::Gdi::FONT_WEIGHT, face: Vec<u16>) -> HFONT {
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0) | u32::from(FF_DONTCARE.0),
            PCWSTR(face.as_ptr()),
        )
    }
}

fn w(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Prev,
    Play,
    Next,
    Seek,
    Volume,
    AutoNext,
    Loop,
    Up,
    Back,
    Speed,
    Top,
    Phone,
    Filters,
    Newest,
    Opacity,
    OpacityBtn,
    YouTube,
    OpenUrl,
    Split,
    Filter(Filter),
    Stage,
    Row(usize),
}

pub struct Layout {
    pub title_bar: RECT,
    pub stage: RECT,
    pub transport: RECT,
    pub prev: RECT,
    pub play: RECT,
    pub next: RECT,
    pub time: RECT,
    pub seek: RECT,
    pub volume: RECT,
    pub auto_btn: RECT,
    pub loop_btn: RECT,
    pub panel: RECT,
    pub up: RECT,
    pub back: RECT,
    pub path: RECT,
    pub phone_btn: RECT,
    pub youtube: RECT,
    pub filter_btn: RECT,
    pub filter_all: RECT,
    pub filter_mp3: RECT,
    pub filter_video: RECT,
    pub filter_images: RECT,
    pub newest_btn: RECT,
    pub speed: RECT,
    pub top: RECT,
    pub opacity_btn: RECT,
    pub opacity_label: RECT,
    pub opacity: RECT,
    pub url: RECT,
    pub open_url: RECT,
    pub split: RECT,
    pub list: RECT,
    pub phone_panel: RECT,
}

impl Layout {
    pub fn hit(&self, x: i32, y: i32, scroll: i32, heights: &[i32]) -> Option<Hit> {
        if contains(self.prev, x, y) {
            return Some(Hit::Prev);
        }
        if contains(self.play, x, y) {
            return Some(Hit::Play);
        }
        if contains(self.next, x, y) {
            return Some(Hit::Next);
        }
        if contains(self.seek, x, y) {
            return Some(Hit::Seek);
        }
        if contains(self.volume, x, y) {
            return Some(Hit::Volume);
        }
        if contains(self.auto_btn, x, y) {
            return Some(Hit::AutoNext);
        }
        if contains(self.loop_btn, x, y) {
            return Some(Hit::Loop);
        }
        if contains(self.up, x, y) {
            return Some(Hit::Up);
        }
        if contains(self.back, x, y) {
            return Some(Hit::Back);
        }
        if contains(self.speed, x, y) {
            return Some(Hit::Speed);
        }
        if contains(self.top, x, y) {
            return Some(Hit::Top);
        }
        if contains(self.phone_btn, x, y) {
            return Some(Hit::Phone);
        }
        if contains(self.youtube, x, y) {
            return Some(Hit::YouTube);
        }
        if contains(self.filter_btn, x, y) {
            return Some(Hit::Filters);
        }
        if contains(self.newest_btn, x, y) {
            return Some(Hit::Newest);
        }
        if contains(self.opacity_btn, x, y) {
            return Some(Hit::OpacityBtn);
        }
        if contains(self.split, x, y) {
            return Some(Hit::Split);
        }
        if contains(self.opacity, x, y) {
            return Some(Hit::Opacity);
        }
        if contains(self.open_url, x, y) {
            return Some(Hit::OpenUrl);
        }
        if contains(self.filter_all, x, y) {
            return Some(Hit::Filter(Filter::All));
        }
        if contains(self.filter_mp3, x, y) {
            return Some(Hit::Filter(Filter::Mp3));
        }
        if contains(self.filter_video, x, y) {
            return Some(Hit::Filter(Filter::Video));
        }
        if contains(self.filter_images, x, y) {
            return Some(Hit::Filter(Filter::Images));
        }
        if contains(self.list, x, y) {
            return row_at(self.list.top, y, scroll, heights).map(Hit::Row);
        }
        if contains(self.stage, x, y) {
            return Some(Hit::Stage);
        }
        None
    }

    pub fn ratio_in(&self, rect: RECT, x: i32) -> f32 {
        let width = (rect.right - rect.left).max(1) as f32;
        ((x - rect.left) as f32 / width).clamp(0.0, 1.0)
    }

    pub fn list_height(&self) -> i32 {
        (self.list.bottom - self.list.top).max(1)
    }
}

pub fn row_at(list_top: i32, y: i32, scroll: i32, heights: &[i32]) -> Option<usize> {
    let mut cursor = list_top - scroll;
    for (index, height) in heights.iter().enumerate() {
        let next = cursor + *height;
        if y >= cursor && y < next {
            return Some(index);
        }
        cursor = next;
    }
    None
}

pub fn content_height(heights: &[i32]) -> i32 {
    heights.iter().sum()
}

pub fn layout(
    width: i32,
    height: i32,
    dpi: i32,
    media: bool,
    title_px: i32,
    phone: bool,
    url_open: bool,
    list_px: i32,
    filters_open: bool,
    opacity_open: bool,
) -> Layout {
    let pad = dp(12, dpi);
    let gap = dp(6, dpi);
    let transport_h = dp(64, dpi);
    let row = dp(28, dpi);
    let split_h = dp(10, dpi);
    let title_h = if media { title_px.max(0) } else { 0 };
    let url_extra = if url_open { row + gap } else { 0 };
    let chrome = gap + row + gap + row + gap + url_extra + split_h;
    let available = (height - title_h - transport_h - chrome).max(dp(32, dpi));
    let list_min = dp(64, dpi).min(available);
    let automatic = (height * 42 / 100 - chrome).clamp(list_min, available);
    let list_h = if list_px > 0 {
        list_px.clamp(list_min, available)
    } else {
        automatic
    };
    let playlist_h = chrome + list_h;
    let transport_top = (height - playlist_h - transport_h).max(title_h);
    let playlist_top = transport_top + transport_h;
    let title_bar = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: title_h,
    };
    let phone_w = if phone {
        dp(220, dpi).min((width / 3).max(dp(140, dpi)))
    } else {
        0
    };
    let stage = RECT {
        left: 0,
        top: title_h,
        right: (width - phone_w).max(0),
        bottom: transport_top,
    };
    let phone_panel = RECT {
        left: width - phone_w,
        top: title_h,
        right: width,
        bottom: transport_top,
    };
    let transport = RECT {
        left: 0,
        top: transport_top,
        right: width,
        bottom: playlist_top,
    };
    let y = transport_top + dp(14, dpi);
    let h = dp(36, dpi);
    let mut x = pad;
    let prev = rect(x, y, dp(36, dpi), h);
    x = prev.right + gap;
    let play = rect(x, y, dp(44, dpi), h);
    x = play.right + gap;
    let next = rect(x, y, dp(36, dpi), h);
    x = next.right + dp(10, dpi);
    let time = rect(x, y, dp(118, dpi), h);
    let auto_w = dp(96, dpi);
    let loop_w = dp(58, dpi);
    let volume_w = dp(84, dpi);
    let right = width - pad;
    let auto_btn = rect(right - auto_w, y, auto_w, h);
    let loop_btn = rect(auto_btn.left - gap - loop_w, y, loop_w, h);
    let volume = rect(loop_btn.left - dp(8, dpi) - volume_w, y, volume_w, h);
    let seek_left = time.right + dp(8, dpi);
    let seek_right = volume.left - dp(8, dpi);
    let seek = rect(
        seek_left,
        y + dp(12, dpi),
        (seek_right - seek_left).max(dp(24, dpi)),
        dp(12, dpi),
    );
    let y1 = playlist_top + gap;
    let up = rect(pad, y1, dp(40, dpi), row);
    let back = rect(up.right + gap, y1, dp(52, dpi), row);
    let phone_btn = rect(width - pad - dp(68, dpi), y1, dp(68, dpi), row);
    let youtube = rect(phone_btn.left - gap - dp(78, dpi), y1, dp(78, dpi), row);
    let path = rect(
        back.right + gap,
        y1,
        (youtube.left - gap - (back.right + gap)).max(dp(40, dpi)),
        row,
    );
    let y2 = y1 + row + gap;
    let filter_btn = rect(pad, y2, dp(64, dpi), row);
    let mut fx = filter_btn.right + gap;
    let (filter_all, filter_mp3, filter_video, filter_images) = if filters_open {
        let all = rect(fx, y2, dp(40, dpi), row);
        fx = all.right + gap;
        let mp3 = rect(fx, y2, dp(44, dpi), row);
        fx = mp3.right + gap;
        let video = rect(fx, y2, dp(54, dpi), row);
        fx = video.right + gap;
        let images = rect(fx, y2, dp(60, dpi), row);
        fx = images.right + gap;
        (all, mp3, video, images)
    } else {
        (blank(), blank(), blank(), blank())
    };
    let newest_btn = rect(fx, y2, dp(78, dpi), row);
    fx = newest_btn.right + gap;
    let opacity_btn = rect(width - pad - row, y2, row, row);
    let top = rect(opacity_btn.left - gap - dp(44, dpi), y2, dp(44, dpi), row);
    let speed = rect(top.left - gap - dp(58, dpi), y2, dp(58, dpi), row);
    let (opacity_label, opacity) = if opacity_open {
        let label_w = dp(42, dpi);
        let label = rect(speed.left - gap - label_w, y2, label_w, row);
        let slider_right = label.left - dp(6, dpi);
        let slider_w = (slider_right - fx).max(dp(36, dpi));
        let slider = rect(slider_right - slider_w, y2 + dp(8, dpi), slider_w, dp(12, dpi));
        (label, slider)
    } else {
        (blank(), blank())
    };
    let y3 = y2 + row + gap;
    let (url, open_url, after_url) = if url_open {
        let open_w = dp(56, dpi);
        let url = rect(pad, y3, (width - pad * 2 - gap - open_w).max(dp(80, dpi)), row);
        let open_url = rect(url.right + gap, y3, open_w, row);
        (url, open_url, y3 + row + gap)
    } else {
        (blank(), blank(), y3)
    };
    let split = rect(0, after_url, width, split_h);
    let list_top = after_url + split_h;
    Layout {
        title_bar,
        stage,
        transport,
        prev,
        play,
        next,
        time,
        seek,
        volume,
        auto_btn,
        loop_btn,
        panel: RECT {
            left: 0,
            top: playlist_top,
            right: width,
            bottom: height,
        },
        up,
        back,
        path,
        phone_btn,
        youtube,
        filter_btn,
        filter_all,
        filter_mp3,
        filter_video,
        filter_images,
        newest_btn,
        speed,
        top,
        opacity_btn,
        opacity_label,
        opacity,
        url,
        open_url,
        split,
        list: RECT {
            left: 0,
            top: list_top,
            right: width,
            bottom: height,
        },
        phone_panel,
    }
}

fn blank() -> RECT {
    RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    }
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

fn contains(rect: RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

pub struct PaintState<'a> {
    pub title: &'a str,
    pub folder: &'a str,
    pub status: &'a str,
    pub speed_label: &'a str,
    pub rows: Vec<RowPaint>,
    pub heights: &'a [i32],
    pub scroll: i32,
    pub current: Option<usize>,
    pub playing: bool,
    pub show_video: bool,
    pub show_image: bool,
    pub image: Option<(HBITMAP, i32, i32)>,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub seek_override: Option<f32>,
    pub volume: f32,
    pub opacity: f32,
    pub auto_next: bool,
    pub loop_playlist: bool,
    pub topmost: bool,
    pub phone: bool,
    pub phone_url: &'a str,
    pub phone_qr: &'a [bool],
    pub phone_qr_width: usize,
    pub filter: Filter,
    pub newest: bool,
    pub hot: Option<Hit>,
}

pub struct RowPaint {
    pub name: String,
    pub badge: &'static str,
}

pub fn paint(hdc: HDC, gfx: &Gfx, area: Layout, state: &PaintState<'_>) {
    unsafe {
        FillRect(
            hdc,
            &area_of(0, 0, area.panel.right, area.panel.bottom),
            gfx.bg,
        );
        FillRect(hdc, &area.stage, gfx.stage);
        if area.phone_panel.right > area.phone_panel.left {
            draw_phone(hdc, gfx, area.phone_panel, state.phone_url, state.phone_qr, state.phone_qr_width);
        }
        if state.show_image {
            if let Some((bitmap, width, height)) = state.image {
                draw_image(hdc, area.stage, bitmap, width, height);
            }
        } else if !state.show_video {
            draw_stage(hdc, gfx, area.stage, state.title, state.status, state.rows.is_empty());
        }
        if area.title_bar.bottom > area.title_bar.top {
            FillRect(hdc, &area.title_bar, gfx.stage);
            draw_wrapped(
                hdc,
                gfx.font,
                state.title,
                inset(area.title_bar, dp(16, gfx.dpi)),
                rgb(236, 236, 240),
            );
        }
        FillRect(hdc, &area.transport, gfx.transport);
        FillRect(hdc, &area.panel, gfx.bg);
        draw_button(hdc, gfx, area.prev, matches!(state.hot, Some(Hit::Prev)), false);
        draw_skip(hdc, area.prev, true);
        draw_button(hdc, gfx, area.play, matches!(state.hot, Some(Hit::Play)), true);
        if state.playing {
            draw_pause(hdc, area.play);
        } else {
            draw_play(hdc, area.play);
        }
        draw_button(hdc, gfx, area.next, matches!(state.hot, Some(Hit::Next)), false);
        draw_skip(hdc, area.next, false);
        let ratio = state.seek_override.unwrap_or_else(|| match state.duration {
            Some(duration) if duration.as_millis() > 0 => {
                (state.position.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
            }
            _ => 0.0,
        });
        draw_slider(hdc, gfx, area.seek, ratio, true);
        draw_slider(hdc, gfx, area.volume, state.volume, false);
        labeled(hdc, gfx, area.auto_btn, "Auto Next", state.auto_next, matches!(state.hot, Some(Hit::AutoNext)));
        labeled(hdc, gfx, area.loop_btn, "Loop", state.loop_playlist, matches!(state.hot, Some(Hit::Loop)));
        let time = match state.duration {
            Some(duration) => format!("{} / {}", fmt_time(state.position), fmt_time(duration)),
            None => format!("{} / --:--", fmt_time(state.position)),
        };
        text(
            hdc,
            gfx.font_small,
            &time,
            area.time,
            rgb(190, 194, 204),
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        labeled(hdc, gfx, area.up, "Up", false, matches!(state.hot, Some(Hit::Up)));
        labeled(hdc, gfx, area.back, "Back", false, matches!(state.hot, Some(Hit::Back)));
        labeled(hdc, gfx, area.phone_btn, "Phone", state.phone, matches!(state.hot, Some(Hit::Phone)));
        labeled(
            hdc,
            gfx,
            area.youtube,
            "YouTube",
            area.url.bottom > area.url.top,
            matches!(state.hot, Some(Hit::YouTube)),
        );
        text(
            hdc,
            gfx.font_small,
            if state.folder.is_empty() { "No folder open" } else { state.folder },
            area.path,
            rgb(190, 194, 204),
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_PATH_ELLIPSIS | DT_NOPREFIX,
        );
        labeled(
            hdc,
            gfx,
            area.filter_btn,
            "Filter",
            area.filter_all.bottom > area.filter_all.top || state.filter != Filter::All,
            matches!(state.hot, Some(Hit::Filters)),
        );
        if area.filter_all.bottom > area.filter_all.top {
            labeled(hdc, gfx, area.filter_all, "All", state.filter == Filter::All, matches!(state.hot, Some(Hit::Filter(Filter::All))));
            labeled(hdc, gfx, area.filter_mp3, "MP3", state.filter == Filter::Mp3, matches!(state.hot, Some(Hit::Filter(Filter::Mp3))));
            labeled(hdc, gfx, area.filter_video, "Video", state.filter == Filter::Video, matches!(state.hot, Some(Hit::Filter(Filter::Video))));
            labeled(hdc, gfx, area.filter_images, "Images", state.filter == Filter::Images, matches!(state.hot, Some(Hit::Filter(Filter::Images))));
        }
        labeled(hdc, gfx, area.newest_btn, "Newest", state.newest, matches!(state.hot, Some(Hit::Newest)));
        labeled(hdc, gfx, area.speed, state.speed_label, false, matches!(state.hot, Some(Hit::Speed)));
        labeled(hdc, gfx, area.top, "Top", state.topmost, matches!(state.hot, Some(Hit::Top)));
        draw_opacity_button(
            hdc,
            gfx,
            area.opacity_btn,
            area.opacity.bottom > area.opacity.top,
            matches!(state.hot, Some(Hit::OpacityBtn)),
        );
        if area.opacity.bottom > area.opacity.top {
            text(
                hdc,
                gfx.font_small,
                &format!("{}%", (45.0 + state.opacity.clamp(0.0, 1.0) * 55.0).round() as i32),
                area.opacity_label,
                rgb(190, 194, 204),
                DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            draw_slider(hdc, gfx, area.opacity, state.opacity, false);
        }
        if area.open_url.bottom > area.open_url.top {
            labeled(hdc, gfx, area.open_url, "Open", false, matches!(state.hot, Some(Hit::OpenUrl)));
        }
        draw_split(hdc, gfx, area.split);
        let saved = SaveDC(hdc);
        let _ = IntersectClipRect(hdc, area.list.left, area.list.top, area.list.right, area.list.bottom);
        let mut top = area.list.top - state.scroll;
        let step = line_height(hdc, gfx.font);
        for (index, row) in state.rows.iter().enumerate() {
            let height = state.heights.get(index).copied().unwrap_or(dp(32, gfx.dpi));
            let bottom = top + height;
            if bottom > area.list.top && top < area.list.bottom {
                let row_rect = RECT {
                    left: 0,
                    top,
                    right: area.list.right,
                    bottom,
                };
                if state.current == Some(index) {
                    FillRect(hdc, &row_rect, gfx.row);
                    FillRect(
                        hdc,
                        &RECT {
                            left: 0,
                            top,
                            right: dp(3, gfx.dpi),
                            bottom,
                        },
                        gfx.accent,
                    );
                }
                let name_rect = RECT {
                    left: dp(16, gfx.dpi),
                    top: top + dp(4, gfx.dpi),
                    right: area.list.right - dp(78, gfx.dpi),
                    bottom: bottom - dp(4, gfx.dpi),
                };
                draw_lines(hdc, gfx.font, &row.name, name_rect, step, rgb(236, 236, 240));
                text(
                    hdc,
                    gfx.font_small,
                    row.badge,
                    RECT {
                        left: area.list.right - dp(72, gfx.dpi),
                        top,
                        right: area.list.right - dp(12, gfx.dpi),
                        bottom: top + step + dp(8, gfx.dpi),
                    },
                    rgb(150, 156, 170),
                    DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
            }
            top = bottom;
            if top > area.list.bottom {
                break;
            }
        }
            if saved != 0 {
                let _ = RestoreDC(hdc, saved);
            }
    }
}

fn draw_phone(hdc: HDC, gfx: &Gfx, panel: RECT, url: &str, modules: &[bool], width: usize) {
    unsafe {
        FillRect(hdc, &panel, gfx.transport);
    }
    let pad = dp(12, gfx.dpi);
    if width > 0 && modules.len() >= width * width {
        let quiet = 2i32;
        let span = width as i32 + quiet * 2;
        let max_px = (panel.right - panel.left - pad * 2)
            .min(panel.bottom - panel.top - dp(72, gfx.dpi))
            .max(span);
        let cell = (max_px / span).max(2);
        let size = cell * span;
        let left = panel.left + ((panel.right - panel.left) - size) / 2;
        let top = panel.top + pad;
        unsafe {
            let brush = HBRUSH(GetStockObject(WHITE_BRUSH).0);
            FillRect(
                hdc,
                &RECT {
                    left,
                    top,
                    right: left + size,
                    bottom: top + size,
                },
                brush,
            );
        }
        for y in 0..width {
            for x in 0..width {
                if modules[y * width + x] {
                    let cell_rect = RECT {
                        left: left + (quiet + x as i32) * cell,
                        top: top + (quiet + y as i32) * cell,
                        right: left + (quiet + x as i32) * cell + cell,
                        bottom: top + (quiet + y as i32) * cell + cell,
                    };
                    unsafe {
                        FillRect(hdc, &cell_rect, gfx.bg);
                    }
                }
            }
        }
        draw_wrapped(
            hdc,
            gfx.font_small,
            url,
            RECT {
                left: panel.left + pad,
                top: top + size + dp(8, gfx.dpi),
                right: panel.right - pad,
                bottom: panel.bottom - pad,
            },
            rgb(236, 236, 240),
        );
    }
}

fn labeled(hdc: HDC, gfx: &Gfx, rect: RECT, label: &str, accent: bool, hot: bool) {
    draw_button(hdc, gfx, rect, hot, accent);
    text(
        hdc,
        gfx.font_small,
        label,
        inset(rect, dp(2, gfx.dpi)),
        rgb(230, 230, 235),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

fn draw_stage(hdc: HDC, gfx: &Gfx, stage: RECT, title: &str, status: &str, empty: bool) {
    if empty && status.is_empty() {
        text(
            hdc,
            gfx.font_bold,
            "Drop a folder or sound",
            RECT {
                left: stage.left + dp(24, gfx.dpi),
                top: stage.top + (stage.bottom - stage.top) / 2 - dp(36, gfx.dpi),
                right: stage.right - dp(24, gfx.dpi),
                bottom: stage.top + (stage.bottom - stage.top) / 2,
            },
            rgb(236, 236, 240),
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
        text(
            hdc,
            gfx.font,
            "Right-click a folder in Explorer and choose Play with Better Media Player",
            RECT {
                left: stage.left + dp(32, gfx.dpi),
                top: stage.top + (stage.bottom - stage.top) / 2 + dp(4, gfx.dpi),
                right: stage.right - dp(32, gfx.dpi),
                bottom: stage.top + (stage.bottom - stage.top) / 2 + dp(48, gfx.dpi),
            },
            rgb(160, 166, 178),
            DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
        );
        return;
    }
    let bounds = RECT {
        left: stage.left + dp(28, gfx.dpi),
        top: stage.top + dp(28, gfx.dpi),
        right: stage.right - dp(28, gfx.dpi),
        bottom: if status.is_empty() {
            stage.bottom - dp(16, gfx.dpi)
        } else {
            stage.bottom - dp(64, gfx.dpi)
        },
    };
    draw_wrapped(
        hdc,
        gfx.font_bold,
        if title.is_empty() { "Better Media Player" } else { title },
        bounds,
        rgb(236, 236, 240),
    );
    if !status.is_empty() {
        text(
            hdc,
            gfx.font,
            status,
            RECT {
                left: stage.left + dp(28, gfx.dpi),
                top: stage.bottom - dp(56, gfx.dpi),
                right: stage.right - dp(28, gfx.dpi),
                bottom: stage.bottom - dp(12, gfx.dpi),
            },
            rgb(255, 140, 130),
            DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
        );
    }
}

fn draw_split(hdc: HDC, gfx: &Gfx, area: RECT) {
    let grip_w = dp(36, gfx.dpi);
    let grip_h = dp(4, gfx.dpi);
    let cx = (area.left + area.right) / 2;
    let cy = (area.top + area.bottom) / 2;
    unsafe {
        let _ = FillRect(
            hdc,
            &RECT {
                left: cx - grip_w / 2,
                top: cy - grip_h / 2,
                right: cx + grip_w / 2,
                bottom: cy + grip_h / 2,
            },
            gfx.button,
        );
    }
}

fn draw_opacity_button(hdc: HDC, gfx: &Gfx, area: RECT, open: bool, hot: bool) {
    draw_button(hdc, gfx, area, hot, open);
    let pad = dp(7, gfx.dpi);
    let left = area.left + pad;
    let top = area.top + pad;
    let right = area.right - pad;
    let bottom = area.bottom - pad;
    if right <= left || bottom <= top {
        return;
    }
    let mid_x = left + (right - left) / 2;
    let mid_y = top + (bottom - top) / 2;
    unsafe {
        let light = GetStockObject(WHITE_BRUSH);
        let dark = gfx.bg;
        let _ = FillRect(hdc, &RECT { left, top, right: mid_x, bottom: mid_y }, HBRUSH(light.0));
        let _ = FillRect(hdc, &RECT { left: mid_x, top: mid_y, right, bottom }, HBRUSH(light.0));
        let _ = FillRect(hdc, &RECT { left: mid_x, top, right, bottom: mid_y }, dark);
        let _ = FillRect(hdc, &RECT { left, top: mid_y, right: mid_x, bottom }, dark);
    }
}

fn draw_button(hdc: HDC, gfx: &Gfx, rect: RECT, hot: bool, accent: bool) {
    let brush = if accent {
        gfx.accent
    } else if hot {
        gfx.button_hot
    } else {
        gfx.button
    };
    unsafe {
        FillRect(hdc, &rect, brush);
    }
}

fn draw_slider(hdc: HDC, gfx: &Gfx, rect: RECT, ratio: f32, accent: bool) {
    unsafe {
        FillRect(hdc, &rect, gfx.seek);
        let fill = RECT {
            left: rect.left,
            top: rect.top,
            right: rect.left + ((rect.right - rect.left) as f32 * ratio.clamp(0.0, 1.0)) as i32,
            bottom: rect.bottom,
        };
        FillRect(hdc, &fill, if accent { gfx.accent } else { gfx.button_hot });
    }
}

fn draw_play(hdc: HDC, rect: RECT) {
    let cx = (rect.left + rect.right) / 2 + dp_raw(1);
    let cy = (rect.top + rect.bottom) / 2;
    let s = (rect.bottom - rect.top) / 5;
    triangle(hdc, cx - s, cy - s, cx - s, cy + s, cx + s + 1, cy);
}

fn draw_pause(hdc: HDC, rect: RECT) {
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    let s = (rect.bottom - rect.top) / 5;
    let bar_w = (s / 2).max(2);
    fill_bar(
        hdc,
        RECT {
            left: cx - s,
            top: cy - s,
            right: cx - s + bar_w,
            bottom: cy + s,
        },
    );
    fill_bar(
        hdc,
        RECT {
            left: cx + s - bar_w,
            top: cy - s,
            right: cx + s,
            bottom: cy + s,
        },
    );
}

fn draw_skip(hdc: HDC, rect: RECT, backward: bool) {
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    let s = (rect.bottom - rect.top) / 5;
    if backward {
        triangle(hdc, cx + s, cy - s, cx + s, cy + s, cx - s, cy);
        fill_bar(hdc, RECT { left: cx - s - 2, top: cy - s, right: cx - s + 1, bottom: cy + s });
    } else {
        triangle(hdc, cx - s, cy - s, cx - s, cy + s, cx + s, cy);
        fill_bar(hdc, RECT { left: cx + s - 1, top: cy - s, right: cx + s + 2, bottom: cy + s });
    }
}

fn triangle(hdc: HDC, x1: i32, y1: i32, x2: i32, y2: i32, x3: i32, y3: i32) {
    let points = [
        windows::Win32::Foundation::POINT { x: x1, y: y1 },
        windows::Win32::Foundation::POINT { x: x2, y: y2 },
        windows::Win32::Foundation::POINT { x: x3, y: y3 },
    ];
    unsafe {
        let _ = SelectObject(hdc, GetStockObject(BLACK_PEN));
        let _ = SelectObject(hdc, GetStockObject(BLACK_BRUSH));
        let _ = Polygon(hdc, &points);
    }
}

fn fill_bar(hdc: HDC, rect: RECT) {
    unsafe {
        let brush = HBRUSH(GetStockObject(BLACK_BRUSH).0);
        let _ = FillRect(hdc, &rect, brush);
    }
}

fn dp_raw(px: i32) -> i32 {
    px
}

pub fn block_height(hdc: HDC, font: HFONT, text: &str, width: i32) -> i32 {
    let lines = wrap_lines(hdc, font, text, width);
    line_height(hdc, font) * lines.len().max(1) as i32
}

pub fn line_height(hdc: HDC, font: HFONT) -> i32 {
    unsafe {
        let previous = SelectObject(hdc, font.into());
        let mut metric = TEXTMETRICW::default();
        let _ = GetTextMetricsW(hdc, &mut metric);
        let _ = SelectObject(hdc, previous);
        metric.tmHeight.max(1)
    }
}

pub fn wrap_lines(hdc: HDC, font: HFONT, text: &str, width: i32) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    let width = width.max(8);
    unsafe {
        let previous = SelectObject(hdc, font.into());
        let mut lines = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            if text_width(hdc, rest) <= width {
                lines.push(rest.to_string());
                break;
            }
            let chars: Vec<(usize, char)> = rest.char_indices().collect();
            let mut lo = 1usize;
            let mut hi = chars.len();
            while lo < hi {
                let mid = (lo + hi + 1) / 2;
                let byte = chars[mid - 1].0 + chars[mid - 1].1.len_utf8();
                if text_width(hdc, &rest[..byte]) <= width {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            let byte_end = chars[lo - 1].0 + chars[lo - 1].1.len_utf8();
            let prefix = &rest[..byte_end];
            let mut split = byte_end;
            if let Some(rel) = prefix.rfind([' ', '-', '_', '.', '/', '\\']) {
                if rel > 0 {
                    let mark = prefix[rel..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(1);
                    split = rel + mark;
                }
            }
            let (line, tail) = rest.split_at(split);
            let shown = line.trim_end_matches(' ');
            if shown.is_empty() {
                let ch = rest.chars().next().unwrap();
                lines.push(ch.to_string());
                rest = &rest[ch.len_utf8()..];
            } else {
                lines.push(shown.to_string());
                rest = tail.trim_start_matches(' ');
            }
        }
        let _ = SelectObject(hdc, previous);
        lines
    }
}

fn text_width(hdc: HDC, text: &str) -> i32 {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return 0;
    }
    let mut size = SIZE::default();
    unsafe {
        let _ = GetTextExtentPoint32W(hdc, &wide, &mut size);
    }
    size.cx
}

fn draw_wrapped(hdc: HDC, font: HFONT, text: &str, bounds: RECT, color: COLORREF) {
    let width = (bounds.right - bounds.left).max(8);
    let lines = wrap_lines(hdc, font, text, width);
    draw_line_list(hdc, font, &lines, bounds, color);
}

fn draw_lines(hdc: HDC, font: HFONT, text: &str, bounds: RECT, _step: i32, color: COLORREF) {
    let width = (bounds.right - bounds.left).max(8);
    let lines = wrap_lines(hdc, font, text, width);
    draw_line_list(hdc, font, &lines, bounds, color);
}

fn draw_line_list(hdc: HDC, font: HFONT, lines: &[String], bounds: RECT, color: COLORREF) {
    let step = line_height(hdc, font);
    let mut top = bounds.top;
    for line in lines {
        if top >= bounds.bottom {
            break;
        }
        text(
            hdc,
            font,
            line,
            RECT {
                left: bounds.left,
                top,
                right: bounds.right,
                bottom: (top + step).min(bounds.bottom + step),
            },
            color,
            DT_LEFT | DT_SINGLELINE | DT_NOPREFIX | DT_VCENTER,
        );
        top += step;
    }
}

fn draw_image(hdc: HDC, stage: RECT, bitmap: HBITMAP, width: i32, height: i32) {
    if width <= 0 || height <= 0 {
        return;
    }
    let dest_w = (stage.right - stage.left).max(1);
    let dest_h = (stage.bottom - stage.top).max(1);
    let scale = (dest_w as f32 / width as f32).min(dest_h as f32 / height as f32).min(1.0);
    let draw_w = (width as f32 * scale) as i32;
    let draw_h = (height as f32 * scale) as i32;
    let x = stage.left + (dest_w - draw_w) / 2;
    let y = stage.top + (dest_h - draw_h) / 2;
    unsafe {
        let memory = windows::Win32::Graphics::Gdi::CreateCompatibleDC(Some(hdc));
        if memory.0.is_null() {
            return;
        }
        let previous = SelectObject(memory, bitmap.into());
        SetStretchBltMode(hdc, COLORONCOLOR);
        let _ = StretchBlt(hdc, x, y, draw_w.max(1), draw_h.max(1), Some(memory), 0, 0, width, height, SRCCOPY);
        let _ = SelectObject(memory, previous);
        let _ = windows::Win32::Graphics::Gdi::DeleteDC(memory);
    }
}

fn text(hdc: HDC, font: HFONT, value: &str, mut rect: RECT, color: COLORREF, format: DRAW_TEXT_FORMAT) {
    let mut wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let previous = SelectObject(hdc, font.into());
        SetTextColor(hdc, color);
        SetBkMode(hdc, TRANSPARENT);
        let _ = DrawTextW(hdc, &mut wide, &mut rect, format);
        let _ = SelectObject(hdc, previous);
    }
}

fn inset(rect: RECT, pad: i32) -> RECT {
    RECT {
        left: rect.left + pad,
        top: rect.top,
        right: rect.right - pad,
        bottom: rect.bottom,
    }
}

fn area_of(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT { left, top, right, bottom }
}

pub fn fmt_time(duration: Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

