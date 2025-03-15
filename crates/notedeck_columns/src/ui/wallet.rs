use egui::{Button, TextEdit};

use notedeck::{NoWallet, Wallet, WalletAction, WalletState};

pub struct WalletView<'a> {
    state: &'a mut WalletState,
}

impl<'a> WalletView<'a> {
    pub fn new(state: &'a mut WalletState) -> Self {
        Self { state }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<WalletAction> {
        egui::Frame::NONE
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| self.inner_ui(ui))
            .inner
    }

    fn inner_ui(&mut self, ui: &mut egui::Ui) -> Option<WalletAction> {
        ui.add(egui::Label::new(egui::RichText::new("Wallet").heading()));

        match &mut self.state {
            WalletState::Wallet(wallet) => show_with_wallet(ui, wallet),
            WalletState::NoWallet(no_wallet) => show_no_wallet(ui, no_wallet),
        }
    }
}

fn show_no_wallet(ui: &mut egui::Ui, no_wallet: &mut NoWallet) -> Option<WalletAction> {
    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        ui.add(TextEdit::singleline(&mut no_wallet.buf).hint_text("Enter your NWC URI here..."));
        if ui.add(Button::new("Save")).clicked() {
            action = Some(WalletAction::SaveURI);
        }
    });

    action
}

fn show_with_wallet(ui: &mut egui::Ui, wallet: &mut Wallet) -> Option<WalletAction> {
    ui.horizontal_wrapped(|ui| {
        ui.label("balance: ");
        let balance = wallet.get_balance();

        if let Some(balance) = balance {
            match balance {
                Ok(msats) => ui.label(format!("{msats} msats")),
                Err(e) => ui.colored_label(egui::Color32::RED, format!("error: {e}")),
            }
        } else {
            ui.spinner()
        }
    });

    None
}
