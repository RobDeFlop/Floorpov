//! Native Windows recording backend.

mod capture;
mod timing;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tokio::sync::mpsc;
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, ContainerSettingsSubType, VideoEncoder,
    VideoSettingsBuilder, VideoSettingsSubType,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use self::capture::{
    start_monitor_capture, start_window_capture, CaptureEvent, CaptureShared, CaptureStats,
    NativeCaptureControl,
};
use self::timing::{black_heartbeat_due, FrameGate, QpcClock, TimestampReservation};
use super::audio_pipeline::{
    run_audio_queue_to_consumer, run_system_audio_capture_to_queue_with_startup,
};
use super::backend::{RecordingBackendKind, RecordingFailurePhase, RecordingRunOutcome};
use super::model::{
    AudioPipelineStats, CaptureInput, RecordingSessionConfig, WindowCaptureAvailability,
    SYSTEM_AUDIO_QUEUE_CAPACITY, WINDOW_CAPTURE_STATUS_POLL_INTERVAL,
    WINDOW_CAPTURE_UNAVAILABLE_WARNING,
};
use super::segments::promote_output_file;
use super::session::{emit_recording_warning, emit_recording_warning_cleared, StartupNotifier};
use super::window_capture::{
    evaluate_window_capture_availability, resolve_window_capture_handle,
    warning_message_for_window_capture,
};

struct WinRtMtaGuard;

impl WinRtMtaGuard {
    fn initialize() -> Result<Self, String> {
        let result = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
        if result < 0 {
            return Err(format!(
                "Failed to initialize the native recording thread as MTA: HRESULT {result:#x}"
            ));
        }
        Ok(Self)
    }
}

impl Drop for WinRtMtaGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn native_partial_path(output_path: &str) -> PathBuf {
    let mut path = PathBuf::from(output_path);
    let partial_name = path
        .file_name()
        .map(|name| format!("{}.native-partial", name.to_string_lossy()))
        .unwrap_or_else(|| "recording.mp4.native-partial".to_string());
    path.set_file_name(partial_name);
    path
}

fn create_black_frame(width: u32, height: u32) -> Result<Vec<u8>, String> {
    let length = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(height as usize))
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| "Native black frame dimensions overflowed".to_string())?;
    let mut frame = vec![0u8; length];
    for alpha in frame.iter_mut().skip(3).step_by(4) {
        *alpha = u8::MAX;
    }
    Ok(frame)
}

fn send_black_frame(
    encoder: &Arc<Mutex<Option<VideoEncoder>>>,
    submission_lock: &Arc<Mutex<()>>,
    timestamps: &Arc<Mutex<TimestampReservation>>,
    frame: &[u8],
    timestamp: i64,
) -> Result<bool, String> {
    let _submission_guard = submission_lock
        .lock()
        .map_err(|_| "Native submission lock was poisoned".to_string())?;
    let reserved_timestamp = timestamps
        .lock()
        .map_err(|_| "Native timestamp lock was poisoned".to_string())?
        .reserve(timestamp);
    let Some(timestamp) = reserved_timestamp else {
        return Ok(false);
    };
    let mut encoder_guard = encoder
        .lock()
        .map_err(|_| "Native encoder lock was poisoned".to_string())?;
    let encoder = encoder_guard
        .as_mut()
        .ok_or_else(|| "Native encoder is no longer available".to_string())?;
    encoder
        .send_frame_buffer(frame, timestamp)
        .map_err(|error| format!("Failed to submit native black frame: {error}"))?;
    Ok(true)
}

fn remove_startup_output(path: &Path) {
    if path.exists() {
        if let Err(error) = fs::remove_file(path) {
            tracing::warn!(
                backend = "native",
                output_path = %path.display(),
                "Failed to remove native startup output: {error}"
            );
        }
    }
}

fn cleanup_startup_encoder(encoder: &Arc<Mutex<Option<VideoEncoder>>>, path: &Path) {
    let encoder = encoder.lock().ok().and_then(|mut guard| guard.take());
    if let Some(encoder) = encoder {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| encoder.finish()));
    }
    remove_startup_output(path);
}

struct NativeAudioPipeline {
    capture_stop_tx: std_mpsc::Sender<()>,
    capture_thread: JoinHandle<Result<(), String>>,
    consumer_thread: JoinHandle<Result<(), String>>,
    error_rx: std_mpsc::Receiver<String>,
    stats: Arc<AudioPipelineStats>,
}

impl NativeAudioPipeline {
    fn start(encoder: Arc<Mutex<Option<VideoEncoder>>>) -> Result<Self, String> {
        let (audio_tx, audio_rx) = std_mpsc::sync_channel::<Vec<u8>>(SYSTEM_AUDIO_QUEUE_CAPACITY);
        let (capture_stop_tx, capture_stop_rx) = std_mpsc::channel();
        let (startup_tx, startup_rx) = std_mpsc::channel();
        let (error_tx, error_rx) = std_mpsc::channel();
        let stats = Arc::new(AudioPipelineStats::default());

        let consumer_stats = Arc::clone(&stats);
        let consumer_error_tx = error_tx.clone();
        let consumer_thread = thread::spawn(move || {
            let result = run_audio_queue_to_consumer(audio_rx, consumer_stats, |chunk| {
                let mut encoder_guard = encoder
                    .lock()
                    .map_err(|_| "Native encoder lock was poisoned by audio".to_string())?;
                let encoder = encoder_guard
                    .as_mut()
                    .ok_or_else(|| "Native encoder closed before audio drained".to_string())?;
                encoder
                    .send_audio_buffer(chunk, 0)
                    .map_err(|error| format!("Failed to submit native audio: {error}"))
            });
            if let Err(message) = &result {
                let _ = consumer_error_tx.send(message.clone());
            }
            result
        });

        let capture_stats = Arc::clone(&stats);
        let capture_thread = thread::spawn(move || {
            let result = run_system_audio_capture_to_queue_with_startup(
                audio_tx,
                capture_stop_rx,
                capture_stats,
                Some(startup_tx),
            );
            if let Err(message) = &result {
                let _ = error_tx.send(message.clone());
            }
            result
        });

        match startup_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                capture_stop_tx,
                capture_thread,
                consumer_thread,
                error_rx,
                stats,
            }),
            Ok(Err(message)) => {
                let _ = capture_stop_tx.send(());
                let _ = capture_thread.join();
                let _ = consumer_thread.join();
                Err(message)
            }
            Err(error) => {
                let _ = capture_stop_tx.send(());
                let _ = capture_thread.join();
                let _ = consumer_thread.join();
                Err(format!("System audio startup did not complete: {error}"))
            }
        }
    }

    fn runtime_error(&self) -> Option<String> {
        self.error_rx.try_recv().ok()
    }

    fn stop(self) -> Result<(), String> {
        if let Err(error) = self.capture_stop_tx.send(()) {
            tracing::debug!("Native audio capture stop channel was closed: {error}");
        }
        let capture_result = match self.capture_thread.join() {
            Ok(result) => result,
            Err(error) => Err(format!("System audio capture thread panicked: {error:?}")),
        };
        let consumer_result = match self.consumer_thread.join() {
            Ok(result) => result,
            Err(error) => Err(format!("Native audio consumer thread panicked: {error:?}")),
        };

        tracing::info!(
            backend = "native",
            audio_chunks_queued = self.stats.queued_chunks.load(Ordering::Relaxed),
            audio_chunks_written = self.stats.dequeued_chunks.load(Ordering::Relaxed),
            audio_chunks_dropped = self.stats.dropped_chunks.load(Ordering::Relaxed),
            "Native audio pipeline stopped"
        );
        capture_result.and(consumer_result)
    }
}

fn stop_audio_pipeline(audio_pipeline: &mut Option<NativeAudioPipeline>) -> Result<(), String> {
    match audio_pipeline.take() {
        Some(pipeline) => pipeline.stop(),
        None => Ok(()),
    }
}

struct ActiveCapture {
    control: NativeCaptureControl,
    event_rx: std_mpsc::Receiver<CaptureEvent>,
    stats: Arc<CaptureStats>,
}

impl ActiveCapture {
    fn next_event(&self) -> Result<Option<CaptureEvent>, String> {
        match self.event_rx.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(std_mpsc::TryRecvError::Empty) if self.control.is_finished() => {
                Err("Native capture thread ended unexpectedly".to_string())
            }
            Err(std_mpsc::TryRecvError::Disconnected) => {
                Err("Native capture event channel disconnected".to_string())
            }
            Err(std_mpsc::TryRecvError::Empty) => Ok(None),
        }
    }

    fn stop(self) -> Result<(), String> {
        let result = self
            .control
            .stop()
            .map_err(|error| format!("Failed to stop native capture: {error}"));
        tracing::info!(
            backend = "native",
            captured_frames = self.stats.captured_frames.load(Ordering::Relaxed),
            accepted_frames = self.stats.accepted_frames.load(Ordering::Relaxed),
            dropped_frames = self.stats.dropped_frames.load(Ordering::Relaxed),
            submission_failures = self.stats.submission_failures.load(Ordering::Relaxed),
            "Native capture stopped"
        );
        result
    }
}

fn stop_capture(active_capture: &mut Option<ActiveCapture>) -> Result<(), String> {
    match active_capture.take() {
        Some(capture) => capture.stop(),
        None => Ok(()),
    }
}

fn update_window_warning(
    app_handle: Option<&AppHandle>,
    active_warning: &mut Option<&'static str>,
    next_warning: Option<&'static str>,
) {
    if *active_warning == next_warning {
        return;
    }
    if let Some(app_handle) = app_handle {
        if let Some(message) = next_warning {
            emit_recording_warning(app_handle, message);
        } else {
            emit_recording_warning_cleared(app_handle);
        }
    }
    *active_warning = next_warning;
}

pub(crate) fn run_recording_session(
    app_handle: &AppHandle,
    session_config: &RecordingSessionConfig,
    stop_rx: &mut mpsc::Receiver<()>,
    startup_notifier: &mut StartupNotifier,
) -> RecordingRunOutcome {
    run_encoder_session(Some(app_handle), session_config, stop_rx, startup_notifier)
}

fn run_encoder_session(
    app_handle: Option<&AppHandle>,
    session_config: &RecordingSessionConfig,
    stop_rx: &mut mpsc::Receiver<()>,
    startup_notifier: &mut StartupNotifier,
) -> RecordingRunOutcome {
    let partial_path = native_partial_path(&session_config.output_path);
    remove_startup_output(&partial_path);

    let _mta_guard = match WinRtMtaGuard::initialize() {
        Ok(guard) => guard,
        Err(message) => return startup_failure(message),
    };
    let clock = match QpcClock::new() {
        Ok(clock) => clock,
        Err(message) => return startup_failure(message),
    };
    let black_frame =
        match create_black_frame(session_config.capture_width, session_config.capture_height) {
            Ok(frame) => frame,
            Err(message) => return startup_failure(message),
        };

    let encoder = match VideoEncoder::new(
        VideoSettingsBuilder::new(session_config.capture_width, session_config.capture_height)
            .sub_type(VideoSettingsSubType::H264)
            .bitrate(session_config.bitrate)
            .frame_rate(session_config.frame_rate),
        AudioSettingsBuilder::new().disabled(!session_config.include_system_audio),
        ContainerSettingsBuilder::new().sub_type(ContainerSettingsSubType::MPEG4),
        &partial_path,
    ) {
        Ok(encoder) => Arc::new(Mutex::new(Some(encoder))),
        Err(error) => {
            remove_startup_output(&partial_path);
            return startup_failure(format!(
                "Media Foundation encoder initialization failed: {error}"
            ));
        }
    };

    let timestamps = Arc::new(Mutex::new(TimestampReservation::default()));
    let submission_lock = Arc::new(Mutex::new(()));
    let initial_timestamp = match clock.now_100ns() {
        Ok(timestamp) => timestamp,
        Err(message) => {
            cleanup_startup_encoder(&encoder, &partial_path);
            return startup_failure(message);
        }
    };
    if let Err(message) = send_black_frame(
        &encoder,
        &submission_lock,
        &timestamps,
        &black_frame,
        initial_timestamp,
    ) {
        cleanup_startup_encoder(&encoder, &partial_path);
        return startup_failure(message);
    }

    let mut audio_pipeline = if session_config.include_system_audio {
        match NativeAudioPipeline::start(Arc::clone(&encoder)) {
            Ok(pipeline) => Some(pipeline),
            Err(message) => {
                cleanup_startup_encoder(&encoder, &partial_path);
                return startup_failure(format!(
                    "System audio cannot initialize for native recording: {message}"
                ));
            }
        }
    } else {
        None
    };

    let capture_stats = Arc::new(CaptureStats::default());
    let capture_shared = CaptureShared {
        encoder: Arc::clone(&encoder),
        submission_lock: Arc::clone(&submission_lock),
        timestamps: Arc::clone(&timestamps),
        frame_gate: Arc::new(Mutex::new(FrameGate::new(session_config.frame_rate))),
        clock,
        output_width: session_config.capture_width,
        output_height: session_config.capture_height,
        enable_diagnostics: session_config.enable_diagnostics,
        dimension_warning_sent: Arc::new(AtomicBool::new(false)),
        diagnostic_delta_logged: Arc::new(AtomicBool::new(false)),
        stats: Arc::clone(&capture_stats),
    };
    let is_window_capture = matches!(&session_config.capture_input, CaptureInput::Window { .. });
    let initial_window_availability =
        evaluate_window_capture_availability(&session_config.capture_input);
    let mut last_window_hwnd = match &session_config.capture_input {
        CaptureInput::Window { window_hwnd, .. } => *window_hwnd,
        CaptureInput::Monitor => None,
    };
    let mut active_capture = match &session_config.capture_input {
        CaptureInput::Monitor => {
            match start_monitor_capture(capture_shared.clone(), session_config.frame_rate) {
                Ok((control, event_rx)) => Some(ActiveCapture {
                    control,
                    event_rx,
                    stats: Arc::clone(&capture_stats),
                }),
                Err(message) => {
                    let _ = stop_audio_pipeline(&mut audio_pipeline);
                    cleanup_startup_encoder(&encoder, &partial_path);
                    return startup_failure(message);
                }
            }
        }
        CaptureInput::Window { .. }
            if initial_window_availability == WindowCaptureAvailability::Available =>
        {
            let window_hwnd = match resolve_window_capture_handle(&session_config.capture_input) {
                Ok(hwnd) => hwnd,
                Err(message) => {
                    let _ = stop_audio_pipeline(&mut audio_pipeline);
                    cleanup_startup_encoder(&encoder, &partial_path);
                    return startup_failure(message);
                }
            };
            match start_window_capture(
                window_hwnd,
                capture_shared.clone(),
                session_config.frame_rate,
            ) {
                Ok((control, event_rx)) => {
                    last_window_hwnd = Some(window_hwnd);
                    Some(ActiveCapture {
                        control,
                        event_rx,
                        stats: Arc::clone(&capture_stats),
                    })
                }
                Err(message) => {
                    let _ = stop_audio_pipeline(&mut audio_pipeline);
                    cleanup_startup_encoder(&encoder, &partial_path);
                    return startup_failure(message);
                }
            }
        }
        CaptureInput::Window { .. } => None,
    };
    let capture_source = match (&session_config.capture_input, active_capture.is_some()) {
        (CaptureInput::Monitor, true) => "monitor",
        (CaptureInput::Window { .. }, true) => "window",
        _ => "black",
    };
    let mut active_window_warning = if is_window_capture && active_capture.is_none() {
        warning_message_for_window_capture(initial_window_availability)
            .or(Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING))
    } else {
        None
    };
    if let (Some(app_handle), Some(message)) = (app_handle, active_window_warning) {
        emit_recording_warning(app_handle, message);
    }

    startup_notifier.notify_success();
    tracing::info!(
        backend = "native",
        capture_source,
        output_path = %session_config.output_path,
        width = session_config.capture_width,
        height = session_config.capture_height,
        frame_rate = session_config.frame_rate,
        bitrate = session_config.bitrate,
        include_system_audio = session_config.include_system_audio,
        "Native encoder started"
    );

    let mut last_black_timestamp = initial_timestamp;
    let mut window_status_checked_at = Instant::now();
    let mut window_retry_at = Instant::now();
    let mut window_retry_delay = Duration::from_millis(250);
    let mut recovered_hwnd_logged = false;
    loop {
        let capture_event = match active_capture.as_ref() {
            Some(capture) => capture.next_event(),
            None => Ok(None),
        };
        match capture_event {
            Ok(Some(CaptureEvent::FirstFrame)) if is_window_capture => {
                update_window_warning(app_handle, &mut active_window_warning, None);
                window_retry_delay = Duration::from_millis(250);
            }
            Ok(Some(CaptureEvent::FirstFrame)) | Ok(None) => {}
            Ok(Some(CaptureEvent::Closed)) if is_window_capture => {
                if let Err(error) = stop_capture(&mut active_capture) {
                    tracing::debug!(
                        backend = "native",
                        "Closed window capture join returned: {error}"
                    );
                }
                update_window_warning(
                    app_handle,
                    &mut active_window_warning,
                    Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING),
                );
                if let Ok(timestamp) = clock.now_100ns() {
                    match send_black_frame(
                        &encoder,
                        &submission_lock,
                        &timestamps,
                        &black_frame,
                        timestamp,
                    ) {
                        Ok(true) => last_black_timestamp = timestamp,
                        Ok(false) => {}
                        Err(message) => {
                            let _ = stop_audio_pipeline(&mut audio_pipeline);
                            return runtime_failure(message, &partial_path);
                        }
                    }
                }
                window_retry_at = Instant::now() + window_retry_delay;
            }
            Ok(Some(CaptureEvent::Closed)) => {
                let _ = stop_capture(&mut active_capture);
                let _ = stop_audio_pipeline(&mut audio_pipeline);
                return runtime_failure(
                    "Native monitor capture closed unexpectedly".to_string(),
                    &partial_path,
                );
            }
            Err(message) if is_window_capture => {
                if let Err(error) = stop_capture(&mut active_capture) {
                    tracing::debug!(backend = "native", "Window capture join returned: {error}");
                }
                update_window_warning(
                    app_handle,
                    &mut active_window_warning,
                    Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING),
                );
                tracing::warn!(backend = "native", "Window capture ended: {message}");
                window_retry_at = Instant::now() + window_retry_delay;
            }
            Err(message) => {
                let _ = stop_capture(&mut active_capture);
                let _ = stop_audio_pipeline(&mut audio_pipeline);
                return runtime_failure(message, &partial_path);
            }
        }

        if let Some(message) = audio_pipeline
            .as_ref()
            .and_then(NativeAudioPipeline::runtime_error)
        {
            let _ = stop_capture(&mut active_capture);
            let _ = stop_audio_pipeline(&mut audio_pipeline);
            return runtime_failure(message, &partial_path);
        }

        match stop_rx.try_recv() {
            Ok(()) | Err(mpsc::error::TryRecvError::Disconnected) => break,
            Err(mpsc::error::TryRecvError::Empty) => {}
        }

        if is_window_capture
            && window_status_checked_at.elapsed() >= WINDOW_CAPTURE_STATUS_POLL_INTERVAL
        {
            window_status_checked_at = Instant::now();
            let availability = evaluate_window_capture_availability(&session_config.capture_input);

            if active_capture.is_some() && availability != WindowCaptureAvailability::Available {
                if let Err(error) = stop_capture(&mut active_capture) {
                    tracing::debug!(backend = "native", "Window capture stop returned: {error}");
                }
                update_window_warning(
                    app_handle,
                    &mut active_window_warning,
                    warning_message_for_window_capture(availability)
                        .or(Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING)),
                );
                if let Ok(timestamp) = clock.now_100ns() {
                    match send_black_frame(
                        &encoder,
                        &submission_lock,
                        &timestamps,
                        &black_frame,
                        timestamp,
                    ) {
                        Ok(true) => last_black_timestamp = timestamp,
                        Ok(false) => {}
                        Err(message) => {
                            let _ = stop_audio_pipeline(&mut audio_pipeline);
                            return runtime_failure(message, &partial_path);
                        }
                    }
                }
                window_retry_at = Instant::now() + window_retry_delay;
            } else if active_capture.is_none() {
                let next_warning = if availability == WindowCaptureAvailability::Available {
                    Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING)
                } else {
                    warning_message_for_window_capture(availability)
                        .or(Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING))
                };
                update_window_warning(app_handle, &mut active_window_warning, next_warning);

                if availability == WindowCaptureAvailability::Available
                    && Instant::now() >= window_retry_at
                {
                    let start_result = resolve_window_capture_handle(&session_config.capture_input)
                        .and_then(|window_hwnd| {
                            start_window_capture(
                                window_hwnd,
                                capture_shared.clone(),
                                session_config.frame_rate,
                            )
                            .map(|(control, event_rx)| (window_hwnd, control, event_rx))
                        });
                    match start_result {
                        Ok((window_hwnd, control, event_rx)) => {
                            if last_window_hwnd.is_some_and(|previous| previous != window_hwnd)
                                && !recovered_hwnd_logged
                            {
                                tracing::info!(
                                    backend = "native",
                                    previous_hwnd = last_window_hwnd,
                                    recovered_hwnd = window_hwnd,
                                    "Recovered native window capture with a new HWND"
                                );
                                recovered_hwnd_logged = true;
                            }
                            last_window_hwnd = Some(window_hwnd);
                            active_capture = Some(ActiveCapture {
                                control,
                                event_rx,
                                stats: Arc::clone(&capture_stats),
                            });
                        }
                        Err(error) => {
                            tracing::warn!(
                                backend = "native",
                                retry_delay_ms = window_retry_delay.as_millis(),
                                "Native window capture retry failed: {error}"
                            );
                            window_retry_at = Instant::now() + window_retry_delay;
                            window_retry_delay = window_retry_delay
                                .saturating_mul(2)
                                .min(Duration::from_secs(2));
                        }
                    }
                }
            }
        }

        match clock.now_100ns() {
            Ok(timestamp)
                if active_capture.is_none()
                    && black_heartbeat_due(last_black_timestamp, timestamp) =>
            {
                match send_black_frame(
                    &encoder,
                    &submission_lock,
                    &timestamps,
                    &black_frame,
                    timestamp,
                ) {
                    Ok(true) => last_black_timestamp = timestamp,
                    Ok(false) => {}
                    Err(message) => {
                        let _ = stop_audio_pipeline(&mut audio_pipeline);
                        return runtime_failure(message, &partial_path);
                    }
                }
            }
            Ok(_) => {}
            Err(message) => {
                let _ = stop_capture(&mut active_capture);
                let _ = stop_audio_pipeline(&mut audio_pipeline);
                return runtime_failure(message, &partial_path);
            }
        }
        thread::sleep(Duration::from_millis(25));
    }

    let was_black_mode = is_window_capture && active_capture.is_none();
    let capture_stop_result = stop_capture(&mut active_capture);
    let audio_stop_result = stop_audio_pipeline(&mut audio_pipeline);
    if let Err(message) = capture_stop_result.and(audio_stop_result) {
        return runtime_failure(message, &partial_path);
    }

    if was_black_mode {
        if let Ok(timestamp) = clock.now_100ns() {
            if let Err(message) = send_black_frame(
                &encoder,
                &submission_lock,
                &timestamps,
                &black_frame,
                timestamp,
            ) {
                return runtime_failure(message, &partial_path);
            }
        }
    }

    let encoder = match encoder.lock() {
        Ok(mut guard) => guard.take(),
        Err(_) => {
            return runtime_failure(
                "Native encoder lock was poisoned during finalization".to_string(),
                &partial_path,
            );
        }
    };
    let Some(encoder) = encoder else {
        return runtime_failure(
            "Native encoder was missing during finalization".to_string(),
            &partial_path,
        );
    };
    let finish_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| encoder.finish()));
    match finish_result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return finalization_failure(
                format!("Native output could not finalize: {error}"),
                &partial_path,
            );
        }
        Err(_) => {
            return finalization_failure(
                "Native encoder panicked during finalization".to_string(),
                &partial_path,
            );
        }
    }

    if !partial_path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > 0)
    {
        return finalization_failure(
            "Native encoder produced an empty output".to_string(),
            &partial_path,
        );
    }
    if let Err(message) = promote_output_file(&partial_path, Path::new(&session_config.output_path))
    {
        return finalization_failure(message, &partial_path);
    }

    RecordingRunOutcome::Finalized {
        backend: RecordingBackendKind::NativeWindows,
    }
}

fn startup_failure(message: String) -> RecordingRunOutcome {
    RecordingRunOutcome::Failed {
        backend: RecordingBackendKind::NativeWindows,
        phase: RecordingFailurePhase::Startup,
        message,
        startup_acknowledged: false,
    }
}

fn runtime_failure(message: String, partial_path: &Path) -> RecordingRunOutcome {
    tracing::error!(
        backend = "native",
        phase = "runtime",
        partial_path = %partial_path.display(),
        "Native recording failed; partial output was retained: {message}"
    );
    RecordingRunOutcome::Failed {
        backend: RecordingBackendKind::NativeWindows,
        phase: RecordingFailurePhase::Runtime,
        message,
        startup_acknowledged: true,
    }
}

fn finalization_failure(message: String, partial_path: &Path) -> RecordingRunOutcome {
    tracing::error!(
        backend = "native",
        phase = "finalization",
        partial_path = %partial_path.display(),
        "Native recording finalization failed; partial output was retained: {message}"
    );
    RecordingRunOutcome::Failed {
        backend: RecordingBackendKind::NativeWindows,
        phase: RecordingFailurePhase::Finalization,
        message,
        startup_acknowledged: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use windows_capture::capture::{Context, GraphicsCaptureApiHandler};
    use windows_capture::encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, ContainerSettingsSubType, VideoEncoder,
        VideoSettingsBuilder, VideoSettingsSubType,
    };
    use windows_capture::frame::Frame;
    use windows_capture::graphics_capture_api::InternalCaptureControl;
    use windows_capture::monitor::Monitor;
    use windows_capture::settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    };
    use windows_capture::window::Window;

    fn temporary_mp4_path(test_name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("floorpov-{test_name}-{suffix}.mp4"))
    }

    #[test]
    fn black_frame_has_expected_size_and_opaque_alpha() {
        let frame = create_black_frame(2, 2).expect("small black frame should fit");
        assert_eq!(frame.len(), 16);
        assert!(frame.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 255]));
        assert!(create_black_frame(u32::MAX, u32::MAX).is_err());
    }

    #[test]
    #[ignore = "requires Windows Media Foundation H.264 support"]
    fn native_black_recording_session() -> Result<(), String> {
        let output_path = temporary_mp4_path("native-black-recording-session");
        let output_path_string = output_path.to_string_lossy().to_string();
        let session_config = RecordingSessionConfig {
            output_path: output_path_string,
            video_quality: "balanced".to_string(),
            video_encoder_preference: "auto".to_string(),
            frame_rate: 30,
            bitrate: 2_000_000,
            capture_input: super::super::model::CaptureInput::Window {
                input_target: "test-window".to_string(),
                window_hwnd: None,
                window_title: None,
                use_wgc: true,
            },
            capture_width: 640,
            capture_height: 360,
            include_system_audio: false,
            enable_diagnostics: false,
        };
        let (stop_tx, mut stop_rx) = tokio::sync::mpsc::channel(1);
        stop_tx
            .try_send(())
            .map_err(|error| format!("Failed to queue test stop: {error}"))?;
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        let mut startup_notifier = StartupNotifier::new(startup_tx);

        let outcome =
            run_encoder_session(None, &session_config, &mut stop_rx, &mut startup_notifier);

        assert!(matches!(
            outcome,
            RecordingRunOutcome::Finalized {
                backend: RecordingBackendKind::NativeWindows
            }
        ));
        assert!(matches!(startup_rx.blocking_recv(), Ok(Ok(()))));
        assert!(output_path
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0));
        fs::remove_file(output_path)
            .map_err(|error| format!("Failed to remove native session test output: {error}"))?;
        Ok(())
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop and Media Foundation H.264 support"]
    fn native_monitor_recording_session() -> Result<(), String> {
        let monitor = Monitor::primary()
            .map_err(|error| format!("Failed to resolve primary monitor: {error}"))?;
        let width = monitor
            .width()
            .map_err(|error| format!("Failed to read monitor width: {error}"))?;
        let height = monitor
            .height()
            .map_err(|error| format!("Failed to read monitor height: {error}"))?;
        let output_path = temporary_mp4_path("native-monitor-recording-session");
        let session_config = RecordingSessionConfig {
            output_path: output_path.to_string_lossy().to_string(),
            video_quality: "balanced".to_string(),
            video_encoder_preference: "auto".to_string(),
            frame_rate: 30,
            bitrate: 5_000_000,
            capture_input: super::super::model::CaptureInput::Monitor,
            capture_width: width,
            capture_height: height,
            include_system_audio: false,
            enable_diagnostics: false,
        };
        let (stop_tx, mut stop_rx) = tokio::sync::mpsc::channel(1);
        let stop_thread = thread::spawn(move || {
            thread::sleep(Duration::from_secs(1));
            stop_tx.blocking_send(())
        });
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        let mut startup_notifier = StartupNotifier::new(startup_tx);

        let outcome =
            run_encoder_session(None, &session_config, &mut stop_rx, &mut startup_notifier);

        stop_thread
            .join()
            .map_err(|error| format!("Monitor test stop thread panicked: {error:?}"))?
            .map_err(|error| format!("Failed to stop monitor test: {error}"))?;
        assert!(matches!(startup_rx.blocking_recv(), Ok(Ok(()))));
        assert!(matches!(outcome, RecordingRunOutcome::Finalized { .. }));
        assert!(output_path
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0));
        fs::remove_file(output_path)
            .map_err(|error| format!("Failed to remove monitor test output: {error}"))?;
        Ok(())
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop and Media Foundation H.264 support"]
    fn native_window_capture_smoke() -> Result<(), String> {
        let window = Window::foreground()
            .map_err(|error| format!("Failed to resolve foreground window: {error}"))?;
        let hwnd = window.as_raw_hwnd() as usize;
        let width = u32::try_from(window.width().unwrap_or(1280).max(2))
            .map_err(|error| format!("Invalid window width: {error}"))?;
        let height = u32::try_from(window.height().unwrap_or(720).max(2))
            .map_err(|error| format!("Invalid window height: {error}"))?;
        let (width, height) =
            super::super::window_capture::sanitize_capture_dimensions(width, height);
        let output_path = temporary_mp4_path("native-window-capture-smoke");
        let session_config = RecordingSessionConfig {
            output_path: output_path.to_string_lossy().to_string(),
            video_quality: "balanced".to_string(),
            video_encoder_preference: "auto".to_string(),
            frame_rate: 30,
            bitrate: 3_000_000,
            capture_input: super::super::model::CaptureInput::Window {
                input_target: format!("hwnd={hwnd}"),
                window_hwnd: Some(hwnd),
                window_title: window.title().ok(),
                use_wgc: true,
            },
            capture_width: width,
            capture_height: height,
            include_system_audio: false,
            enable_diagnostics: false,
        };
        let (stop_tx, mut stop_rx) = tokio::sync::mpsc::channel(1);
        let stop_thread = thread::spawn(move || {
            thread::sleep(Duration::from_secs(1));
            stop_tx.blocking_send(())
        });
        let (startup_tx, startup_rx) = tokio::sync::oneshot::channel();
        let mut startup_notifier = StartupNotifier::new(startup_tx);

        let outcome =
            run_encoder_session(None, &session_config, &mut stop_rx, &mut startup_notifier);

        stop_thread
            .join()
            .map_err(|error| format!("Window test stop thread panicked: {error:?}"))?
            .map_err(|error| format!("Failed to stop window test: {error}"))?;
        assert!(matches!(startup_rx.blocking_recv(), Ok(Ok(()))));
        assert!(matches!(outcome, RecordingRunOutcome::Finalized { .. }));
        assert!(output_path
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0));
        fs::remove_file(output_path)
            .map_err(|error| format!("Failed to remove window test output: {error}"))?;
        Ok(())
    }

    #[test]
    #[ignore = "requires Windows Media Foundation H.264/AAC support"]
    fn native_encoder_black_video() -> Result<(), String> {
        let _com_guard = WinRtMtaGuard::initialize()?;
        let output_path = temporary_mp4_path("native-encoder-black-video");
        let width = 640u32;
        let height = 360u32;
        let frame_rate = 30u32;
        let black_frame = create_black_frame(width, height)?;

        let mut encoder = VideoEncoder::new(
            VideoSettingsBuilder::new(width, height)
                .sub_type(VideoSettingsSubType::H264)
                .bitrate(2_000_000)
                .frame_rate(frame_rate),
            AudioSettingsBuilder::new(),
            ContainerSettingsBuilder::new().sub_type(ContainerSettingsSubType::MPEG4),
            &output_path,
        )
        .map_err(|error| format!("Failed to create native encoder: {error}"))?;

        let frame_duration = 10_000_000i64 / i64::from(frame_rate);
        let silence = vec![0u8; 960 * 2 * 2];
        for frame_index in 0..frame_rate {
            encoder
                .send_frame_buffer(&black_frame, i64::from(frame_index) * frame_duration)
                .map_err(|error| format!("Failed to submit black video frame: {error}"))?;
            encoder
                .send_audio_buffer(&silence, 0)
                .map_err(|error| format!("Failed to submit silence buffer: {error}"))?;
        }
        encoder
            .finish()
            .map_err(|error| format!("Failed to finalize native encoder: {error}"))?;

        let output_length = fs::metadata(&output_path)
            .map_err(|error| format!("Native encoder did not create output: {error}"))?
            .len();
        if output_length == 0 {
            return Err("Native encoder created an empty MP4".to_string());
        }
        fs::remove_file(&output_path)
            .map_err(|error| format!("Failed to remove native encoder test output: {error}"))?;
        Ok(())
    }

    struct MonitorSmokeCapture {
        first_frame_tx: Option<mpsc::Sender<(u32, u32)>>,
    }

    impl GraphicsCaptureApiHandler for MonitorSmokeCapture {
        type Flags = mpsc::Sender<(u32, u32)>;
        type Error = String;

        fn new(context: Context<Self::Flags>) -> Result<Self, Self::Error> {
            Ok(Self {
                first_frame_tx: Some(context.flags),
            })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> Result<(), Self::Error> {
            if let Some(first_frame_tx) = self.first_frame_tx.take() {
                first_frame_tx
                    .send((frame.width(), frame.height()))
                    .map_err(|error| format!("Failed to publish monitor frame: {error}"))?;
                capture_control.stop();
            }
            Ok(())
        }

        fn on_closed(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    #[ignore = "requires an interactive Windows desktop"]
    fn native_monitor_capture_smoke() -> Result<(), String> {
        let monitor = Monitor::primary()
            .map_err(|error| format!("Failed to resolve primary monitor: {error}"))?;
        let (first_frame_tx, first_frame_rx) = mpsc::channel();
        let settings = Settings::new(
            monitor,
            CursorCaptureSettings::WithCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Exclude,
            MinimumUpdateIntervalSettings::Custom(Duration::from_millis(33)),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            first_frame_tx,
        );
        let capture = MonitorSmokeCapture::start_free_threaded(settings)
            .map_err(|error| format!("Failed to start primary monitor capture: {error}"))?;
        let (width, height) = first_frame_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| format!("Primary monitor did not produce a frame: {error}"))?;
        capture
            .stop()
            .map_err(|error| format!("Failed to stop primary monitor capture: {error}"))?;
        if width == 0 || height == 0 {
            return Err(format!(
                "Primary monitor returned invalid dimensions {width}x{height}"
            ));
        }
        Ok(())
    }
}
