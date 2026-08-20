/*
 * SPDX-License-Identifier: CC0-1.0
 *
 * Hardware values are derived from Waveshare's ESP32-S3-Touch-LCD-4.3
 * ESP-IDF demo. Pocket Pi removes every LVGL dependency.
 */

#include "waveshare_s3_board.h"

#include "driver/gpio.h"
#include "driver/i2c.h"
#include "esp_check.h"
#include "esp_lcd_panel_io.h"
#include "esp_lcd_panel_rgb.h"
#include "esp_lcd_touch_gt911.h"
#include "esp_rom_sys.h"
#include "freertos/FreeRTOS.h"

#define LCD_WIDTH 800
#define LCD_HEIGHT 480
#define LCD_PIXEL_CLOCK_HZ (16 * 1000 * 1000)
#define LCD_BOUNCE_BUFFER_PIXELS (LCD_WIDTH * 10)

#define I2C_PORT I2C_NUM_0
#define I2C_SDA GPIO_NUM_8
#define I2C_SCL GPIO_NUM_9
#define TOUCH_INT GPIO_NUM_4
#define I2C_TIMEOUT_MS 1000

static const char *TAG = "pi_s3_board";
static bool i2c_initialized;

static esp_err_t init_i2c(void)
{
    if (i2c_initialized) {
        return ESP_OK;
    }
    const i2c_config_t config = {
        .mode = I2C_MODE_MASTER,
        .sda_io_num = I2C_SDA,
        .scl_io_num = I2C_SCL,
        .sda_pullup_en = GPIO_PULLUP_ENABLE,
        .scl_pullup_en = GPIO_PULLUP_ENABLE,
        .master.clk_speed = 400000,
    };
    ESP_RETURN_ON_ERROR(i2c_param_config(I2C_PORT, &config), TAG, "configure I2C");
    ESP_RETURN_ON_ERROR(i2c_driver_install(I2C_PORT, config.mode, 0, 0, 0), TAG, "install I2C");
    i2c_initialized = true;
    return ESP_OK;
}

static esp_err_t write_expander(uint8_t address, uint8_t value)
{
    return i2c_master_write_to_device(
        I2C_PORT,
        address,
        &value,
        1,
        I2C_TIMEOUT_MS / portTICK_PERIOD_MS
    );
}

static esp_err_t reset_touch(void)
{
    const gpio_config_t config = {
        .pin_bit_mask = 1ULL << TOUCH_INT,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    ESP_RETURN_ON_ERROR(gpio_config(&config), TAG, "configure touch interrupt");
    ESP_RETURN_ON_ERROR(write_expander(0x24, 0x01), TAG, "configure CH422G mode");
    ESP_RETURN_ON_ERROR(write_expander(0x38, 0x2c), TAG, "assert touch reset");
    esp_rom_delay_us(100 * 1000);
    ESP_RETURN_ON_ERROR(gpio_set_level(TOUCH_INT, 0), TAG, "drive touch interrupt");
    esp_rom_delay_us(100 * 1000);
    ESP_RETURN_ON_ERROR(write_expander(0x38, 0x2e), TAG, "release touch reset");
    esp_rom_delay_us(200 * 1000);
    return ESP_OK;
}

esp_err_t pi_s3_board_init(
    esp_lcd_panel_handle_t *panel,
    esp_lcd_touch_handle_t *touch,
    uint16_t **framebuffer
)
{
    ESP_RETURN_ON_FALSE(panel && touch && framebuffer, ESP_ERR_INVALID_ARG, TAG, "invalid output");
    *panel = NULL;
    *touch = NULL;
    *framebuffer = NULL;

    const esp_lcd_rgb_panel_config_t panel_config = {
        .clk_src = LCD_CLK_SRC_DEFAULT,
        .timings = {
            .pclk_hz = LCD_PIXEL_CLOCK_HZ,
            .h_res = LCD_WIDTH,
            .v_res = LCD_HEIGHT,
            .hsync_pulse_width = 4,
            .hsync_back_porch = 8,
            .hsync_front_porch = 8,
            .vsync_pulse_width = 4,
            .vsync_back_porch = 8,
            .vsync_front_porch = 8,
            .flags = {
                .pclk_active_neg = 1,
            },
        },
        .data_width = 16,
        .bits_per_pixel = 16,
        .num_fbs = 1,
        .bounce_buffer_size_px = LCD_BOUNCE_BUFFER_PIXELS,
        .sram_trans_align = 4,
        .psram_trans_align = 64,
        .hsync_gpio_num = GPIO_NUM_46,
        .vsync_gpio_num = GPIO_NUM_3,
        .de_gpio_num = GPIO_NUM_5,
        .pclk_gpio_num = GPIO_NUM_7,
        .disp_gpio_num = GPIO_NUM_NC,
        .data_gpio_nums = {
            GPIO_NUM_14, GPIO_NUM_38, GPIO_NUM_18, GPIO_NUM_17,
            GPIO_NUM_10, GPIO_NUM_39, GPIO_NUM_0, GPIO_NUM_45,
            GPIO_NUM_48, GPIO_NUM_47, GPIO_NUM_21, GPIO_NUM_1,
            GPIO_NUM_2, GPIO_NUM_42, GPIO_NUM_41, GPIO_NUM_40,
        },
        .flags = {
            .fb_in_psram = 1,
        },
    };
    ESP_RETURN_ON_ERROR(esp_lcd_new_rgb_panel(&panel_config, panel), TAG, "create RGB panel");
    ESP_RETURN_ON_ERROR(esp_lcd_panel_init(*panel), TAG, "initialize RGB panel");
    ESP_RETURN_ON_ERROR(
        esp_lcd_rgb_panel_get_frame_buffer(*panel, 1, (void **)framebuffer),
        TAG,
        "get RGB framebuffer"
    );

    ESP_RETURN_ON_ERROR(init_i2c(), TAG, "initialize I2C");
    ESP_RETURN_ON_ERROR(reset_touch(), TAG, "reset GT911");
    esp_lcd_panel_io_handle_t touch_io = NULL;
    esp_lcd_panel_io_i2c_config_t touch_io_config = ESP_LCD_TOUCH_IO_I2C_GT911_CONFIG();
    touch_io_config.scl_speed_hz = 0;
    ESP_RETURN_ON_ERROR(
        esp_lcd_new_panel_io_i2c((esp_lcd_i2c_bus_handle_t)I2C_PORT, &touch_io_config, &touch_io),
        TAG,
        "create GT911 I2C IO"
    );
    const esp_lcd_touch_config_t touch_config = {
        .x_max = LCD_WIDTH,
        .y_max = LCD_HEIGHT,
        .rst_gpio_num = GPIO_NUM_NC,
        .int_gpio_num = GPIO_NUM_NC,
        .levels = { .reset = 0, .interrupt = 0 },
        .flags = { .swap_xy = 0, .mirror_x = 0, .mirror_y = 0 },
    };
    ESP_RETURN_ON_ERROR(
        esp_lcd_touch_new_i2c_gt911(touch_io, &touch_config, touch),
        TAG,
        "create GT911"
    );
    return ESP_OK;
}

esp_err_t pi_s3_backlight_on(void)
{
    ESP_RETURN_ON_ERROR(init_i2c(), TAG, "initialize I2C");
    ESP_RETURN_ON_ERROR(write_expander(0x24, 0x01), TAG, "configure CH422G mode");
    return write_expander(0x38, 0x1e);
}

bool pi_s3_touch_read(esp_lcd_touch_handle_t touch, uint16_t *x, uint16_t *y)
{
    uint16_t strength = 0;
    uint8_t count = 0;
    if (esp_lcd_touch_read_data(touch) != ESP_OK) {
        return false;
    }
    return esp_lcd_touch_get_coordinates(touch, x, y, &strength, &count, 1) && count > 0;
}
