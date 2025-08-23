use std::sync::Arc;

use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Clone)]
pub struct Reader<T>(Arc<RwLock<T>>);

pub struct Writer<T>(Arc<RwLock<T>>);

impl<T> Writer<T> {
    pub fn new(value: T) -> (Self, Reader<T>) {
        let arc = Arc::new(RwLock::new(value));
        (Self(arc.clone()), Reader(arc))
    }

    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        self.0.read().await
    }

    pub async fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.0.write().await
    }

    pub fn reader(&self) -> Reader<T> {
        Reader(self.0.clone())
    }
}

impl<T> Reader<T> {
    pub async fn read(&self) -> RwLockReadGuard<'_, T> {
        self.0.read().await
    }
}
