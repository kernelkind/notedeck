use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use crate::key_storage::MacOSKeyStorageType;

#[cfg(target_os = "linux")]
use crate::key_storage::LinuxKeyStorageType;

#[derive(Serialize, Deserialize)]
pub struct NotedeckSettings {
    storage_settings: StorageSettings,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            macos_key_storage_type: MacOSKeyStorageType::BasicFileStorage,
            #[cfg(target_os = "linux")]
            LINUX_KEY_STORAGE_TYPE: LinuxKeyStorageType::BasicFileStorage,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct StorageSettings {
    #[cfg(target_os = "macos")]
    pub macos_key_storage_type: MacOSKeyStorageType,

    #[cfg(target_os = "linux")]
    pub linux_key_storage_type: LinuxKeyStorageType,
}
