use serde::{Deserialize, Serialize};

#[cfg(target_os = "macos")]
use crate::macos_key_storage::MacOSKeyStorageType;

#[cfg(target_os = "linux")]
use crate::linux_key_storage::LinuxKeyStorageType;

#[derive(Serialize, Deserialize, Default)]
pub struct NotedeckSettings {
    pub storage_settings: StorageSettings,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            macos_key_storage_type: MacOSKeyStorageType::BasicFileStorage,
            #[cfg(target_os = "linux")]
            linux_key_storage_type: LinuxKeyStorageType::BasicFileStorage,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
pub struct StorageSettings {
    #[cfg(target_os = "macos")]
    pub macos_key_storage_type: MacOSKeyStorageType,

    #[cfg(target_os = "linux")]
    pub LINUX_KEY_STORAGE_TYPE: LinuxKeyStorageType,
}
