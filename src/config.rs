use std::{env::current_exe, fs, path::PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
  #[serde(default)]
  pub exclude: Vec<String>,    
  #[serde(default)]
  pub targets: Vec<String>,

  #[serde(default = "default_transform_speed")]
  pub transform_speed: f32,
  #[serde(default = "default_resotre_volume")]
  pub resotre_volume: f32,
  #[serde(default = "default_reduce_volume")]
  pub reduce_volume: f32,
  #[serde(default = "default_sensitivity")]
  pub sensitivity: f32,
}


impl Config {
  pub fn new() -> Self {
    Self {
      exclude: vec![],
      targets: vec![],
      transform_speed: default_transform_speed(),
      resotre_volume: default_resotre_volume(),
      reduce_volume: default_reduce_volume(),
      sensitivity: default_sensitivity(),
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

fn default_transform_speed() -> f32 { 1.0 }
fn default_resotre_volume() -> f32 { 1.0 }
fn default_reduce_volume() -> f32 { 0.5 }
fn default_sensitivity() -> f32 { 0.1 }
