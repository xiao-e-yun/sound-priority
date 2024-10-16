#![windows_subsystem = "windows"]

pub mod config;
pub mod menu;
pub mod settings;
pub mod winmix;

use std::collections::HashSet;
use std::sync::mpsc::RecvError;
use std::sync::mpsc::Sender;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;
use std::vec::IntoIter;

use config::Config;
use menu::MenuSystem;
use settings::Settings;
use tray_icon::menu::MenuEvent;
use tray_icon::Icon;
use tray_icon::TrayIconBuilder;
use winit::application::ApplicationHandler;
use winit::event::DeviceEvent;
use winit::event::DeviceId;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::window::WindowId;
use winmix::WinMix;

pub const APP_NAME: &str = "Volume Controller";

pub fn main() {
  let config = Config::load().unwrap_or_default();

  let tray = TrayIconBuilder::new()
    .with_tooltip(APP_NAME)
    .with_icon(Icon::from_resource(32512, None).expect("failed to load icon"))
    .with_menu_on_left_click(true)
    .build()
    .unwrap();

  let settings = Settings::new(config.clone());
  let mut menu = MenuSystem::new(tray);
  menu.update(&settings);

  let daemon = start_daemon(config);

  let mut app = App {
    settings,
    menu,
    daemon,
  };

  let event_loop = EventLoop::builder().build().unwrap();
  event_loop.set_control_flow(ControlFlow::Wait);
  event_loop.run_app(&mut app).unwrap();
}

pub enum DaemonCommand {
  Resumed,
  Suspended,
  Update(Config),
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

const TICK: Duration = Duration::from_millis(100);
const TRANSFORM_SPEED: f32 = 0.05;
const REFRESH_AFTER_TICKS: usize = 50;

const REDUCE_TIMEOUT: Duration = Duration::from_millis(200);
const RESOTRE_TIMEOUT: Duration = Duration::from_secs(3);

fn start_daemon(mut config: Config) -> Sender<DaemonCommand> {
  let (send, recv) = std::sync::mpsc::channel();

  thread::spawn(move || {
    let winmix = WinMix::default();
    let mut transform = true;

    let mut ticks = 0_usize;

    let mut volume_status = VolumeStatus::Restore;
    let mut expect_volume = config.resotre_volume;
    let mut timeout = Duration::ZERO;

    let mut sessions = winmix.get_default().unwrap().sessions;
    'main: loop {
      let command = recv.try_recv();

      // receive command
      match command {
        Ok(DaemonCommand::Update(new_config)) => {
          println!("updated config");
          config = new_config;
        }
        Ok(DaemonCommand::Suspended) => loop {
          let command = recv.recv();
          match command {
            Ok(DaemonCommand::Resumed) => break,
            Err(RecvError) => break 'main,
            _ => {}
          }
        },
        Err(TryRecvError::Disconnected) => break,
        Err(TryRecvError::Empty) | Ok(DaemonCommand::Resumed) => {}
      }

      // running daemon
      if ticks % REFRESH_AFTER_TICKS == 0 {
        let derive = winmix.get_default().unwrap();
        sessions = derive.sessions;
      }

      let mut peak = 0.0_f32;
      let mut targets = HashSet::new();
      for session in sessions.iter() {
        let is_target = config
          .targets
          .iter()
          .any(|target| session.name.contains(target));
        let need_check = !(is_target
          || config
            .exclude
            .iter()
            .any(|exclude| session.name.contains(exclude)));

        if need_check {
          if let Ok(session_peak) = session.vol.get_peak() {
            peak = peak.max(session_peak);
          }
        }

        if is_target {
          targets.insert(session);
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
          let volume = target.vol.get_volume().unwrap();
          let offset = expect_volume - volume;
          let volume = if offset.abs() > TRANSFORM_SPEED {
            volume + offset.signum() * TRANSFORM_SPEED
          } else {
            fadeing -= 1;
            expect_volume
          };
          let _ = target.vol.set_volume(volume);
        }

        if fadeing == 0 {
          transform = false;
        }
      }

      ticks = ticks.wrapping_add(1);
      thread::sleep(TICK);
    }
  });

  send
}

pub struct App {
  pub daemon: Sender<DaemonCommand>,
  pub settings: Settings,
  pub menu: MenuSystem,
}

impl ApplicationHandler for App {
  fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, _: DeviceEvent) {
    let mut updated = false;

    if let Ok(event) = MenuEvent::receiver().try_recv() {
      updated |= self.click_menu_item(event);
    }

    // update menu
    if updated {
      println!("reload menu");
      self.menu.update(&self.settings);
    }
  }

  fn resumed(&mut self, _: &ActiveEventLoop) {}
  fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

impl App {
  fn click_menu_item(&mut self, event: MenuEvent) -> bool {
    let id = event.id().0.as_str();
    let idents = id.split('.').collect::<Vec<_>>();
    let mut idents = idents.into_iter();

    match idents.next().unwrap() {
      "volume" => {
        let ident = idents.next().unwrap();
        let volume = get_slider_valuee(idents);
        let config = &mut self.settings.config;
        match ident {
          "sensitivity" => config.sensitivity = volume,
          "restore" => config.resotre_volume = volume,
          "reduce" => config.reduce_volume = volume,
          _ => unimplemented!(),
        }
        config.save().unwrap();
        self
          .daemon
          .send(DaemonCommand::Update(config.clone()))
          .unwrap();
      }
      "apps" => {
        let app_name = idents.next().unwrap();
        match idents.next().unwrap() {
          "exclude" => self.settings.select_exclude(app_name),
          "target" => self.settings.select_target(app_name),
          _ => unimplemented!(),
        }
        self
          .daemon
          .send(DaemonCommand::Update(self.settings.config.clone()))
          .unwrap();
      }
      "settings" => match idents.next().unwrap() {
        "autolaunch" => {
          let autolaunch = self.settings.get_autolaunch();
          self.settings.set_autolaunch(!autolaunch);
        }
        _ => unimplemented!(),
      },
      //--------------------------------
      "exit" => std::process::exit(0),
      "reload" => {}
      _ => {
        return false;
      }
    }

    fn get_slider_valuee(mut event: IntoIter<&str>) -> f32 {
      match event.next().unwrap() {
        "a" => 1.0,
        "9" => 0.9,
        "8" => 0.8,
        "7" => 0.7,
        "6" => 0.6,
        "5" => 0.5,
        "4" => 0.4,
        "3" => 0.3,
        "2" => 0.2,
        "1" => 0.1,
        "0" => 0.0,
        _ => unreachable!(),
      }
    }

    true
  }
}

//     builder
//         .setup(move |app| {
//             let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
//             let menu = Menu::with_items(app, &[&quit_item])?;
//             let _tray = TrayIconBuilder::new()
//                 .icon(app.default_window_icon().unwrap().clone())
//                 .menu(&menu)
//                 .menu_on_left_click(false)
//                 .on_menu_event(|app, event| match event.id.as_ref() {
//                     "quit" => {
//                         app.exit(0);
//                     }
//                     _ => {}
//                 })
//                 .on_tray_icon_event(|tray, event| match event {
//                     TrayIconEvent::DoubleClick { .. } => {
//                         let app = tray.app_handle();
//                         if let Some(window) = app.get_webview_window("main") {
//                             let _ = app.emit("show", true);
//                             let _ = window.show();
//                             let _ = window.set_focus();
//                         }
//                     }
//                     _ => {}
//                 })
//                 .build(app)?;

//             app.manage(Mutex::new(None::<Sender<MixerCommand>>));
//             app.manage(cli.clone());

//             let window = app.get_webview_window("main").expect("no main window");

//             if !cli.quiet {
//                 let _ = app.emit("show", true);
//                 let _ = window.show().unwrap();
//             }

//             window.clone().on_window_event(move |event| {
//                 if let WindowEvent::CloseRequested { api, .. } = event {
//                     api.prevent_close();
//                     let _ = window.clone().hide().unwrap();
//                     let _ = window.app_handle().emit("show", false);
//                 }
//             });

//             start_mixer(app.state(), app.state());

//             Ok(())
//         })
//         .plugin(tauri_plugin_shell::init())
//         .invoke_handler(tauri::generate_handler![
//             get_mixer,
//             start_mixer,
//             update_mixer,
//             get_default_derive,
//             set_autolaunch,
//             get_autolaunch,
//         ])
//         .run(tauri::generate_context!())
//         .expect("error while running tauri application");
// }

// #[tauri::command]
// fn start_mixer(cli: State<Cli>, sender: State<Mutex<Option<Sender<MixerCommand>>>>) {
//     let mixer = Mixer::from_path(cli.configs.clone()).unwrap_or_default();
//     if let Some(sender) = sender.lock().unwrap().as_ref() {
//         let _ = sender.send(MixerCommand::Stop);
//     }
//     unsafe {
//         let (tx, rx) = std::sync::mpsc::channel();
//         thread::spawn(move || {
//             println!("start mixer");
//             let winmix = WinMix::default();
//             let mut include = mixer.include;
//             let mut targets = mixer.targets;
//             let mut exclude = mixer.exclude;
//             let mut resotre_volume = mixer.resotre_volume;
//             let mut reduce_volume = mixer.reduce_volume;

//             let mut reduce = false;
//             let mut ticks = 0_usize;
//             let mut transform = true;

//             let volume_step = 0.02;
//             loop {
//                 match rx.try_recv() {
//                     Ok(MixerCommand::Update(new_mixer)) => {
//                         println!("update mixer");
//                         include = new_mixer.include;
//                         targets = new_mixer.targets;
//                         exclude = new_mixer.exclude;
//                         resotre_volume = new_mixer.resotre_volume;
//                         reduce_volume = new_mixer.reduce_volume;
//                     }
//                     Ok(MixerCommand::Stop) | Err(TryRecvError::Disconnected) => {
//                         println!("stop mixer");
//                         break;
//                     }
//                     Err(TryRecvError::Empty) => {}
//                 }

//                 let derive = winmix.get_default().unwrap();
//                 let sessions = derive.sessions;

//                 let mut volume_targets = vec![];
//                 let sessions: Vec<_> = sessions
//                     .iter()
//                     .filter_map(|session| {
//                         let name = &session.name;
//                         if targets.iter().any(|target| name.contains(target)) {
//                             volume_targets.push(session);
//                             return None;
//                         }
//                         if exclude.iter().any(|exclude| name.contains(exclude)) {
//                             return None;
//                         }
//                         if include.is_empty()
//                             || include.iter().any(|include| name.contains(include))
//                         {
//                             return Some(session.vol.get_peak().unwrap());
//                         }
//                         None
//                     })
//                     .collect();

//                 let max_volume = sessions
//                     .iter()
//                     .max_by(|x, y| x.partial_cmp(y).unwrap())
//                     .unwrap();
//                 let is_reduce = *max_volume > 0.1;

//                 if is_reduce != reduce {
//                     ticks += 1;
//                     let transform_ticks = if reduce { 60 } else { 3 };
//                     if ticks >= transform_ticks {
//                         reduce = is_reduce;
//                         transform = true;
//                         ticks = 0;
//                     }
//                 } else {
//                     ticks = 0;
//                 }

//                 if transform {
//                     let expect_volume = if reduce {
//                         reduce_volume
//                     } else {
//                         resotre_volume
//                     };

//                     let mut fadeing = volume_targets.len();
//                     for target in volume_targets {
//                         let prev_volume = target.vol.get_volume().unwrap();
//                         let offset = expect_volume - prev_volume;
//                         if offset.abs() < f32::EPSILON {
//                             continue;
//                         } else {
//                             let volume = if offset.abs() > volume_step {
//                                 volume_step * offset.signum() + prev_volume
//                             } else {
//                                 fadeing -= 1;
//                                 expect_volume
//                             };
//                             let _ = target.vol.set_volume(volume);
//                         }
//                     }

//                     if fadeing == 0 {
//                         transform = false;
//                     }
//                 }

//                 sleep(Duration::from_millis(50));
//             }
//         });
//         *sender.lock().unwrap() = Some(tx);
//     }
// }

// #[tauri::command]
// fn get_max_volume(winmix: State<WinMix>, filter: Vec<String>) -> f32 {
//     unsafe {
//         let derive = winmix.get_default().unwrap();
//         derive
//             .sessions
//             .iter()
//             .filter_map(|session| {
//                 if filter.contains(&session.name) {
//                     return None;
//                 }
//                 Some(session.vol.get_peak().unwrap())
//             })
//             .max_by(|x, y| x.partial_cmp(y).unwrap())
//             .expect("failed to get max volume")
//     }
// }

// #[tauri::command]
// fn get_min_volume(winmix: State<WinMix>, filter: Vec<String>) -> f32 {
//     unsafe {
//         let derive = winmix.get_default().unwrap();
//         derive
//             .sessions
//             .iter()
//             .filter_map(|session| {
//                 if filter.contains(&session.name) {
//                     return None;
//                 }
//                 Some(session.vol.get_peak().unwrap())
//             })
//             .min_by(|x, y| x.partial_cmp(y).unwrap())
//             .expect("failed to get max volume")
//     }
// }

// #[tauri::command]
// fn get_default_derive() -> DeriveView {
//     let winmix = WinMix::default();
//     unsafe {
//         let derive = winmix.get_default().unwrap();
//         derive.view().expect("failed display derive info")
//     }
// }

// #[tauri::command]
// fn get_mixer(cli: State<Cli>) -> Mixer {
//     let mixer = Mixer::from_path(cli.configs.clone()).unwrap_or_default();
//     mixer
// }

// #[tauri::command]
// fn update_mixer(cli: State<Cli>, sender: State<Mutex<Option<Sender<MixerCommand>>>>, mixer: Mixer) {
//     mixer.save(cli.configs.clone()).unwrap();
//     if let Some(sender) = sender.lock().unwrap().as_ref() {
//         let _ = sender.send(MixerCommand::Update(mixer));
//     }
// }

// #[cfg(desktop)]
// #[tauri::command]
// fn set_autolaunch(app: AppHandle, set: bool) {
//     // Get the autostart manager
//     let autostart_manager = app.autolaunch();
//     if set {
//         // Enable autostart
//         autostart_manager
//             .enable()
//             .expect("failed to enable autostart");
//     } else {
//         // Disable autostart
//         autostart_manager
//             .disable()
//             .expect("failed to disable autostart");
//     }
// }

// #[cfg(desktop)]
// #[tauri::command]
// fn get_autolaunch(app: AppHandle) -> bool {
//     let autostart_manager = app.autolaunch();
//     autostart_manager.is_enabled().unwrap()
// }
