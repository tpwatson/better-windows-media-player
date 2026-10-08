use crate::images::Frame;
use crate::lan;
use crate::player::{Player, PlayerNotice, WM_MF_EVENT, WM_TRACK_ENDED};
use crate::playlist::{self, entry_name, list_dir, paths_equal, Filter, Kind, Order};
use crate::remote::{self, WM_URL_READY};
use crate::settings::Settings;
use crate::ui::{self, Gfx, Hit, PaintState, RowPaint};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
    DeleteObject, EndPaint, GetDC, GetStockObject, ReleaseDC, SelectObject, SetBkColor, SetTextColor,
    BLACK_BRUSH, HBRUSH, HDC, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, VK_DOWN, VK_END, VK_HOME, VK_LEFT,
    VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_RETURN, VK_RIGHT, VK_SPACE,
    VK_UP, TRACKMOUSEEVENT, TME_LEAVE,
};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, HDROP};
use windows::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    DestroyIcon, LoadCursorW, LoadIconW, LoadImageW, MoveWindow, PostMessageW, PostQuitMessage,
    RegisterClassW, SendMessageW, SetCursor, SetLayeredWindowAttributes, SetTimer,
    SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, CS_DBLCLKS,
    CS_HREDRAW, CS_VREDRAW,
    ES_AUTOHSCROLL, GWL_EXSTYLE, GWLP_USERDATA, GWLP_WNDPROC, HWND_NOTOPMOST, HWND_TOPMOST,
    HICON, ICON_BIG, ICON_SMALL, IDC_ARROW, IDC_SIZENS, IDI_APPLICATION, IMAGE_FLAGS, IMAGE_ICON,
    LR_DEFAULTSIZE,
    LWA_ALPHA,
    MINMAXINFO, MSG, SM_CXICON, SM_CXSMICON, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SW_HIDE, SW_SHOW, SW_SHOWDEFAULT, WINDOWPLACEMENT, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_APP, WM_APPCOMMAND, WM_CHAR, WM_COPYDATA, WM_CREATE, WM_CTLCOLOREDIT, WM_DESTROY, WM_DPICHANGED,
    WM_DROPFILES, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_SETCURSOR, WM_SETICON,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETFONT, WM_SIZE, WM_TIMER, WNDCLASSW, WNDPROC, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
    WS_EX_LAYERED, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};

const WM_HOLD: u32 = WM_APP + 13;
const WM_OPEN_URL: u32 = WM_APP + 14;
const SPEEDS: [f32; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0];
static URL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
const APPCOMMAND_MEDIA_NEXTTRACK: u16 = 11;
const APPCOMMAND_MEDIA_PREVIOUSTRACK: u16 = 12;
const APPCOMMAND_MEDIA_STOP: u16 = 13;
const APPCOMMAND_MEDIA_PLAY_PAUSE: u16 = 14;

struct Slot(RefCell<App>);

pub struct App {
    player: Player,
    video: HWND,
    url_edit: HWND,
    edit_brush: HBRUSH,
    entries: Vec<playlist::Entry>,
    browser_dir: Option<PathBuf>,
    nav_back: Vec<PathBuf>,
    current: Option<usize>,
    scroll_px: i32,
    row_heights: Vec<i32>,
    metrics_key: i32,
    title_px: i32,
    title_name: String,
    page_url: Option<String>,
    url_gen: u64,
    status: String,
    failures: u32,
    auto_next: bool,
    loop_playlist: bool,
    filter: Filter,
    order: Order,
    speed: f32,
    topmost: bool,
    opacity: u8,
    phone: Option<lan::Server>,
    phone_token: String,
    phone_port: u16,
    phone_url: String,
    url_open: bool,
    filters_open: bool,
    opacity_open: bool,
    url_focus: bool,
    list_px: i32,
    list_drag: Option<(i32, i32)>,
    phone_qr: Vec<bool>,
    phone_qr_width: usize,
    show_video: bool,
    image: Option<Frame>,
    gfx: Gfx,
    hot: Option<Hit>,
    seek_drag: Option<f32>,
    volume_drag: bool,
    opacity_drag: bool,
    holding: bool,
    tracking: bool,
    icon_big: Option<HICON>,
    icon_small: Option<HICON>,
}

struct Launch {
    pub settings: Settings,
}

pub fn run(prepare: impl FnOnce(HWND)) -> windows_core::Result<()> {
    let settings = Settings::load();
    register_classes()?;
    let mut launch = Launch { settings: settings.clone() };
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows_core::w!("BetterMediaPlayer"),
            windows_core::w!("Better Media Player"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VISIBLE,
            if settings.placed { settings.x } else { 80 },
            if settings.placed { settings.y } else { 80 },
            settings.width,
            settings.height,
            None,
            None,
            Some(module()?),
            Some(&mut launch as *mut Launch as *mut _),
        )?
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWDEFAULT);
    }
    prepare(hwnd);
    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if status.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn load_app_icon(px: i32) -> windows_core::Result<HICON> {
    let flags = if px == 0 { LR_DEFAULTSIZE } else { IMAGE_FLAGS(0) };
    let handle = unsafe {
        LoadImageW(
            Some(module()?),
            windows_core::PCWSTR(1u32 as _),
            IMAGE_ICON,
            px,
            px,
            flags,
        )?
    };
    Ok(HICON(handle.0))
}

fn apply_window_icons(hwnd: HWND) {
    let dpi = dpi_of(hwnd) as u32;
    let big_px = unsafe { GetSystemMetricsForDpi(SM_CXICON, dpi) }.max(16);
    let small_px = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) }.max(16);
    let big = load_app_icon(big_px).ok();
    let small = load_app_icon(small_px).ok();
    with_app(hwnd, |app| {
        if let Some(icon) = big {
            let _ = unsafe {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_BIG as usize)),
                    Some(LPARAM(icon.0 as isize)),
                )
            };
            drop_icon(app.icon_big.replace(icon));
        }
        if let Some(icon) = small {
            let _ = unsafe {
                SendMessageW(
                    hwnd,
                    WM_SETICON,
                    Some(WPARAM(ICON_SMALL as usize)),
                    Some(LPARAM(icon.0 as isize)),
                )
            };
            drop_icon(app.icon_small.replace(icon));
        }
    });
}

fn drop_icon(icon: Option<HICON>) {
    if let Some(icon) = icon {
        let _ = unsafe { DestroyIcon(icon) };
    }
}

fn module() -> windows_core::Result<windows::Win32::Foundation::HINSTANCE> {
    let module = unsafe { GetModuleHandleW(None)? };
    Ok(windows::Win32::Foundation::HINSTANCE(module.0))
}

fn register_classes() -> windows_core::Result<()> {
    let instance = module()?;
    let icon = load_app_icon(0).or_else(|_| unsafe { LoadIconW(None, IDI_APPLICATION) })?;
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW)? };
    let main = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: icon,
        hCursor: cursor,
        hbrBackground: unsafe { HBRUSH(GetStockObject(BLACK_BRUSH).0) },
        lpszMenuName: windows_core::PCWSTR::null(),
        lpszClassName: windows_core::w!("BetterMediaPlayer"),
    };
    let video = WNDCLASSW {
        style: CS_DBLCLKS,
        lpfnWndProc: Some(video_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: icon,
        hCursor: cursor,
        hbrBackground: unsafe { HBRUSH(GetStockObject(BLACK_BRUSH).0) },
        lpszMenuName: windows_core::PCWSTR::null(),
        lpszClassName: windows_core::w!("BetterMediaPlayerVideo"),
    };
    unsafe {
        RegisterClassW(&main);
        RegisterClassW(&video);
    }
    Ok(())
}

pub fn attach_audio_and_open(
    hwnd: HWND,
    audio: Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    target: Option<PathBuf>,
) {
    mutate(hwnd, |app| {
        if let Some((stream, handle)) = audio {
            app.player.attach_output(stream, handle);
        }
        if let Some(target) = target {
            app.open_target(target);
        }
    });
}

pub fn find_class_name() -> windows_core::PCWSTR {
    windows_core::w!("BetterMediaPlayer")
}

fn mutate(hwnd: HWND, f: impl FnOnce(&mut App)) {
    with_app(hwnd, f);
    sync_window(hwnd);
}

fn with_app<R>(hwnd: HWND, f: impl FnOnce(&mut App) -> R) -> Option<R> {
    let pointer = window_slot(hwnd);
    if pointer.is_null() {
        return None;
    }
    let slot = unsafe { &*(pointer as *const Slot) };
    let mut app = slot.0.borrow_mut();
    Some(f(&mut app))
}

fn window_slot(hwnd: HWND) -> *mut Slot {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Slot }
}

fn sync_window(hwnd: HWND) {
    let Some(snap) = with_app(hwnd, |app| {
        let hdc = unsafe { GetDC(Some(hwnd)) };
        if !hdc.0.is_null() {
            app.ensure_metrics(hwnd, hdc);
            unsafe {
                let _ = ReleaseDC(Some(hwnd), hdc);
            }
        }
        app.sync_phone();
        let title = app.window_title();
        let show_video = app.show_video;
        let video = app.video;
        (title, show_video, video)
    }) else {
        return;
    };
    let (title, show_video, video) = snap;
    let wide = wide(&title);
    unsafe {
        let _ = SetWindowTextW(hwnd, windows_core::PCWSTR(wide.as_ptr()));
        let _ = ShowWindow(video, if show_video { SW_SHOW } else { SW_HIDE });
    }
    reposition_url(hwnd);
    reposition_video(hwnd);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, false);
    }
}

fn reposition_video(hwnd: HWND) {
    let Some((video, show, x, y, w, h)) = with_app(hwnd, |app| {
        let (width, height) = client_size(hwnd);
        let layout = app.layout_for(hwnd, width, height);
        (
            app.video,
            app.show_video,
            layout.stage.left,
            layout.stage.top,
            layout.stage.right - layout.stage.left,
            layout.stage.bottom - layout.stage.top,
        )
    }) else {
        return;
    };
    if show {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::MoveWindow(video, x, y, w.max(0), h.max(0), true);
        }
    }
    with_app(hwnd, |app| app.player.set_video_size(w, h));
}

fn reposition_url(hwnd: HWND) {
    let Some((url_edit, url, open, focus)) = with_app(hwnd, |app| {
        let (width, height) = client_size(hwnd);
        let layout = app.layout_for(hwnd, width, height);
        let focus = app.url_focus;
        app.url_focus = false;
        (app.url_edit, layout.url, app.url_open, focus)
    }) else {
        return;
    };
    if url_edit.0.is_null() {
        return;
    }
    unsafe {
        if open {
            let _ = ShowWindow(url_edit, SW_SHOW);
            let _ = MoveWindow(
                url_edit,
                url.left,
                url.top,
                (url.right - url.left).max(1),
                (url.bottom - url.top).max(1),
                true,
            );
            if focus {
                let _ = SetFocus(Some(url_edit));
            }
        } else {
            let _ = ShowWindow(url_edit, SW_HIDE);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let create = &*(lparam.0 as *const windows::Win32::UI::WindowsAndMessaging::CREATESTRUCTW);
            let launch = &*(create.lpCreateParams as *const Launch);
            let video = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows_core::w!("BetterMediaPlayerVideo"),
                windows_core::w!(""),
                WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                0,
                0,
                Some(hwnd),
                None,
                Some(module().unwrap_or(windows::Win32::Foundation::HINSTANCE(
                    std::ptr::null_mut(),
                ))),
                None,
            )
            .unwrap_or(HWND(std::ptr::null_mut()));
            let dpi = dpi_of(hwnd);
            let url_edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows_core::w!("EDIT"),
                windows_core::w!(""),
                WS_CHILD | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                0,
                0,
                0,
                0,
                Some(hwnd),
                None,
                Some(module().unwrap_or(windows::Win32::Foundation::HINSTANCE(
                    std::ptr::null_mut(),
                ))),
                None,
            )
            .unwrap_or(HWND(std::ptr::null_mut()));
            let gfx = Gfx::new(dpi);
            if !url_edit.0.is_null() {
                let previous = SetWindowLongPtrW(url_edit, GWLP_WNDPROC, url_proc as isize);
                URL_WNDPROC.store(previous, Ordering::Relaxed);
                let cue = windows_core::w!("Paste a YouTube or media URL and press Enter");
                let _ = SendMessageW(url_edit, 0x1501, Some(WPARAM(1)), Some(LPARAM(cue.0 as isize)));
                let _ = SendMessageW(
                    url_edit,
                    WM_SETFONT,
                    Some(WPARAM(gfx.font.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
            let mut app = App {
                player: Player::new(hwnd, video),
                video,
                url_edit,
                edit_brush: CreateSolidBrush(ui::rgb(32, 32, 36)),
                entries: Vec::new(),
                browser_dir: None,
                nav_back: Vec::new(),
                current: None,
                scroll_px: 0,
                row_heights: Vec::new(),
                metrics_key: -1,
                title_px: 0,
                title_name: String::new(),
                page_url: None,
                url_gen: 0,
                status: String::new(),
                failures: 0,
                auto_next: launch.settings.auto_next,
                loop_playlist: launch.settings.loop_playlist,
                filter: Filter::parse(&launch.settings.filter),
                order: Order::parse(&launch.settings.order),
                speed: launch.settings.speed,
                topmost: launch.settings.topmost,
                opacity: launch.settings.opacity,
                phone: None,
                phone_token: launch.settings.phone_token.clone(),
                phone_port: launch.settings.phone_port,
                phone_url: String::new(),
                url_open: false,
                filters_open: false,
                opacity_open: false,
                url_focus: false,
                list_px: launch.settings.list_px,
                list_drag: None,
                phone_qr: Vec::new(),
                phone_qr_width: 0,
                show_video: false,
                image: None,
                gfx,
                hot: None,
                seek_drag: None,
                volume_drag: false,
                opacity_drag: false,
                holding: false,
                tracking: false,
                icon_big: None,
                icon_small: None,
            };
            app.player.set_volume(launch.settings.volume);
            app.player.set_speed(launch.settings.speed);
            let slot = Box::new(Slot(RefCell::new(app)));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(slot) as isize);
            let dark = 1i32;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const i32 as *const _,
                4,
            );
            apply_chrome(hwnd, launch.settings.topmost, launch.settings.opacity);
            let resume_phone = launch.settings.phone;
            let _ = SetTimer(Some(hwnd), 1, 100, None);
            DragAcceptFiles(hwnd, true);
            if resume_phone {
                with_app(hwnd, |app| app.start_phone());
            }
            apply_window_icons(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            save_window(hwnd);
            with_app(hwnd, |app| {
                app.stop_phone();
                drop_icon(app.icon_big.take());
                drop_icon(app.icon_small.take());
            });
            if let Some(brush) = with_app(hwnd, |app| app.edit_brush) {
                let _ = DeleteObject(brush.into());
            }
            let pointer = SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) as *mut Slot;
            if !pointer.is_null() {
                drop(Box::from_raw(pointer));
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE | WM_DPICHANGED => {
            if msg == WM_DPICHANGED {
                let suggested = *(lparam.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                let dpi = dpi_of(hwnd);
                with_app(hwnd, |app| {
                    app.gfx = Gfx::new(dpi);
                    app.metrics_key = -1;
                    if !app.url_edit.0.is_null() {
                        let _ = SendMessageW(
                            app.url_edit,
                            WM_SETFONT,
                            Some(WPARAM(app.gfx.font.0 as usize)),
                            Some(LPARAM(1)),
                        );
                    }
                });
                apply_window_icons(hwnd);
            }
            reposition_video(hwnd);
            reposition_url(hwnd);
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, false);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            let sizing = with_app(hwnd, |app| {
                app.list_drag.is_some() || matches!(app.hot, Some(Hit::Split))
            })
            .unwrap_or(false);
            if sizing {
                if let Ok(cursor) = unsafe { LoadCursorW(None, IDC_SIZENS) } {
                    unsafe {
                        SetCursor(Some(cursor));
                    }
                }
                return LRESULT(1);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_PAINT => {
            paint_window(hwnd);
            LRESULT(0)
        }
        WM_TIMER => {
            let _ = unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = mouse_xy(lparam);
            let track = with_app(hwnd, |app| {
                let start = !app.tracking;
                app.tracking = true;
                app.hover(x, y, hwnd);
                start
            })
            .unwrap_or(false);
            if with_app(hwnd, |app| app.list_drag.is_some()).unwrap_or(false) {
                reposition_video(hwnd);
                reposition_url(hwnd);
            }
            if track {
                let mut event = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe {
                    let _ = TrackMouseEvent(&mut event);
                }
            }
            let _ = unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_app(hwnd, |app| {
                app.hot = None;
                app.tracking = false;
            });
            let _ = unsafe { windows::Win32::Graphics::Gdi::InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = mouse_xy(lparam);
            mutate(hwnd, |app| app.mouse_down(hwnd, x, y));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = mouse_xy(lparam);
            mutate(hwnd, |app| app.mouse_up(x, y, hwnd));
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => LRESULT(0),
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
            mutate(hwnd, |app| app.wheel(hwnd, delta));
            LRESULT(0)
        }
        WM_KEYDOWN => {
            mutate(hwnd, |app| app.key(hwnd, wparam.0 as u16));
            LRESULT(0)
        }
        WM_APPCOMMAND => {
            let command = ((lparam.0 >> 16) & 0xffff) as u16;
            let handled = with_app(hwnd, |app| app.app_command(hwnd, command)).unwrap_or(false);
            if handled {
                sync_window(hwnd);
                LRESULT(1)
            } else {
                unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
            }
        }
        WM_COPYDATA => {
            if let Some(path) = path_from_copydata(lparam) {
                mutate(hwnd, |app| app.open_target(path));
                unsafe {
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
                }
            }
            LRESULT(1)
        }
        WM_DROPFILES => {
            let paths = drop_paths(HDROP(wparam.0 as *mut _));
            unsafe { DragFinish(HDROP(wparam.0 as *mut _)) };
            mutate(hwnd, |app| app.open_dropped(paths));
            LRESULT(0)
        }
        WM_TRACK_ENDED => {
            let generation = wparam.0 as u64;
            mutate(hwnd, |app| {
                if app.player.on_track_ended(generation) {
                    app.auto_advance();
                }
            });
            LRESULT(0)
        }
        WM_MF_EVENT => {
            mutate(hwnd, |app| app.on_mf(wparam.0 as u32, lparam.0));
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => {
            let hdc = HDC(wparam.0 as *mut _);
            unsafe {
                let _ = SetTextColor(hdc, ui::rgb(236, 236, 240));
                let _ = SetBkColor(hdc, COLORREF(36 | (36 << 8) | (40 << 16)));
            }
            let brush = with_app(hwnd, |app| app.edit_brush).unwrap_or_default();
            LRESULT(brush.0 as isize)
        }
        WM_HOLD => {
            mutate(hwnd, |app| {
                let held = wparam.0 != 0;
                app.holding = held;
                app.player.set_boost(held);
            });
            LRESULT(0)
        }
        WM_URL_READY => {
            mutate(hwnd, |app| app.finish_url(wparam.0 as u64));
            LRESULT(0)
        }
        WM_OPEN_URL => {
            let text = edit_text(with_app(hwnd, |app| app.url_edit).unwrap_or(HWND(std::ptr::null_mut())));
            mutate(hwnd, |app| app.submit_url(hwnd, text));
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let info = &mut *(lparam.0 as *mut MINMAXINFO);
            let dpi = dpi_of(hwnd).max(96);
            info.ptMinTrackSize = POINT {
                x: 520 * dpi / 96,
                y: 360 * dpi / 96,
            };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn video_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_LBUTTONDOWN => {
            if let Ok(parent) = windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) {
                let _ = SetFocus(Some(parent));
                let _ = SetCapture(hwnd);
                let _ = PostMessageW(Some(parent), WM_HOLD, WPARAM(1), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if let Ok(parent) = windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) {
                let _ = ReleaseCapture();
                let _ = PostMessageW(Some(parent), WM_HOLD, WPARAM(0), LPARAM(0));
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => LRESULT(0),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn url_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_KEYDOWN && wparam.0 == VK_RETURN.0 as usize {
        if let Ok(parent) = windows::Win32::UI::WindowsAndMessaging::GetParent(hwnd) {
            let _ = PostMessageW(Some(parent), WM_OPEN_URL, WPARAM(0), LPARAM(0));
        }
        return LRESULT(0);
    }
    if msg == WM_CHAR && (wparam.0 == 13 || wparam.0 == 10) {
        return LRESULT(0);
    }
    let previous = URL_WNDPROC.load(Ordering::Relaxed);
    if previous == 0 {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let proc: WNDPROC = std::mem::transmute(previous);
    CallWindowProcW(proc, hwnd, msg, wparam, lparam)
}

impl App {
    fn media_open(&self) -> bool {
        self.show_video || self.image.is_some()
    }

    fn open_target(&mut self, path: PathBuf) {
        self.failures = 0;
        if path.is_dir() {
            self.enter_dir(path, true);
            self.activate_first(true);
            if self.current.is_none() && self.entries.iter().all(|entry| entry.kind == Kind::Folder) {
                self.status.clear();
            } else if self.current.is_none() {
                self.status = "No matching media in this folder".into();
                self.show_video = false;
            }
            return;
        }
        if !path.is_file() {
            self.status = format!("Not found: {}", path.display());
            return;
        }
        if let Some(dir) = path.parent() {
            if self.browser_dir.as_ref().is_none_or(|current| !paths_equal(current, dir)) {
                self.enter_dir(dir.to_path_buf(), self.browser_dir.is_some());
            } else {
                self.reload_listing();
            }
        }
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| paths_equal(&entry.path, &path))
        {
            self.activate(index, true);
        } else {
            self.play_path(path);
        }
    }

    fn open_dropped(&mut self, paths: Vec<PathBuf>) {
        if paths.len() == 1 {
            self.open_target(paths.into_iter().next().unwrap());
            return;
        }
        let mut files = Vec::new();
        for path in paths {
            if path.is_dir() {
                files.extend(list_dir(&path, self.filter, self.order).into_iter().filter(|entry| entry.kind != Kind::Folder));
            } else if path.is_file() {
                let kind = if playlist::is_image(&path) {
                    Kind::Image
                } else if playlist::is_video(&path) {
                    Kind::Video
                } else if playlist::is_audio(&path) {
                    Kind::Audio
                } else {
                    continue;
                };
                files.push(playlist::Entry { path, kind });
            }
        }
        if files.is_empty() {
            self.status = "Nothing in that drop can be opened".into();
            return;
        }
        self.entries = files;
        self.metrics_key = -1;
        self.scroll_px = 0;
        self.current = None;
        self.activate_first(true);
    }

    fn enter_dir(&mut self, dir: PathBuf, record: bool) {
        if record {
            if let Some(current) = self.browser_dir.clone() {
                if !paths_equal(&current, &dir) {
                    self.nav_back.push(current);
                }
            }
        }
        self.browser_dir = Some(dir);
        self.reload_listing();
        self.scroll_px = 0;
    }

    fn reload_listing(&mut self) {
        let Some(dir) = self.browser_dir.clone() else {
            return;
        };
        let playing = self
            .current
            .and_then(|index| self.entries.get(index))
            .map(|entry| entry.path.clone());
        self.entries = list_dir(&dir, self.filter, self.order);
        self.metrics_key = -1;
        self.current = playing.and_then(|path| {
            self.entries
                .iter()
                .position(|entry| paths_equal(&entry.path, &path))
        });
        self.clamp_scroll(10_000);
    }

    fn activate_first(&mut self, user: bool) {
        if let Some(index) = self.entries.iter().position(|entry| entry.kind != Kind::Folder) {
            self.activate(index, user);
        }
    }

    fn activate(&mut self, index: usize, user: bool) {
        let Some(entry) = self.entries.get(index).cloned() else {
            return;
        };
        match entry.kind {
            Kind::Folder => self.enter_dir(entry.path, true),
            Kind::Image => self.show_still(index, entry.path),
            Kind::Audio | Kind::Video => self.play_entry(index, entry.path, user),
        }
    }

    fn play_path(&mut self, path: PathBuf) {
        self.page_url = None;
        self.image = None;
        self.title_name = entry_name(&path);
        self.show_video = playlist::is_video(&path);
        self.metrics_key = -1;
        match self.player.play_file(&path) {
            Ok(()) => self.status.clear(),
            Err(error) => self.note_failure(error),
        }
    }

    fn play_entry(&mut self, index: usize, path: PathBuf, user: bool) {
        if user {
            self.failures = 0;
        }
        self.current = Some(index);
        self.page_url = None;
        self.image = None;
        self.title_name = entry_name(&path);
        self.show_video = playlist::is_video(&path);
        self.metrics_key = -1;
        self.reveal_current();
        match self.player.play_file(&path) {
            Ok(()) => self.status.clear(),
            Err(error) => self.note_failure(error),
        }
    }

    fn show_still(&mut self, index: usize, path: PathBuf) {
        self.current = Some(index);
        self.page_url = None;
        self.player.stop_playback();
        self.show_video = false;
        self.title_name = entry_name(&path);
        self.metrics_key = -1;
        self.reveal_current();
        match crate::images::load(&path) {
            Ok(frame) => {
                self.image = Some(frame);
                self.status.clear();
            }
            Err(error) => {
                self.image = None;
                self.status = error;
            }
        }
    }

    fn note_failure(&mut self, mut error: String) {
        self.failures += 1;
        self.status = error.clone();
        self.show_video = false;
        self.image = None;
        let start = self.current.unwrap_or(0);
        for index in (start + 1)..self.entries.len() {
            if self.failures >= self.entries.len() as u32 {
                break;
            }
            let Some(entry) = self.entries.get(index).cloned() else {
                break;
            };
            if entry.kind == Kind::Folder || entry.kind == Kind::Image {
                continue;
            }
            self.current = Some(index);
            self.title_name = entry_name(&entry.path);
            self.show_video = playlist::is_video(&entry.path);
            self.reveal_current();
            match self.player.play_file(&entry.path) {
                Ok(()) => {
                    self.status.clear();
                    return;
                }
                Err(next_error) => {
                    self.failures += 1;
                    error = next_error;
                    self.status = error.clone();
                    self.show_video = false;
                }
            }
        }
    }

    fn auto_advance(&mut self) {
        if self.auto_next {
            self.advance();
        }
    }

    fn advance(&mut self) {
        self.failures = 0;
        let start = self.current.map(|index| index + 1).unwrap_or(0);
        if let Some(index) = self.next_playable(start) {
            self.activate(index, false);
            return;
        }
        if self.loop_playlist {
            if let Some(index) = self.next_playable(0) {
                self.activate(index, false);
            }
        }
    }

    fn previous(&mut self) {
        if self.image.is_none() && self.player.position().as_secs() >= 3 {
            self.player.seek_ratio(0.0);
            return;
        }
        let start = self.current.unwrap_or(0);
        if start > 0 {
            if let Some(index) = self.previous_playable(start - 1) {
                self.activate(index, true);
                return;
            }
        }
        self.player.seek_ratio(0.0);
    }

    fn next_playable(&self, start: usize) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .skip(start)
            .find(|(_, entry)| entry.kind != Kind::Folder)
            .map(|(index, _)| index)
    }

    fn previous_playable(&self, start: usize) -> Option<usize> {
        self.entries
            .iter()
            .enumerate()
            .take(start + 1)
            .rev()
            .find(|(_, entry)| entry.kind != Kind::Folder)
            .map(|(index, _)| index)
    }

    fn hover(&mut self, x: i32, y: i32, hwnd: HWND) {
        let layout = self.view(hwnd);
        self.hot = layout.hit(x, y, self.scroll_px, &self.row_heights);
        if self.seek_drag.is_some() {
            self.seek_drag = Some(layout.ratio_in(layout.seek, x));
        }
        if self.volume_drag {
            self.player.set_volume(layout.ratio_in(layout.volume, x));
        }
        if self.opacity_drag {
            self.set_opacity_ratio(hwnd, layout.ratio_in(layout.opacity, x));
        }
        if let Some((origin_y, origin_h)) = self.list_drag {
            self.list_px = origin_h + (origin_y - y);
        }
    }

    fn mouse_down(&mut self, hwnd: HWND, x: i32, y: i32) {
        let layout = self.view(hwnd);
        match layout.hit(x, y, self.scroll_px, &self.row_heights) {
            Some(Hit::Seek) => {
                self.seek_drag = Some(layout.ratio_in(layout.seek, x));
                let _ = unsafe { SetCapture(hwnd) };
            }
            Some(Hit::Volume) => {
                self.volume_drag = true;
                self.player.set_volume(layout.ratio_in(layout.volume, x));
                let _ = unsafe { SetCapture(hwnd) };
            }
            Some(Hit::Opacity) => {
                self.opacity_drag = true;
                self.set_opacity_ratio(hwnd, layout.ratio_in(layout.opacity, x));
                let _ = unsafe { SetCapture(hwnd) };
            }
            Some(Hit::Split) => {
                self.list_drag = Some((y, layout.list_height()));
                let _ = unsafe { SetCapture(hwnd) };
            }
            Some(Hit::Stage) => {
                self.holding = true;
                self.player.set_boost(true);
                let _ = unsafe { SetCapture(hwnd) };
            }
            _ => {}
        }
    }

    fn mouse_up(&mut self, x: i32, y: i32, hwnd: HWND) {
        let layout = self.view(hwnd);
        if let Some(ratio) = self.seek_drag.take() {
            self.player.seek_ratio(ratio);
            let _ = unsafe { ReleaseCapture() };
            return;
        }
        if self.volume_drag {
            self.volume_drag = false;
            let _ = unsafe { ReleaseCapture() };
            return;
        }
        if self.opacity_drag {
            self.opacity_drag = false;
            let _ = unsafe { ReleaseCapture() };
            return;
        }
        if self.holding {
            self.holding = false;
            self.player.set_boost(false);
            let _ = unsafe { ReleaseCapture() };
            return;
        }
        if self.list_drag.take().is_some() {
            self.list_px = layout.list_height();
            let _ = unsafe { ReleaseCapture() };
            return;
        }
        match layout.hit(x, y, self.scroll_px, &self.row_heights) {
            Some(Hit::Play) => self.toggle_playback(hwnd),
            Some(Hit::Next) => self.advance(),
            Some(Hit::Prev) => self.previous(),
            Some(Hit::AutoNext) => self.auto_next = !self.auto_next,
            Some(Hit::Loop) => self.loop_playlist = !self.loop_playlist,
            Some(Hit::Up) => self.go_up(),
            Some(Hit::Back) => self.go_back(),
            Some(Hit::Speed) => {
                self.speed = cycle_speed(self.speed);
                self.player.set_speed(self.speed);
            }
            Some(Hit::Top) => {
                self.topmost = !self.topmost;
                apply_chrome(hwnd, self.topmost, self.opacity);
            }
            Some(Hit::Phone) => {
                if self.phone.is_some() {
                    self.stop_phone();
                    self.status = "Phone link is off.".into();
                } else {
                    self.start_phone();
                }
            }
            Some(Hit::Filters) => self.filters_open = !self.filters_open,
            Some(Hit::Newest) => {
                self.order = if self.order == Order::Newest {
                    Order::Name
                } else {
                    Order::Newest
                };
                self.reload_listing();
            }
            Some(Hit::OpacityBtn) => self.opacity_open = !self.opacity_open,
            Some(Hit::YouTube) => {
                self.url_open = !self.url_open;
                self.url_focus = self.url_open;
            }
            Some(Hit::OpenUrl) => self.submit_url(hwnd, edit_text(self.url_edit)),
            Some(Hit::Filter(filter)) => {
                self.filter = filter;
                self.reload_listing();
            }
            Some(Hit::Row(index)) => self.activate(index, true),
            _ => {}
        }
    }

    fn go_up(&mut self) {
        let Some(dir) = self.browser_dir.clone() else {
            return;
        };
        if let Some(parent) = dir.parent() {
            if parent != dir {
                self.enter_dir(parent.to_path_buf(), true);
            }
        }
    }

    fn go_back(&mut self) {
        if let Some(dir) = self.nav_back.pop() {
            self.browser_dir = Some(dir);
            self.reload_listing();
            self.scroll_px = 0;
        }
    }

    fn wheel(&mut self, hwnd: HWND, delta: i32) {
        let layout = self.view(hwnd);
        let steps = -delta / 120;
        self.scroll_px += steps * ui::dp(48, dpi_of(hwnd));
        self.clamp_scroll(layout.list_height());
    }

    fn key(&mut self, hwnd: HWND, key: u16) {
        match key {
            value if value == VK_SPACE.0 => self.toggle_playback(hwnd),
            value if value == VK_MEDIA_PLAY_PAUSE.0 => self.toggle_playback(hwnd),
            value if value == VK_LEFT.0 => self.player.seek_by(-5_000),
            value if value == VK_RIGHT.0 => self.player.seek_by(5_000),
            value if value == VK_UP.0 || value == VK_MEDIA_PREV_TRACK.0 => self.previous(),
            value if value == VK_DOWN.0 || value == VK_MEDIA_NEXT_TRACK.0 => self.advance(),
            value if value == VK_HOME.0 => self.activate_first(true),
            value if value == VK_END.0 => {
                if let Some(index) = self.previous_playable(self.entries.len().saturating_sub(1)) {
                    self.activate(index, true);
                }
            }
            _ => {}
        }
    }

    fn app_command(&mut self, hwnd: HWND, command: u16) -> bool {
        match command {
            APPCOMMAND_MEDIA_PLAY_PAUSE => self.toggle_playback(hwnd),
            APPCOMMAND_MEDIA_NEXTTRACK => self.advance(),
            APPCOMMAND_MEDIA_PREVIOUSTRACK => self.previous(),
            APPCOMMAND_MEDIA_STOP => {
                self.player.pause();
                self.player.seek_ratio(0.0);
            }
            _ => return false,
        }
        true
    }

    fn toggle_playback(&mut self, hwnd: HWND) {
        if self.player.is_playing() {
            self.player.pause();
            return;
        }
        if self.player.wants_replay() {
            if let Some(page) = self.page_url.clone() {
                self.submit_url(hwnd, page);
                return;
            }
        }
        self.player.toggle_pause();
    }

    fn submit_url(&mut self, hwnd: HWND, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if !(text.starts_with("http://") || text.starts_with("https://") || text.starts_with("www.")) {
            let path = playlist::normalize_shell_path(&text);
            if path.exists() {
                self.open_target(path);
                return;
            }
        }
        self.url_gen = self.url_gen.wrapping_add(1);
        self.page_url = Some(text.clone());
        self.status = "Opening link…".into();
        remote::resolve_later(hwnd.0 as isize, self.url_gen, text);
        let _ = unsafe { SetFocus(Some(hwnd)) };
    }

    fn finish_url(&mut self, generation: u64) {
        let Some((gen, result)) = remote::take_result() else {
            return;
        };
        if gen != generation || gen != self.url_gen {
            return;
        }
        match result {
            Ok(link) => {
                self.title_name = link.title.clone();
                self.page_url = Some(link.page);
                self.image = None;
                self.current = None;
                self.show_video = true;
                self.metrics_key = -1;
                match self.player.play_url(&link.stream) {
                    Ok(()) => self.status.clear(),
                    Err(error) => {
                        self.status = error;
                        self.show_video = false;
                    }
                }
            }
            Err(error) => self.status = error,
        }
    }

    fn on_mf(&mut self, kind: u32, extra: isize) {
        match self.player.on_mf_event(kind, extra) {
            PlayerNotice::None => {}
            PlayerNotice::Ended => self.auto_advance(),
            PlayerNotice::Failed(error) => self.note_failure(error),
            PlayerNotice::ShowVideo(show) => {
                self.show_video = show && self.image.is_none();
            }
        }
    }

    fn layout_for(&self, hwnd: HWND, width: i32, height: i32) -> ui::Layout {
        ui::layout(
            width,
            height,
            dpi_of(hwnd),
            self.media_open(),
            self.title_px,
            self.phone.is_some(),
            self.url_open,
            self.list_px,
            self.filters_open,
            self.opacity_open,
        )
    }

    fn sync_phone(&self) {
        let Some(server) = &self.phone else {
            return;
        };
        let playing = self.current.and_then(|index| self.entries.get(index)).and_then(|entry| {
            if entry.kind == Kind::Folder {
                None
            } else {
                Some(entry.path.clone())
            }
        });
        server.publish(self.browser_dir.clone(), playing);
    }

    fn start_phone(&mut self) {
        if self.phone.is_some() {
            return;
        }
        if self.phone_token.len() < 16 {
            self.phone_token = lan::new_token();
        }
        match lan::start(self.phone_token.clone(), self.phone_port) {
            Ok(server) => {
                self.phone_url = server.url.clone();
                self.phone_port = server.port;
                self.phone_qr = server.qr.clone();
                self.phone_qr_width = server.qr_width;
                let firewall_ok = server.firewall_ok;
                self.phone = Some(server);
                self.sync_phone();
                self.status = if firewall_ok {
                    "Phone link is on. Scan the code with a phone on the same Wi-Fi. Leave this window open.".into()
                } else {
                    "Phone link is on. If the phone cannot connect, allow Better Media Player on private networks.".into()
                };
            }
            Err(error) => self.status = error,
        }
    }

    fn stop_phone(&mut self) {
        if let Some(server) = self.phone.take() {
            server.stop();
        }
        self.phone_url.clear();
        self.phone_qr.clear();
        self.phone_qr_width = 0;
    }

    fn set_opacity_ratio(&mut self, hwnd: HWND, ratio: f32) {
        let value = 0.45 + ratio.clamp(0.0, 1.0) * 0.55;
        self.opacity = (value * 255.0).round() as u8;
        apply_chrome(hwnd, self.topmost, self.opacity);
    }

    fn view(&mut self, hwnd: HWND) -> ui::Layout {
        let hdc = unsafe { GetDC(Some(hwnd)) };
        if !hdc.0.is_null() {
            self.ensure_metrics(hwnd, hdc);
            unsafe {
                let _ = ReleaseDC(Some(hwnd), hdc);
            }
        }
        let (width, height) = client_size(hwnd);
        self.layout_for(hwnd, width, height)
    }

    fn ensure_metrics(&mut self, hwnd: HWND, hdc: HDC) {
        let (width, _height) = client_size(hwnd);
        let dpi = dpi_of(hwnd);
        let media = self.media_open();
        let title_width = (width - ui::dp(32, dpi)).max(40);
        self.title_px = if media && !self.title_name.is_empty() {
            ui::block_height(hdc, self.gfx.font, &self.title_name, title_width) + ui::dp(12, dpi)
        } else {
            0
        };
        let layout = self.layout_for(hwnd, width, client_size(hwnd).1);
        let name_width = (layout.list.right - layout.list.left - ui::dp(96, dpi)).max(40);
        let key = name_width ^ ((self.entries.len() as i32) << 8) ^ (dpi << 1);
        if self.metrics_key != key {
            self.metrics_key = key;
            self.row_heights = self
                .entries
                .iter()
                .map(|entry| {
                    ui::block_height(hdc, self.gfx.font, &entry_name(&entry.path), name_width)
                        + ui::dp(10, dpi)
                })
                .collect();
        }
    }

    fn reveal_current(&mut self) {
        let Some(index) = self.current else {
            return;
        };
        let mut before = 0i32;
        for height in self.row_heights.iter().take(index) {
            before += *height;
        }
        let row = self.row_heights.get(index).copied().unwrap_or(32);
        if before < self.scroll_px {
            self.scroll_px = before;
        } else if before + row > self.scroll_px + 160 {
            self.scroll_px = (before + row - 160).max(0);
        }
    }

    fn clamp_scroll(&mut self, view: i32) {
        let content = ui::content_height(&self.row_heights);
        let max_scroll = (content - view).max(0);
        self.scroll_px = self.scroll_px.clamp(0, max_scroll);
    }

    fn window_title(&self) -> String {
        if !self.title_name.is_empty() {
            return format!("{} — Better Media Player", self.title_name);
        }
        "Better Media Player".into()
    }
}

fn paint_window(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    if hdc.0.is_null() {
        return;
    }
    let (width, height) = client_size(hwnd);
    let memory = unsafe { CreateCompatibleDC(Some(hdc)) };
    let bitmap = unsafe { CreateCompatibleBitmap(hdc, width.max(1), height.max(1)) };
    let previous = unsafe { SelectObject(memory, bitmap.into()) };
    with_app(hwnd, |app| {
        app.ensure_metrics(hwnd, memory);
        let layout = app.layout_for(hwnd, width, height);
        app.clamp_scroll(layout.list_height());
        let rows: Vec<RowPaint> = app
            .entries
            .iter()
            .map(|entry| RowPaint {
                name: entry_name(&entry.path),
                badge: playlist::badge(entry.kind, &entry.path),
            })
            .collect();
        let image = app
            .image
            .as_ref()
            .map(|frame| (frame.bitmap, frame.width, frame.height));
        let folder = app
            .browser_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default();
        let speed_label = if app.holding {
            "3×".to_string()
        } else {
            speed_label(app.player.speed())
        };
        let state = PaintState {
            title: &app.title_name,
            folder: &folder,
            status: &app.status,
            speed_label: &speed_label,
            rows,
            heights: &app.row_heights,
            scroll: app.scroll_px,
            current: app.current,
            playing: app.player.is_playing(),
            show_video: app.show_video,
            show_image: app.image.is_some(),
            image,
            position: app.player.position(),
            duration: app.player.duration(),
            seek_override: app.seek_drag,
            volume: app.player.volume(),
            opacity: (((app.opacity as f32 / 255.0) - 0.45) / 0.55).clamp(0.0, 1.0),
            auto_next: app.auto_next,
            loop_playlist: app.loop_playlist,
            topmost: app.topmost,
            phone: app.phone.is_some(),
            phone_url: &app.phone_url,
            phone_qr: &app.phone_qr,
            phone_qr_width: app.phone_qr_width,
            filter: app.filter,
            newest: app.order == Order::Newest,
            hot: app.hot,
        };
        ui::paint(memory, &app.gfx, layout, &state);
    });
    unsafe {
        let _ = BitBlt(hdc, 0, 0, width, height, Some(memory), 0, 0, SRCCOPY);
        let _ = SelectObject(memory, previous);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        let _ = EndPaint(hwnd, &paint);
    }
}

fn save_window(hwnd: HWND) {
    let Some(settings) = with_app(hwnd, |app| {
        let mut placement = WINDOWPLACEMENT::default();
        placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
        let (x, y, width, height) = unsafe {
            if GetWindowPlacement(hwnd, &mut placement).is_ok() {
                let rect = placement.rcNormalPosition;
                (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
            } else {
                let mut rect = RECT::default();
                let _ = GetWindowRect(hwnd, &mut rect);
                (rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top)
            }
        };
        Settings {
            x,
            y,
            width,
            height,
            volume: app.player.volume(),
            speed: app.speed,
            loop_playlist: app.loop_playlist,
            auto_next: app.auto_next,
            topmost: app.topmost,
            opacity: app.opacity,
            filter: app.filter.as_str().into(),
            order: app.order.as_str().into(),
            phone: app.phone.is_some(),
            phone_token: app.phone_token.clone(),
            phone_port: app.phone_port,
            list_px: app.list_px.max(0),
            placed: true,
        }
    }) else {
        return;
    };
    settings.save();
}

fn apply_chrome(hwnd: HWND, topmost: bool, opacity: u8) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let layered = WS_EX_LAYERED.0 as isize;
        let next = if opacity < 255 { ex | layered } else { ex & !layered };
        if next != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
        if opacity < 255 {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), opacity, LWA_ALPHA);
        }
        let insert = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
        let _ = SetWindowPos(
            hwnd,
            Some(insert),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
}

fn edit_text(hwnd: HWND) -> String {
    if hwnd.0.is_null() {
        return String::new();
    }
    unsafe {
        let length = GetWindowTextLengthW(hwnd);
        let mut buffer = vec![0u16; length as usize + 1];
        let written = GetWindowTextW(hwnd, &mut buffer);
        String::from_utf16_lossy(&buffer[..written as usize])
    }
}

fn cycle_speed(speed: f32) -> f32 {
    let index = SPEEDS
        .iter()
        .position(|item| (*item - speed).abs() < 0.02)
        .unwrap_or(2);
    SPEEDS[(index + 1) % SPEEDS.len()]
}

fn speed_label(speed: f32) -> String {
    let text = format!("{speed:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}×")
}

fn client_size(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    ((rect.right - rect.left).max(1), (rect.bottom - rect.top).max(1))
}

fn dpi_of(hwnd: HWND) -> i32 {
    unsafe { GetDpiForWindow(hwnd) as i32 }.max(96)
}

fn mouse_xy(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xffff) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
    (x, y)
}

fn path_from_copydata(lparam: LPARAM) -> Option<PathBuf> {
    if lparam.0 == 0 {
        return None;
    }
    let data = unsafe { &*(lparam.0 as *const COPYDATASTRUCT) };
    if data.lpData.is_null() || data.cbData < 2 {
        return None;
    }
    let len = data.cbData as usize / 2;
    let slice = unsafe { std::slice::from_raw_parts(data.lpData as *const u16, len) };
    let text = String::from_utf16_lossy(slice);
    let text = text.trim_end_matches('\0').trim();
    if text.is_empty() {
        None
    } else {
        Some(playlist::normalize_shell_path(text))
    }
}

fn drop_paths(drop: HDROP) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    unsafe {
        let count = DragQueryFileW(drop, 0xFFFF_FFFF, None);
        for index in 0..count {
            let needed = DragQueryFileW(drop, index, None);
            let mut buffer = vec![0u16; needed as usize + 1];
            DragQueryFileW(drop, index, Some(&mut buffer));
            let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
            let text = String::from_utf16_lossy(&buffer[..end]);
            if !text.is_empty() {
                paths.push(PathBuf::from(text));
            }
        }
    }
    paths
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
