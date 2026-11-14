#!/usr/bin/env bash
# Build kstream-rtmp for Android targets.
# Requires: Android NDK (ANDROID_NDK_HOME or ANDROID_HOME/ndk/<ver>), Rust targets installed.
set -euo pipefail

NDK="${ANDROID_NDK_HOME:-}"
if [ -z "$NDK" ]; then
  if [ -n "${ANDROID_HOME:-}" ]; then
    NDK="$(ls -d "$ANDROID_HOME"/ndk/* 2>/dev/null | sort -V | tail -1)"
  fi
fi
if [ -z "$NDK" ] || [ ! -d "$NDK" ]; then
  echo "Android NDK not found. Set ANDROID_NDK_HOME." >&2
  exit 1
fi

TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
export PATH="$TOOLCHAIN/bin:$PATH"

TARGETS="${TARGETS:-aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android}"
API="${API_LEVEL:-24}"

for T in $TARGETS; do
  echo ">> building $T (API $API)"
  CC="$TOOLCHAIN/bin/${T}${API}-clang"
  AR="$TOOLCHAIN/bin/llvm-ar"
  CARGO_TARGET_$(echo $T | tr 'a-z-' 'A-Z_')_LINKER="$CC" \
  AR_$(echo $T | tr 'a-z-' 'A-Z_')="$AR" \
  cargo build --release --target "$T"
done

echo ">> done. artifacts in target/<triple>/release/"
EOF
chmod +x /home/shrawan/experiments/kstream/kstream-rtmp/scripts/build-android.sh