use std::{collections::HashMap, str::FromStr};

use enostr::{NoteId, Pubkey};
use nostrdb::Ndb;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    accounts::AccountsRoute,
    column::{Column, Columns},
    decks::{Deck, Decks, DecksCache},
    route::Route,
    timeline::{kind::ListKind, PubkeySource, TimelineKind, TimelineRoute},
    ui::add_column::AddColumnRoute,
    Error,
};

use super::{write_file, DataPath, DataPathType, Directory};

static DECKS_CACHE_FILE: &str = "decks_cache.json";

pub fn load_decks_cache(path: &DataPath, ndb: &Ndb) -> Option<DecksCache> {
    let data_path = path.path(DataPathType::Setting);

    let decks_cache_str = match Directory::new(data_path).get_file(DECKS_CACHE_FILE.to_owned()) {
        Ok(s) => s,
        Err(e) => {
            error!(
                "Could not read decks cache from file {}:  {}",
                DECKS_CACHE_FILE, e
            );
            return None;
        }
    };

    let serializable_decks_cache =
        serde_json::from_str::<SerializableDecksCache>(&decks_cache_str).ok()?;

    serializable_decks_cache.to_decks_cache(ndb).ok()
}

pub fn save_decks_cache(path: &DataPath, decks_cache: &DecksCache) {
    let serialized_decks_cache =
        match serde_json::to_string(&SerializableDecksCache::to_serializable(decks_cache)) {
            Ok(s) => s,
            Err(e) => {
                error!("Could not serialize decks cache: {}", e);
                return;
            }
        };

    let data_path = path.path(DataPathType::Setting);

    if let Err(e) = write_file(
        &data_path,
        DECKS_CACHE_FILE.to_string(),
        &serialized_decks_cache,
    ) {
        error!(
            "Could not write decks cache to file {}: {}",
            DECKS_CACHE_FILE, e
        );
    } else {
        info!("Successfully wrote decks cache to {}", DECKS_CACHE_FILE);
    }
}

#[derive(Serialize, Deserialize)]
struct SerializableDecksCache {
    #[serde(serialize_with = "serialize_map", deserialize_with = "deserialize_map")]
    decks_cache: HashMap<Pubkey, SerializableDecks>,
}

impl SerializableDecksCache {
    fn to_serializable(decks_cache: &DecksCache) -> Self {
        SerializableDecksCache {
            decks_cache: decks_cache
                .account_to_decks
                .iter()
                .map(|(k, v)| (k.clone(), SerializableDecks::from_decks(v)))
                .collect(),
        }
    }

    pub fn to_decks_cache(self, ndb: &Ndb) -> Result<DecksCache, Error> {
        let account_to_decks = self
            .decks_cache
            .into_iter()
            .map(|(pubkey, serializable_decks)| {
                let deck_key = pubkey.bytes();
                serializable_decks
                    .to_decks(ndb, deck_key)
                    .map(|decks| (pubkey, decks))
            })
            .collect::<Result<HashMap<Pubkey, Decks>, Error>>()?;

        Ok(DecksCache::new(account_to_decks))
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

#[derive(Serialize, Deserialize)]
struct SerializableDecks {
    active_deck: usize,
    decks: Vec<SerializableDeck>,
}

impl SerializableDecks {
    pub fn from_decks(decks: &Decks) -> Self {
        Self {
            active_deck: decks.active_index(),
            decks: decks
                .decks()
                .iter()
                .map(|d| SerializableDeck::from_deck(d))
                .collect(),
        }
    }

    fn to_decks(self, ndb: &Ndb, deck_key: &[u8; 32]) -> Result<Decks, Error> {
        Ok(Decks::new2(
            self.active_deck,
            self.decks
                .into_iter()
                .map(|d| d.to_deck(ndb, deck_key))
                .collect::<Result<_, _>>()?,
        ))
    }
}

#[derive(Serialize, Deserialize)]
struct SerializableDeck {
    icon: String,
    name: String,
    columns: Vec<Vec<String>>,
}

impl SerializableDeck {
    pub fn from_deck(deck: &Deck) -> Self {
        let icon = deck.icon.to_string();
        let name = deck.name.clone();
        let columns = serialize_columns(deck.columns());

        SerializableDeck {
            icon,
            name,
            columns,
        }
    }

    pub fn to_deck(self, ndb: &Ndb, deck_user: &[u8; 32]) -> Result<Deck, Error> {
        let columns = deserialize_columns(ndb, deck_user, self.columns);
        Ok(Deck::new_with_columns(
            self.icon
                .parse::<char>()
                .map_err(|_| Error::Generic("could not convert String -> char".to_owned()))?,
            self.name,
            columns,
        ))
    }
}

fn serialize_columns(columns: &Columns) -> Vec<Vec<String>> {
    let mut cols_serialized: Vec<Vec<String>> = Vec::new();

    for column in columns.columns() {
        let mut column_routes = Vec::new();
        for route in column.router().routes() {
            if let Some(route_str) = serialize_route(route, columns) {
                column_routes.push(route_str);
            }
        }
        cols_serialized.push(column_routes);
    }

    cols_serialized
}

fn deserialize_columns(ndb: &Ndb, deck_user: &[u8; 32], serialized: Vec<Vec<String>>) -> Columns {
    let mut cols = Columns::new();
    for serialized_routes in serialized {
        let mut cur_routes = Vec::new();
        let mut cur_timeline = None;
        for serialized_route in serialized_routes {
            let selections = Selection::from_serialized(&serialized_route);
            if let Some(route_intermediary) = selections_to_route(selections) {
                match route_intermediary {
                    RouteIntermediary::ToTimeline(timeline_kind) => {
                        if let Some(timeline) = timeline_kind.into_timeline(ndb, Some(&deck_user)) {
                            cur_timeline = Some(timeline);
                        }
                    }
                    RouteIntermediary::ToRoute(route) => cur_routes.push(route),
                }
            }
        }

        if let Some(timeline) = cur_timeline {
            cols.timeline_with_routes(timeline, cur_routes);
        } else {
            cols.add_column(Column::new(cur_routes));
        }
    }

    cols
}

#[derive(Clone)]
enum Selection {
    Keyword(Keyword),
    Payload(String),
}

#[derive(Clone, PartialEq)]
enum Keyword {
    Notifs,
    Universe,
    Contact,
    Explicit,
    DeckAuthor,
    Profile,
    Hashtag,
    Generic,
    Thread,
    Reply,
    Quote,
    Account,
    Show,
    New,
    Relay,
    Compose,
    Column,
    NotificationSelection,
    ExternalNotifSelection,
    HashtagSelection,
    Support,
    Deck,
    Edit,
}

impl Keyword {
    const MAPPING: &'static [(&'static str, Keyword, bool)] = &[
        ("notifs", Keyword::Notifs, false),
        ("universe", Keyword::Universe, false),
        ("contact", Keyword::Contact, false),
        ("explicit", Keyword::Explicit, true),
        ("deck_author", Keyword::DeckAuthor, false),
        ("profile", Keyword::Profile, true),
        ("hashtag", Keyword::Hashtag, true),
        ("generic", Keyword::Generic, false),
        ("thread", Keyword::Thread, true),
        ("reply", Keyword::Reply, true),
        ("quote", Keyword::Quote, true),
        ("account", Keyword::Account, false),
        ("show", Keyword::Show, false),
        ("new", Keyword::New, false),
        ("relay", Keyword::Relay, false),
        ("compose", Keyword::Compose, false),
        ("column", Keyword::Column, false),
        (
            "notification_selection",
            Keyword::NotificationSelection,
            false,
        ),
        (
            "external_notif_selection",
            Keyword::ExternalNotifSelection,
            false,
        ),
        ("hashtag_selection", Keyword::HashtagSelection, false),
        ("support", Keyword::Support, false),
        ("deck", Keyword::Deck, false),
        ("edit", Keyword::Edit, true),
    ];

    fn has_payload(&self) -> bool {
        Keyword::MAPPING
            .iter()
            .find(|(_, keyword, _)| keyword == self)
            .map(|(_, _, has_payload)| *has_payload)
            .unwrap_or(false)
    }
}

impl ToString for Keyword {
    fn to_string(&self) -> String {
        Keyword::MAPPING
            .iter()
            .find(|(_, keyword, _)| keyword == self)
            .map(|(name, _, _)| *name)
            .expect("MAPPING is incorrect")
            .to_string()
    }
}

impl FromStr for Keyword {
    type Err = Error;

    fn from_str(serialized: &str) -> Result<Self, Self::Err> {
        Keyword::MAPPING
            .iter()
            .find(|(name, _, _)| *name == serialized)
            .map(|(_, keyword, _)| keyword.clone())
            .ok_or(Error::Generic(
                "Could not convert string to Keyword enum".to_owned(),
            ))
    }
}

enum RouteIntermediary {
    ToTimeline(TimelineKind),
    ToRoute(Route),
}

// TODO: The public-accessible version will be a subset of this
fn serialize_route(route: &Route, columns: &Columns) -> Option<String> {
    let mut selections: Vec<Selection> = Vec::new();
    match route {
        Route::Timeline(timeline_route) => match timeline_route {
            TimelineRoute::Timeline(timeline_id) => {
                if let Some(timeline) = columns.find_timeline(*timeline_id) {
                    match &timeline.kind {
                        TimelineKind::List(list_kind) => match list_kind {
                            ListKind::Contact(pubkey_source) => {
                                selections.push(Selection::Keyword(Keyword::Contact));
                                selections.extend(generate_pubkey_selections(pubkey_source));
                            }
                        },
                        TimelineKind::Notifications(pubkey_source) => {
                            selections.push(Selection::Keyword(Keyword::Notifs));
                            selections.extend(generate_pubkey_selections(pubkey_source));
                        }
                        TimelineKind::Profile(pubkey_source) => {
                            selections.push(Selection::Keyword(Keyword::Profile));
                            selections.extend(generate_pubkey_selections(pubkey_source));
                        }
                        TimelineKind::Universe => {
                            selections.push(Selection::Keyword(Keyword::Universe))
                        }
                        TimelineKind::Generic => {
                            selections.push(Selection::Keyword(Keyword::Generic))
                        }
                        TimelineKind::Hashtag(hashtag) => {
                            selections.push(Selection::Keyword(Keyword::Hashtag));
                            selections.push(Selection::Payload(hashtag.to_string()));
                        }
                    }
                }
            }
            TimelineRoute::Thread(note_id) => {
                selections.push(Selection::Keyword(Keyword::Thread));
                selections.push(Selection::Payload(note_id.hex()));
            }
            TimelineRoute::Profile(pubkey) => {
                selections.push(Selection::Keyword(Keyword::Profile));
                selections.push(Selection::Keyword(Keyword::Explicit));
                selections.push(Selection::Payload(pubkey.hex()));
            }
            TimelineRoute::Reply(note_id) => {
                selections.push(Selection::Keyword(Keyword::Reply));
                selections.push(Selection::Payload(note_id.hex()));
            }
            TimelineRoute::Quote(note_id) => {
                selections.push(Selection::Keyword(Keyword::Quote));
                selections.push(Selection::Payload(note_id.hex()));
            }
        },
        Route::Accounts(accounts_route) => {
            selections.push(Selection::Keyword(Keyword::Account));
            match accounts_route {
                AccountsRoute::Accounts => selections.push(Selection::Keyword(Keyword::Show)),
                AccountsRoute::AddAccount => selections.push(Selection::Keyword(Keyword::New)),
            }
        }
        Route::Relays => selections.push(Selection::Keyword(Keyword::Relay)),
        Route::ComposeNote => selections.push(Selection::Keyword(Keyword::Compose)),
        Route::AddColumn(add_column_route) => {
            selections.push(Selection::Keyword(Keyword::Column));
            match add_column_route {
                AddColumnRoute::Base => (),
                AddColumnRoute::UndecidedNotification => {
                    selections.push(Selection::Keyword(Keyword::NotificationSelection))
                }
                AddColumnRoute::ExternalNotification => {
                    selections.push(Selection::Keyword(Keyword::ExternalNotifSelection))
                }
                AddColumnRoute::Hashtag => {
                    selections.push(Selection::Keyword(Keyword::HashtagSelection))
                }
            }
        }
        Route::Support => selections.push(Selection::Keyword(Keyword::Support)),
        Route::NewDeck => {
            selections.push(Selection::Keyword(Keyword::Deck));
            selections.push(Selection::Keyword(Keyword::New));
        }
        Route::EditDeck(index) => {
            selections.push(Selection::Keyword(Keyword::Deck));
            selections.push(Selection::Keyword(Keyword::Edit));
            selections.push(Selection::Payload(index.to_string()));
        }
    }

    if selections.is_empty() {
        None
    } else {
        Some(
            selections
                .iter()
                .map(|k| k.to_string())
                .collect::<Vec<String>>()
                .join(":"),
        )
    }
}

fn generate_pubkey_selections(source: &PubkeySource) -> Vec<Selection> {
    let mut selections = Vec::new();
    match source {
        PubkeySource::Explicit(pubkey) => {
            selections.push(Selection::Keyword(Keyword::Explicit));
            selections.push(Selection::Payload(pubkey.hex()));
        }
        PubkeySource::DeckAuthor => {
            selections.push(Selection::Keyword(Keyword::DeckAuthor));
        }
    }
    selections
}

impl Selection {
    fn from_serialized(serialized: &str) -> Vec<Self> {
        let mut selections = Vec::new();
        let seperator = ":";

        let mut serialized_copy = serialized.to_string();
        let mut buffer = serialized_copy.as_mut();

        let mut next_is_payload = false;
        while let Some(index) = buffer.find(seperator) {
            if next_is_payload {
                let payload = &buffer[index + 1..];
                selections.push(Selection::Payload(payload.to_string()));
                next_is_payload = false;
            } else if let Ok(keyword) = Keyword::from_str(&buffer[..index]) {
                selections.push(Selection::Keyword(keyword.clone()));
                if keyword.has_payload() {
                    next_is_payload = true;
                }
            }

            buffer = &mut buffer[index + seperator.len()..];
        }

        return selections;
    }
}

fn selections_to_route(selections: Vec<Selection>) -> Option<RouteIntermediary> {
    match selections.get(0)? {
        Selection::Keyword(Keyword::Contact) => match selections.get(1)? {
            Selection::Keyword(Keyword::Explicit) => {
                if let Selection::Payload(hex) = selections.get(2)? {
                    Some(RouteIntermediary::ToTimeline(TimelineKind::contact_list(
                        PubkeySource::Explicit(Pubkey::from_hex(hex.as_str()).ok()?),
                    )))
                } else {
                    None
                }
            }
            Selection::Keyword(Keyword::DeckAuthor) => Some(RouteIntermediary::ToTimeline(
                TimelineKind::contact_list(PubkeySource::DeckAuthor),
            )),
            _ => None,
        },
        Selection::Keyword(Keyword::Notifs) => match selections.get(1)? {
            Selection::Keyword(Keyword::Explicit) => {
                if let Selection::Payload(hex) = selections.get(2)? {
                    Some(RouteIntermediary::ToTimeline(TimelineKind::notifications(
                        PubkeySource::Explicit(Pubkey::from_hex(hex.as_str()).ok()?),
                    )))
                } else {
                    None
                }
            }
            Selection::Keyword(Keyword::DeckAuthor) => Some(RouteIntermediary::ToTimeline(
                TimelineKind::notifications(PubkeySource::DeckAuthor),
            )),
            _ => None,
        },
        Selection::Keyword(Keyword::Profile) => match selections.get(1)? {
            Selection::Keyword(Keyword::Explicit) => {
                if let Selection::Payload(hex) = selections.get(2)? {
                    Some(RouteIntermediary::ToTimeline(TimelineKind::profile(
                        PubkeySource::Explicit(Pubkey::from_hex(hex.as_str()).ok()?),
                    )))
                } else {
                    None
                }
            }
            Selection::Keyword(Keyword::DeckAuthor) => Some(RouteIntermediary::ToTimeline(
                TimelineKind::profile(PubkeySource::DeckAuthor),
            )),
            _ => None,
        },
        Selection::Keyword(Keyword::Universe) => {
            Some(RouteIntermediary::ToTimeline(TimelineKind::Universe))
        }
        Selection::Keyword(Keyword::Hashtag) => {
            if let Selection::Payload(hashtag) = selections.get(1)? {
                Some(RouteIntermediary::ToTimeline(TimelineKind::Hashtag(
                    hashtag.to_string(),
                )))
            } else {
                None
            }
        }
        Selection::Keyword(Keyword::Generic) => {
            Some(RouteIntermediary::ToTimeline(TimelineKind::Generic))
        }
        Selection::Keyword(Keyword::Thread) => {
            if let Selection::Payload(hex) = selections.get(1)? {
                Some(RouteIntermediary::ToRoute(Route::thread(
                    NoteId::from_hex(hex.as_str()).ok()?,
                )))
            } else {
                None
            }
        }
        Selection::Keyword(Keyword::Reply) => {
            if let Selection::Payload(hex) = selections.get(1)? {
                Some(RouteIntermediary::ToRoute(Route::reply(
                    NoteId::from_hex(hex.as_str()).ok()?,
                )))
            } else {
                None
            }
        }
        Selection::Keyword(Keyword::Quote) => {
            if let Selection::Payload(hex) = selections.get(1)? {
                Some(RouteIntermediary::ToRoute(Route::quote(
                    NoteId::from_hex(hex.as_str()).ok()?,
                )))
            } else {
                None
            }
        }
        Selection::Keyword(Keyword::Account) => match selections.get(1)? {
            Selection::Keyword(Keyword::Show) => Some(RouteIntermediary::ToRoute(Route::Accounts(
                AccountsRoute::Accounts,
            ))),
            Selection::Keyword(Keyword::New) => Some(RouteIntermediary::ToRoute(Route::Accounts(
                AccountsRoute::AddAccount,
            ))),
            _ => None,
        },
        Selection::Keyword(Keyword::Relay) => Some(RouteIntermediary::ToRoute(Route::Relays)),
        Selection::Keyword(Keyword::Compose) => {
            Some(RouteIntermediary::ToRoute(Route::ComposeNote))
        }
        Selection::Keyword(Keyword::Column) => match selections.get(1)? {
            Selection::Keyword(Keyword::NotificationSelection) => Some(RouteIntermediary::ToRoute(
                Route::AddColumn(AddColumnRoute::UndecidedNotification),
            )),
            Selection::Keyword(Keyword::ExternalNotifSelection) => Some(
                RouteIntermediary::ToRoute(Route::AddColumn(AddColumnRoute::ExternalNotification)),
            ),
            Selection::Keyword(Keyword::HashtagSelection) => Some(RouteIntermediary::ToRoute(
                Route::AddColumn(AddColumnRoute::Hashtag),
            )),
            _ => None,
        },
        Selection::Keyword(Keyword::Support) => Some(RouteIntermediary::ToRoute(Route::Support)),
        Selection::Keyword(Keyword::Deck) => match selections.get(1)? {
            Selection::Keyword(Keyword::New) => Some(RouteIntermediary::ToRoute(Route::NewDeck)),
            Selection::Keyword(Keyword::Edit) => {
                if let Selection::Payload(index_str) = selections.get(2)? {
                    let parsed_index = index_str.parse::<usize>().ok()?;
                    Some(RouteIntermediary::ToRoute(Route::EditDeck(parsed_index)))
                } else {
                    None
                }
            }
            _ => None,
        },
        Selection::Payload(_)
        | Selection::Keyword(Keyword::Explicit)
        | Selection::Keyword(Keyword::New)
        | Selection::Keyword(Keyword::DeckAuthor)
        | Selection::Keyword(Keyword::Show)
        | Selection::Keyword(Keyword::NotificationSelection)
        | Selection::Keyword(Keyword::ExternalNotifSelection)
        | Selection::Keyword(Keyword::HashtagSelection)
        | Selection::Keyword(Keyword::Edit) => None,
    }
}

impl ToString for Selection {
    fn to_string(&self) -> String {
        match &self {
            Selection::Keyword(keyword) => keyword.to_string(),
            Selection::Payload(payload) => payload.to_string(),
        }
    }
}
