use egui::{Response, RichText};
use enostr::{Keypair, NoteId, Pubkey};
use std::fmt::{self};

use crate::{ui::AccountManagementView, Damus};

/// App routing. These describe different places you can go inside Notedeck.
#[derive(Clone, Debug)]
pub enum Route {
    Timeline(String),
    ManageAccount,
    Thread(NoteId),
    Reply(NoteId),
    Relays,
    Profile(Pubkey),
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::ManageAccount => write!(f, "Manage Account"),
            Route::Timeline(name) => write!(f, "{}", name),
            Route::Thread(_id) => write!(f, "Thread"),
            Route::Reply(_id) => write!(f, "Reply"),
            Route::Relays => write!(f, "Relays"),
            Route::Profile(_) => write!(f, "Profile"), // TODO: probably should include user's display name
        }
    }
}

impl Route {
    pub fn show_global_popup(&self, app: &mut Damus, ui: &mut egui::Ui) -> Option<Response> {
        match self {
            Route::ManageAccount => AccountManagementView::ui(app, ui),
            _ => None,
        }
    }

    pub fn title(&self) -> RichText {
        match self {
            Route::ManageAccount => RichText::new("Manage Account").size(24.0),
            Route::Thread(_) => RichText::new("Thread"),
            Route::Reply(_) => RichText::new("Reply"),
            Route::Relays => RichText::new("Relays"),
            Route::Timeline(_) => RichText::new("Timeline"),
            Route::Profile(_) => RichText::new("Profile"),
        }
    }
}
