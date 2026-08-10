# Source provenance

The C ABI source is copied from `pocket-stack/pocketjs` at commit
`afc8d4e8e877dac7f9b0c01b5c0d667642009fc0`, matching the Rust dependency
pinned in the firmware manifest. The component manifest retains the
ESP-IDF 5.5 compatibility range already proven by `esp32-pi-agent` on this
board. It provides the C ABI used by
`pocketjs_esp32p4_ppa::EspIdfPpaOps` and remains under PocketJS's MIT license.
