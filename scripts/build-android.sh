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
  case "$T" in
    aarch64-linux-android) CC_NAME="aarch64-linux-android${API}-clang" ;;
    armv7-linux-androideabi) CC_NAME="armv7a-linux-androideabi${API}-clang" ;;
    i686-linux-android) CC_NAME="i686-linux-android${API}-clang" ;;
    x86_64-linux-android) CC_NAME="x86_64-linux-android${API}-clang" ;;
  esac
  CC="$TOOLCHAIN/bin/$CC_NAME"
  AR="$TOOLCHAIN/bin/llvm-ar"
  ENV_PREFIX=$(echo "$T" | tr 'a-z-' 'A-Z_')
  export "CARGO_TARGET_${ENV_PREFIX}_LINKER=$CC"
  export "AR_${ENV_PREFIX}=$AR"
  cargo build --release --target "$T"
done

echo ">> done. artifacts in target/<triple>/release/"
EOF
chmod +x /home/shrawan/experiments/kstream/kstream-rtmp/scripts/build-android.sh