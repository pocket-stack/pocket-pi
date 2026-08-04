
idf_component_register(
    SRCS "esp32_p4_wifi6_touch_lcd_5.c"
    INCLUDE_DIRS "include"
    PRIV_INCLUDE_DIRS "priv_include"
    REQUIRES driver
    PRIV_REQUIRES esp_lcd usb spiffs fatfs
)

# PocketJS owns the graphics runtime. Compile only the BSP's physical panel,
# touch, backlight, storage and audio lifecycle without pulling in LVGL.
target_compile_definitions(${COMPONENT_LIB} PUBLIC BSP_CONFIG_NO_GRAPHIC_LIB=1)
