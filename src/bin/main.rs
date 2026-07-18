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
use embedded_graphics::pixelcolor::BinaryColor::On as Black;
use epd_waveshare::epd2in13_v2;
use esp_hal::clock::CpuClock;
use esp_hal::system::Stack;
use esp_hal::timer::timg::TimerGroup;
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
async fn core_1_tick() {
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
    let sw_interrupt = esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);


    static CORE_1_STACK: StaticCell<Stack<8192>> = StaticCell::new();
    let stack = CORE_1_STACK.init(Stack::new());

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw_interrupt.software_interrupt1,
        stack,
        || {
            static EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = EXECUTOR.init(Executor::new());
            executor.run(|spawner| {
                spawner.spawn(core_1_tick().unwrap());
            });
        },
    );

    let _ = spawner;

    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        info!("Hello world!\r");
        ticker.next().await;
        info!("waited...\r");
        ticker.next().await;
    }
}
