use std::rc::Rc;
use std::sync::Mutex;

use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{EDataFlow, ERole};
use windows::Win32::Media::Audio::{
  IMMNotificationClient, IMMNotificationClient_Impl, DEVICE_STATE,
};
use windows_core::{implement, Error};

use super::derive::Derive;
use super::session::Session;
use super::WinMix;

#[derive(Debug)]
pub struct DefaultDerive<'a> {
  winmix: &'a WinMix,
  derive: Derive<'a>,
  sessions: Vec<Session<'a>>,
  derive_change: Rc<Mutex<bool>>,
  ssession_change: Rc<Mutex<bool>>,
  /// You need to keep a reference to the client to prevent it from being dropped.
  /// https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-immdeviceenumerator-unregisterendpointnotificationcallback
  client: Option<IMMNotificationClient>,
}

impl<'a> DefaultDerive<'a> {
  pub fn from_winmix(winmix: &'a WinMix) -> Result<Self, Error> {
    let derive = winmix.get_current_default()?;
    let sessions = derive.sessions()?;
    Ok(DefaultDerive {
      winmix,
      derive,
      sessions,
      client: None,
      derive_change: Rc::new(Mutex::new(false)),
      ssession_change: Rc::new(Mutex::new(false)),
    })
  }
  pub fn sync(&mut self, force: bool) -> Result<(), Error> {
    let derive_changed = force || self.derive_change.lock().map_or(false, |value| *value);
    let mut ssession_changed = force || self.ssession_change.lock().map_or(false, |value| *value);

    if derive_changed {
      match self.winmix.get_current_default() {
        Ok(derive) => {
          self.derive = derive;
          ssession_changed |= true;
        }
        Err(_) => log::warn!("Failed to get default device"),
      };

      let value = self.derive_change.lock();
      if let Ok(mut value) = value {
        *value = false;
      }
    }

    if ssession_changed {
      match self.derive.sessions() {
        Ok(sessions) => {
          self.sessions = sessions;
        }
        Err(_) => log::warn!("Failed to get sessions"),
      };

      let value = self.ssession_change.lock();
      if let Ok(mut value) = value {
        *value = false;
      }
    }

    Ok(())
  }

  pub fn register(&mut self) -> Result<(), Error> {
    let client = self.get_notification_client();
    let derive_enumerator = self.winmix.get_derive_enumerator()?;

    if !self.client.is_none() {
      unsafe {
        let vcallback: IMMNotificationClient = client.cast().unwrap();
        derive_enumerator.RegisterEndpointNotificationCallback(&vcallback)?;
        self.client = Some(vcallback);
      }
    }

    Ok(())
  }

  pub fn unregister(&mut self) -> Result<(), Error> {
    let client = self.get_notification_client();
    let derive_enumerator = self.winmix.get_derive_enumerator()?;

    if self.client.is_some() {
      unsafe {
        let vcallback: IMMNotificationClient = client.cast().unwrap();
        derive_enumerator.UnregisterEndpointNotificationCallback(&vcallback)?;
        self.client = None;
      }
    }

    Ok(())
  }

  pub fn get_notification_client(&self) -> WinMixImmNotificationClient {
    WinMixImmNotificationClient {
      derive_change: self.derive_change.clone(),
      session_change: self.ssession_change.clone(),
    }
  }

  pub fn sync_derive(&mut self) -> &Derive<'a> {
    let _ = self.sync(false);
    &self.derive
  }

  pub fn derive(&self) -> &Derive<'a> {
    &self.derive
  }

  pub fn sync_sessions(&mut self) -> &Vec<Session<'a>> {
    let _ = self.sync(false);
    &self.sessions
  }

  pub fn sessions(&self) -> &Vec<Session<'a>> {
    &self.sessions
  }
}

impl Drop for DefaultDerive<'_> {
  fn drop(&mut self) {
    let _ = self.unregister();
  }
}

#[allow(non_camel_case_types)]
#[implement(IMMNotificationClient)]
pub struct WinMixImmNotificationClient {
  derive_change: Rc<Mutex<bool>>,
  session_change: Rc<Mutex<bool>>,
}

#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for WinMixImmNotificationClient {
  fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()>
  where
    Self: Sized,
  {
    let value = self.session_change.lock();
    if let Ok(mut value) = value {
      *value = true;
    }
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
    let value = self.derive_change.lock();
    if let Ok(mut value) = value {
      *value = true;
    }
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
