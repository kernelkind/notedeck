//! OS network-interface monitoring for realtime relay connection status.
//!
//! TCP keepalive ([`crate::relay::ws`]) catches a silently-dropped connection
//! within seconds, but that is still a timeout. When the route to a relay
//! disappears outright, such as when a VPN tunnel or network interface goes
//! down, the OS knows immediately. [`NetworkWatcher`] surfaces interface-down
//! events directly to the outbox service actor so it can close affected relay
//! legs immediately without polling.
//!
//! Android has no OS watcher here: if-watch only implements netlink for
//! `target_os = "linux"`, so Android would get its `getifaddrs()` polling
//! fallback — a battery drain that isn't even an instant signal, and a link
//! error below Android API 24 where `getifaddrs()` doesn't exist. There
//! [`NetworkWatcher::next_down`] remains permanently pending, leaving TCP
//! keepalive as the liveness signal.

#[cfg(not(target_os = "android"))]
mod platform {
    use std::time::{Duration, Instant};

    use futures_util::StreamExt;
    use if_watch::{tokio::IfWatcher, IfEvent};
    use ipnet::IpNet;

    use crate::relay::backoff;

    const NETWORK_WATCH_RETRY_BACKOFF_BASE: Duration = Duration::from_secs(5);
    const MAX_NETWORK_WATCH_RETRY_BACKOFF: Duration = Duration::from_secs(30 * 60);

    /// Watches network interfaces and reports the subnets that go away.
    pub struct NetworkWatcher {
        watcher: Option<IfWatcher>,
        retry_attempt: u32,
        retry_at: Option<Instant>,
    }

    impl NetworkWatcher {
        /// Create the route monitor. The platform watcher opens when
        /// [`Self::next_down`] is first polled.
        pub fn new() -> Self {
            Self {
                watcher: None,
                retry_attempt: 0,
                retry_at: None,
            }
        }

        /// Wait for the next interface-down subnet. Interface-up events do not
        /// invalidate live sockets and are consumed here; normal relay demand
        /// and reconnect handling bring eligible connections online on the new
        /// route. Watcher failures retry without blocking other service work.
        pub async fn next_down(&mut self) -> IpNet {
            loop {
                self.wait_for_retry().await;

                if self.watcher.is_none() {
                    match IfWatcher::new() {
                        Ok(watcher) => self.watcher = Some(watcher),
                        Err(err) => {
                            let retry_after = self.schedule_retry();
                            tracing::warn!(
                                "network watcher unavailable: {err}; retrying in {retry_after:?}"
                            );
                            continue;
                        }
                    }
                }

                let Some(watcher) = self.watcher.as_mut() else {
                    continue;
                };
                match watcher.next().await {
                    Some(Ok(IfEvent::Down(subnet))) => {
                        self.reset_retry();
                        return subnet;
                    }
                    Some(Ok(IfEvent::Up(_))) => {
                        self.reset_retry();
                        // Initial interface discovery can queue several `Up`
                        // events; let the service poll its other inputs between them.
                        tokio::task::yield_now().await;
                    }
                    Some(Err(err)) => {
                        let retry_after = self.schedule_retry();
                        tracing::warn!("network watcher error: {err}; retrying in {retry_after:?}");
                    }
                    None => {
                        self.watcher = None;
                        let retry_after = self.schedule_retry();
                        tracing::warn!("network watcher ended; retrying in {retry_after:?}");
                    }
                }
            }
        }

        /// Resume once the retained retry deadline is due.
        ///
        /// Clearing the deadline after the await keeps this cancellation-safe:
        /// if another service input wins `tokio::select!`, the same deadline
        /// remains.
        async fn wait_for_retry(&mut self) {
            let Some(retry_at) = self.retry_at else {
                return;
            };
            tokio::time::sleep_until(tokio::time::Instant::from_std(retry_at)).await;
            self.retry_at = None;
        }

        /// Record one watcher failure and return its capped backoff duration.
        fn schedule_retry(&mut self) -> Duration {
            let retry_after = network_watch_retry_after(self.retry_attempt);
            self.retry_attempt = self.retry_attempt.saturating_add(1);
            self.retry_at = Some(Instant::now() + retry_after);
            retry_after
        }

        /// Reset backoff after the watcher produces a valid interface event.
        fn reset_retry(&mut self) {
            self.retry_attempt = 0;
            self.retry_at = None;
        }
    }

    fn network_watch_retry_after(attempt: u32) -> Duration {
        backoff::next_duration_from_base(
            attempt,
            NETWORK_WATCH_RETRY_BACKOFF_BASE,
            backoff::jitter_seed(&"network_watcher", attempt),
            MAX_NETWORK_WATCH_RETRY_BACKOFF,
        )
    }
}

#[cfg(target_os = "android")]
mod platform {
    use ipnet::IpNet;

    /// Android fallback that leaves TCP keepalive as the liveness signal.
    pub struct NetworkWatcher;

    impl NetworkWatcher {
        /// Create the disabled route monitor (see the module docs).
        pub fn new() -> Self {
            Self
        }

        /// Wait forever because Android route monitoring is disabled.
        pub async fn next_down(&mut self) -> IpNet {
            std::future::pending().await
        }
    }
}

pub use platform::NetworkWatcher;
