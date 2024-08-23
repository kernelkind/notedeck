use crate::note::NoteRef;
use crate::timeline::{TimelineTab, ViewFilter};
use crate::Error;
use nostrdb::{Filter, FilterBuilder, Ndb, Subscription, Transaction};
use std::cmp::Ordering;
use std::collections::HashMap;
use tracing::{debug, warn};

#[derive(Default)]
pub struct Thread {
    pub view: TimelineTab,
    sub: Option<Subscription>,
    remote_sub: Option<String>,
    pub subscribers: i32,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum DecrementResult {
    LastSubscriber(u64),
    ActiveSubscribers,
}

impl Thread {
    pub fn new(notes: Vec<NoteRef>) -> Self {
        let mut cap = ((notes.len() as f32) * 1.5) as usize;
        if cap == 0 {
            cap = 25;
        }
        let mut view = TimelineTab::new_with_capacity(ViewFilter::NotesAndReplies, cap);
        view.notes = notes;
        let sub: Option<Subscription> = None;
        let remote_sub: Option<String> = None;
        let subscribers: i32 = 0;

        Thread {
            view,
            sub,
            remote_sub,
            subscribers,
        }
    }

    /// Look for new thread notes since our last fetch
    pub fn new_notes(
        notes: &[NoteRef],
        root_id: &[u8; 32],
        txn: &Transaction,
        ndb: &Ndb,
    ) -> Vec<NoteRef> {
        if notes.is_empty() {
            return vec![];
        }

        let last_note = notes[0];
        let filters = Thread::filters_since(root_id, last_note.created_at + 1);

        if let Ok(results) = ndb.query(txn, filters, 1000) {
            debug!("got {} results from thread update", results.len());
            results
                .into_iter()
                .map(NoteRef::from_query_result)
                .collect()
        } else {
            debug!("got no results from thread update",);
            vec![]
        }
    }

    pub fn decrement_sub(&mut self) -> Result<DecrementResult, Error> {
        self.subscribers -= 1;

        match self.subscribers.cmp(&0) {
            Ordering::Equal => {
                if let Some(sub) = self.subscription() {
                    Ok(DecrementResult::LastSubscriber(sub.id))
                } else {
                    Err(Error::no_active_sub())
                }
            }
            Ordering::Less => Err(Error::unexpected_sub_count(self.subscribers)),
            Ordering::Greater => Ok(DecrementResult::ActiveSubscribers),
        }
    }

    pub fn subscription(&self) -> Option<&Subscription> {
        self.sub.as_ref()
    }

    pub fn remote_subscription(&self) -> &Option<String> {
        &self.remote_sub
    }

    pub fn remote_subscription_mut(&mut self) -> &mut Option<String> {
        &mut self.remote_sub
    }

    pub fn subscription_mut(&mut self) -> &mut Option<Subscription> {
        &mut self.sub
    }

    fn filters_raw(root: &[u8; 32]) -> Vec<FilterBuilder> {
        vec![
            nostrdb::Filter::new().kinds([1]).event(root),
            nostrdb::Filter::new().ids([root]).limit(1),
        ]
    }

    pub fn filters_since(root: &[u8; 32], since: u64) -> Vec<Filter> {
        Self::filters_raw(root)
            .into_iter()
            .map(|fb| fb.since(since).build())
            .collect()
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
    pub root_id_to_thread: HashMap<[u8; 32], Thread>,
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

    /// Get a thread by root id, if it doesn't exist, query ndb for the thread's notes & add the new thread to the cache
    pub fn thread_mut<'a>(
        &'a mut self,
        ndb: &Ndb,
        txn: &Transaction,
        root_id: &[u8; 32],
    ) -> ThreadResult<'a> {
        // we can't use the naive hashmap entry API here because lookups
        // require a copy, wait until we have a raw entry api. We could
        // also use hashbrown?

        if self.root_id_to_thread.contains_key(root_id) {
            return ThreadResult::Stale(self.root_id_to_thread.get_mut(root_id).unwrap());
        }

        // we don't have the thread, query for it!
        let filters = Thread::filters(root_id);

        let notes = if let Ok(results) = ndb.query(txn, filters, 1000) {
            results
                .into_iter()
                .map(NoteRef::from_query_result)
                .collect()
        } else {
            debug!(
                "got no results from thread lookup for {}",
                hex::encode(root_id)
            );
            vec![]
        };

        if notes.is_empty() {
            warn!("thread query returned 0 notes? ")
        } else {
            debug!("found thread with {} notes", notes.len());
        }

        self.root_id_to_thread
            .insert(root_id.to_owned(), Thread::new(notes));
        ThreadResult::Fresh(self.root_id_to_thread.get_mut(root_id).unwrap())
    }

    //fn thread_by_id(&self, ndb: &Ndb, id: &[u8; 32]) -> &mut Thread {
    //}
}
