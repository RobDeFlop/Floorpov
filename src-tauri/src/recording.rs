use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, RwLock};
use wasapi::{initialize_mta, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};

#[derive(Clone, serde::Serialize)]
pub struct RecordingStartedPayload {
    output_path: String,
    width: u32,
    height: u32,
}

const FFMPEG_RESOURCE_PATH: &str = "bin/ffmpeg.exe";
const FFMPEG_STOP_TIMEOUT: Duration = Duration::from_secs(5);
const SYSTEM_AUDIO_SAMPLE_RATE_HZ: usize = 48_000;
const SYSTEM_AUDIO_CHANNEL_COUNT: usize = 2;
const SYSTEM_AUDIO_BITS_PER_SAMPLE: usize = 16;
const SYSTEM_AUDIO_CHUNK_FRAMES: usize = 960;
const SYSTEM_AUDIO_EVENT_TIMEOUT_MS: u32 = 500;
const AUDIO_TCP_ACCEPT_WAIT_MS: u64 = 25;
const SYSTEM_AUDIO_QUEUE_CAPACITY: usize = 256;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Default)]
struct AudioPipelineStats {
    queued_chunks: AtomicU64,
    dequeued_chunks: AtomicU64,
    dropped_chunks: AtomicU64,
    write_timeouts: AtomicU64,
}

pub struct RecordingState {
    is_recording: bool,
    is_stopping: bool,
    current_output_path: Option<String>,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            is_stopping: false,
            current_output_path: None,
            stop_tx: None,
        }
    }
}

pub type SharedRecordingState = Arc<RwLock<RecordingState>>;

fn resolve_ffmpeg_binary_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(resource_path) = app_handle
        .path()
        .resolve(FFMPEG_RESOURCE_PATH, BaseDirectory::Resource)
    {
        candidates.push(resource_path);
    }

    let manifest_candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("bin")
        .join("ffmpeg.exe");
    candidates.push(manifest_candidate.clone());

    if let Ok(current_executable) = std::env::current_exe() {
        if let Some(executable_directory) = current_executable.parent() {
            candidates.push(executable_directory.join("ffmpeg.exe"));
            candidates.push(
                executable_directory
                    .join("resources")
                    .join("bin")
                    .join("ffmpeg.exe"),
            );
        }
    }

    if let Some(found_path) = candidates.into_iter().find(|path| path.exists()) {
        return Ok(found_path);
    }

    Err(format!(
        "FFmpeg binary was not found. Place ffmpeg.exe at '{}' or rebuild the app so bundled resources are available.",
        manifest_candidate.display()
    ))
}

fn select_video_encoder(ffmpeg_binary_path: &Path) -> (String, Option<String>) {
    let mut command = Command::new(ffmpeg_binary_path);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command
        .arg("-hide_banner")
        .arg("-encoders")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let encoders_output = match output {
        Ok(result) => String::from_utf8(result.stdout)
            .unwrap_or_default()
            .to_lowercase(),
        Err(_) => String::new(),
    };

    if encoders_output.contains(" h264_nvenc") {
        return ("h264_nvenc".to_string(), Some("p3".to_string()));
    }

    if encoders_output.contains(" h264_qsv") {
        return ("h264_qsv".to_string(), None);
    }

    if encoders_output.contains(" h264_amf") {
        return ("h264_amf".to_string(), None);
    }

    ("libx264".to_string(), Some("superfast".to_string()))
}

fn parse_ffmpeg_speed(line: &str) -> Option<f64> {
    let speed_index = line.find("speed=")?;
    let speed_slice = &line[speed_index + 6..];
    let speed_token = speed_slice.split_whitespace().next()?;
    let numeric = speed_token.trim_end_matches('x');
    numeric.parse::<f64>().ok()
}

fn build_loopback_capture_context(
) -> Result<(wasapi::AudioClient, wasapi::AudioCaptureClient, WaveFormat), String> {
    initialize_mta()
        .ok()
        .map_err(|error| format!("Failed to initialize COM for system audio capture: {error}"))?;

    let enumerator = DeviceEnumerator::new()
        .map_err(|error| format!("Failed to enumerate audio devices: {error}"))?;
    let device = enumerator
        .get_default_device(&Direction::Render)
        .map_err(|error| format!("Failed to access default output audio device: {error}"))?;
    let mut audio_client = device
        .get_iaudioclient()
        .map_err(|error| format!("Failed to create WASAPI audio client: {error}"))?;

    let wave_format = WaveFormat::new(
        SYSTEM_AUDIO_BITS_PER_SAMPLE,
        SYSTEM_AUDIO_BITS_PER_SAMPLE,
        &SampleType::Int,
        SYSTEM_AUDIO_SAMPLE_RATE_HZ,
        SYSTEM_AUDIO_CHANNEL_COUNT,
        None,
    );
    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: 0,
    };

    audio_client
        .initialize_client(&wave_format, &Direction::Capture, &mode)
        .map_err(|error| {
            format!("Failed to initialize WASAPI loopback client for system audio: {error}")
        })?;

    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|error| format!("Failed to create WASAPI capture client: {error}"))?;

    Ok((audio_client, capture_client, wave_format))
}

fn validate_system_audio_capture_available() -> Result<(), String> {
    let _ = build_loopback_capture_context()?;
    Ok(())
}

fn run_system_audio_capture_to_queue(
    audio_tx: std_mpsc::SyncSender<Vec<u8>>,
    stop_rx: std_mpsc::Receiver<()>,
    stats: Arc<AudioPipelineStats>,
) -> Result<(), String> {
    let (audio_client, capture_client, wave_format) = build_loopback_capture_context()?;
    let event_handle = audio_client
        .set_get_eventhandle()
        .map_err(|error| format!("Failed to configure WASAPI event handle: {error}"))?;

    audio_client
        .start_stream()
        .map_err(|error| format!("Failed to start system audio stream: {error}"))?;

    let mut sample_queue: VecDeque<u8> = VecDeque::new();
    let chunk_size_bytes = wave_format.get_blockalign() as usize * SYSTEM_AUDIO_CHUNK_FRAMES;
    let mut should_stop = false;
    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(std_mpsc::TryRecvError::Disconnected) => {
                should_stop = true;
            }
            Err(std_mpsc::TryRecvError::Empty) => {}
        }

        let next_packet_frames = match capture_client.get_next_packet_size() {
            Ok(packet_size) => packet_size.unwrap_or(0),
            Err(error) => {
                tracing::warn!("Failed to poll system audio packets: {error}");
                thread::sleep(Duration::from_millis(10));
                continue;
            }
        };

        if next_packet_frames > 0 {
            if let Err(error) = capture_client.read_from_device_to_deque(&mut sample_queue) {
                tracing::warn!("Failed to read system audio packet: {error}");
                thread::sleep(Duration::from_millis(10));
                continue;
            }
        }

        while sample_queue.len() >= chunk_size_bytes {
            let mut chunk = Vec::with_capacity(chunk_size_bytes);
            chunk.extend(sample_queue.drain(..chunk_size_bytes));

            match audio_tx.try_send(chunk) {
                Ok(()) => {
                    stats.queued_chunks.fetch_add(1, Ordering::Relaxed);
                }
                Err(std_mpsc::TrySendError::Full(_)) => {
                    let dropped_chunks = stats.dropped_chunks.fetch_add(1, Ordering::Relaxed) + 1;
                    if dropped_chunks % 64 == 0 {
                        tracing::warn!(
                            dropped_chunks,
                            "Dropping system audio chunks due to queue backpressure"
                        );
                    }
                }
                Err(std_mpsc::TrySendError::Disconnected(_)) => return Ok(()),
            }
        }

        if should_stop {
            break;
        }

        if let Err(error) = event_handle.wait_for_event(SYSTEM_AUDIO_EVENT_TIMEOUT_MS) {
            tracing::debug!("System audio wait event timed/failed: {error}");
        }
    }

    if !sample_queue.is_empty() {
        let mut remaining = Vec::with_capacity(sample_queue.len());
        remaining.extend(sample_queue.drain(..));
        if audio_tx.try_send(remaining).is_ok() {
            stats.queued_chunks.fetch_add(1, Ordering::Relaxed);
        }
    }

    if let Err(error) = audio_client.stop_stream() {
        tracing::warn!("Failed to stop system audio stream cleanly: {error}");
    }

    Ok(())
}

fn run_audio_queue_to_writer<TWriter: Write>(
    mut writer: TWriter,
    audio_rx: std_mpsc::Receiver<Vec<u8>>,
    stop_rx: std_mpsc::Receiver<()>,
    stats: Arc<AudioPipelineStats>,
) -> Result<(), String> {
    loop {
        match stop_rx.try_recv() {
            Ok(()) | Err(std_mpsc::TryRecvError::Disconnected) => break,
            Err(std_mpsc::TryRecvError::Empty) => {}
        }

        match audio_rx.recv_timeout(Duration::from_millis(10)) {
            Ok(chunk) => {
                stats.dequeued_chunks.fetch_add(1, Ordering::Relaxed);
                if let Err(error) = writer.write_all(&chunk) {
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) {
                        stats.write_timeouts.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    return Err(format!(
                        "Failed to write system audio buffer to FFmpeg: {error}"
                    ));
                }
            }
            Err(std_mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std_mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = writer.flush();
    Ok(())
}

fn clear_recording_state(state: &SharedRecordingState) {
    let mut recording_state = state.blocking_write();
    recording_state.is_recording = false;
    recording_state.is_stopping = false;
    recording_state.current_output_path = None;
    recording_state.stop_tx = None;
}

fn signal_audio_threads_stop(
    audio_capture_stop_tx: &Option<std_mpsc::Sender<()>>,
    audio_writer_stop_tx: &Option<std_mpsc::Sender<()>>,
) {
    if let Some(capture_stop_tx) = audio_capture_stop_tx {
        if let Err(error) = capture_stop_tx.send(()) {
            tracing::debug!("Audio capture stop signal channel is closed: {error}");
        }
    }

    if let Some(writer_stop_tx) = audio_writer_stop_tx {
        if let Err(error) = writer_stop_tx.send(()) {
            tracing::debug!("Audio writer stop signal channel is closed: {error}");
        }
    }
}

fn emit_recording_stopped(app_handle: &AppHandle) {
    if let Err(error) = app_handle.emit("recording-stopped", ()) {
        tracing::error!("Failed to emit recording-stopped event: {error}");
    }
}

fn emit_recording_finalized(app_handle: &AppHandle, output_path: &str) {
    if let Err(error) = app_handle.emit("recording-finalized", output_path) {
        tracing::error!("Failed to emit recording-finalized event: {error}");
    }
}

fn spawn_ffmpeg_recording_task(
    app_handle: AppHandle,
    state: SharedRecordingState,
    output_path: String,
    ffmpeg_binary_path: PathBuf,
    requested_frame_rate: u32,
    output_frame_rate: u32,
    bitrate: u32,
    include_system_audio: bool,
    enable_diagnostics: bool,
    mut stop_rx: mpsc::Receiver<()>,
) {
    thread::spawn(move || {
        let bitrate_string = bitrate.to_string();
        let maxrate_string = bitrate.to_string();
        let buffer_size_string = bitrate.saturating_mul(2).to_string();
        let (video_encoder, encoder_preset) = select_video_encoder(&ffmpeg_binary_path);

        tracing::info!(
            ffmpeg_path = %ffmpeg_binary_path.display(),
            requested_frame_rate,
            output_frame_rate,
            bitrate,
            include_system_audio,
            enable_diagnostics,
            video_encoder,
            "Starting FFmpeg recording"
        );

        let mut command = Command::new(&ffmpeg_binary_path);
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        command
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("warning")
            .arg("-stats")
            .arg("-stats_period")
            .arg("1")
            .arg("-y");

        let mut audio_listener: Option<TcpListener> = None;

        if include_system_audio {
            let listener = match TcpListener::bind(("127.0.0.1", 0)) {
                Ok(listener) => listener,
                Err(error) => {
                    tracing::error!("Failed to allocate local audio TCP listener: {error}");
                    clear_recording_state(&state);
                    emit_recording_stopped(&app_handle);
                    return;
                }
            };

            if let Err(error) = listener.set_nonblocking(true) {
                tracing::error!("Failed to configure audio TCP listener: {error}");
                clear_recording_state(&state);
                emit_recording_stopped(&app_handle);
                return;
            }

            let audio_port = match listener.local_addr() {
                Ok(address) => address.port(),
                Err(error) => {
                    tracing::error!("Failed to resolve audio TCP listener port: {error}");
                    clear_recording_state(&state);
                    emit_recording_stopped(&app_handle);
                    return;
                }
            };

            command
                .arg("-thread_queue_size")
                .arg("1024")
                .arg("-f")
                .arg("s16le")
                .arg("-ar")
                .arg(SYSTEM_AUDIO_SAMPLE_RATE_HZ.to_string())
                .arg("-ac")
                .arg(SYSTEM_AUDIO_CHANNEL_COUNT.to_string())
                .arg("-i")
                .arg(format!("tcp://127.0.0.1:{audio_port}"))
                .arg("-f")
                .arg("lavfi")
                .arg("-i")
                .arg(format!(
                    "ddagrab=output_idx=0:framerate={requested_frame_rate}:draw_mouse=1,hwdownload,format=bgra"
                ))
                .arg("-map")
                .arg("1:v:0")
                .arg("-map")
                .arg("0:a:0")
                .arg("-af")
                .arg("aresample=async=1:min_hard_comp=0.100:first_pts=0,volume=2.2,alimiter=limit=0.98")
                .arg("-vf")
                .arg(format!("fps={output_frame_rate},format=yuv420p"))
                .arg("-thread_queue_size")
                .arg("512")
                .arg("-c:a")
                .arg("aac")
                .arg("-b:a")
                .arg("192k")
                .arg("-ar")
                .arg("48000")
                .arg("-ac")
                .arg("2");

            audio_listener = Some(listener);
        } else {
            command
                .arg("-f")
                .arg("lavfi")
                .arg("-i")
                .arg(format!(
                    "ddagrab=output_idx=0:framerate={requested_frame_rate}:draw_mouse=1,hwdownload,format=bgra"
                ))
                .arg("-vf")
                .arg(format!("fps={output_frame_rate},format=yuv420p"))
                .arg("-an");
        }

        command.arg("-c:v").arg(&video_encoder);

        if let Some(preset) = encoder_preset {
            command.arg("-preset").arg(preset);
        }

        command
            .arg("-b:v")
            .arg(&bitrate_string)
            .arg("-maxrate")
            .arg(&maxrate_string)
            .arg("-bufsize")
            .arg(&buffer_size_string)
            .arg("-fps_mode")
            .arg("cfr")
            .arg("-max_muxing_queue_size")
            .arg("2048")
            .arg("-movflags")
            .arg("+faststart")
            .arg(&output_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = match command.spawn() {
            Ok(process) => process,
            Err(error) => {
                tracing::error!("Failed to spawn FFmpeg recording process: {error}");
                clear_recording_state(&state);
                emit_recording_stopped(&app_handle);
                return;
            }
        };

        let stderr_thread = child.stderr.take().map(|stderr| {
            thread::spawn(move || {
                let mut low_speed_streak = 0u32;
                let mut low_speed_warned = false;

                for line in BufReader::new(stderr).lines() {
                    match line {
                        Ok(content) if !content.trim().is_empty() => {
                            let is_progress_line = content.contains("frame=")
                                || content.contains("fps=")
                                || content.contains("dup=")
                                || content.contains("drop=")
                                || content.contains("speed=");

                            if let Some(speed) = parse_ffmpeg_speed(&content) {
                                if speed < 0.90 {
                                    low_speed_streak = low_speed_streak.saturating_add(1);
                                    if low_speed_streak >= 3 && !low_speed_warned {
                                        tracing::warn!(
                                            speed,
                                            "FFmpeg encode speed is below realtime; consider lower quality preset"
                                        );
                                        low_speed_warned = true;
                                    }
                                } else {
                                    low_speed_streak = 0;
                                }
                            }

                            if is_progress_line {
                                if enable_diagnostics {
                                    tracing::info!("ffmpeg: {content}");
                                }
                            } else {
                                if enable_diagnostics {
                                    tracing::debug!("ffmpeg: {content}");
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!("Failed to read FFmpeg stderr: {error}");
                            break;
                        }
                    }
                }
            })
        });

        let (
            audio_capture_stop_tx,
            audio_writer_stop_tx,
            audio_capture_thread,
            audio_writer_thread,
            audio_stats,
        ) = if include_system_audio {
            let Some(listener) = audio_listener else {
                tracing::error!("System audio was enabled but audio listener was unavailable");
                clear_recording_state(&state);
                emit_recording_stopped(&app_handle);
                return;
            };

            let (audio_tx, audio_rx) =
                std_mpsc::sync_channel::<Vec<u8>>(SYSTEM_AUDIO_QUEUE_CAPACITY);
            let (capture_stop_tx, capture_stop_rx) = std_mpsc::channel::<()>();
            let (writer_stop_tx, writer_stop_rx) = std_mpsc::channel::<()>();
            let stats = Arc::new(AudioPipelineStats::default());

            let writer_stats = Arc::clone(&stats);
            let writer_thread = thread::spawn(move || {
                tracing::info!("Waiting for FFmpeg audio socket connection");
                let audio_stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            tracing::info!("FFmpeg audio socket connected");
                            break Ok(stream);
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            match writer_stop_rx.try_recv() {
                                Ok(()) | Err(std_mpsc::TryRecvError::Disconnected) => {
                                    return Ok(());
                                }
                                Err(std_mpsc::TryRecvError::Empty) => {
                                    thread::sleep(Duration::from_millis(AUDIO_TCP_ACCEPT_WAIT_MS));
                                }
                            }
                        }
                        Err(error) => {
                            break Err(format!("Failed to accept audio TCP stream: {error}"))
                        }
                    }
                }?;

                let _ = audio_stream.set_nodelay(true);
                let _ = audio_stream.set_write_timeout(Some(Duration::from_millis(12)));
                let writer_result =
                    run_audio_queue_to_writer(audio_stream, audio_rx, writer_stop_rx, writer_stats);
                tracing::info!("System audio writer thread exited");
                writer_result
            });

            let capture_stats = Arc::clone(&stats);
            let capture_thread = thread::spawn(move || {
                let capture_result =
                    run_system_audio_capture_to_queue(audio_tx, capture_stop_rx, capture_stats);
                tracing::info!("System audio capture thread exited");
                capture_result
            });

            (
                Some(capture_stop_tx),
                Some(writer_stop_tx),
                Some(capture_thread),
                Some(writer_thread),
                Some(stats),
            )
        } else {
            (None, None, None, None, None)
        };

        let mut stop_requested_at: Option<Instant> = None;
        let mut kill_sent = false;
        let mut stats_logged_at = Instant::now();
        let mut previous_queued = 0u64;
        let mut previous_dequeued = 0u64;
        let mut previous_dropped = 0u64;
        let mut previous_timeouts = 0u64;
        let mut drop_warning_emitted = false;

        let exit_status = loop {
            if stop_requested_at.is_none() {
                match stop_rx.try_recv() {
                    Ok(()) | Err(TryRecvError::Disconnected) => {
                        stop_requested_at = Some(Instant::now());
                        signal_audio_threads_stop(&audio_capture_stop_tx, &audio_writer_stop_tx);

                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = stdin.write_all(b"q\n");
                            let _ = stdin.flush();
                        }
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }

            if let Some(requested_at) = stop_requested_at {
                if !kill_sent && requested_at.elapsed() >= FFMPEG_STOP_TIMEOUT {
                    if let Err(error) = child.kill() {
                        tracing::warn!("Failed to force-stop FFmpeg process: {error}");
                    }
                    kill_sent = true;
                }
            }

            if let Some(audio_stats) = &audio_stats {
                if stats_logged_at.elapsed() >= Duration::from_secs(1) {
                    let queued_total = audio_stats.queued_chunks.load(Ordering::Relaxed);
                    let dequeued_total = audio_stats.dequeued_chunks.load(Ordering::Relaxed);
                    let dropped_total = audio_stats.dropped_chunks.load(Ordering::Relaxed);
                    let timeouts_total = audio_stats.write_timeouts.load(Ordering::Relaxed);
                    let queue_depth = queued_total.saturating_sub(dequeued_total);
                    let dropped_delta = dropped_total.saturating_sub(previous_dropped);
                    let timeout_delta = timeouts_total.saturating_sub(previous_timeouts);

                    if dropped_delta > 0 && !drop_warning_emitted {
                        tracing::warn!(
                            dropped_delta,
                            "Audio chunks were dropped to keep video smooth"
                        );
                        drop_warning_emitted = true;
                    }

                    if timeout_delta > 0 {
                        tracing::warn!(
                            timeout_delta,
                            "Audio writer hit socket timeouts during this interval"
                        );
                    }

                    if enable_diagnostics {
                        tracing::info!(
                            audio_queue_depth = queue_depth,
                            audio_chunks_queued = queued_total.saturating_sub(previous_queued),
                            audio_chunks_written = dequeued_total.saturating_sub(previous_dequeued),
                            audio_chunks_dropped = dropped_delta,
                            audio_write_timeouts = timeout_delta,
                            "Audio pipeline stats"
                        );
                    }

                    previous_queued = queued_total;
                    previous_dequeued = dequeued_total;
                    previous_dropped = dropped_total;
                    previous_timeouts = timeouts_total;
                    stats_logged_at = Instant::now();
                }
            }

            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(Duration::from_millis(25)),
                Err(error) => break Err(error),
            }
        };

        signal_audio_threads_stop(&audio_capture_stop_tx, &audio_writer_stop_tx);

        if let Some(stderr_thread) = stderr_thread {
            if let Err(error) = stderr_thread.join() {
                tracing::warn!("Failed to join FFmpeg stderr thread: {error:?}");
            }
        }

        if let Some(audio_capture_thread) = audio_capture_thread {
            match audio_capture_thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::error!("System audio capture thread failed: {error}");
                }
                Err(error) => {
                    tracing::error!("System audio capture thread panicked: {error:?}");
                }
            }
        }

        if let Some(audio_writer_thread) = audio_writer_thread {
            match audio_writer_thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    tracing::error!("System audio writer thread failed: {error}");
                }
                Err(error) => {
                    tracing::error!("System audio writer thread panicked: {error:?}");
                }
            }
        }

        let ffmpeg_completed_successfully = match exit_status {
            Ok(status) if status.success() => {
                tracing::info!("FFmpeg recording process finished successfully");
                true
            }
            Ok(status) => {
                tracing::error!("FFmpeg recording process exited with status: {status}");
                false
            }
            Err(error) => {
                tracing::error!("Failed while waiting for FFmpeg recording process: {error}");
                if let Err(kill_error) = child.kill() {
                    tracing::debug!("FFmpeg kill after wait failure returned: {kill_error}");
                }
                if let Err(wait_error) = child.wait() {
                    tracing::warn!("Failed to collect FFmpeg exit status after kill: {wait_error}");
                }
                false
            }
        };

        if ffmpeg_completed_successfully {
            if Path::new(&output_path).exists() {
                emit_recording_finalized(&app_handle, &output_path);
            } else {
                tracing::warn!("FFmpeg reported success but output file was not found");
            }
        }

        clear_recording_state(&state);
        emit_recording_stopped(&app_handle);
    });
}

#[tauri::command]
pub async fn start_recording(
    app_handle: AppHandle,
    state: tauri::State<'_, SharedRecordingState>,
    settings: crate::settings::RecordingSettings,
    output_folder: String,
    max_storage_bytes: u64,
) -> Result<RecordingStartedPayload, String> {
    {
        let recording_state = state.read().await;
        if recording_state.is_recording || recording_state.is_stopping {
            return Err("Recording already in progress".to_string());
        }
    }

    std::fs::create_dir_all(&output_folder)
        .map_err(|error| format!("Failed to create output directory: {error}"))?;

    let width = 1920;
    let height = 1080;
    let effective_bitrate = settings.effective_bitrate(width, height);
    let estimated_size = settings.estimate_size_bytes_for_capture(width, height);

    let current_size = crate::settings::get_folder_size(output_folder.clone())?;
    if current_size + estimated_size > max_storage_bytes {
        let cleanup_result = crate::settings::cleanup_old_recordings(
            output_folder.clone(),
            max_storage_bytes,
            estimated_size,
        )?;

        if cleanup_result.deleted_count > 0 {
            if let Err(error) = app_handle.emit("storage-cleanup", cleanup_result) {
                tracing::warn!("Failed to emit storage-cleanup event: {error}");
            }
        }
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("screen_recording_{timestamp}.mp4");
    let output_path = Path::new(&output_folder).join(filename);
    let output_path_str = output_path.to_string_lossy().to_string();

    let mut recording_settings = settings;
    recording_settings.bitrate = effective_bitrate;
    if recording_settings.enable_system_audio {
        recording_settings.bitrate = recording_settings.bitrate.min(16_000_000);
    }
    let output_frame_rate = recording_settings.frame_rate.max(1);
    let ffmpeg_binary_path = resolve_ffmpeg_binary_path(&app_handle)?;

    if recording_settings.enable_system_audio {
        validate_system_audio_capture_available()?;
    }

    tracing::info!(
        backend = "ffmpeg",
        video_quality = %recording_settings.video_quality,
        requested_frame_rate = recording_settings.frame_rate,
        output_frame_rate,
        include_system_audio = recording_settings.enable_system_audio,
        enable_diagnostics = recording_settings.enable_recording_diagnostics,
        effective_bitrate_bps = recording_settings.bitrate,
        "Using recording settings"
    );

    let (stop_tx, stop_rx) = mpsc::channel(1);

    {
        let mut recording_state = state.write().await;
        if recording_state.is_recording || recording_state.is_stopping {
            return Err("Recording already in progress".to_string());
        }

        recording_state.is_recording = true;
        recording_state.is_stopping = false;
        recording_state.current_output_path = Some(output_path_str.clone());
        recording_state.stop_tx = Some(stop_tx);
    }

    spawn_ffmpeg_recording_task(
        app_handle.clone(),
        state.inner().clone(),
        output_path_str.clone(),
        ffmpeg_binary_path,
        recording_settings.frame_rate,
        output_frame_rate,
        recording_settings.bitrate,
        recording_settings.enable_system_audio,
        recording_settings.enable_recording_diagnostics,
        stop_rx,
    );

    Ok(RecordingStartedPayload {
        output_path: output_path_str,
        width,
        height,
    })
}

#[tauri::command]
pub async fn stop_recording(
    state: tauri::State<'_, SharedRecordingState>,
) -> Result<String, String> {
    let (output_path, stop_tx) = {
        let mut recording_state = state.write().await;

        if !recording_state.is_recording {
            return Err("No active recording to stop".to_string());
        }

        let output_path = recording_state
            .current_output_path
            .clone()
            .ok_or_else(|| "No output path found".to_string())?;

        if recording_state.is_stopping {
            return Ok(output_path);
        }

        recording_state.is_stopping = true;

        (output_path, recording_state.stop_tx.take())
    };

    if let Some(stop_tx) = stop_tx {
        if let Err(error) = stop_tx.send(()).await {
            tracing::warn!("Failed to send stop signal to recording task: {error}");
        }
    }

    Ok(output_path)
}
