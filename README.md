### Meshtastic x ESP32
A playground to hook up ESP32 ICs to Meshtastic radios via serial UART and/or bluetooth. There's no goal here except messing around with embedded `no_std` no alloc rust on an ESP32/S3, and interacting with a Meshtastic node over serial. Eventually some data will be rendered to an epaper display. I might hook up the ESP32 to the nymesh.sh MQTT via WIFI to render data + compare data w/ my local radio(s).

### Setup
- Toolchain/tooling: [Official Espressif Docs](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html#xtensa-devices)
- Source envs: `source ./env-esp.sh`
- Project local Rust-analyzer config: `./.nvim.lua`

- Add ESP variant targets as crates, and update:
  - Add targets + aliases in `./cargo/config.toml`
  - Manually update `./nvim.lua` to match the actual target you want to work on

NOTE: Espressif's custom toolchain doesn't work with the latest Rust-analyzer, I've found the older version `rust-analyzer 0.3.2500-standalone (a207299344 2025-06-15)` works fine.

#### Related
- NYC mesh: [nyme.sh](https://nyme.sh)
