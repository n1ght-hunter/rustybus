//! # Rusty sub is a simple library for creating a pub/sub system in Rust.

mod event_trait;

use std::{
    any::{Any, TypeId},
    collections::{HashMap, HashSet},
    sync::{Arc, atomic::AtomicUsize},
};

use event_trait::EventGroup;
use parking_lot::RwLock;

type Callback = Box<dyn Fn(Arc<dyn Any>) -> bool>;

#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("Already subscribed")]
    AlreadySubscribed,
    #[error("Not subscribed")]
    NotSubscribed,
    #[error("callback not found")]
    CallbackNotFound,
    #[error("Multiple errors occured {0:?}")]
    Muti(Vec<BusError>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallBackId {
    id: usize,
}

pub struct Bus {
    current_id: AtomicUsize,
    event_table: RwLock<HashMap<TypeId, HashSet<CallBackId>>>,
    subscribers: RwLock<HashMap<CallBackId, Subscribers>>,
}

pub struct Subscribers {
    callback: Callback,
    event_ids: HashSet<TypeId>,
}

impl Subscribers {
    pub fn new(callback: Callback) -> Self {
        Subscribers {
            callback,
            event_ids: HashSet::new(),
        }
    }

    pub fn subscribe_to(&mut self, id: TypeId) -> bool {
        self.event_ids.insert(id)
    }

    pub fn call(&self, event: Arc<dyn Any>) -> bool {
        (self.callback)(event)
    }
}

impl Bus {
    pub fn new() -> Self {
        Bus {
            current_id: AtomicUsize::new(0),
            event_table: RwLock::new(HashMap::new()),
            subscribers: RwLock::new(HashMap::new()),
        }
    }

    /// Get the next id for a callback
    fn next_id(&self) -> CallBackId {
        CallBackId {
            id: self
                .current_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        }
    }

    pub fn once<T: Any + 'static, F: FnOnce(Arc<dyn Any>) + 'static>(&self, callback: F) {
        let once = parking_lot::Mutex::new(Some(callback));
        let id = self.add_callback(move |event| {
            if let Some(callback) = once.lock().take() {
                callback(event);
            }
            false
        });
        self.subscribe_to::<(T,)>(id).unwrap();
    }

    pub fn add_callback<F: Fn(Arc<dyn Any>) -> bool + 'static>(&self, callback: F) -> CallBackId {
        let id = self.next_id();
        let mut subscribers = self.subscribers.write();
        subscribers.insert(id, Subscribers::new(Box::new(callback)));

        id
    }

    pub fn subscribe_to_one<E: Any>(&self, id: CallBackId) -> Result<(), BusError> {
        self.subscribe_to_event(TypeId::of::<E>(), id)
    }

    pub fn subscribe_to<E: EventGroup>(&self, id: CallBackId) -> Result<(), BusError> {
        let mut errors = Vec::new();
        for event_id in E::event_ids() {
            if let Err(err) = self.subscribe_to_event(event_id, id) {
                errors.push(err);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(BusError::Muti(errors))
        }
    }

    fn subscribe_to_event(&self, event_id: TypeId, id: CallBackId) -> Result<(), BusError> {
        {
            let mut event_table = self.event_table.write();
            let subscribers = event_table.entry(event_id).or_insert(HashSet::new());
            if !subscribers.insert(id) {
                return Err(BusError::AlreadySubscribed);
            }
        }
        let mut subscribers = self.subscribers.write();
        if let Some(subscriber) = subscribers.get_mut(&id) {
            subscriber.subscribe_to(event_id);
        } else {
            return Err(BusError::CallbackNotFound);
        }

        Ok(())
    }

    pub fn publish<E: Any>(&self, event: E) {
        let event = Arc::new(event);
        let event_id = TypeId::of::<E>();
        if let Some(callback_ids) = { self.event_table.read().get(&event_id).cloned() } {
            let subscribers = self.subscribers.read();
            let callbacks = callback_ids
                .iter()
                .filter_map(|callback_id| {
                    if let Some(subscriber) = subscribers.get(callback_id) {
                        if subscriber.call(event.clone()) {
                            return None;
                        } else {
                            return Some(callback_id);
                        }
                    }

                    None
                })
                .collect::<Vec<_>>();
            // drop the lock before removing the callback
            drop(subscribers);

            if !callbacks.is_empty() {
                for callback_id in callbacks {
                    if let Err(err) = self.remove_callback(*callback_id) {
                        tracing::error!("Error removing callback: {:?}", err);
                    }
                }
            }
        }
    }

    pub fn remove_callback(&self, id: CallBackId) -> Result<(), BusError> {
        let subscriber = {
            // remove the subscriber from the subscriber table
            let mut subscribers = self.subscribers.write();
            if let Some(subscriber) = subscribers.remove(&id) {
                subscriber
            } else {
                return Err(BusError::CallbackNotFound);
            }
        };
        // remove subscriber from the event table
        let mut event_table = self.event_table.write();
        for event_id in subscriber.event_ids {
            if let Some(callbacks) = event_table.get_mut(&event_id) {
                callbacks.remove(&id);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEvent;

    struct TestEvent2;

    #[test]
    fn test() {
        let bus = Bus::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let id = bus.add_callback(move |event| {
            tx.send(event).unwrap();
            true
        });

        bus.subscribe_to_one::<i32>(id).unwrap();
        bus.subscribe_to::<(TestEvent, TestEvent2)>(id).unwrap();

        bus.once::<i32, _>(|event| {
            assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
        });

        bus.publish(42);
        bus.publish(TestEvent);
        bus.publish(TestEvent2);

        for (idx, event) in rx.try_iter().enumerate() {
            match idx {
                0 => {
                    assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
                }
                1 => {
                    assert!(event.downcast_ref::<TestEvent>().is_some());
                }
                2 => {
                    assert!(event.downcast_ref::<TestEvent2>().is_some());
                }
                _ => panic!("Too many events"),
            }
        }
    }
}
