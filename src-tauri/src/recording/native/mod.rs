//! Native Windows recording backend.

mod timing;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tauri::AppHandle;
use tokio::sync::mpsc;
use windows_capture::encoder::{
    AudioSettingsBuilder, ContainerSettingsBuilder, ContainerSettingsSubType, VideoEncoder,
    VideoSettingsBuilder, VideoSettingsSubType,
};
use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use self::timing::{black_heartbeat_due, QpcClock, TimestampReservation};
use super::audio_pipeline::{
    run_audio_queue_to_consumer, run_system_audio_capture_to_queue_with_startup,
};
use super::backend::{RecordingBackendKind, RecordingFailurePhase, RecordingRunOutcome};
use super::model::{AudioPipelineStats, RecordingSessionConfig, SYSTEM_AUDIO_QUEUE_CAPACITY};
use super::segments::promote_output_file;
use super::session::StartupNotifier;

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
    timestamps: &mut TimestampReservation,
    frame: &[u8],
    timestamp: i64,
) -> Result<bool, String> {
    let Some(timestamp) = timestamps.reserve(timestamp) else {
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

pub(crate) fn run_recording_session(
    _app_handle: &AppHandle,
    session_config: &RecordingSessionConfig,
    stop_rx: &mut mpsc::Receiver<()>,
    startup_notifier: &mut StartupNotifier,
) -> RecordingRunOutcome {
    run_encoder_session(session_config, stop_rx, startup_notifier)
}

fn run_encoder_session(
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

    let mut timestamps = TimestampReservation::default();
    let initial_timestamp = match clock.now_100ns() {
        Ok(timestamp) => timestamp,
        Err(message) => {
            cleanup_startup_encoder(&encoder, &partial_path);
            return startup_failure(message);
        }
    };
    if let Err(message) =
        send_black_frame(&encoder, &mut timestamps, &black_frame, initial_timestamp)
    {
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

    startup_notifier.notify_success();
    tracing::info!(
        backend = "native",
        capture_source = "black",
        output_path = %session_config.output_path,
        width = session_config.capture_width,
        height = session_config.capture_height,
        frame_rate = session_config.frame_rate,
        bitrate = session_config.bitrate,
        include_system_audio = session_config.include_system_audio,
        "Native encoder started"
    );

    let mut last_black_timestamp = initial_timestamp;
    loop {
        if let Some(message) = audio_pipeline
            .as_ref()
            .and_then(NativeAudioPipeline::runtime_error)
        {
            let _ = stop_audio_pipeline(&mut audio_pipeline);
            return runtime_failure(message, &partial_path);
        }

        match stop_rx.try_recv() {
            Ok(()) | Err(mpsc::error::TryRecvError::Disconnected) => break,
            Err(mpsc::error::TryRecvError::Empty) => {}
        }

        match clock.now_100ns() {
            Ok(timestamp) if black_heartbeat_due(last_black_timestamp, timestamp) => {
                match send_black_frame(&encoder, &mut timestamps, &black_frame, timestamp) {
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
                let _ = stop_audio_pipeline(&mut audio_pipeline);
                return runtime_failure(message, &partial_path);
            }
        }
        thread::sleep(Duration::from_millis(25));
    }

    if let Err(message) = stop_audio_pipeline(&mut audio_pipeline) {
        return runtime_failure(message, &partial_path);
    }

    if let Ok(timestamp) = clock.now_100ns() {
        if let Err(message) = send_black_frame(&encoder, &mut timestamps, &black_frame, timestamp) {
            return runtime_failure(message, &partial_path);
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
            capture_input: super::super::model::CaptureInput::Monitor,
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

        let outcome = run_encoder_session(&session_config, &mut stop_rx, &mut startup_notifier);

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
