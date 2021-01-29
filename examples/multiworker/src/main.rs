#![recursion_limit = "256"]

use rand::{rngs::OsRng, Rng};
use std::collections::HashMap;
use yew::prelude::*;
use yew::worker::{Bridge, Bridged};

mod worker;
use worker::Worker;

use browseraft::Role;

fn main() {
    yew::start_app::<App>();
}

pub struct App {
    link: ComponentLink<Self>,
    rng: OsRng,
    workers: HashMap<u32, Box<dyn Bridge<Worker>>>,
    states: HashMap<u32, Role>,
}

pub enum Message {
    AddWorker,
    RemoveWorker(u32),
    WorkerMessage(worker::Output),
}

impl Component for App {
    type Message = Message;
    type Properties = ();

    fn create(_props: Self::Properties, link: ComponentLink<Self>) -> Self {
        Self {
            link,
            rng: OsRng::new().expect("failed to create RNG"),
            workers: HashMap::new(),
            states: HashMap::new(), // TODO: combine these hash maps?
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Message::AddWorker => {
                let id = self.rng.gen::<u32>();
                let mut bridge = Worker::bridge(self.link.callback(Message::WorkerMessage));
                bridge.send(worker::Input::Initialize(id));
                self.workers.insert(id, bridge);
                self.states.insert(id, Role::Follower);
                true
            }

            Message::RemoveWorker(id) => {
                if let Some(mut bridge) = self.workers.remove(&id) {
                    bridge.send(worker::Input::Stop)
                };
                true
            }

            Message::WorkerMessage(msg) => match msg {
                worker::Output::Message(_msg) => {
                    //
                    false
                }
                worker::Output::State(id, state) => {
                    yew::services::ConsoleService::log(
                        format!("worker message: {:?}, {:?}", id, state).as_str(),
                    );
                    self.states.insert(id, state);
                    true
                }
            },
        }
    }

    fn view(&self) -> Html {
        html! {
            <div>
                <h1>{"Multiple Workers"}</h1>
                <button onclick={self.link.callback(|_| Message::AddWorker)}>{"Add Worker"}</button>
                {
                    for self.workers.iter().map(|(id, bridge) | {
                        html! {
                            <p>
                                <span>{id}</span>
                                <span>{
                                    if let Some(state) = self.states.get(id) {
                                        format!("{}", state)
                                    } else {
                                        String::from("None")
                                    }
                                }</span>
                                <button onclick={self.remove_click(*id)}>{"Remove"}</button>
                            </p>
                        }
                    })
                }
            </div>
        }
    }
}

impl App {
    fn remove_click(&self, id: u32) -> Callback<yew::MouseEvent> {
        self.link.callback(move |_| Message::RemoveWorker(id))
    }
}
