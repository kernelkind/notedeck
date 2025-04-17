use tokenator::{ParseError, TokenParser, TokenSerializable};

const DEFAULT_ZAP_MSATS: u64 = 10_000;

#[derive(Debug, Default)]
pub struct DefaultZapMsats {
    msats: Option<u64>,
}

impl DefaultZapMsats {
    pub fn set_user_selection(&mut self, msats: u64) {
        self.msats = Some(msats);
    }

    pub fn get_default_zap_msats(&self) -> u64 {
        let Some(default_zap_msats) = self.msats else {
            return DEFAULT_ZAP_MSATS;
        };

        default_zap_msats
    }

    pub fn has_user_selection(&self) -> bool {
        self.msats.is_some()
    }

    pub fn try_into_user(&self) -> Option<UserZapMsats> {
        let user_zap_amount = self.msats?;

        Some(UserZapMsats {
            msats: user_zap_amount,
        })
    }

    pub fn try_into_user_unowned(&self) -> Option<UserZapMsatsUnowned> {
        let Some(user_zap_amount) = &self.msats else {
            return None;
        };

        Some(UserZapMsatsUnowned {
            msats: user_zap_amount,
        })
    }
}

#[derive(Debug)]
pub struct UserZapMsats {
    pub msats: u64,
}

#[derive(Debug)]
pub struct UserZapMsatsUnowned<'a> {
    pub msats: &'a u64,
}

impl TokenSerializable for UserZapMsats {
    fn parse_from_tokens<'a>(parser: &mut TokenParser<'a>) -> Result<Self, ParseError<'a>> {
        parser.parse_token("default_zap")?;

        let msats: u64 = parser
            .pull_token()?
            .parse()
            .map_err(|_| ParseError::DecodeFailed)?;

        Ok(UserZapMsats { msats })
    }

    fn serialize_tokens(&self, writer: &mut tokenator::TokenWriter) {
        writer.write_token("default_zap");
        writer.write_token(&self.msats.to_string());
    }
}

#[derive(Debug, Default)]
pub struct PendingDefaultZapState {
    pub amount_sats: String,
    pub error_message: Option<DefaultZapError>,
    pub is_rewriting: bool,
}

#[derive(Debug)]
pub enum DefaultZapError {
    InvalidUserInput,
}
