#![no_std]

use core::{
    future::poll_fn,
    task::Poll,
};

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
    // flag: AtomicBool,
}

impl Wakee {
    pub const fn new() -> Wakee {
        Self {
            waker: AtomicWaker::new(),
            // flag: AtomicBool::new(false),
        }
    }

    pub fn wake(&self) {
        self.waker.wake();
    }

    // pub fn flag(&self) -> &AtomicBool {
    //     &self.flag
    // }
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
        // self.read.flag.store(true, Ordering::Release);
        self.read.wake();
    }
    fn wake_one_producer(&self) {
        // self.write.flag.store(true, Ordering::Release);
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

pub struct BbDmaRx<P> {
    descriptor: DmaDescriptor,
    grant: P,
}

impl<Q: BbqHandle> BbDmaRx<StreamGrantW<Q>> {
    pub fn new(mut grant: StreamGrantW<Q>) -> Self {
        let mut descriptor = DmaDescriptor::EMPTY;
        descriptor.buffer = grant.as_mut_ptr();
        descriptor.set_size(grant.len());
        descriptor.next = core::ptr::null_mut();
        Self { descriptor, grant }
    }

    pub fn commit(self) {
        self.grant.commit(self.descriptor.len());
    }
}

unsafe impl<Q: BbqHandle> DmaRxBuffer for BbDmaRx<StreamGrantW<Q>> {
    type View = Self;
    type Final = Self;
    fn prepare(&mut self) -> Preparation {
        self.descriptor.reset_for_rx();
        Preparation {
            start: &mut self.descriptor as *mut DmaDescriptor,
            direction: TransferDirection::In,
            accesses_psram: false,
            burst_transfer: BurstConfig::DEFAULT,
            check_owner: None,
            auto_write_back: false,
        }
    }

    fn into_view(self) -> Self::View {
        self
    }

    fn from_view(view: Self::View) -> Self::Final {
        view
    }
}

// impl<'a> BbDmaRx<'a> {
//     pub fn new(mut grant: GrantW<'a>) -> Self {}

//     pub fn received_len(&self) -> usize {}
// }
