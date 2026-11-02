//! LumiRaw in a window of its own: the same static site as lumiraw.app, served
//! from inside the app, with nothing sent anywhere.

// No console window behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cache;
#[cfg(target_os = "linux")]
mod linux;

fn main() {
    #[cfg(target_os = "linux")]
    linux::integrate();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![cache::cache_get, cache::cache_put])
        .run(tauri::generate_context!())
        .expect("the window could not be opened");
}
