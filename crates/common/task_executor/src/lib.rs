use std::sync::Weak;

use async_channel;
use futures::{channel::mpsc::Sender, prelude::*};
use tokio::runtime::{Handle, Runtime};
pub use tokio::task::JoinHandle;

#[derive(Clone)]
pub enum HandleProvider {
    Runtime(Weak<Runtime>),
    Handle(Handle),
}

impl From<Handle> for HandleProvider {
    fn from(handle: Handle) -> Self {
        HandleProvider::Handle(handle)
    }
}

impl From<Weak<Runtime>> for HandleProvider {
    fn from(weak_runtime: Weak<Runtime>) -> Self {
        HandleProvider::Runtime(weak_runtime)
    }
}

impl HandleProvider {
    pub fn handle(&self) -> Option<Handle> {
        match self {
            HandleProvider::Runtime(weak_runtime) => weak_runtime
                .upgrade()
                .map(|runtime| runtime.handle().clone()),
            HandleProvider::Handle(handle) => Some(handle.clone()),
        }
    }
}

#[derive(Clone)]
pub struct TaskExecutor {
    handle_provider: HandleProvider,
    exit: async_channel::Receiver<()>,
}

impl TaskExecutor {
    pub fn new<T: Into<HandleProvider>>(handle: T, exit: async_channel::Receiver<()>) -> Self {
        Self {
            handle_provider: handle.into(),
            exit,
        }
    }

    pub fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        if let Some(handle) = self.handle() {
            let exit = self.exit();
            handle.spawn(async move {
                futures::pin_mut!(exit);
                match future::select(Box::pin(task), exit).await {
                    future::Either::Left(_) => (),
                    future::Either::Right(_) => (),
                }
            });
        }
    }

    pub fn spawn_blocking<F>(&self, task: F)
    where
        F: FnOnce() -> () + Send + 'static,
    {
        if let Some(handle) = self.handle() {
            handle.spawn_blocking(task);
        }
    }

    pub fn spawn_handle<R: Send + 'static>(
        &self,
        task: impl Future<Output = R> + Send + 'static,
    ) -> Option<JoinHandle<Option<R>>> {
        let exit = self.exit();

        if let Some(handle) = self.handle() {
            Some(handle.spawn(async move {
                futures::pin_mut!(exit);
                match future::select(Box::pin(task), exit).await {
                    future::Either::Left((value, _)) => Some(value),
                    future::Either::Right(_) => None,
                }
            }))
        } else {
            None
        }
    }

    pub fn handle(&self) -> Option<Handle> {
        self.handle_provider.handle()
    }

    pub fn exit(&self) -> impl Future<Output = ()> {
        let exit = self.exit.clone();
        async move {
            let _ = exit.recv().await;
        }
    }
}
