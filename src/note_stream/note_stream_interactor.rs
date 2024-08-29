use std::collections::HashMap;

use enostr::Filter;

use crate::note::NoteRef;

use super::misc::{HashableFilter, NoteStreamInstanceId};

/// user interacts with NoteStreamIteractor, not NoteStreamManager
#[derive(Default)]
pub struct NoteStreamInteractor {
    pub(crate) commands: Vec<NoteStreamCommand>,
    pub(crate) cache: HashMap<NoteStreamInstanceId, Vec<NoteRef>>,
}

impl NoteStreamInteractor {
    pub fn begin_searching(&mut self, filters: Vec<Filter>) -> NoteStreamInstanceId {
        let id = NoteStreamInstanceId::default();
        self.commands.push(NoteStreamCommand::NewStreamInstance(
            id.clone(),
            HashableFilter::new(filters.into_iter().collect()),
        ));
        id
    }

    pub fn resume_search(&mut self, id: NoteStreamInstanceId) {
        self.commands
            .push(NoteStreamCommand::ResumeStreamInstance(id.clone()));
    }

    pub fn pause_searching(&mut self, id: &NoteStreamInstanceId) {
        self.commands
            .push(NoteStreamCommand::PauseStreamInstance(id.clone()));
    }

    pub fn take_unseen(&mut self, id: &NoteStreamInstanceId) -> Option<Vec<NoteRef>> {
        self.cache.remove(id)
    }
}

pub enum NoteStreamCommand {
    NewStreamInstance(NoteStreamInstanceId, HashableFilter),
    PauseStreamInstance(NoteStreamInstanceId),
    ResumeStreamInstance(NoteStreamInstanceId),
}
