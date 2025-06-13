use egui_virtual_list::VirtualList;
use enostr::{NoteId, RelayPool};
use hashbrown::{hash_map::RawEntryMut, HashMap};
use nostrdb::{Filter, Ndb, Note, NoteReplyBuf, Transaction};
use notedeck::{NoteCache, NoteRef, UnknownIds};

use crate::{
    actionbar::{process_thread_notes, NewThreadNotes},
    multi_subscriber::{MultiSubscriber2, SubscriberId},
    timeline::MergeKind,
};

use super::ThreadSelection;

pub struct ThreadNode {
    pub replies: Vec<NoteRef>,
    pub replies_state: RepliesState,
    pub prev: ParentState,
    pub have_all_ancestors: bool,
    pub list: VirtualList,
}

pub enum RepliesState {
    Stale,
    Fresh,
}

#[derive(Clone)]
pub enum ParentState {
    Unknown,
    None,
    Parent(NoteId),
}

impl ThreadNode {
    pub fn new(parent: ParentState) -> Self {
        Self {
            replies: Vec::default(),
            replies_state: RepliesState::Fresh,
            prev: parent,
            have_all_ancestors: false,
            list: VirtualList::new(),
        }
    }

    pub fn as_ref(&self) -> &Self {
        self
    }

    pub fn insert_replies(&mut self, new_replies: &[NoteRef]) {
        if new_replies.is_empty() {
            return;
        }

        let num_prev_items = self.replies.len();
        let (notes, merge_kind) = crate::timeline::merge_sorted_vecs(&self.replies, &new_replies);

        self.replies = notes;

        let new_items = self.replies.len() - num_prev_items;

        // TODO: technically items could have been added inbetween
        if new_items > 0 {
            // TODO(jb55): update egui_virtual_list to support spliced inserts
            if let MergeKind::Spliced = merge_kind {
                tracing::debug!(
                    "spliced when inserting {} new notes, resetting virtual list",
                    new_replies.len()
                );
                let list = &mut self.list;
                list.reset();
            }
        }
    }
}

pub type RootNoteId = NoteId;

#[derive(Default)]
pub struct Threads {
    pub threads: HashMap<NoteId, ThreadNode>,
    pub subs: HashMap<RootNoteId, MultiSubscriber2>,
}

impl Threads {
    /// Opening a thread.
    /// Similar to [[super::cache::TimelineCache::open]]
    pub fn open(
        &mut self,
        ndb: &Ndb,
        txn: &Transaction,
        pool: &mut RelayPool,
        thread: &ThreadSelection,
    ) -> Option<NewThreadNotes> {
        let local_sub_filter = if let Some(selected) = &thread.selected_note {
            vec![direct_replies_filter_non_root(
                selected.bytes(),
                thread.root_id.bytes(),
            )]
        } else {
            vec![direct_replies_filter_root(thread.root_id.bytes())]
        };

        let selected_note_id = thread.selected_or_root();

        let filter = match self.threads.raw_entry_mut().from_key(&selected_note_id) {
            RawEntryMut::Occupied(_entry) => {
                // TODO(kernelkind): reenable this once the panic is fixed
                //
                // let node = entry.into_mut();
                // if let Some(first) = node.replies.first() {
                //     &filter::make_filters_since(&local_sub_filter, first.created_at + 1)
                // } else {
                //     &local_sub_filter
                // }
                &local_sub_filter
            }
            RawEntryMut::Vacant(entry) => {
                let id = NoteId::new(*selected_note_id);

                let node = ThreadNode::new(ParentState::Unknown);
                entry.insert(id, node);

                &local_sub_filter
            }
        };

        let new_notes = ndb.query(txn, filter, 500).ok().map(|r| {
            r.into_iter()
                .map(NoteRef::from_query_result)
                .collect::<Vec<_>>()
        });

        self.subs
            .entry(thread.root_id.to_note_id())
            .or_default()
            .subscribe(
                ndb,
                pool,
                &SubscriberId::Thread(NoteId::new(*selected_note_id)),
                local_sub_filter,
                || replies_filter_remote(thread),
            );

        new_notes.and_then(|notes| {
            Some(NewThreadNotes {
                selected_note_id: NoteId::new(*selected_note_id),
                notes: notes.into_iter().map(|f| f.key).collect(),
            })
        })
    }

    pub fn close(&mut self, ndb: &mut Ndb, pool: &mut RelayPool, thread: &ThreadSelection) {
        if let Some(thread_node) = self.threads.get_mut(&thread.selected_or_root()) {
            thread_node.replies_state = RepliesState::Stale;
        };

        if let Some(sub) = self.subs.get_mut(&thread.root_id.to_note_id()) {
            sub.unsubscribe(
                ndb,
                pool,
                &SubscriberId::Thread(NoteId::new(*thread.selected_or_root())),
            );
        } else {
            tracing::error!("Called close but don't have a multisub");
        }
    }

    /// Responsible for making sure the chain and the direct replies are up to date
    pub fn update(
        &mut self,
        selected: &Note<'_>,
        note_cache: &mut NoteCache,
        ndb: &Ndb,
        txn: &Transaction,
        unknown_ids: &mut UnknownIds,
    ) {
        let reply = note_cache
            .cached_note_or_insert_mut(selected.key().unwrap(), &selected)
            .reply; // TODO(kernelkind): handle unwrap

        self.fill_reply_chain_recursive(selected, &reply, note_cache, ndb, txn, unknown_ids, 0);
        let node = self.threads.get_mut(&selected.id()).unwrap(); //guarenteed to be created in previous method;

        let root_id = if reply.root.is_some() {
            reply
                .borrow(selected.tags())
                .root()
                .map(|r| r.id)
                .unwrap_or_else(|| selected.id())
        } else {
            selected.id()
        };

        // TODO(kernelkind): this should not need to do a copy
        let Some(multi_sub) = self.subs.get(&NoteId::new(*root_id)) else {
            tracing::error!("Was expecting to find multisub");
            return;
        };

        // TODO(kernelkind): this should not need to copy
        let Some(sub) = multi_sub.get_local(&SubscriberId::Thread(NoteId::new(*selected.id())))
        else {
            tracing::error!("Was expecting to find local sub");
            return;
        };

        let keys = ndb.poll_for_notes(sub.sub.clone(), 10);

        if keys.is_empty() {
            return;
        }

        tracing::info!("Got {} new notes", keys.len());

        process_thread_notes(&keys, node, ndb, txn, unknown_ids, note_cache);
    }

    fn fill_reply_chain_recursive(
        &mut self,
        cur_note: &Note<'_>,
        cur_reply: &NoteReplyBuf,
        note_cache: &mut NoteCache,
        ndb: &Ndb,
        txn: &Transaction,
        unknown_ids: &mut UnknownIds,
        recur_depth: usize,
    ) -> bool {
        let (unknown_parent_state, mut have_all_ancestors) = self
            .threads
            .get(&cur_note.id())
            .map(|t| (matches!(t.prev, ParentState::Unknown), t.have_all_ancestors))
            .unwrap_or((true, false));

        if have_all_ancestors {
            return true;
        }

        let mut new_parent = None;

        if let Some(parent) = cur_reply.borrow(cur_note.tags()).reply() {
            if unknown_parent_state {
                new_parent = Some(ParentState::Parent(NoteId::new(*parent.id)));
            }

            if let Ok(reply_note) = ndb.get_note_by_id(txn, parent.id) {
                UnknownIds::update_from_note(txn, ndb, unknown_ids, note_cache, &reply_note);
                let cached_note =
                    note_cache.cached_note_or_insert_mut(reply_note.key().unwrap(), &reply_note); // TODO(kernelkind): handle unwrap
                if cached_note.reply.reply.is_some() || cached_note.reply.root.is_some() {
                    let next_reply = cached_note.reply;

                    let depth = recur_depth + 1;
                    if self.fill_reply_chain_recursive(
                        &reply_note,
                        &next_reply,
                        note_cache,
                        ndb,
                        txn,
                        unknown_ids,
                        depth,
                    ) {
                        have_all_ancestors = true;
                    }
                }
            } else {
                unknown_ids.add_note_id_if_missing(ndb, txn, &NoteId::new(*parent.id));
                // TODO(kernelkind): shouldn't need to clone this
            };
        } else {
            have_all_ancestors = true;
            new_parent = Some(ParentState::None);
            tracing::info!("Found root");
        }

        match self.threads.raw_entry_mut().from_key(&cur_note.id()) {
            RawEntryMut::Occupied(entry) => {
                let node = entry.into_mut();
                if let Some(parent) = new_parent {
                    node.prev = parent;
                }

                if have_all_ancestors {
                    node.have_all_ancestors = true;
                }
            }
            RawEntryMut::Vacant(entry) => {
                let id = NoteId::new(*cur_note.id());
                let parent = new_parent.unwrap_or(ParentState::Unknown);
                let (_, res) = entry.insert(id, ThreadNode::new(parent));

                if have_all_ancestors {
                    res.have_all_ancestors = true;
                }
            }
        }

        have_all_ancestors
    }
}

fn direct_replies_filter_non_root(
    selected_note_id: &[u8; 32],
    root_id: &[u8; 32],
) -> nostrdb::Filter {
    nostrdb::Filter::new()
        .kinds([1])
        .custom(|n: nostrdb::Note<'_>| {
            for tag in n.tags() {
                if tag.count() < 4 {
                    continue;
                }

                let Some("e") = tag.get_str(0) else {
                    continue;
                };

                let Some(tagged_id) = tag.get_id(1) else {
                    continue;
                };

                if *tagged_id != *selected_note_id {
                    // NOTE: if these aren't dereferenced a segfault occurs...
                    continue;
                }

                if let Some(data) = tag.get_str(3) {
                    if data == "reply" {
                        return true;
                    }
                }
            }
            false
        })
        .event(root_id)
        .build()
}

/// for some reason data must be dereferenced *inside* the custom closure, not outside
fn direct_replies_filter_root(root_id: &[u8; 32]) -> nostrdb::Filter {
    nostrdb::Filter::new()
        .kinds([1])
        .custom(|n: nostrdb::Note<'_>| {
            let mut contains_root = false;
            for tag in n.tags() {
                if tag.count() < 4 {
                    continue;
                }

                let Some("e") = tag.get_str(0) else {
                    continue;
                };

                if let Some(s) = tag.get_str(3) {
                    if s == "reply" {
                        return false;
                    }
                }

                let Some(tagged_id) = tag.get_id(1) else {
                    continue;
                };

                if *tagged_id != *root_id {
                    continue;
                }

                if let Some(s) = tag.get_str(3) {
                    if s == "root" {
                        contains_root = true;
                    }
                }
            }

            contains_root
        })
        .event(root_id)
        .build()
}

fn replies_filter_remote(selection: &ThreadSelection) -> Vec<Filter> {
    vec![
        nostrdb::Filter::new()
            .kinds([1])
            .event(selection.root_id.bytes())
            .build(),
        nostrdb::Filter::new()
            .ids([selection.root_id.bytes()])
            .limit(1)
            .build(),
    ]
}
