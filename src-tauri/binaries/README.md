# Managed Media Tools

V1 is designed to use managed local binaries for media work:

- `yt-dlp` for supported source downloads where the user is allowed to save the media.
- `ffmpeg` and `ffprobe` for duration, codec inspection, thumbnails, and normalization.
- `mpv`, `VLC`, or `ffplay` for native playback of broad local video/audio formats that the
  Tauri WebView cannot decode directly.

Do not commit user cookies, source credentials, Telegram sessions, or private API tokens here. Production packaging should pin binary versions per platform and verify checksums before enabling downloader jobs.
