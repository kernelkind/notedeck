use egui_virtual_list::VirtualList;
use enostr::KeypairUnowned;
use nostrdb::{Note, Transaction};
use notedeck::{MuteFun, NoteAction, NoteContext, UnknownIds};
use notedeck_ui::jobs::JobsCache;
use notedeck_ui::{NoteOptions, NoteView};

use crate::timeline::thread::{ParentState, Threads};

pub struct ThreadView<'a, 'd> {
    threads: &'a mut Threads,
    unknown_ids: &'a mut UnknownIds,
    selected_note_id: &'a [u8; 32],
    note_options: NoteOptions,
    id_source: egui::Id,
    is_muted: &'a MuteFun, // TODO(kernelkind): reintroduce muting stuff
    note_context: &'a mut NoteContext<'d>,
    cur_acc: &'a Option<KeypairUnowned<'a>>,
    jobs: &'a mut JobsCache,
}

impl<'a, 'd> ThreadView<'a, 'd> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        threads: &'a mut Threads,
        unknown_ids: &'a mut UnknownIds,
        selected_note_id: &'a [u8; 32],
        note_options: NoteOptions,
        is_muted: &'a MuteFun,
        note_context: &'a mut NoteContext<'d>,
        cur_acc: &'a Option<KeypairUnowned<'a>>,
        jobs: &'a mut JobsCache,
    ) -> Self {
        let id_source = egui::Id::new("threadscroll_threadview");
        ThreadView {
            threads,
            unknown_ids,
            selected_note_id,
            note_options,
            id_source,
            is_muted,
            note_context,
            cur_acc,
            jobs,
        }
    }

    pub fn id_source(mut self, id: egui::Id) -> Self {
        self.id_source = id;
        self
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<NoteAction> {
        let txn = Transaction::new(self.note_context.ndb).expect("txn");

        let mut scroll_area = egui::ScrollArea::vertical()
            .id_salt(self.id_source)
            .animated(false)
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);

        let offset_id = self.id_source.with("scroll_offset");

        if let Some(offset) = ui.data(|i| i.get_temp::<f32>(offset_id)) {
            scroll_area = scroll_area.vertical_scroll_offset(offset);
        }

        let output = scroll_area.show(ui, |ui| self.notes(ui, &txn));

        ui.data_mut(|d| d.insert_temp(offset_id, output.state.offset.y));

        output.inner
    }

    fn notes(&mut self, ui: &mut egui::Ui, txn: &Transaction) -> Option<NoteAction> {
        let Ok(cur_note) = self
            .note_context
            .ndb
            .get_note_by_id(txn, self.selected_note_id)
        else {
            tracing::error!("Did not find selected note for thread");
            return None;
        };

        self.threads.update(
            &cur_note,
            self.note_context.note_cache,
            self.note_context.ndb,
            txn,
            self.unknown_ids,
        );

        let cur_node = self.threads.threads.get(&self.selected_note_id).unwrap();

        let mut full_chain = cur_node.have_all_ancestors;
        let mut note_builder = ThreadNoteBuilder::default();
        note_builder.selected = Some(cur_note);

        let mut parent_state = cur_node.prev.clone();
        while let ParentState::Parent(id) = parent_state {
            if let Ok(note) = self.note_context.ndb.get_note_by_id(txn, id.bytes()) {
                note_builder.add_chain(note);
                if let Some(res) = self.threads.threads.get(&id.bytes()) {
                    parent_state = res.prev.clone();
                    continue;
                } else {
                    full_chain = false;
                }
            } else {
                full_chain = false;
            }
            parent_state = ParentState::Unknown;
        }

        for note_ref in &cur_node.replies {
            if let Ok(note) = self.note_context.ndb.get_note_by_key(txn, note_ref.key) {
                note_builder.add_reply(note);
            } else {
                full_chain = false;
            }
        }

        let list = &mut self
            .threads
            .threads
            .get_mut(&self.selected_note_id)
            .unwrap()
            .list;

        let mut action = None;
        if let Some(notes) = note_builder.to_notes() {
            if !full_chain {
                // TODO: insert UI denoting we don't have the full chain yet
                ui.colored_label(ui.visuals().error_fg_color, "LOADING NOTES");
            }

            let zapping_acc = self
                .cur_acc
                .as_ref()
                .filter(|_| self.note_context.current_account_has_wallet)
                .or(self.cur_acc.as_ref());

            action = notedeck_ui::padding(8.0, ui, |ui| {
                show_notes(
                    ui,
                    list,
                    &notes,
                    self.note_context,
                    zapping_acc,
                    self.note_options,
                    self.jobs,
                )
            })
            .inner;
        } else {
            tracing::error!("Did not find selected note for thread"); // TODO: make this msg more verbose
        }

        action
    }
}

fn show_notes(
    ui: &mut egui::Ui,
    list: &mut VirtualList,
    notes: &Vec<ThreadNote>,
    note_context: &mut NoteContext<'_>,
    zapping_acc: Option<&KeypairUnowned<'_>>,
    flags: NoteOptions,
    jobs: &mut JobsCache,
) -> Option<NoteAction> {
    let mut action = None;

    list.ui_custom_layout(ui, notes.len(), |ui, cur_index| {
        let note = &notes[cur_index];
        let options = note.options(flags);

        let resp = NoteView::new(note_context, zapping_acc, &note.note, options, jobs).show(ui);

        if let Some(note_action) = resp.action {
            action = Some(note_action);
        }

        notedeck_ui::hline(ui);

        1
    });

    action
}

#[derive(Default)]
struct ThreadNoteBuilder<'a> {
    chain: Vec<Note<'a>>,
    selected: Option<Note<'a>>,
    replies: Vec<Note<'a>>,
}

impl<'a> ThreadNoteBuilder<'a> {
    pub fn add_chain(&mut self, note: Note<'a>) {
        self.chain.push(note);
    }

    pub fn add_reply(&mut self, note: Note<'a>) {
        self.replies.push(note);
    }

    pub fn to_notes(mut self) -> Option<Vec<ThreadNote<'a>>> {
        let Some(selected) = self.selected else {
            return None;
        };

        let mut out = Vec::new();

        while let Some(note) = self.chain.pop() {
            out.push(ThreadNote {
                note,
                note_type: ThreadNoteType::Chain,
            });
        }

        out.push(ThreadNote {
            note: selected,
            note_type: ThreadNoteType::Selected,
        });

        for reply in self.replies {
            out.push(ThreadNote {
                note: reply,
                note_type: ThreadNoteType::Reply,
            });
        }

        Some(out)
    }
}

enum ThreadNoteType {
    Chain,
    Selected,
    Reply,
}

struct ThreadNote<'a> {
    pub note: Note<'a>,
    note_type: ThreadNoteType,
}

impl<'a> ThreadNote<'a> {
    fn options(&self, cur_options: NoteOptions) -> NoteOptions {
        match self.note_type {
            ThreadNoteType::Chain => chain_options(cur_options),
            ThreadNoteType::Selected => selected_options(cur_options),
            ThreadNoteType::Reply => reply_options(cur_options),
        }
    }
}

fn chain_options(options: NoteOptions) -> NoteOptions {
    options
}

fn selected_options(mut options: NoteOptions) -> NoteOptions {
    options.set_wide(true);
    options
}

fn reply_options(options: NoteOptions) -> NoteOptions {
    options
}
