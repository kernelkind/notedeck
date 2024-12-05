use super::{AccountLoginResponse, AccountsViewResponse};
use serde::{Deserialize, Serialize};

pub enum AccountsRouteResponse {
    Accounts(AccountsViewResponse),
    AddAccount(AccountLoginResponse),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Serialize, Deserialize)]
pub enum AccountsRoute {
    Accounts,
    AddAccount,
}

impl ToString for AccountsRoute {
    fn to_string(&self) -> String {
        let self_str = "accounts";
        let sub_str = match &self {
            AccountsRoute::Accounts => "show".to_owned(),
            AccountsRoute::AddAccount => "add".to_owned(),
        };
        format!("{}:{}", self_str, sub_str)
    }
}

#[derive(Debug)]
pub enum AccountsAction {
    Switch(usize),
    Remove(usize),
}
