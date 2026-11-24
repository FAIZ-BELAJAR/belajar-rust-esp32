#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{self as eh, gpio::Flex, system::{CpuControl, Stack}, time::Duration};
use eh::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    i2c::master::I2c, // Tambahan driver I2C esp-hal
    main,
};


use core::{fmt::Write, ptr::addr_of_mut, sync::atomic::{AtomicU8, AtomicU32, Ordering}};
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

// Import untuk Interrupt (tambahan)
// Butuh di Cargo.toml: critical-section = "1"
use core::{cell::RefCell, sync::atomic::AtomicBool};
use critical_section::Mutex;
use esp_hal::{
    gpio::{Event, Input, InputConfig, Io, Pull},
    handler,
    time::Instant,
};

mod dht11_gue;

static mut APP_CORE_STACK: Stack<8192> = Stack::new();

static HITUNG_CORE1: AtomicU32 = AtomicU32::new(0);

// Data yang dibagi antara ISR dan main (bagian Management Interrupt)
// Atomic dipakai untuk angka/flag sederhana, tanpa lock
static TOMBOL_DITEKAN: AtomicBool = AtomicBool::new(false);
static JUMLAH_TEKAN: AtomicU32 = AtomicU32::new(0);
static TERAKHIR_MS: AtomicU32 = AtomicU32::new(0); // untuk debounce

static JUMLAH_TEKAN2: AtomicU32 = AtomicU32::new(0);
static TERAKHIR_MS2: AtomicU32 = AtomicU32::new(0);

// Objek hardware yang dipakai ISR dibungkus Mutex<RefCell<Option<..>>>
// Option karena static butuh nilai awal sebelum Input-nya ada (None dulu, diisi di main)
static TOMBOL: Mutex<RefCell<Option<Input<'static>>>> = Mutex::new(RefCell::new(None));
static TOMBOL2: Mutex<RefCell<Option<Input<'static>>>> = Mutex::new(RefCell::new(None));

// ISR (Interrupt Service Routine): jalan otomatis tiap ada sinyal turun di GPIO27
// Aturan: singkat, tanpa flush OLED, tanpa alokasi heap, tanpa delay, dan WAJIB clear interrupt
#[handler]
fn gpio_isr() {
    critical_section::with(|cs| {
        let mut slot = TOMBOL.borrow_ref_mut(cs);
        if let Some(tombol) = slot.as_mut() {
            if tombol.is_interrupt_set() {
                // debounce sederhana: abaikan kalau < 50 ms dari tekanan sebelumnya
                let sekarang = Instant::now().duration_since_epoch().as_millis() as u32;
                if sekarang.wrapping_sub(TERAKHIR_MS.load(Ordering::Relaxed)) > 50 {
                    TERAKHIR_MS.store(sekarang, Ordering::Relaxed);
                    JUMLAH_TEKAN.fetch_add(1, Ordering::Relaxed);
                    TOMBOL_DITEKAN.store(true, Ordering::Release);
                }
                tombol.clear_interrupt(); // WAJIB, kalau lupa ISR dipanggil terus dan chip terlihat hang
            }
        }

         let mut slot2 = TOMBOL2.borrow_ref_mut(cs);
        if let Some(tombol2) = slot2.as_mut() {
            if tombol2.is_interrupt_set() {
                let sekarang = Instant::now().duration_since_epoch().as_millis() as u32;
                if sekarang.wrapping_sub(TERAKHIR_MS2.load(Ordering::Relaxed)) > 50 {
                    TERAKHIR_MS2.store(sekarang, Ordering::Relaxed);
                    JUMLAH_TEKAN2.fetch_add(1, Ordering::Relaxed);
                }
                tombol2.clear_interrupt(); // WAJIB juga untuk tombol kedua
            }
        }
    });
}

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

    // Jenis pin di esp hal terbagi menjadi 3, yakni
    // Input : pin khusus baca (menerima tegangan)
    // Output: pin khusus tulis, push-pull (mengubah ubah tegangan)
    // Flex: gabungan dari keduanya baca&tulis (membaca dan mengubah ubah tegangan)
    // Pada dasarnya pin itu berstatus hanya Low(tak ada tegangan) High (ada tegangan)

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

    // Guard (jangan sampai ke-drop)
    // start_app_core mengembalikan sebuah guard
    // Kalau guard ini di-drop (misalnya ditulis let _ = ...), core 1 langsung dimatikan
    // Makanya harus ditampung ke variabel bernama, contoh: let _guard = ...

    // Ini akan berjalan di core 1
    let _guard = cpu_control
    .start_app_core(unsafe {&mut *addr_of_mut!(APP_CORE_STACK)}, move || {
        let delay = Delay::new();
        let mut pin_sensor = Flex::new(peripherals.GPIO25);

        delay.delay(Duration::from_millis(5000));

        loop {
            delay.delay(Duration::from_millis(2000));
            if HITUNG_CORE1.load(Ordering::Relaxed) == 4 {
                HITUNG_CORE1.store(0, Ordering::Relaxed);
            } else {
                HITUNG_CORE1.fetch_add(1, Ordering::Relaxed);
            }
            
        }
    } ).unwrap();


    // Management Interrupt
    // Selama ini program kita memakai polling: loop terus berputar dan mengecek "ada kejadian belum?"
    // Interrupt membalik cara itu: hardware yang "menyela" CPU saat kejadian terjadi (tombol ditekan, timer habis, data UART masuk)
    // CPU berhenti sebentar dari kerjaan sekarang, lompat ke fungsi khusus (ISR / handler), lalu kembali persis ke tempat semula
    // Keuntungannya respons cepat tanpa harus mengecek terus, dan CPU bebas mengerjakan hal lain di antaranya
    // Kita tidak perlu menulis kode "kembali ke tempat semula", hardware dan esp-hal yang mengurus

    // Tiga pemain yang harus dikoordinasikan sendiri di bare-metal
    // 1. main (core 0): jalan terus-menerus (super-loop)
    // 2. ISR: jalan tiba-tiba, bisa menyela main di baris kode mana pun
    // 3. Closure core 1: jalan paralel di core lain
    // Masalah utamanya ada di nomor 2, ISR bisa memotong main di tengah operasi, maka data yang dibagi tidak boleh diakses sembarangan

    // Alur interrupt di esp-hal
    // 1. Tulis fungsi handler dengan atribut #[handler] (lihat gpio_isr di atas main)
    // 2. Daftarkan ke sistem: Io::new(peripherals.IO_MUX) lalu io.set_interrupt_handler(gpio_isr)
    // 3. Aktifkan sumbernya: tombol.listen(Event::FallingEdge)
    // 4. Di dalam handler, WAJIB bersihkan penanda interrupt: tombol.clear_interrupt()
    // Tanpa langkah 4 interrupt dianggap masih aktif, handler dipanggil berulang dan chip terlihat hang

    // Jenis event GPIO (kapan interrupt dipicu)
    // Event::RisingEdge  : sinyal naik LOW -> HIGH
    // Event::FallingEdge : sinyal turun HIGH -> LOW (cocok untuk tom`bol dengan pull-up, ditekan = LOW)
    // Event::AnyEdge     : naik maupun turun, bisa dibedakan dengan membaca tombol.is_low() di handler
    // Event::LowLevel    : selama sinyal LOW (hati-hati, terus memicu selama level bertahan)
    // Event::HighLevel   : selama sinyal HIGH (sama, hati-hati)
    // Edge (tepi) lebih aman dari level karena hanya memicu sekali per perubahan

    // Aturan emas ISR
    // 1. SINGKAT. Set flag atau tambah counter lalu keluar
    // 2. Jangan flush OLED, jangan info!/println, jangan delay, jangan alokasi heap (Vec::push, Box::new, String)
    // 3. Wajib clear interrupt
    // 4. Kerjaan berat dilempar ke main lewat flag: ISR cuma bilang "ada kejadian", main yang mengerjakan
    // Alasannya, selama ISR jalan, kerjaan main berhenti dan interrupt lain bisa tertahan

    // Berbagi data ISR dan main
    // 1. AtomicBool / AtomicU32: untuk flag dan counter sederhana (TOMBOL_DITEKAN, JUMLAH_TEKAN)
    //    swap(false) membaca sekaligus mereset flag dalam satu operasi, jadi tidak ada celah ISR menyelip lalu flag hilang
    // 2. critical_section::with: mematikan interrupt sementara (dan mengunci core lain) selama blok berjalan
    //    Dipakai untuk objek yang diakses ISR dan main, seperti Input pada static TOMBOL
    // 3. Mutex<RefCell<Option<T>>>: pola standar objek hardware yang dipakai ISR
    //    Mutex untuk akses aman, RefCell untuk mengubah isi, Option karena awalnya kosong (None)
    // 4. Queue (heapless::spsc::Queue): kalau yang dikirim ISR adalah data, bukan sekadar sinyal
    // Pasangan Ordering Release (penulis) dan Acquire (pembaca) menjamin data yang ditulis sebelum flag terlihat setelah flag dibaca

    // Interrupt dan core
    // ISR jalan di core yang mendaftarkannya (core yang memanggil set_interrupt_handler dan listen)
    // Pada kode ini pendaftaran dilakukan di main, jadi ISR jalan di core 0
    // Kalau ingin ISR di core 1, lakukan pendaftaran dari dalam closure core 1
    // Satu sumber interrupt dipakai satu handler, jadi jangan daftarkan hal yang sama dari dua core

    // Prioritas interrupt
    // Tiap interrupt punya level prioritas, interrupt berprioritas tinggi bisa menyela ISR berprioritas rendah
    // Di esp-hal prioritas bisa diatur lewat atribut handler (contoh: #[handler(priority = Priority::Priority1)])
    // dan tipe Priority dari esp_hal::interrupt, nama persisnya bisa bergeser antar versi, cek docs versi esp-hal yang dipakai
    // Pegangan: kerjaan yang timing-nya kritis berprioritas lebih tinggi, tapi tetap harus super singkat

    // Debounce
    // Tombol mekanis "bergetar" beberapa milidetik saat ditekan, satu tekanan bisa memicu banyak interrupt
    // Solusi sederhana (dipakai di gpio_isr): catat waktu tekanan terakhir, abaikan kalau selisihnya < 50 ms
    // Wrapping: gunakan wrapping_sub agar tetap benar saat counter waktu u32 overflow

    // Bug umum
    // 1. Lupa clear_interrupt: handler dipanggil terus, program seperti hang
    // 2. ISR terlalu berat (flush OLED, log): main tertahan, interrupt lain terlewat
    // 3. Variabel biasa (non-atomic) dibagi ISR dan main: data race, nilai bisa rusak atau tidak terlihat
    // 4. Lupa debounce: satu tekanan terhitung banyak kali
    // 5. Memakai pin tombol yang sama dengan fungsi lain (GPIO27 di sini bebas, sedangkan strapping pin berisiko)
    // 6. Menyimpan objek yang dipakai ISR di static mut: gunakan Mutex<RefCell<Option<T>>>

    // Wiring contoh: tombol antara GPIO27 dan GND, pull-up internal aktif (Pull::Up), jadi ditekan = LOW
    // Daftarkan handler lebih dulu, baru aktifkan listen, supaya tidak ada interrupt yang tiba saat handler belum siap
        // Io::new(peripherals.IO_MUX)
    // Mengambil token IO_MUX (blok hardware pengelola pin) dan membungkusnya jadi struct Io
    // Io adalah tempat mendaftarkan handler interrupt GPIO, karena semua pin GPIO berbagi satu sumber interrupt
    // mut dibutuhkan karena set_interrupt_handler mengubah isi Io

    //     pub enum Event {
    //     RisingEdge,
    //     FallingEdge,
    //     AnyEdge,
    //     LowLevel,
    //     HighLevel,
    // }
    // Varian	Artinya	Kapan memicu
    // RisingEdge	Tepi naik	Sekali, saat sinyal berubah LOW → HIGH
    // FallingEdge	Tepi turun	Sekali, saat sinyal berubah HIGH → LOW
    // AnyEdge	Tepi mana saja	Sekali tiap perubahan, naik maupun turun
    // LowLevel	Level rendah	Terus-menerus selama sinyal berada di LOW
    // HighLevel	Level tinggi	Terus-menerus selama sinyal berada di HIGH
    // Edge vs level
    // Sinyal:  HIGH ─────┐         ┌───── HIGH
    //                    │         │
    //          LOW       └─────────┘
    //                    ↑         ↑
    //               FallingEdge  RisingEdge
    //                    └─ LowLevel aktif di sepanjang bagian bawah ini ─┘
    
    let mut io = Io::new(peripherals.IO_MUX);

    // io.set_interrupt_handler(gpio_isr)
    // Mendaftarkan gpio_isr sebagai fungsi yang dipanggil otomatis tiap ada interrupt GPIO
    // Ini baru pendaftaran, interrupt belum aktif sampai listen() dipanggil di bawah
    // Memanggil ini dua kali akan MENGGANTI handler lama, jadi cukup sekali untuk semua pin GPIO
    io.set_interrupt_handler(gpio_isr);

    // Input::new(peripherals.GPIO27, ...)
    // Mengubah GPIO27 menjadi pin input (pin khusus baca tegangan)
    // peripherals.GPIO27 dipindahkan (moved) ke variabel tombol, jadi pin ini tidak bisa dipakai lagi di tempat lain
    // mut dibutuhkan karena listen() dan clear_interrupt() mengubah state pin
    let mut tombol = Input::new(
        peripherals.GPIO27,
        // InputConfig::default().with_pull(Pull::Up)
        // Mengaktifkan resistor pull-up internal: saat tombol dilepas pin ditarik ke HIGH (3.3V)
        // Saat tombol ditekan, pin tersambung ke GND jadi LOW
        // Tanpa ini pin mengambang (floating) dan membaca nilai acak, jadi tombol tidak butuh resistor eksternal
        InputConfig::default().with_pull(Pull::Up),
    );

    let mut tombol2 = Input::new(peripherals.GPIO26, InputConfig::default().with_pull(Pull::Up));
    // critical_section::with(|cs| { ... })
    // Menjalankan blok di dalamnya dengan interrupt dimatikan sementara (dan core lain dikunci)
    // cs adalah bukti bahwa kita sedang di dalam critical section, dan dibutuhkan untuk mengakses Mutex (TOMBOL)
    // Dipakai supaya listen dan replace terjadi sebagai satu kesatuan, ISR tidak bisa menyela di antaranya
    critical_section::with(|cs| {
        // tombol.listen(Event::FallingEdge)
        // Mengaktifkan interrupt untuk pin ini, mulai saat ini sinyal turun (HIGH -> LOW) memicu gpio_isr
        // FallingEdge = tepat saat tombol ditekan (karena pull-up), hanya sekali per tekanan
        // Sengaja dipanggil di dalam critical section agar ISR baru bisa jalan setelah tombol sudah ada di static
        tombol.listen(Event::FallingEdge);

        // TOMBOL.borrow_ref_mut(cs).replace(tombol)
        // borrow_ref_mut(cs): meminjam isi static TOMBOL (RefCell) secara mutable, butuh bukti cs
        // replace(tombol): mengisi Option yang tadinya None dengan Some(tombol)
        // Ownership Input dipindah ke static agar bisa diakses ISR (untuk clear_interrupt)
        // Setelah baris ini variabel tombol di main sudah tidak bisa dipakai lagi
        TOMBOL.borrow_ref_mut(cs).replace(tombol);
    });

    let mut tegangan_32 = Output::new(peripherals.GPIO32, Level::Low, OutputConfig::default());

    tegangan_32.set_high();

    critical_section::with(|cs|{
        tombol2.listen(Event::FallingEdge);
        TOMBOL2.borrow_ref_mut(cs).replace(tombol2)
    });


    // Ini akan berjalan di core 0 (default)
   // Ini akan berjalan di core 0 (default)
    loop {
        let now_count = HITUNG_CORE1.load(Ordering::Relaxed) as usize;
        if !pins[now_count].is_set_high() {
            pins[now_count].set_high();
        }

        // Buat string untuk ditampilkan ke layar OLED
        let mut buff_pin: heapless::String<17> = heapless::String::new();
        let _ = write!(buff_pin, "Pin: {}", now_count + 1);

        // Hitung tekanan tombol dari ISR (tanda * muncul di frame setelah tombol baru ditekan)
        let baru_ditekan = TOMBOL_DITEKAN.swap(false, Ordering::Acquire);
        let mut buff_tekan: heapless::String<24> = heapless::String::new();
        let _ = write!(
            buff_tekan,
            "Tekan: {}{}",
            JUMLAH_TEKAN.load(Ordering::Relaxed),
            if baru_ditekan { " *" } else { "" }
        );

        let mut buff_tekan2: heapless::String<24> = heapless::String::new();
        let _ = write!(buff_tekan2, 
            "Tekan2: {} {}", JUMLAH_TEKAN2.load(Ordering::Relaxed), 
            if baru_ditekan { " *" } else { "" });
    

        layar.clear_buffer();

        // Render teks ke layar dengan koordinat Y berbeda agar tidak bertumpuk
        Text::new(&buff_pin, Point::new(0, 12), text_style).draw(&mut layar).unwrap();
        Text::new(&buff_tekan, Point::new(0, 24), text_style).draw(&mut layar).unwrap();
        Text::new(&buff_tekan2, Point::new(0, 36), text_style).draw(&mut layar).unwrap();

        layar.flush().unwrap();

        if now_count != 0 {
            if pins[now_count - 1].is_set_high() {
                pins[now_count - 1].set_low();
            }
        } else {
            if !pins[4].is_set_low() {
                pins[4].set_low();
            }
        }
    }
}