#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use embedded_graphics::{
    mono_font::{ascii::FONT_9X18_BOLD, MonoFont, MonoTextStyle, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, LineHeight, Text},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use epd_waveshare::{
    color::Color,
    epd2in13_v2::{Display2in13, Epd2in13},
    prelude::{RefreshLut, WaveshareDisplay, DisplayRotation},
};
use esp_hal::{
    Blocking,
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull},
    peripherals::SPI2,
    spi::master::{AnySpi, Config, Spi},
    system::Stack,
    timer::timg::TimerGroup,
};
use esp_rtos::embassy::Executor;
use log::{error, info};
use static_cell::StaticCell;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

#[embassy_executor::task]
async fn core_1_display() {
    let mut ticker = Ticker::every(Duration::from_secs(2));
    loop {
        ticker.next().await;
        info!("Core 1 tick");
    }
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

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

    let spi = Spi::new(peripherals.SPI2, Config::default())
        .unwrap()
        .with_sck(peripherals.GPIO18)
        .with_mosi(peripherals.GPIO23);

    let bsy = Input::new(
        peripherals.GPIO15,
        InputConfig::default().with_pull(Pull::None),
    );
    let dc = Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default());
    let rst = Output::new(peripherals.GPIO22, Level::High, OutputConfig::default());
    let cs = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());

    let delay: Delay = Delay::default();

    let mut spi_device = ExclusiveDevice::new(spi, cs, delay).unwrap();

    static CORE_1_STACK: StaticCell<Stack<8192>> = StaticCell::new();
    let stack = CORE_1_STACK.init(Stack::new());

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw_interrupt.software_interrupt1,
        stack,
        move || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            let mut delay: Delay = Delay::default();
            let mut epd = Epd2in13::new(&mut spi_device, bsy, dc, rst, &mut delay, None).unwrap();

            executor.run(|spawner| {
                spawner.spawn(core_1_main(epd, spi_device).unwrap());
            });
        },
    );

    let _ = spawner;

    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        info!("Core 0 tick");
        ticker.next().await;
    }
}

type SpiBus = Spi<'static, Blocking>;
type ExclDvc = ExclusiveDevice<SpiBus, Output<'static>, Delay>;
type EPDisplay = Epd2in13<ExclDvc, Input<'static>, Output<'static>, Output<'static>, Delay>;

#[embassy_executor::task]
async fn core_1_main(mut display: EPDisplay, mut spi: ExclDvc) {
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

    display.set_refresh(&mut spi, &mut delay, RefreshLut::Quick).unwrap();

    let mut txt = Text::new("Hello world", Point::new(20, 30), style);
    txt.draw(&mut buf).unwrap();


    let mut ticker = Ticker::every(Duration::from_millis(500));

    loop {
        info!("Core 1 tick");
        ticker.next().await;
        display.wake_up(&mut spi, &mut delay).unwrap();
        txt.translate_mut(Point::new(1, 0));
        txt.draw(&mut buf).unwrap();
        display.update_and_display_frame(&mut spi, buf.buffer(), &mut delay).unwrap();
        display.sleep(&mut spi, &mut delay).unwrap();
    }
}
