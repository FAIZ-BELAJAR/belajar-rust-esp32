#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal as eh;
use eh::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    
    // Initial
    // di esp-hal, diawal kode program kita, kita diwajibkan untuk melakukan inisialisasi pada konfigurasi esp kita, seperti cpu clock, peripheral, dan semacam nya
    // Kita bisa melakukan config dengan menggunakan fungsi Config::default()
    // dari fungsi tersebut, kita bisa mengatur juga clock cpu dari esp
    // dengan method .with_cpu_clock(CpuClock::opsi(bisa _80Mhz/160Mhz/240Mhz))
    // setelahnya fungsi Config::default() akan mengembalikan Struct Config dimana belum otomatis menjalankan konfigurasinya
    // Untuk menjalankan Konfigurasinya kita perlu menjalankan fungsi init(Config) dan akan mengembalikan peripherals
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::_240MHz);
    let peripherals: eh::peripherals::Peripherals = esp_hal::init(config);

    // Peripherals
    // Di esp32 Peripherals bertidak sebagai
    // Katalog Fisik Cip: Mewakili seluruh inventaris dan modul perangkat keras nyata yang ada di dalam chip ESP32 (seperti pin GPIO, UART, I2C, Timer, dan ADC).
    // Token Kepemilikan Mutlak: Bertindak sebagai "kunci akses eksklusif" yang memastikan setiap modul hardware hanya dipegang dan dikontrol oleh satu bagian program saja.
    // Pencegah Konflik (Safety): Menerapkan aturan kepemilikan (ownership) ala Rust untuk mencegah bug berbahaya seperti race condition atau konfigurasi hardware ganda.


    // Peripherals.GPIO
    // kita pasti tahu bahwa mikrokontroller memiliki pin pin yang tersebar di dalam nya
    // dan nantinya kita akan mengontrol komponen komponen robotic atau elektronika dengan menggunakan pin pin tersebut
    // nah di esp-hal, hal ini sudah disediakan di crate esphal, dengan menggunakan function Output::new(peripherals.GPION(Dimana N adalah pin yang ingin kita manipulasi), Level::Low(default), OutputConfig::default())
    // Bebas dipakai (aman buat input/output biasa)
    // GPIO4 → D4
    // GPIO5 → D5
    // GPIO13 → D13
    // GPIO14 → D14
    // GPIO16 → D16 (RX2)
    // GPIO17 → D17 (TX2)
    // GPIO18 → D18 (VSPI SCK)
    // GPIO19 → D19 (VSPI MISO)
    // GPIO21 → D21 (SDA)
    // GPIO22 → D22 (SCL)
    // GPIO23 → D23 (VSPI MOSI)
    // GPIO25 → D25 (DAC1)
    // GPIO26 → D26 (DAC2)
    // GPIO27 → D27
    // GPIO32 → D32
    // GPIO33 → D33

    // Strapping pin (bisa dipakai, tapi hati-hati pas boot)

    // GPIO0 → D0 (BOOT; harus HIGH pas boot normal, LOW = mode flash)
    // GPIO2 → D2 (LED onboard; harus LOW/floating pas mode flash)
    // GPIO12 → D12 (MTDI; HIGH pas boot bisa bikin gagal boot di flash 3.3V)
    // GPIO15 → D15 (MTDO; ngeluarin log boot, pull-up internal)

    // Input only (gak bisa output, gak ada pull-up/down internal)

    // GPIO34 → 34 (ADC1)
    // GPIO35 → 35 (ADC1)
    // GPIO36 → VP (ADC1)
    // GPIO39 → VN (ADC1)

    // Serial USB (jangan dipakai kalau masih butuh Serial Monitor/upload)

    // GPIO1 → TX (UART0 TX)
    // GPIO3 → RX (UART0 RX)

    // Jangan dipakai (nyambung ke flash SPI internal)

    // GPIO6 → CLK flash
    // GPIO7 → D0 flash
    // GPIO8 → D1 flash
    // GPIO9 → D2 flash
    // GPIO10 → D3 flash
    // GPIO11 → CMD flash

    let mut pins: [Output; 5] = [
        Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default()), 
        Output::new(peripherals.GPIO5, Level::Low, OutputConfig::default()), 
        Output::new(peripherals.GPIO13, Level::Low, OutputConfig::default()), 
        Output::new(peripherals.GPIO14, Level::Low, OutputConfig::default()), 
        Output::new(peripherals.GPIO16, Level::Low, OutputConfig::default())];

    let delay = Delay::new();


    // 3. Atur sesuka lo di sini
    loop {
        for pin in &mut pins {
            pin.set_high();
            delay.delay_millis(200);
            pin.set_low();
        }
        // Nyalain GPIO14 selama 2 detik

    }
}