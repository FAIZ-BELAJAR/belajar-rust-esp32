// Oke kali ini kita akan berkenalan dengan sensor suhu dan humiditas dht11
#![no_std]
#![no_main]

use core::sync::atomic::{AtomicI32, AtomicU8, Ordering};

use esp_hal::{delay::Delay, gpio::Flex};
use embedded_dht_rs::dht11::Dht11;

// Dht11
// Dht11 adalah struct yang digunakan untuk mengontrol sensor DHT11, dimana dalah bawaan dari crate embedded_dht_rs


pub fn sensor_dht11<'a>(pin: &'a mut Flex<'a>, delay: &'a mut Delay) -> Dht11<&'a mut Flex<'a>, &'a mut Delay>{
    Dht11::new(pin, delay)
}

pub fn dapatkan_data(
    sensor: &mut Dht11<&mut Flex<'_>, &mut Delay>,
    referensi_suhu: &AtomicU8,
    referensi_humiditas: &AtomicU8,
) {
    match sensor.read() {
        Ok(reading) => {
            referensi_suhu.store(reading.temperature, Ordering::Relaxed);
            referensi_humiditas.store(reading.humidity, Ordering::Relaxed);
        }
        Err(err) => {
            log::error!("DHT11 gagal baca: {:?}", err);
            referensi_suhu.store(255, Ordering::Relaxed);
            referensi_humiditas.store(255, Ordering::Relaxed);
        }
    }
}