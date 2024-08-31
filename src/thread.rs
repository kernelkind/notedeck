use crate::actionbar::{BarResult, NewThreadNotes};
use crate::note::NoteRef;
use crate::note_stream::misc::NoteStreamInstanceId;
use crate::note_stream::note_stream_interactor::NoteStreamInteractor;
use crate::timeline::{TimelineTab, ViewFilter};
use crate::Error;
use enostr::NoteId;
use nostrdb::{Filter, FilterBuilder, Ndb, Subscription, Transaction};
use std::cmp::Ordering;
use std::collections::HashMap;
use tracing::{debug, info, warn};

#[derive(Default)]
pub struct Thread {
    pub view: TimelineTab,
    pub note_stream_id: NoteStreamInstanceId,
}

impl Thread {
    pub fn new(note_stream_id: NoteStreamInstanceId) -> Self {
        let cap = 25;
        let mut view = TimelineTab::new_with_capacity(ViewFilter::NotesAndReplies, cap);
        view.notes = Vec::new();

        Thread {
            view,
            note_stream_id,
        }
    }

    fn filters_raw(root: &[u8; 32]) -> Vec<FilterBuilder> {
        vec![
            nostrdb::Filter::new().kinds([1]).event(root),
            nostrdb::Filter::new().ids([root]).limit(1),
        ]
    }

    pub fn filters(root: &[u8; 32]) -> Vec<Filter> {
        Self::filters_raw(root)
            .into_iter()
            .map(|mut fb| fb.build())
            .collect()
    }
}

#[derive(Default)]
pub struct Threads {
    /// root id to thread
    root_id_to_thread: HashMap<[u8; 32], Thread>,
}

pub enum ThreadResult<'a> {
    Fresh(&'a mut Thread),
    Stale(&'a mut Thread),
}

impl<'a> ThreadResult<'a> {
    pub fn get_ptr(self) -> &'a mut Thread {
        match self {
            Self::Fresh(ptr) => ptr,
            Self::Stale(ptr) => ptr,
        }
    }

    pub fn is_stale(&self) -> bool {
        match self {
            Self::Fresh(_ptr) => false,
            Self::Stale(_ptr) => true,
        }
    }
}

impl Threads {
    pub fn thread_expected_mut(&mut self, root_id: &[u8; 32]) -> &mut Thread {
        self.root_id_to_thread
            .get_mut(root_id)
            .expect("thread_expected_mut used but there was no thread")
    }

    pub fn thread_mut<'a>(
        &'a mut self,
        root_id: &[u8; 32],
        interactor: &mut NoteStreamInteractor,
    ) -> ThreadResult<'a> {
        // we can't use the naive hashmap entry API here because lookups
        // require a copy, wait until we have a raw entry api. We could
        // also use hashbrown?

        // thread already exists
        if self.root_id_to_thread.contains_key(root_id) {
            ThreadResult::Stale(self.get_thread_mut(root_id).unwrap())
        } else {
            // we don't have the thread, query for it!
            let filters = Thread::filters(root_id);
            let id = interactor.begin_searching(filters);

            self.root_id_to_thread
                .insert(root_id.to_owned(), Thread::new(id));
            ThreadResult::Fresh(self.get_thread_mut(root_id).unwrap())
        }
    }

    pub fn get_thread(&self, root_id: &[u8; 32]) -> Option<&Thread> {
        self.root_id_to_thread.get(root_id)
    }

    pub fn get_thread_mut(&mut self, root_id: &[u8; 32]) -> Option<&mut Thread> {
        self.root_id_to_thread.get_mut(root_id)
    }
}
