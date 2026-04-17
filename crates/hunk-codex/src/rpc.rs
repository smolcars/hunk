use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::protocol::RequestId;

#[derive(Debug)]
pub struct RequestIdGenerator {
    next: AtomicI64,
}

impl Default for RequestIdGenerator {
    fn default() -> Self {
        Self::new(1)
    }
}

impl RequestIdGenerator {
    pub const fn new(start: i64) -> Self {
        Self {
            next: AtomicI64::new(start),
        }
    }

    pub fn next_request_id(&self) -> RequestId {
        let value = self.next.fetch_add(1, Ordering::Relaxed);
        RequestId::Integer(value)
    }
}

#[derive(Debug, Default)]
pub struct PendingRequestMap<T> {
    inner: Mutex<HashMap<RequestId, T>>,
}

impl<T> PendingRequestMap<T> {
    pub fn insert(&self, request_id: &RequestId, value: T) -> Option<T> {
        let mut guard = self
            .inner
            .lock()
            .expect("pending request map mutex poisoned");
        guard.insert(request_id.clone(), value)
    }

    pub fn remove(&self, request_id: &RequestId) -> Option<T> {
        let mut guard = self
            .inner
            .lock()
            .expect("pending request map mutex poisoned");
        guard.remove(request_id)
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .expect("pending request map mutex poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
