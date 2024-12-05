use std::collections::HashMap;

use enostr::Pubkey;
use nostrdb::Ndb;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    accounts::AccountsRoute,
    column::{Column, Columns, SerializableColumns},
    route::Route,
    timeline::{self, Timeline, TimelineKind},
    ui::{add_column::AddColumnRoute, configure_deck::ConfigureDeckResponse},
};

static FALLBACK_PUBKEY: &str = "aa733081e4f0f79dd43023d8983265593f2b41a988671cfcef3f489b91ad93fe";

pub enum DecksAction {
    Switch(usize),
    Removing(usize),
}

#[derive(Serialize)]
pub struct DecksCache {
    pub account_to_decks: HashMap<Pubkey, Decks>,
    pub fallback_pubkey: Pubkey,
}

impl Default for DecksCache {
    fn default() -> Self {
        let mut account_to_decks: HashMap<Pubkey, Decks> = Default::default();
        account_to_decks.insert(Pubkey::from_hex(FALLBACK_PUBKEY).unwrap(), Decks::default());
        DecksCache::new(account_to_decks)
    }
}

impl DecksCache {
    pub fn new(account_to_decks: HashMap<Pubkey, Decks>) -> Self {
        let fallback_pubkey = Pubkey::from_hex(FALLBACK_PUBKEY).unwrap();

        Self {
            account_to_decks,
            fallback_pubkey,
        }
    }

    pub fn new_with_demo_config(ndb: &Ndb) -> Self {
        let mut account_to_decks: HashMap<Pubkey, Decks> = Default::default();
        let fallback_pubkey = Pubkey::from_hex(FALLBACK_PUBKEY).unwrap();
        account_to_decks.insert(fallback_pubkey, demo_decks(fallback_pubkey, ndb));
        DecksCache::new(account_to_decks)
    }

    pub fn decks(&self, key: &Pubkey) -> &Decks {
        self.account_to_decks
            .get(key)
            .unwrap_or_else(|| panic!("{:?} not found", key))
    }

    pub fn decks_mut(&mut self, key: &Pubkey) -> &mut Decks {
        self.account_to_decks
            .get_mut(key)
            .unwrap_or_else(|| panic!("{:?} not found", key))
    }

    pub fn fallback_mut(&mut self) -> &mut Decks {
        self.account_to_decks
            .get_mut(&self.fallback_pubkey)
            .unwrap_or_else(|| panic!("fallback deck not found"))
    }

    pub fn add_deck_default(&mut self, key: Pubkey) {
        self.account_to_decks.insert(key, Decks::default());
        info!(
            "Adding new default deck for {:?}. New decks size is {}",
            key,
            self.account_to_decks.get(&key).unwrap().decks.len()
        );
    }

    pub fn add_decks(&mut self, key: Pubkey, decks: Decks) {
        self.account_to_decks.insert(key, decks);
        info!(
            "Adding new deck for {:?}. New decks size is {}",
            key,
            self.account_to_decks.get(&key).unwrap().decks.len()
        );
    }

    pub fn add_deck(&mut self, key: Pubkey, deck: Deck) {
        match self.account_to_decks.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let decks = entry.get_mut();
                decks.add_deck(deck);
                info!(
                    "Created new deck for {:?}. New number of decks is {}",
                    key,
                    decks.decks.len()
                );
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                info!("Created first deck for {:?}", key);
                entry.insert(Decks::new(deck));
            }
        }
    }

    pub fn remove_for(&mut self, key: &Pubkey) {
        info!("Removing decks for {:?}", key);
        self.account_to_decks.remove(key);
    }
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDecksCache {
    #[serde(serialize_with = "serialize_map", deserialize_with = "deserialize_map")]
    pub account_to_decks: HashMap<Pubkey, SerializableDecks>,
}

impl From<&DecksCache> for SerializableDecksCache {
    fn from(value: &DecksCache) -> Self {
        SerializableDecksCache {
            account_to_decks: value
                .account_to_decks
                .iter()
                .map(|(id, d)| (*id, d.into()))
                .collect(),
        }
    }
}

impl SerializableDecksCache {
    pub fn into_decks_cache(self, ndb: &nostrdb::Ndb) -> DecksCache {
        let account_to_decks = self
            .account_to_decks
            .into_iter()
            .map(|(id, s)| (id, s.into_decks(ndb, Some(id.bytes()))))
            .collect();
        DecksCache::new(account_to_decks)
    }
}

fn serialize_map<S>(
    map: &HashMap<Pubkey, SerializableDecks>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let stringified_map: HashMap<String, &SerializableDecks> =
        map.iter().map(|(k, v)| (k.hex(), v)).collect();
    stringified_map.serialize(serializer)
}

fn deserialize_map<'de, D>(deserializer: D) -> Result<HashMap<Pubkey, SerializableDecks>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let stringified_map: HashMap<String, SerializableDecks> = HashMap::deserialize(deserializer)?;

    stringified_map
        .into_iter()
        .map(|(k, v)| {
            let key = Pubkey::from_hex(&k).map_err(serde::de::Error::custom)?;
            Ok((key, v))
        })
        .collect()
}

#[derive(Serialize)]
pub struct Decks {
    active_deck: usize,
    removal_request: Option<usize>,
    decks: Vec<Deck>,
}

impl Default for Decks {
    fn default() -> Self {
        Decks::new(Deck::default())
    }
}

impl Decks {
    pub fn new(deck: Deck) -> Self {
        let decks = vec![deck];

        Decks {
            active_deck: 0,
            removal_request: None,
            decks,
        }
    }

    pub fn active(&self) -> &Deck {
        self.decks
            .get(self.active_deck)
            .expect("active_deck index was invalid")
    }

    pub fn active_mut(&mut self) -> &mut Deck {
        self.decks
            .get_mut(self.active_deck)
            .expect("active_deck index was invalid")
    }

    pub fn decks(&self) -> &Vec<Deck> {
        &self.decks
    }

    pub fn decks_mut(&mut self) -> &mut Vec<Deck> {
        &mut self.decks
    }

    pub fn add_deck(&mut self, deck: Deck) {
        self.decks.push(deck);
    }

    pub fn active_index(&self) -> usize {
        self.active_deck
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.decks.len() {
            self.active_deck = index;
        } else {
            error!(
                "requested deck change that is invalid. decks len: {}, requested index: {}",
                self.decks.len(),
                index
            );
        }
    }

    pub fn remove_deck(&mut self, index: usize) {
        if index < self.decks.len() {
            if self.decks.len() > 1 {
                self.decks.remove(index);

                let info_prefix = format!("Removed deck at index {}", index);
                match index.cmp(&self.active_deck) {
                    std::cmp::Ordering::Less => {
                        info!(
                            "{}. The active deck was index {}, now it is {}",
                            info_prefix,
                            self.active_deck,
                            self.active_deck - 1
                        );
                        self.active_deck -= 1
                    }
                    std::cmp::Ordering::Greater => {
                        info!(
                            "{}. Active deck remains at index {}.",
                            info_prefix, self.active_deck
                        )
                    }
                    std::cmp::Ordering::Equal => {
                        if index != 0 {
                            info!(
                                "{}. Active deck was index {}, now it is {}",
                                info_prefix,
                                self.active_deck,
                                self.active_deck - 1
                            );
                            self.active_deck -= 1;
                        } else {
                            info!(
                                "{}. Active deck remains at index {}.",
                                info_prefix, self.active_deck
                            )
                        }
                    }
                }
                self.removal_request = None;
            } else {
                error!("attempted unsucessfully to remove the last deck for this account");
            }
        } else {
            error!("index was out of bounds");
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SerializableDecks {
    active_deck: usize,
    removal_request: Option<usize>,
    decks: Vec<SerializableDeck>,
}

impl SerializableDecks {
    pub fn into_decks(self, ndb: &nostrdb::Ndb, deck_pubkey: Option<&[u8; 32]>) -> Decks {
        Decks {
            active_deck: self.active_deck,
            removal_request: self.removal_request,
            decks: self
                .decks
                .into_iter()
                .map(|d| d.into_deck(ndb, deck_pubkey))
                .collect(),
        }
    }
}

impl From<&Decks> for SerializableDecks {
    fn from(value: &Decks) -> Self {
        SerializableDecks {
            active_deck: value.active_deck,
            removal_request: value.removal_request,
            decks: value.decks.iter().map(|d| d.into()).collect(),
        }
    }
}

pub struct Deck {
    pub icon: char,
    pub name: String,
    columns: Columns,
}

impl Default for Deck {
    fn default() -> Self {
        let mut columns = Columns::default();
        columns.new_column_picker();
        Self {
            icon: '🇩',
            name: String::from("Default Deck"),
            columns,
        }
    }
}

impl Deck {
    pub fn new(icon: char, name: String) -> Self {
        let mut columns = Columns::default();
        columns.new_column_picker();
        Self {
            icon,
            name,
            columns,
        }
    }

    pub fn columns(&self) -> &Columns {
        &self.columns
    }

    pub fn columns_mut(&mut self) -> &mut Columns {
        &mut self.columns
    }

    pub fn edit(&mut self, changes: ConfigureDeckResponse) {
        self.name = changes.name;
        self.icon = changes.icon;
    }
}

#[derive(Serialize, Deserialize)]
struct SerializableDeck {
    pub icon: char,
    pub name: String,
    columns: SerializableColumns,
}

impl Serialize for Deck {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let helper = SerializableDeck {
            icon: self.icon,
            name: self.name.to_owned(),
            columns: (&self.columns).into(),
        };

        helper.serialize(serializer)
    }
}

impl From<&Deck> for SerializableDeck {
    fn from(value: &Deck) -> Self {
        SerializableDeck {
            icon: value.icon,
            name: value.name.to_owned(),
            columns: (&value.columns).into(),
        }
    }
}

impl SerializableDeck {
    pub fn into_deck(self, ndb: &nostrdb::Ndb, deck_pubkey: Option<&[u8; 32]>) -> Deck {
        Deck {
            icon: self.icon,
            name: self.name,
            columns: self.columns.into_columns(ndb, deck_pubkey),
        }
    }
}

pub fn demo_decks(demo_pubkey: Pubkey, ndb: &Ndb) -> Decks {
    let deck = {
        let mut columns = Columns::default();
        columns.add_column(Column::new(vec![
            Route::AddColumn(AddColumnRoute::Base),
            Route::Accounts(AccountsRoute::Accounts),
        ]));

        if let Some(timeline) =
            TimelineKind::contact_list(timeline::PubkeySource::Explicit(demo_pubkey))
                .into_timeline(ndb, Some(demo_pubkey.bytes()))
        {
            columns.add_new_timeline_column(timeline);
        }

        columns.add_new_timeline_column(Timeline::hashtag("introductions".to_string()));

        Deck {
            icon: '🇩',
            name: String::from("Demo Deck"),
            columns,
        }
    };

    Decks::new(deck)
}
