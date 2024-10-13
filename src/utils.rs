use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

use parking_lot::Mutex;

use crate::io::{IoHandler, NotificationHandler};

pub(crate) struct Defered<F: FnOnce()>(Option<F>);

impl<F: FnOnce()> Defered<F> {
    pub(crate) fn abort(mut self) {
        self.0.take();
    }
}

impl<F: FnOnce()> Drop for Defered<F> {
    fn drop(&mut self) {
        if let Some(f) = self.0.take() {
            f()
        }
    }
}

pub fn defer<F: FnOnce()>(f: F) -> Defered<F> {
    Defered(Some(f))
}

pub enum Subscription {
    Notification {
        method: &'static str,
        notification_handlers: Option<Arc<Mutex<HashMap<&'static str, NotificationHandler>>>>,
    },

    Io {
        id: i32,
        io_handlers: Option<Weak<Mutex<HashMap<i32, IoHandler>>>>,
    },
}

impl Subscription {
    // Detach the handler from foreground task
    pub(crate) fn detach(&mut self) {
        match self {
            Subscription::Notification {
                notification_handlers,
                ..
            } => *notification_handlers = None,
            Subscription::Io { io_handlers, .. } => *io_handlers = None,
        }
    }
}
