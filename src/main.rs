#![windows_subsystem = "windows"]

mod app;
mod images;
mod lan;
mod player;
mod playlist;
mod remote;
mod settings;
mod shell;
mod ui;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, MessageBoxW, SendMessageW, SetForegroundWindow, ShowWindow, MB_ICONINFORMATION,
    MB_OK, SW_RESTORE, WM_COPYDATA,
};
use windows_core::PCWSTR;

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let text = format!("Better Media Player hit an error:\n{info}");
        message_box(&text, "Better Media Player");
    }));

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--uninstall-shell") {
        shell::uninstall();
        message_box(
            "Removed Play with Better Media Player from the Explorer menu.",
            "Better Media Player",
        );
        return;
    }
    if args.iter().any(|arg| arg == "--install-shell") {
        shell::install();
        message_box(
            "Explorer will show Play with Better Media Player when you right-click a folder or media file.",
            "Better Media Player",
        );
        return;
    }

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let target = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(|arg| playlist::normalize_shell_path(arg));

    if let Some(existing) = existing_window() {
        if let Some(path) = target.as_ref() {
            send_path(existing, path);
        }
        unsafe {
            let _ = ShowWindow(existing, SW_RESTORE);
            let _ = SetForegroundWindow(existing);
        }
        return;
    }

    if let Err(error) = player::startup() {
        message_box(&error, "Better Media Player");
        return;
    }

    thread::spawn(shell::install);

    let run = app::run(move |hwnd| {
        let audio = rodio::OutputStream::try_default().ok();
        app::attach_audio_and_open(hwnd, audio, target);
    });
    if let Err(error) = run {
        message_box(&error.to_string(), "Better Media Player");
    }
    player::shutdown_platform();
}

fn existing_window() -> Option<HWND> {
    let mutex_name: Vec<u16> = "Local\\BetterMediaPlayer.SingleInstance"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mutex = unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) }.ok()?;
    let already = unsafe { windows::Win32::Foundation::GetLastError() } == ERROR_ALREADY_EXISTS;
    if !already {
        Box::leak(Box::new(mutex));
        return None;
    }
    unsafe {
        let _ = windows::Win32::Foundation::CloseHandle(mutex);
    }
    for _ in 0..100 {
        if let Ok(hwnd) = unsafe { FindWindowW(app::find_class_name(), PCWSTR::null()) } {
            if !hwnd.0.is_null() {
                return Some(hwnd);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

fn send_path(hwnd: HWND, path: &PathBuf) {
    let wide: Vec<u16> = OsStr::new(path.as_os_str())
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let data = COPYDATASTRUCT {
        dwData: 1,
        cbData: (wide.len() * 2) as u32,
        lpData: wide.as_ptr() as *mut _,
    };
    unsafe {
        let _ = SendMessageW(
            hwnd,
            WM_COPYDATA,
            Some(WPARAM(0)),
            Some(LPARAM(&data as *const COPYDATASTRUCT as isize)),
        );
    }
}

fn message_box(text: &str, caption: &str) {
    let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let caption_w: Vec<u16> = caption.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text_w.as_ptr()),
            PCWSTR(caption_w.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
