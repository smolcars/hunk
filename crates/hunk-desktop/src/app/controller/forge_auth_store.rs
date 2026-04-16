use std::sync::{Mutex, OnceLock};

use hunk_forge::ForgeSecretStore;

const FORGE_KEYRING_SERVICE: &str = "com.niteshbalusu.hunk.forge";

#[derive(Debug, Clone, Copy, Default)]
struct KeyringForgeSecretStore;

fn forge_secret_store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn load_forge_secret(credential_id: &str) -> anyhow::Result<Option<String>> {
    KeyringForgeSecretStore.load_secret(credential_id)
}

fn save_forge_secret(credential_id: &str, secret: &str) -> anyhow::Result<()> {
    KeyringForgeSecretStore.save_secret(credential_id, secret)
}

fn delete_forge_secret(credential_id: &str) -> anyhow::Result<()> {
    KeyringForgeSecretStore.delete_secret(credential_id)
}

impl KeyringForgeSecretStore {
    fn entry(&self, credential_id: &str) -> anyhow::Result<keyring::Entry> {
        keyring::Entry::new(FORGE_KEYRING_SERVICE, credential_id)
            .with_context(|| format!("failed to create forge keyring entry for '{credential_id}'"))
    }
}

impl ForgeSecretStore for KeyringForgeSecretStore {
    fn load_secret(&self, credential_id: &str) -> anyhow::Result<Option<String>> {
        let _guard = forge_secret_store_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = self.entry(credential_id)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(err).with_context(|| {
                format!("failed to load forge credential secret for '{credential_id}'")
            }),
        }
    }

    fn save_secret(&self, credential_id: &str, secret: &str) -> anyhow::Result<()> {
        let _guard = forge_secret_store_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = self.entry(credential_id)?;
        entry
            .set_password(secret)
            .with_context(|| format!("failed to save forge credential secret for '{credential_id}'"))
    }

    fn delete_secret(&self, credential_id: &str) -> anyhow::Result<()> {
        let _guard = forge_secret_store_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = self.entry(credential_id)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err).with_context(|| {
                format!("failed to delete forge credential secret for '{credential_id}'")
            }),
        }
    }
}
