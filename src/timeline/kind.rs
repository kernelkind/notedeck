use crate::error::{Error, FilterError};
use crate::filter;
use crate::filter::FilterState;
use crate::timeline::Timeline;
use crate::ui::profile::preview::get_profile_displayname_string;
use enostr::{Filter, Pubkey};
use nostrdb::{Ndb, Transaction};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PubkeySource {
    Explicit(Pubkey),
    DeckAuthor,
}

impl ToString for PubkeySource {
    fn to_string(&self) -> String {
        let self_name = "pubkey_source";
        let sub_name = match &self {
            PubkeySource::Explicit(pubkey) => format!("explicit:{}", pubkey.hex()),
            PubkeySource::DeckAuthor => "deck_author".to_owned(),
        };

        format!("{}:{}", self_name, sub_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListKind {
    Contact(PubkeySource),
}

impl ToString for ListKind {
    fn to_string(&self) -> String {
        let self_name = "list";
        let kind_type = match &self {
            ListKind::Contact(pubkey_source) => format!("contact:{}", pubkey_source.to_string()),
        };
        format!("{}:{}", self_name, kind_type)
    }
}

///
/// What kind of timeline is it?
///   - Follow List
///   - Notifications
///   - DM
///   - filter
///   - ... etc
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineKind {
    List(ListKind),

    Notifications(PubkeySource),

    Profile(PubkeySource),

    Universe,

    /// Generic filter
    Generic,

    Hashtag(String),
}

impl TimelineKind {
    pub fn contact_list(pk: PubkeySource) -> Self {
        TimelineKind::List(ListKind::Contact(pk))
    }

    pub fn is_contacts(&self) -> bool {
        matches!(self, TimelineKind::List(ListKind::Contact(_)))
    }

    pub fn profile(pk: PubkeySource) -> Self {
        TimelineKind::Profile(pk)
    }

    pub fn is_notifications(&self) -> bool {
        matches!(self, TimelineKind::Notifications(_))
    }

    pub fn notifications(pk: PubkeySource) -> Self {
        TimelineKind::Notifications(pk)
    }

    pub fn into_timeline(self, ndb: &Ndb, default_user: Option<&[u8; 32]>) -> Option<Timeline> {
        match self {
            TimelineKind::Universe => Some(Timeline::new(
                TimelineKind::Universe,
                FilterState::ready(vec![Filter::new()
                    .kinds([1])
                    .limit(filter::default_limit())
                    .build()]),
            )),

            TimelineKind::Generic => {
                warn!("you can't convert a TimelineKind::Generic to a Timeline");
                None
            }

            TimelineKind::Profile(pk_src) => {
                let pk = match &pk_src {
                    PubkeySource::DeckAuthor => default_user?,
                    PubkeySource::Explicit(pk) => pk.bytes(),
                };

                let filter = Filter::new()
                    .authors([pk])
                    .kinds([1])
                    .limit(filter::default_limit())
                    .build();

                Some(Timeline::new(
                    TimelineKind::profile(pk_src),
                    FilterState::ready(vec![filter]),
                ))
            }

            TimelineKind::Notifications(pk_src) => {
                let pk = match &pk_src {
                    PubkeySource::DeckAuthor => default_user?,
                    PubkeySource::Explicit(pk) => pk.bytes(),
                };

                let notifications_filter = Filter::new()
                    .pubkeys([pk])
                    .kinds([1])
                    .limit(crate::filter::default_limit())
                    .build();

                Some(Timeline::new(
                    TimelineKind::notifications(pk_src),
                    FilterState::ready(vec![notifications_filter]),
                ))
            }

            TimelineKind::Hashtag(hashtag) => Some(Timeline::hashtag(hashtag)),

            TimelineKind::List(ListKind::Contact(pk_src)) => {
                let pk = match &pk_src {
                    PubkeySource::DeckAuthor => default_user?,
                    PubkeySource::Explicit(pk) => pk.bytes(),
                };

                let contact_filter = Filter::new().authors([pk]).kinds([3]).limit(1).build();

                let txn = Transaction::new(ndb).expect("txn");
                let results = ndb
                    .query(&txn, &[contact_filter.clone()], 1)
                    .expect("contact query failed?");

                if results.is_empty() {
                    return Some(Timeline::new(
                        TimelineKind::contact_list(pk_src),
                        FilterState::needs_remote(vec![contact_filter.clone()]),
                    ));
                }

                match Timeline::contact_list(&results[0].note, pk_src.clone()) {
                    Err(Error::Filter(FilterError::EmptyContactList)) => Some(Timeline::new(
                        TimelineKind::contact_list(pk_src),
                        FilterState::needs_remote(vec![contact_filter]),
                    )),
                    Err(e) => {
                        error!("Unexpected error: {e}");
                        None
                    }
                    Ok(tl) => Some(tl),
                }
            }
        }
    }

    pub fn to_title(&self, ndb: &Ndb) -> String {
        match self {
            TimelineKind::List(list_kind) => match list_kind {
                ListKind::Contact(pubkey_source) => match pubkey_source {
                    PubkeySource::Explicit(pubkey) => {
                        format!("{}'s Contacts", get_profile_displayname_string(ndb, pubkey))
                    }
                    PubkeySource::DeckAuthor => "Contacts".to_owned(),
                },
            },
            TimelineKind::Notifications(pubkey_source) => match pubkey_source {
                PubkeySource::DeckAuthor => "Notifications".to_owned(),
                PubkeySource::Explicit(pk) => format!(
                    "{}'s Notifications",
                    get_profile_displayname_string(ndb, pk)
                ),
            },
            TimelineKind::Profile(pubkey_source) => match pubkey_source {
                PubkeySource::DeckAuthor => "Profile".to_owned(),
                PubkeySource::Explicit(pk) => {
                    format!("{}'s Profile", get_profile_displayname_string(ndb, pk))
                }
            },
            TimelineKind::Universe => "Universe".to_owned(),
            TimelineKind::Generic => "Custom Filter".to_owned(),
            TimelineKind::Hashtag(hashtag) => format!("#{}", hashtag),
        }
    }
}
