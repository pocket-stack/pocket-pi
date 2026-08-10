# Source provenance

The C ABI source is copied from `pocket-stack/pocketjs` at commit
`9c809bbd047ddc75c27caa4990951a78d942477a`, matching the Rust dependency
pinned in the firmware manifest. The component manifest retains the
ESP-IDF 5.5 compatibility range already proven by `esp32-pi-agent` on this
board. It provides the C ABI used by
`pocketjs_esp32p4_ppa::EspIdfPpaOps` and remains under PocketJS's MIT license.
