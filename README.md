# Better Media Player

A fast native Windows player for local audio and video. It opens quickly, plays a folder as an ordered playlist, and can stream that folder to a phone on the same Wi-Fi.

## Features

- Fast playback for mp3, wav, flac, and ogg, and Windows Media Foundation for video and other formats
- Explorer menu: **Play with Better Media Player** on files, folders, and the folder background
- Playlist with Up, Back, folder rows, filters, and newest-first or name order
- Playback speed, hold the picture for 3×, loop, and auto-next
- Always on top and a window opacity control
- Paste a direct media URL, or a YouTube link when [yt-dlp](https://github.com/yt-dlp/yt-dlp) is on `PATH`
- Phone page on the same Wi-Fi: playlist, folder browsing, audio-only, and seeking

## Requirements

- Windows 10 or later
- [Rust](https://rustup.rs/) with the MSVC toolchain, to build
- [ffmpeg](https://ffmpeg.org/) and `ffprobe` on `PATH`, for phone playback of formats the browser cannot play directly, and for seeking in audio-only streams
- `yt-dlp` on `PATH`, only for YouTube and other web pages

## Build

```
cargo build --release
```

The program is `target\release\BetterMediaPlayer.exe`. The first launch adds the Explorer menu for the current user. No administrator install is required.

Remove that menu with:

```
BetterMediaPlayer.exe --uninstall-shell
```

Settings are stored in `%APPDATA%\BetterMediaPlayer\window.txt`.

## Phone listening

Turn on **Phone** in the player and open the link or QR code from a phone on the same Wi-Fi. Files stay on the PC. Leave the player open while listening.

The link contains a random token. Anyone with that link can browse and play files on the PC, starting from the open folder and moving to other folders. Do not share the link or the QR code. Windows is asked to allow the port on private networks only, from the local subnet.

## License

[MIT](LICENSE)
