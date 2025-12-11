use enostr::Pubkey;
use nostrdb::{Ndb, Transaction};
use notedeck::{ContactState, Images};
use notedeck_ui::{contacts_list::ContactsCollection, ContactsListView};

use crate::cache::{ConversationCache, ConversationStates};

pub struct CreateConvoUi<'a> {
    cache: &'a ConversationCache,
    states: &'a mut ConversationStates,
    ndb: &'a Ndb,
    img_cache: &'a mut Images,
    contacts: &'a ContactState,
}

pub struct CreateConvoResponse {
    pub recipient: Pubkey,
}

impl<'a> CreateConvoUi<'a> {
    pub fn new(
        cache: &'a ConversationCache,
        states: &'a mut ConversationStates,
        ndb: &'a Ndb,
        img_cache: &'a mut Images,
        contacts: &'a ContactState,
    ) -> Self {
        Self {
            cache,
            states,
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
        ContactsListView::new(
            ContactsCollection::Set(contacts),
            self.ndb,
            self.img_cache,
            &txn,
        )
        .ui(ui);

        unimplemented!()
    }
}
