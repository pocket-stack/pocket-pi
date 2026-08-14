# Source provenance

The C ABI source is copied from `pocket-stack/pocketjs` at commit
`9c809bbd047ddc75c27caa4990951a78d942477a`. Pocket Pi's Rust dependencies now
pin `e12cf12f82cc60b636368119d49a06eb9ed2a3d5`; that later revision does not
change this copied C component. The component manifest retains the
ESP-IDF 5.5 compatibility range already proven by `esp32-pi-agent` on this
board. It provides the C ABI used by
`pocketjs_esp32p4_ppa::EspIdfPpaOps` and remains under PocketJS's MIT license.
