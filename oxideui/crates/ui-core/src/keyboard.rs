//! Keyboard geometry tracking and event fan-out for OxideUI.

use crate::prelude::platform_api as api;
use alloc::collections::BTreeMap;

#[derive(Default)]
pub struct KeyboardTracker {
    info: api::KeyboardGeometry,
    listeners: BTreeMap<u64, Box<dyn Fn(api::KeyboardEvent) + Send + Sync>>,
    next_id: u64,
}

impl KeyboardTracker {
    #[must_use]
    pub fn new() -> Self {
        Self { info: api::KeyboardGeometry::default(), listeners: BTreeMap::new(), next_id: 1 }
    }

    #[must_use]
    pub fn geometry(&self) -> api::KeyboardGeometry {
        self.info
    }

    pub fn on_event(&mut self, event: api::KeyboardEvent) {
        if let Some(transition) = event.transition() {
            self.info = transition.geometry;
        }
        for listener in self.listeners.values() {
            listener(event);
        }
    }

    #[must_use]
    pub fn add_listener<F>(&mut self, listener: F) -> u64
    where
        F: Fn(api::KeyboardEvent) + Send + Sync + 'static,
    {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.listeners.insert(id, Box::new(listener));
        id
    }

    pub fn remove_listener(&mut self, id: u64) {
        self.listeners.remove(&id);
    }
}

pub trait KeyboardEventExt {
    fn transition(&self) -> Option<api::KeyboardTransition>;
}

impl KeyboardEventExt for api::KeyboardEvent {
    fn transition(&self) -> Option<api::KeyboardTransition> {
        match *self {
            api::KeyboardEvent::WillChange(tx) | api::KeyboardEvent::DidChange(tx) => Some(tx),
        }
    }
}

extern crate alloc;
