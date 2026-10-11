// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Path canonicalisation for rules.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::SandboxError;

/// Resolve `path` to its canonical form, following every symlink.
///
/// A rule must name the real path. Landlock attaches a rule to the inode behind the
/// opened file descriptor, so a symlink target is what matters. A missing path is
/// `SandboxError::PathMissing`; any other failure is `SandboxError::PathOpen`.
pub fn canonical(path: &Path) -> Result<PathBuf, SandboxError> {
    match std::fs::canonicalize(path) {
        Ok(real) => Ok(real),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            Err(SandboxError::PathMissing(path.to_path_buf()))
        }
        Err(err) => Err(SandboxError::PathOpen(path.to_path_buf(), err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_path_is_its_own_error() {
        let err = canonical(Path::new("/definitely/not/here/uas-sandbox"))
            .err()
            .map(|e| matches!(e, SandboxError::PathMissing(_)));
        assert_eq!(err, Some(true));
    }

    #[test]
    fn root_is_canonical() {
        assert_eq!(canonical(Path::new("/")).ok(), Some(PathBuf::from("/")));
    }
}
