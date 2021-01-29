use gloo::events::EventListener;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::MessageEvent;

use super::{
    raft::{Peer, Role},
    Node,
};

#[derive(Serialize, Deserialize)]
struct MessageWrapper<T> {
    from: Peer,
    to: Recipient,
    msg: Message<T>,
}

#[derive(Serialize, Deserialize)]
pub enum Recipient {
    Everyone,
    Peer(Peer),
}

#[derive(Serialize, Deserialize)] //FromWasmAbi)]
pub enum Message<T> {
    PeerAdded,
    PeerRemoved,
    PeerSet(HashSet<Peer>),

    VoteRequest {
        term: u32,
        candidate: Peer,
    },
    VoteResponse {
        term: u32,
        candidate: Peer,
        follower: Peer,
    },

    Heartbeat {
        term: u32,
    },
    // HeartbeatResponse,
    // Unknown,
    Payload(T),
}

impl<T> MessageWrapper<T>
where
    T: Serialize + DeserializeOwned + 'static,
{
    fn to_js(&self) -> JsValue {
        JsValue::from_serde(&self).expect("failed to serialize")
    }

    fn from_js(value: JsValue) -> Self {
        JsValue::into_serde(&value).expect("failed to deserialize")
    }
}

impl<T> Node<T>
where
    T: serde::ser::Serialize + serde::de::DeserializeOwned + 'static,
{
    pub(crate) fn send(&self, message: Message<T>, to: Recipient) {
        let message = MessageWrapper {
            from: self.peer(),
            to,
            msg: message,
        };
        let message = message.to_js();
        self.channel
            .post_message(&message)
            .expect("failed to post message");
    }

    pub(crate) fn new_listener(self: Arc<Self>) -> EventListener {
        let node = self.clone();
        EventListener::new(&*self.channel.clone(), "message", move |event| {
            let event: &MessageEvent = event.dyn_ref::<MessageEvent>().unwrap_throw();
            node.clone()
                .on_message(MessageWrapper::from_js(event.data()));
        })
    }

    fn on_message(self: Arc<Self>, wrapper: MessageWrapper<T>) {
        let MessageWrapper { from, to, msg } = wrapper;

        if let Recipient::Peer(to) = to {
            if !self.is(&to) {
                return;
            }
        }

        match msg {
            Message::PeerAdded => self.add_peer(from),
            Message::PeerRemoved => self.remove_peer(from),
            Message::PeerSet(peers) => self.reconcile_peers(peers),
            Message::Heartbeat { term } => self.receive_hearbeat(term, &from),
            Message::VoteRequest { term, candidate } => {
                if !self.is(&candidate) {
                    self.receive_vote_request(term, candidate)
                }
            }
            Message::VoteResponse {
                term,
                candidate,
                follower,
            } => {
                if self.is(&candidate) {
                    self.receive_vote(term, follower);
                }
            }
            Message::Payload(payload) => self.call_on_received(payload),
        }
    }
}
