// SPDX-License-Identifier: GPL-3.0-or-later
#[cfg(not(test))]
use std::sync::Mutex;
pub(super) fn thumbnails(
    thumbs: &[nectarlink_core::PhotoThumb],
    extension: &str,
) -> std::collections::HashMap<String, String> {
    #[cfg(test)]
    {
        let _ = (thumbs, extension);
        Default::default()
    }
    #[cfg(not(test))]
    {
        use std::io::Write;
        static CACHE: Mutex<Vec<std::path::PathBuf>> = Mutex::new(Vec::new());
        static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let mut files = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = crate::core_host::host().data_dir.join("noctalia-photo-previews");
        if std::fs::create_dir_all(&dir).is_err() {
            return Default::default();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).is_err() {
                return Default::default();
            }
        }
        let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // This dedicated cache contains only our generated preview files.
        // Retain four pages so an older in-flight read cannot delete the
        // current page's images. Clean stale previews on the first read.
        if generation == 0 {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name
                        .to_str()
                        .and_then(|n| n.strip_prefix("preview-"))
                        .and_then(|n| n.strip_suffix(".jpg").or_else(|| n.strip_suffix(".png")))
                        .is_some_and(|n| {
                            !n.is_empty() && n.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
                        })
                    {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
        while files.len() > 288 {
            let path = files.remove(0);
            let _ = std::fs::remove_file(path);
        }
        let mut result = std::collections::HashMap::new();
        for t in thumbs.iter().filter(|t| t.data.len() <= 128 * 1024).take(96) {
            let path = dir.join(format!("preview-{}.{extension}", blake3::hash(&t.data).to_hex()));
            if let Some(index) = files.iter().position(|p| p == &path) {
                if path.is_file() {
                    let reused = files.remove(index);
                    files.push(reused);
                    result.insert(t.id.clone(), path.to_string_lossy().into_owned());
                    continue;
                }
                files.remove(index);
            }
            if crate::settings::create_private_file(&path).and_then(|mut f| f.write_all(&t.data)).is_ok() {
                result.insert(t.id.clone(), path.to_string_lossy().into_owned());
                files.push(path);
            }
        }
        result
    }
}
