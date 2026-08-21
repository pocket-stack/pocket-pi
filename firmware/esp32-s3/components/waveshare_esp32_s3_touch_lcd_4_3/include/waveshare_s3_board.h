#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "esp_err.h"
#include "esp_lcd_panel_ops.h"
#include "esp_lcd_touch.h"

esp_err_t pi_s3_flash_dispatcher_init(void);

esp_err_t pi_s3_board_init(
    esp_lcd_panel_handle_t *panel,
    esp_lcd_touch_handle_t *touch,
    uint16_t **framebuffer_0,
    uint16_t **framebuffer_1
);

esp_err_t pi_s3_backlight_on(void);

esp_err_t pi_s3_present(
    esp_lcd_panel_handle_t panel,
    const uint16_t *framebuffer
);

typedef struct {
    uint32_t vsync_count;
    uint32_t frame_count;
    uint32_t max_vsync_cycles;
    uint32_t max_frame_cycles;
} pi_s3_scan_stats_t;

bool pi_s3_take_scan_stats(pi_s3_scan_stats_t *stats);

bool pi_s3_touch_read(
    esp_lcd_touch_handle_t touch,
    uint16_t *x,
    uint16_t *y
);
