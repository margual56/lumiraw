//! LumiRaw in a window of its own: the same static site as lumiraw.app, served
//! from inside the app, with nothing sent anywhere.

// No console window behind the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("the window could not be opened");
}
