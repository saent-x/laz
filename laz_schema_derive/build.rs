fn main() {
    // Tell cargo to rerun this script if any Rust files change
    println!("cargo:rerun-if-changed=src/lib.rs");
    
    // This build script doesn't need to do much for now
    // The actual schema generation will happen in the proc macro
}