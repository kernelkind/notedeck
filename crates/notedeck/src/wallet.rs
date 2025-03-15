use std::sync::Arc;

use egui::ahash::HashMap;
use nwc::{
    nostr::nips::nip47::{PayInvoiceRequest, PayInvoiceResponse},
    NWC,
};
use poll_promise::Promise;
use tokio::sync::RwLock;

use crate::Error;

pub enum WalletState {
    Wallet(Wallet),
    NoWallet(NoWallet),
}

pub struct NoWallet {
    pub buf: String,
}

pub struct Wallet {
    wallet: Arc<RwLock<NWC>>,
    balance: Option<Promise<Result<u64, nwc::Error>>>,
    invoices: HashMap<String, Promise<Result<PayInvoiceResponse, nwc::Error>>>, // TODO(kernelkind): move to Jobs when blurhash is merged
}

impl Default for WalletState {
    fn default() -> Self {
        WalletState::NoWallet(NoWallet { buf: String::new() })
    }
}

impl Wallet {
    pub fn get_balance(&mut self) -> Option<&Result<u64, nwc::Error>> {
        if self.balance.is_none() {
            self.balance = Some(get_balance(self.wallet.clone()));
            return None;
        }

        self.balance.as_ref().unwrap().ready()
    }

    pub fn pay_invoice(&mut self, invoice: &str) -> Option<Result<PayInvoiceResponse, Error>> {
        let promise = self.invoices.get(invoice)?;

        if let Some(res) = promise.ready() {
            return Some(
                res.as_ref()
                    .cloned()
                    .map_err(|e| Error::Generic(e.to_string())),
            );
        }

        let res = pay_invoice(
            self.wallet.clone(),
            PayInvoiceRequest::new(invoice.to_owned()),
        );

        self.invoices.insert(invoice.to_owned(), res);

        None
    }
}

fn get_balance(nwc: Arc<RwLock<NWC>>) -> Promise<Result<u64, nwc::Error>> {
    let (sender, promise) = Promise::new();

    tokio::spawn(async move {
        sender.send(nwc.read().await.get_balance().await);
    });

    promise
}

fn pay_invoice(
    nwc: Arc<RwLock<NWC>>,
    invoice: PayInvoiceRequest,
) -> Promise<Result<PayInvoiceResponse, nwc::Error>> {
    let (sender, promise) = Promise::new();

    tokio::spawn(async move {
        sender.send(nwc.read().await.pay_invoice(invoice).await);
    });

    promise
}

pub enum WalletAction {
    SaveURI,
}

// impl WalletAction {
//     pub fn process(wallet_state: &mut WalletState) {
//         let WalletState::NoWallet(no_wallet) = wallet_state else {
//             return;
//         };

//         let uri = &no_wallet.buf;

//         let nwc_uri = NostrWalletConnectURI::new(public_key, relay_url, random_secret_key, lud16)

//     }
// }
