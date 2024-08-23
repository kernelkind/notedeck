pub mod contents;
pub mod options;
pub mod post;
pub mod reply;

pub use contents::NoteContents;
pub use options::NoteOptions;
pub use post::{PostAction, PostResponse, PostView};
pub use reply::PostReplyView;

use crate::{actionbar::BarAction, colors, notecache::CachedNote, ui, ui::View, Damus};
use egui::{Label, RichText, Sense};
use nostrdb::{Note, NoteKey, NoteReply, Transaction};

pub struct NoteView<'a> {
    app: &'a mut Damus,
    note: &'a nostrdb::Note<'a>,
    flags: NoteOptions,
}

pub struct NoteResponse {
    pub response: egui::Response,
    pub action: Option<BarAction>,
    pub profile_clicked: bool,
}

impl<'a> View for NoteView<'a> {
    fn ui(&mut self, ui: &mut egui::Ui) {
        self.show(ui);
    }
}

fn reply_desc(ui: &mut egui::Ui, txn: &Transaction, note_reply: &NoteReply, app: &mut Damus) {
    #[cfg(feature = "profiling")]
    puffin::profile_function!();

    let size = 10.0;
    let selectable = false;

    ui.add(
        Label::new(
            RichText::new("replying to")
                .size(size)
                .color(colors::GRAY_SECONDARY),
        )
        .selectable(selectable),
    );

    let reply = if let Some(reply) = note_reply.reply() {
        reply
    } else {
        return;
    };

    let reply_note = if let Ok(reply_note) = app.ndb.get_note_by_id(txn, reply.id) {
        reply_note
    } else {
        ui.add(
            Label::new(
                RichText::new("a note")
                    .size(size)
                    .color(colors::GRAY_SECONDARY),
            )
            .selectable(selectable),
        );
        return;
    };

    if note_reply.is_reply_to_root() {
        // We're replying to the root, let's show this
        ui.add(
            ui::Mention::new(app, txn, reply_note.pubkey())
                .size(size)
                .selectable(selectable),
        );
        ui.add(
            Label::new(
                RichText::new("'s note")
                    .size(size)
                    .color(colors::GRAY_SECONDARY),
            )
            .selectable(selectable),
        );
    } else if let Some(root) = note_reply.root() {
        // replying to another post in a thread, not the root

        if let Ok(root_note) = app.ndb.get_note_by_id(txn, root.id) {
            if root_note.pubkey() == reply_note.pubkey() {
                // simply "replying to bob's note" when replying to bob in his thread
                ui.add(
                    ui::Mention::new(app, txn, reply_note.pubkey())
                        .size(size)
                        .selectable(selectable),
                );
                ui.add(
                    Label::new(
                        RichText::new("'s note")
                            .size(size)
                            .color(colors::GRAY_SECONDARY),
                    )
                    .selectable(selectable),
                );
            } else {
                // replying to bob in alice's thread

                ui.add(
                    ui::Mention::new(app, txn, reply_note.pubkey())
                        .size(size)
                        .selectable(selectable),
                );
                ui.add(
                    Label::new(RichText::new("in").size(size).color(colors::GRAY_SECONDARY))
                        .selectable(selectable),
                );
                ui.add(
                    ui::Mention::new(app, txn, root_note.pubkey())
                        .size(size)
                        .selectable(selectable),
                );
                ui.add(
                    Label::new(
                        RichText::new("'s thread")
                            .size(size)
                            .color(colors::GRAY_SECONDARY),
                    )
                    .selectable(selectable),
                );
            }
        } else {
            ui.add(
                ui::Mention::new(app, txn, reply_note.pubkey())
                    .size(size)
                    .selectable(selectable),
            );
            ui.add(
                Label::new(
                    RichText::new("in someone's thread")
                        .size(size)
                        .color(colors::GRAY_SECONDARY),
                )
                .selectable(selectable),
            );
        }
    }
}

impl<'a> NoteView<'a> {
    pub fn new(app: &'a mut Damus, note: &'a nostrdb::Note<'a>) -> Self {
        let flags = NoteOptions::actionbar | NoteOptions::note_previews;
        Self { app, note, flags }
    }

    pub fn actionbar(mut self, enable: bool) -> Self {
        self.options_mut().set_actionbar(enable);
        self
    }

    pub fn small_pfp(mut self, enable: bool) -> Self {
        self.options_mut().set_small_pfp(enable);
        self
    }

    pub fn medium_pfp(mut self, enable: bool) -> Self {
        self.options_mut().set_medium_pfp(enable);
        self
    }

    pub fn note_previews(mut self, enable: bool) -> Self {
        self.options_mut().set_note_previews(enable);
        self
    }

    pub fn selectable_text(mut self, enable: bool) -> Self {
        self.options_mut().set_selectable_text(enable);
        self
    }

    pub fn wide(mut self, enable: bool) -> Self {
        self.options_mut().set_wide(enable);
        self
    }

    pub fn options(&self) -> NoteOptions {
        self.flags
    }

    pub fn options_mut(&mut self) -> &mut NoteOptions {
        &mut self.flags
    }

    fn textmode_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let note_key = self.note.key().expect("todo: implement non-db notes");
        let txn = self.note.txn().expect("todo: implement non-db notes");

        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
            let profile = self.app.ndb.get_profile_by_pubkey(txn, self.note.pubkey());

            //ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;

            let cached_note = self
                .app
                .note_cache_mut()
                .cached_note_or_insert_mut(note_key, self.note);

            let (_id, rect) = ui.allocate_space(egui::vec2(50.0, 20.0));
            ui.allocate_rect(rect, Sense::hover());
            ui.put(rect, |ui: &mut egui::Ui| {
                render_reltime(ui, cached_note, false).response
            });
            let (_id, rect) = ui.allocate_space(egui::vec2(150.0, 20.0));
            ui.allocate_rect(rect, Sense::hover());
            ui.put(rect, |ui: &mut egui::Ui| {
                ui.add(
                    ui::Username::new(profile.as_ref().ok(), self.note.pubkey())
                        .abbreviated(6)
                        .pk_colored(true),
                )
            });

            ui.add(NoteContents::new(
                self.app, txn, self.note, note_key, self.flags,
            ));
            //});
        })
        .response
    }

    pub fn expand_size() -> f32 {
        5.0
    }

    fn pfp(
        &mut self,
        note_key: NoteKey,
        profile: &Result<nostrdb::ProfileRecord<'_>, nostrdb::Error>,
        ui: &mut egui::Ui,
    ) -> egui::Response {
        if !self.options().has_wide() {
            ui.spacing_mut().item_spacing.x = 16.0;
        } else {
            ui.spacing_mut().item_spacing.x = 4.0;
        }

        let pfp_size = self.options().pfp_size();

        let sense = Sense::click();
        match profile
            .as_ref()
            .ok()
            .and_then(|p| p.record().profile()?.picture())
        {
            // these have different lifetimes and types,
            // so the calls must be separate
            Some(pic) => {
                let anim_speed = 0.05;
                let profile_key = profile.as_ref().unwrap().record().note_key();
                let note_key = note_key.as_u64();

                if self.app.is_mobile() {
                    ui.add(ui::ProfilePic::new(&mut self.app.img_cache, pic))
                        .interact(sense)
                } else {
                    let (rect, size, _resp) = ui::anim::hover_expand(
                        ui,
                        egui::Id::new((profile_key, note_key)),
                        pfp_size,
                        ui::NoteView::expand_size(),
                        anim_speed,
                    );

                    ui.put(
                        rect,
                        ui::ProfilePic::new(&mut self.app.img_cache, pic).size(size),
                    )
                    .on_hover_ui_at_pointer(|ui| {
                        ui.set_max_width(300.0);
                        ui.add(ui::ProfilePreview::new(
                            profile.as_ref().unwrap(),
                            &mut self.app.img_cache,
                        ));
                    });
                    _resp
                }
            }
            None => ui
                .add(
                    ui::ProfilePic::new(&mut self.app.img_cache, ui::ProfilePic::no_pfp_url())
                        .size(pfp_size),
                )
                .interact(sense),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> NoteResponse {
        if self.app.textmode {
            NoteResponse {
                response: self.textmode_ui(ui),
                action: None,
                profile_clicked: false,
            }
        } else {
            self.show_standard(ui)
        }
    }

    fn note_header(
        ui: &mut egui::Ui,
        app: &mut Damus,
        note: &Note,
        profile: &Result<nostrdb::ProfileRecord<'_>, nostrdb::Error>,
    ) -> egui::Response {
        let note_key = note.key().unwrap();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add(ui::Username::new(profile.as_ref().ok(), note.pubkey()).abbreviated(20));

            let cached_note = app
                .note_cache_mut()
                .cached_note_or_insert_mut(note_key, note);
            render_reltime(ui, cached_note, true);
        })
        .response
    }

    fn show_standard(&mut self, ui: &mut egui::Ui) -> NoteResponse {
        #[cfg(feature = "profiling")]
        puffin::profile_function!();
        let note_key = self.note.key().expect("todo: support non-db notes");
        let txn = self.note.txn().expect("todo: support non-db notes");
        let mut note_action: Option<BarAction> = None;
        let profile = self.app.ndb.get_profile_by_pubkey(txn, self.note.pubkey());

        let mut profile_clicked = false;

        // wide design
        let response = if self.options().has_wide() {
            ui.horizontal(|ui| {
                profile_clicked = self.pfp(note_key, &profile, ui).clicked();

                let size = ui.available_size();
                ui.vertical(|ui| {
                    ui.add_sized([size.x, self.options().pfp_size()], |ui: &mut egui::Ui| {
                        ui.horizontal_centered(|ui| {
                            NoteView::note_header(ui, self.app, self.note, &profile);
                        })
                        .response
                    });

                    let note_reply = self
                        .app
                        .note_cache_mut()
                        .cached_note_or_insert_mut(note_key, self.note)
                        .reply
                        .borrow(self.note.tags());

                    if note_reply.reply().is_some() {
                        ui.horizontal(|ui| {
                            reply_desc(ui, txn, &note_reply, self.app);
                        });
                    }
                });
            });

            let resp = ui.add(NoteContents::new(
                self.app,
                txn,
                self.note,
                note_key,
                self.options(),
            ));

            if self.options().has_actionbar() {
                note_action = render_note_actionbar(ui, note_key).inner;
            }

            resp
        } else {
            // main design
            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                profile_clicked = self.pfp(note_key, &profile, ui).clicked();

                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    NoteView::note_header(ui, self.app, self.note, &profile);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;

                        let note_reply = self
                            .app
                            .note_cache_mut()
                            .cached_note_or_insert_mut(note_key, self.note)
                            .reply
                            .borrow(self.note.tags());

                        if note_reply.reply().is_some() {
                            reply_desc(ui, txn, &note_reply, self.app);
                        }
                    });

                    ui.add(NoteContents::new(
                        self.app,
                        txn,
                        self.note,
                        note_key,
                        self.options(),
                    ));

                    if self.options().has_actionbar() {
                        note_action = render_note_actionbar(ui, note_key).inner;
                    }
                });
            })
            .response
        };

        NoteResponse {
            response,
            action: note_action,
            profile_clicked,
        }
    }
}

fn render_note_actionbar(
    ui: &mut egui::Ui,
    note_key: NoteKey,
) -> egui::InnerResponse<Option<BarAction>> {
    ui.horizontal(|ui| {
        let reply_resp = reply_button(ui, note_key);
        let thread_resp = thread_button(ui, note_key);

        if reply_resp.clicked() {
            Some(BarAction::Reply)
        } else if thread_resp.clicked() {
            Some(BarAction::OpenThread)
        } else {
            None
        }
    })
}

fn secondary_label(ui: &mut egui::Ui, s: impl Into<String>) {
    ui.add(Label::new(
        RichText::new(s).size(10.0).color(colors::GRAY_SECONDARY),
    ));
}

fn render_reltime(
    ui: &mut egui::Ui,
    note_cache: &mut CachedNote,
    before: bool,
) -> egui::InnerResponse<()> {
    #[cfg(feature = "profiling")]
    puffin::profile_function!();

    ui.horizontal(|ui| {
        if before {
            secondary_label(ui, "⋅");
        }

        secondary_label(ui, note_cache.reltime_str_mut());

        if !before {
            secondary_label(ui, "⋅");
        }
    })
}

fn reply_button(ui: &mut egui::Ui, note_key: NoteKey) -> egui::Response {
    let img_data = if ui.style().visuals.dark_mode {
        egui::include_image!("../../../assets/icons/reply.png")
    } else {
        egui::include_image!("../../../assets/icons/reply-dark.png")
    };

    let (rect, size, resp) =
        ui::anim::hover_expand_small(ui, ui.id().with(("reply_anim", note_key)));

    // align rect to note contents
    let expand_size = 5.0; // from hover_expand_small
    let rect = rect.translate(egui::vec2(-(expand_size / 2.0), 0.0));

    let put_resp = ui.put(rect, egui::Image::new(img_data).max_width(size));

    resp.union(put_resp)
}

fn thread_button(ui: &mut egui::Ui, note_key: NoteKey) -> egui::Response {
    let id = ui.id().with(("thread_anim", note_key));
    let size = 8.0;
    let expand_size = 5.0;
    let anim_speed = 0.05;

    let (rect, size, resp) = ui::anim::hover_expand(ui, id, size, expand_size, anim_speed);

    let color = if ui.style().visuals.dark_mode {
        egui::Color32::WHITE
    } else {
        egui::Color32::BLACK
    };

    ui.painter_at(rect).circle_stroke(
        rect.center(),
        (size - 1.0) / 2.0,
        egui::Stroke::new(1.0, color),
    );

    resp
}
