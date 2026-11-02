//! What the calculators fetch (the forecast, the terrain, the light pollution
//! atlas), kept on disk, since the app has no service worker to keep it. How
//! long a copy is good for is the page's to say (web/src/lib/remote.js): this
//! only stores files and lets the oldest go.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::ipc::{InvokeBody, Request, Response};
use tauri::{AppHandle, Manager};

/// Enough for many spots, as on the site.
const ENTRIES: usize = 500;

/// A file's name: FNV-1a of its address, which is stable between builds. A
/// collision only loses a copy, since the file says which address it holds.
fn name_of(url: &str) -> String {
    let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64)
}

/// Keep `body` for `url`: when it was saved, the address, then the bytes.
fn store(dir: &Path, url: &str, body: &[u8], saved: u64) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut file = Vec::with_capacity(12 + url.len() + body.len());
    file.extend_from_slice(&saved.to_le_bytes());
    file.extend_from_slice(&(url.len() as u32).to_le_bytes());
    file.extend_from_slice(url.as_bytes());
    file.extend_from_slice(body);
    // Under another name first, so a crash cannot leave half a file to be read.
    let path = dir.join(name_of(url));
    let partial = path.with_extension("part");
    fs::write(&partial, file)?;
    fs::rename(partial, path)?;
    trim(dir, ENTRIES)
}

/// What is kept for `url`: when it was saved, then the bytes.
fn load(dir: &Path, url: &str) -> Option<Vec<u8>> {
    let file = fs::read(dir.join(name_of(url))).ok()?;
    let length = u32::from_le_bytes(file.get(8..12)?.try_into().ok()?) as usize;
    if file.get(12..12 + length)? != url.as_bytes() {
        return None;
    }
    let mut out = file[..8].to_vec();
    out.extend_from_slice(&file[12 + length..]);
    Some(out)
}

/// Let the oldest files go, down to `keep`.
fn trim(dir: &Path, keep: usize) -> std::io::Result<()> {
    let mut files: Vec<_> = fs::read_dir(dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            Some((entry.metadata().ok()?.modified().ok()?, entry.path()))
        })
        .collect();
    if files.len() > keep {
        files.sort();
        for (_, path) in &files[..files.len() - keep] {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}

fn folder(app: &AppHandle) -> Result<PathBuf, String> {
    app.path().app_cache_dir().map(|dir| dir.join("remote")).map_err(|e| e.to_string())
}

/// When a copy was saved and its bytes, or nothing at all when there is none.
#[tauri::command(async)]
pub fn cache_get(app: AppHandle, url: String) -> Result<Response, String> {
    Ok(Response::new(load(&folder(&app)?, &url).unwrap_or_default()))
}

/// Keep the request's body for the address in its `x-url` header.
#[tauri::command(async)]
pub fn cache_put(app: AppHandle, request: Request<'_>) -> Result<(), String> {
    let url = request.headers().get("x-url").and_then(|v| v.to_str().ok()).ok_or("no address")?;
    let InvokeBody::Raw(body) = request.body() else { return Err("no bytes".into()) };
    store(&folder(&app)?, url, body, now_ms()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lumiraw-cache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_copy_comes_back_with_when_it_was_saved() {
        let dir = temp("back");
        store(&dir, "https://example.org/a?b=1", &[1, 2, 3], 42).unwrap();
        let out = load(&dir, "https://example.org/a?b=1").unwrap();
        assert_eq!(u64::from_le_bytes(out[..8].try_into().unwrap()), 42);
        assert_eq!(&out[8..], &[1, 2, 3]);
        assert!(load(&dir, "https://example.org/other").is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn another_address_in_the_same_file_is_not_taken_for_this_one() {
        let dir = temp("collide");
        store(&dir, "https://example.org/a", &[1], 1).unwrap();
        fs::rename(dir.join(name_of("https://example.org/a")), dir.join(name_of("https://example.org/b"))).unwrap();
        assert!(load(&dir, "https://example.org/b").is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_damaged_file_is_no_copy_at_all() {
        let dir = temp("damaged");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name_of("https://example.org/a")), [1, 2, 3]).unwrap();
        assert!(load(&dir, "https://example.org/a").is_none());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_oldest_go_first() {
        let dir = temp("trim");
        fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["old", "middle", "new"].iter().enumerate() {
            let path = dir.join(name);
            fs::write(&path, [0]).unwrap();
            let when = UNIX_EPOCH + std::time::Duration::from_secs(1000 * (i as u64 + 1));
            fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
        }
        trim(&dir, 2).unwrap();
        assert!(!dir.join("old").exists() && dir.join("middle").exists() && dir.join("new").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
