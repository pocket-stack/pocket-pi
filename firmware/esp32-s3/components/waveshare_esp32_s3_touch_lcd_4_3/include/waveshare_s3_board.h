#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "esp_err.h"
#include "esp_lcd_panel_ops.h"
#include "esp_lcd_touch.h"

esp_err_t pi_s3_board_init(
    esp_lcd_panel_handle_t *panel,
    esp_lcd_touch_handle_t *touch,
    uint16_t **framebuffer
);

esp_err_t pi_s3_backlight_on(void);

bool pi_s3_touch_read(
    esp_lcd_touch_handle_t touch,
    uint16_t *x,
    uint16_t *y
);
