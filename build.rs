use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let whisper_dir = Path::new("vendor/whisper.cpp");
    let dst = cmake::Config::new(whisper_dir)
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("GGML_STATIC", "ON")
        .define("WHISPER_BUILD_TESTS", "OFF")
        .define("WHISPER_BUILD_EXAMPLES", "OFF")
        .define("WHISPER_BUILD_SERVER", "OFF")
        .define("WHISPER_CURL", "OFF")
        .define("WHISPER_SDL2", "OFF")
        .define("WHISPER_FFMPEG", "OFF")
        .define("GGML_OPENMP", "ON")
        .define("GGML_VULKAN", "ON")
        .define("WHISPER_OPENVINO", "OFF")
        .define("WHISPER_COREML", "OFF")
        .build();

    let lib_dir = find_lib_dir(&dst);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=whisper");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");
    println!("cargo:rustc-link-lib=static=ggml-vulkan");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=stdc++");

    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=gomp");
        println!("cargo:rustc-link-lib=vulkan");
    }

    let header_path = whisper_dir.join("include/whisper.h");
    println!("cargo:rerun-if-changed={}", header_path.display());

    let bindings = bindgen::Builder::default()
        .header(header_path.to_string_lossy().into_owned())
        .clang_arg(format!("-I{}", whisper_dir.join("include").display()))
        .clang_arg(format!("-I{}", whisper_dir.join("ggml/include").display()))
        .allowlist_function("whisper_context_default_params")
        .allowlist_function("whisper_init_from_file_with_params")
        .allowlist_function("whisper_full_default_params")
        .allowlist_function("whisper_full")
        .allowlist_function("whisper_full_n_segments")
        .allowlist_function("whisper_full_get_segment_text")
        .allowlist_function("whisper_free")
        .allowlist_type("whisper_context")
        .allowlist_type("whisper_context_params")
        .allowlist_type("whisper_full_params")
        .allowlist_type("whisper_sampling_strategy")
        .allowlist_var("whisper_sampling_strategy_.*")
        .generate()
        .expect("Unable to generate whisper bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    bindings
        .write_to_file(out_path.join("whisper_bindings.rs"))
        .expect("Failed to write whisper bindings");
}

fn find_lib_dir(prefix: &Path) -> PathBuf {
    let lib_dir = prefix.join("lib");
    if lib_dir.exists() {
        return lib_dir;
    }
    let lib64_dir = prefix.join("lib64");
    if lib64_dir.exists() {
        return lib64_dir;
    }
    prefix.to_path_buf()
}
