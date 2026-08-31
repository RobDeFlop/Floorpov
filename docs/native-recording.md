# Native Windows Recording Backend

FloorPoV includes an alternative Windows-native recording backend alongside the existing FFmpeg backend. FFmpeg remains bundled and is the production default during rollout.

## Architecture

The recording command creates a backend-neutral session. The session selects one runner and owns startup notification, final lifecycle events, and state cleanup.

The native runner uses:

- Windows Graphics Capture through `windows-capture` 2.0.1;
- Windows Media Foundation for H.264 video, optional AAC audio, and MPEG-4 output;
- the existing WASAPI loopback pipeline at 48 kHz, stereo, signed 16-bit PCM;
- Query Performance Counter timestamps for capture and synthetic black frames;
- one Media Foundation encoder for the complete recording.

Window recording keeps the encoder and audio pipeline alive when the selected window is minimized or unavailable. It writes black frames every 500 ms, retries capture with bounded backoff, and resolves a replacement HWND by the saved title. Native recordings do not use FFmpeg segments.

Native output is staged beside the requested output as:

```text
<recording-name>.mp4.native-partial
```

The file is promoted to the final `.mp4` only after successful encoder finalization. A non-empty partial file remains available for recovery after a runtime or finalization failure.

## Backend selection

Set `FLOORPOV_RECORDING_BACKEND` before starting FloorPoV:

| Value | Behavior |
|---|---|
| unset | Use FFmpeg |
| `ffmpeg` | Force FFmpeg |
| `native` | Force the native backend; do not fall back |
| `auto` | Try native, then use FFmpeg only after an unacknowledged startup failure |

An invalid value logs a warning and uses FFmpeg. In `auto` mode, an explicit NVENC, QSV, AMF, or libx264 preference selects FFmpeg so the saved preference is honored. Media Foundation chooses the concrete encoder in native mode.

PowerShell example:

```powershell
$env:FLOORPOV_RECORDING_BACKEND = "native"
bun run tauri dev
```

Return to FFmpeg:

```powershell
$env:FLOORPOV_RECORDING_BACKEND = "ffmpeg"
bun run tauri dev
```

Remove the environment variable to restore the production default:

```powershell
Remove-Item Env:FLOORPOV_RECORDING_BACKEND
```

## Running without `ffmpeg.exe`

Forced-native recording resolves no FFmpeg executable. For development verification:

1. Move `src-tauri/bin/ffmpeg.exe` outside `src-tauri/bin`.
2. Set `FLOORPOV_RECORDING_BACKEND=native`.
3. Run FloorPoV and test monitor and window recording.
4. Restore `src-tauri/bin/ffmpeg.exe` before an FFmpeg test or package build.

Do not delete the binary, downloader, bundle resource, or release preparation step during the rollout.

## Automated Windows smoke tests

These tests are ignored in normal CI because they require Media Foundation or an interactive desktop:

```powershell
cd src-tauri
cargo test native_encoder_black_video -- --ignored --nocapture
cargo test native_black_recording_session -- --ignored --nocapture
cargo test native_monitor_capture_smoke -- --ignored --nocapture
cargo test native_monitor_recording_session -- --ignored --nocapture
cargo test native_window_capture_smoke -- --ignored --nocapture
```

## Manual QA matrix

Run both 30 and 60 FPS where applicable.

| Scenario | Check |
|---|---|
| Monitor without audio | H.264 MP4, dimensions, duration, playback |
| Monitor with audio | AAC presence, synchronization, clean stop |
| Window available | Real frames replace the initial black frame |
| Window unavailable at start | Black video starts and duration advances |
| Minimize and restore | Warning, black interval, real-frame recovery |
| Close and reopen | Recovery by saved title and new HWND |
| Resize or move window | Fixed output remains playable |
| Stop while unavailable | Final duration includes the black interval |
| Forced native without FFmpeg | Recording starts and finalizes |
| Forced FFmpeg | Existing recording behavior remains unchanged |
| Auto startup failure | FFmpeg starts once; lifecycle events remain single |
| Runtime native failure | No FFmpeg fallback; partial output is retained |
| Auto-recording and combat watch | Start/stop, discard, markers, and metadata remain intact |
| Storage cleanup and playback list | Existing cleanup, listing, and playback remain intact |

Record Windows version, GPU, display resolution and DPI, duration, FPS, and audio state with each manual result.

## Known limitations

- Media Foundation chooses the H.264 encoder; explicit GPU encoder selection is unavailable.
- The WGC update interval throttles delivery but does not guarantee exact CFR output.
- A resized window is padded or cropped to a fixed output size; aspect-ratio scaling is not implemented.
- Native output has no FFmpeg-style segment recovery or fragmented MP4 crash recovery.
- Color behavior and finalization require broader Intel, AMD, and NVIDIA validation.
- Long-session A/V drift and two-hour stability require hardware testing.
- A non-empty `.native-partial` file can remain after runtime or finalization failure.

These limitations prevent switching the default or removing FFmpeg without a separate rollout decision.
