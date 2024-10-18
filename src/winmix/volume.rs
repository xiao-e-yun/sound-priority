use std::{marker::PhantomData, ptr};

use windows::{
  core::Interface,
  Win32::Media::Audio::{
    Endpoints::{IAudioEndpointVolume, IAudioMeterInformation},
    ISimpleAudioVolume,
  },
};
use windows_result::Error;

#[derive(Debug)]
pub struct EndpointVolume<'a> {
  audio_endpoint_volume: IAudioEndpointVolume,
  phantom: PhantomData<&'a ()>,
}

impl<'a> EndpointVolume<'a> {
  pub fn new(audio_endpoint_volume: IAudioEndpointVolume) -> Self {
    EndpointVolume {
      audio_endpoint_volume,
      phantom: PhantomData,
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

#[derive(Debug)]
pub struct SessionVolume<'a> {
  simple_audio_volume: ISimpleAudioVolume,
  audio_meter_information: IAudioMeterInformation,
  phantom: PhantomData<&'a ()>,
}

impl<'a> SessionVolume<'a> {
  pub fn new(simple_audio_volume: ISimpleAudioVolume) -> Self {
    let audio_meter_information = simple_audio_volume.cast().unwrap();
    SessionVolume {
      audio_meter_information,
      simple_audio_volume,
      phantom: PhantomData,
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
    unsafe { self.audio_meter_information.GetPeakValue() }
  }
}
