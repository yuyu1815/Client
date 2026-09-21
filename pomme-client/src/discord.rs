use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use discord_rich_presence::activity::*;
use discord_rich_presence::{DiscordIpc, DiscordIpcClient};

const DISCORD_CLIENT_ID: &str = "1489624876909330452";

fn base_activity(version: &str) -> Activity<'static> {
    Activity::new()
        .details(format!("Pomme Client — {version}"))
        .assets(
            Assets::new()
                .large_image("green-apple")
                .large_text("Pomme Client"),
        )
}

#[derive(PartialEq)]
pub enum PresenceState {
    Loading,
    InMenu,
    Multiplayer,
    Singleplayer,
}

struct PresenceUpdate {
    version: String,
    text: &'static str,
}

struct PresenceQueue {
    pending: Arc<Mutex<Option<PresenceUpdate>>>,
    wake: Option<SyncSender<()>>,
}

impl PresenceQueue {
    fn publish(&self, update: PresenceUpdate) {
        *self.pending.lock().expect("presence queue poisoned") = Some(update);
        if let Some(wake) = &self.wake
            && let Err(TrySendError::Disconnected(())) = wake.try_send(())
        {
            tracing::debug!("Discord presence worker stopped");
        }
    }
}

fn take_update(pending: &Mutex<Option<PresenceUpdate>>) -> Option<PresenceUpdate> {
    pending.lock().expect("presence queue poisoned").take()
}

fn requeue_failed_update(pending: &Mutex<Option<PresenceUpdate>>, update: PresenceUpdate) {
    pending
        .lock()
        .expect("presence queue poisoned")
        .get_or_insert(update);
}

fn run_worker(pending: Arc<Mutex<Option<PresenceUpdate>>>, wake: Receiver<()>) {
    let mut client = None;
    let mut warned = false;

    loop {
        if client.is_none() {
            match wake.try_recv() {
                Ok(()) => {}
                Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => {}
            }

            crate::app::startup_mark("discord_worker_connect_start");
            let mut candidate = DiscordIpcClient::new(DISCORD_CLIENT_ID);
            match candidate.connect() {
                Ok(()) => {
                    crate::app::startup_mark("discord_worker_connected");
                    if warned {
                        tracing::info!("Discord rich presence reconnected");
                    }
                    warned = false;
                    client = Some(candidate);
                }
                Err(error) => {
                    if !warned {
                        tracing::warn!("Discord rich presence unavailable: {error}");
                        warned = true;
                    }
                    match wake.recv_timeout(Duration::from_secs(1)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        }

        let Some(update) = take_update(&pending) else {
            match wake.recv_timeout(Duration::from_secs(1)) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        };

        let Some(active) = client.as_mut() else {
            continue;
        };
        crate::app::startup_mark("discord_worker_set_activity_start");
        if active
            .set_activity(base_activity(&update.version).state(update.text))
            .is_err()
        {
            client = None;
            requeue_failed_update(&pending, update);
        } else {
            crate::app::startup_mark("discord_worker_set_activity_ready");
        }
    }

    if let Some(mut client) = client {
        let _ = client.clear_activity();
        let _ = client.close();
    }
}

pub struct DiscordPresence {
    queue: PresenceQueue,
    state: PresenceState,
}

impl DiscordPresence {
    pub fn start(version: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let (wake_tx, wake_rx) = mpsc::sync_channel(1);
        let pending = Arc::new(Mutex::new(None));
        let worker_pending = Arc::clone(&pending);
        std::thread::Builder::new()
            .name("Pomme Discord presence".to_string())
            .spawn(move || run_worker(worker_pending, wake_rx))?;
        let presence = Self {
            queue: PresenceQueue {
                pending,
                wake: Some(wake_tx),
            },
            state: PresenceState::Loading,
        };
        presence.queue.publish(PresenceUpdate {
            version: version.to_owned(),
            text: "Starting...",
        });
        Ok(presence)
    }

    pub fn set_in_menu(&mut self, version: &str) {
        self.enter(PresenceState::InMenu, version, "In the menu");
    }

    pub fn playing_multiplayer(&mut self, version: &str) {
        self.enter(PresenceState::Multiplayer, version, "In a server");
    }

    pub fn playing_singleplayer(&mut self, version: &str) {
        self.enter(PresenceState::Singleplayer, version, "In a world");
    }

    /// Re-entering the current state sends nothing.
    fn enter(&mut self, state: PresenceState, version: &str, text: &'static str) {
        if self.state == state {
            return;
        }
        self.state = state;
        self.queue.publish(PresenceUpdate {
            version: version.to_owned(),
            text,
        });
    }
}

impl Drop for DiscordPresence {
    fn drop(&mut self) {
        // Dropping the wake sender tells the worker to close. Do not join: a
        // stuck Discord IPC read must never delay application shutdown.
        self.queue.wake.take();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, mpsc};

    use super::{PresenceQueue, PresenceUpdate, requeue_failed_update, take_update};

    #[test]
    fn latest_presence_replaces_a_full_bounded_queue() {
        let (wake, _rx) = mpsc::sync_channel(1);
        let queue = PresenceQueue {
            pending: Arc::new(Mutex::new(None)),
            wake: Some(wake),
        };
        queue.publish(PresenceUpdate {
            version: "26.2".to_owned(),
            text: "In a server",
        });
        queue.publish(PresenceUpdate {
            version: "26.2".to_owned(),
            text: "In a world",
        });

        assert_eq!(take_update(&queue.pending).unwrap().text, "In a world");
    }

    #[test]
    fn failed_worker_does_not_block_presence_update() {
        let (wake, receiver) = mpsc::sync_channel(1);
        drop(receiver);
        let queue = PresenceQueue {
            pending: Arc::new(Mutex::new(None)),
            wake: Some(wake),
        };

        queue.publish(PresenceUpdate {
            version: "26.2".to_owned(),
            text: "In the menu",
        });

        assert_eq!(take_update(&queue.pending).unwrap().text, "In the menu");
    }

    #[test]
    fn failed_update_requeues_only_when_no_newer_update_is_pending() {
        let pending = Mutex::new(Some(PresenceUpdate {
            version: "26.2".to_owned(),
            text: "In the menu",
        }));
        let (wake, _receiver) = mpsc::sync_channel(1);
        let queue = PresenceQueue {
            pending: Arc::new(pending),
            wake: Some(wake),
        };
        let failed = take_update(&queue.pending).expect("old update should be in flight");
        queue.publish(PresenceUpdate {
            version: "26.2".to_owned(),
            text: "In a world",
        });
        requeue_failed_update(&queue.pending, failed);
        assert_eq!(take_update(&queue.pending).unwrap().text, "In a world");

        let pending = Mutex::new(None);
        let failed = PresenceUpdate {
            version: "26.2".to_owned(),
            text: "In a server",
        };
        requeue_failed_update(&pending, failed);
        assert_eq!(take_update(&pending).unwrap().text, "In a server");
    }
}
