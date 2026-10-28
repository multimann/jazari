//! Gives the executable a 4 MB main-thread stack on Windows, as Ruffle's own
//! desktop player does: some SWFs recurse deeply (notably around gotos), and
//! the Windows default of 1 MB is not enough. Linux's default is already
//! large enough.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows") {
        if target.contains("msvc") {
            println!("cargo:rustc-link-arg=/STACK:4000000");
        } else {
            println!("cargo:rustc-link-arg=-Xlinker");
            println!("cargo:rustc-link-arg=--stack");
            println!("cargo:rustc-link-arg=4000000");
        }
    }
}
