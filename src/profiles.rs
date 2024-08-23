use enostr::Pubkey;
use indexmap::IndexMap;

use crate::profile_state::ProfileState;

pub struct Profiles {
    profiles: IndexMap<Pubkey, ProfileState>,
}

impl Default for Profiles {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiles {
    pub fn new() -> Self {
        Self {
            profiles: IndexMap::new(),
        }
    }

    pub fn get_profile_state(&self, key: &Pubkey) -> Option<&ProfileState> {
        self.profiles.get(key)
    }

    pub fn get_profile_state_mut(&mut self, key: &Pubkey) -> Option<&mut ProfileState> {
        self.profiles.get_mut(key)
    }

    pub fn create_profile_state_if_nonexistent(&mut self, key: &Pubkey) {
        self.profiles.entry(*key).or_insert(ProfileState::new(key));
    }

    pub fn num_profiles(&self) -> usize {
        self.profiles.len()
    }

    pub fn get_profile_at_index(&self, index: usize) -> Option<(&Pubkey, &ProfileState)> {
        self.profiles.get_index(index)
    }

    pub fn get_profile_at_index_mut(
        &mut self,
        index: usize,
    ) -> Option<(&Pubkey, &mut ProfileState)> {
        self.profiles.get_index_mut(index)
    }

    pub fn get_states(&self) -> indexmap::map::Values<Pubkey, ProfileState> {
        self.profiles.values()
    }

    pub fn get_pubkeys(&self) -> indexmap::map::Keys<Pubkey, ProfileState> {
        self.profiles.keys()
    }

    pub fn get_iterator(&self) -> indexmap::map::Iter<Pubkey, ProfileState> {
        self.profiles.iter()
    }
}
