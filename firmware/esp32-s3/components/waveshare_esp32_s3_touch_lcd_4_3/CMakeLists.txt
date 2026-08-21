idf_component_register(
    SRCS "waveshare_s3_board.c"
    INCLUDE_DIRS "include"
    REQUIRES driver esp_flash_dispatcher esp_hw_support esp_lcd esp_lcd_touch_gt911
    WHOLE_ARCHIVE
)
