use egui::{ahash::HashMap, TextureHandle};
use poll_promise::Promise;

#[allow(dead_code)]
#[derive(Default)]
pub struct Jobs {
    pub jobs: HashMap<JobId, Promise<Job>>,
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum JobId {
    Blurhash(String),
}

#[allow(dead_code)]
pub enum Job {
    ProcessBlurhash(Option<TextureHandle>),
}
