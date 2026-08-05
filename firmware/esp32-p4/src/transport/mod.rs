mod provisioning;
mod uart;

use std::time::Duration;

pub use provisioning::{request_runtime_config, RuntimeConfig};
pub use uart::UartLineTransport;

pub trait LineTransport: Send + Sync {
    fn write_line(&self, line: &str);
    fn read_frame(&self, prefix: &str, timeout: Duration) -> Result<String, String>;
}
