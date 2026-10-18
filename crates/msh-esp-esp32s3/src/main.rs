#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use bbqueue::{
    BBQueue,
    nicknames::Texas,
    prod_cons::framed::FramedConsumer,
    traits::{coordination::cas::AtomicCoord, storage::Inline},
};
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::{
    clock::CpuClock,
    dma::DmaError,
    dma_tx_buffer, handler,
    interrupt::{self, Priority},
    peripherals::Interrupt,
    system::Stack,
    timer::timg::TimerGroup,
    uart::{
        self, RxConfig, TxConfig, Uart,
        uhci::{Error as UhciError, RxConfig as UhciRxConfig, Uhci},
    },
};
use esp_rtos::embassy::Executor;
use micropb::MessageDecode;
use msh_esp_core::{
    bbqueue::Notify,
    micropb::{MessageEncode, PbEncoder},
    proto::{FromRadio, PayloadVariant, ToRadio},
    serial::{
        frame::{
            ConsumedFrameBytes, FRAME_MAX_SIZE, FRAME_START1, FRAME_START2, Frame, FrameProcessor,
        },
    },
};
use static_cell::StaticCell;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

static BB: Texas<4096, Notify> = Texas::new();
static DMA_RX_SIGNAL: Signal<CriticalSectionRawMutex, ()> = Signal::new();

#[handler(priority = Priority::Priority1)]
fn on_dma_in_done() {
    let dma = unsafe { &*esp_hal::peripherals::DMA::PTR };
    dma.ch(0)
        .in_int()
        .clr()
        .write(|w| w.in_done().clear_bit_by_one());
    DMA_RX_SIGNAL.signal(());
}

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
            .with_rx(RxConfig::default().with_fifo_full_threshold(64))
            .with_tx(TxConfig::default())
            .with_baudrate(38400),
    )
    .unwrap()
    .with_tx(tx)
    .with_rx(rx);

    let mut uhci = Uhci::new(uart, peripherals.UHCI0, peripherals.DMA_CH0);
    let _ = uhci.apply_rx_config(&UhciRxConfig::default().with_chunk_limit(5));

    let dma = unsafe { &*esp_hal::peripherals::DMA::PTR };
    dma.ch(0)
        .in_int()
        .ena()
        .modify(|_, w| w.in_done().set_bit());

    let reg = unsafe { &*esp_hal::peripherals::UHCI0::PTR };
    reg.conf0()
        .modify(|_, w| w.seper_en().clear_bit().uart_idle_eof_en().set_bit());

    let (uhci_rx, uhci_tx) = uhci.split();

    interrupt::bind_handler(Interrupt::DMA_IN_CH0, on_dma_in_done);
    interrupt::enable(Interrupt::DMA_IN_CH0, Priority::Priority1);

    static CORE_1_STACK: StaticCell<Stack<8192>> = StaticCell::new();
    let stack = CORE_1_STACK.init(Stack::new());

    let trx = BB.framed_producer();
    let drx = BB.framed_consumer();

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

    let to_radio = ToRadio {
        payload_variant: Some(PayloadVariant::WantConfigId(111)),
    };

    let proto_len = to_radio.compute_size();
    let mut tx_buffer = dma_tx_buffer!(6).unwrap();
    let buf = tx_buffer.as_mut_slice();

    buf[0..2].copy_from_slice(&[FRAME_START1, FRAME_START2]);
    buf[2..4].copy_from_slice(&(proto_len as u16).to_be_bytes());
    let mut encoder = PbEncoder::new(&mut buf[4..]);
    to_radio.encode(&mut encoder).unwrap();

    let w_transfer = uhci_tx
        .write(tx_buffer)
        .unwrap_or_else(|e| panic!("{:?}", e.0));

    let (err, _tx_back, _init_buf) = w_transfer.wait();

    if let Err(e) = err {
        error!("WRITE ERR: {}", defmt::Debug2Format(&e));
    }

    let stream = esp_hal::dma_rx_stream_buffer!(4096, 64);
    let mut transfer = uhci_rx.read(stream).unwrap_or_else(|e| panic!("{:?}", e.0));

    let mut frame_parser = FrameProcessor::new();
    let frame_size: u16 = FRAME_MAX_SIZE.try_into().unwrap();
    let mut grant = trx.grant(frame_size).ok();

    loop {
        let data = transfer.peek();
        if data.is_empty() {
            DMA_RX_SIGNAL.wait().await;
            continue;
        }

        if grant.is_none() {
            grant = trx.grant(frame_size).ok();
        }

        let consumed = if let Some(ref mut grant_buf) = grant {
            let res = frame_parser.process(data, grant_buf);

            match res {
                Ok(Frame::Complete { len, .. }) => {
                    grant.take().unwrap().commit(len as u16);
                }
                Ok(Frame::Incomplete { .. }) => {}
                Err(ref err) => {
                    grant.take().unwrap().commit(0);
                    error!("{}", defmt::Debug2Format(err));
                }
            };
            res.consumed()
        } else {
            data.len()
        };

        transfer.consume(consumed);

        if let Err(e) = err {
            match e {
                UhciError::Dma(DmaError::DescriptorError) => {
                    error!("DMA DESCRIPTOR ERROR");
                }
                UhciError::Dma(_) => {
                    error!("DMA ERROR: other");
                }
                _ => {
                    error!("READ ERR: {}", defmt::Debug2Format(&e));
                }
            }
        }
    }
}

type Cons = FramedConsumer<&'static BBQueue<Inline<4096>, AtomicCoord, Notify>>;
#[embassy_executor::task]
async fn core_1_serial(bbq: Cons) {
    info!("core 1 loop starting...");
    loop {
        let grant = bbq.wait_read().await;
        let mut msg = FromRadio::default();

        match msg.decode_from_bytes(&grant) {
            Ok(()) => {
                info!("Decoded protobuf: len: '{}'", msg.compute_size());
            }
            Err(err) => {
                error!("{:?}", defmt::Debug2Format(&err));
            }
        }

        grant.release();
    }
}
