pub mod cache;
pub mod nip17;
pub mod ui;

use enostr::{Pubkey, RelayEvent, RelayPool};
use hashbrown::HashMap;
use nostrdb::{Filter, Transaction};
use notedeck::{try_process_events_core, Accounts, App, AppContext, AppResponse};

use crate::{
    cache::{ConversationCache, ConversationStates},
    nip17::{giftwrap_filter, remote_sub},
    ui::messages::{login_nsec_prompt, MessagesUi},
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

        self.subs.ensure(ctx.pool, &ctx.accounts);

        {
            let txn = Transaction::new(&ctx.ndb).expect("txn");
            if !cache.initialized_convos {
                if let Some(nsec) = ctx
                    .accounts
                    .selected_filled()
                    .map(|f| f.secret_key.secret_bytes())
                {
                    ctx.ndb.add_key(&nsec);
                    ctx.ndb.process_giftwraps(&txn);
                }
                cache.initialized_convos = true;
            } else {
                ctx.ndb.process_giftwraps(&txn);
            }
            cache.init_conversations(&ctx.ndb, &txn, ctx.accounts.selected_account_pubkey());
        }

        MessagesUi::new(cache, &mut self.states, &ctx.ndb).ui(ui);
        AppResponse::none()
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

    pub fn ensure(&mut self, pool: &mut RelayPool, accounts: &Accounts) {
        if self.acc == *accounts.selected_account_pubkey()
            && accounts
                .get_selected_account()
                .keypair()
                .secret_key
                .is_some()
            && !self.remotes.is_empty()
        {
            return;
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
            return;
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
        }]
    }
}

fn remote_filters(acc: &Pubkey) -> Vec<Filter> {
    vec![giftwrap_filter(&acc)]
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
