//! Windows per-process playback mute; HLAE's WAV recording remains game-side.
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

pub(super) struct GameAudioMute {
    sessions: Vec<(String, ISimpleAudioVolume, bool)>,
}

impl GameAudioMute {
    // Construct, poll, and drop on the same dedicated thread (COM apartment).
    pub(super) fn new() -> windows::core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok()? };
        Ok(Self { sessions: vec![] })
    }

    pub(super) fn poll(&mut self, pid: u32) -> windows::core::Result<usize> {
        unsafe {
            let devices: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let endpoints = devices.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            let mut added = 0;
            for i in 0..endpoints.GetCount()? {
                let manager: IAudioSessionManager2 =
                    endpoints.Item(i)?.Activate(CLSCTX_ALL, None)?;
                let sessions = manager.GetSessionEnumerator()?;
                for j in 0..sessions.GetCount()? {
                    let control: IAudioSessionControl2 = sessions.GetSession(j)?.cast()?;
                    if control.GetProcessId()? != pid {
                        continue;
                    }
                    let raw_id = control.GetSessionInstanceIdentifier()?;
                    let id = raw_id.to_string();
                    CoTaskMemFree(Some(raw_id.0.cast()));
                    let id = id?;
                    if self.sessions.iter().any(|(known, _, _)| known == &id) {
                        continue;
                    }
                    let volume: ISimpleAudioVolume = control.cast()?;
                    let was_muted = volume.GetMute()?.as_bool();
                    volume.SetMute(true, std::ptr::null())?;
                    self.sessions.push((id, volume, was_muted));
                    added += 1;
                }
            }
            Ok(added)
        }
    }

    pub(super) fn restore(&mut self) -> Vec<String> {
        let mut errors = vec![];
        for (_, volume, was_muted) in self.sessions.drain(..) {
            if let Err(e) = unsafe { volume.SetMute(was_muted, std::ptr::null()) } {
                errors.push(format!(
                    "warning: could not restore CS2 Windows mute state: {e}"
                ));
            }
        }
        errors
    }
}

impl Drop for GameAudioMute {
    fn drop(&mut self) {
        let _ = self.restore();
        unsafe { CoUninitialize() };
    }
}
