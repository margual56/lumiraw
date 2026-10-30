//! An AppImage is a single file nobody installed, so the desktop has never
//! been told about it, and on Wayland that means no icon: the compositor finds
//! a window's icon through the `.desktop` file named after the window's app id
//! (`app.lumiraw.LumiRaw`, the bundle identifier), and GTK 3 has no other way
//! to hand one over. So the app tells the desktop about itself: its entry and
//! icons go into the user's own data directory, the way an installer would put
//! them, pointing at wherever the AppImage is now. Only when run as an
//! AppImage, and only when something has changed.

use std::fs;
use std::path::{Path, PathBuf};

/// The app id GTK gives the window; see `enableGTKAppId` in tauri.conf.json.
const APP_ID: &str = "app.lumiraw.LumiRaw";

const ICONS: [(&str, &[u8]); 4] = [
    ("32x32", include_bytes!("../icons/32x32.png")),
    ("128x128", include_bytes!("../icons/128x128.png")),
    ("256x256", include_bytes!("../icons/128x128@2x.png")),
    ("512x512", include_bytes!("../icons/icon.png")),
];

/// Where desktop entries and icons for this user go.
fn data_home() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")),
    }
}

/// The desktop entry for an AppImage at `appimage`.
fn entry(appimage: &Path) -> String {
    // Quoted, since the file may well sit in a folder with a space in its name.
    let exec = appimage.display().to_string().replace('\\', "\\\\").replace('"', "\\\"");
    format!("[Desktop Entry]\nType=Application\nName=LumiRaw\nComment=Raw photo developer\n\
             Exec=\"{exec}\"\nIcon={APP_ID}\nStartupWMClass={APP_ID}\n\
             Categories=Graphics;Photography;\nTerminal=false\n")
}

/// Write `bytes` to `path` unless it already holds exactly that.
fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

/// Put the entry and icons under `data` for an AppImage at `appimage`.
fn install(data: &Path, appimage: &Path) -> std::io::Result<()> {
    for (size, png) in ICONS {
        write_if_changed(&data.join(format!("icons/hicolor/{size}/apps/{APP_ID}.png")), png)?;
    }
    write_if_changed(&data.join(format!("applications/{APP_ID}.desktop")), entry(appimage).as_bytes())
}

/// Tell the desktop about this AppImage. A failure costs the icon and nothing
/// else, so it is not worth stopping the app over.
pub fn integrate() {
    let (Some(appimage), Some(data)) = (std::env::var_os("APPIMAGE"), data_home()) else { return };
    let _ = install(&data, Path::new(&appimage));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_and_icons_land_where_the_desktop_looks() {
        let data = std::env::temp_dir().join(format!("lumiraw-integrate-{}", std::process::id()));
        let appimage = Path::new("/home/someone/My Apps/LumiRaw_1.4.0_amd64.AppImage");
        install(&data, appimage).unwrap();
        let desktop = fs::read_to_string(data.join(format!("applications/{APP_ID}.desktop"))).unwrap();
        assert!(desktop.contains("Exec=\"/home/someone/My Apps/LumiRaw_1.4.0_amd64.AppImage\""));
        assert!(desktop.contains(&format!("Icon={APP_ID}\n")));
        assert!(desktop.contains(&format!("StartupWMClass={APP_ID}\n")));
        for (size, _) in ICONS {
            assert!(data.join(format!("icons/hicolor/{size}/apps/{APP_ID}.png")).is_file());
        }
        // Moved: the entry follows it.
        install(&data, Path::new("/opt/LumiRaw.AppImage")).unwrap();
        let moved = fs::read_to_string(data.join(format!("applications/{APP_ID}.desktop"))).unwrap();
        assert!(moved.contains("Exec=\"/opt/LumiRaw.AppImage\""));
        fs::remove_dir_all(&data).unwrap();
    }
}
