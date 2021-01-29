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
use web_sys::BroadcastChannel;

mod raft;
mod rpc;

use raft::Peer;
pub use raft::Role;
use rpc::{Message, Recipient};

pub struct Node<T>
where
    T: serde::ser::Serialize + serde::de::DeserializeOwned + 'static,
{
    pub id: u32, // TODO: better type? String? uuid?

    pub election_timeout_ms: u32,
    pub heartbeat_timeout_ms: u32,

    state: Arc<Mutex<NodeState>>,

    channel: Arc<BroadcastChannel>,

    // TODO: builder pattern
    // phantom_data: std::marker::PhantomData<T>,
    on_received: Option<Box<dyn Fn(T) + 'static>>,
    on_role_change: Option<Box<dyn Fn(Role) + 'static>>,
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

pub struct NodeBuilder<T> {
    election_timeout_ms: Option<u32>,
    election_timeout_ms_range: Option<(u32, u32)>,
    heartbeat_timeout_ms: Option<u32>,
    id: Option<u32>,
    channel_name: Option<String>,
    on_received_handler: Option<Box<dyn Fn(T) + 'static>>,
    on_role_change_handler: Option<Box<dyn Fn(Role) + 'static>>,
}

impl<T> Default for NodeBuilder<T> {
    fn default() -> Self {
        Self {
            election_timeout_ms: None,
            election_timeout_ms_range: None,
            heartbeat_timeout_ms: None,
            id: None,
            channel_name: None,
            on_received_handler: None,
            on_role_change_handler: None,
        }
    }
}

impl<T> NodeBuilder<T>
where
    T: serde::ser::Serialize + serde::de::DeserializeOwned + 'static,
{
    /// Set the election timeout in milliseconds. This is discouraged, as the
    /// timeout should be randomized (so that nodes are less likely to
    /// simultaneously initiate elections). See
    /// [`eleciton_timeout_range`](NodeBuilder::election_timeout_range)
    ///
    /// NB: If election_timeout_range is also set, this will overwrite it.
    ///
    /// Defaults to a random value in the range (150, 300)
    pub fn election_timeout(mut self, ms: u32) -> Self {
        self.election_timeout_ms = Some(ms);
        self
    }

    /// Set the range for the randomized value of the election timeout.
    ///
    /// Defaults to (150, 300).
    pub fn election_timeout_range(mut self, low_ms: u32, high_ms: u32) -> Self {
        if low_ms < high_ms {
            self.election_timeout_ms_range = Some((low_ms, high_ms));
        } else {
            self.election_timeout_ms_range = Some((high_ms, low_ms));
        }
        self
    }

    /// Set the leader heartbeat interval in milliseconds
    ///
    /// Defaults to 50
    pub fn heartbeat_timeout(mut self, ms: u32) -> Self {
        self.heartbeat_timeout_ms = Some(ms);
        self
    }

    /// Set the node's id.
    ///
    /// Defaults to a random number
    pub fn id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the name of the `BroadcastChannel` the node should connect to
    ///
    /// Defaults to `"raft-nodes"`
    pub fn channel(mut self, name: &str) -> Self {
        self.channel_name = Some(name.to_string());
        self
    }

    /// Attach a closure to the node that will be called when the node receives
    /// a [Message::Payload] message
    pub fn on_received<F>(mut self, callback: F) -> Self
    where
        F: Fn(T) + 'static,
    {
        self.on_received_handler = Some(Box::new(callback) as Box<dyn Fn(T)>);
        self
    }

    /// Attach a closure to the node that will be called when the node's role
    /// is changed.
    pub fn on_role_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(Role) + 'static,
    {
        self.on_role_change_handler = Some(Box::new(callback) as Box<dyn Fn(Role)>);
        self
    }

    /// Finalize the builder and construct a Node
    pub fn build(self) -> Arc<Node<T>> {
        // Deconstruct builder
        let NodeBuilder {
            election_timeout_ms,
            election_timeout_ms_range,
            heartbeat_timeout_ms,
            id,
            on_received_handler,
            on_role_change_handler,
            channel_name,
            ..
        } = self;

        // Create Channel
        let channel = {
            let channel_name = match channel_name {
                Some(ref name) => name.as_str(),
                None => Node::<T>::DEFAULT_CHANNEL,
            };
            BroadcastChannel::new(channel_name).expect("failed to connect to channel")
        };

        // Use or generate id
        let (id, maybe_rng) = if let Some(id) = id {
            (id, None)
        } else {
            let mut rng = OsRng::new().expect("failed to create RNG");
            (rng.gen(), Some(rng))
        };

        // Use or generate election_timeout
        let election_timeout_ms = if let Some(timeout) = election_timeout_ms {
            timeout
        } else if let Some((low, high)) = election_timeout_ms_range {
            if let Some(mut rng) = maybe_rng {
                rng
            } else {
                OsRng::new().expect("failed to create RNG")
            }
            .gen_range(low, high)
        } else {
            if let Some(mut rng) = maybe_rng {
                rng
            } else {
                OsRng::new().expect("failed to create RNG")
            }
            .gen_range(150, 300)
        };

        let node = Node {
            id,
            election_timeout_ms,
            heartbeat_timeout_ms: heartbeat_timeout_ms.unwrap_or(50),

            state: Arc::new(Mutex::new(NodeState::default())),
            channel: Arc::new(channel),

            on_received: on_received_handler,
            on_role_change: on_role_change_handler,
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
}

impl<T> Node<T>
where
    T: serde::ser::Serialize + serde::de::DeserializeOwned + 'static,
{
    const DEFAULT_CHANNEL: &'static str = "raft-nodes";

    pub fn builder() -> NodeBuilder<T> {
        NodeBuilder::default()
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

    /// Issue a message to all nodes (only sends if this node is the leader)
    pub fn issue(&self, payload: T) {
        if self.role() == Role::Leader {
            self.send(Message::Payload(payload), Recipient::Everyone)
        }
    }

    pub(crate) fn call_on_received(&self, data: T) {
        if let Some(ref func) = self.on_received {
            (func)(data)
        }
    }

    pub(crate) fn call_on_role_change(&self, role: Role) {
        if let Some(ref func) = self.on_role_change {
            (func)(role)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
