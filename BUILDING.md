## Gotchas

### Link failed on Windows:

If you encounter linking errors such as

```console
error LNK2019: unresolved external symbol __std_mismatch_1 referenced in function "private: class onnxruntime::common::Status
```

Please make sure your visual studio is >= 17.11 (Update through Visual studio installer)

## Publish new version

```console
cargo publish -p espeak-rs-sys
cargo publish -p espeak-rs
cargo publish -p sonic-rs-sys
cargo publish -p piper-rs
cargo publish -p piper-rs-cli
```

Note: Please don't create PR from your main branch. only from new feature branch!

## Install piper-rs-cli from Git

```console
cargo install piper-rs-cli --git https://github.com/thewh1teagle/piper-rs
```

## Cross-compiling for Android

Supported as of the Android build-script fixes. Requires an NDK and the target:

```sh
rustup target add aarch64-linux-android
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/27.0.12077973
TC="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC_aarch64_linux_android="$TC/aarch64-linux-android24-clang"
export CXX_aarch64_linux_android="$TC/aarch64-linux-android24-clang++"
export AR_aarch64_linux_android="$TC/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android24-clang"

git submodule update --init      # espeak-ng is a submodule; without it the
                                 # build fails on a missing speak_lib.h
cargo build --target aarch64-linux-android
```

`cc-rs` looks for an unversioned `aarch64-linux-android-clang++`, which the NDK
does not ship — hence the explicit `CC_*`/`CXX_*` above. Without them the build
fails with `failed to find tool "aarch64-linux-android-clang++"`.

### Keep the checkout path short

espeak-ng does `strcpy(path_home, PATH_ESPEAK_DATA)` into a fixed **160–230
byte** buffer, where `PATH_ESPEAK_DATA` is the compile-time install path derived
from `OUT_DIR`. From a deep directory that overflows, and Android's bionic
headers catch it at compile time as a hard error:

```
speech.c:338: error: 'strcpy' called with string bigger than buffer
```

It is a genuine overflow, not an over-eager diagnostic — glibc simply does not
check it as aggressively, which is why the same tree builds on the host. It is
also **not** an Android incompatibility, despite looking like one. Build from a
short path (`~/repos/piper-rs` is fine; a ~100-character scratch directory is
not).
