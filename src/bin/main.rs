#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::{info, error};
use embassy_executor::Spawner;
use esp_hal::{
    clock::CpuClock,
    timer::timg::TimerGroup,
    uart::{self, RxConfig, Uart},
};

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

esp_bootloader_esp_idf::esp_app_desc!();

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

    // let tx = peripherals.GPIO16;
    // let rx = Input::new(
    //     peripherals.GPIO17,
    //     InputConfig::default().with_pull(Pull::Up),
    let tx = peripherals.GPIO43;
    // let rx = Input::new(
    //     peripherals.GPIO44,
    //     // InputConfig::default().with_pull(Pull::Up),
    //     InputConfig::default(),
    // );
    let rx = peripherals.GPIO44;

    let uart = Uart::new(
        peripherals.UART1,
        uart::Config::default()
            .with_rx(
                RxConfig::default()
                    // .with_fifo_full_threshold(32)
                    // .with_timeout(4),
            )
            .with_clock_source(uart::ClockSource::Apb)
            // .with_baudrate(38400)
            // .with_data_bits(uart::DataBits::_8)
            // .with_parity(uart::Parity::None)
            // .with_stop_bits(uart::StopBits::_1),
    )
    .unwrap()
    .with_tx(tx)
    .with_rx(rx)
    .into_async();

    let (mut rx_uart, tx_uart) = uart.split();

    loop {
        // let mut grant = trx.grant_max_remaining(64).unwrap();
        let mut buf = [0u8; 64];

        match rx_uart.read_async(&mut buf).await {
            Ok(read) => {
                info!("READ: {}", read);
            }
            Err(_err) => {
                if let Err(err) = rx_uart.check_for_errors() {
                    error!("check >>>> {}", err);
                }
            }
        }
    }
}
