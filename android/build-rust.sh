#!/usr/bin/env bash
# Builds libpassel_ffi.so for each Android ABI and generates its Kotlin
# bindings. Gradle runs it before every build (task `buildRust`); it can
# also be run by hand:
#
#   android/build-rust.sh <out-dir> [abi...]
#
# <out-dir>/jniLibs/<abi>/libpassel_ffi.so and <out-dir>/kotlin/ are
# written. ABIs default to arm64-v8a and x86_64 (the emulator).
#
# Needs the Android NDK: $ANDROID_NDK_HOME, or the newest one under
# $ANDROID_HOME/ndk (or ~/Android/Sdk/ndk).
set -euo pipefail

out=$(realpath -m "${1:?usage: build-rust.sh <out-dir> [abi...]}")
shift
abis=("${@:-arm64-v8a x86_64}")
# shellcheck disable=SC2206 # split the default list
abis=(${abis[*]})

repo=$(cd "$(dirname "$0")/.." && pwd)
min_api=26

ndk=${ANDROID_NDK_HOME:-}
if [[ -z $ndk ]]; then
    for sdk in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Android/Sdk"; do
        [[ -n $sdk && -d $sdk/ndk ]] || continue
        ndk=$(find "$sdk/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n1)
        [[ -n $ndk ]] && break
    done
fi
[[ -d $ndk ]] || { echo "Android NDK not found: set ANDROID_NDK_HOME" >&2; exit 1; }
toolchain="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
export PATH="$toolchain:$PATH" ANDROID_NDK_ROOT="$ndk" ANDROID_NDK_HOME="$ndk"

target_dir="$repo/build/target"
for abi in "${abis[@]}"; do
    case $abi in
        arm64-v8a) triple=aarch64-linux-android ;;
        x86_64) triple=x86_64-linux-android ;;
        armeabi-v7a) triple=armv7-linux-androideabi ;;
        *) echo "unsupported ABI: $abi" >&2; exit 1 ;;
    esac
    clang_triple=${triple/armv7-linux-androideabi/armv7a-linux-androideabi}
    env_triple=${triple//-/_}
    linker="$toolchain/$clang_triple$min_api-clang"
    export "CC_$env_triple=$linker" "AR_$env_triple=$toolchain/llvm-ar" \
        "RANLIB_$env_triple=$toolchain/llvm-ranlib"
    export "CARGO_TARGET_${env_triple^^}_LINKER=$linker"

    # Release even for debug APKs: key derivation is painfully slow
    # without optimizations.
    cargo build --manifest-path "$repo/Cargo.toml" --target-dir "$target_dir" \
        -p passel-ffi --features vendored --release --target "$triple"
    mkdir -p "$out/jniLibs/$abi"
    cp "$target_dir/$triple/release/libpassel_ffi.so" "$out/jniLibs/$abi/"
done

rm -rf "$out/kotlin"
cargo run --manifest-path "$repo/Cargo.toml" --target-dir "$target_dir" \
    -p passel-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library "$out/jniLibs/${abis[0]}/libpassel_ffi.so" \
    --language kotlin --no-format --out-dir "$out/kotlin"
