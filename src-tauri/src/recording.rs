use base64::Engine;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, RwLock};
use windows_capture::{
    capture::{Context, GraphicsCaptureApiHandler},
    encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, ImageEncoder, ImageEncoderPixelFormat,
        ImageFormat, VideoEncoder, VideoSettingsBuilder, VideoSettingsSubType,
    },
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

#[derive(Clone, serde::Serialize)]
pub struct RecordingStartedPayload {
    output_path: String,
    width: u32,
    height: u32,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PreviewFramePayload {
    data_base64: String,
}

const PREVIEW_FRAME_INTERVAL: Duration = Duration::from_millis(83);
const MAX_FILENAME_SEGMENT_LENGTH: usize = 48;

struct RecordingHandler {
    app_handle: AppHandle,
    output_path: String,
    encoder: Option<VideoEncoder>,
    preview_encoder: ImageEncoder,
    preview_buffer: Vec<u8>,
    next_preview_emit_at: Instant,
    stop_rx: mpsc::Receiver<()>,
    state: SharedRecordingState,
    finalized_emitted: bool,
}

type RecordingHandlerFlags = <RecordingHandler as GraphicsCaptureApiHandler>::Flags;

impl RecordingHandler {
    fn finish_encoder_if_present(&mut self, context: &str) {
        if let Some(encoder) = self.encoder.take() {
            if let Err(error) = encoder.finish() {
                tracing::error!("Failed to finalize recording encoder {context}: {error}");
            }
        }
    }

    fn emit_recording_finalized(&mut self) {
        if self.finalized_emitted {
            return;
        }

        if let Err(error) = self
            .app_handle
            .emit("recording-finalized", &self.output_path)
        {
            tracing::error!("Failed to emit recording-finalized event: {error}");
            return;
        }

        self.finalized_emitted = true;
    }

    fn emit_preview_frame(&mut self, frame: &mut Frame) {
        let now = Instant::now();
        if now < self.next_preview_emit_at {
            return;
        }
        self.next_preview_emit_at = now + PREVIEW_FRAME_INTERVAL;

        let width = frame.width();
        let height = frame.height();

        let mut frame_buffer = match frame.buffer() {
            Ok(buffer) => buffer,
            Err(error) => {
                tracing::warn!("Failed to read frame buffer for recording preview: {error}");
                return;
            }
        };

        let pixels: &[u8] = if frame_buffer.has_padding() {
            let bytes_per_pixel = match frame_buffer.color_format() {
                ColorFormat::Rgba16F => 8usize,
                ColorFormat::Rgba8 | ColorFormat::Bgra8 => 4usize,
            };

            let width_usize = width as usize;
            let height_usize = height as usize;
            let row_bytes = width_usize * bytes_per_pixel;
            let row_pitch = frame_buffer.row_pitch() as usize;

            if row_bytes > row_pitch {
                tracing::warn!("Skipping recording preview frame due to invalid row pitch");
                return;
            }

            let frame_size = row_bytes * height_usize;
            self.preview_buffer.resize(frame_size, 0);

            let raw_buffer = frame_buffer.as_raw_buffer();

            for row in 0..height_usize {
                let source_start = row * row_pitch;
                let source_end = source_start + row_bytes;
                let target_start = row * row_bytes;
                let target_end = target_start + row_bytes;

                if source_end > raw_buffer.len() || target_end > self.preview_buffer.len() {
                    tracing::warn!("Skipping recording preview frame due to invalid buffer bounds");
                    return;
                }

                self.preview_buffer[target_start..target_end]
                    .copy_from_slice(&raw_buffer[source_start..source_end]);
            }

            &self.preview_buffer
        } else {
            frame_buffer.as_raw_buffer()
        };

        let jpeg_bytes = match self.preview_encoder.encode(pixels, width, height) {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!("Failed to encode recording preview frame: {error}");
                return;
            }
        };
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(jpeg_bytes);

        if let Err(error) = self
            .app_handle
            .emit("preview-frame", PreviewFramePayload { data_base64 })
        {
            tracing::warn!("Failed to emit recording preview frame: {error}");
        }
    }

    fn stop_requested(&mut self) -> bool {
        match self.stop_rx.try_recv() {
            Ok(()) => true,
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("Recording stop channel disconnected; stopping capture");
                true
            }
        }
    }
}

impl GraphicsCaptureApiHandler for RecordingHandler {
    type Flags = (
        String,
        u32,
        u32,
        crate::settings::RecordingSettings,
        AppHandle,
        mpsc::Receiver<()>,
        SharedRecordingState,
    );
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (output_path, width, height, settings, app_handle, stop_rx, state) = context.flags;

        let video_settings = VideoSettingsBuilder::new(width, height)
            .sub_type(VideoSettingsSubType::H264)
            .frame_rate(settings.frame_rate)
            .bitrate(settings.bitrate);

        let audio_settings = AudioSettingsBuilder::default().disabled(true);
        let container_settings = ContainerSettingsBuilder::default();

        let encoder = VideoEncoder::new(
            video_settings,
            audio_settings,
            container_settings,
            &output_path,
        )?;

        Ok(Self {
            app_handle,
            output_path,
            encoder: Some(encoder),
            preview_encoder: ImageEncoder::new(ImageFormat::Jpeg, ImageEncoderPixelFormat::Bgra8)?,
            preview_buffer: Vec::new(),
            next_preview_emit_at: Instant::now(),
            stop_rx,
            state,
            finalized_emitted: false,
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        capture_control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        if self.stop_requested() {
            self.finish_encoder_if_present("after stop request");
            self.emit_recording_finalized();
            capture_control.stop();
            return Ok(());
        }

        self.emit_preview_frame(frame);

        if let Some(encoder) = &mut self.encoder {
            encoder.send_frame(frame)?;
        }
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.finish_encoder_if_present("on close");
        self.emit_recording_finalized();
        clear_recording_state(&self.state);
        emit_recording_stopped(&self.app_handle);
        Ok(())
    }
}

pub struct RecordingState {
    is_recording: bool,
    current_output_path: Option<String>,
    stop_tx: Option<mpsc::Sender<()>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            current_output_path: None,
            stop_tx: None,
        }
    }
}

pub type SharedRecordingState = Arc<RwLock<RecordingState>>;

enum CaptureTarget {
    Monitor(Monitor),
    Window(Window),
}

fn sanitize_filename_segment(value: &str, fallback: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    let mut previous_was_separator = false;

    for character in value.chars() {
        let is_safe_character = character.is_ascii_alphanumeric();
        if is_safe_character {
            sanitized.push(character);
            previous_was_separator = false;
            continue;
        }

        if character.is_ascii_whitespace() || matches!(character, '-' | '_' | '.') {
            if !previous_was_separator && !sanitized.is_empty() {
                sanitized.push('-');
                previous_was_separator = true;
            }
        }
    }

    let sanitized = sanitized.trim_matches('-').to_string();
    let mut truncated = if sanitized.is_empty() {
        fallback.to_string()
    } else {
        sanitized
    };

    if truncated.len() > MAX_FILENAME_SEGMENT_LENGTH {
        truncated.truncate(MAX_FILENAME_SEGMENT_LENGTH);
        while truncated.ends_with('-') {
            truncated.pop();
        }
    }

    if truncated.is_empty() {
        return fallback.to_string();
    }

    truncated
}

fn window_id(window: &Window) -> String {
    format!("hwnd:{}", window.as_raw_hwnd() as usize)
}

fn find_window_by_selection(selection: &str) -> Result<Window, String> {
    let windows = Window::enumerate().map_err(|error| error.to_string())?;

    if let Some(window) = windows
        .iter()
        .copied()
        .find(|window| window_id(window) == selection)
    {
        return Ok(window);
    }

    if let Some(window) = windows.iter().copied().find(|window| {
        window
            .title()
            .map(|title| title.trim() == selection)
            .unwrap_or(false)
    }) {
        return Ok(window);
    }

    Window::from_contains_name(selection).map_err(|_| {
        format!("Could not find window '{selection}'. Refresh the window list and try again.")
    })
}

fn resolve_capture_target(
    capture_source: &str,
    selected_window: Option<&str>,
) -> Result<(CaptureTarget, u32, u32, String), String> {
    match capture_source {
        "primary-monitor" => {
            let monitor = Monitor::primary().map_err(|error| error.to_string())?;
            let width = monitor.width().map_err(|error| error.to_string())?;
            let height = monitor.height().map_err(|error| error.to_string())?;
            Ok((
                CaptureTarget::Monitor(monitor),
                width,
                height,
                "screen".to_string(),
            ))
        }
        "window" => {
            let selected_value = selected_window
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or("Select a window before starting recording".to_string())?;

            let window = find_window_by_selection(selected_value)?;

            let window_name = window
                .title()
                .unwrap_or_else(|_| selected_value.to_string());

            if !window.is_valid() {
                return Err(format!(
                    "Window '{window_name}' is not capturable right now. Try a different window."
                ));
            }

            let width = window.width().map_err(|error| error.to_string())?;
            let height = window.height().map_err(|error| error.to_string())?;

            if width <= 0 || height <= 0 {
                return Err(format!(
                    "Window '{window_name}' has an invalid size and cannot be captured."
                ));
            }

            let process_name = window
                .process_name()
                .map(|value| value.trim().to_lowercase())
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "window".to_string());
            let sanitized_window_name = sanitize_filename_segment(&process_name, "window");

            Ok((
                CaptureTarget::Window(window),
                width as u32,
                height as u32,
                format!("window_{}", sanitized_window_name),
            ))
        }
        _ => Err(format!("Unsupported capture source: {capture_source}")),
    }
}

fn minimum_update_interval_for_frame_rate(frame_rate: u32) -> Duration {
    let bounded_frame_rate = frame_rate.max(1) as u64;
    Duration::from_millis((1000 / bounded_frame_rate).max(1))
}

fn clear_recording_state(state: &SharedRecordingState) {
    match state.try_write() {
        Ok(mut recording_state) => {
            recording_state.is_recording = false;
            recording_state.current_output_path = None;
            recording_state.stop_tx = None;
        }
        Err(_) => {
            tracing::warn!("Could not clear recording state immediately due to lock contention");
        }
    }
}

fn emit_recording_stopped(app_handle: &AppHandle) {
    if let Err(error) = app_handle.emit("recording-stopped", ()) {
        tracing::error!("Failed to emit recording-stopped event: {error}");
    }
}

fn build_recording_flags(
    output_path: String,
    width: u32,
    height: u32,
    recording_settings: crate::settings::RecordingSettings,
    app_handle: AppHandle,
    stop_rx: mpsc::Receiver<()>,
    state: SharedRecordingState,
) -> RecordingHandlerFlags {
    (
        output_path,
        width,
        height,
        recording_settings,
        app_handle,
        stop_rx,
        state,
    )
}

fn build_recording_capture_settings<TCaptureTarget>(
    capture_target: TCaptureTarget,
    minimum_update_interval: Duration,
    flags: RecordingHandlerFlags,
) -> Settings<RecordingHandlerFlags, TCaptureTarget>
where
    TCaptureTarget: TryInto<windows_capture::settings::GraphicsCaptureItemType>,
{
    Settings::new(
        capture_target,
        CursorCaptureSettings::Default,
        DrawBorderSettings::WithoutBorder,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Custom(minimum_update_interval),
        DirtyRegionSettings::ReportAndRender,
        ColorFormat::Bgra8,
        flags,
    )
}

fn spawn_recording_task<TCaptureTarget>(
    capture_settings: Settings<RecordingHandlerFlags, TCaptureTarget>,
    state: SharedRecordingState,
    app_handle: AppHandle,
) where
    TCaptureTarget: TryInto<windows_capture::settings::GraphicsCaptureItemType>,
    Settings<RecordingHandlerFlags, TCaptureTarget>: Send + 'static,
{
    tokio::spawn(async move {
        if let Err(error) = RecordingHandler::start(capture_settings) {
            tracing::error!("Recording error: {error}");
            clear_recording_state(&state);
            emit_recording_stopped(&app_handle);
        }
    });
}

#[tauri::command]
pub async fn start_recording(
    app_handle: AppHandle,
    state: tauri::State<'_, SharedRecordingState>,
    settings: crate::settings::RecordingSettings,
    output_folder: String,
    max_storage_bytes: u64,
    capture_source: String,
    selected_window: Option<String>,
) -> Result<RecordingStartedPayload, String> {
    {
        let recording_state = state.read().await;
        if recording_state.is_recording {
            return Err("Recording already in progress".to_string());
        }
    }

    std::fs::create_dir_all(&output_folder)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    let (capture_target, width, height, capture_name_prefix) =
        resolve_capture_target(capture_source.as_str(), selected_window.as_deref())?;

    let effective_bitrate = settings.effective_bitrate(width, height);
    let estimated_size = settings.estimate_size_bytes_for_capture(width, height);

    tracing::info!(
        video_quality = %settings.video_quality,
        frame_rate = settings.frame_rate,
        capture_width = width,
        capture_height = height,
        requested_bitrate_bps = settings.bitrate,
        effective_bitrate_bps = effective_bitrate,
        "Using adaptive recording bitrate"
    );

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
    let filename = format!("{}_recording_{}.mp4", capture_name_prefix, timestamp);
    let output_path = std::path::Path::new(&output_folder).join(filename);
    let output_path_str = output_path.to_string_lossy().to_string();

    let mut recording_settings = settings;
    recording_settings.bitrate = effective_bitrate;
    let minimum_update_interval =
        minimum_update_interval_for_frame_rate(recording_settings.frame_rate);

    let (stop_tx, stop_rx) = mpsc::channel(1);

    {
        let mut recording_state = state.write().await;
        if recording_state.is_recording {
            return Err("Recording already in progress".to_string());
        }

        recording_state.is_recording = true;
        recording_state.current_output_path = Some(output_path_str.clone());
        recording_state.stop_tx = Some(stop_tx);
    }

    let shared_state = state.inner().clone();
    let app_handle_for_task = app_handle.clone();
    let handler_flags = build_recording_flags(
        output_path_str.clone(),
        width,
        height,
        recording_settings,
        app_handle.clone(),
        stop_rx,
        state.inner().clone(),
    );

    match capture_target {
        CaptureTarget::Monitor(monitor) => {
            let capture_settings = build_recording_capture_settings(
                monitor,
                minimum_update_interval,
                handler_flags,
            );
            spawn_recording_task(capture_settings, shared_state, app_handle_for_task);
        }
        CaptureTarget::Window(window) => {
            let capture_settings = build_recording_capture_settings(
                window,
                minimum_update_interval,
                handler_flags,
            );
            spawn_recording_task(capture_settings, shared_state, app_handle_for_task);
        }
    }

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

        recording_state.is_recording = false;

        (output_path, recording_state.stop_tx.take())
    };

    if let Some(stop_tx) = stop_tx {
        if let Err(error) = stop_tx.send(()).await {
            tracing::warn!("Failed to send stop signal to recording task: {error}");
        }
    }

    Ok(output_path)
}
