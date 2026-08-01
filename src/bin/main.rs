#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use msh_esp_rs::BBNotify;

use bbqueue::{
    BBQueue,
    nicknames::Texas,
    prod_cons::stream::StreamConsumer,
    traits::{
        bbqhdl::BbqHandle,
        coordination::cas::AtomicCoord,
        notifier::{AsyncNotifier, Notifier},
        storage::Inline,
    },
};
use defmt::{info, error};
use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker, Timer};
use embedded_graphics::{
    mono_font::{MonoFont, MonoTextStyle, MonoTextStyleBuilder, ascii::FONT_9X18_BOLD},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, LineHeight, Text},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use epd_waveshare::{
    color::Color,
    epd2in13_v2::{Display2in13, Epd2in13},
    prelude::{DisplayRotation, RefreshLut, WaveshareDisplay},
};
use esp_hal::{
    Blocking,
    clock::CpuClock,
    delay::Delay,
    dma::{Channel, DmaDescriptor, DmaRxBuf, DmaRxStreamBuf},
    dma_buffers,
    gpio::{Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
    peripherals::{SPI2, UART1, UHCI0},
    spi::master::{AnySpi, Config, Spi},
    system::Stack,
    timer::timg::TimerGroup,
    uart::{self, RxConfig, Uart},
};
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;

static BB: Texas<4096, BBNotify> = Texas::new();

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

enum MsgStreamState {
    Searching,
    Start0,
    DecodeLen,
    Decode,
}

impl MsgStreamState {
    const START_0: u8 = 0x94;
    const START_1: u8 = 0xc3;
    const MAX_LEN: u16 = 512;
    pub fn new() -> Self {
        Self::Searching
    }
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
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

    // let spi = Spi::new(peripherals.SPI2, Config::default())
    //     .unwrap()
    //     .with_sck(peripherals.GPIO18)
    //     .with_mosi(peripherals.GPIO23);

    let tx = peripherals.GPIO16;
    let rx = Input::new(
        peripherals.GPIO17,
        InputConfig::default().with_pull(Pull::Up),
    );

    let uart = Uart::new(
        peripherals.UART1,
        uart::Config::default()
            .with_rx(
                RxConfig::default()
                    .with_fifo_full_threshold(16)
                    .with_timeout(4),
            )
            .with_clock_source(uart::ClockSource::Apb)
            .with_baudrate(38400)
            .with_data_bits(uart::DataBits::_8)
            .with_parity(uart::Parity::None)
            .with_stop_bits(uart::StopBits::_1),
    )
    .unwrap()
    .with_tx(tx)
    .with_rx(rx)
    .into_async();

    let (mut rx_uart, tx_uart) = uart.split();

    // let (rx_buf, rx_descript, tx_buf, tx_descript) = dma_buffers!(4096, 512);
    // let mut rx_dma = DmaRxBuf::new(rx_descript, rx_buf).unwrap();

    // let bsy = Input::new(
    //     peripherals.GPIO15,
    //     InputConfig::default().with_pull(Pull::None),
    // );
    // let dc = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    // let rst = Output::new(peripherals.GPIO22, Level::High, OutputConfig::default());
    // let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    // let delay: Delay = Delay::default();

    // let mut spi_device = ExclusiveDevice::new(spi, cs, delay).unwrap();

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

    let _ = spawner;

    // let mut ticker = Ticker::every(Duration::from_millis(2000));

    loop {
        let mut grant = trx.grant_max_remaining(64).unwrap();

        match rx_uart.read_async(&mut grant).await {
            Ok(read) => {
                grant.commit(read);
            }
            Err(err) => {
                error!(">>>>>>>>>>>>>>>>>>>> {err}");
                let _ = rx_uart.check_for_errors().inspect_err(|e| {
                    error!(">>> check: {e}");
                });
                grant.commit(0);
            }
        }
    }
}

type SpiBus = Spi<'static, Blocking>;
type ExclDvc = ExclusiveDevice<SpiBus, Output<'static>, Delay>;
type EPDisplay = Epd2in13<ExclDvc, Input<'static>, Output<'static>, Output<'static>, Delay>;

#[embassy_executor::task]
async fn core_1_display(mut display: EPDisplay, mut spi: ExclDvc) {
    let mut delay = Delay::new();
    let mut buf = Display2in13::default();
    buf.set_rotation(DisplayRotation::Rotate90);
    buf.clear(Color::White);

    let style = MonoTextStyleBuilder::new()
        .font(&FONT_9X18_BOLD)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();

    display.set_background_color(Color::White);
    display.clear_frame(&mut spi, &mut delay).unwrap();
    display.display_frame(&mut spi, &mut delay).unwrap();

    display
        .set_refresh(&mut spi, &mut delay, RefreshLut::Quick)
        .unwrap();

    let mut txt = Text::new("Hello world", Point::new(20, 30), style);
    txt.draw(&mut buf).unwrap();

    let mut ticker = Ticker::every(Duration::from_millis(500));

    loop {
        ticker.next().await;
        display.wake_up(&mut spi, &mut delay).unwrap();
        txt.translate_mut(Point::new(1, 0));
        txt.draw(&mut buf).unwrap();
        display
            .update_and_display_frame(&mut spi, buf.buffer(), &mut delay)
            .unwrap();
        display.sleep(&mut spi, &mut delay).unwrap();
    }
}
type Cons = StreamConsumer<&'static BBQueue<Inline<4096>, AtomicCoord, BBNotify>>;
#[embassy_executor::task]
async fn core_1_serial(bbq: Cons) {
    info!("spawn serial task");
    let mut msg_state = MsgStreamState::new();
    let mut msg_len = 0;
    let mut msg_len_bytes: [u8; 2] = [0; 2];
    let mut len_idx = 0;
    let mut payload_idx = 0;
    loop {
        let mut offset = 0;
        let grant = bbq.wait_read().await;
        offset = grant.len();

        // while offset < grant.len() {
        //     let data = &grant[offset..];
        //     match msg_state {
        //         MsgStreamState::Searching => {
        //             if let Some(pos) = data.windows(2).position(|w| {
        //                 w[0] == MsgStreamState::START_0 && w[1] == MsgStreamState::START_1
        //             }) {
        //                 offset += pos + 2;
        //                 len_idx = 0;
        //                 msg_state = MsgStreamState::DecodeLen;
        //             } else {
        //                 if let Some(last) = data.last()
        //                     && *last == MsgStreamState::START_0
        //                 {
        //                     msg_state = MsgStreamState::Start0;
        //                 }
        //                 offset += data.len();
        //             }
        //         }

        //         MsgStreamState::Start0 => {
        //             if let Some(first) = grant.first()
        //                 && *first == MsgStreamState::START_1
        //             {
        //                 offset += 1;
        //                 len_idx = 0;
        //                 info!("found start");
        //                 msg_state = MsgStreamState::DecodeLen;
        //             } else {
        //                 msg_state = MsgStreamState::Searching;
        //             }
        //         }
        //         MsgStreamState::DecodeLen => {
        //             let needed = 2 - len_idx;
        //             let to_copy = needed.min(data.len());
        //             msg_len_bytes[len_idx..len_idx + to_copy].copy_from_slice(&data[..to_copy]);

        //             offset += to_copy;
        //             len_idx += to_copy;

        //             if len_idx == 2 {
        //                 let body_len = u16::from_be_bytes(msg_len_bytes);
        //                 if body_len <= MsgStreamState::MAX_LEN && body_len > 0 {
        //                     msg_len = body_len;
        //                     payload_idx = 0;
        //                     info!("found len: {msg_len}");
        //                     msg_state = MsgStreamState::Decode;
        //                 } else {
        //                     msg_state = MsgStreamState::Searching;
        //                 }
        //             }
        //         }
        //         MsgStreamState::Decode => {
        //             let needed = msg_len - payload_idx;
        //             let to_copy = needed.min(data.len() as u16);
        //             offset += to_copy as usize;
        //             payload_idx += to_copy;
        //             if payload_idx == msg_len {
        //                 info!("finished decoding");
        //                 msg_state = MsgStreamState::Searching;
        //             }
        //         }
        //     }
        // }
        grant.release(offset);
    }
}
