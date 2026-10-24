//! Lets binaries and tests find llama.cpp's dynamic libraries on macOS: next to the
//! executable (`cargo run`), one level up (test binaries in `deps/`), and in the app
//! bundle's Frameworks folder (M8).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        [
            "@executable_path",
            "@executable_path/..",
            "@executable_path/../Frameworks",
        ]
        .iter()
        .for_each(|path| println!("cargo:rustc-link-arg=-Wl,-rpath,{path}"));
    }
}
