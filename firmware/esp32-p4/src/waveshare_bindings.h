#pragma once

// Keep the BSP's LVGL entry points out of the Rust binding surface. PocketJS
// owns UI layout and rendering; the BSP owns only the physical panel, touch,
// and backlight lifecycle.
#define BSP_CONFIG_NO_GRAPHIC_LIB 1

#include "bsp/esp-bsp.h"
#include "bsp/display.h"
