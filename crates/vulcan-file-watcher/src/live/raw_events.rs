//! Bounded nonblocking admission for OS events and explicit recovery after overflow.
//! OS 事件的有界非阻塞准入，以及超限后的明确恢复。

use super::LiveWatchHealthState;
use notify::Event;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{Notify, mpsc};

const RAW_EVENT_CAPACITY: usize = 256;
const RAW_EVENT_PATH_LIMIT: usize = 256;
const RAW_EVENT_PATH_BYTES: usize = 64 * 1_024;

struct Overflow {
    pending: AtomicBool,
    notify: Notify,
    health: Arc<LiveWatchHealthState>,
}

impl Overflow {
    fn record(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            self.health.record_gap();
        }
        self.notify.notify_one();
    }
}

pub(super) enum RawWatchMessage {
    Event(notify::Result<Event>),
    Rescan,
}

pub(super) struct RawWatchSender {
    sender: mpsc::Sender<notify::Result<Event>>,
    overflow: Arc<Overflow>,
}

pub(super) struct RawWatchReceiver {
    receiver: mpsc::Receiver<notify::Result<Event>>,
    overflow: Arc<Overflow>,
}

pub(super) fn channel(health: Arc<LiveWatchHealthState>) -> (RawWatchSender, RawWatchReceiver) {
    let (sender, receiver) = mpsc::channel(RAW_EVENT_CAPACITY);
    let overflow = Arc::new(Overflow {
        pending: AtomicBool::new(false),
        notify: Notify::new(),
        health,
    });
    (
        RawWatchSender {
            sender,
            overflow: Arc::clone(&overflow),
        },
        RawWatchReceiver { receiver, overflow },
    )
}

impl RawWatchSender {
    /// Never blocks the OS callback; a full queue requests reconciliation instead of silent loss.
    /// 绝不阻塞 OS 回调；队列满时要求重新核对，而非静默丢失。
    pub(super) fn send(
        &self,
        event: notify::Result<Event>,
    ) -> Result<(), mpsc::error::SendError<notify::Result<Event>>> {
        if let Ok(event) = &event {
            let too_large = event.paths.len() > RAW_EVENT_PATH_LIMIT
                || event
                    .paths
                    .iter()
                    .try_fold(0usize, |bytes, path| {
                        bytes
                            .checked_add(path.as_os_str().len())
                            .filter(|bytes| *bytes <= RAW_EVENT_PATH_BYTES)
                    })
                    .is_none();
            if too_large {
                self.overflow.record();
                return Ok(());
            }
        }
        match self.sender.try_send(event) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow.record();
                Ok(())
            }
            Err(mpsc::error::TrySendError::Closed(event)) => Err(mpsc::error::SendError(event)),
        }
    }
}

impl RawWatchReceiver {
    /// Returns the latched rescan even when no later filesystem event arrives.
    /// 即使之后没有文件系统事件，也返回已经锁存的重新扫描要求。
    pub(super) async fn recv(&mut self) -> Option<RawWatchMessage> {
        loop {
            let notified = self.overflow.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.overflow.pending.swap(false, Ordering::AcqRel) {
                return Some(RawWatchMessage::Rescan);
            }
            tokio::select! {
                event = self.receiver.recv() => return event.map(RawWatchMessage::Event),
                _ = notified => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::EventKind;
    use std::path::PathBuf;

    #[tokio::test]
    async fn overflow_is_bounded_and_recoverable_without_a_later_event() {
        let (sender, mut receiver) = channel(Arc::new(LiveWatchHealthState::default()));
        for _ in 0..RAW_EVENT_CAPACITY + 20 {
            sender
                .send(Ok(Event::new(EventKind::Other)))
                .expect("bounded admission");
        }
        assert_eq!(receiver.receiver.len(), RAW_EVENT_CAPACITY);
        assert!(matches!(
            receiver.recv().await,
            Some(RawWatchMessage::Rescan)
        ));
        drop(sender);
        let mut remaining = 0;
        while let Some(event) = receiver.recv().await {
            assert!(matches!(event, RawWatchMessage::Event(_)));
            remaining += 1;
        }
        assert_eq!(remaining, RAW_EVENT_CAPACITY);
    }

    #[tokio::test]
    async fn oversized_event_is_not_retained_in_the_raw_queue() {
        let (sender, mut receiver) = channel(Arc::new(LiveWatchHealthState::default()));
        let mut event = Event::new(EventKind::Other);
        event.paths = vec![PathBuf::from("file"); RAW_EVENT_PATH_LIMIT + 1];
        sender.send(Ok(event)).expect("root fallback");
        assert!(receiver.receiver.is_empty());
        assert!(matches!(
            receiver.recv().await,
            Some(RawWatchMessage::Rescan)
        ));
        drop(sender);
        assert!(receiver.recv().await.is_none());
    }
}
