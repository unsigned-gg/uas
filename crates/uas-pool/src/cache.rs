// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Slot build cache retention and eviction (spec 0047).
//!
//! Provides isolated, per-slot cargo build cache retention across lease cycles.
//! Each slot build cache lives outside the worktree under `${XDG_CACHE_HOME:-~/.cache}/uas/slots/<i>/`.
//! Caches are managed under host slot locks and bounded by per-slot disk quotas.

use std::collections::BTreeMap;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use uas_domain::HostSlotIndex;

/// Default maximum size for each slot build cache in bytes (20 GiB, spec 0047 D4).
pub const DEFAULT_CACHE_MAX_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Default maximum size for the shared sccache store in bytes (10 GiB, spec 0047 D5).
pub const DEFAULT_SCCACHE_MAX_BYTES: u64 = 10 * 1024 * 1024 * 1024;

/// Subdirectory containing access timestamp stamps.
pub const STAMPS_DIR_NAME: &str = ".stamps";

/// Outcome of a slot cache eviction pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EvictionReport {
    /// Slot cache size in bytes before eviction.
    pub bytes_before: u64,
    /// Slot cache size in bytes after eviction.
    pub bytes_after: u64,
    /// Number of cache key directories removed during eviction.
    pub evicted_count: u32,
}

/// Resolves the base root directory for UAS caches.
#[must_use]
pub fn base_cache_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(xdg).join("uas")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".cache").join("uas")
    } else {
        PathBuf::from("/tmp/uas-cache")
    }
}

/// Sanitizes a toolchain channel name according to spec 0047 D3.
///
/// Every byte outside `[A-Za-z0-9._-]` is replaced by `_`.
#[must_use]
pub fn sanitize_channel(channel: &str) -> String {
    let mut out = String::with_capacity(channel.len());
    for b in channel.bytes() {
        if b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-' {
            out.push(b as char);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "default".to_string()
    } else {
        out
    }
}

/// Reads the toolchain channel from the target repository directory.
#[must_use]
pub fn read_toolchain_channel(repo_path: &Path) -> String {
    let toml_path = repo_path.join("rust-toolchain.toml");
    if let Ok(content) = fs::read_to_string(&toml_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("channel") {
                let rest = rest.trim();
                if let Some(val) = rest.strip_prefix('=') {
                    let val = val.trim().trim_matches('"').trim_matches('\'').trim();
                    if !val.is_empty() {
                        return sanitize_channel(val);
                    }
                }
            }
        }
    }

    let legacy_path = repo_path.join("rust-toolchain");
    if let Ok(content) = fs::read_to_string(&legacy_path) {
        let trimmed = content.trim();
        if !trimmed.is_empty() {
            return sanitize_channel(trimmed);
        }
    }

    "default".to_string()
}

/// Derives the canonical cache key directory name for a base repository (spec 0047 D3, REQ-0047-04).
///
/// Format: `<repo16>-<channel>`.
pub fn derive_key(base_repo_path: &Path) -> Result<String, std::io::Error> {
    let canonical = match base_repo_path.canonicalize() {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => base_repo_path.to_path_buf(),
        Err(e) => return Err(e),
    };

    let channel = read_toolchain_channel(&canonical);
    derive_key_with_channel(&canonical, Some(&channel))
}

/// Derives the canonical cache key directory name with an explicit toolchain channel.
pub fn derive_key_with_channel(
    base_repo_path: &Path,
    channel: Option<&str>,
) -> Result<String, std::io::Error> {
    let canonical = match base_repo_path.canonicalize() {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => base_repo_path.to_path_buf(),
        Err(e) => return Err(e),
    };

    let canonical_str = canonical.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(canonical_str.as_bytes());
    let digest = hasher.finalize();

    let mut hex = String::with_capacity(16);
    for byte in &digest[..8] {
        hex.push_str(&format!("{byte:02x}"));
    }

    let ch = channel
        .map(sanitize_channel)
        .unwrap_or_else(|| "default".to_string());
    Ok(format!("{hex}-{ch}"))
}

/// Resolves an executable binary in the system `PATH`.
#[must_use]
pub fn resolve_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            if let Ok(meta) = candidate.metadata() {
                if meta.permissions().mode() & 0o111 != 0 {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// Computes the disk space occupied by a directory or file in bytes.
pub fn calculate_dir_size(path: &Path) -> Result<u64, std::io::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.is_file() || meta.is_symlink() {
        return Ok(meta.len());
    }

    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_meta = entry.metadata()?;
        if entry_meta.is_dir() {
            total += calculate_dir_size(&entry.path())?;
        } else {
            total += entry_meta.len();
        }
    }
    Ok(total)
}

/// Manages build cache state for a single host slot.
#[derive(Debug, Clone)]
pub struct SlotBuildCache {
    host_index: HostSlotIndex,
    root_dir: PathBuf,
}

impl SlotBuildCache {
    /// Creates a slot build cache manager for the given host slot index.
    #[must_use]
    pub fn new(host_index: HostSlotIndex, base_dir: Option<PathBuf>) -> Self {
        let base = base_dir.unwrap_or_else(base_cache_dir);
        let root_dir = base.join("slots").join(host_index.0.to_string());
        Self {
            host_index,
            root_dir,
        }
    }

    /// Returns the host slot index.
    #[must_use]
    pub const fn host_index(&self) -> HostSlotIndex {
        self.host_index
    }

    /// Returns the root directory of this slot cache.
    #[must_use]
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Returns the stamps directory path for this slot cache.
    #[must_use]
    pub fn stamps_dir(&self) -> PathBuf {
        self.root_dir.join(STAMPS_DIR_NAME)
    }

    /// Returns the key directory path for a specific cache key.
    #[must_use]
    pub fn key_dir(&self, key: &str) -> PathBuf {
        self.root_dir.join(key)
    }

    /// Returns the target build output directory path for a specific cache key.
    #[must_use]
    pub fn target_dir(&self, key: &str) -> PathBuf {
        self.key_dir(key).join("target")
    }

    /// Initializes the slot cache directory with mode 0700 (spec 0047 D1).
    pub fn init(&self) -> Result<(), std::io::Error> {
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        builder.mode(0o700);
        builder.create(&self.root_dir)?;
        builder.create(self.stamps_dir())?;
        Ok(())
    }

    /// Records access to a key by touching `.stamps/<key>` (spec 0047 D4, REQ-0047-06).
    pub fn touch_stamp(&self, key: &str) -> Result<PathBuf, std::io::Error> {
        self.init()?;
        let stamp_path = self.stamps_dir().join(key);
        if stamp_path.exists() {
            let file = OpenOptions::new().write(true).open(&stamp_path)?;
            file.set_modified(SystemTime::now())?;
        } else {
            File::create(&stamp_path)?;
        }
        Ok(stamp_path)
    }

    /// Prepares the key directory and target subdirectory with mode 0700.
    pub fn prepare_key_dir(&self, key: &str) -> Result<PathBuf, std::io::Error> {
        self.init()?;
        let target = self.target_dir(key);
        let mut builder = DirBuilder::new();
        builder.recursive(true);
        builder.mode(0o700);
        builder.create(&target)?;
        self.touch_stamp(key)?;
        Ok(self.key_dir(key))
    }

    /// Performs quota eviction, removing whole key directories oldest first (spec 0047 D4, REQ-0047-05).
    pub fn evict(&self, cache_max_bytes: u64) -> Result<EvictionReport, std::io::Error> {
        if !self.root_dir.exists() {
            return Ok(EvictionReport::default());
        }

        let mut key_entries = Vec::new();
        let mut total_size = 0u64;

        for entry in fs::read_dir(&self.root_dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                // Skip stamps or hidden entries
                continue;
            }

            let path = entry.path();
            if path.is_dir() {
                let size = calculate_dir_size(&path)?;
                total_size = total_size.saturating_add(size);

                // Read stamp timestamp, falling back to directory modification time
                let stamp_path = self.stamps_dir().join(&name);
                let mtime = if let Ok(meta) = stamp_path.metadata() {
                    meta.modified().unwrap_or(UNIX_EPOCH)
                } else if let Ok(meta) = path.metadata() {
                    meta.modified().unwrap_or(UNIX_EPOCH)
                } else {
                    UNIX_EPOCH
                };

                key_entries.push((name, path, size, mtime));
            }
        }

        let bytes_before = total_size;
        let mut bytes_after = total_size;
        let mut evicted_count = 0u32;

        // Sort by stamp mtime ascending (oldest stamp first)
        key_entries.sort_by_key(|(_, _, _, mtime)| *mtime);

        for (name, path, size, _) in key_entries {
            // Evict if total exceeds quota or this individual key exceeds quota
            if bytes_after > cache_max_bytes || size > cache_max_bytes {
                let _ = fs::remove_dir_all(&path);
                let _ = fs::remove_file(self.stamps_dir().join(&name));
                bytes_after = bytes_after.saturating_sub(size);
                evicted_count = evicted_count.saturating_add(1);
            }
        }

        Ok(EvictionReport {
            bytes_before,
            bytes_after,
            evicted_count,
        })
    }

    /// Configures environment variables for a worker process (spec 0047 D2, D5, D6).
    pub fn configure_worker_env(
        &self,
        env: &mut BTreeMap<String, String>,
        key: &str,
        sccache: bool,
        sccache_max_bytes: u64,
    ) {
        let target_path = self.target_dir(key);
        env.insert(
            "CARGO_TARGET_DIR".to_string(),
            target_path.to_string_lossy().to_string(),
        );

        if sccache {
            if let Some(sccache_bin) = resolve_in_path("sccache") {
                let base = self
                    .root_dir
                    .parent()
                    .and_then(|p| p.parent())
                    .map(Path::to_path_buf)
                    .unwrap_or_else(base_cache_dir);
                let sccache_dir = base.join("sccache");
                let _ = fs::create_dir_all(&sccache_dir);

                env.insert(
                    "RUSTC_WRAPPER".to_string(),
                    sccache_bin.to_string_lossy().to_string(),
                );
                env.insert(
                    "SCCACHE_DIR".to_string(),
                    sccache_dir.to_string_lossy().to_string(),
                );
                env.insert(
                    "SCCACHE_CACHE_SIZE".to_string(),
                    format!("{sccache_max_bytes}"),
                );
            } else {
                tracing::warn!(
                    "sccache enabled in PoolConfig but sccache binary not found in PATH"
                );
            }
        }

        // REQ-0047-10: Ensure no variable starting with RA_ is exported
        let keys_to_remove: Vec<String> = env
            .keys()
            .filter(|k| k.starts_with("RA_"))
            .cloned()
            .collect();
        for k in keys_to_remove {
            env.remove(&k);
        }
    }
}
