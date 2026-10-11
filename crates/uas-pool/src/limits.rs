//! Pure limits core: the ruled memory, pids and disk defaults and their override order.
//!
//! Spec 0041 D1, D2, D5, D7. Nothing here writes a cgroup file; the freezer
//! applies the resolved values and the pool calls [`resolve`] once at start.

use std::fmt;
use std::io;
use std::path::Path;

use crate::error::PoolError;

const GIB: u64 = 1024 * 1024 * 1024;

/// Smallest per-slot `memory.max` the pool accepts (spec 0041 D1). Below it is a refusal.
pub const MEMORY_FLOOR_BYTES: u64 = 2 * GIB;
/// Default minimum free bytes on the worktree filesystem (spec 0041 D5).
pub const DEFAULT_MIN_FREE_BYTES: u64 = 20 * GIB;
/// Default `pids.max` (spec 0041 D2).
pub const DEFAULT_PIDS_MAX: u32 = 4096;
/// `memory.swap.max` for every slot: no swap (spec 0041 D1).
pub const MEMORY_SWAP_MAX_BYTES: u64 = 0;

/// Environment twin of the memory limit (spec 0041 D7).
pub const ENV_MEMORY_MAX: &str = "UAS_SLOT_MEMORY_MAX";
/// Environment twin of the pids limit (spec 0041 D7).
pub const ENV_PIDS_MAX: &str = "UAS_SLOT_PIDS_MAX";
/// Environment twin of the disk floor (spec 0041 D7).
pub const ENV_MIN_FREE_BYTES: &str = "UAS_MIN_FREE_BYTES";

/// Where the memory limit in force came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitSource {
    /// The `MemTotal` formula.
    Formula,
    /// An environment variable.
    Env,
    /// A `PoolConfig` value (a spec 0038 `spec.limits` value arrives here).
    Config,
}

impl fmt::Display for LimitSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Formula => "formula",
            Self::Env => "env",
            Self::Config => "config",
        })
    }
}

/// Explicit overrides from `PoolConfig`; every field `None` means "use the default".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LimitOverrides {
    /// `memory.max` per slot.
    pub memory_max_bytes: Option<u64>,
    /// `memory.high` per slot.
    pub memory_high_bytes: Option<u64>,
    /// `pids.max` per slot.
    pub pids_max: Option<u32>,
    /// Minimum free bytes on the worktree filesystem.
    pub min_free_bytes: Option<u64>,
}

/// The limits in force after precedence is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLimits {
    /// `memory.max`; `None` when `/proc/meminfo` was unreadable and nothing overrode it.
    pub memory_max_bytes: Option<u64>,
    /// `memory.high`; `None` exactly when `memory_max_bytes` is.
    pub memory_high_bytes: Option<u64>,
    /// `memory.swap.max`, always 0.
    pub memory_swap_max_bytes: u64,
    /// `pids.max`.
    pub pids_max: u32,
    /// Minimum free bytes on the worktree filesystem.
    pub min_free_bytes: u64,
    /// Source of `memory_max_bytes`; `None` when no memory limit is set.
    pub memory_source: Option<LimitSource>,
}

/// Parse the `MemTotal:` line of `/proc/meminfo` into bytes.
pub fn parse_mem_total(meminfo: &str) -> Option<u64> {
    let rest = meminfo.lines().find_map(|l| l.strip_prefix("MemTotal:"))?;
    let kb: u64 = rest.trim().strip_suffix("kB")?.trim().parse().ok()?;
    kb.checked_mul(1024)
}

/// Read `MemTotal` from `/proc/meminfo`; `None` when it cannot be read or parsed.
pub fn read_mem_total() -> Option<u64> {
    parse_mem_total(&std::fs::read_to_string("/proc/meminfo").ok()?)
}

/// Per-slot `memory.max`: `MemTotal` divided by twice the host slot count. No ceiling, no floor.
pub fn memory_max_from_formula(mem_total: u64, host_max_slots: usize) -> u64 {
    let slots = u64::try_from(host_max_slots.max(1)).unwrap_or(u64::MAX);
    mem_total / slots.saturating_mul(2)
}

fn env_u64(env: &dyn Fn(&str) -> Option<String>, key: &str) -> Option<u64> {
    let raw = env(key)?;
    match raw.trim().parse::<u64>() {
        Ok(v) => Some(v),
        Err(_) => {
            tracing::warn!(variable = key, "ignoring non-decimal limit override");
            None
        }
    }
}

/// Apply precedence (config, then env, then formula) and the floor.
///
/// Returns [`PoolError::MemoryLow`] when the memory limit that wins is below
/// [`MEMORY_FLOOR_BYTES`]; it never clamps.
pub fn resolve(
    cfg: &LimitOverrides,
    mem_total: Option<u64>,
    host_max_slots: usize,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<ResolvedLimits, PoolError> {
    let memory = if let Some(v) = cfg.memory_max_bytes {
        Some((v, LimitSource::Config))
    } else if let Some(v) = env_u64(env, ENV_MEMORY_MAX) {
        Some((v, LimitSource::Env))
    } else {
        mem_total.map(|t| {
            (
                memory_max_from_formula(t, host_max_slots),
                LimitSource::Formula,
            )
        })
    };
    if let Some((max, _)) = memory {
        if max < MEMORY_FLOOR_BYTES {
            return Err(PoolError::MemoryLow {
                per_slot_bytes: max,
                floor_bytes: MEMORY_FLOOR_BYTES,
            });
        }
    }
    let memory_high_bytes = memory.map(|(max, _)| match cfg.memory_high_bytes {
        Some(high) if high > max => {
            tracing::warn!(
                memory_high_bytes = high,
                memory_max_bytes = max,
                "memory_high_bytes above memory.max; clamping high to max"
            );
            max
        }
        Some(high) => high,
        None => (u128::from(max) * 4 / 5) as u64,
    });
    let pids_max = cfg.pids_max.unwrap_or_else(|| {
        env_u64(env, ENV_PIDS_MAX)
            .and_then(|v| {
                u32::try_from(v)
                    .map_err(|_| {
                        tracing::warn!(
                            variable = ENV_PIDS_MAX,
                            "ignoring limit override above u32::MAX"
                        );
                    })
                    .ok()
            })
            .unwrap_or(DEFAULT_PIDS_MAX)
    });
    let min_free_bytes = cfg
        .min_free_bytes
        .or_else(|| env_u64(env, ENV_MIN_FREE_BYTES))
        .unwrap_or(DEFAULT_MIN_FREE_BYTES);
    Ok(ResolvedLimits {
        memory_max_bytes: memory.map(|(m, _)| m),
        memory_high_bytes,
        memory_swap_max_bytes: MEMORY_SWAP_MAX_BYTES,
        pids_max,
        min_free_bytes,
        memory_source: memory.map(|(_, s)| s),
    })
}

/// Log the limits in force once at pool start (spec 0041 REQ-0041-12).
pub fn log_start(limits: &ResolvedLimits) {
    match (limits.memory_max_bytes, limits.memory_source) {
        (Some(max), Some(source)) => {
            tracing::info!(memory_max_bytes = max, memory_source = %source, "slot limits");
        }
        _ => tracing::warn!("slot limits: no memory limit set, MemTotal unreadable"),
    }
}

/// Refuse when the filesystem has less free space than the floor.
pub fn check_free(free_bytes: u64, min_free_bytes: u64) -> Result<(), PoolError> {
    if free_bytes < min_free_bytes {
        return Err(PoolError::DiskLow {
            free_bytes,
            min_free_bytes,
        });
    }
    Ok(())
}

/// Free bytes available to unprivileged users on the filesystem holding `path`.
pub fn free_bytes_at(path: &Path) -> io::Result<u64> {
    let st = rustix::fs::statvfs(path)?;
    Ok(st.f_bavail.saturating_mul(st.f_frsize))
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: u64 = GIB;
    fn no_env(_: &str) -> Option<String> {
        None
    }
    fn mem_max(r: Result<ResolvedLimits, PoolError>) -> u64 {
        r.unwrap().memory_max_bytes.unwrap()
    }

    #[test]
    fn limits_formula_fixtures() {
        let c = LimitOverrides::default();
        assert!(resolve(&c, Some(6 * G), 2, &no_env).is_err());
        let r = resolve(&c, Some(54 * G), 2, &no_env).unwrap();
        assert_eq!(r.memory_max_bytes, Some(13 * G + G / 2));
        assert_eq!(r.memory_high_bytes, Some(11_596_411_699));
        assert_eq!(r.memory_swap_max_bytes, 0);
        assert_eq!(r.pids_max, 4096);
        let r = resolve(&c, Some(256 * G), 2, &no_env).unwrap();
        assert_eq!(r.memory_max_bytes, Some(64 * G));
        assert_eq!(r.memory_high_bytes, Some(54_975_581_388));
        assert_eq!(mem_max(resolve(&c, Some(54 * G), 3, &no_env)), 9 * G);
    }

    #[test]
    fn limits_below_floor_refuses_and_does_not_clamp() {
        let e = resolve(&LimitOverrides::default(), Some(6 * G), 2, &no_env).unwrap_err();
        assert!(matches!(
            e,
            PoolError::MemoryLow { per_slot_bytes, floor_bytes }
                if per_slot_bytes == G + G / 2 && floor_bytes == 2 * G
        ));
    }

    #[test]
    fn limits_override_below_floor_refuses() {
        let cfg = LimitOverrides {
            memory_max_bytes: Some(G),
            ..Default::default()
        };
        assert!(matches!(
            resolve(&cfg, Some(54 * G), 2, &no_env),
            Err(PoolError::MemoryLow { per_slot_bytes, .. }) if per_slot_bytes == G
        ));
        let env = |k: &str| (k == ENV_MEMORY_MAX).then(|| G.to_string());
        assert!(matches!(
            resolve(&LimitOverrides::default(), Some(54 * G), 2, &env),
            Err(PoolError::MemoryLow { per_slot_bytes, .. }) if per_slot_bytes == G
        ));
    }

    #[test]
    fn limits_source_is_formula_env_config_in_turn() {
        let env = |k: &str| (k == ENV_MEMORY_MAX).then(|| (4 * G).to_string());
        let cfg = LimitOverrides {
            memory_max_bytes: Some(8 * G),
            ..Default::default()
        };
        let none = LimitOverrides::default();
        let f = resolve(&none, Some(54 * G), 2, &no_env).unwrap();
        assert_eq!(f.memory_source, Some(LimitSource::Formula));
        let e = resolve(&none, Some(54 * G), 2, &env).unwrap();
        assert_eq!(
            (e.memory_max_bytes, e.memory_source),
            (Some(4 * G), Some(LimitSource::Env))
        );
        let c = resolve(&cfg, Some(54 * G), 2, &env).unwrap();
        assert_eq!(
            (c.memory_max_bytes, c.memory_source),
            (Some(8 * G), Some(LimitSource::Config))
        );
        assert_eq!(c.memory_high_bytes, Some(8 * G * 4 / 5));
        assert_eq!(LimitSource::Env.to_string(), "env");
    }

    #[test]
    fn limits_unreadable_meminfo_sets_no_memory_limit() {
        let r = resolve(&LimitOverrides::default(), None, 2, &no_env).unwrap();
        assert_eq!(
            (r.memory_max_bytes, r.memory_high_bytes, r.memory_source),
            (None, None, None)
        );
    }

    #[test]
    fn limits_pids_and_disk_env_and_config() {
        let env = |k: &str| match k {
            ENV_PIDS_MAX => Some("64".into()),
            ENV_MIN_FREE_BYTES => Some("1000".into()),
            _ => None,
        };
        let r = resolve(&LimitOverrides::default(), Some(54 * G), 2, &env).unwrap();
        assert_eq!((r.pids_max, r.min_free_bytes), (64, 1000));
        let cfg = LimitOverrides {
            pids_max: Some(8),
            min_free_bytes: Some(5),
            ..Default::default()
        };
        let r = resolve(&cfg, Some(54 * G), 2, &env).unwrap();
        assert_eq!((r.pids_max, r.min_free_bytes), (8, 5));
        let r = resolve(&LimitOverrides::default(), Some(54 * G), 2, &no_env).unwrap();
        assert_eq!(r.min_free_bytes, 20 * G);
    }

    #[test]
    fn limits_pids_env_above_u32_falls_back_to_default() {
        let env = |k: &str| (k == ENV_PIDS_MAX).then(|| "4294967296".to_string());
        let r = resolve(&LimitOverrides::default(), Some(54 * G), 2, &env).unwrap();
        assert_eq!(r.pids_max, DEFAULT_PIDS_MAX);
    }

    #[test]
    fn limits_config_high_above_max_is_clamped() {
        let cfg = LimitOverrides {
            memory_max_bytes: Some(4 * G),
            memory_high_bytes: Some(8 * G),
            ..Default::default()
        };
        let r = resolve(&cfg, Some(54 * G), 2, &no_env).unwrap();
        assert_eq!(r.memory_high_bytes, Some(4 * G));
        let cfg = LimitOverrides {
            memory_max_bytes: Some(4 * G),
            memory_high_bytes: Some(3 * G),
            ..Default::default()
        };
        let r = resolve(&cfg, Some(54 * G), 2, &no_env).unwrap();
        assert_eq!(r.memory_high_bytes, Some(3 * G));
    }

    #[test]
    fn limits_disk_floor_refuses_one_gib_free() {
        assert!(matches!(
            check_free(G, 20 * G),
            Err(PoolError::DiskLow { .. })
        ));
        assert!(check_free(20 * G, 20 * G).is_ok());
    }

    #[test]
    fn limits_meminfo_parses_kb_line() {
        let s = "MemTotal:       56623104 kB\nMemFree:  1 kB\n";
        assert_eq!(parse_mem_total(s), Some(56_623_104 * 1024));
        assert_eq!(parse_mem_total("MemFree: 1 kB\n"), None);
        assert_eq!(parse_mem_total("MemTotal: lots kB\n"), None);
    }

    #[test]
    fn limits_statvfs_reads_free_bytes() {
        assert!(free_bytes_at(Path::new(".")).unwrap() > 0);
    }
}
