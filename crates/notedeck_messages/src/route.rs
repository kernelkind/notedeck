use crate::cache::ConversationId;

#[derive(Clone, Debug)]
pub enum Route {
    ConvoList,
    CreateConvo,
    Conversation,
}
