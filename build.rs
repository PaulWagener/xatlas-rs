use std::env;
use std::path::PathBuf;

fn main() {
    let mut build = cc::Build::new();
    build
        .file("vendor/source/xatlas/xatlas.cpp")
        .flag("-std=c++11")
        .cpp(true)
        .warnings(false);

    if let Ok(crt) = env::var("XATLAS_MSVC_CRT") {
        match crt.as_str() {
            "dynamic" => build.static_crt(false),
            "static" => build.static_crt(true),
            _ => panic!("Invalid value of OPENCV_MSVC_CRT var, expected \"static\" or \"dynamic\""),
        };
    }

    build.compile("xatlas");

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
