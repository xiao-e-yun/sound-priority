use std::env::current_exe;

use auto_launch::AutoLaunch;

use crate::{config::Config, APP_NAME};

#[derive(Debug, Clone)]
pub struct Settings {
  autolaunch: AutoLaunch,
  pub config: Config,
}

impl Settings {
  pub fn new(config: Config) -> Self {
    let autolaunch = {
      let path = current_exe().expect("failed to get exe path");
      let path = path.to_str().unwrap();
      AutoLaunch::new(APP_NAME, &path)
    };

    Self { autolaunch, config }
  }
  pub fn update(&mut self, config: Config) {
    self.config = config;
  }

  // functions
  pub fn get_autolaunch(&self) -> bool {
    self
      .autolaunch
      .is_enabled()
      .expect("failed to get autolaunch")
  }
  pub fn set_autolaunch(&mut self, autolaunch: bool) {
    if autolaunch {
      self
        .autolaunch
        .enable()
        .expect("failed to enable autolaunch");
    } else {
      self
        .autolaunch
        .disable()
        .expect("failed to disable autolaunch");
    }
  }

  pub fn select_exclude(&mut self, name: &str) {
    select_item(&mut self.config.exclude, name);
    self.save();
  }

  pub fn select_target(&mut self, name: &str) {
    select_item(&mut self.config.targets, name);
    self.save();
  }

  pub fn save(&self) {
    let _ = self.config.save();
  }
}

fn select_item(list: &mut Vec<String>, name: &str) {
  if list.contains(&name.to_string()) {
    list.retain(|n| n != name)
  } else {
    list.push(name.to_string())
  }
}
