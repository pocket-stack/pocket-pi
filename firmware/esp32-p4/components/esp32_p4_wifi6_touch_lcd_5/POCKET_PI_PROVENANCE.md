# Source provenance

This directory is copied from Waveshare's Apache-2.0 product repository:

- repository: `waveshareteam/ESP32-P4-WIFI6-Touch-LCD-5`
- source commit: `5905c4156f250c61d05c4c48b34d83d367b0ae7d`
- source path: `examples/esp-idf/07_Displaycolorbar/components/esp32_p4_wifi6_touch_lcd_5`

Pocket Pi changes the component build to define
`BSP_CONFIG_NO_GRAPHIC_LIB=1` and removes its LVGL dependencies. PocketJS is
the only graphics runtime in the firmware.

The vendor's `noglib` branch referenced an LVGL-only display configuration
type from its standalone touch API. Pocket Pi splits that API onto the small
`bsp_touch_config_t` structure so the advertised no-graphics build compiles.
