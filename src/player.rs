use crate::playlist::prefers_fast_audio;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use rodio::source::Source;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use windows::Win32::Foundation::HWND;
use windows::Win32::Media::MediaFoundation::{
    IMFAsyncCallback, IMFAsyncCallback_Impl, IMFAsyncResult, IMFAttributes, IMFMediaEvent,
    IMFMediaSession, IMFMediaSource, IMFPresentationDescriptor, IMFRateControl, IMFSimpleAudioVolume, IMFStreamDescriptor, IMFTopology, IMFVideoDisplayControl,
    MEEndOfPresentation, MEError, MESessionClosed, MESessionEnded, MESessionPaused,
    MESessionStarted, MESessionTopologyStatus, MFCreateAttributes, MFCreateAudioRendererActivate,
    MFCreateMediaSession, MFCreateSourceResolver, MFCreateTopology, MFCreateTopologyNode,
    MFCreateVideoRendererActivate, MFGetService, MFASYNC_CALLBACK_QUEUE_MULTITHREADED,
    MFMediaType_Audio, MFMediaType_Video, MFSTARTUP_LITE, MF_EVENT_TOPOLOGY_STATUS,
    MF_LOW_LATENCY, MF_OBJECT_MEDIASOURCE, MF_PD_DURATION,
    MF_RESOLUTION_CONTENT_DOES_NOT_HAVE_TO_MATCH_EXTENSION_OR_MIME_TYPE, MF_RESOLUTION_MEDIASOURCE,
    MF_TOPOLOGY_OUTPUT_NODE, MF_TOPOLOGY_SOURCESTREAM_NODE, MF_TOPONODE_PRESENTATION_DESCRIPTOR,
    MF_TOPONODE_SOURCE, MF_TOPONODE_STREAM_DESCRIPTOR, MF_TOPOSTATUS_READY, MR_POLICY_VOLUME_SERVICE,
    MR_VIDEO_RENDER_SERVICE, MFVideoARMode_PreservePicture,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Variant::VT_I8;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use windows_core::{implement, Interface, GUID, PCWSTR};

pub const WM_TRACK_ENDED: u32 = WM_APP + 2;
pub const WM_MF_EVENT: u32 = WM_APP + 3;

pub enum PlayerNotice {
    None,
    Ended,
    Failed(String),
    ShowVideo(bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine {
    Idle,
    Audio,
    MediaFoundation,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MfPhase {
    Empty,
    Active,
    Closing,
}

struct AudioOutput {
    _stream: OutputStream,
    sink: Sink,
}

struct EndNotice<S> {
    inner: S,
    hwnd: isize,
    generation: u64,
}

impl<S> Drop for EndNotice<S> {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_TRACK_ENDED,
                windows::Win32::Foundation::WPARAM(self.generation as usize),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

impl<S: Iterator> Iterator for EndNotice<S> {
    type Item = S::Item;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl<S> Source for EndNotice<S>
where
    S: Source,
    S::Item: rodio::Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }

    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

struct SendSession(IMFMediaSession);

impl Clone for SendSession {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

// Media Foundation delivers session events on its own thread and requires
// EndGetEvent there. The session pointer is free-threaded for that call.
unsafe impl Send for SendSession {}

struct MfShared {
    session: Mutex<Option<SendSession>>,
    callback: Mutex<Option<IMFAsyncCallback>>,
}

#[implement(IMFAsyncCallback)]
struct SessionCallback {
    shared: Arc<MfShared>,
    hwnd: isize,
}

impl IMFAsyncCallback_Impl for SessionCallback_Impl {
    fn GetParameters(&self, flags: *mut u32, queue: *mut u32) -> windows_core::Result<()> {
        unsafe {
            if !flags.is_null() {
                *flags = 0;
            }
            if !queue.is_null() {
                *queue = MFASYNC_CALLBACK_QUEUE_MULTITHREADED;
            }
        }
        Ok(())
    }

    fn Invoke(&self, result: windows_core::Ref<'_, IMFAsyncResult>) -> windows_core::Result<()> {
        let session = self
            .shared
            .session
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let Some(session) = session else {
            return Ok(());
        };
        let event = unsafe { session.0.EndGetEvent(result.as_ref()) };
        let Ok(event) = event else {
            return Ok(());
        };
        if let Some(callback) = self
            .shared
            .callback
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
        {
            unsafe {
                let _ = session.0.BeginGetEvent(&callback, None);
            }
        }
        post_event(self.hwnd, &event);
        Ok(())
    }
}

fn post_event(hwnd: isize, event: &IMFMediaEvent) {
    let Ok(kind) = (unsafe { event.GetType() }) else {
        return;
    };
    let mut extra = 0isize;
    if kind == MESessionTopologyStatus.0 as u32 {
        extra = unsafe { event.GetUINT32(&MF_EVENT_TOPOLOGY_STATUS) }.unwrap_or(0) as isize;
    } else if kind == MEError.0 as u32 {
        extra = unsafe { event.GetStatus() }
            .map(|hr| hr.0 as isize)
            .unwrap_or(0);
    }
    unsafe {
        let _ = PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_MF_EVENT,
            windows::Win32::Foundation::WPARAM(kind as usize),
            windows::Win32::Foundation::LPARAM(extra),
        );
    }
}

pub struct Player {
    video_hwnd: HWND,
    hwnd: isize,
    audio: Option<AudioOutput>,
    audio_gen: u64,
    audio_duration: Option<Duration>,
    engine: Engine,
    paused: bool,
    finished: bool,
    current_path: Option<std::path::PathBuf>,
    current_url: Option<String>,
    speed: f32,
    boosting: bool,
    volume: f32,
    has_video: bool,
    video_width: i32,
    video_height: i32,
    mf: Arc<MfShared>,
    callback: Option<IMFAsyncCallback>,
    session: Option<IMFMediaSession>,
    source: Option<IMFMediaSource>,
    display: Option<IMFVideoDisplayControl>,
    mf_volume: Option<IMFSimpleAudioVolume>,
    phase: MfPhase,
    pending: Option<std::path::PathBuf>,
    pending_url: Option<String>,
    duration_hns: u64,
}

impl Player {
    pub fn new(hwnd: HWND, video_hwnd: HWND) -> Self {
        Self {
            video_hwnd,
            hwnd: hwnd.0 as isize,
            audio: None,
            audio_gen: 0,
            audio_duration: None,
            engine: Engine::Idle,
            paused: false,
            finished: false,
            current_path: None,
            current_url: None,
            speed: 1.0,
            boosting: false,
            volume: 1.0,
            has_video: false,
            video_width: 0,
            video_height: 0,
            mf: Arc::new(MfShared {
                session: Mutex::new(None),
                callback: Mutex::new(None),
            }),
            callback: None,
            session: None,
            source: None,
            display: None,
            mf_volume: None,
            phase: MfPhase::Empty,
            pending: None,
            pending_url: None,
            duration_hns: 0,
        }
    }

    pub fn attach_output(&mut self, stream: OutputStream, handle: OutputStreamHandle) {
        if let Ok(sink) = Sink::try_new(&handle) {
            sink.set_volume(self.volume);
            self.audio = Some(AudioOutput {
                _stream: stream,
                sink,
            });
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(audio) = &self.audio {
            audio.sink.set_volume(self.volume);
        }
        if let Some(volume_control) = &self.mf_volume {
            unsafe {
                let _ = volume_control.SetMasterVolume(self.volume);
            }
        }
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.25, 4.0);
        if !self.boosting {
            self.apply_rate();
        }
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn set_boost(&mut self, on: bool) {
        self.boosting = on;
        self.apply_rate();
    }

    pub fn wants_replay(&self) -> bool {
        self.finished || self.audio_finished()
    }

    pub fn replay(&mut self) -> Result<(), String> {
        if let Some(url) = self.current_url.clone() {
            return self.play_url(&url);
        }
        if let Some(path) = self.current_path.clone() {
            return self.play_file(&path);
        }
        Ok(())
    }

    pub fn stop_playback(&mut self) {
        self.finished = false;
        self.paused = true;
        self.current_path = None;
        self.current_url = None;
        self.pending = None;
        self.pending_url = None;
        self.stop_audio();
        self.request_close_mf();
        self.engine = Engine::Idle;
        self.has_video = false;
    }

    pub fn is_playing(&self) -> bool {
        if self.paused {
            return false;
        }
        match self.engine {
            Engine::Audio => self
                .audio
                .as_ref()
                .is_some_and(|audio| !audio.sink.is_paused() && !audio.sink.empty()),
            Engine::MediaFoundation => self.phase == MfPhase::Active,
            Engine::Idle => false,
        }
    }

    pub fn position(&self) -> Duration {
        match self.engine {
            Engine::Audio => self
                .audio
                .as_ref()
                .map(|audio| audio.sink.get_pos())
                .unwrap_or_default(),
            Engine::MediaFoundation => self.mf_position(),
            Engine::Idle => Duration::ZERO,
        }
    }

    pub fn duration(&self) -> Option<Duration> {
        match self.engine {
            Engine::Audio => self.audio_duration,
            Engine::MediaFoundation if self.duration_hns > 0 => {
                Some(Duration::from_nanos(self.duration_hns.saturating_mul(100)))
            }
            _ => None,
        }
    }

    pub fn play_file(&mut self, path: &Path) -> Result<(), String> {
        self.paused = false;
        self.finished = false;
        self.current_path = Some(path.to_path_buf());
        self.current_url = None;
        self.pending_url = None;
        if prefers_fast_audio(path) && self.play_audio(path).is_ok() {
            self.pending = None;
            self.has_video = false;
            self.engine = Engine::Audio;
            self.request_close_mf();
            return Ok(());
        }
        self.stop_audio();
        self.pending = Some(path.to_path_buf());
        self.pending_url = None;
        self.engine = Engine::MediaFoundation;
        self.has_video = crate::playlist::is_video(path);
        if self.phase == MfPhase::Closing {
            return Ok(());
        }
        if self.session.is_some() {
            self.begin_close();
            return Ok(());
        }
        let path = self.pending.take().unwrap();
        self.open_mf(&path)
    }

    pub fn toggle_pause(&mut self) {
        if self.is_playing() {
            self.pause();
        } else if self.wants_replay() {
            let _ = self.replay();
        } else {
            self.resume();
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
        if let Some(audio) = &self.audio {
            if self.engine == Engine::Audio {
                audio.sink.pause();
            }
        }
        if self.engine == Engine::MediaFoundation {
            if let Some(session) = &self.session {
                unsafe {
                    let _ = session.Pause();
                }
            }
        }
    }

    pub fn resume(&mut self) {
        self.paused = false;
        match self.engine {
            Engine::Audio => {
                if let Some(audio) = &self.audio {
                    audio.sink.play();
                }
            }
            Engine::MediaFoundation => self.start_at(None),
            Engine::Idle => {}
        }
    }

    pub fn seek_ratio(&mut self, ratio: f32) {
        let Some(duration) = self.duration() else {
            return;
        };
        let ratio = ratio.clamp(0.0, 1.0);
        let pos = duration.mul_f32(ratio);
        self.seek_to(pos);
    }

    pub fn seek_by(&mut self, delta_ms: i64) {
        let pos = self.position();
        let next = if delta_ms >= 0 {
            pos.saturating_add(Duration::from_millis(delta_ms as u64))
        } else {
            pos.saturating_sub(Duration::from_millis((-delta_ms) as u64))
        };
        let target = match self.duration() {
            Some(duration) => next.min(duration),
            None => next,
        };
        self.seek_to(target);
    }

    pub fn set_video_size(&mut self, width: i32, height: i32) {
        self.video_width = width.max(0);
        self.video_height = height.max(0);
        self.apply_video_size();
    }

    pub fn on_track_ended(&mut self, generation: u64) -> bool {
        if self.engine == Engine::Audio && generation == self.audio_gen && !self.paused {
            self.finished = true;
            true
        } else {
            false
        }
    }

    fn audio_finished(&self) -> bool {
        self.engine == Engine::Audio
            && !self.paused
            && self.audio.as_ref().is_some_and(|audio| audio.sink.empty())
    }

    pub fn on_mf_event(&mut self, kind: u32, extra: isize) -> PlayerNotice {
        if kind == MESessionClosed.0 as u32 {
            self.finish_close();
            if let Some(url) = self.pending_url.take() {
                self.engine = Engine::MediaFoundation;
                if let Err(error) = self.open_mf_url(&url) {
                    return PlayerNotice::Failed(error);
                }
            } else if let Some(path) = self.pending.take() {
                self.engine = Engine::MediaFoundation;
                if let Err(error) = self.open_mf(&path) {
                    return PlayerNotice::Failed(error);
                }
            }
            return PlayerNotice::ShowVideo(self.has_video && self.engine == Engine::MediaFoundation);
        }
        if self.phase == MfPhase::Closing {
            return PlayerNotice::None;
        }
        if kind == MESessionTopologyStatus.0 as u32 && extra as u32 == MF_TOPOSTATUS_READY.0 as u32
        {
            self.on_topology_ready();
            if !self.paused {
                self.start_at(None);
            }
            return PlayerNotice::ShowVideo(self.has_video);
        }
        if kind == MESessionStarted.0 as u32 {
            self.phase = MfPhase::Active;
            self.paused = false;
            self.apply_rate();
            return PlayerNotice::None;
        }
        if kind == MESessionPaused.0 as u32 {
            self.paused = true;
            return PlayerNotice::None;
        }
        if kind == MEEndOfPresentation.0 as u32 || kind == MESessionEnded.0 as u32 {
            if self.engine == Engine::MediaFoundation && !self.paused {
                self.paused = true;
                self.finished = true;
                return PlayerNotice::Ended;
            }
        }
        if kind == MEError.0 as u32 {
            let code = extra as u32;
            self.fail_open();
            return PlayerNotice::Failed(format!("Playback failed (0x{code:08X})"));
        }
        PlayerNotice::None
    }

    pub fn shutdown(&mut self) {
        self.pending = None;
        self.pending_url = None;
        self.stop_audio();
        if let Ok(mut callback) = self.mf.callback.lock() {
            *callback = None;
        }
        if let Some(session) = self.session.take() {
            unsafe {
                let _ = session.Close();
                let _ = session.Shutdown();
            }
        }
        if let Some(source) = self.source.take() {
            unsafe {
                let _ = source.Shutdown();
            }
        }
        if let Ok(mut session) = self.mf.session.lock() {
            *session = None;
        }
        self.callback = None;
        self.display = None;
        self.mf_volume = None;
        self.phase = MfPhase::Empty;
    }

    fn play_audio(&mut self, path: &Path) -> Result<(), String> {
        let audio = self.audio.as_ref().ok_or("No audio output device")?;
        self.audio_gen = self.audio_gen.wrapping_add(1);
        let generation = self.audio_gen;
        audio.sink.stop();
        let file = File::open(path).map_err(|error| error.to_string())?;
        let decoder = Decoder::new(BufReader::new(file)).map_err(|error| error.to_string())?;
        self.audio_duration = decoder.total_duration();
        audio.sink.append(EndNotice {
            inner: decoder,
            hwnd: self.hwnd,
            generation,
        });
        audio.sink.set_volume(self.volume);
        audio.sink.play();
        self.apply_rate();
        self.paused = false;
        Ok(())
    }

    fn stop_audio(&mut self) {
        self.audio_gen = self.audio_gen.wrapping_add(1);
        if let Some(audio) = &self.audio {
            audio.sink.stop();
        }
        if self.engine == Engine::Audio {
            self.engine = Engine::Idle;
        }
    }

    fn seek_to(&mut self, pos: Duration) {
        match self.engine {
            Engine::Audio => {
                if let Some(audio) = &self.audio {
                    let _ = audio.sink.try_seek(pos);
                }
            }
            Engine::MediaFoundation => {
                self.paused = false;
                self.start_at(Some(pos));
            }
            Engine::Idle => {}
        }
    }

    fn request_close_mf(&mut self) {
        if self.session.is_some() && self.phase != MfPhase::Closing {
            self.begin_close();
        }
    }

    fn begin_close(&mut self) {
        self.phase = MfPhase::Closing;
        if let Some(volume) = &self.mf_volume {
            unsafe {
                let _ = volume.SetMasterVolume(0.0);
            }
        }
        if let Some(session) = &self.session {
            unsafe {
                let _ = session.Stop();
                let _ = session.Close();
            }
        }
    }

    fn finish_close(&mut self) {
        if let Some(source) = self.source.take() {
            unsafe {
                let _ = source.Shutdown();
            }
        }
        if let Some(session) = self.session.take() {
            unsafe {
                let _ = session.Shutdown();
            }
        }
        self.publish_session(None);
        self.display = None;
        self.mf_volume = None;
        self.duration_hns = 0;
        self.phase = MfPhase::Empty;
        if self.engine == Engine::MediaFoundation {
            self.has_video = false;
        }
    }

    fn fail_open(&mut self) {
        self.pending = None;
        self.pending_url = None;
        if self.session.is_some() {
            self.begin_close();
        }
        self.engine = Engine::Idle;
        self.has_video = false;
    }

    pub fn play_url(&mut self, url: &str) -> Result<(), String> {
        self.paused = false;
        self.finished = false;
        self.current_path = None;
        self.current_url = Some(url.to_string());
        self.pending = None;
        self.stop_audio();
        self.pending_url = Some(url.to_string());
        self.engine = Engine::MediaFoundation;
        self.has_video = true;
        if self.phase == MfPhase::Closing {
            return Ok(());
        }
        if self.session.is_some() {
            self.begin_close();
            return Ok(());
        }
        let url = self.pending_url.take().unwrap();
        self.open_mf_url(&url)
    }

    fn open_mf(&mut self, path: &Path) -> Result<(), String> {
        self.open_location(&wide_path(path), crate::playlist::is_video(path))
    }

    fn open_mf_url(&mut self, url: &str) -> Result<(), String> {
        let wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        self.open_location(&wide, true)
    }

    fn open_location(&mut self, wide: &[u16], video_hint: bool) -> Result<(), String> {
        self.finish_close();
        self.engine = Engine::MediaFoundation;
        self.has_video = video_hint;
        let callback = self.ensure_callback()?;
        let mut attributes = None;
        unsafe { MFCreateAttributes(&mut attributes, 1) }.map_err(|error| error.to_string())?;
        let attributes = attributes.ok_or("Media Foundation failed to start")?;
        unsafe { attributes.SetUINT32(&MF_LOW_LATENCY, 1) }.ok();
        let session = unsafe { MFCreateMediaSession(&attributes) }.map_err(|error| error.to_string())?;
        unsafe { session.BeginGetEvent(&callback, None) }.map_err(|error| error.to_string())?;
        self.session = Some(session.clone());
        self.publish_session(Some(session.clone()));
        self.phase = MfPhase::Active;

        let source = match create_source(wide) {
            Ok(source) => source,
            Err(error) => {
                self.fail_open();
                return Err(error);
            }
        };
        if let Ok(low_latency) = source.cast::<IMFAttributes>() {
            unsafe {
                let _ = low_latency.SetUINT32(&MF_LOW_LATENCY, 1);
            }
        }
        let (topology, has_video, duration) = match build_topology(&source, self.video_hwnd) {
            Ok(built) => built,
            Err(error) => {
                unsafe {
                    let _ = source.Shutdown();
                }
                self.fail_open();
                return Err(error);
            }
        };
        self.has_video = has_video;
        self.duration_hns = duration;
        self.source = Some(source);
        if let Err(error) = unsafe { session.SetTopology(0, &topology) } {
            self.fail_open();
            return Err(error.to_string());
        }
        Ok(())
    }

    fn ensure_callback(&mut self) -> Result<IMFAsyncCallback, String> {
        if let Some(callback) = &self.callback {
            return Ok(callback.clone());
        }
        let callback: IMFAsyncCallback = SessionCallback {
            shared: self.mf.clone(),
            hwnd: self.hwnd,
        }
        .into();
        if let Ok(mut slot) = self.mf.callback.lock() {
            *slot = Some(callback.clone());
        }
        self.callback = Some(callback.clone());
        Ok(callback)
    }

    fn publish_session(&self, session: Option<IMFMediaSession>) {
        if let Ok(mut slot) = self.mf.session.lock() {
            *slot = session.map(SendSession);
        }
    }

    fn on_topology_ready(&mut self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        self.display = mf_service(&session, &MR_VIDEO_RENDER_SERVICE);
        self.mf_volume = mf_service(&session, &MR_POLICY_VOLUME_SERVICE);
        if let Some(display) = &self.display {
            unsafe {
                let _ = display.SetAspectRatioMode(MFVideoARMode_PreservePicture.0 as u32);
                let _ = display.SetVideoWindow(self.video_hwnd);
            }
        }
        if let Some(volume) = &self.mf_volume {
            unsafe {
                let _ = volume.SetMasterVolume(self.volume);
            }
        }
        self.apply_video_size();
        self.apply_rate();
        self.phase = MfPhase::Active;
    }

    fn apply_video_size(&self) {
        let Some(display) = &self.display else {
            return;
        };
        if self.video_width <= 0 || self.video_height <= 0 {
            return;
        }
        let dest = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: self.video_width,
            bottom: self.video_height,
        };
        unsafe {
            let _ = display.SetVideoPosition(std::ptr::null(), &dest);
            let _ = display.RepaintVideo();
        }
    }

    fn apply_rate(&self) {
        let rate = if self.boosting { 3.0 } else { self.speed };
        if let Some(audio) = &self.audio {
            audio.sink.set_speed(rate);
        }
        let Some(session) = &self.session else {
            return;
        };
        if let Ok(control) = session.cast::<IMFRateControl>() {
            unsafe {
                let _ = control.SetRate(false, rate);
            }
        }
    }

    fn start_at(&mut self, pos: Option<Duration>) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let var = match pos {
            Some(pos) => time_prop((pos.as_nanos() / 100) as i64),
            None => PROPVARIANT::default(),
        };
        let format = GUID::from_u128(0);
        unsafe {
            let _ = session.Start(&format, &var);
        }
    }

    fn mf_position(&self) -> Duration {
        let Some(session) = &self.session else {
            return Duration::ZERO;
        };
        let Ok(clock) = (unsafe { session.GetClock() }) else {
            return Duration::ZERO;
        };
        let mut time = 0i64;
        let mut system = 0i64;
        if unsafe { clock.GetCorrelatedTime(0, &mut time, &mut system) }.is_err() || time < 0 {
            return Duration::ZERO;
        }
        Duration::from_nanos((time as u64).saturating_mul(100))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn create_source(wide: &[u16]) -> Result<IMFMediaSource, String> {
    let resolver = unsafe { MFCreateSourceResolver() }.map_err(|error| error.to_string())?;
    let mut object_type = MF_OBJECT_MEDIASOURCE;
    let mut object = None;
    let flags = (MF_RESOLUTION_MEDIASOURCE.0
        | MF_RESOLUTION_CONTENT_DOES_NOT_HAVE_TO_MATCH_EXTENSION_OR_MIME_TYPE.0) as u32;
    unsafe {
        resolver.CreateObjectFromURL(
            PCWSTR(wide.as_ptr()),
            flags,
            None::<&windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore>,
            &mut object_type,
            &mut object,
        )
    }
    .map_err(|error| error.to_string())?;
    if object_type != MF_OBJECT_MEDIASOURCE {
        return Err("Windows could not open this file".into());
    }
    object
        .ok_or_else(|| "Windows could not open this file".to_string())?
        .cast::<IMFMediaSource>()
        .map_err(|error| error.to_string())
}

fn build_topology(
    source: &IMFMediaSource,
    video_hwnd: HWND,
) -> Result<(IMFTopology, bool, u64), String> {
    let pd = unsafe { source.CreatePresentationDescriptor() }.map_err(|error| error.to_string())?;
    let duration = unsafe { pd.GetUINT64(&MF_PD_DURATION) }.unwrap_or(0);
    let count = unsafe { pd.GetStreamDescriptorCount() }.map_err(|error| error.to_string())?;
    let topology = unsafe { MFCreateTopology() }.map_err(|error| error.to_string())?;
    let mut has_video = false;
    let mut connected = 0u32;
    for index in 0..count {
        let mut selected = windows_core::BOOL(0);
        let mut descriptor = None;
        unsafe {
            pd.GetStreamDescriptorByIndex(index, &mut selected, &mut descriptor)
        }
        .map_err(|error| error.to_string())?;
        if selected.0 == 0 {
            continue;
        }
        let Some(descriptor) = descriptor else {
            continue;
        };
        let handler = unsafe { descriptor.GetMediaTypeHandler() }.map_err(|error| error.to_string())?;
        let major = unsafe { handler.GetMajorType() }.map_err(|error| error.to_string())?;
        let video = major == MFMediaType_Video;
        let audio = major == MFMediaType_Audio;
        if !video && !audio {
            continue;
        }
        add_branch(&topology, source, &pd, &descriptor, video, video_hwnd)?;
        has_video |= video;
        connected += 1;
    }
    if connected == 0 {
        return Err("This file has no audio or video Windows can play".into());
    }
    Ok((topology, has_video, duration))
}

fn add_branch(
    topology: &IMFTopology,
    source: &IMFMediaSource,
    pd: &IMFPresentationDescriptor,
    descriptor: &IMFStreamDescriptor,
    video: bool,
    video_hwnd: HWND,
) -> Result<(), String> {
    let source_node = unsafe { MFCreateTopologyNode(MF_TOPOLOGY_SOURCESTREAM_NODE) }
        .map_err(|error| error.to_string())?;
    unsafe {
        source_node
            .SetUnknown(&MF_TOPONODE_SOURCE, source)
            .map_err(|error| error.to_string())?;
        source_node
            .SetUnknown(&MF_TOPONODE_PRESENTATION_DESCRIPTOR, pd)
            .map_err(|error| error.to_string())?;
        source_node
            .SetUnknown(&MF_TOPONODE_STREAM_DESCRIPTOR, descriptor)
            .map_err(|error| error.to_string())?;
    }
    let output = unsafe { MFCreateTopologyNode(MF_TOPOLOGY_OUTPUT_NODE) }
        .map_err(|error| error.to_string())?;
    let activate = if video {
        unsafe { MFCreateVideoRendererActivate(video_hwnd) }
    } else {
        unsafe { MFCreateAudioRendererActivate() }
    }
    .map_err(|error| error.to_string())?;
    unsafe {
        output
            .SetObject(&activate)
            .map_err(|error| error.to_string())?;
        topology
            .AddNode(&source_node)
            .map_err(|error| error.to_string())?;
        topology
            .AddNode(&output)
            .map_err(|error| error.to_string())?;
        source_node
            .ConnectOutput(0, &output, 0)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn mf_service<T: Interface>(object: &IMFMediaSession, service: &GUID) -> Option<T> {
    let mut raw = std::ptr::null_mut();
    unsafe {
        MFGetService(object, service, &T::IID, &mut raw).ok()?;
        windows_core::Type::<T>::from_abi(raw).ok()
    }
}

fn time_prop(hns: i64) -> PROPVARIANT {
    let mut value = PROPVARIANT::default();
    unsafe {
        let header = &mut *value.Anonymous.Anonymous;
        header.vt = VT_I8;
        header.Anonymous.hVal = hns;
    }
    value
}

fn wide_path(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn startup() -> Result<(), String> {
    unsafe {
        let hr = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
        if hr.is_err() {
            return Err(format!("COM init failed: 0x{:08X}", hr.0 as u32));
        }
        windows::Win32::Media::MediaFoundation::MFStartup(
            windows::Win32::Media::MediaFoundation::MF_VERSION,
            MFSTARTUP_LITE,
        )
        .map_err(|error| error.to_string())
    }
}

pub fn shutdown_platform() {
    unsafe {
        let _ = windows::Win32::Media::MediaFoundation::MFShutdown();
        windows::Win32::System::Com::CoUninitialize();
    }
}
