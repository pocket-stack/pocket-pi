// Build-only input for system/view-sdk.pak. Pocket Pi Views set native
// properties directly; these literals select the fixed PocketJS font slots
// and glyphs shared by every source App.
const fontSlots = [
  "text-base", "text-lg", "text-base font-bold", "text-lg font-bold",
  "text-xl font-bold", "text-2xl font-bold",
];
const glyphs = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~ ‘’“”—…·•‹›↑↓$%";
void fontSlots;
void glyphs;
