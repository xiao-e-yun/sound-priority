use default::DefaultDerive;
use derive::Derive;
use windows::Win32::{
  Media::Audio::{eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator},
  System::Com::{CoCreateInstance, CoInitialize, CoUninitialize, CLSCTX_ALL},
};
use windows_result::{Error, HRESULT};

// WinMix: Change Windows Volume Mixer via Rust
pub mod derive;
pub mod session;
pub mod volume;
pub mod default;

#[derive(Debug)]
pub struct WinMix {
  initialized: bool,
}

impl WinMix {
  pub fn get_default<'a>(&'a self) -> Result<DefaultDerive<'a>, Error> {
    DefaultDerive::from_winmix(&self)
  }
  pub fn get_current_default<'a>(&'a self) -> Result<Derive<'a>, Error> {
    unsafe {
      let res: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
      let device = res.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
      Derive::from_immdevice(device)
    }
  }
  // Enumerate all audio sessions from all audio endpoints via WASAPI.
  // pub fn enumerate(&self) -> Result<Vec<Derive>, Error> {
  //   let mut result = Vec::<Derive>::new();

  //   unsafe {
  //     let res: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

  //     let collection: IMMDeviceCollection = res.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;

  //     let device_count = collection.GetCount()?;

  //     for device_id in 0..device_count {
  //       let device = collection.Item(device_id)?;
  //       result.push(Derive::from_immdevice(device)?);
  //     }
  //   }
  //   Ok(result)
  // }
  pub fn get_derive_enumerator(&self) -> Result<IMMDeviceEnumerator,Error> {
    unsafe {
      CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
    }
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
        initialized: hres.is_ok(),
      }
    }
  }
}

impl Drop for WinMix {
  fn drop(&mut self) {
    unsafe {
      if self.initialized {
        // We initialized COM, so we uninitialize it
        CoUninitialize();
      }
    }
  }
}
