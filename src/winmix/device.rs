use core::slice;
use std::{
  ffi::OsString,
  os::windows::ffi::OsStringExt,
  sync::mpsc::{self, Receiver, SyncSender},
};

use windows::{
  core::Interface,
  Win32::{
    Devices::Properties::DEVPKEY_Device_FriendlyName,
    Foundation::{CloseHandle, MAX_PATH},
    Media::Audio::{
      EDataFlow, ERole, Endpoints::IAudioEndpointVolume, IAudioSessionControl,
      IAudioSessionControl2, IAudioSessionEnumerator, IAudioSessionManager2,
      IAudioSessionNotification, IAudioSessionNotification_Impl, IMMDevice, IMMNotificationClient,
      IMMNotificationClient_Impl, ISimpleAudioVolume, DEVICE_STATE,
    },
    System::{
      Com::{StructuredStorage, CLSCTX_ALL, STGM_READ},
      ProcessStatus::GetModuleFileNameExW,
      Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
      Variant::VT_LPWSTR,
    },
  },
};
use windows_core::{implement, PCWSTR};
use windows_result::{Error, HRESULT};

use super::{
  session::Session,
  volume::{EndpointVolume, SessionVolume},
  WinMix,
};

#[derive(Debug)]
pub struct Device<'a> {
  winmix: &'a WinMix,
  manager: IAudioSessionManager2,

  device: IMMDevice,
  device_receiver: Option<Receiver<()>>,
  device_vcallback: Option<IMMNotificationClient>,

  sessions: Option<Vec<Session<'a>>>,
  sessions_receiver: Option<Receiver<()>>,
  sessions_vcallback: Option<IAudioSessionNotification>,
}

impl<'a> Device<'a> {
  pub fn new(winmix: &'a WinMix, device: IMMDevice) -> Self {
    let manager: IAudioSessionManager2 = unsafe {
      device
        .Activate(CLSCTX_ALL, None)
        .expect("Failed to activate IAudioSessionManager2")
    };
    Device {
      winmix,
      manager,

      device,
      device_receiver: None,
      device_vcallback: None,

      sessions: None,
      sessions_receiver: None,
      sessions_vcallback: None,
    }
  }

  pub fn get_sessions(&self) -> Result<Vec<Session<'a>>, Error> {
    unsafe {
      let enumerator: IAudioSessionEnumerator = self.manager.GetSessionEnumerator()?;
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

  pub fn current_sessions(&self) -> Vec<Session<'a>> {
    match &self.sessions {
      Some(sessions) => sessions.clone(),
      None => vec![],
    }
  }

  pub fn sync(&mut self, force: bool) -> Result<(), Error> {
    let device_synced = self
      .device_receiver
      .as_ref()
      .and_then(|receiver| receiver.try_recv().ok())
      .is_none();

    let mut sessions_synced = self
      .sessions_receiver
      .as_ref()
      .and_then(|receiver| receiver.try_recv().ok())
      .is_none();

    if !device_synced || force {
      log::info!("syncing device");
      let is_registered_sessions = self.sessions_receiver.is_some();
      if is_registered_sessions {
        self.unregister_sessions()?; // unregister old sessions
      }

      self.device = self.winmix.get_default_immdevice()?;
      self.manager = unsafe {
        self
          .device
          .Activate(CLSCTX_ALL, None)
          .expect("Failed to activate IAudioSessionManager2")
      };

      if is_registered_sessions {
        self.register_sessions()?; // register new sessions
        sessions_synced = false;
      }
    }

    if !sessions_synced || force {
      log::info!("syncing sessions");
      self.sessions = Some(self.get_sessions()?);
    }

    Ok(())
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

  pub fn register(&mut self) -> Result<(), Error> {
    self.register_device()?;
    self.register_sessions()?;
    Ok(())
  }
  pub fn unregister(&mut self) -> Result<(), Error> {
    self.unregister_device()?;
    self.unregister_sessions()?;
    Ok(())
  }

  pub fn register_sessions(&mut self) -> Result<(), Error> {
    if self.sessions_vcallback.is_none() {
      let (sender, receiver) = mpsc::sync_channel(1);
      let client = SessionsClient(sender);
      unsafe {
        let vcallback: IAudioSessionNotification = client.into();
        self.manager.RegisterSessionNotification(&vcallback)?;
        self.sessions_vcallback = Some(vcallback);
        self.sessions_receiver = Some(receiver);
        self.sessions = Some(self.get_sessions()?);
      }
    }

    Ok(())
  }
  pub fn unregister_sessions(&mut self) -> Result<(), Error> {
    if let Some(vcallback) = self.sessions_vcallback.take() {
      unsafe {
        self
          .manager
          .UnregisterSessionNotification(&vcallback)
          .unwrap();
        self.sessions_vcallback = Some(vcallback);
        self.sessions_receiver = None;
      }
    }

    Ok(())
  }

  pub fn register_device(&mut self) -> Result<(), Error> {
    if self.device_vcallback.is_none() {
      let device_enumerator = self.winmix.get_device_enumerator()?;
      let (sender, receiver) = mpsc::sync_channel(1);
      let client = DeviceClient(sender);
      unsafe {
        let vcallback: IMMNotificationClient = client.into();
        device_enumerator.RegisterEndpointNotificationCallback(&vcallback)?;
        self.device_vcallback = Some(vcallback);
      }
      self.device_receiver = Some(receiver);
    }
    Ok(())
  }
  pub fn unregister_device(&mut self) -> Result<(), Error> {
    if let Some(vcallback) = self.device_vcallback.take() {
      let device_enumerator = self.winmix.get_device_enumerator()?;
      unsafe {
        device_enumerator.UnregisterEndpointNotificationCallback(&vcallback)?;
      }
    }
    Ok(())
  }
}

#[allow(non_camel_case_types)]
#[implement(IAudioSessionNotification)]
pub struct SessionsClient(SyncSender<()>);

impl IAudioSessionNotification_Impl for SessionsClient {
  fn OnSessionCreated(&self, _: Option<&IAudioSessionControl>) -> windows_core::Result<()> {
    let _ = self.0.try_send(());
    Ok(())
  }
}

#[allow(non_camel_case_types)]
#[implement(IMMNotificationClient)]
pub struct DeviceClient(SyncSender<()>);

impl IMMNotificationClient_Impl for DeviceClient {
  fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()>
  where
    Self: Sized,
  {
    Ok(())
  }

  fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()>
  where
    Self: Sized,
  {
    Ok(())
  }

  fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()>
  where
    Self: Sized,
  {
    Ok(())
  }

  fn OnDefaultDeviceChanged(&self, _: EDataFlow, _: ERole, _: &PCWSTR) -> windows::core::Result<()>
  where
    Self: Sized,
  {
    let _ = self.0.try_send(());
    Ok(())
  }

  fn OnPropertyValueChanged(
    &self,
    _: &PCWSTR,
    _: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY,
  ) -> windows::core::Result<()>
  where
    Self: Sized,
  {
    Ok(())
  }
}
