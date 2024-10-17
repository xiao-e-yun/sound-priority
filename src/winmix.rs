//! WinMix: Change Windows Volume Mixer via Rust
//!
//! This is a rust library that allows you to individually change the volume of each program in the Windows Volume Mixer.
//!
//! For example, you can set the volume of `chrome.exe` to `0` while leaving other apps alone.
//!
//! ⚠ This libary uses **unsafe** functions from the [windows](https://crates.io/crates/windows) crate. ⚠
//!
//! # Usage
//!
//! ```no_run
//! use winmix::WinMix;
//!
//! unsafe {
//!     let winmix = WinMix::default();
//!
//!     // Get a list of all programs that have an entry in the volume mixer
//!     let sessions = winmix.enumerate()?;
//!
//!     for session in sessions {
//!         // PID and path of the process
//!         println!("pid: {}   path: {}", session.pid, session.path);
//!
//!         // Mute
//!         session.vol.set_mute(true)?;
//!         session.vol.set_mute(false)?;
//!
//!         // 50% volume
//!         session.vol.set_master_volume(0.5)?;
//!         // Back to 100% volume
//!         session.vol.set_master_volume(1.0)?;
//!
//!         // Get the current volume, or see if it's muted
//!         let vol = session.vol.get_master_volume()?;
//!         let is_muted = session.vol.get_mute()?;
//!
//!         println!("Vol: {}   Muted: {}", vol, is_muted);
//!         println!();
//!     }
//! }
//! ```
//!
use core::slice;
use serde::{Deserialize, Serialize};
use std::{
  ffi::OsString, fmt::Debug, hash::Hash, os::windows::ffi::OsStringExt, path::PathBuf, ptr,
};
use windows::{
  core::Interface,
  Win32::{
    Devices::Properties::DEVPKEY_Device_FriendlyName,
    Foundation::{CloseHandle, MAX_PATH},
    Media::Audio::{
      eMultimedia, eRender,
      Endpoints::{IAudioEndpointVolume, IAudioMeterInformation},
      IAudioSessionControl, IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2,
      IMMDevice, IMMDeviceCollection, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
      DEVICE_STATE_ACTIVE,
    },
    System::{
      Com::{
        CoCreateInstance, CoInitialize, CoUninitialize, StructuredStorage, CLSCTX_ALL, STGM_READ,
      },
      ProcessStatus::GetModuleFileNameExW,
      Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
      Variant::VT_LPWSTR,
    },
  },
};
use windows_result::{Error, HRESULT};

#[derive(Clone)]
pub struct WinMix {
  // Whether or not we initialized COM; if so, we have to clean up later
  com_initialized: bool,
}

impl WinMix {
  /// Enumerate all audio sessions from all audio endpoints via WASAPI.
  ///
  /// # Safety
  /// This function calls other unsafe functions from the [windows](https://crates.io/crates/windows) crate.
  pub fn enumerate(&self) -> Result<Vec<Derive>, Error> {
    let mut result = Vec::<Derive>::new();

    unsafe {
      let res: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

      let collection: IMMDeviceCollection = res.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;

      let device_count = collection.GetCount()?;

      for device_id in 0..device_count {
        let device = collection.Item(device_id)?;
        result.push(Derive::from_immdevice(device)?);
      }
    }

    Ok(result)
  }

  pub fn get_default(&self) -> Result<Derive, Error> {
    unsafe {
      let res: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
      let device = res.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
      Derive::from_immdevice(device)
    }
  }
}

impl Debug for WinMix {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WinMix").finish()
  }
}

impl Default for WinMix {
  /// Create a default instance of WinMix.
  fn default() -> WinMix {
    unsafe {
      let hres: HRESULT = CoInitialize(None);

      // If we initialized COM, we are responsible for cleaning it up later.
      // If it was already initialized, we don't have to do anything.
      WinMix {
        com_initialized: hres.is_ok(),
      }
    }
  }
}

impl Drop for WinMix {
  fn drop(&mut self) {
    unsafe {
      if self.com_initialized {
        // We initialized COM, so we uninitialize it
        CoUninitialize();
      }
    }
  }
}

#[derive(Debug)]
pub struct Derive {
  device: IMMDevice,
  pub master: EndpointVolume,
  pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeriveView {
  pub name: String,
  pub volume: f32,
  pub muted: bool,
  pub sessions: Vec<SessionView>,
}

impl Derive {
  pub unsafe fn from_immdevice(device: IMMDevice) -> Result<Self, Error> {
    let endpoint: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
    let master = EndpointVolume::new(endpoint);

    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
    let enumerator: IAudioSessionEnumerator = manager.GetSessionEnumerator()?;

    let session_count = enumerator.GetCount()?;

    let mut has_system = false;
    let mut sessions = Vec::<Session>::new();
    for session_id in 0..session_count {
      let ctrl: IAudioSessionControl = enumerator.GetSession(session_id)?;
      let ctrl2: IAudioSessionControl2 = ctrl.cast()?;

      let pid = ctrl2.GetProcessId()?;
      let vol: ISimpleAudioVolume = ctrl2.cast()?;

      if pid == 0 {
        if !has_system {
          sessions.push(Session::new(
            pid,
            "$system".to_string(),
            SimpleVolume::new(vol),
          ));
          has_system = true;
        };
        continue;
      }

      let Ok(proc) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) else {
        continue;
      };

      let mut path: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];

      let _ = GetModuleFileNameExW(proc, None, &mut path);

      CloseHandle(proc)?;

      // Trim trailing \0
      let mut path = String::from_utf16_lossy(&path);
      path.truncate(path.trim_matches(char::from(0)).len());

      sessions.push(Session::new(pid, path, SimpleVolume::new(vol)));
    }

    Ok(Derive {
      device,
      master,
      sessions,
    })
  }

  pub fn get_name(&self) -> Result<String, Error> {
    unsafe {
      let property_store = self.device.OpenPropertyStore(STGM_READ)?;

      // https://github.com/RustAudio/cpal/blob/master/src/host/wasapi/device.rs#L274
      let mut property_value =
        property_store.GetValue(&DEVPKEY_Device_FriendlyName as *const _ as *const _)?;

      let prop_variant = &property_value.as_raw().Anonymous.Anonymous;

      // Read the friendly-name from the union data field, expecting a *const u16.
      if prop_variant.vt != VT_LPWSTR.0 {
        return Err(Error::new(
          HRESULT::from_win32(0x80070005),
          "Property value is not a VT_LPWSTR",
        ));
      }
      let ptr_utf16 = *(&prop_variant.Anonymous as *const _ as *const *const u16);

      // Find the length of the friendly name.
      let mut len = 0;
      while *ptr_utf16.offset(len) != 0 {
        len += 1;
      }

      // Create the utf16 slice and convert it into a string.
      let name_slice = slice::from_raw_parts(ptr_utf16, len as usize);
      let name_os_string: OsString = OsStringExt::from_wide(name_slice);
      let name_string = match name_os_string.into_string() {
        Ok(string) => string,
        Err(os_string) => os_string.to_string_lossy().into(),
      };

      // Clean up the property.
      StructuredStorage::PropVariantClear(&mut property_value).ok();

      Ok(name_string)
    }
  }

  pub fn view(&self) -> Result<DeriveView, Error> {
    let name = self.get_name()?;
    let volume = self.master.get_volume()?;
    let muted = self.master.get_mute()?;
    let sessions = self
      .sessions
      .iter()
      .map(|s| s.view())
      .collect::<Result<Vec<SessionView>, Error>>()?;
    Ok(DeriveView {
      name,
      volume,
      muted,
      sessions,
    })
  }
}

#[derive(Debug)]
pub struct Session {
  /// The PID of the process that controls this audio session.
  pub pid: u32,
  /// The exe path for the process that controls this audio session.
  pub path: String,
  /// The name of the process that controls this audio session.
  pub name: String,
  /// A wrapper that lets you control the volume for this audio session.
  pub vol: SimpleVolume,
}

impl Hash for Session {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.pid.hash(state);
  }
}

impl PartialEq for Session {
  fn eq(&self, other: &Self) -> bool {
    self.pid == other.pid
  }
}

impl Eq for Session {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionView {
  pub name: String,
  pub volume: f32,
  pub muted: bool,
  pub peak: f32,
}

impl Session {
  pub fn new(pid: u32, path: String, vol: SimpleVolume) -> Self {
    // path to name without extension
    let name = PathBuf::from(&path)
      .file_stem()
      .expect("failed to get file stem")
      .to_string_lossy()
      .to_string();
    Session {
      pid,
      name,
      path,
      vol,
    }
  }
  pub fn view(&self) -> Result<SessionView, Error> {
    let volume = self.vol.get_volume()?;
    let muted = self.vol.get_mute()?;
    let peak = self.vol.get_peak()?;
    Ok(SessionView {
      name: self.name.clone(),
      volume,
      muted,
      peak,
    })
  }
}

#[derive(Debug)]
pub struct SimpleVolume {
  simple_audio_volume: ISimpleAudioVolume,
  audio_meter_information: Option<IAudioMeterInformation>,
}

impl SimpleVolume {
  pub fn new(simple_audio_volume: ISimpleAudioVolume) -> Self {
    let audio_meter_information = Some(simple_audio_volume.cast().unwrap());
    SimpleVolume {
      audio_meter_information,
      simple_audio_volume,
    }
  }

  /// Get the master volume for this session.
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.GetMasterVolume](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-getmastervolume) which is unsafe.
  pub fn get_volume(&self) -> Result<f32, Error> {
    unsafe { self.simple_audio_volume.GetMasterVolume() }
  }

  /// Set the master volume for this session.
  ///
  /// * `level` - the volume level, between `0.0` and `1.0`\
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.SetMasterVolume](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-setmastervolume) which is unsafe.
  pub fn set_volume(&self, level: f32) -> Result<(), Error> {
    unsafe { self.simple_audio_volume.SetMasterVolume(level, ptr::null()) }
  }

  /// Check if this session is muted.
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.GetMute](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-getmute) which is unsafe.
  pub fn get_mute(&self) -> Result<bool, Error> {
    unsafe {
      match self.simple_audio_volume.GetMute() {
        Ok(val) => Ok(val.as_bool()),
        Err(e) => Err(e),
      }
    }
  }

  /// Mute or unmute this session.
  ///
  /// * `val` - `true` to mute, `false` to unmute
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.SetMute](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-setmute) which is unsafe.
  pub fn set_mute(&self, val: bool) -> Result<(), Error> {
    unsafe { self.simple_audio_volume.SetMute(val, ptr::null()) }
  }

  pub fn get_peak(&self) -> Result<f32, Error> {
    unsafe {
      if let Some(audio_meter_information) = &self.audio_meter_information {
        audio_meter_information.GetPeakValue()
      } else {
        Err(Error::new(
          HRESULT::from_win32(0x80070005),
          "No audio meter information",
        ))
      }
    }
  }
}

#[derive(Debug)]
pub struct EndpointVolume {
  audio_endpoint_volume: IAudioEndpointVolume,
}

impl EndpointVolume {
  pub fn new(audio_endpoint_volume: IAudioEndpointVolume) -> Self {
    EndpointVolume {
      audio_endpoint_volume,
    }
  }

  /// Get the master volume for this session.
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.GetMasterVolume](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-getmastervolume) which is unsafe.
  pub fn get_volume(&self) -> Result<f32, Error> {
    unsafe { self.audio_endpoint_volume.GetMasterVolumeLevelScalar() }
  }

  /// Set the master volume for this session.
  ///
  /// * `level` - the volume level, between `0.0` and `1.0`\
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.SetMasterVolume](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-setmastervolume) which is unsafe.
  pub fn set_volume(&self, level: f32) -> Result<(), Error> {
    unsafe {
      self
        .audio_endpoint_volume
        .SetMasterVolumeLevelScalar(level, ptr::null())
    }
  }

  /// Check if this session is muted.
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.GetMute](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-getmute) which is unsafe.
  pub fn get_mute(&self) -> Result<bool, Error> {
    unsafe {
      self
        .audio_endpoint_volume
        .GetMute()
        .and_then(|val| Ok(val.as_bool()))
    }
  }

  /// Mute or unmute this session.
  ///
  /// * `val` - `true` to mute, `false` to unmute
  ///
  /// # Safety
  /// This function calls [ISimpleAudioVolume.SetMute](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-isimpleaudiovolume-setmute) which is unsafe.
  pub fn set_mute(&self, val: bool) -> Result<(), Error> {
    unsafe { self.audio_endpoint_volume.SetMute(val, ptr::null()) }
  }
}
