#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod config;
pub mod deamon;
pub mod menu;
pub mod settings;
pub mod winmix;

use std::fs;
use std::vec::IntoIter;

use config::Config;
use deamon::Deamon;
use ftail::Ftail;
use menu::MenuSystem;
use settings::Settings;
use single_instance::SingleInstance;
use tray_icon::menu::MenuEvent;
use winit::application::ApplicationHandler;
use winit::event::DeviceEvent;
use winit::event::DeviceId;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::window::WindowId;

pub const APP_NAME: &str = "Sound Priority";

fn main() {
  start_logger();

  let instance = SingleInstance::new(APP_NAME).unwrap();
  if !instance.is_single() {
    log::info!("[main] detected another instance");
    return;
  }

  log::info!("[main] loading config");
  let config = Config::load().unwrap_or_default();

  log::info!("[main] loading settings");
  let settings = Settings::new(config.clone());

  log::info!("[main] loading menu");
  let mut menu = MenuSystem::new();

  log::info!("[main] update menu");
  menu.update(&settings);

  log::info!("[main] start daemon");
  let daemon = Deamon::create(config);

  log::info!("[main] start create event loop");
  let event_loop = EventLoop::builder().build().unwrap();
  event_loop.set_control_flow(ControlFlow::Wait);

  log::info!("[main] start create app");
  let mut app = App::new(daemon, settings, menu);

  log::info!("[main] mount app");
  event_loop.run_app(&mut app).unwrap();
}

struct App {
  pub daemon: Deamon,
  pub settings: Settings,
  pub menu: MenuSystem,
}

impl App {
  fn new(daemon: Deamon, settings: Settings, menu: MenuSystem) -> Self {
    Self {
      daemon,
      settings,
      menu,
    }
  }
  fn click_menu_item(&mut self, event: MenuEvent) -> bool {
    let id = event.id().0.as_str();
    let idents = id.split('.').collect::<Vec<_>>();
    let mut idents = idents.into_iter();

    log::info!("[main] click menu item: {}", id);
    match idents.next().unwrap_or_default() {
      "volume" => {
        let ident = idents.next().unwrap();
        let volume = get_slider_valuee(idents);
        let config = &mut self.settings.config;
        match ident {
          "sensitivity" => config.sensitivity = volume,
          "restore" => config.resotre_volume = volume,
          "reduce" => config.reduce_volume = volume,
          "speed" => config.transform_speed = volume,
          _ => unimplemented!(),
        }
        let _ = config.save();
        self.daemon.update(&config);
      }
      "apps" => {
        let app_name = idents.next().unwrap();
        match idents.next().unwrap() {
          "exclude" => self.settings.select_exclude(app_name),
          "target" => self.settings.select_target(app_name),
          _ => unimplemented!(),
        }
        self.daemon.update(&self.settings.config);
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

impl ApplicationHandler for App {
  fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, _: DeviceEvent) {
    let mut updated = false;

    if let Ok(event) = MenuEvent::receiver().try_recv() {
      updated |= self.click_menu_item(event);
    }

    // update menu
    if updated {
      self.menu.update(&self.settings);
    }
  }

  fn resumed(&mut self, _: &ActiveEventLoop) {}
  fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn start_logger() {
  let logfile = std::env::current_exe()
    .unwrap()
    .with_file_name("sound-priority.log");

  fs::remove_file(&logfile).ok();

  let logfile = logfile.to_str().unwrap_or("sound-priority.log");
  let mut ftail = Ftail::new();
  ftail = ftail.datetime_format("%m-%d %H:%M:%S");

  if cfg!(debug_assertions) {
    ftail = ftail.formatted_console(log::LevelFilter::Debug);
  }

  ftail = ftail.single_file(logfile, false, log::LevelFilter::Info);

  ftail.init().unwrap();
}
