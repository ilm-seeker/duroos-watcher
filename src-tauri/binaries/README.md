# Managed Media Tools

V1 is designed to use managed local binaries for media work:

- `yt-dlp` for supported source downloads where the user is allowed to save the media.
- `ffmpeg` and `ffprobe` for duration, codec inspection, thumbnails, and normalization.
- `mpv`, `VLC`, or `ffplay` for native playback of broad local video/audio formats that the
  Tauri WebView cannot decode directly.

Do not commit user cookies, source credentials, Telegram sessions, or private API tokens here.

`media-tools.manifest.json` now locks the release-matrix media tools to immutable upstream package
or release URLs with SHA-256 checksums. Packaging must fetch only those archives, verify the hash
before extraction, and include the corresponding upstream license material in the final artifact.

Use the checked-in fetcher before packaging a target:

```bash
npm run media-tools:fetch -- --target=aarch64-apple-darwin
```

The fetcher stages verified executables under `src-tauri/binaries/vendor/<target>/` and writes a
`media-tools-report.json` with source URLs, checksums, sizes, and licenses. The staged binaries are
ignored by Git because they are generated release inputs, not source files.

Unsigned builds, unnotarized builds, or builds that fetch tools outside this manifest are
alpha/testing artifacts only.
