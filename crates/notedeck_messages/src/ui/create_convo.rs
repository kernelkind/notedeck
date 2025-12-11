use egui::{Label, RichText};
use enostr::Pubkey;
use nostrdb::{Ndb, Transaction};
use notedeck::{ContactState, Images, NotedeckTextStyle};
use notedeck_ui::{contacts_list::ContactsCollection, ContactsListView};

pub struct CreateConvoUi<'a> {
    ndb: &'a Ndb,
    img_cache: &'a mut Images,
    contacts: &'a ContactState,
}

pub struct CreateConvoResponse {
    pub recipient: Pubkey,
}

impl<'a> CreateConvoUi<'a> {
    pub fn new(ndb: &'a Ndb, img_cache: &'a mut Images, contacts: &'a ContactState) -> Self {
        Self {
            ndb,
            img_cache,
            contacts,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<CreateConvoResponse> {
        let ContactState::Received { contacts, .. } = self.contacts else {
            // TODO render something about not having contacts
            return None;
        };

        let txn = Transaction::new(self.ndb).expect("txn");

        ui.add(Label::new(
            RichText::new("Contacts").text_style(NotedeckTextStyle::Heading.text_style()),
        ));
        let resp = ContactsListView::new(
            ContactsCollection::Set(contacts),
            self.ndb,
            self.img_cache,
            &txn,
        )
        .ui(ui);

        resp.output.map(|a| match a {
            notedeck_ui::ContactsListAction::Select(pubkey) => {
                CreateConvoResponse { recipient: pubkey }
            }
        })
    }
}
