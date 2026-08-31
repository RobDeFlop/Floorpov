//! Native Windows recording backend.

#[cfg(test)]
mod tests {
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
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    struct ComGuard;

    impl ComGuard {
        fn initialize_mta() -> Result<Self, String> {
            let result = unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
            if result < 0 {
                return Err(format!(
                    "Failed to initialize the native encoder thread as MTA: HRESULT {result:#x}"
                ));
            }
            Ok(Self)
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn temporary_mp4_path(test_name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        std::env::temp_dir().join(format!("floorpov-{test_name}-{suffix}.mp4"))
    }

    #[test]
    #[ignore = "requires Windows Media Foundation H.264/AAC support"]
    fn native_encoder_black_video() -> Result<(), String> {
        let _com_guard = ComGuard::initialize_mta()?;
        let output_path = temporary_mp4_path("native-encoder-black-video");
        let width = 640u32;
        let height = 360u32;
        let frame_rate = 30u32;
        let buffer_length = usize::try_from(width)
            .ok()
            .and_then(|value| value.checked_mul(height as usize))
            .and_then(|value| value.checked_mul(4))
            .ok_or_else(|| "Black frame dimensions overflowed".to_string())?;
        let mut black_frame = vec![0u8; buffer_length];
        for alpha in black_frame.iter_mut().skip(3).step_by(4) {
            *alpha = u8::MAX;
        }

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
