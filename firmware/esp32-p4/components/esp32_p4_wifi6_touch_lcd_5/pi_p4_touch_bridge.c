#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include "bsp/esp32_p4_wifi6_touch_lcd_5.h"
#include "bsp/touch.h"
#include "driver/i2c_master.h"
#include "esp_lcd_panel_io.h"
#include "esp_lcd_touch_gt911.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

esp_err_t pi_p4_touch_new(esp_lcd_touch_handle_t *ret_touch)
{
    if (ret_touch == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    i2c_master_bus_handle_t bus = NULL;
    esp_err_t result = i2c_master_get_bus_handle(BSP_I2C_NUM, &bus);
    if (result != ESP_OK || bus == NULL) {
        return result == ESP_OK ? ESP_ERR_INVALID_STATE : result;
    }

    uint8_t address = ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS;
    if (i2c_master_probe(bus, address, 100) != ESP_OK) {
        address = ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS_BACKUP;
        if (i2c_master_probe(bus, address, 100) != ESP_OK) {
            return ESP_ERR_NOT_FOUND;
        }
    }

    esp_lcd_panel_io_i2c_config_t io_config = ESP_LCD_TOUCH_IO_I2C_GT911_CONFIG();
    io_config.dev_addr = address;
    io_config.scl_speed_hz = CONFIG_BSP_I2C_CLK_SPEED_HZ;
    esp_lcd_panel_io_handle_t io = NULL;
    result = esp_lcd_new_panel_io_i2c(bus, &io_config, &io);
    if (result != ESP_OK) {
        return result;
    }

    const esp_lcd_touch_config_t touch_config = {
        .x_max = BSP_LCD_H_RES,
        .y_max = BSP_LCD_V_RES,
        .rst_gpio_num = BSP_LCD_TOUCH_RST,
        .int_gpio_num = BSP_LCD_TOUCH_INT,
        .levels = {.reset = 0, .interrupt = 0},
        .flags = {.swap_xy = 0, .mirror_x = 0, .mirror_y = 0},
    };
    result = esp_lcd_touch_new_i2c_gt911(io, &touch_config, ret_touch);
    if (result != ESP_OK) {
        esp_lcd_panel_io_del(io);
    }
    return result;
}

bool pi_p4_touch_read(esp_lcd_touch_handle_t touch, uint16_t *x, uint16_t *y)
{
    esp_lcd_touch_point_data_t point = {0};
    uint8_t count = 0;
    if (touch == NULL || x == NULL || y == NULL) {
        return false;
    }
    if (esp_lcd_touch_read_data(touch) != ESP_OK) {
        return false;
    }
    if (esp_lcd_touch_get_data(touch, &point, &count, 1) != ESP_OK || count == 0) {
        return false;
    }
    *x = point.x;
    *y = point.y;
    return true;
}

bool pi_p4_cpu_load_percent(uint8_t *percent)
{
#if configGENERATE_RUN_TIME_STATS
    enum { MAX_TASKS = 64 };
    static TaskStatus_t tasks[MAX_TASKS];
    static configRUN_TIME_COUNTER_TYPE previous_total = 0;
    static uint64_t previous_idle = 0;
    configRUN_TIME_COUNTER_TYPE total = 0;
    uint64_t idle = 0;

    if (percent == NULL) {
        return false;
    }
    UBaseType_t count = uxTaskGetSystemState(tasks, MAX_TASKS, &total);
    if (count == 0) {
        return false;
    }
    for (UBaseType_t index = 0; index < count; ++index) {
        if (tasks[index].pcTaskName != NULL && strncmp(tasks[index].pcTaskName, "IDLE", 4) == 0) {
            idle += tasks[index].ulRunTimeCounter;
        }
    }
    if (previous_total == 0 || total <= previous_total || idle < previous_idle) {
        previous_total = total;
        previous_idle = idle;
        return false;
    }

    const uint64_t elapsed = (uint64_t)(total - previous_total) * configNUMBER_OF_CORES;
    const uint64_t idle_elapsed = idle - previous_idle;
    previous_total = total;
    previous_idle = idle;
    if (elapsed == 0) {
        return false;
    }
    const uint64_t idle_percent = (idle_elapsed * 100U) / elapsed;
    *percent = (uint8_t)(idle_percent >= 100U ? 0U : 100U - idle_percent);
    return true;
#else
    (void)percent;
    return false;
#endif
}
