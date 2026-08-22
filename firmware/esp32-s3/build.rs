fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=components/waveshare_esp32_s3_touch_lcd_4_3");
    embuild::espidf::sysenv::output();
}
