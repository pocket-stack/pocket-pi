# Source provenance

The panel timing, GPIO map, CH422G reset/backlight sequence, and GT911 setup
come from Waveshare's `ESP32-S3-Touch-LCD-4.3` repository:

- repository: `waveshareteam/ESP32-S3-Touch-LCD-4.3`
- source commit: `7de23f06d4f26de69b3ec6cd07f6028b4f58b424`
- source path: `examples/ESP-IDF/08_lvgl_v8_demo/components/waveshare_rgb_lcd_port.c`
- upstream license: `CC0-1.0`

Pocket Pi keeps only the RGB panel, GT911 touch, and backlight lifecycle. LVGL
and the vendor's LVGL adapter are intentionally not included; PocketJS owns
all UI layout and rendering.
