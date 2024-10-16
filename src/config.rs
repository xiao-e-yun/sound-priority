use std::{env::current_exe, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  pub exclude: Vec<String>,
  pub targets: Vec<String>,

  pub resotre_volume: f32,
  pub reduce_volume: f32,
  pub sensitivity: f32,
}

impl Config {
  pub fn new() -> Self {
    Self {
      exclude: vec![],
      targets: vec![],
      resotre_volume: 1.0,
      reduce_volume: 0.5,
      sensitivity: 0.1,
    }
  }
  pub fn load() -> Option<Self> {
    let path = Self::path();
    if !path.exists() {
      return None;
    }
    let file = fs::File::open(path).expect("Failed to open config config file");
    serde_json::from_reader(file).ok()
  }
  pub fn save(&self) -> std::io::Result<()> {
    let path = Self::path();
    let json = serde_json::to_vec(self).expect("Failed to serialize config config");
    fs::write(path, json)
  }
  pub fn path() -> PathBuf {
    let path = current_exe().expect("Failed to get exe path");
    path.parent().unwrap().to_path_buf().join("config.json")
  }
}

impl Default for Config {
  fn default() -> Self {
    Self::new()
  }
}
