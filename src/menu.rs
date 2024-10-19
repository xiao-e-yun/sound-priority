use std::collections::HashSet;

use convert_case::{Case, Casing};
use tray_icon::{
  menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
  Icon, TrayIcon, TrayIconBuilder,
};

use crate::{settings::Settings, winmix::WinMix, APP_NAME};

pub struct MenuSystem {
  tray: TrayIcon,
}

impl MenuSystem {
  pub fn new() -> Self {
    let tray = TrayIconBuilder::new()
      .with_tooltip(APP_NAME)
      .with_icon(Icon::from_resource(32512, None).expect("failed to load icon"))
      .with_menu_on_left_click(true)
      .build()
      .unwrap();
    Self { tray }
  }
  pub fn update(&mut self, settings: &Settings) {
    log::info!("[menu] update menu");
    let menu = Menu::with_items(&[
      &MenuItem::with_id("reload", "Reload", true, None),
      &PredefinedMenuItem::separator(),
    ])
    .unwrap();

    log::info!("[menu] reload apps list");
    let apps = self.get_apps(settings);
    for app in apps.into_iter() {
      let app = app.as_ref();
      menu.append(app).expect("failed to create menu");
    }

    log::info!("[menu] reload settings");
    menu
      .append_items(&[
        &PredefinedMenuItem::separator(),
        &self.get_settings(settings),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id("exit", "&Exit", true, None),
      ])
      .unwrap();

    log::info!("[menu] flush menu");
    self.tray.set_menu(Some(Box::new(menu)));
  }
  pub fn get_apps(&self, settings: &Settings) -> Vec<Box<dyn IsMenuItem>> {
    let config = &settings.config;

    let mut exclude = config.exclude.clone();
    let mut targets = config.targets.clone();
    let mut sessions: Vec<String> = {
      let winmix = WinMix::default();
       // we only reload the apps list after operation
       // so we can just get the current default
      let device = winmix.get_default();
      let sessions = device.and_then(|device| device.get_sessions());
      sessions.map(|session| session.into_iter().map(|session| session.name).collect())
    }
    .unwrap_or_default();

    exclude.sort();
    targets.sort();
    sessions.sort();

    let list = [exclude.clone(), targets.clone(), sessions.clone()].concat();
    let mut set = HashSet::new();

    list
      .into_iter()
      .filter_map(|name| {
        if set.contains(&name) {
          return None;
        } else {
          set.insert(name.clone());
        }

        let is_exclude = exclude.contains(&name);
        let is_target = targets.contains(&name);

        let display_name = {
          let mut name = name.clone();
          if name.starts_with('$') {
            name.remove(0);
          }

          name = name.to_case(Case::Title);
          if name.len() > 30 {
            name.truncate(27);
            name.push_str("...");
          }

          if is_exclude {
            name.push_str(" ×");
          }
          if is_target {
            name.push_str(" ♪");
          }
          name
        };

        let name = name.replace(" ", "/");

        let menu = Submenu::with_items(
          display_name,
          true,
          &[
            &MenuItem::with_id(
              &format!("apps.{}.target", name),
              checkbox("Target", is_target),
              !is_exclude,
              None,
            ),
            &MenuItem::with_id(
              &format!("apps.{}.exclude", name),
              checkbox("Exclude", is_exclude),
              !is_target,
              None,
            ),
          ],
        )
        .unwrap();

        Some(Box::new(menu) as Box<dyn IsMenuItem>)
      })
      .collect()
  }
  pub fn get_settings(&self, settings: &Settings) -> Submenu {
    let config = &settings.config;
    let settings = Submenu::with_items(
      "Settings",
      true,
      &[
        &slider("volume.sensitivity", "Sensitivity", config.sensitivity),
        &slider("volume.restore", "Restore Volume", config.resotre_volume),
        &slider("volume.reduce", "Reduce Volume", config.reduce_volume),
        &MenuItem::with_id(
          "settings.autolaunch",
          checkbox("Launch on startup", settings.get_autolaunch()),
          true,
          None,
        ),
      ],
    )
    .expect("failed to create settings submenu");

    fn slider(id: &str, text: &str, value: f32) -> Submenu {
      fn enabled(value: f32, condition: f32) -> bool {
        (value - condition).abs() > f32::EPSILON
      }

      Submenu::with_id_and_items(
        id,
        format!("{} ({})", text, value),
        true,
        &[
          &MenuItem::with_id(format!("{}.a", id), "100%", enabled(value, 1.0), None),
          &MenuItem::with_id(format!("{}.9", id), "90%", enabled(value, 0.9), None),
          &MenuItem::with_id(format!("{}.8", id), "80%", enabled(value, 0.8), None),
          &MenuItem::with_id(format!("{}.7", id), "70%", enabled(value, 0.7), None),
          &MenuItem::with_id(format!("{}.6", id), "60%", enabled(value, 0.6), None),
          &MenuItem::with_id(format!("{}.5", id), "50%", enabled(value, 0.5), None),
          &MenuItem::with_id(format!("{}.4", id), "40%", enabled(value, 0.4), None),
          &MenuItem::with_id(format!("{}.3", id), "30%", enabled(value, 0.3), None),
          &MenuItem::with_id(format!("{}.2", id), "20%", enabled(value, 0.2), None),
          &MenuItem::with_id(format!("{}.1", id), "10%", enabled(value, 0.1), None),
          &MenuItem::with_id(format!("{}.0", id), " 0%", enabled(value, 0.0), None),
        ],
      )
      .unwrap()
    }

    settings
  }
}

fn checkbox(name: &str, value: bool) -> String {
  let icon = if value { "✔" } else { "✖" };
  format!("[{}] {}", icon, name)
}
