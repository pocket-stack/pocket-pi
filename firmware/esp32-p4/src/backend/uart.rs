use std::sync::Arc;
use std::time::Duration;

use pocket_pi_embedded::ModelBackend;

use crate::transport::LineTransport;

const READY: &str = "PPI-RPC-READY";
const REQUEST: &str = "PPI-RPC-REQUEST:";
const STREAM: &str = "PPI-RPC-STREAM:";

pub struct UartBackend {
    transport: Arc<dyn LineTransport>,
}

impl UartBackend {
    pub fn new(transport: Arc<dyn LineTransport>) -> Self {
        Self { transport }
    }
}

impl ModelBackend for UartBackend {
    fn complete(
        &self,
        request_json: &str,
        on_delta: &mut dyn FnMut(&str),
    ) -> Result<String, String> {
        self.transport.write_line("PPI-RPC-WAITING");
        self.transport.read_frame(READY, Duration::from_secs(45))?;
        self.transport
            .write_line(&format!("{REQUEST}{request_json}"));
        loop {
            let frame = self
                .transport
                .read_frame(STREAM, Duration::from_secs(180))?;
            let event: serde_json::Value = serde_json::from_str(
                frame
                    .strip_prefix(STREAM)
                    .ok_or_else(|| "UART stream prefix missing".to_owned())?,
            )
            .map_err(|error| format!("UART stream JSON: {error}"))?;
            match event.get("type").and_then(serde_json::Value::as_str) {
                Some("text_delta") => on_delta(
                    event
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| "UART text delta is missing text".to_owned())?,
                ),
                Some("done") => {
                    return serde_json::to_string(
                        event
                            .get("result")
                            .ok_or_else(|| "UART done event is missing result".to_owned())?,
                    )
                    .map_err(|error| format!("serialize UART model result: {error}"));
                }
                Some("error") => {
                    return Err(event
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Mac model bridge failed")
                        .to_owned());
                }
                Some(kind) => return Err(format!("unknown UART stream event: {kind}")),
                None => return Err("UART stream event is missing type".to_owned()),
            }
        }
    }
}
