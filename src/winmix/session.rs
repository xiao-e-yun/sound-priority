use std::{hash::Hash, path::PathBuf};

use super::volume::SessionVolume;

#[derive(Debug)]
pub struct Session<'a> {
  /// The PID of the process that controls this audio session.
  pub pid: u32,
  /// The exe path for the process that controls this audio session.
  pub path: String,
  /// The name of the process that controls this audio session.
  pub name: String,
  /// A wrapper that lets you control the volume for this audio session.
  pub volume: SessionVolume<'a>,
}

impl<'a> Hash for Session<'a> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.pid.hash(state);
  }
}

impl<'a> PartialEq for Session<'a> {
  fn eq(&self, other: &Self) -> bool {
    self.pid == other.pid
  }
}

impl<'a> Eq for Session<'a> {}

impl<'a> Session<'a> {
  pub fn new(pid: u32, path: String, volume: SessionVolume<'a>) -> Self {
    // path to name without extension
    let name = PathBuf::from(&path)
      .file_stem()
      .expect("failed to get file stem")
      .to_string_lossy()
      .to_string();
    Session {
      pid,
      name,
      path,
      volume,
    }
  }
}
