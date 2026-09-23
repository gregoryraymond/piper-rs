use cmake::Config;
use glob::glob;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if std::env::var("BUILD_DEBUG").is_ok() {
            println!("cargo:warning=[DEBUG] {}", format!($($arg)*));
        }
    };
}

/// `cfg!(target_os = ...)` inside a build script reports the machine doing the
/// BUILDING, not the machine being built for, so it silently takes the wrong
/// branch whenever the two differ. Cargo exposes the real target here.
fn target_os() -> String {
    env::var("CARGO_CFG_TARGET_OS").unwrap_or_default()
}

/// The NDK sysroot, when building for Android. bindgen and CMake both need it;
/// without it bindgen reads the host's /usr/include and generates bindings with
/// host-sized types.
fn android_ndk_root() -> Option<std::path::PathBuf> {
    for k in ["ANDROID_NDK_ROOT", "ANDROID_NDK_HOME", "NDK_HOME"] {
        if let Ok(v) = env::var(k) {
            let p = std::path::PathBuf::from(v);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    None
}

fn get_cargo_target_dir() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    let profile = std::env::var("PROFILE")?;
    let mut target_dir = None;
    let mut sub_path = out_dir.as_path();
    while let Some(parent) = sub_path.parent() {
        if parent.ends_with(&profile) {
            target_dir = Some(parent);
            break;
        }
        sub_path = parent;
    }
    let target_dir = target_dir.ok_or("not found")?;
    Ok(target_dir.to_path_buf())
}

fn copy_folder(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).expect("Failed to create dst directory");
    if cfg!(unix) {
        std::process::Command::new("cp")
            .arg("-rf")
            .arg(src)
            .arg(dst.parent().unwrap())
            .status()
            .expect("Failed to execute cp command");
    }

    if cfg!(windows) {
        std::process::Command::new("robocopy.exe")
            .arg("/e")
            .arg(src)
            .arg(dst)
            .status()
            .expect("Failed to execute robocopy command");
    }
}

fn extract_lib_names(out_dir: &Path, build_shared_libs: bool) -> Vec<String> {
    let lib_pattern = if target_os() == "windows" {
        "*.lib"
    } else if target_os() == "macos" {
        if build_shared_libs {
            "*.dylib"
        } else {
            "*.a"
        }
    } else {
        if build_shared_libs {
            "*.so"
        } else {
            "*.a"
        }
    };
    let libs_dir = out_dir.join("lib");
    let pattern = libs_dir.join(lib_pattern);
    debug_log!("Extract libs {}", pattern.display());

    let mut lib_names: Vec<String> = Vec::new();

    // Process the libraries based on the pattern
    for entry in glob(pattern.to_str().unwrap()).unwrap() {
        match entry {
            Ok(path) => {
                let stem = path.file_stem().unwrap();
                let stem_str = stem.to_str().unwrap();

                // Remove the "lib" prefix if present
                let lib_name = if stem_str.starts_with("lib") {
                    stem_str.strip_prefix("lib").unwrap_or(stem_str)
                } else {
                    stem_str
                };
                lib_names.push(lib_name.to_string());
            }
            Err(e) => println!("cargo:warning=error={}", e),
        }
    }
    lib_names
}

fn extract_lib_assets(out_dir: &Path) -> Vec<PathBuf> {
    let shared_lib_pattern = if target_os() == "windows" {
        "*.dll"
    } else if target_os() == "macos" {
        "*.dylib"
    } else {
        "*.so"
    };

    let libs_dir = out_dir.join("lib");
    let pattern = libs_dir.join(shared_lib_pattern);
    debug_log!("Extract lib assets {}", pattern.display());
    let mut files = Vec::new();

    for entry in glob(pattern.to_str().unwrap()).unwrap() {
        match entry {
            Ok(path) => {
                files.push(path);
            }
            Err(e) => eprintln!("cargo:warning=error={}", e),
        }
    }

    files
}

fn macos_link_search_path() -> Option<String> {
    let output = Command::new("clang")
        .arg("--print-search-dirs")
        .output()
        .ok()?;
    if !output.status.success() {
        println!(
            "failed to run 'clang --print-search-dirs', continuing without a link search path"
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("libraries: =") {
            let path = line.split('=').nth(1)?;
            return Some(format!("{}/lib/darwin", path));
        }
    }

    println!("failed to determine link search path, continuing without it");
    None
}

fn main() {
    println!("cargo:rustc-link-lib=speechPlayer");
    println!("cargo:rustc-link-lib=espeak-ng");
    println!("cargo:rustc-link-lib=ucd");
    let target = env::var("TARGET").unwrap();
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let target_dir = get_cargo_target_dir().unwrap();
    let espeak_dst = out_dir.join("espeak-ng");
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("Failed to get CARGO_MANIFEST_DIR");
    let espeak_src = Path::new(&manifest_dir).join("espeak-ng");
    let build_shared_libs = false;

    let build_shared_libs = std::env::var("ESPEAK_BUILD_SHARED_LIBS")
        .map(|v| v == "1")
        .unwrap_or(build_shared_libs);
    let profile = env::var("ESPEAK_LIB_PROFILE").unwrap_or("Release".to_string());
    let static_crt = env::var("ESPEAK_STATIC_CRT")
        .map(|v| v == "1")
        .unwrap_or(false);

    debug_log!("TARGET: {}", target);
    debug_log!("CARGO_MANIFEST_DIR: {}", manifest_dir);
    debug_log!("TARGET_DIR: {}", target_dir.display());
    debug_log!("OUT_DIR: {}", out_dir.display());
    debug_log!("BUILD_SHARED: {}", build_shared_libs);

    // Prepare espeak-ng source
    // Check for a FILE the copy must have produced, not merely the directory:
    // copy_folder create_dir_all's the destination before copying, so a run that
    // fails afterwards leaves an empty directory that makes every later run skip
    // the copy and then fail in CMake with "does not contain CMakeLists.txt".
    if !espeak_dst.join("CMakeLists.txt").exists() {
        debug_log!("Copy {} to {}", espeak_src.display(), espeak_dst.display());
        copy_folder(&espeak_src, &espeak_dst);
    }
    // Speed up build
    env::set_var(
        "CMAKE_BUILD_PARALLEL_LEVEL",
        std::thread::available_parallelism()
            .unwrap()
            .get()
            .to_string(),
    );

    // Bindings
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", espeak_dst.display()))
        .clang_arg(format!(
            "-I{}",
            espeak_dst.join("src").join("include").display()
        ))
        // wrapper.h's quoted includes resolve relative to wrapper.h itself, i.e.
        // into the SOURCE tree, not the OUT_DIR copy. The headers it pulls in
        // then use angle-bracket includes (<espeak-ng/speak_lib.h>), which only
        // search -I paths - so the source tree needs to be on that list too, or
        // the second hop fails with "file not found" even though the first
        // resolved fine.
        .clang_arg(format!("-I{}", espeak_src.display()))
        .clang_arg(format!(
            "-I{}",
            espeak_src.join("src").join("include").display()
        ))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    // Cross-compiling: bindgen shells out to libclang, which defaults to the
    // HOST triple and host headers. Left alone it derives host-sized types and
    // fails with "Target platform requires `--no-size_t-is-usize`. The size of
    // `ssize_t` (4) does not match the target pointer size (8)".
    if target != env::var("HOST").unwrap_or_default() {
        builder = builder
            .clang_arg(format!("--target={target}"))
            .size_t_is_usize(false);

        if target_os() == "android" {
            let ndk = android_ndk_root().expect(
                "building for Android requires ANDROID_NDK_ROOT (or ANDROID_NDK_HOME) \
                 pointing at an NDK installation",
            );
            let sysroot = ndk
                .join("toolchains")
                .join("llvm")
                .join("prebuilt")
                .join(format!("{}-x86_64", env::consts::OS))
                .join("sysroot");
            builder = builder.clang_arg(format!("--sysroot={}", sysroot.display()));
        }
    }

    let bindings = builder.generate().expect("Failed to generate bindings");

    // Write the generated bindings to an output file
    let bindings_path = out_dir.join("bindings.rs");
    bindings
        .write_to_file(bindings_path)
        .expect("Failed to write bindings");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=./espeak-ng");

    debug_log!("Bindings Created");

    // Build with Cmake

    let mut config = Config::new(&espeak_dst);

    config.define(
        "BUILD_SHARED_LIBS",
        if build_shared_libs { "ON" } else { "OFF" },
    );

    if target_os() == "windows" {
        config.static_crt(static_crt);
    }

    if target_os() == "macos" {
        config.define("USE_LIBPCAUDIO", "OFF");
    }

    if target_os() == "android" {
        let ndk =
            android_ndk_root().expect("building for Android requires ANDROID_NDK_ROOT to be set");
        let abi = match target.split('-').next().unwrap_or("") {
            "aarch64" => "arm64-v8a",
            "armv7" | "thumbv7neon" => "armeabi-v7a",
            "x86_64" => "x86_64",
            "i686" => "x86",
            other => panic!("unsupported Android arch: {other}"),
        };
        // The NDK ships its own CMake toolchain file; without it CMake probes
        // the host compiler and produces host objects that fail to link.
        config
            .define(
                "CMAKE_TOOLCHAIN_FILE",
                ndk.join("build")
                    .join("cmake")
                    .join("android.toolchain.cmake"),
            )
            .define("ANDROID_ABI", abi)
            .define("ANDROID_PLATFORM", "android-24")
            // No pcaudio on Android - the app plays through oboe, and espeak-ng
            // only needs to synthesise into a buffer.
            .define("USE_LIBPCAUDIO", "OFF")
            .define("USE_ASYNC", "OFF")
            // The NDK's clang turns on fortify diagnostics that espeak-ng's
            // build treats as fatal, so an upstream warning
            // ("'strcpy' called with string bigger than buffer" in speech.c)
            // stops the build. These are pre-existing upstream findings, not
            // something this port introduces, so downgrade them rather than
            // patching vendored C we do not maintain.
            .define("CMAKE_C_FLAGS", "-Wno-error")
            .define("CMAKE_CXX_FLAGS", "-Wno-error");
    }

    // General
    config
        .profile(&profile)
        .define("ENABLE_TESTS", "OFF")
        .define(
            "COMPILE_INTONATIONS",
            if cfg!(feature = "compile-espeak-intonations") {
                "ON"
            } else {
                "OFF"
            },
        )
        .very_verbose(std::env::var("CMAKE_VERBOSE").is_ok()) // Not verbose by default
        .always_configure(false);

    let bindings_dir = config.build();

    // Search paths
    println!("cargo:rustc-link-search={}", out_dir.join("lib").display());
    println!(
        "cargo:rustc-link-search={}",
        out_dir.join("build/src/speechPlayer").display()
    );
    println!(
        "cargo:rustc-link-search={}",
        out_dir.join("build/src/ucd-tools").display()
    );
    println!("cargo:rustc-link-search={}", bindings_dir.display());

    if target_os() == "windows" {
        println!(
            "cargo:rustc-link-search={}",
            out_dir.join("build/src/speechPlayer/Release").display()
        );
        println!(
            "cargo:rustc-link-search={}",
            out_dir.join("build/src/ucd-tools/Release").display()
        );
    }

    // macOS
    if target_os() == "macos" {
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=c++");
    }

    // Link libraries
    let espeak_libs_kind = if build_shared_libs { "dylib" } else { "static" };
    let espeak_libs = extract_lib_names(&out_dir, build_shared_libs);

    for lib in espeak_libs {
        debug_log!(
            "LINK {}",
            format!("cargo:rustc-link-lib={}={}", espeak_libs_kind, lib)
        );
        println!(
            "{}",
            format!("cargo:rustc-link-lib={}={}", espeak_libs_kind, lib)
        );
    }

    // Windows debug
    if cfg!(debug_assertions) && target.contains("msvc") {
        println!("cargo:rustc-link-lib=dylib=msvcrtd");
    }

    // Linux
    if target_os() == "linux" {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }

    if target.contains("apple") {
        // On (older) OSX we need to link against the clang runtime,
        // which is hidden in some non-default path.
        //
        // More details at https://github.com/alexcrichton/curl-rust/issues/279.
        if let Some(path) = macos_link_search_path() {
            println!("cargo:rustc-link-lib=clang_rt.osx");
            println!("cargo:rustc-link-search={}", path);
        }
    }

    // copy DLLs to target
    if build_shared_libs {
        let libs_assets = extract_lib_assets(&out_dir);
        for asset in libs_assets {
            let asset_clone = asset.clone();
            let filename = asset_clone.file_name().unwrap();
            let filename = filename.to_str().unwrap();
            let dst = target_dir.join(filename);
            debug_log!("HARD LINK {} TO {}", asset.display(), dst.display());
            if !dst.exists() {
                std::fs::hard_link(asset.clone(), dst).unwrap();
            }

            // Copy DLLs to examples as well
            if target_dir.join("examples").exists() {
                let dst = target_dir.join("examples").join(filename);
                debug_log!("HARD LINK {} TO {}", asset.display(), dst.display());
                if !dst.exists() {
                    std::fs::hard_link(asset.clone(), dst).unwrap();
                }
            }

            // Copy DLLs to target/profile/deps as well for tests
            let dst = target_dir.join("deps").join(filename);
            debug_log!("HARD LINK {} TO {}", asset.display(), dst.display());
            if !dst.exists() {
                std::fs::hard_link(asset.clone(), dst).unwrap();
            }
        }
    }
}
