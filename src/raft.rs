use gloo::timers::callback::Timeout;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::Arc};

use crate::{
    rpc::{Message, Recipient},
    NodeState,
};

use super::Node;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Follower => write!(f, "Follower"),
            Role::Candidate => write!(f, "Candidate"),
            Role::Leader => write!(f, "Leader"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Peer(u32);

impl Peer {
    pub fn id(&self) -> u32 {
        self.0
    }
}

impl Node {
    pub fn peer(&self) -> Peer {
        Peer(self.id)
    }

    pub fn is(&self, peer: &Peer) -> bool {
        peer.id() == self.id
    }

    pub(crate) fn add_peer(&self, peer: Peer) {
        let mut state = self.state.lock().expect("poisoned mutex!");
        state.peers.insert(peer);

        if state.role == Role::Leader {
            self.send(Message::PeerSet(state.peers.clone()), Recipient::Everyone)
        }
    }

    pub(crate) fn remove_peer(&self, peer: Peer) {
        let mut state = self.state.lock().expect("poisoned mutex!");
        state.peers.remove(&peer);
    }

    /// When the leader sees PeerAdded message, it sends out a PeerSet response
    /// so that all nodes know
    pub(crate) fn reconcile_peers(&self, peers: HashSet<Peer>) {
        let mut state = self.state.lock().expect("poisoned mutex!");
        state.peers = peers;
    }

    pub(crate) fn new_election_task(self: Arc<Self>) -> Timeout {
        Timeout::new(self.election_timeout_ms, || self.start_election())
    }

    /// When election times out (the node hasn't heard from it's leader in the
    /// specified interval), this node becomes a candidate and initiates an
    /// election.
    fn start_election(self: Arc<Self>) {
        let mut state = self.state.lock().expect("poisoned mutex!");
        match state.peers.len() {
            // When only node, automatically win the election
            1 => {
                drop(state);
                self.clone().win_election();
            }

            // When two nodes, we could possibly deadlock. Default to the node with lower id
            2 => {
                // This requires waiting until the lower node's election times out
                let mut iter = state.peers.iter();
                let lower = std::cmp::min(iter.next().unwrap(), iter.next().unwrap());
                if self.peer().eq(lower) {
                    drop(state);
                    self.clone().win_election();
                }
            }

            // With at least 3 nodes, elect normally
            _ => {
                let candidate = self.peer();
                state.role = Role::Candidate;
                state.term += 1;
                state.votes.insert(self.peer());
                state.voted_for = Some(candidate);
                state.election_task = Some(self.clone().new_election_task());

                self.send(
                    Message::VoteRequest {
                        term: state.term,
                        candidate,
                    },
                    Recipient::Everyone,
                );
            }
        }
    }

    /// Receive a vote from the given follower
    pub(crate) fn receive_vote(self: Arc<Self>, term: u32, follower: Peer) {
        let mut state = self.state.lock().expect("poisoned mutex!");
        if state.role != Role::Candidate {
            return;
        }

        match term.cmp(&state.term) {
            // Got vote for a future term // TODO?
            std::cmp::Ordering::Greater => {}
            // Got vote for the current term: continue
            std::cmp::Ordering::Equal => {}
            // Got vote for previous term. Ignore
            std::cmp::Ordering::Less => return,
        }

        state.votes.insert(follower);

        if state.votes.len() > (state.peers.len() / 2) {
            drop(state);
            self.win_election()
        }
    }

    /// Win the current election and start sending out heartbeats
    fn win_election(self: Arc<Self>) {
        {
            let mut state = self.state.lock().expect("poisoned mutex!");
            // if state.role != Role::Candidate {
            //     return;
            // }

            state.role = Role::Leader;
            state.voted_for = None;
            state.votes.clear();
            state.election_task = None;
        }
        self.send_heartbeat();
    }

    pub(crate) fn receive_vote_request(self: Arc<Self>, term: u32, candidate: Peer) {
        let mut state = self.state.lock().expect("poisoned mutex!");

        if self.peer() == candidate {
            return;
        }

        // Update term
        match term.cmp(&state.term) {
            std::cmp::Ordering::Less => return,
            std::cmp::Ordering::Equal => (),
            std::cmp::Ordering::Greater => {
                state.term = term;
                state.voted_for = None;
                state.votes.clear();
            }
        }

        if state.role == Role::Follower && state.voted_for.is_none() {
            state.voted_for = Some(candidate);
            self.send(
                Message::VoteResponse {
                    term,
                    candidate,
                    follower: Peer(self.id),
                },
                Recipient::Peer(candidate),
            );
        }
        state.replace_election_task(Some(self.clone().new_election_task()));
    }

    pub(crate) fn new_heartbeat_task(self: Arc<Self>) -> Timeout {
        Timeout::new(self.heartbeat_timeout_ms, || self.send_heartbeat())
    }

    fn send_heartbeat(self: Arc<Self>) {
        let mut state = self.state.lock().expect("poisoned mutex!");
        self.send(Message::Heartbeat { term: state.term }, Recipient::Everyone);
        state.replace_heartbeat_task(Some(self.clone().new_heartbeat_task()));
    }

    pub(crate) fn receive_hearbeat(self: Arc<Self>, term: u32, _leader: &Peer) {
        let mut state = self.state.lock().expect("poisoned mutex!");

        match state.role {
            // Leader's own heartbeat
            Role::Leader => return,

            // Someone else won an election
            Role::Candidate => {
                if term >= state.term {
                    state.role = Role::Follower;
                    state.voted_for = None;
                    state.votes.clear();
                }
            }

            // Update term if there's a new term
            Role::Follower => {
                if term > state.term {
                    state.term = term;
                    state.voted_for = None;
                }
            }
        }

        state.replace_election_task(Some(self.clone().new_election_task()));
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.stop();
        self.send(Message::PeerRemoved, Recipient::Everyone)
    }
}

impl NodeState {
    pub(crate) fn replace_election_task(&mut self, new_task: Option<Timeout>) {
        if let Some(old_task) = if let Some(new_task) = new_task {
            self.election_task.replace(new_task)
        } else {
            self.election_task.take()
        } {
            old_task.cancel();
        }
    }

    pub(crate) fn replace_heartbeat_task(&mut self, new_task: Option<Timeout>) {
        if let Some(old_task) = if let Some(new_task) = new_task {
            self.heartbeat_task.replace(new_task)
        } else {
            self.heartbeat_task.take()
        } {
            old_task.cancel();
        }
    }
}
