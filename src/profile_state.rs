use enostr::{Filter, Pubkey};
use poll_promise::Promise;

use crate::timeline::Timeline;

pub struct ProfileState {
    pub timeline: Timeline,
    following_count: Promise<Option<u32>>,
    follower_count: Promise<Option<u32>>,
    relay_count: Promise<Option<u32>>,
}

impl ProfileState {
    pub fn new(key: &Pubkey) -> Self {
        let filters = vec![Filter::new()
            .authors(vec![key.clone().bytes()])
            .kinds(vec![1])
            .limit(100)
            .build()];
        Self {
            timeline: Timeline::new(filters),
            following_count: Promise::from_ready(Some(0)),
            follower_count: Promise::from_ready(Some(0)),
            relay_count: Promise::from_ready(Some(0)),
        }
    }

    pub fn get_following_count(&self) -> Option<&u32> {
        if let Some(following_count) = self.following_count.ready() {
            return following_count.as_ref();
        } else {
            None
        }
    }

    pub fn get_followers_count(&self) -> Option<&u32> {
        if let Some(follower_count) = self.follower_count.ready() {
            return follower_count.as_ref();
        } else {
            None
        }
    }

    pub fn get_relays_count(&self) -> Option<&u32> {
        if let Some(relay_count) = self.relay_count.ready() {
            return relay_count.as_ref();
        } else {
            None
        }
    }
}
