use crate::{
    note::NoteRef,
    route::Route,
    thread::{Thread, ThreadResult},
    Damus,
};
use enostr::NoteId;
use nostrdb::Transaction;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum BarAction {
    Reply,
    OpenThread,
}

pub struct NewThreadNotes {
    pub root_id: NoteId,
    pub notes: Vec<NoteRef>,
}

pub enum BarResult {
    NewThreadNotes(NewThreadNotes),
}

/// open_thread is called when a note is selected and we need to navigate
/// to a thread It is responsible for managing the subscription and
/// making sure the thread is up to date. In a sense, it's a model for
/// the thread view. We don't have a concept of model/view/controller etc
/// in egui, but this is the closest thing to that.
fn open_thread(
    app: &mut Damus,
    txn: &Transaction,
    timeline: usize,
    selected_note: &[u8; 32],
) -> Option<BarResult> {
    {
        let timeline = &mut app.timelines[timeline];
        timeline
            .routes
            .push(Route::Thread(NoteId::new(selected_note.to_owned())));
        timeline.navigating = true;
    }

    let root_id = crate::note::root_note_id_from_selected_id(app, txn, selected_note);

    let thread_res = app
        .threads
        .thread_mut(root_id, &mut app.note_stream_interactor);

    None
}

impl BarAction {
    pub fn execute(
        self,
        app: &mut Damus,
        timeline: usize,
        replying_to: &[u8; 32],
        txn: &Transaction,
    ) -> Option<BarResult> {
        match self {
            BarAction::Reply => {
                let timeline = &mut app.timelines[timeline];
                timeline
                    .routes
                    .push(Route::Reply(NoteId::new(replying_to.to_owned())));
                timeline.navigating = true;
                None
            }

            BarAction::OpenThread => open_thread(app, txn, timeline, replying_to),
        }
    }
}

impl BarResult {
    pub fn new_thread_notes(notes: Vec<NoteRef>, root_id: NoteId) -> Self {
        BarResult::NewThreadNotes(NewThreadNotes::new(notes, root_id))
    }
}

impl NewThreadNotes {
    pub fn new(notes: Vec<NoteRef>, root_id: NoteId) -> Self {
        NewThreadNotes { notes, root_id }
    }

    /// Simple helper for processing a NewThreadNotes result. It simply
    /// inserts/merges the notes into the thread cache
    pub fn process(&self, thread: &mut Thread) {
        // threads are chronological, ie reversed from reverse-chronological, the default.
        let reversed = true;
        thread.view.insert(&self.notes, reversed);
    }
}
