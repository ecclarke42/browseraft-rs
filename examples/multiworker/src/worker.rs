use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use yew::worker::*;

use browseraft::{Node, Role};

pub struct Worker {
    link: AgentLink<Self>,
    node: Option<Arc<Node<NodeMsg>>>,
    handlers: HashSet<HandlerId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeMsg {}

#[derive(Serialize, Deserialize)]
pub enum Input {
    Initialize(u32),
    Send(NodeMsg),
    Stop,
}

#[derive(Serialize, Deserialize)]
pub enum Output {
    State(u32, Role),
    Message(NodeMsg),
}

pub enum Message {
    NodePayload(NodeMsg),
    RoleChange(browseraft::Role),
}

impl Agent for Worker {
    type Reach = Job<Self>;
    type Message = Message;
    type Input = Input;
    type Output = Output;

    fn create(link: AgentLink<Self>) -> Self {
        Self {
            link,
            handlers: HashSet::new(),
            node: None,
        }
    }

    fn update(&mut self, msg: Self::Message) {
        match msg {
            Message::NodePayload(msg) => {
                yew::services::ConsoleService::log(format!("recieved {:?}", &msg).as_str());
                if self.node.is_some() {
                    for id in self.handlers.iter() {
                        self.link.respond(*id, Output::Message(msg.clone()))
                    }
                }
            }
            Message::RoleChange(role) => {
                yew::services::ConsoleService::log(format!("role changed {:?}", role).as_str());
                if let Some(node) = self.node.clone() {
                    for id in self.handlers.iter() {
                        self.link.respond(*id, Output::State(node.id, role))
                    }
                }
            }
        }
    }

    fn handle_input(&mut self, msg: Self::Input, _id: HandlerId) {
        match msg {
            Input::Initialize(id) => {
                let on_received = self.link.callback(Message::NodePayload);
                let on_role_change = self.link.callback(Message::RoleChange);
                self.node = Some(
                    Node::builder()
                        .id(id)
                        .channel("node-workers")
                        .election_timeout_range(500, 1000)
                        .on_received(move |message| on_received.emit(message))
                        .on_role_change(move |role| on_role_change.emit(role))
                        .build(),
                );
            }

            Input::Send(msg) => {
                if let Some(node) = self.node.clone() {
                    node.issue(msg)
                }
            }

            Input::Stop => {
                if let Some(ref node) = self.node.take() {
                    node.stop();
                }
            }
        }
    }

    fn connected(&mut self, id: HandlerId) {
        self.handlers.insert(id);
    }

    fn disconnected(&mut self, id: HandlerId) {
        self.handlers.remove(&id);
    }
}
