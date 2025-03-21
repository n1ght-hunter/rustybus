//! # Rusty sub is a simple library for creating a pub/sub system in Rust.
//! 
//! All events are identified by their type id and can be subscribed to with there type.
//! 
//! ## Example
//! ```rust
//! use rustybus::Bus;
//! let bus = Bus::new();
//! bus.once::<i32, _>(|event| {
//!     assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
//! });
//! bus.publish(42);
//! ```
//! 

mod event_group;
mod event;

pub use event::Event;

use std::{
    any::{Any, TypeId}, collections::{HashMap, HashSet}, ops::Deref, sync::{atomic::AtomicUsize, Arc}
};

use event_group::EventGroup;
use parking_lot::RwLock;


type Callback = Box<dyn Fn(Event) -> bool>;

#[derive(Debug, thiserror::Error)]
pub enum BusError {
    #[error("Already subscribed")]
    AlreadySubscribed,
    #[error("Not subscribed")]
    NotSubscribed,
    #[error("callback not found")]
    CallbackNotFound,
    #[error("SubscribeToMany multiple errors: {0:?}")]
    SubscribeToMany(HashMap<TypeId, BusError>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallBackId {
    id: usize,
}

#[derive(Default, Clone)]
pub struct Bus {
    inner: Arc<BusInner>,
}


impl Deref for Bus {
    type Target = BusInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[derive(Default)]
pub struct BusInner {
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

    pub fn call(&self, event: Event) -> bool {
        (self.callback)(event)
    }
}

// private methods
impl Bus {
    /// Get the next id for a callback
    fn next_id(&self) -> CallBackId {
        CallBackId {
            id: self
                .current_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        }
    }

    /// Subscribe to an event
    fn subscribe_to_event(&self, event_id: TypeId, id: CallBackId) -> Result<(), BusError> {
        // add the subscriber to the event table
        {
            let mut event_table = self.event_table.write();
            let subscribers = event_table.entry(event_id).or_insert(HashSet::new());
            if !subscribers.insert(id) {
                return Err(BusError::AlreadySubscribed);
            }
        }
        // update the subscriber with the event id
        let mut subscribers = self.subscribers.write();
        if let Some(subscriber) = subscribers.get_mut(&id) {
            subscriber.subscribe_to(event_id);
        } else {
            return Err(BusError::CallbackNotFound);
        }

        Ok(())
    }

    fn publish_inner<F>(&self, event: F, event_id: TypeId) 
    where
        F: Fn() -> Event,
    {
        if let Some(callback_ids) = { self.event_table.read().get(&event_id).cloned() } {
            let subscribers = self.subscribers.read();
            let callbacks = callback_ids
                .iter()
                .filter_map(|callback_id| {
                    if let Some(subscriber) = subscribers.get(callback_id) {
                        if subscriber.call(event()) {
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
}

impl Bus {
    /// Create a new bus
    /// 
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// bus.once::<i32, _>(|event| {
    /// assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
    /// });
    /// bus.publish(42);
    /// 
    /// let id = bus.add_callback(|event| {
    ///  true
    /// });
    /// 
    /// bus.subscribe_to_one::<i32>(id);
    /// bus.publish(42);
    /// ```
    pub fn new() -> Self {
        Bus {
            inner: Arc::new(BusInner::default()),
        }
    }



    /// Add a callback that will only be called once and for a specific event and then removed
    /// 
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// bus.once::<i32, _>(|event| {
    ///  assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
    /// });
    /// bus.publish(42);
    /// ```
    pub fn once<T: Any + 'static, F: FnOnce(Event) + 'static>(&self, callback: F) {
        let once = parking_lot::Mutex::new(Some(callback));
        let id = self.add_callback(move |event| {
            if let Some(callback) = once.lock().take() {
                callback(event);
            }
            false
        });
        self.subscribe_to::<(T,)>(id).unwrap();
    }

    /// Add a callback to the bus
    /// 
    /// a bool is returned from the callback to indicate if the callback should continue to be called \
    /// if [`true`] the callback will continue to be called \
    /// if [`false`] the callback will be removed from the bus and will no longer be called
    /// 
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// let id = bus.add_callback(|event| {
    ///   true
    /// });
    /// ```
    pub fn add_callback<F: Fn(Event) -> bool + 'static>(&self, callback: F) -> CallBackId {
        let id = self.next_id();
        let mut subscribers = self.subscribers.write();
        subscribers.insert(id, Subscribers::new(Box::new(callback)));

        id
    }

    /// Subscribe to a single event
    /// 
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// let id = bus.add_callback(|event| {
    ///   true
    /// });
    /// bus.subscribe_to_one::<i32>(id);
    /// ```
    pub fn subscribe_to_one<E: Any>(&self, id: CallBackId) -> Result<(), BusError> {
        self.subscribe_to_event(TypeId::of::<E>(), id)
    }

    /// Subscribe to multiple events
    /// 
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// let id = bus.add_callback(|event| {
    ///   true
    /// });
    /// bus.subscribe_to::<(i32, String)>(id);
    /// ```
    pub fn subscribe_to<E: EventGroup>(&self, id: CallBackId) -> Result<(), BusError> {
        let mut errors = HashMap::new();
        for event_id in E::event_ids() {
            if let Err(err) = self.subscribe_to_event(event_id, id) {
                errors.insert(event_id, err);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(BusError::SubscribeToMany(errors))
        }
    }



    /// Publish an event to the bus
    /// 
    /// the event id is the type id
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// bus.publish(42);
    /// ```
    pub fn publish<E: Any + Clone + Send + Sync>(&self, event: E) {
        self.publish_inner(
            || Event::Boxed(Box::new(event.clone())),
            TypeId::of::<E>(),
        );
    }

    /// Publish an event to the bus as an Arc
    /// 
    /// the event id is the type id
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// bus.publish_arc(42);
    /// ```
    pub fn publish_arc<E: Any + Send + Sync>(&self, event: E) {
        let event: Arc<dyn Any + Send + Sync> = Arc::new(event);
        self.publish_inner(
            || Event::Arc(Arc::clone(&event)),
            TypeId::of::<E>(),
        );
    }

    /// Remove a callback from the bus
    /// 
    /// # Example
    /// ```
    /// use rustybus::Bus;
    /// let bus = Bus::new();
    /// let id = bus.add_callback(|event| {
    ///    true
    /// });
    /// bus.remove_callback(id);
    /// ```
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

    #[derive(Clone)]
    struct TestEvent;

    #[derive(Clone)]
    struct TestEvent2;

    #[test]
    fn test() {
        let bus = Bus::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let id = bus.add_callback(move |event| {
            let mut continue_processing = true;
            if event.is::<TestEvent2>() {
                continue_processing = false;
            }
            tx.send(event).unwrap();
            continue_processing
        });

        bus.subscribe_to_one::<i32>(id).unwrap();
        bus.subscribe_to::<(TestEvent, TestEvent2)>(id).unwrap();

        bus.once::<i32, _>(|event| {
            assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
        });

        bus.publish(42);
        bus.publish(TestEvent);
        bus.publish(TestEvent2);

        while let Ok(event) = rx.recv() {
            if event.is::<i32>() {
                assert_eq!(*event.downcast_ref::<i32>().unwrap(), 42);
            } else if event.is::<TestEvent>() {
                assert!(true);
            } else if event.is::<TestEvent2>() {
                assert!(true);
            } else {
                panic!("Unknown event type");
            }
        }
    }
}
