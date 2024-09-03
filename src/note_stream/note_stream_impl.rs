use std::collections::HashSet;

use enostr::{ClientMessage, Filter, RelayPool};
use log::error;
use nostrdb::{Ndb, Subscription, Transaction};
use tracing::{debug, info};
use uuid::Uuid;

use crate::{
    app::{get_unknown_ids_filter, get_unknown_note_ids, UnknownId},
    note::NoteRef,
    notecache::NoteCache,
    Damus,
};

use super::{
    misc::{NoteStreamInstanceId, NoteStreamInstanceState},
    note_stream_interactor::{NoteStreamCommand, NoteStreamInteractor},
    note_stream_manager::NoteStreamManager,
};

fn process_new_subscriptions(
    ndb: &Ndb,
    pool: &mut RelayPool,
    note_stream_manager: &mut NoteStreamManager,
) {
    for filters in note_stream_manager.find_new_ndb_subscriptions() {
        let sub = ndb.subscribe(&filters);
        let subid = Uuid::new_v4().to_string();
        pool.subscribe(subid.clone(), filters.clone());
        if let Ok(sub) = sub {
            info!("saving subscription {:?}", sub);
            note_stream_manager.save_subscription(filters, sub, subid);
        }
    }
}

fn process_subscription_deletions(
    ndb: &Ndb,
    pool: &mut RelayPool,
    note_stream_manager: &mut NoteStreamManager,
) {
    for sub_id in note_stream_manager.take_ndb_subscription_deletions() {
        info!("unsubscribing: {:?}", sub_id);
        let _ = ndb.unsubscribe(sub_id.ndb_id);
        pool.unsubscribe(sub_id.remote_id.clone());
    }
}

fn process_new_note_queries(app: &mut Damus) {
    let ids_map = app.note_stream_manager.get_active_filters_for_ids();
    for (id, filters) in ids_map {
        let cur_sub =
            if let Some(sub_id) = app.note_stream_manager.get_subscription_for_instance(&id) {
                *sub_id
            } else {
                return;
            };
        let unknown_ids_filter = process_new_note_query(app, &id, cur_sub);

        if let Some(filters) = unknown_ids_filter {
            send_unknown_ids_filter(&mut app.pool, filters);
        }
    }
}

fn send_unknown_ids_filter(pool: &mut RelayPool, filters: Vec<Filter>) {
    let msg = ClientMessage::req("unknown_ids".to_string(), filters);
    pool.send(&msg);
}

fn process_new_note_query(
    app: &mut Damus,
    id: &NoteStreamInstanceId,
    subscription: Subscription,
) -> Option<Vec<Filter>> {
    let new_note_ids = app.ndb.poll_for_notes(subscription, 100);
    if !new_note_ids.is_empty() {
        info!(
            "found {} new notes for noteStreamInstanceId:\n{:?} and subscription:\n{:?}",
            new_note_ids.len(),
            id,
            subscription
        );
    }

    let mut notes: Vec<NoteRef> = Vec::with_capacity(new_note_ids.len());
    let mut unknown_ids: HashSet<UnknownId> = HashSet::new();

    let txn = Transaction::new(&app.ndb).expect("txn");
    for key in new_note_ids {
        let note = if let Ok(note) = app.ndb.get_note_by_key(&txn, key) {
            note
        } else {
            error!("hit race condition in poll_notes_into_view: https://github.com/damus-io/nostrdb/issues/35 note {:?} was not added to timeline", key);
            continue;
        };

        let cached_note = app
            .note_cache_mut()
            .cached_note_or_insert(key, &note)
            .clone();
        let _ = get_unknown_note_ids(&app.ndb, &cached_note, &txn, &note, key, &mut unknown_ids);

        let created_at = note.created_at();
        notes.push(NoteRef { key, created_at });
    }
    let unknown_ids: Vec<UnknownId> = unknown_ids.into_iter().collect();
    let unknown_ids_filter = get_unknown_ids_filter(&unknown_ids);

    if let Some(last_note) = notes.last() {
        // TODO: is this the right way to get the last note?
        let last_seen = last_note.created_at;
        app.note_stream_manager.update_last_seen(id, last_seen);
    }
    app.note_stream_interactor.cache.insert(id.clone(), notes);

    if let Some(instance) = app.note_stream_manager.get_note_stream_instance_mut(id) {
        if *instance.get_status() == NoteStreamInstanceState::Reactivating {
            instance.set_status(NoteStreamInstanceState::Active);
        }
    }

    unknown_ids_filter
}

fn process_interactor_commands(
    note_stream_manager: &mut NoteStreamManager,
    note_stream_interactor: &mut NoteStreamInteractor,
) {
    for command in note_stream_interactor.commands.drain(..) {
        match command {
            NoteStreamCommand::NewStreamInstance(id, filters_id) => {
                note_stream_manager.add_new(id, filters_id);
            }
            NoteStreamCommand::PauseStreamInstance(id) => {
                note_stream_manager.pause(&id);
            }
            NoteStreamCommand::ResumeStreamInstance(id) => {
                note_stream_manager.resume(&id);
            }
        }
    }
}

pub fn process_note_streams(app: &mut Damus) {
    process_interactor_commands(
        &mut app.note_stream_manager,
        &mut app.note_stream_interactor,
    );
    process_new_subscriptions(&app.ndb, &mut app.pool, &mut app.note_stream_manager);
    process_subscription_deletions(&app.ndb, &mut app.pool, &mut app.note_stream_manager);
    process_new_note_queries(app);
}
