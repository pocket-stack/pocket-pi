#pragma once

// Keep the BSP's LVGL entry points out of the Rust binding surface. PocketJS
// owns UI layout and rendering; the BSP owns only the physical panel, touch,
// and backlight lifecycle.
#define BSP_CONFIG_NO_GRAPHIC_LIB 1

#include "bsp/esp-bsp.h"
#include "bsp/display.h"
#include "bsp/touch.h"

esp_err_t pi_p4_touch_new(esp_lcd_touch_handle_t *ret_touch);
bool pi_p4_touch_read(esp_lcd_touch_handle_t touch, uint16_t *x, uint16_t *y);
bool pi_p4_cpu_load_percent(uint8_t *percent);
