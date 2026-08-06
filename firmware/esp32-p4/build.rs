fn main() {
    // Cargo package metadata declares the board BSP and Wi-Fi remote
    // components. Re-run esp-idf-sys when those component pins change.
    println!("cargo:rerun-if-changed=Cargo.toml");
    embuild::espidf::sysenv::output();
}
