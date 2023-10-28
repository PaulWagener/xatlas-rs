use std::env;
use std::path::PathBuf;

fn main() {
    cc::Build::new()
        .file("vendor/source/xatlas/xatlas.cpp")
        .std("c++11")
        .cpp(true)
        .warnings(false)
        .compile("xatlas");

    bindgen::builder()
        .header("vendor/source/xatlas/xatlas.h")
        .enable_cxx_namespaces()
        .clang_args(&["-xc++", "-std=c++11"])
        .layout_tests(false)
        .generate()
        .expect("Unable to generate bindings!")
        .write_to_file(PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs"))
        .expect("Unable to write bindings!");
}
