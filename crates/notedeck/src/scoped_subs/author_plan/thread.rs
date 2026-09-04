use enostr::{NormRelayUrl, NoteId, Pubkey, RelayUrlSource};
use futures_util::Stream;
use hashbrown::{HashMap, HashSet};
use nostrdb::{Error, Filter, Ndb, SubscriptionStream};
use std::{
    pin::Pin,
    task::{Context, Poll},
};

use super::{
    send_routed_relays, PlannedRoutedRelay, SendAuthorOutboxPlanConfig,
    SendAuthorOutboxPlanJobResult, SendPlanFilter, SendPlannedRoutedRelay,
};
use crate::author_outbox::{
    thread::ThreadSnapshot, RelayDirectoryRead, RelayDirectorySnapshot, RelayDirectoryState,
    RoutedRelayPriority,
};

/// Data read by one thread-planning job, used to request and watch missing ancestry.
#[derive(Debug, Default)]
pub(super) struct ThreadPlanSnapshot {
    /// Exact thread IDs encountered by this job, including missing ancestors.
    pub(super) note_ids: HashSet<NoteId>,
    /// Authors whose relay-list changes can affect these routes.
    pub(super) authors: HashSet<Pubkey>,
    /// Exact IDs that the account-read baseline still needs to fetch.
    pub(super) missing_ids: HashSet<NoteId>,
}

/// Runtime-owned subscription retained across planning jobs.
/// The stream queues arrivals during a job and unsubscribes when dropped.
pub(super) struct ThreadWatch {
    note_ids: HashSet<NoteId>,
    authors: HashSet<Pubkey>,
    stream: SubscriptionStream,
}

impl ThreadWatch {
    /// Subscribe before the runtime schedules the job that catches up this coverage.
    pub(super) fn new(
        ndb: &Ndb,
        note_ids: HashSet<NoteId>,
        authors: HashSet<Pubkey>,
    ) -> Result<Self, Error> {
        if note_ids.is_empty() {
            return Err(Error::SubscriptionError);
        }
        let mut filters = vec![Filter::new()
            .ids(note_ids.iter().map(NoteId::bytes))
            .build()];
        if !authors.is_empty() {
            filters.push(
                Filter::new()
                    .authors(authors.iter().map(Pubkey::bytes))
                    .kinds([10002])
                    .build(),
            );
        }
        let stream = ndb.subscribe(&filters)?.stream(ndb).notes_per_await(64);
        Ok(Self {
            note_ids,
            authors,
            stream,
        })
    }

    /// Keep the existing subscription and its queued arrivals when coverage suffices.
    pub(super) fn covers(&self, note_ids: &HashSet<NoteId>, authors: &HashSet<Pubkey>) -> bool {
        note_ids.is_subset(&self.note_ids) && authors.is_subset(&self.authors)
    }
}

impl Stream for ThreadWatch {
    type Item = ();

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.stream)
            .poll_next(cx)
            .map(|batch| batch.map(|_| ()))
    }
}

/// Build thread routes from available ancestors, relay hints, and author lists.
///
/// Context authors select relays without narrowing the requested event authors.
/// Available routes are returned even when other authors still need discovery.
#[profiling::function]
pub(super) fn build_thread_plan(
    ndb: Ndb,
    input: SendAuthorOutboxPlanConfig,
    seeds: HashSet<NoteId>,
) -> SendAuthorOutboxPlanJobResult {
    let snapshot = match ThreadSnapshot::load(&ndb, &seeds) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            return SendAuthorOutboxPlanJobResult {
                live_routed_relays: Vec::new(),
                full_history_routed_relays: Vec::new(),
                missing_authors: HashSet::new(),
                thread: Some(Err(err)),
            };
        }
    };
    let directory = RelayDirectorySnapshot::from_ndb_authors(&ndb, &snapshot.authors);
    let missing_authors = directory.missing_authors(&snapshot.authors);
    let mut relays = snapshot.relays;
    for author in &snapshot.authors {
        if let RelayDirectoryState::Known(author_relays) = directory.author_state(author) {
            relays.extend(author_relays.iter().cloned());
        }
    }
    relays.retain(|relay| {
        !input.account_read_relays.contains(relay)
            && relay.allowed_for_source(RelayUrlSource::RemoteAdvertised)
    });
    let mut relays = relays.into_iter().collect::<Vec<_>>();
    relays.sort_unstable();

    let mut missing_ids = snapshot.missing_ids.iter().copied().collect::<Vec<_>>();
    missing_ids.sort_unstable_by_key(|id| *id.bytes());
    let missing_filter = (!missing_ids.is_empty()).then(|| {
        Filter::new()
            .ids(missing_ids.iter().map(NoteId::bytes))
            .build()
    });

    SendAuthorOutboxPlanJobResult {
        live_routed_relays: thread_routes(&relays, &input.live_filters, missing_filter.as_ref()),
        full_history_routed_relays: thread_routes(&relays, &input.full_history_filters, None),
        missing_authors,
        thread: Some(Ok(ThreadPlanSnapshot {
            note_ids: snapshot.note_ids,
            authors: snapshot.authors,
            missing_ids: snapshot.missing_ids,
        })),
    }
}

/// Copy unchanged thread filters to each relay, retaining filter membership.
///
/// Routed live state uses map membership to distinguish demand from a removed
/// route. Empty author sets suffice because these filters remain unmodified;
/// context authors only select relays and need not be copied to every route.
fn thread_routes(
    relays: &[NormRelayUrl],
    filters: &[SendPlanFilter],
    missing_filter: Option<&Filter>,
) -> Vec<SendPlannedRoutedRelay> {
    if filters.is_empty() && missing_filter.is_none() {
        return Vec::new();
    }

    let mut filters = filters.iter().collect::<Vec<_>>();
    filters.sort_unstable_by_key(|filter| filter.filter_index);
    let mut route_filters = filters
        .iter()
        .map(|filter| filter.filter.as_filter().clone())
        .collect::<Vec<_>>();
    let mut authors_by_filter_index = filters
        .iter()
        .map(|filter| (filter.filter_index, HashSet::new()))
        .collect::<HashMap<usize, HashSet<Pubkey>>>();
    if let Some(missing) = missing_filter {
        let filter_index = filters.last().map_or(0, |filter| filter.filter_index + 1);
        route_filters.push(missing.clone());
        authors_by_filter_index.insert(filter_index, HashSet::new());
    }

    let routes = relays
        .iter()
        .enumerate()
        .map(|(order, relay)| PlannedRoutedRelay {
            relay: relay.clone(),
            // Every route carries the same thread filters, so each contributes
            // one thread's relay coverage regardless of its context authors.
            relay_priority: RoutedRelayPriority {
                connection_weight: 1,
                order,
            },
            filters: route_filters.clone(),
            authors_by_filter_index: authors_by_filter_index.clone(),
        })
        .collect();
    let mut routes = send_routed_relays(routes);
    for route in &mut routes {
        route
            .authors_by_filter_index
            .sort_unstable_by_key(|(filter_index, _)| *filter_index);
    }
    routes
}
