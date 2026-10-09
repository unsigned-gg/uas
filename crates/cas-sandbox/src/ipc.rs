//! The worker's IPC mount: where the gateway socket lives inside the workspace.
//!
//! Spec 0013. `cas-sandbox` owns only the place. The proxy that listens there is
//! `cas-router`, which sits below this crate in the layering.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::error::SandboxError;

/// The directory under the workspace that holds sockets for the worker.
pub const IPC_DIR: &str = ".ipc";
/// The gateway socket's file name inside [`IPC_DIR`].
pub const LLM_SOCKET: &str = "llm.sock";

/// `<workspace>/.ipc/llm.sock`. Pure path arithmetic; nothing is created.
pub fn socket_path(workspace: &Path) -> PathBuf {
    workspace.join(IPC_DIR).join(LLM_SOCKET)
}

/// Create `<workspace>/.ipc` with mode `0700` and return it. The worker preset
/// already grants every access beneath the workspace, so no new rule is needed.
pub fn ensure_ipc_dir(workspace: &Path) -> Result<PathBuf, SandboxError> {
    let dir = workspace.join(IPC_DIR);
    // The worker owns the workspace. A symlink in place of .ipc would send the
    // proxy's socket, and its cleanup, somewhere the worker chose.
    if let Ok(meta) = std::fs::symlink_metadata(&dir) {
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err(SandboxError::PathOpen(
                dir.clone(),
                std::io::Error::other(".ipc exists and is a symlink or not a directory"),
            ));
        }
    }
    std::fs::create_dir_all(&dir).map_err(|err| SandboxError::PathOpen(dir.clone(), err))?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|err| SandboxError::PathOpen(dir.clone(), err))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_path_is_under_ipc() {
        assert_eq!(
            socket_path(Path::new("/workspace")),
            PathBuf::from("/workspace/.ipc/llm.sock")
        );
    }

    #[test]
    fn ensure_ipc_dir_creates_a_private_directory() {
        let base = std::env::temp_dir().join(format!("cas-ipc-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("scratch");
        let dir = ensure_ipc_dir(&base).expect("ipc dir");
        let mode = std::fs::metadata(&dir)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
        // Idempotent.
        ensure_ipc_dir(&base).expect("second call");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn ensure_ipc_dir_refuses_a_symlink() {
        let base = std::env::temp_dir().join(format!("cas-ipc-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("elsewhere")).expect("scratch");
        std::os::unix::fs::symlink(base.join("elsewhere"), base.join(IPC_DIR)).expect("link");
        let err = ensure_ipc_dir(&base).expect_err("symlinked .ipc");
        assert!(matches!(err, SandboxError::PathOpen(_, _)), "{err}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
