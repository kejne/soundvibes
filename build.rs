use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn main() {
    let whisper_dir = Path::new("vendor/whisper.cpp");
    let mut config = cmake::Config::new(whisper_dir);
    let enable_vulkan = env::var_os("CARGO_FEATURE_VULKAN").is_some();
    if enable_vulkan {
        if let Some(sdk_root) = resolve_vulkan_sdk() {
            let mut prefix = env::var_os("CMAKE_PREFIX_PATH").unwrap_or_default();
            if !prefix.is_empty() {
                prefix.push(OsString::from(";"));
            }
            prefix.push(sdk_root.as_os_str());
            let prefix_string = prefix.to_string_lossy().into_owned();
            config.env("VULKAN_SDK", &sdk_root);
            config.env("CMAKE_PREFIX_PATH", &prefix_string);
            config.define("CMAKE_PREFIX_PATH", &prefix_string);
            config.define("Vulkan_ROOT", sdk_root.to_string_lossy().as_ref());
            config.define(
                "Vulkan_INCLUDE_DIR",
                sdk_root.join("include").to_string_lossy().as_ref(),
            );
            config.define(
                "Vulkan_GLSLC_EXECUTABLE",
                sdk_root.join("bin/glslc").to_string_lossy().as_ref(),
            );
        }
    }
    let dst = config
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("GGML_STATIC", "ON")
        .define("WHISPER_BUILD_TESTS", "OFF")
        .define("WHISPER_BUILD_EXAMPLES", "OFF")
        .define("WHISPER_BUILD_SERVER", "OFF")
        .define("WHISPER_CURL", "OFF")
        .define("WHISPER_SDL2", "OFF")
        .define("WHISPER_FFMPEG", "OFF")
        .define("GGML_OPENMP", "ON")
        .define("GGML_VULKAN", if enable_vulkan { "ON" } else { "OFF" })
        .define("WHISPER_OPENVINO", "OFF")
        .define("WHISPER_COREML", "OFF")
        .build();

    let lib_dir = find_lib_dir(&dst);
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=whisper");
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-base");
    println!("cargo:rustc-link-lib=static=ggml-cpu");
    if enable_vulkan {
        println!("cargo:rustc-link-lib=static=ggml-vulkan");
    }
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=stdc++");

    if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=gomp");
        if enable_vulkan {
            println!("cargo:rustc-link-lib=vulkan");
        }
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
        .allowlist_function("whisper_log_set")
        .allowlist_type("whisper_context")
        .allowlist_type("whisper_context_params")
        .allowlist_type("whisper_full_params")
        .allowlist_type("ggml_log_callback")
        .allowlist_type("ggml_log_level")
        .allowlist_type("whisper_sampling_strategy")
        .allowlist_var("whisper_sampling_strategy_.*")
        .generate()
        .expect("Unable to generate whisper bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    bindings
        .write_to_file(out_path.join("whisper_bindings.rs"))
        .expect("Failed to write whisper bindings");
}

fn resolve_vulkan_sdk() -> Option<PathBuf> {
    let home = env::var("HOME").ok()?;
    let sdk_base = PathBuf::from(home).join(".local/share/vulkan-sdk");
    let current = sdk_base.join("current").join("x86_64");
    if current.exists() {
        return Some(current);
    }

    if let Ok(sdk) = env::var("VULKAN_SDK") {
        let path = PathBuf::from(&sdk);
        if path.exists() {
            if let Some(version_path) = path.parent().and_then(|parent| parent.parent()) {
                if version_path.starts_with(&sdk_base) {
                    let candidate = sdk_base.join("current").join("x86_64");
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
            return Some(path);
        }
    }

    None
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
