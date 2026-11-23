// SPDX-License-Identifier: GPL-3.0-or-later
use super::TOKEN_FILE_NAME;
use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
/// Contents of `<data_dir>/api-token.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiTokenFile {
    pub port: u16,
    pub token: String,
}

/// Returns the path to `<data_dir>/api-token.json` for a specific `data_dir`.
pub fn token_file_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(TOKEN_FILE_NAME)
}

/// Returns the path to `<data_dir>/api-token.json` for the running desktop host.
pub fn token_file_path() -> PathBuf {
    token_file_path_in(&crate::core_host::host().data_dir)
}

/// Generates a 256-bit (32-byte) cryptographically random token formatted as
/// 64 lowercase hexadecimal characters, from the system's secure random generator.
/// `None` if no secure source answered: the API then stays off rather than run with a
/// guessable token.
pub fn generate_token() -> Option<String> {
    let mut bytes = [0u8; 32];
    crate::win::secure_random(&mut bytes).then(|| HEXLOWER.encode(&bytes))
}

fn is_valid_token_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Reads and validates `<data_dir>/api-token.json` if present.
pub fn read_token_file(data_dir: &Path) -> Option<ApiTokenFile> {
    let text = fs::read_to_string(token_file_path_in(data_dir)).ok()?;
    let parsed: ApiTokenFile = serde_json::from_str(&text).ok()?;
    if parsed.port > 0 && is_valid_token_hex(&parsed.token) { Some(parsed) } else { None }
}

/// Atomically writes `<data_dir>/api-token.json` with `port` and `token`.
pub fn write_token_file(data_dir: &Path, port: u16, token: &str) -> std::io::Result<PathBuf> {
    fs::create_dir_all(data_dir)?;
    let path = token_file_path_in(data_dir);
    let tmp = path.with_extension("json.tmp");
    let data = ApiTokenFile { port, token: token.to_owned() };
    {
        let mut file = crate::settings::create_private_file(&tmp)?;
        file.write_all(serde_json::to_string_pretty(&data)?.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Ensures `<data_dir>/api-token.json` exists with a 64-hex-char 256-bit token
/// and the given `port`.
pub fn ensure_token_file(data_dir: &Path, port: u16) -> std::io::Result<ApiTokenFile> {
    let token = match read_token_file(data_dir) {
        Some(existing) => existing.token,
        None => generate_token()
            .ok_or_else(|| std::io::Error::other("no secure random source for the API token"))?,
    };
    write_token_file(data_dir, port, &token)?;
    Ok(ApiTokenFile { port, token })
}

/// Removes `<data_dir>/api-token.json` when the local API is disabled.
pub fn remove_token_file(data_dir: &Path) {
    let path = token_file_path_in(data_dir);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

/// Constant-time string comparison for bearer tokens.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let max_len = ab.len().max(bb.len());
    let mut diff = ab.len() ^ bb.len();
    for i in 0..max_len {
        let x = ab.get(i).copied().unwrap_or(0);
        let y = bb.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generates_64_hex_char_token_and_persists_api_token_json() {
        let dir = tempfile::tempdir().unwrap();
        let tok = generate_token().expect("secure random");
        assert_eq!(tok.len(), 64);
        assert!(tok.chars().all(|c| c.is_ascii_hexdigit()));

        let created = ensure_token_file(dir.path(), 53320).unwrap();
        assert_eq!(created.port, 53320);
        assert_eq!(created.token.len(), 64);

        let loaded = read_token_file(dir.path()).unwrap();
        assert_eq!(loaded, created);

        // Calling ensure_token_file again preserves the existing token.
        let again = ensure_token_file(dir.path(), 54000).unwrap();
        assert_eq!(again.token, created.token);
        assert_eq!(again.port, 54000);

        remove_token_file(dir.path());
        assert!(read_token_file(dir.path()).is_none());
    }
}
