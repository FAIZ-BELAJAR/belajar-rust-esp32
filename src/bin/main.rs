#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{self as eh, system::{CpuControl, Stack}, time::Duration};
use eh::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    i2c::master::I2c, // Tambahan driver I2C esp-hal
    main,
};


use core::{fmt::Write, ptr::addr_of_mut, sync::atomic::{AtomicU32, Ordering}};
esp_bootloader_esp_idf::esp_app_desc!();



extern crate alloc;
use alloc::boxed::Box;

// Import untuk OLED (embedded-graphics & ssd1306)
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use ssd1306::{I2CDisplayInterface, Ssd1306, mode::DisplayConfig, size::DisplaySize128x64};

static mut APP_CORE_STACK: Stack<8192> = Stack::new();

static HITUNG_CORE1: AtomicU32 = AtomicU32::new(0);
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

    // Peripherals
    // Di esp32 Peripherals bertidak sebagai
    // Katalog Fisik Cip: Mewakili seluruh inventaris dan modul perangkat keras nyata yang ada di dalam chip ESP32 (seperti pin GPIO, UART, I2C, Timer, dan ADC).
    // Token Kepemilikan Mutlak: Bertindak sebagai "kunci akses eksklusif" yang memastikan setiap modul hardware hanya dipegang dan dikontrol oleh satu bagian program saja.
    // Pencegah Konflik (Safety): Menerapkan aturan kepemilikan (ownership) ala Rust untuk mencegah bug berbahaya seperti race condition atau konfigurasi hardware ganda.
    let peripherals: eh::peripherals::Peripherals = esp_hal::init(config);

    
    // Setup Layar OLED
    // tidak menjadi topik inti kita, karna kita akan fokus memahami dulu ekosistem mikrokontroller esp32 dahulu

    let i2c_prot = I2c::new(
        peripherals.I2C0,
        eh::i2c::master::Config::default(),
    ).unwrap().with_sda(peripherals.GPIO22).with_scl(peripherals.GPIO23);

    let ui = I2CDisplayInterface::new(i2c_prot);
    let mut layar = Ssd1306::new(ui, DisplaySize128x64, ssd1306::rotation::DisplayRotation::Rotate0).into_buffered_graphics_mode();

    layar.init().unwrap();

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


    // Management Memory
    // Berbeda dengan development program di komputer dimana memory dan rom tersedia sangat banyak
    // di embedded system, memory dan rom hanya tersedia sangat sedikit
    // Maka dari itu, developer harus pintar pintar melakukan management memory
    // ada beberapa Masalah memory yang sering muncul di development embedded sys
    // 1. OOM(Out Of Memory): disebabkan oleh memory leak, yang tak pernah dibersihkan selama program berjalan
    // 2. Dangling Pointer: mereferensikan data pada alamat memory yang sudah tidak ada
    // 3. Stack Overflow: Terlalu banyak fungsi bersarang (rekursi) atau alokasi variabel lokal 
    //    yang kelewat besar, bikin memori stack nabrak batasan RAM yang sempit.
    // 4. Memory Fragmentation: Alokasi dan de-alokasi memori dinamis (heap) yang terus-menerus 
    //    bikin sisa RAM terpecah-pecah jadi blok kecil, akibatnya alokasi besar berikutnya gagal 
    //    padahal total RAM masih cukup (biasanya dihindari dengan pakai static allocation).
    // 5. Buffer Overflow: Menulis data melebihi kapasitas ukuran buffer yang disediakan, 
    //    bisa menimpa data di alamat memori sebelahnya dan bikin program nge-hang atau 
    //    bahkan celah keamanan.
    
    // Kita akan mengaloksikan heap sebesar ±98 KB
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 98768);

    // Maka dari itu disarankan untuk 
    // 1. Hindari membuat program reporting / agregasi yang berlebihan sampai sampai memakan memory besar
    // 2. Membuat dan menggunakana deref manual (Gunakan best practice Box)
    // 3. membuat fungsi rekursi
    let delay = Delay::new();

    let text_style = MonoTextStyleBuilder::new().font(&FONT_6X10).text_color(BinaryColor::On).build();

    // Management Core CPU
    // Berbeda dengan di komputer dengan OS dimana cpu, thread dll, sudah diatur oleh OS, lagi lagi di embedded kita betul betul di bebaskan untuk melakukan management nya manual sendiri
    // Kita bisa menggunakan struct CpuControl untuk melakukan kontrol pada CPU
    // Dengan cara menggunakan function dari esp_hal::CpuControl::new(peripherals.CPU_CTRL)
    
    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    // Konsep dasar
    // ESP32 punya 2 core: Core 0 (PRO_CPU) dan Core 1 (APP_CPU)
    // Core 0 otomatis nyala dan menjalankan fungsi main kita
    // Core 1 mati sampai kita nyalain manual dengan CpuControl::start_app_core(stack, closure)
    // Kalau WiFi/BLE tidak dipakai, kedua core bebas kita pakai sepenuhnya
    // Cpu::current() bisa dipakai untuk mengecek kode sedang jalan di core mana

    // Stack milik core 1
    // Core 1 tidak bisa berbagi stack dengan core 0, jadi kita wajib menyediakan stack sendiri
    // Caranya dengan membuat static Stack<N>, contoh: static mut APP_CORE_STACK: Stack<8192> = Stack::new();
    // Stack ini masuk ke .bss (RAM statis), jadi ikut mengurangi sisa RAM yang tersedia
    // Mulai dari 8 KB, naikkan ke 16 KB kalau closure memakai log, format string, atau array besar
    // Stack overflow di core 1 sama diam-diamnya dengan core 0, jadi crash tanpa pesan jelas

    // Closure dan ownership
    // Program untuk core 1 dikirim lewat closure yang diberi keyword move
    // Pin atau peripheral yang dipakai di dalam closure dipindahkan (moved) ke core 1
    // Artinya pin itu tidak bisa lagi dipakai di core 0, ownership Rust mencegah dua core rebutan hardware yang sama
    // Closure harus berisi loop {} karena core 1 tidak punya OS yang menutup programnya

    // Guard (jangan sampai ke-drop)
    // start_app_core mengembalikan sebuah guard
    // Kalau guard ini di-drop (misalnya ditulis let _ = ...), core 1 langsung dimatikan
    // Makanya harus ditampung ke variabel bernama, contoh: let _guard = ...

    let _guard = cpu_control
    .start_app_core(unsafe {&mut *addr_of_mut!(APP_CORE_STACK)}, move || {
        let delay = Delay::new();
        loop {
            delay.delay(Duration::from_millis(800));
            if HITUNG_CORE1.load(Ordering::Relaxed) == 4 {
                HITUNG_CORE1.store(0, Ordering::Relaxed);
            } else {
                HITUNG_CORE1.fetch_add(1, Ordering::Relaxed);
            }
            
        }
    } ).unwrap();


    // Berbagi data antar core
    // Memori (RAM dan heap) dilihat oleh kedua core, tapi akses bersamaan harus aman
    // 1. AtomicXxx (AtomicU32, AtomicBool, dll): untuk angka atau flag sederhana, tanpa lock
    //    Ordering::Relaxed cukup kalau hanya butuh nilainya, bukan urutan antar variabel
    // 2. critical_section::Mutex<RefCell<T>>: untuk struct atau data yang lebih kompleks
    // 3. Queue/channel (misal heapless::spsc::Queue): untuk kirim data satu arah antar core
    // Jangan pakai static mut untuk data yang dibagi, itu sumber data race

    // Catatan penting
    // 1. delay_millis itu blocking, core yang memanggilnya berhenti total selama waktu itu
    //    Dengan 2 core, tiap core bisa punya timing sendiri tanpa saling menunggu
    // 2. Heap (esp_alloc) aman dipakai dua core, tapi alokasi berulang di dua core bikin saling antre
    // 3. Peripheral dipakai satu core saja, kalau butuh dipakai bersama, bungkus dengan Mutex
    // 4. Di debug build, panic di core 1 tidak otomatis menghentikan core 0
    // 5. Kalau butuh banyak task dengan penjadwalan otomatis (preemptive, mirip FreeRTOS),
    //    pakai esp-rtos atau Embassy, itu level di atas CpuControl yang bare-metal

  
    // Ini akan berjalan di core 0 (default)
    loop {
            let now_count = HITUNG_CORE1.load(Ordering::Relaxed) as usize;
            if !pins[now_count].is_set_high() {
                pins[now_count].set_high();
            }
            let mut buff: heapless::String<17> = heapless::String::new();
            let _ = write!(buff, "Pin menyala: {}", now_count+1);

            layar.clear_buffer();

            Text::new(&buff, Point::new(0, 12), text_style).draw(&mut layar).unwrap();

            layar.flush().unwrap();


            if now_count != 0 {
                if pins[now_count-1].is_set_high(){
                pins[now_count-1].set_low();
                }
            } else {
                 if !pins[4].is_set_low() {
                    pins[4].set_low()
                 }
            }
            
    }
}