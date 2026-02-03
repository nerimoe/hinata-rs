use tokio::sync::mpsc;
use tokio::sync::mpsc::{Receiver, Sender};

pub(crate) enum InMessage {
    SendPacket(Vec<u8>),
    SendPacketAndSubscribe(Vec<u8>, Subscription),
    Subscribe(u8, Subscription),
    UnSubscribe(u8)
}

#[derive(Debug)]
pub(crate) enum OutMessage {
    Response(Vec<u8>),
    DeviceDisconnect,
}

pub(crate) enum UnSubscribePolicy {
    Count(usize),
    Never,
    SpecificIsOn(usize, u8),
    SpecificNotOn(usize, u8)
}

impl UnSubscribePolicy {
    pub(crate) fn need_dispose(&self, msg: &OutMessage, count: usize) -> bool {
        if let OutMessage::Response(packet) = msg {
            match self {
                UnSubscribePolicy::Count(n) => count >= *n,
                UnSubscribePolicy::Never => false,
                UnSubscribePolicy::SpecificIsOn(index, byte) => if let Some(b) = packet.get(*index) {
                    if b == byte {
                        true
                    } else {
                        false
                    }
                } else {
                    true
                },
                UnSubscribePolicy::SpecificNotOn(index, byte) => if let Some(b) = packet.get(*index) {
                    if b != byte {
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
        } else {
            true
        }
    }
}

pub(crate) struct Subscription {
    sender: Sender<OutMessage>,
    policy: UnSubscribePolicy,
    count: usize
}

impl Subscription {
    pub(crate) fn new(policy: UnSubscribePolicy) -> (Self, Receiver<OutMessage>) {
        let (sender, receiver) = mpsc::channel::<OutMessage>(32);
        (
            Self {
                sender,
                policy,
                count: 0,
            },
            receiver
        )
    }
    pub(crate) fn send(&mut self, msg: OutMessage) -> bool {
        self.count = self.count + 1;
        let mut need_dispose = self.policy.need_dispose(&msg, self.count);
        if let Err(_) = self.sender.blocking_send(msg) { need_dispose = true }
        need_dispose
    }

    pub(crate) fn send_no_check(&self, msg: OutMessage) {
        let _ = self.sender.blocking_send(msg);
    }
}