//! Legacy FFmpeg recording session runner.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::mpsc;

use super::super::backend::{RecordingBackendKind, RecordingFailurePhase, RecordingRunOutcome};
use super::super::ffmpeg::{resolve_ffmpeg_binary_path, select_video_encoder};
use super::super::model::{
    RecordingSessionConfig, RuntimeCaptureMode, SegmentConfig, SegmentTransition,
    WindowCaptureAvailability, WINDOW_CAPTURE_UNAVAILABLE_WARNING,
};
use super::super::segments::{
    build_segment_output_path, cleanup_segment_workspace, create_segment_workspace,
    finalize_segmented_recording,
};
use super::super::window_capture::{
    evaluate_window_capture_availability, resolve_window_capture_region,
    warning_message_for_window_capture,
};
use super::common::{runtime_capture_label, to_runtime_capture_mode, StartupNotifier};
use super::events::emit_recording_warning;
use super::segment_runner::run_ffmpeg_recording_segment;

pub(super) fn run_ffmpeg_recording_session(
    app_handle: &AppHandle,
    session_config: &RecordingSessionConfig,
    stop_rx: &mut mpsc::Receiver<()>,
    startup_notifier: &mut StartupNotifier,
) -> RecordingRunOutcome {
    let ffmpeg_binary_path = match resolve_ffmpeg_binary_path(app_handle) {
        Ok(path) => path,
        Err(message) => return failure(startup_notifier, RecordingFailurePhase::Startup, message),
    };
    let mut capture_input = session_config.capture_input.clone();
    let (video_encoder, encoder_preset) = select_video_encoder(
        &ffmpeg_binary_path,
        &session_config.video_quality,
        &session_config.video_encoder_preference,
    );
    let mut runtime_capture_mode = to_runtime_capture_mode(&capture_input);
    let capture_target = capture_input.target_label();

    if matches!(runtime_capture_mode, RuntimeCaptureMode::Window) {
        let initial_availability = evaluate_window_capture_availability(&capture_input);
        let mut startup_warning: Option<&str> = None;

        if initial_availability != WindowCaptureAvailability::Available {
            runtime_capture_mode = RuntimeCaptureMode::Black;
            startup_warning = warning_message_for_window_capture(initial_availability);
        } else if let Err(error) = resolve_window_capture_region(&capture_input) {
            tracing::warn!(
                backend = "ffmpeg",
                "Failed to resolve initial window capture region: {error}"
            );
            runtime_capture_mode = RuntimeCaptureMode::Black;
            startup_warning = Some(WINDOW_CAPTURE_UNAVAILABLE_WARNING);
        }

        if matches!(runtime_capture_mode, RuntimeCaptureMode::Black) {
            emit_recording_warning(
                app_handle,
                startup_warning.unwrap_or(WINDOW_CAPTURE_UNAVAILABLE_WARNING),
            );
        }
    }

    let segment_workspace = if matches!(
        capture_input,
        super::super::model::CaptureInput::Window { .. }
    ) {
        match create_segment_workspace(&session_config.output_path) {
            Ok(workspace) => Some(workspace),
            Err(message) => {
                return failure(startup_notifier, RecordingFailurePhase::Startup, message);
            }
        }
    } else {
        None
    };

    tracing::info!(
        backend = "ffmpeg",
        ffmpeg_path = %ffmpeg_binary_path.display(),
        video_quality = %session_config.video_quality,
        video_encoder_preference = %session_config.video_encoder_preference,
        frame_rate = session_config.frame_rate,
        bitrate = session_config.bitrate,
        capture_source = runtime_capture_label(runtime_capture_mode),
        capture_target = %capture_target,
        include_system_audio = session_config.include_system_audio,
        enable_diagnostics = session_config.enable_diagnostics,
        video_encoder,
        "Starting FFmpeg recording"
    );

    let mut segment_paths: Vec<PathBuf> = Vec::new();
    let mut segment_durations: Vec<Duration> = Vec::new();
    let mut segment_index: usize = 0;
    let mut consecutive_segment_failures = 0u32;
    let mut runtime_failed = false;

    loop {
        let segment_output_path = if let Some(workspace) = &segment_workspace {
            build_segment_output_path(workspace, segment_index)
        } else {
            PathBuf::from(&session_config.output_path)
        };

        let segment_config = SegmentConfig {
            ffmpeg_binary_path: &ffmpeg_binary_path,
            runtime_capture_mode,
            output_path: &segment_output_path,
            video_quality: &session_config.video_quality,
            requested_frame_rate: session_config.frame_rate,
            output_frame_rate: session_config.frame_rate,
            bitrate: session_config.bitrate,
            include_system_audio: session_config.include_system_audio,
            enable_diagnostics: session_config.enable_diagnostics,
            video_encoder: &video_encoder,
            encoder_preset: encoder_preset.as_deref(),
            capture_width: session_config.capture_width,
            capture_height: session_config.capture_height,
        };

        let run_result = run_ffmpeg_recording_segment(
            app_handle,
            &segment_config,
            &mut capture_input,
            stop_rx,
            startup_notifier,
        );

        if run_result.output_written {
            if run_result.force_killed {
                tracing::warn!(
                    backend = "ffmpeg",
                    segment_path = %segment_output_path.display(),
                    wall_clock_secs = run_result.wall_clock_duration.as_secs_f32(),
                    "FFmpeg was force-killed before clean finalization; segment discarded. \
                     Consider increasing FFMPEG_STOP_TIMEOUT if this happens on normal stops."
                );
            } else {
                segment_paths.push(segment_output_path);
                segment_durations.push(run_result.wall_clock_duration);
            }
        }

        if run_result.ffmpeg_succeeded {
            consecutive_segment_failures = 0;
        } else if matches!(run_result.transition, SegmentTransition::Switch(_)) {
            tracing::debug!(
                backend = "ffmpeg",
                runtime_capture_mode = runtime_capture_label(runtime_capture_mode),
                "Ignoring non-zero FFmpeg exit for expected capture transition"
            );
        } else {
            consecutive_segment_failures = consecutive_segment_failures.saturating_add(1);
        }

        if consecutive_segment_failures >= 3 {
            runtime_failed = true;
            tracing::error!(
                backend = "ffmpeg",
                runtime_capture_mode = runtime_capture_label(runtime_capture_mode),
                "Stopping recording after repeated FFmpeg segment failures"
            );
            break;
        }

        match run_result.transition {
            SegmentTransition::Stop => break,
            SegmentTransition::Switch(next_runtime_capture_mode) => {
                runtime_capture_mode = next_runtime_capture_mode;
                segment_index = segment_index.saturating_add(1);
            }
            SegmentTransition::RestartSameMode => {
                if matches!(runtime_capture_mode, RuntimeCaptureMode::Monitor) {
                    break;
                }
                segment_index = segment_index.saturating_add(1);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    let finalization_result = if let Some(workspace) = &segment_workspace {
        let result = finalize_segmented_recording(
            &ffmpeg_binary_path,
            workspace,
            &segment_paths,
            &segment_durations,
            &session_config.output_path,
        );
        cleanup_segment_workspace(workspace);
        result
    } else {
        let output_file = Path::new(&session_config.output_path);
        if output_file.exists()
            && output_file
                .metadata()
                .is_ok_and(|metadata| metadata.len() > 0)
        {
            Ok(())
        } else {
            Err("FFmpeg did not produce a non-empty recording".to_string())
        }
    };

    match finalization_result {
        Ok(()) => RecordingRunOutcome::Finalized {
            backend: RecordingBackendKind::Ffmpeg,
        },
        Err(message) if runtime_failed => {
            failure(startup_notifier, RecordingFailurePhase::Runtime, message)
        }
        Err(message) if startup_notifier.is_acknowledged() && segment_paths.is_empty() => {
            RecordingRunOutcome::StoppedWithoutOutput {
                backend: RecordingBackendKind::Ffmpeg,
                message: Some(message),
                startup_acknowledged: true,
            }
        }
        Err(message) => failure(
            startup_notifier,
            if startup_notifier.is_acknowledged() {
                RecordingFailurePhase::Finalization
            } else {
                RecordingFailurePhase::Startup
            },
            message,
        ),
    }
}

fn failure(
    startup_notifier: &StartupNotifier,
    phase: RecordingFailurePhase,
    message: String,
) -> RecordingRunOutcome {
    RecordingRunOutcome::Failed {
        backend: RecordingBackendKind::Ffmpeg,
        phase,
        message,
        startup_acknowledged: startup_notifier.is_acknowledged(),
    }
}
