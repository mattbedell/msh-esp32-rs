#![no_std]

use core::{
    future::poll_fn,
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};

use bbqueue::{export::ConstInit, traits::notifier::{AsyncNotifier, Notifier}};
use embassy_sync::waitqueue::AtomicWaker;

struct Wakee {
    waker: AtomicWaker,
    flag: AtomicBool,
}

impl Wakee {
    pub const fn new() -> Wakee {
        Self {
            waker: AtomicWaker::new(),
            flag: AtomicBool::new(false),
        }
    }

    pub fn wake(&self) {
        self.waker.wake();
    }

    pub fn flag(&self) -> &AtomicBool {
        &self.flag
    }
}

pub struct BBNotify {
    read: Wakee,
    write: Wakee,
}

impl BBNotify {
    pub const fn new() -> BBNotify {
        Self::INIT
    }

    fn wait_with<T, F: FnMut() -> Option<T>>(wk: &Wakee) -> impl AsyncFn(F) -> T {
        async move |mut f| {
            poll_fn(|ctx| {
                if let Some(val) = f() {
                    Poll::Ready(val)
                } else {
                    if wk.flag().swap(false, Ordering::Acquire) {
                        ctx.waker().wake_by_ref();
                    } else {
                        wk.waker.register(ctx.waker());
                    }
                    Poll::Pending
                }
            })
            .await
        }
    }
}

impl Default for BBNotify {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstInit for BBNotify {
    const INIT: Self = Self {
        read: Wakee::new(),
        write: Wakee::new(),
    };
}

impl Notifier for BBNotify {
    fn wake_one_consumer(&self) {
        self.read.flag.store(true, Ordering::Release);
        self.read.wake();
    }
    fn wake_one_producer(&self) {
        self.write.flag.store(true, Ordering::Release);
        self.write.wake();
    }
}

impl AsyncNotifier for BBNotify {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        Self::wait_with(&self.read)(f).await
    }
    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        Self::wait_with(&self.write)(f).await
    }
}
