use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::HashSet, time::Duration};
use yew::worker::*;
use yew::{
    prelude::*,
    services::interval::{IntervalService, IntervalTask},
};

use browseraft::{Node, Role};

pub struct Worker {
    link: AgentLink<Self>,
    node: Option<Arc<Node>>,
    last_state: Option<Role>,
    handlers: HashSet<HandlerId>,
    interval_task: Option<IntervalTask>,
}

#[derive(Serialize, Deserialize)]
pub enum Input {
    InitializeNode(u32, String),
    StopNode,
}

#[derive(Serialize, Deserialize)]
pub enum Output {
    State(u32, Role),
}

pub enum Message {
    PollState,
    // StateChanged(State),
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
            last_state: None,
            interval_task: None,
        }
    }

    fn update(&mut self, msg: Self::Message) {
        match msg {
            Message::PollState => {
                if let Some(node) = self.node.clone() {
                    let node_id = node.id;
                    let state = node.role();
                    let new_state = if let Some(last) = self.last_state {
                        if last != state {
                            Some(state)
                        } else {
                            None
                        }
                    } else {
                        Some(state)
                    };
                    if let Some(new_state) = new_state {
                        for id in self.handlers.iter() {
                            self.link.respond(*id, Output::State(node_id, new_state))
                        }
                    }
                }
            }
        }
    }

    fn handle_input(&mut self, msg: Self::Input, _id: HandlerId) {
        match msg {
            Input::InitializeNode(id, channel) => {
                self.node = Some(Node::create(id, &channel));
                self.interval_task = Some(IntervalService::spawn(
                    Duration::from_millis(300),
                    self.link.callback(|_| Message::PollState),
                ));
            }

            Input::StopNode => {
                if let Some(ref node) = self.node.take() {
                    node.stop();
                }
                // TODO: just set none?
                if let Some(task) = self.interval_task.take() {
                    drop(task);
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
