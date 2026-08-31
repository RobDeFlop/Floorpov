mod common;
mod events;
mod ffmpeg;
mod segment_runner;

use std::thread;

use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot};

use self::common::clear_recording_state;
pub(crate) use self::common::StartupNotifier;
use self::events::{emit_recording_finalized, emit_recording_stopped};
pub(crate) use self::events::{emit_recording_warning, emit_recording_warning_cleared};
use self::ffmpeg::run_ffmpeg_recording_session;
use super::backend::{
    initial_backend, requested_backend, should_fallback_to_ffmpeg, RecordingBackendKind,
    RecordingBackendRequest, RecordingRunOutcome,
};
use super::model::{RecordingSessionConfig, SharedRecordingState};

pub(crate) fn spawn_recording_task(
    app_handle: AppHandle,
    state: SharedRecordingState,
    session_config: RecordingSessionConfig,
    stop_rx: mpsc::Receiver<()>,
) -> oneshot::Receiver<Result<(), String>> {
    let (startup_tx, startup_rx) = oneshot::channel();
    thread::spawn(move || {
        run_recording_session(app_handle, state, session_config, stop_rx, startup_tx);
    });
    startup_rx
}

fn run_recording_session(
    app_handle: AppHandle,
    state: SharedRecordingState,
    session_config: RecordingSessionConfig,
    mut stop_rx: mpsc::Receiver<()>,
    startup_tx: oneshot::Sender<Result<(), String>>,
) {
    let request = requested_backend();
    let initial_backend = initial_backend(request, &session_config.video_encoder_preference);
    let mut startup_notifier = StartupNotifier::new(startup_tx);

    if matches!(request, RecordingBackendRequest::Native)
        && session_config.video_encoder_preference != "auto"
    {
        tracing::warn!(
            backend = "native",
            video_encoder_preference = %session_config.video_encoder_preference,
            "Forced native recording ignores the legacy FFmpeg encoder preference"
        );
    }

    tracing::info!(
        backend_request = ?request,
        backend = initial_backend.label(),
        output_path = %session_config.output_path,
        "Starting recording session"
    );

    let mut outcome = run_backend(
        initial_backend,
        &app_handle,
        &session_config,
        &mut stop_rx,
        &mut startup_notifier,
    );

    let stop_requested = match stop_rx.try_recv() {
        Ok(()) | Err(mpsc::error::TryRecvError::Disconnected) => true,
        Err(mpsc::error::TryRecvError::Empty) => false,
    };

    let mut did_fallback = false;
    if should_fallback_to_ffmpeg(request, &outcome, stop_requested) {
        did_fallback = true;
        tracing::warn!(
            backend = outcome.backend().label(),
            "Native recording startup failed; falling back to FFmpeg"
        );
        outcome = run_ffmpeg_recording_session(
            &app_handle,
            &session_config,
            &mut stop_rx,
            &mut startup_notifier,
        );
    }

    if !startup_notifier.is_acknowledged() {
        startup_notifier.notify_error(startup_error_message(&outcome, did_fallback));
    }

    match &outcome {
        RecordingRunOutcome::Finalized { backend } => {
            tracing::info!(
                backend = backend.label(),
                output_path = %session_config.output_path,
                "Recording finalized"
            );
            emit_recording_finalized(&app_handle, &session_config.output_path);
        }
        RecordingRunOutcome::Failed {
            backend,
            phase,
            message,
            ..
        } => {
            tracing::error!(
                backend = backend.label(),
                phase = ?phase,
                output_path = %session_config.output_path,
                "Recording failed: {message}"
            );
        }
        RecordingRunOutcome::StoppedWithoutOutput {
            backend, message, ..
        } => {
            tracing::warn!(
                backend = backend.label(),
                output_path = %session_config.output_path,
                message,
                "Recording stopped without finalized output"
            );
        }
    }

    emit_recording_warning_cleared(&app_handle);
    clear_recording_state(&state);
    emit_recording_stopped(&app_handle);
}

fn run_backend(
    backend: RecordingBackendKind,
    app_handle: &AppHandle,
    session_config: &RecordingSessionConfig,
    stop_rx: &mut mpsc::Receiver<()>,
    startup_notifier: &mut StartupNotifier,
) -> RecordingRunOutcome {
    match backend {
        RecordingBackendKind::Ffmpeg => {
            run_ffmpeg_recording_session(app_handle, session_config, stop_rx, startup_notifier)
        }
        RecordingBackendKind::NativeWindows => {
            #[cfg(target_os = "windows")]
            {
                super::native::run_recording_session(
                    app_handle,
                    session_config,
                    stop_rx,
                    startup_notifier,
                )
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = (app_handle, session_config, stop_rx, startup_notifier);
                RecordingRunOutcome::Failed {
                    backend: RecordingBackendKind::NativeWindows,
                    phase: super::backend::RecordingFailurePhase::Startup,
                    message: "The native recording backend is only available on Windows"
                        .to_string(),
                    startup_acknowledged: false,
                }
            }
        }
    }
}

fn startup_error_message(outcome: &RecordingRunOutcome, did_fallback: bool) -> String {
    match outcome {
        RecordingRunOutcome::Failed { message, .. } => {
            if did_fallback {
                format!("Native recording and legacy FFmpeg fallback failed: {message}")
            } else {
                message.clone()
            }
        }
        RecordingRunOutcome::StoppedWithoutOutput {
            message: Some(message),
            ..
        } => message.clone(),
        _ => "Recording stopped before startup completed".to_string(),
    }
}
