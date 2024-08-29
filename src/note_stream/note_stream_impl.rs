use enostr::RelayPool;
use nostrdb::{Ndb, Transaction};
use uuid::Uuid;

use crate::{note::NoteRef, Damus};

use super::{
    note_stream_interactor::{NoteStreamCommand, NoteStreamInteractor},
    note_stream_manager::NoteStreamManager,
};

fn process_new_subscriptions(
    ndb: &Ndb,
    pool: &mut RelayPool,
    note_stream_manager: &mut NoteStreamManager,
) {
    for filters in note_stream_manager.find_new_ndb_subscriptions() {
        let sub = ndb.subscribe(filters.clone());
        let subid = Uuid::new_v4().to_string();
        pool.subscribe(subid.clone(), filters);
        if let Ok(sub) = sub {
            note_stream_manager.save_subscription(sub, subid);
        }
    }
}

fn process_subscription_deletions(
    ndb: &Ndb,
    pool: &mut RelayPool,
    note_stream_manager: &mut NoteStreamManager,
) {
    for sub_id in note_stream_manager.take_ndb_subscription_deletions() {
        let _ = ndb.unsubscribe(sub_id.ndb_id);
        pool.unsubscribe(sub_id.remote_id.clone());
    }
}

fn process_new_note_queries(
    ndb: &Ndb,
    note_stream_manager: &mut NoteStreamManager,
    note_stream_interactor: &mut NoteStreamInteractor,
    txn: &Transaction,
) {
    for (id, filters) in note_stream_manager.get_active_filters_for_ids() {
        let results = ndb.query(txn, filters, 1000).unwrap();
        let notes: Vec<NoteRef> = results
            .into_iter()
            .map(NoteRef::from_query_result)
            .collect();
        if let Some(last_note) = notes.last() {
            // TODO: is this the right way to get the last note?
            let last_seen = last_note.created_at;
            note_stream_manager.update_last_seen(&id, last_seen);
        }
        note_stream_interactor.cache.insert(id, notes);
    }
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
    let txn = Transaction::new(&app.ndb).expect("txn");
    process_new_note_queries(
        &app.ndb,
        &mut app.note_stream_manager,
        &mut app.note_stream_interactor,
        &txn,
    );
}
