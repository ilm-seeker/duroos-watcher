# Managed Media Tools

V1 is designed to use managed local binaries for media work:

- `yt-dlp` for supported source downloads where the user is allowed to save the media.
- `ffmpeg` and `ffprobe` for duration, codec inspection, thumbnails, and normalization.
- `mpv`, `VLC`, or `ffplay` for native playback of broad local video/audio formats that the
  Tauri WebView cannot decode directly.

Do not commit user cookies, source credentials, Telegram sessions, or private API tokens here.

Production packaging must populate `media-tools.manifest.json` with one entry per bundled or
CI-fetched executable. Each entry needs a target triple, upstream source URL, version, license, and
SHA-256 checksum before an artifact can be treated as production. Unsigned builds or builds without
pinned media-tool checksums are alpha/testing artifacts only.
