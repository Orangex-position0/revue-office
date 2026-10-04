//! Synthetic test adapter; never selected by Runtime Profile/Bootstrap.
use crate::providers::credentials::{
    CredentialError, CredentialScope, CredentialSet, CredentialStore,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Default)]
pub struct MemoryFailures {
    pub read: bool,
    pub write: bool,
    pub delete: bool,
    pub corrupt_read: bool,
}
#[derive(Default)]
pub struct MemoryCredentialStore {
    values: Mutex<HashMap<CredentialScope, CredentialSet>>,
    failures: Mutex<MemoryFailures>,
}
impl MemoryCredentialStore {
    pub fn set_failures(&self, value: MemoryFailures) {
        *self.failures.lock().expect("synthetic failures lock") = value;
    }
}
#[async_trait]
impl CredentialStore for MemoryCredentialStore {
    async fn resolve(
        &self,
        scope: &CredentialScope,
    ) -> Result<Option<CredentialSet>, CredentialError> {
        let failures = *self
            .failures
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        if failures.read {
            return Err(CredentialError::Unavailable);
        }
        let values = self
            .values
            .lock()
            .map_err(|_| CredentialError::Unavailable)?;
        Ok(values.get(scope).map(|v| {
            if failures.corrupt_read {
                CredentialSet::new([secrecy::SecretString::new("synthetic-corruption".into())])
            } else {
                v.copy_for_store()
            }
        }))
    }
    async fn replace(
        &self,
        scope: &CredentialScope,
        value: CredentialSet,
    ) -> Result<(), CredentialError> {
        if self
            .failures
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .write
        {
            return Err(CredentialError::Unavailable);
        }
        self.values
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .insert(scope.clone(), value);
        Ok(())
    }
    async fn delete(&self, scope: &CredentialScope) -> Result<(), CredentialError> {
        if self
            .failures
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .delete
        {
            return Err(CredentialError::Unavailable);
        }
        self.values
            .lock()
            .map_err(|_| CredentialError::Unavailable)?
            .remove(scope);
        Ok(())
    }
}
