use std::{env, error::Error, fs, path::Path};

use ini::Ini;
use nix::sys::utsname;

const X_SESSIONS: &str = "/usr/share/xsessions";
const WAYLAND_SESSIONS: &str = "/usr/share/wayland-sessions";

pub fn get_hostname() -> String {
  utsname::uname().nodename().to_string()
}

pub fn get_issue() -> Option<String> {
  let vtnr: usize = env::var("XDG_VTNR").unwrap_or_else(|_| "0".to_string()).parse().expect("unable to parse VTNR");
  let uts = utsname::uname();

  if let Ok(issue) = fs::read_to_string("/etc/issue") {
    return Some(
      issue
        .replace("\\S", "Linux")
        .replace("\\l", &format!("tty{}", vtnr))
        .replace("\\s", &uts.sysname())
        .replace("\\r", &uts.release())
        .replace("\\v", &uts.version())
        .replace("\\n", &uts.nodename())
        .replace("\\m", &uts.machine())
        .replace("\\\\", "\\"),
    );
  }

  None
}

pub fn get_sessions() -> Result<Vec<(String, String)>, Box<dyn Error>> {
  let directories = vec![X_SESSIONS, WAYLAND_SESSIONS];

  let files = directories
    .iter()
    .flat_map(|directory| fs::read_dir(&directory))
    .flat_map(|directory| directory.flat_map(|entry| entry.map(|entry| load_desktop_file(entry.path()))).flatten())
    .collect::<Vec<_>>();

  Ok(files)
}

fn load_desktop_file<P>(path: P) -> Result<(String, String), Box<dyn Error>>
where
  P: AsRef<Path>,
{
  let desktop = Ini::load_from_file(path)?;
  let section = desktop.section(Some("Desktop Entry")).ok_or("no Desktop Entry section in desktop file")?;

  let name = section.get("Name").ok_or("no Name property in desktop file")?;
  let exec = section.get("Exec").ok_or("no Exec property in desktop file")?;

  Ok((name.to_string(), exec.to_string()))
}
