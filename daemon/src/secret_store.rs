//! Secret storage abstraction (Task A1).
//!
//! Secrets (`vtApiKey`, `apiToken`) currently live as plaintext in `config.json`
//! and are readable by any process with the same UID. This module introduces a
//! backend-agnostic [`SecretStore`] so those secrets can move into the macOS
//! Keychain instead.
//!
//! - [`KeychainStore`] (macOS) — OS-managed generic-password items under the
//!   `dev.artemmac.filesandbox` service.
//! - [`MemoryStore`] — in-process map; the portable fallback on non-macOS and
//!   the test double for the contract tests below.
//!
//! Account names are the camelCase config field names (`vtApiKey`, `apiToken`).

use anyhow::Result;

/// Keychain service identifier shared by every secret this app stores.
pub const SERVICE: &str = "dev.artemmac.filesandbox";

/// A named secret store. Implementations must be safe to share across threads.
pub trait SecretStore: Send + Sync {
    /// Return the secret for `account`, or `None` when it is absent.
    fn get(&self, account: &str) -> Result<Option<String>>;
    /// Store (or overwrite) the secret for `account`.
    fn set(&self, account: &str, value: &str) -> Result<()>;
    /// Remove the secret for `account`. Absent secrets are not an error.
    fn delete(&self, account: &str) -> Result<()>;
}

/// In-process [`SecretStore`]. Used as the portable fallback (non-macOS) and as
/// the test double. Not persistent.
#[derive(Default)]
pub struct MemoryStore {
    inner: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.inner.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .insert(account.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(account);
        Ok(())
    }
}

/// macOS Keychain-backed [`SecretStore`] (generic password items).
#[cfg(target_os = "macos")]
pub struct KeychainStore {
    service: String,
}

#[cfg(target_os = "macos")]
impl KeychainStore {
    pub fn new() -> Self {
        Self {
            service: SERVICE.to_string(),
        }
    }

    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

#[cfg(target_os = "macos")]
impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl SecretStore for KeychainStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        use security_framework::passwords::get_generic_password;
        // errSecItemNotFound — the only "absent, not an error" outcome.
        const ITEM_NOT_FOUND: i32 = -25300;
        match get_generic_password(&self.service, account) {
            Ok(bytes) => Ok(Some(String::from_utf8(bytes)?)),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(anyhow::anyhow!("keychain get {account}: {e}")),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<()> {
        use security_framework::passwords::set_generic_password_options;
        use security_framework::passwords_options::PasswordOptions;
        // Mark the item non-synchronizable so it can never propagate to iCloud
        // Keychain — the half of "device-only" the no-entitlement (unsigned) path
        // can actually enforce, matching the Swift writer's kSecAttrSynchronizable
        // = false. A true ThisDeviceOnly protection class requires the
        // data-protection keychain, which needs a signed app with a
        // keychain-access-group entitlement; on the legacy file-based login
        // keychain used here, items are already non-synced and not plaintext in
        // backups, so this is the meaningful, signing-free guarantee.
        let mut options = PasswordOptions::new_generic_password(&self.service, account);
        options.set_access_synchronized(Some(false));
        set_generic_password_options(value.as_bytes(), options)
            .map_err(|e| anyhow::anyhow!("keychain set {account}: {e}"))
    }

    fn delete(&self, account: &str) -> Result<()> {
        use security_framework::passwords::delete_generic_password;
        const ITEM_NOT_FOUND: i32 = -25300;
        match delete_generic_password(&self.service, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(anyhow::anyhow!("keychain delete {account}: {e}")),
        }
    }
}

// ── migration planning (pure, no I/O) ───────────────────────────────────────

/// Config account name for the VirusTotal API key.
pub const ACCOUNT_VT_API_KEY: &str = "vtApiKey";
/// Config account name for the local HTTP API token.
pub const ACCOUNT_API_TOKEN: &str = "apiToken";

/// Current state of one secret across both stores, as seen at startup.
#[derive(Debug, Clone, Copy)]
pub struct SecretState<'a> {
    /// camelCase config field name (also the Keychain account).
    pub account: &'a str,
    /// Plaintext value read from `config.json`, if any.
    pub file_value: Option<&'a str>,
    /// Whether the secret already exists in the [`SecretStore`].
    pub in_store: bool,
}

/// The actions required to converge secrets into the [`SecretStore`] and strip
/// their plaintext copies from `config.json`. Computed without touching disk or
/// the Keychain so it is fully unit-testable; the caller executes it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SecretMigration {
    /// Secrets to write into the store: `(account, value)`.
    pub writes: Vec<(String, String)>,
    /// Accounts whose plaintext copy must be cleared from `config.json`.
    pub blank_file_fields: Vec<String>,
}

impl SecretMigration {
    /// True when nothing needs to change.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.blank_file_fields.is_empty()
    }
}

/// Decide how to migrate each secret. The store is the source of truth: a value
/// already present there is never overwritten, only its stale plaintext copy is
/// scheduled for removal. A plaintext-only value is scheduled to be written to
/// the store and then blanked from the file. An empty/absent file value with no
/// stored secret is a no-op.
pub fn plan_secret_migration(secrets: &[SecretState]) -> SecretMigration {
    let mut plan = SecretMigration::default();
    for s in secrets {
        let file_value = s.file_value.filter(|v| !v.is_empty());
        match (s.in_store, file_value) {
            // Already in the store — drop any stale plaintext still in the file.
            (true, Some(_)) => plan.blank_file_fields.push(s.account.to_string()),
            (true, None) => {}
            // Plaintext only — migrate into the store, then blank the file.
            (false, Some(v)) => {
                plan.writes.push((s.account.to_string(), v.to_string()));
                plan.blank_file_fields.push(s.account.to_string());
            }
            // Nothing anywhere.
            (false, None) => {}
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(store: &dyn SecretStore, account: &str) {
        // Absent → None.
        assert_eq!(store.get(account).unwrap(), None);
        // Set then get.
        store.set(account, "s3cret").unwrap();
        assert_eq!(store.get(account).unwrap().as_deref(), Some("s3cret"));
        // Overwrite.
        store.set(account, "rotated").unwrap();
        assert_eq!(store.get(account).unwrap().as_deref(), Some("rotated"));
        // Delete → None, and deleting again is not an error.
        store.delete(account).unwrap();
        assert_eq!(store.get(account).unwrap(), None);
        store.delete(account).unwrap();
    }

    #[test]
    fn memory_store_satisfies_contract() {
        let store = MemoryStore::new();
        contract(&store, "vtApiKey");
    }

    #[test]
    fn memory_store_isolates_accounts() {
        let store = MemoryStore::new();
        store.set("vtApiKey", "a").unwrap();
        store.set("apiToken", "b").unwrap();
        assert_eq!(store.get("vtApiKey").unwrap().as_deref(), Some("a"));
        assert_eq!(store.get("apiToken").unwrap().as_deref(), Some("b"));
        store.delete("vtApiKey").unwrap();
        assert_eq!(store.get("vtApiKey").unwrap(), None);
        assert_eq!(store.get("apiToken").unwrap().as_deref(), Some("b"));
    }

    fn state<'a>(account: &'a str, file_value: Option<&'a str>, in_store: bool) -> SecretState<'a> {
        SecretState {
            account,
            file_value,
            in_store,
        }
    }

    #[test]
    fn migrates_plaintext_only_secret() {
        let plan = plan_secret_migration(&[state(ACCOUNT_VT_API_KEY, Some("vt-key"), false)]);
        assert_eq!(
            plan.writes,
            vec![(ACCOUNT_VT_API_KEY.to_string(), "vt-key".to_string())]
        );
        assert_eq!(plan.blank_file_fields, vec![ACCOUNT_VT_API_KEY.to_string()]);
    }

    #[test]
    fn blanks_stale_plaintext_when_already_in_store() {
        // Store wins: never overwrite it, but strip the leftover plaintext.
        let plan = plan_secret_migration(&[state(ACCOUNT_API_TOKEN, Some("stale"), true)]);
        assert!(plan.writes.is_empty());
        assert_eq!(plan.blank_file_fields, vec![ACCOUNT_API_TOKEN.to_string()]);
    }

    #[test]
    fn noop_when_only_in_store() {
        let plan = plan_secret_migration(&[state(ACCOUNT_API_TOKEN, None, true)]);
        assert!(plan.is_empty());
    }

    #[test]
    fn noop_when_nothing_anywhere() {
        let plan = plan_secret_migration(&[state(ACCOUNT_VT_API_KEY, None, false)]);
        assert!(plan.is_empty());
    }

    #[test]
    fn empty_string_file_value_is_treated_as_absent() {
        let plan = plan_secret_migration(&[state(ACCOUNT_VT_API_KEY, Some(""), false)]);
        assert!(plan.is_empty());
    }

    #[test]
    fn plans_multiple_secrets_independently() {
        let plan = plan_secret_migration(&[
            state(ACCOUNT_VT_API_KEY, Some("vt-key"), false), // migrate
            state(ACCOUNT_API_TOKEN, Some("stale"), true),    // blank only
        ]);
        assert_eq!(
            plan.writes,
            vec![(ACCOUNT_VT_API_KEY.to_string(), "vt-key".to_string())]
        );
        assert_eq!(
            plan.blank_file_fields,
            vec![
                ACCOUNT_VT_API_KEY.to_string(),
                ACCOUNT_API_TOKEN.to_string()
            ]
        );
    }

    /// Hits the real login Keychain — run explicitly with
    /// `cargo test --lib -- --ignored keychain_store_satisfies_contract`.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn keychain_store_satisfies_contract() {
        let store = KeychainStore::with_service("dev.artemmac.filesandbox.test");
        contract(&store, "vtApiKey");
    }
}
