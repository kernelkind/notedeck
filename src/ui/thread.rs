use crate::{actionbar::BarResult, thread::ThreadResult, timeline::TimelineSource, ui, Damus};
use nostrdb::{NoteKey, Transaction};
use tracing::{error, warn};

pub struct ThreadView<'a> {
    app: &'a mut Damus,
    timeline: usize,
    selected_note_id: &'a [u8; 32],
}

impl<'a> ThreadView<'a> {
    pub fn new(app: &'a mut Damus, timeline: usize, selected_note_id: &'a [u8; 32]) -> Self {
        ThreadView {
            app,
            timeline,
            selected_note_id,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<BarResult> {
        let txn = Transaction::new(&self.app.ndb).expect("txn");
        let mut result: Option<BarResult> = None;

        let selected_note_key = if let Ok(key) = self
            .app
            .ndb
            .get_notekey_by_id(&txn, self.selected_note_id)
            .map(NoteKey::new)
        {
            key
        } else {
            // TODO: render 404 ?
            return None;
        };

        let scroll_id = egui::Id::new((
            "threadscroll",
            self.app.timelines[self.timeline].selected_view,
            self.timeline,
            selected_note_key,
        ));

        ui.label(
            egui::RichText::new("Threads ALPHA! It's not done. Things will be broken.")
                .color(egui::Color32::RED),
        );

        egui::ScrollArea::vertical()
            .id_source(scroll_id)
            .animated(false)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                let note = if let Ok(note) = self.app.ndb.get_note_by_key(&txn, selected_note_key) {
                    note
                } else {
                    return;
                };

                let root_id = {
                    let cached_note = self
                        .app
                        .note_cache_mut()
                        .cached_note_or_insert(selected_note_key, &note);

                    cached_note
                        .reply
                        .borrow(note.tags())
                        .root()
                        .map_or_else(|| self.selected_note_id, |nr| nr.id)
                };

                // TODO: unify poll_notes_into_view
                // // poll for new notes and insert them into our existing notes
                // {
                //     let mut ids = HashSet::new();
                //     let _ = TimelineSource::Thread(root_id)
                //         .poll_notes_into_view(self.app, &txn, &mut ids);
                //     // TODO: do something with unknown ids
                // }

                let thread = match self
                    .app
                    .threads
                    .thread_mut(&root_id, &mut self.app.note_stream_interactor)
                {
                    ThreadResult::Fresh(thread) => thread,
                    ThreadResult::Stale(thread) => thread,
                };
                if let Some(notes) = self
                    .app
                    .note_stream_interactor
                    .take_unseen(&thread.note_stream_id)
                {
                    // todo, remove this & bring back poll_notes_into_view
                    thread.view.insert(notes.as_ref(), true);
                }

                let (len, list) = {
                    if let Some(thread) = self.app.threads.get_thread_mut(&root_id) {
                        let len = thread.view.notes.len();
                        (len, &mut thread.view.list)
                    } else {
                        return;
                    }
                };

                list.clone()
                    .borrow_mut()
                    .ui_custom_layout(ui, len, |ui, start_index| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.spacing_mut().item_spacing.x = 4.0;

                        let ind = len - 1 - start_index;
                        let note_key = {
                            if let Some(thread) = self.app.threads.get_thread_mut(&root_id) {
                                thread.view.notes[ind].key
                            } else {
                                warn!("failed to get note key");
                                return 0;
                            }
                        };

                        let note = if let Ok(note) = self.app.ndb.get_note_by_key(&txn, note_key) {
                            note
                        } else {
                            warn!("failed to query note {:?}", note_key);
                            return 0;
                        };

                        ui::padding(8.0, ui, |ui| {
                            let textmode = self.app.textmode;
                            let resp = ui::NoteView::new(self.app, &note)
                                .note_previews(!textmode)
                                .show(ui);

                            if let Some(action) = resp.action {
                                let br = action.execute(self.app, self.timeline, note.id(), &txn);
                                if br.is_some() {
                                    result = br;
                                }
                            }
                        });

                        ui::hline(ui);
                        //ui.add(egui::Separator::default().spacing(0.0));

                        1
                    });
            });

        result
    }
}
