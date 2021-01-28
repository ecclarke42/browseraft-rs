//! # browseraft
//!
//! For more, see the [raft](https://raft.github.io/) protocol
//!
//! ## Processes
//!
//! ### Leader Election
//!
//! When any follower's election times out, they
//! // TODO
//!

use gloo::{events::EventListener, timers::callback::Timeout};
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};
use wasm_bindgen::prelude::*;
use web_sys::BroadcastChannel;

mod raft;
mod rpc;

use raft::Peer;
pub use raft::Role;
use rpc::{Message, Recipient};

pub struct Node {
    pub id: u32, // TODO: better type? String? uuid?

    pub election_timeout_ms: u32,
    pub heartbeat_timeout_ms: u32,

    state: Arc<Mutex<NodeState>>,

    // state: State,
    // term: u64,
    channel: Arc<BroadcastChannel>,
}

pub(crate) struct NodeState {
    role: raft::Role,
    term: u32,
    voted_for: Option<Peer>,

    votes: HashSet<Peer>,
    peers: HashSet<Peer>,

    election_task: Option<Timeout>,
    heartbeat_task: Option<Timeout>,
    channel_listener: Option<EventListener>,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            role: Role::Follower,
            term: 0,
            voted_for: None,
            votes: HashSet::new(),
            peers: HashSet::new(),

            election_task: None,
            heartbeat_task: None,
            channel_listener: None,
        }
    }
}

impl Node {
    fn new(id: u32, channel: &str, rng: &mut OsRng) -> Arc<Self> {
        let channel =
            Arc::new(BroadcastChannel::new(channel).expect("failed to connect to channel"));

        let node = Self {
            id,

            election_timeout_ms: rng.gen_range(150, 300), // randomized 150 - 300?
            heartbeat_timeout_ms: 50,

            state: Arc::new(Mutex::new(NodeState::default())),

            channel,
        };

        let node = Arc::new(node);
        // TODO: update with below when available
        // https://github.com/rustwasm/gloo/issues/43
        let listener = node.clone().new_listener();

        // Mutex scope
        {
            // let node_ref = node.clone();
            let mut state = node.state.lock().expect("poisoned!");
            state.election_task = Some(node.clone().new_election_task());
            state.channel_listener = Some(listener);
            state.peers.insert(node.peer());
        }

        node.send(Message::PeerAdded, Recipient::Everyone);

        node
    }

    pub fn create(id: u32, channel: &str) -> Arc<Self> {
        let mut rng = OsRng::new().expect("failed to create RNG");
        Self::new(id, channel, &mut rng)
    }

    pub fn create_rand(channel: &str) -> Arc<Self> {
        let mut rng = OsRng::new().expect("failed to create RNG");
        let id = rng.gen();
        Self::new(id, channel, &mut rng)
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().expect("poisoned!");
        state.replace_election_task(None);
        state.replace_heartbeat_task(None);

        // EventListener drop handles removing listener
        if let Some(listener) = state.channel_listener.take() {
            drop(listener)
        }
    }

    pub fn role(&self) -> Role {
        self.state.lock().expect("poisoned!").role
    }

    pub fn peers(&self) -> HashSet<Peer> {
        self.state.lock().expect("poisoned!").peers.clone()
    }
}

// #[wasm_bindgen]
// pub fn handle_broadcast_message(channel: &BroadcastChannel, message: Message) {
//     channel.on_message(message)
// }

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
