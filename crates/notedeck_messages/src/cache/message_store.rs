use hashbrown::HashSet;
use nostrdb::NoteKey;
use notedeck::NoteRef;

/// Maintains a strictly ordered list of message references for a single
/// conversation. It mirrors the lightweight ordering guarantees that
/// `TimelineCache` and `Threads` rely on so UI code can assume the
/// backing data is already sorted from newest to oldest.
#[derive(Default)]
pub struct MessageStore {
    pub messages_ordered: Vec<NoteRef>,
    seen: HashSet<NoteKey>,
}

impl MessageStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a new `NoteRef` while keeping the store sorted. Returns
    /// `true` when the reference was new to the conversation.
    pub fn insert(&mut self, note: NoteRef) -> bool {
        if !self.seen.insert(note.key) {
            return false;
        }

        match self.messages_ordered.binary_search(&note) {
            Ok(_) => {
                debug_assert!(
                    false,
                    "MessageStore::insert was asked to insert a duplicate NoteRef"
                );
                false
            }
            Err(idx) => {
                self.messages_ordered.insert(idx, note);
                true
            }
        }
    }

    /// Bulk insert helper used when hydrating from nostrdb queries.
    pub fn extend<I>(&mut self, notes: I) -> Vec<NoteRef>
    where
        I: IntoIterator<Item = NoteRef>,
    {
        let mut inserted = Vec::new();
        for note in notes {
            if self.insert(note) {
                inserted.push(note);
            }
        }
        inserted
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &NoteRef> {
        self.messages_ordered.iter()
    }

    pub fn as_slice(&self) -> &[NoteRef] {
        &self.messages_ordered
    }

    pub fn is_empty(&self) -> bool {
        self.messages_ordered.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages_ordered.len()
    }

    pub fn latest(&self) -> Option<&NoteRef> {
        self.messages_ordered.first()
    }

    pub fn newest_timestamp(&self) -> Option<u64> {
        self.latest().map(|n| n.created_at)
    }
}
