use enostr::{NormRelayUrl, NoteId, Pubkey, RelayUrlSource};
use hashbrown::HashSet;
use nostrdb::{Error, Ndb, NoteReply, Transaction};

/// Thread ancestry and routing information read in one NostrDB transaction.
///
/// Missing references remain in `note_ids` so a subscription can observe their
/// later arrival. Authors named by root/reply tags are routing hints only; they
/// do not restrict the authors of events requested from relays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ThreadSnapshot {
    /// Every selected note, root, and reply parent encountered during traversal.
    pub(crate) note_ids: HashSet<NoteId>,
    /// Authors of available notes and authors claimed by their root/reply tags.
    pub(crate) authors: HashSet<Pubkey>,
    /// Referenced notes absent from this database snapshot.
    pub(crate) missing_ids: HashSet<NoteId>,
    /// Allowed observed relays and NIP-10 root/reply relay hints.
    pub(crate) relays: HashSet<NormRelayUrl>,
}

impl ThreadSnapshot {
    /// Traverse available root/reply ancestry without recursion or UI work.
    ///
    /// Each ID is visited once, including missing IDs and cyclic references.
    /// Mentions and other unrelated tags do not contribute routing information.
    #[profiling::function]
    pub(crate) fn load(ndb: &Ndb, seeds: &HashSet<NoteId>) -> Result<Self, Error> {
        let txn = Transaction::new(ndb)?;
        let mut snapshot = Self::default();
        let mut pending = seeds.iter().copied().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if !snapshot.note_ids.insert(id) {
                continue;
            }

            let note = match ndb.get_note_by_id(&txn, id.bytes()) {
                Ok(note) => note,
                Err(Error::NotFound) => {
                    snapshot.missing_ids.insert(id);
                    continue;
                }
                Err(err) => return Err(err),
            };
            snapshot.authors.insert(Pubkey::new(*note.pubkey()));
            for relay in note.relays(&txn) {
                snapshot.retain_relay(relay);
            }

            let reply = NoteReply::new(note.tags());
            for reference in [reply.root(), reply.reply()].into_iter().flatten() {
                pending.push(NoteId::new(*reference.id));
                if let Some(relay) = reference.relay {
                    snapshot.retain_relay(relay);
                }
                if let Some(author) = note
                    .tags()
                    .into_iter()
                    .nth(usize::from(reference.index))
                    .and_then(|tag| tag.get_id(4))
                {
                    snapshot.authors.insert(Pubkey::new(*author));
                }
            }
        }
        Ok(snapshot)
    }

    /// Keep normalized relay hints under the existing remote-advertised policy.
    fn retain_relay(&mut self, relay: &str) {
        let Ok(relay) = NormRelayUrl::new(relay) else {
            return;
        };
        if relay.allowed_for_source(RelayUrlSource::RemoteAdvertised) {
            self.relays.insert(relay);
        }
    }
}
