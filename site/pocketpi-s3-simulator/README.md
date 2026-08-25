# PocketPi S3 Hero screen frames

The four PNGs in `public/pocketpi-device/screens/` are deterministic output
from PocketPi's real ESP32 simulator, not website mockups.

They are rendered with the S3 logical viewport `480x800`, which corresponds to
the Waveshare 800x480 panel after the physical device is rotated into portrait.
The website maps raycast UV coordinates on the GLB screen back into that same
480x800 coordinate system, so the navigation buttons visible in the simulator
frame are also the controls users tap in the 3D model.

Run `./render-screens.sh` to refresh all four frames from a local simulator
checkout. Set `POCKETPI_SIM_ROOT` when that checkout lives elsewhere.
