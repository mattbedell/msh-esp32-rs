
use core::{future::poll_fn, task::Poll};

use bbqueue::{
    export::ConstInit,
    prod_cons::stream::StreamGrantW,
    traits::{
        bbqhdl::BbqHandle,
        notifier::{AsyncNotifier, Notifier},
    },
};
use embassy_sync::waitqueue::AtomicWaker;
use esp_hal::dma::{BurstConfig, DmaDescriptor, DmaRxBuffer, Preparation, TransferDirection};

struct Wakee {
    waker: AtomicWaker,
}

impl Wakee {
    pub const fn new() -> Wakee {
        Self {
            waker: AtomicWaker::new(),
        }
    }

    pub fn wake(&self) {
        self.waker.wake();
    }
}

pub struct Notify {
    read: Wakee,
    write: Wakee,
}

impl Notify {
    pub const fn new() -> Notify {
        Self::INIT
    }

    fn wait_with<T, F: FnMut() -> Option<T>>(wk: &Wakee) -> impl AsyncFn(F) -> T {
        async move |mut f| {
            poll_fn(|ctx| {
                if let Some(val) = f() {
                    return Poll::Ready(val);
                }

                wk.waker.register(ctx.waker());

                if let Some(val) = f() {
                    return Poll::Ready(val);
                }
                Poll::Pending
            })
            .await
        }
    }
}

impl Default for Notify {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstInit for Notify {
    const INIT: Self = Self {
        read: Wakee::new(),
        write: Wakee::new(),
    };
}

impl Notifier for Notify {
    fn wake_one_consumer(&self) {
        self.read.wake();
    }
    fn wake_one_producer(&self) {
        self.write.wake();
    }
}

impl AsyncNotifier for Notify {
    async fn wait_for_not_empty<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        Self::wait_with(&self.read)(f).await
    }
    async fn wait_for_not_full<T, F: FnMut() -> Option<T>>(&self, f: F) -> T {
        Self::wait_with(&self.write)(f).await
    }
}

pub struct DmaRx<P> {
    pub descriptor: &'static mut DmaDescriptor,
    grant: P,
}

impl<Q: BbqHandle> DmaRx<StreamGrantW<Q>> {
    pub fn new(mut grant: StreamGrantW<Q>, descriptor: &'static mut DmaDescriptor) -> Self {
        *descriptor = DmaDescriptor::EMPTY;
        descriptor.buffer = grant.as_mut_ptr();
        descriptor.set_size(grant.len());
        descriptor.next = core::ptr::null_mut();
        Self { descriptor, grant }
    }

    pub fn commit(self) -> &'static mut DmaDescriptor {
        self.grant.commit(self.descriptor.len());
        self.descriptor
    }
}

unsafe impl<Q: BbqHandle> DmaRxBuffer for DmaRx<StreamGrantW<Q>> {
    type View = Self;
    type Final = Self;
    fn prepare(&mut self) -> Preparation {
        self.descriptor.reset_for_rx();
        Preparation {
            start: self.descriptor as *mut DmaDescriptor,
            direction: TransferDirection::In,
            accesses_psram: false,
            burst_transfer: BurstConfig::DEFAULT,
            check_owner: None,
            auto_write_back: true,
        }
    }

    fn into_view(self) -> Self::View {
        self
    }

    fn from_view(view: Self::View) -> Self::Final {
        view
    }
}
