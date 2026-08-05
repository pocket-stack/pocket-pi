use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::LineTransport;

pub struct UartLineTransport {
    pending: Mutex<Vec<u8>>,
}

impl UartLineTransport {
    pub fn new() -> Result<Self, String> {
        let port = esp_idf_svc::sys::uart_port_t_UART_NUM_0;
        if unsafe { esp_idf_svc::sys::uart_is_driver_installed(port) } {
            let status = unsafe { esp_idf_svc::sys::uart_driver_delete(port) };
            if status != esp_idf_svc::sys::ESP_OK {
                return Err(format!("replace console UART driver: ESP error {status}"));
            }
        }
        let status = unsafe {
            esp_idf_svc::sys::uart_driver_install(port, 32 * 1024, 0, 0, core::ptr::null_mut(), 0)
        };
        if status != esp_idf_svc::sys::ESP_OK {
            return Err(format!("install UART driver: ESP error {status}"));
        }
        Ok(Self {
            pending: Mutex::new(Vec::with_capacity(1024)),
        })
    }
}

impl LineTransport for UartLineTransport {
    fn write_line(&self, line: &str) {
        let line = format!("{line}\n");
        unsafe {
            esp_idf_svc::sys::uart_write_bytes(
                esp_idf_svc::sys::uart_port_t_UART_NUM_0,
                line.as_ptr().cast(),
                line.len(),
            );
        }
    }

    fn read_frame(&self, prefix: &str, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        let mut frame = self
            .pending
            .lock()
            .map_err(|_| "UART receive buffer lock was poisoned".to_owned())?;
        let mut byte = [0u8; 1];
        while Instant::now() < deadline {
            let count = unsafe {
                esp_idf_svc::sys::uart_read_bytes(
                    esp_idf_svc::sys::uart_port_t_UART_NUM_0,
                    byte.as_mut_ptr().cast(),
                    1,
                    1,
                )
            };
            match count {
                0 => std::thread::sleep(Duration::from_millis(10)),
                1 if byte[0] == b'\n' => {
                    let line = String::from_utf8_lossy(&frame).trim().to_owned();
                    frame.clear();
                    if line.starts_with(prefix) {
                        return Ok(line);
                    }
                }
                1 if byte[0] == b'\r' => {}
                1 if frame.len() < 32 * 1024 => frame.push(byte[0]),
                1 => frame.clear(),
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        Err(format!("timed out waiting for {prefix}"))
    }
}
