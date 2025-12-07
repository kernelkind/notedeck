pub mod cache;
pub mod nip17;
pub mod ui;

use enostr::{ClientMessage, Pubkey, RelayEvent, RelayPool};
use hashbrown::HashMap;
use nostr::{
    nips::nip44,
    prelude::{EventBuilder, Keys, Kind, PublicKey, Tag, Timestamp, UnsignedEvent},
    secp256k1::rand::{rngs::OsRng, Rng},
    util::JsonUtil,
};
use nostrdb::{Filter, NoteBuilder, Transaction};
use notedeck::{try_process_events_core, Accounts, App, AppContext, AppResponse};

use crate::{
    cache::{ConversationCache, ConversationId, ConversationStates},
    nip17::{giftwrap_filter, remote_sub},
    ui::messages::{login_nsec_prompt, MessagesAction, MessagesUi},
};

pub struct MessagesApp {
    messages: ConversationsCtx,
    states: ConversationStates,
    subs: ConversationSubs,
}

impl MessagesApp {
    pub fn new(ctx: &AppContext) -> Self {
        Self {
            messages: ConversationsCtx::default(),
            subs: ConversationSubs::new(&ctx.accounts),
            states: ConversationStates::default(),
        }
    }
}

impl App for MessagesApp {
    fn update(&mut self, ctx: &mut AppContext<'_>, ui: &mut egui::Ui) -> AppResponse {
        try_process_events_messages(ctx, ui.ctx(), &self.subs.remotes);

        let Some(cache) = self.messages.get_current_mut(&ctx.accounts) else {
            login_nsec_prompt(ui);
            return AppResponse::none();
        };

        's: {
            if !self.subs.ensure_remote(ctx.pool, &ctx.accounts) {
                break 's;
            }

            let Some(secret) = &ctx.accounts.get_selected_account().key.secret_key else {
                break 's;
            };

            ctx.ndb.add_key(&secret.secret_bytes());
            let txn = Transaction::new(&ctx.ndb).expect("txn");
            ctx.ndb.process_giftwraps(&txn);
        }

        if !cache.initialized_convos {
            let txn = Transaction::new(&ctx.ndb).expect("txn");
            let selected_pubkey = ctx.accounts.selected_account_pubkey().clone();
            cache.init_conversations(
                &ctx.ndb,
                &txn,
                &selected_pubkey,
                &mut *ctx.note_cache,
                &mut *ctx.unknown_ids,
            );
            cache.initialized_convos = true;
        }

        let selected_pubkey = ctx.accounts.selected_account_pubkey();
        let action = MessagesUi::new(cache, &mut self.states, &ctx.ndb, selected_pubkey)
            .ui(ui, &mut *ctx.img_cache);
        if let Some(action) = action {
            handle_messages_action(action, ctx, cache);
        }
        AppResponse::none()
    }
}

fn handle_messages_action(
    action: MessagesAction,
    ctx: &mut AppContext<'_>,
    cache: &ConversationCache,
) {
    match action {
        MessagesAction::SendMessage {
            conversation_id,
            content,
        } => send_conversation_message(conversation_id, content, cache, ctx),
    }
}

/// Storage for conversations per account. Account management is performed by `Accounts`
#[derive(Default)]
struct ConversationsCtx {
    convos_per_acc: HashMap<Pubkey, ConversationCache>,
}

impl ConversationsCtx {
    /// Get the conversation cache for the selected account. Return None if we don't have a full kp
    pub fn get_current_mut(&mut self, accounts: &Accounts) -> Option<&mut ConversationCache> {
        if accounts
            .get_selected_account()
            .keypair()
            .secret_key
            .is_none()
        {
            return None;
        }

        let current = accounts.selected_account_pubkey();
        Some(
            self.convos_per_acc
                .raw_entry_mut()
                .from_key(current)
                .or_insert_with(|| (*current, ConversationCache::new()))
                .1,
        )
    }
}

struct ConversationSubs {
    acc: Pubkey,
    remotes: Vec<RemoteFilter>,
}

struct RemoteFilter {
    remote_id: String,
    filter: Vec<Filter>,
}

impl ConversationSubs {
    pub fn new(accounts: &Accounts) -> Self {
        Self {
            acc: accounts.selected_account_pubkey().clone(),
            remotes: Vec::new(),
        }
    }

    /// Ensure we are subscribed remotely
    /// return whether we switched & have nsec
    pub fn ensure_remote(&mut self, pool: &mut RelayPool, accounts: &Accounts) -> bool {
        if self.acc == *accounts.selected_account_pubkey()
            && accounts
                .get_selected_account()
                .keypair()
                .secret_key
                .is_some()
            && !self.remotes.is_empty()
        {
            return false;
        }

        if !self.remotes.is_empty() {
            let remotes = std::mem::take(&mut self.remotes);
            for remote in remotes {
                pool.unsubscribe(remote.remote_id.to_string());
            }
        }

        if accounts
            .get_selected_account()
            .keypair()
            .secret_key
            .is_none()
        {
            return false;
        }

        let filter = remote_filters(accounts.selected_account_pubkey());

        let s = filter
            .iter()
            .map(|f| f.json().unwrap())
            .collect::<Vec<String>>()
            .join(",");
        tracing::info!("Performing remote sub for giftwrap filter for: {s}");
        self.remotes = vec![RemoteFilter {
            remote_id: remote_sub(pool, filter.clone()),
            filter,
        }];

        true
    }
}

fn remote_filters(acc: &Pubkey) -> Vec<Filter> {
    vec![giftwrap_filter(&acc)]
}

fn send_conversation_message(
    conversation_id: ConversationId,
    content: String,
    cache: &ConversationCache,
    ctx: &mut AppContext<'_>,
) {
    if content.trim().is_empty() {
        return;
    }

    let Some(conversation) = cache.get(conversation_id) else {
        tracing::warn!("missing conversation {conversation_id} for send action");
        return;
    };

    let Some(filled) = ctx.accounts.selected_filled() else {
        tracing::warn!("cannot send message without a full keypair");
        return;
    };

    let sender_secret = filled.secret_key.clone();
    let sender_enostr_pk = *filled.pubkey;
    let Some(sender_pubkey) = nostr_public_key(filled.pubkey) else {
        tracing::error!("invalid sender pubkey for conversation {conversation_id}");
        return;
    };
    let sender_keys = Keys::new(sender_secret);
    let rumor = build_rumor_event(&content, &conversation.metadata.participants, sender_pubkey);

    let rumor_json = rumor.as_json();

    let mut rng = OsRng;
    let mut sent_any = false;
    for participant in &conversation.metadata.participants {
        if participant == &sender_enostr_pk {
            continue;
        }
        sent_any |= wrap_and_store_message(ctx, &mut rng, &sender_keys, participant, &rumor_json);
    }

    if sent_any {
        if let Ok(txn) = Transaction::new(&ctx.ndb) {
            ctx.ndb.process_giftwraps(&txn);
        }
    }
}

fn build_rumor_event(message: &str, participants: &[Pubkey], sender: PublicKey) -> UnsignedEvent {
    let mut tags = Vec::new();
    for participant in participants {
        if let Some(pk) = nostr_public_key(participant) {
            tags.push(Tag::public_key(pk));
        } else {
            tracing::warn!("invalid participant pubkey {}", participant);
        }
    }

    let builder = EventBuilder::new(Kind::PrivateDirectMessage, message)
        .custom_created_at(Timestamp::now())
        .tags(tags);
    builder.build(sender)
}

fn wrap_and_store_message(
    ctx: &mut AppContext<'_>,
    rng: &mut OsRng,
    sender_keys: &Keys,
    recipient: &Pubkey,
    rumor_json: &str,
) -> bool {
    let Some(recipient_pk) = nostr_public_key(recipient) else {
        tracing::warn!("failed to convert recipient pubkey {}", recipient);
        return false;
    };

    let encrypted_rumor = match nip44::encrypt_with_rng(
        rng,
        sender_keys.secret_key(),
        &recipient_pk,
        rumor_json,
        nip44::Version::V2,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::error!("failed to encrypt rumor for {recipient}: {err}");
            return false;
        }
    };

    let seal_event = match EventBuilder::new(Kind::Seal, encrypted_rumor)
        .custom_created_at(randomized_timestamp(rng))
        .sign_with_keys(sender_keys)
    {
        Ok(event) => event,
        Err(err) => {
            tracing::error!("failed to build seal for {recipient}: {err}");
            return false;
        }
    };

    let seal_json = seal_event.as_json();

    let wrap_keys = Keys::generate_with_rng(rng);
    let encrypted_seal = match nip44::encrypt_with_rng(
        rng,
        wrap_keys.secret_key(),
        &recipient_pk,
        &seal_json,
        nip44::Version::V2,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::error!("failed to encrypt seal for wrap: {err}");
            return false;
        }
    };

    let wrap_event = match EventBuilder::new(Kind::GiftWrap, encrypted_seal)
        .custom_created_at(randomized_timestamp(rng))
        .tags([Tag::public_key(recipient_pk)])
        .sign_with_keys(&wrap_keys)
    {
        Ok(event) => event,
        Err(err) => {
            tracing::error!("failed to build giftwrap event: {err}");
            return false;
        }
    };

    let wrap_json = &wrap_event.as_json();

    if let Err(e) = ctx.ndb.process_client_event(&wrap_json) {
        tracing::error!("failed to ingest giftwrap into ndb: {e:?}");
    }

    match ClientMessage::event_json(wrap_json.clone()) {
        Ok(msg) => ctx.pool.send(&msg),
        Err(err) => tracing::error!("failed to build client message: {err}"),
    };

    true
}

fn nostr_public_key(pk: &Pubkey) -> Option<PublicKey> {
    PublicKey::from_slice(pk.bytes()).ok()
}

fn randomized_timestamp(rng: &mut OsRng) -> Timestamp {
    const MAX_SKEW_SECS: u64 = 2 * 24 * 60 * 60;
    let mut secs = Timestamp::now().as_u64();
    let tweak = rng.gen_range(0..MAX_SKEW_SECS);
    secs = secs.saturating_sub(tweak);
    Timestamp::from_secs(secs)
}

fn try_process_events_messages(
    app_ctx: &mut AppContext,
    ctx: &egui::Context,
    filters: &Vec<RemoteFilter>,
) {
    try_process_events_core(app_ctx, ctx, |app_ctx, ev| {
        if let RelayEvent::Opened = (&ev.event).into() {
            for remote_filter in filters {
                app_ctx.pool.send_to(
                    &enostr::ClientMessage::Req {
                        sub_id: remote_filter.remote_id.clone(),
                        filters: remote_filter.filter.clone(),
                    },
                    &ev.relay,
                );
            }
        }
    });
}
