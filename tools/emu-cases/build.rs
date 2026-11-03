fn main() {
    let dir = std::env::var("UNICORN_LIB_DIR").unwrap_or_else(|_| r"C:\Python314\Lib\site-packages\unicorn\lib".into());
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=dylib=unicorn");
    println!("cargo:rerun-if-env-changed=UNICORN_LIB_DIR");
}
