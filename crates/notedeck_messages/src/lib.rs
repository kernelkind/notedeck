pub mod cache;
pub mod nip17;
pub mod ui;

use enostr::{ClientMessage, FullKeypair, Pubkey, RelayEvent, RelayPool, SecretKey};
use hashbrown::HashMap;
use nostr::{
    nips::nip44,
    prelude::{EventBuilder, Kind, PublicKey, Tag},
    secp256k1::rand::{rngs::OsRng, Rng},
    JsonUtil,
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
            if let Some(first) = cache.first_convo_id() {
                cache.open_conversation(&ctx.ndb, &txn, first, ctx.note_cache, ctx.unknown_ids);
            }
            cache.initialized_convos = true;
        }

        's: {
            let Some(active_convo) = self.states.active else {
                break 's;
            };

            let txn = Transaction::new(&ctx.ndb).expect("txn");
            cache.check_for_updates(
                &ctx.ndb,
                &txn,
                active_convo,
                ctx.note_cache,
                ctx.unknown_ids,
            );
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

    let Some(selected_kp) = ctx.accounts.selected_filled() else {
        tracing::warn!("cannot send message without a full keypair");
        return;
    };

    let Some(rumor_json) = build_rumor_json(
        &content,
        &conversation.metadata.participants,
        selected_kp.pubkey,
    ) else {
        tracing::error!("failed to build rumor for conversation {conversation_id}");
        return;
    };

    let Some(sender_secret) = ctx.accounts.selected_filled().map(|f| f.secret_key) else {
        return;
    };

    let mut rng = OsRng;
    for participant in &conversation.metadata.participants {
        let Some(giftwrap_json) =
            giftwrap_message(&mut rng, sender_secret, participant, &rumor_json)
        else {
            continue;
        };
        if participant == selected_kp.pubkey {
            if let Err(e) = ctx.ndb.process_client_event(&giftwrap_json) {
                tracing::error!("Could not ingest event: {e:?}");
            }
        } else {
            match ClientMessage::event_json(giftwrap_json.clone()) {
                Ok(msg) => ctx.pool.send(&msg),
                Err(err) => tracing::error!("failed to build client message: {err}"),
            };
        }
    }
}

fn build_rumor_json(
    message: &str,
    participants: &[Pubkey],
    sender_pubkey: &Pubkey,
) -> Option<String> {
    let sender = nostrcrate_pk(sender_pubkey)?;
    let mut tags = Vec::new();
    for participant in participants {
        if let Some(pk) = nostrcrate_pk(participant) {
            tags.push(Tag::public_key(pk));
        } else {
            tracing::warn!("invalid participant {}", participant);
        }
    }

    let builder = EventBuilder::new(Kind::PrivateDirectMessage, message).tags(tags);
    Some(builder.build(sender).as_json())
}

fn giftwrap_message(
    rng: &mut OsRng,
    sender_secret: &SecretKey,
    recipient: &Pubkey,
    rumor_json: &str,
) -> Option<String> {
    let Some(recipient_pk) = nostrcrate_pk(recipient) else {
        tracing::warn!("failed to convert recipient pubkey {}", recipient);
        return None;
    };

    let encrypted_rumor = match nip44::encrypt_with_rng(
        rng,
        sender_secret,
        &recipient_pk,
        rumor_json,
        nip44::Version::V2,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::error!("failed to encrypt rumor for {recipient}: {err}");
            return None;
        }
    };

    let seal_created = randomized_timestamp(rng);
    let Some(seal_json) = build_seal_json(&encrypted_rumor, sender_secret, seal_created) else {
        tracing::error!("failed to build seal for recipient {}", recipient);
        return None;
    };

    let wrap_keys = FullKeypair::generate();
    let encrypted_seal = match nip44::encrypt_with_rng(
        rng,
        &wrap_keys.secret_key,
        &recipient_pk,
        &seal_json,
        nip44::Version::V2,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::error!("failed to encrypt seal for wrap: {err}");
            return None;
        }
    };

    let wrap_created = randomized_timestamp(rng);
    build_giftwrap_json(&encrypted_seal, &wrap_keys, recipient, wrap_created)
}

fn build_seal_json(
    content_ciphertext: &str,
    sender_secret: &SecretKey,
    created_at: u64,
) -> Option<String> {
    let builder = NoteBuilder::new()
        .kind(13)
        .content(content_ciphertext)
        .created_at(created_at);

    builder
        .sign(&sender_secret.secret_bytes())
        .build()?
        .json()
        .ok()
}

fn build_giftwrap_json(
    content: &str,
    wrap_keys: &FullKeypair,
    recipient: &Pubkey,
    created_at: u64,
) -> Option<String> {
    let builder = NoteBuilder::new()
        .kind(1059)
        .content(content)
        .created_at(created_at)
        .start_tag()
        .tag_str("p")
        .tag_str(&recipient.hex());

    builder
        .sign(&wrap_keys.secret_key.secret_bytes())
        .build()?
        .json()
        .ok()
}

fn nostrcrate_pk(pk: &Pubkey) -> Option<PublicKey> {
    PublicKey::from_slice(pk.bytes()).ok()
}

fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn randomized_timestamp(rng: &mut OsRng) -> u64 {
    const MAX_SKEW_SECS: u64 = 2 * 24 * 60 * 60;
    let now = current_timestamp();
    let tweak = rng.gen_range(0..=MAX_SKEW_SECS);
    now.saturating_sub(tweak)
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
