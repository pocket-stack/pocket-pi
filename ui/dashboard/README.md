# Pocket Pi dashboard

This is the PocketJS frontend for the P4 appliance. It currently uses a
deterministic credential-free projection so it can be compiled and visually
tested without a broker account. The firmware driver will replace that
projection with versioned `DeviceState` and `PortfolioSummary` updates.

The 480x272 logical viewport is provisional. The exact board profile selects
the final viewport and presentation policy; layout intentionally uses flexbox
so the component structure does not depend on a particular panel.
