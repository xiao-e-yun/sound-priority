use std::{
  collections::HashSet,
  sync::mpsc::{channel, Receiver, Sender, TryRecvError},
  thread,
  time::Duration,
};

use crate::{config::Config, winmix::WinMix};

const TICK: Duration = Duration::from_millis(100);
const TRANSFORM_SPEED: f32 = 0.05;

const REDUCE_TIMEOUT: Duration = Duration::from_millis(200);
const RESOTRE_TIMEOUT: Duration = Duration::from_secs(3);

const FORCE_RELOAD_TICKS: usize = 600;

pub struct Deamon {
  sender: Sender<DaemonCommand>,
}

impl Deamon {
  pub fn create(config: Config) -> Self {
    let (sender, receiver) = channel();
    create_daemon(receiver, config.clone());
    Self { sender }
  }
  pub fn start(&mut self) {
    let _ = self.sender.send(DaemonCommand::Resume);
  }
  pub fn stop(&self) {
    let _ = self.sender.send(DaemonCommand::Suspend);
  }
  pub fn update(&mut self, config: &Config) {
    let _ = self.sender.send(DaemonCommand::Update(config.clone()));
  }
}

pub enum DaemonCommand {
  Resume,
  Suspend,
  Update(Config),
}

fn create_daemon(receiver: Receiver<DaemonCommand>, mut config: Config) {
  thread::spawn(move || {
    let winmix = WinMix::default();
    let mut transform = true;
    let mut ticks = 1_usize;
    let mut volume_status = VolumeStatus::Restore;
    let mut expect_volume = config.resotre_volume;
    let mut timeout = Duration::ZERO;

    let mut device = winmix.get_default().expect("failed to get default device");
    if device.register().is_err() {
      log::error!("[daemon] failed to register device");
    }

    log::info!("[daemon.started]");
    'main: loop {
      let command = receiver.try_recv();

      // receive command
      match command {
        Ok(DaemonCommand::Update(new_config)) => {
          log::info!("[daemon.updated]");
          config = new_config;
        }
        Ok(DaemonCommand::Suspend) => loop {
          log::info!("[daemon.suspended]");
          let command = receiver.recv();
          match command {
            Ok(DaemonCommand::Resume) => {
              log::info!("[daemon.resumed]");
              break;
            }
            Ok(_) => log::warn!("[daemon.suspended] command ignored"),
            Err(_) => break 'main,
          }
        },
        Ok(DaemonCommand::Resume) => log::warn!("[daemon.resumed] Already running"),
        Err(TryRecvError::Disconnected) => break,
        Err(TryRecvError::Empty) => {}
      }

      // running daemon
      let faill = device.sync(ticks % FORCE_RELOAD_TICKS == 0).is_err();
      if faill {
        log::warn!("[daemon] failed to sync");
      }

      let mut peak = 0.0_f32;
      let mut targets = HashSet::new();
      let sessions = device.current_sessions();
      for session in sessions.iter() {
        let name = &session.name;
        let is_target = config.targets.iter().any(|exclude| name.contains(exclude));

        if is_target {
          targets.insert(session);
        }

        let is_exclude = config.exclude.iter().any(|exclude| name.contains(exclude));
        let need_check = !is_target && !is_exclude;

        if need_check {
          if let Ok(session_peak) = session.volume.get_peak() {
            peak = peak.max(session_peak);
          }
        }
      }

      let status = VolumeStatus::new(peak > config.sensitivity);

      if status != volume_status {
        timeout += TICK;
        if status.is_timeout(timeout) {
          volume_status.toggle();
          expect_volume = volume_status.volume(&config);
          timeout = Duration::ZERO;
          transform = true;
        }
      } else {
        timeout = Duration::ZERO;
      }

      if transform {
        let mut fadeing = targets.len();
        for target in targets.iter() {
          let volume = target.volume.get_volume().unwrap();
          let offset = expect_volume - volume;
          let volume = if offset.abs() > TRANSFORM_SPEED {
            volume + offset.signum() * TRANSFORM_SPEED
          } else {
            fadeing -= 1;
            expect_volume
          };
          let _ = target.volume.set_volume(volume);
        }

        if fadeing == 0 {
          transform = false;
        }
      }

      ticks = ticks.wrapping_add(1);
      thread::sleep(TICK);
    }

    log::info!("[daemon.stopped]");
  });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeStatus {
  Restore,
  Reduce,
}

impl VolumeStatus {
  fn toggle(&mut self) {
    *self = match self {
      VolumeStatus::Restore => VolumeStatus::Reduce,
      VolumeStatus::Reduce => VolumeStatus::Restore,
    }
  }
  fn is_timeout(&self, time: Duration) -> bool {
    time
      >= match self {
        VolumeStatus::Restore => RESOTRE_TIMEOUT,
        VolumeStatus::Reduce => REDUCE_TIMEOUT,
      }
  }
  fn volume(&self, config: &Config) -> f32 {
    match self {
      VolumeStatus::Restore => config.resotre_volume,
      VolumeStatus::Reduce => config.reduce_volume,
    }
  }
  fn new(reduce: bool) -> Self {
    if reduce {
      VolumeStatus::Reduce
    } else {
      VolumeStatus::Restore
    }
  }
}
