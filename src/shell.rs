use crate::playlist::shell_extensions;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_FLUSHNOWAIT, SHCNF_IDLIST};
use windows_core::PCWSTR;

const VERB: &str = "BetterMediaPlayer";
const LABEL: &str = "Play with Better Media Player";

pub fn install() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.display().to_string();
    let file_command = format!("\"{exe}\" \"%1\"");
    let folder_command = format!("\"{exe}\" \"%V\"");

    install_verb(r"Software\Classes\Directory", &file_command, &exe);
    install_verb(
        r"Software\Classes\Directory\Background",
        &folder_command,
        &exe,
    );
    install_verb(
        r"Software\Classes\SystemFileAssociations\audio",
        &file_command,
        &exe,
    );
    install_verb(
        r"Software\Classes\SystemFileAssociations\video",
        &file_command,
        &exe,
    );
    for ext in shell_extensions() {
        let parent = format!(r"Software\Classes\SystemFileAssociations\.{ext}");
        install_verb(&parent, &file_command, &exe);
    }
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST | SHCNF_FLUSHNOWAIT,
            None,
            None,
        );
    }
}

pub fn uninstall() {
    delete_verb(r"Software\Classes\Directory");
    delete_verb(r"Software\Classes\Directory\Background");
    delete_verb(r"Software\Classes\SystemFileAssociations\audio");
    delete_verb(r"Software\Classes\SystemFileAssociations\video");
    for ext in shell_extensions() {
        let parent = format!(r"Software\Classes\SystemFileAssociations\.{ext}");
        delete_verb(&parent);
    }
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST | SHCNF_FLUSHNOWAIT,
            None,
            None,
        );
    }
}

fn install_verb(parent: &str, command: &str, exe: &str) {
    let verb = format!(r"{parent}\shell\{VERB}");
    let command_key = format!(r"{verb}\command");
    let Ok(verb_key) = create_key(&verb) else {
        return;
    };
    set_default(verb_key, LABEL);
    set_value(verb_key, "Icon", exe);
    set_value(verb_key, "MultiSelectModel", "Player");
    close_key(verb_key);
    let Ok(cmd_key) = create_key(&command_key) else {
        return;
    };
    set_default(cmd_key, command);
    close_key(cmd_key);
}

fn delete_verb(parent: &str) {
    let verb = format!(r"{parent}\shell\{VERB}");
    let wide = wide(&verb);
    unsafe {
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(wide.as_ptr()));
    }
}

fn create_key(path: &str) -> Result<HKEY, ()> {
    let wide = wide(path);
    let mut key = HKEY::default();
    let error = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(wide.as_ptr()),
            Some(0),
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        )
    };
    if error == ERROR_SUCCESS {
        Ok(key)
    } else {
        Err(())
    }
}

fn set_default(key: HKEY, value: &str) {
    set_value_pcwstr(key, PCWSTR::null(), value);
}

fn set_value(key: HKEY, name: &str, value: &str) {
    let wide_name = wide(name);
    set_value_pcwstr(key, PCWSTR(wide_name.as_ptr()), value);
}

fn set_value_pcwstr(key: HKEY, name: PCWSTR, value: &str) {
    let data = reg_sz(value);
    unsafe {
        let _ = RegSetValueExW(key, name, Some(0), REG_SZ, Some(&data));
    }
}

fn close_key(key: HKEY) {
    unsafe {
        let _ = RegCloseKey(key);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn reg_sz(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

