fn main() {
    #[cfg(not(debug_assertions))]
    {
        std::fs::write("./version", env!("CARGO_PKG_VERSION")).unwrap();
    }
}
