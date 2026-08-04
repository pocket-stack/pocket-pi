# Pocket Pi dashboard

This is the PocketJS frontend for the P4 appliance. It currently uses a
deterministic credential-free projection so it can be compiled and visually
tested without a broker account. The firmware driver will replace that
projection with versioned `DeviceState` and `PortfolioSummary` updates.

The dashboard remains on PocketJS's validated 480x272 portable target while
the ESP32-P4 host contract is added upstream. The firmware already knows the
native 720x1280 panel geometry; it must not mislabel a new viewport as PSP just
to bypass target admission. Layout intentionally uses flexbox so it can move
to the portrait P4 contract without restructuring the component tree.
