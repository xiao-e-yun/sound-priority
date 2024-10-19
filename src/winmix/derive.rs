use core::slice;
use std::{ffi::OsString, marker::PhantomData, os::windows::ffi::OsStringExt};

use windows::{
  core::Interface,
  Win32::{
    Devices::Properties::DEVPKEY_Device_FriendlyName,
    Foundation::{CloseHandle, MAX_PATH},
    Media::Audio::{
      Endpoints::IAudioEndpointVolume, IAudioSessionControl, IAudioSessionControl2,
      IAudioSessionEnumerator, IAudioSessionManager2, IMMDevice, ISimpleAudioVolume,
    },
    System::{
      Com::{StructuredStorage, CLSCTX_ALL, STGM_READ},
      ProcessStatus::GetModuleFileNameExW,
      Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
      Variant::VT_LPWSTR,
    },
  },
};
use windows_result::{Error, HRESULT};

use super::{
  session::Session,
  volume::{EndpointVolume, SessionVolume},
};

#[derive(Debug, Clone)]
pub struct Derive<'a> {
  device: IMMDevice,
  phantom: PhantomData<&'a ()>,
}

impl<'a> Derive<'a> {
  pub fn from_immdevice(device: IMMDevice) -> Result<Self, Error> {
    Ok(Derive {
      device,
      phantom: PhantomData::<&()>,
    })
  }

  pub fn sessions(&self) -> Result<Vec<Session<'a>>, Error> {
    unsafe {
      let manager: IAudioSessionManager2 = self.device.Activate(CLSCTX_ALL, None)?;
      let enumerator: IAudioSessionEnumerator = manager.GetSessionEnumerator().unwrap();

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
              SessionVolume::new(vol),
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

        sessions.push(Session::new(pid, path, SessionVolume::new(vol)));
      }

      Ok(sessions)
    }
  }

  pub fn master(&self) -> Result<EndpointVolume, Error> {
    unsafe {
      let endpoint: IAudioEndpointVolume = self.device.Activate(CLSCTX_ALL, None)?;
      Ok(EndpointVolume::new(endpoint.clone()))
    }
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

      // Create the utf16 Stringd convert it into a string.
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
}
