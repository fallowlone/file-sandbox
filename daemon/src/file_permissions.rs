//! Port of `src/file-permissions.ts`.
//!
//! Best-effort `chmod`: like the TS version it swallows failures (the original
//! logs the error and continues) so a permission tweak never aborts the
//! pipeline.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Change the permission bits of `file_path` to `mode` (octal, e.g. `0o600`).
/// Failures are logged and ignored, matching the TS behavior.
pub async fn change_permissions(file_path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        let perms = std::fs::Permissions::from_mode(mode);
        if let Err(e) = tokio::fs::set_permissions(file_path, perms).await {
            eprintln!("chmod {} failed: {e}", file_path.display());
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (file_path, mode);
    }
}
