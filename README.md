# piper-rs

> ## Fork: adds Android cross-compilation
>
> This is a fork of [`piper-rs`](https://github.com/thewh1teagle/piper-rs) that
> makes `espeak-rs-sys` build for `aarch64-linux-android`. Upstream cannot
> cross-compile at all — not "with difficulty", but with a hard failure before
> any code is generated.
>
> Nothing else is changed: the API, the host build and the crates.io versions are
> untouched, so this is a drop-in replacement wherever upstream is used.
>
> **The four fixes, all in `crates/espeak-rs-sys/build.rs`:**
>
> 1. **`cfg!(target_os = ...)` was used in five places.** Inside a build script
>    `cfg!` reports the machine doing the *building*, not the target, so every
>    one of those branches took the wrong path when cross-compiling. Now reads
>    `CARGO_CFG_TARGET_OS`.
> 2. **bindgen was never told the target.** It read the host's `/usr/include`
>    while generating bindings for aarch64, which fails with:
>    `Target platform requires --no-size_t-is-usize. The size of ssize_t (4) does
>    not match the target pointer size (8)`. Now passed `--target`, `--sysroot`
>    and `size_t_is_usize(false)`.
> 3. **Include paths were incomplete.** `wrapper.h`'s *quoted* includes resolve
>    relative to `wrapper.h` itself (into the source tree), but the headers they
>    pull in use *angle-bracket* includes that only search `-I` paths — so the
>    first hop resolved and the second failed with `'espeak-ng/speak_lib.h' file
>    not found`. The source tree is now on the include path too.
> 4. **CMake got no Android toolchain.** Now passed the NDK's
>    `android.toolchain.cmake` plus `ANDROID_ABI` / `ANDROID_PLATFORM`, with
>    `USE_LIBPCAUDIO` and `USE_ASYNC` off — Android apps play audio themselves
>    (oboe), and espeak-ng only needs to synthesise into a buffer.
>
> A fifth fix is **not Android-specific** and is worth upstreaming on its own:
> the vendored-source copy was guarded by `espeak_dst.exists()`, but
> `copy_folder` calls `create_dir_all` *before* copying. Any run that failed
> after that point left an empty directory, which made every later run skip the
> copy and then die in CMake with `does not contain CMakeLists.txt` — a
> confusing failure with no relation to the original one. It now checks for
> `CMakeLists.txt` itself.
>
> **Two traps when building for Android**, both documented in
> [`BUILDING.md`](BUILDING.md):
>
> - **The checkout path must be short.** espeak-ng `strcpy`s its install path
>   (derived from `OUT_DIR`) into a fixed 160–230 byte buffer. From a deep
>   directory that overflows, and Android's bionic headers catch it at compile
>   time as `'strcpy' called with string bigger than buffer`. It is a genuine
>   overflow that glibc simply does not check as aggressively — so it looks like
>   an Android incompatibility and is not one.
> - **`CC`/`CXX`/`AR` must be set explicitly.** `cc-rs` looks for an unversioned
>   `aarch64-linux-android-clang++` that the NDK does not ship.
>
> Requires `git submodule update --init` — espeak-ng is a submodule, and without
> it the build fails on a missing `speak_lib.h`.


[![Crates](https://img.shields.io/crates/v/piper-rs?logo=rust&color=F07B3C)](https://crates.io/crates/piper-rs/)

Use [Piper](https://github.com/rhasspy/piper) TTS models in Rust.

## Features

-  Compatibility with all Piper TTS models
-  Support for multiple languages
-  High performance with pure Rust implementation

## Install

```console
cargo add piper-rs
```

## Examples

See [examples](examples)

## Models

All pretrained models available at [huggingface.co/rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices/tree/main)

## Credits

This project is inspired by [sonata](https://github.com/mush42/sonata), originally created by [mush42](https://github.com/mush42).