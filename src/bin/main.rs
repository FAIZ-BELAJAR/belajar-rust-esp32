#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::time::{Duration, Instant, Rate};
use esp_hal::timer::timg::TimerGroup;
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use log::info;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};

use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

use embedded_dht_rs::dht11::Dht11;
use esp_hal::gpio::{DriveMode, Flex, Input, InputConfig, Level, OutputConfig, Pull};
use esp_hal::delay::Delay;
use core::fmt::Write;

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    let (mut _wifi_controller, _interfaces) =
        esp_radio::wifi::new(peripherals.WIFI, Default::default())
            .expect("Failed to initialize Wi-Fi controller");

    // ---- Setup Layar LCD ----
    let monitor_display = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_khz(400)),
    )
    .unwrap()
    .with_scl(peripherals.GPIO22)
    .with_sda(peripherals.GPIO21);

    let tampilan = I2CDisplayInterface::new(monitor_display);
    let mut tampilkan = Ssd1306::new(tampilan, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    tampilkan.init().unwrap();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(BinaryColor::On)
        .build();

    // ---- Setup Sensor Suhu (DHT11) ----
    let dht_config = OutputConfig::default()
        .with_drive_mode(DriveMode::OpenDrain)
        .with_pull(Pull::None);

    let mut dht_pin = Flex::new(peripherals.GPIO4);
    dht_pin.apply_output_config(&dht_config);
    dht_pin.set_output_enable(true);
    dht_pin.set_input_enable(true);
    dht_pin.set_level(Level::High);

    let mut sensor_suhu = Dht11::new(dht_pin, Delay::new());
    let mut suhu: u8 = 0;

    // ---- Setup Sensor Cahaya (LM393, DO=D13) ----
    let sensor_cahaya = Input::new(peripherals.GPIO13, InputConfig::default().with_pull(Pull::Up));

    Delay::delay_millis(&Delay::new(), 2000);

    loop {
        match sensor_suhu.read() {
            Ok(reading) => {
                suhu = reading.temperature;
            }
            Err(e) => {
                info!("Gagal baca sensor: {:?}", e);
            }
        }

        // LM393: biasanya HIGH = gelap, LOW = terang (tergantung kalibrasi potentiometer di modul)
        let cahaya_menyala = sensor_cahaya.is_low();

        tampilkan.clear_buffer();

        let mut buf_suhu: heapless::String<32> = heapless::String::new();
        write!(buf_suhu, "Suhu saat ini: {}C", suhu).unwrap();

        Text::with_baseline(&buf_suhu, Point::zero(), text_style, Baseline::Top)
            .draw(&mut tampilkan)
            .unwrap();

        let status_lampu = if cahaya_menyala { "Cahaya: Menyala" } else { "Cahaya: Mati" };
        Text::with_baseline(status_lampu, Point::new(0, 16), text_style, Baseline::Top)
            .draw(&mut tampilkan)
            .unwrap();

        info!("Suhu: {}C | {}", suhu, status_lampu);

        tampilkan.flush().unwrap();

        let delay_start = Instant::now();
        while delay_start.elapsed() < Duration::from_millis(1000) {}
    }
}