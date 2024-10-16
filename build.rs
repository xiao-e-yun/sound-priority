use {
  ico::{IconDir, IconDirEntry, IconImage, ResourceType},
  std::{env, fs::File, io},
  winresource::WindowsResource,
};

fn main() -> io::Result<()> {
  if env::var_os("CARGO_CFG_WINDOWS").is_some() {
    // parse the icon file and generate the icon
    let icon = generate_icon("assets/icon.png");

    // add the icon to the resources
    WindowsResource::new().set_icon(icon).compile()?;
  }
  Ok(())
}

fn generate_icon(from: &str) -> &'static str {
  let icon = "assets/.favicon.ico";

  let mut icon_dir = IconDir::new(ResourceType::Icon);

  let file = File::open(from).unwrap();
  let image = IconImage::read_png(file).unwrap();
  icon_dir.add_entry(IconDirEntry::encode(&image).unwrap());

  let file = File::create(icon).unwrap();
  icon_dir.write(file).unwrap();

  icon
}
