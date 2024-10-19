use device::Device;
use windows::Win32::{
  Media::Audio::{eMultimedia, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator},
  System::Com::{CoCreateInstance, CoInitialize, CoUninitialize, CLSCTX_ALL},
};
use windows_result::{Error, HRESULT};

// WinMix: Change Windows Volume Mixer via Rust
pub mod device;
pub mod session;
pub mod volume;

#[derive(Debug)]
pub struct WinMix {
  initialized: bool,
}

impl WinMix {
  pub fn get_default<'a>(&'a self) -> Result<Device<'a>, Error> {
    let device = self.get_default_immdevice()?;
    Ok(Device::new(&self, device))
  }
  pub fn get_default_immdevice<'a>(&'a self) -> Result<IMMDevice, Error> {
    unsafe {
      let enumerator = self.get_device_enumerator()?;
      enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)
    }
  }
  // Enumerate all audio sessions from all audio endpoints via WASAPI.
  // pub fn enumerate(&self) -> Result<Vec<Device>, Error> {
  //   let mut result = Vec::<Device>::new();

  //   unsafe {
  //     let res: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

  //     let collection: IMMDeviceCollection = res.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;

  //     let device_count = collection.GetCount()?;

  //     for device_id in 0..device_count {
  //       let device = collection.Item(device_id)?;
  //       result.push(Device::from_immdevice(device)?);
  //     }
  //   }
  //   Ok(result)
  // }
  pub fn get_device_enumerator(&self) -> Result<IMMDeviceEnumerator, Error> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
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
