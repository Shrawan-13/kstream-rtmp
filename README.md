# `kstream-rtmp`

Sans-I/O (Enhanced) RTMP protocol engine for **kstream** — an Android-first,
super-lightweight, high-performance streaming core.

Forked from [`rtmpx`](https://github.com/darfink/rtmpx) 3.1.1 and re-engineered
for the kstream use case: a small, single-crate RTMP transport with an
ergonomic blocking [`Publisher`](src/publisher.rs) API, Android cross-build
support, and an optimized release profile.

## What it provides

- Client + server RTMP sessions (publish and ingest/playback).
- Legacy + **Enhanced RTMP v2 r2** (HEVC, AV1, VVC, VP8/9, Opus, FLAC, AC-3, E-AC-3, AAC, MP3; multitrack, multichannel, ModEx).
- AMF0 + AMF3 (graph documents with identity/cycles).
- Zero-copy media path: segmented payloads, `PayloadPool`, borrowed `ValidatedMedia`.
- Media inspection + elementary-unit extraction (no codec decode).
- High-level blocking [`Publisher`](src/publisher.rs) for "very little work" publishing.

It performs **no I/O, no TLS, no reconnect, no muxing, no codec packaging** —
the app (kstream engine) owns those. This keeps the crate small and predictable.

## Quick start (publish)

```rust
use kstream_rtmp::publisher::{Publisher, PublisherConfig};

let mut publisher = Publisher::connect(
    "192.168.1.10:1935",
    PublisherConfig {
        app: "live".into(),
        stream_key: "demo".into(),
        ..Default::default()
    },
)?;
// `data` is the RTMP video message body (FLV video tag body).
publisher.send_video(0, true, &[0x17, 0x00])?;
publisher.finish()?;
# Ok::<(), kstream_rtmp::publisher::PublisherError>(())
```

## Build

```sh
cargo build                       # host
cargo test                        # full suite
cargo clippy --all-targets -- -D warnings
```

### Android

```sh
export ANDROID_HOME=/path/to/sdk
./scripts/build-android.sh        # aarch64, armv7, x86, x86_64
```

Release artifact (stripped) is ~340 KB for `aarch64-linux-android`.

## Low-level API

The full sans-I/O API is re-exported from the crate root (`sessions`, `chunk_io`,
`payload`, `handshake`, `media`, `enhanced`, `metadata`, `elementary`, `amf`,
`flv`, `time`), identical to rtmpx. Use it when you need full control.

## Provenance & license

Derived from `rtmpx` 3.1.1 (itself derived from `rml_rtmp`), MIT licensed.
See `LICENSE`.
```

Now update Cargo.toml readme already README.md. Good.

Now update lib.rs doc references? The doctests use `rtmpx::` → already replaced to `kstream_rtmp::`. Good. Let me verify lib.rs doctest compiles (part of cargo test). Already tests passed? Let me run cargo test again to be sure after publisher changes.<|end|><|start|>-assistant to=shell<|channel|>analysis <|constrain|>xml<|message|><rfl:parameter name="command">
cd /home/shrawan/experiments/kstream/kstream-rtmp && cargo test 2>&1 | grep -E "test result: (ok|FAILED)|error\[" | head; echo "=== doctest count ==="; cargo test 2>&1 | grep -c "test result: ok"