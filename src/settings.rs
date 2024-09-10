use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use crate::macos_key_storage::MacOSKeyStorageType;

#[cfg(target_os = "linux")]
use crate::linux_key_storage::LinuxKeyStorageType;

#[derive(Serialize, Deserialize)]
pub struct NotedeckSettings {
    STORAGE_SETTINGS: StorageSettings,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            MACOS_KEY_STORAGE_TYPE: MacOSKeyStorageType::BasicFileStorage,
            #[cfg(target_os = "linux")]
            LINUX_KEY_STORAGE_TYPE: LinuxKeyStorageType::BasicFileStorage,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct StorageSettings {
    #[cfg(target_os = "macos")]
    pub MACOS_KEY_STORAGE_TYPE: MacOSKeyStorageType,

    #[cfg(target_os = "linux")]
    pub LINUX_KEY_STORAGE_TYPE: LinuxKeyStorageType,
}
