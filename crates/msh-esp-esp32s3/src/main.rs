#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::str;

use bbqueue::{
    BBQueue,
    nicknames::Texas,
    prod_cons::stream::StreamConsumer,
    traits::{coordination::cas::AtomicCoord, storage::Inline},
};
use defmt::{error, info};
use embassy_executor::Spawner;
use esp_hal::{
    clock::CpuClock,
    dma::DmaDescriptor,
    system::Stack,
    timer::timg::TimerGroup,
    uart::{
        self, RxConfig, Uart,
        uhci::{RxConfig as UhciRxConfig, Uhci},
    },
};
use esp_rtos::embassy::Executor;
use msh_esp_rs::{BBNotify, BbDmaRx};
use static_cell::StaticCell;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

static DMA_DESC: StaticCell<DmaDescriptor> = StaticCell::new();
static BB: Texas<4096, BBNotify> = Texas::new();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(_spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO2
    // - GPIO5
    // - GPIO12
    // - GPIO15
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _ = peripherals.GPIO6;
    let _ = peripherals.GPIO7;
    let _ = peripherals.GPIO8;
    let _ = peripherals.GPIO9;
    let _ = peripherals.GPIO10;
    let _ = peripherals.GPIO11;
    let _ = peripherals.GPIO16;
    let _ = peripherals.GPIO20;

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);

    let tx = peripherals.GPIO1;
    let rx = peripherals.GPIO2;

    let uart = Uart::new(
        peripherals.UART1,
        uart::Config::default()
            .with_rx(RxConfig::default())
            .with_baudrate(38400),
    )
    .unwrap()
    .with_tx(tx)
    .with_rx(rx);

    let mut uhci = Uhci::new(uart, peripherals.UHCI0, peripherals.DMA_CH0);
    let _ = uhci.apply_rx_config(&UhciRxConfig::default().with_chunk_limit(5));

    let reg = unsafe { &*esp_hal::peripherals::UHCI0::PTR };
    reg.conf0()
        .modify(|_, w| w.seper_en().clear_bit().uart_idle_eof_en().set_bit());

    let uhci = uhci.into_async();
    let (mut uhci_rx, _uhci_tx) = uhci.split();

    static CORE_1_STACK: StaticCell<Stack<8192>> = StaticCell::new();
    let stack = CORE_1_STACK.init(Stack::new());

    let trx = BB.stream_producer();
    let drx = BB.stream_consumer();

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw_interrupt.software_interrupt1,
        stack,
        move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            // let mut delay: Delay = Delay::default();
            // let mut epd = Epd2in13::new(&mut spi_device, bsy, dc, rst, &mut delay, None).unwrap();

            executor.run(|spawner| {
                // spawner.spawn(core_1_display(epd, spi_device).unwrap());
                spawner.spawn(core_1_serial(drx).unwrap());
            });
        },
    );

    let descriptor = DMA_DESC.init(DmaDescriptor::EMPTY);
    let mut desc = descriptor;

    info!("main loop starting...");
    loop {
        let grant = trx.grant_max_remaining(1024).unwrap();
        info!("obtained grant...");
        let dma_rx = BbDmaRx::new(grant, desc);
        let mut transfer = uhci_rx.read(dma_rx).unwrap_or_else(|e| panic!("{:?}", e.0));
        info!("dma read...");
        transfer.wait_for_done().await;
        info!("wait done...");
        let (err, rx_back, finished) = transfer.wait();
        info!("transfer done...");
        uhci_rx = rx_back;
        err.unwrap();

        desc = finished.commit();
        info!("commit done...");
    }
}

type Cons = StreamConsumer<&'static BBQueue<Inline<4096>, AtomicCoord, BBNotify>>;
#[embassy_executor::task]
async fn core_1_serial(bbq: Cons) {
    info!("core 1 loop starting...");
    loop {
        let grant = bbq.wait_read().await;
        match str::from_utf8(&grant) {
            Ok(val) => info!("{}", val),

            Err(_e) => {
                error!("Decode err");
            }
        }
        let len = grant.len();
        grant.release(len);
    }
}
