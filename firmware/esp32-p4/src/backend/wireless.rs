use core::time::Duration;

use embedded_svc::http::{client::Client as HttpClient, Method};
use embedded_svc::io::Write;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use pocket_pi_embedded::ModelBackend;
use pocket_pi_protocols::anthropic_messages;
use pocket_pi_protocols::model::{ModelStreamEvent, WirelessProvider};
use pocket_pi_protocols::openai_chat;

pub struct WirelessBackend {
    provider: WirelessProvider,
    api_key: String,
}

impl WirelessBackend {
    pub fn new(provider: WirelessProvider, api_key: String) -> Result<Self, String> {
        if api_key.is_empty() || api_key.len() > 512 || !api_key.is_ascii() {
            return Err(format!("{} API key is invalid", provider.id()));
        }
        Ok(Self { provider, api_key })
    }

    fn request(&self, pi_request: &str) -> Result<(&'static str, String), String> {
        match self.provider {
            WirelessProvider::OpenAi => Ok((
                "https://api.openai.com/v1/chat/completions",
                openai_chat::build_request(pi_request)?,
            )),
            WirelessProvider::OpenRouter => Ok((
                "https://openrouter.ai/api/v1/chat/completions",
                openai_chat::build_request_for(pi_request, openai_chat::Dialect::OpenRouter)?,
            )),
            WirelessProvider::DeepSeek => Ok((
                "https://api.deepseek.com/chat/completions",
                openai_chat::build_request_for(pi_request, openai_chat::Dialect::DeepSeek)?,
            )),
            WirelessProvider::Anthropic => Ok((
                "https://api.anthropic.com/v1/messages",
                anthropic_messages::build_request(pi_request)?,
            )),
        }
    }
}

impl ModelBackend for WirelessBackend {
    fn complete(
        &self,
        request_json: &str,
        on_event: &mut dyn FnMut(ModelStreamEvent),
    ) -> Result<String, String> {
        let (endpoint, body) = self.request(request_json)?;
        let content_length = body.len().to_string();
        let bearer = format!("Bearer {}", self.api_key);
        let mut headers = vec![
            ("accept", "text/event-stream"),
            ("content-type", "application/json"),
            ("content-length", content_length.as_str()),
            ("user-agent", "pocket-pi-p4/0.1"),
        ];
        match self.provider {
            WirelessProvider::Anthropic => {
                headers.push(("x-api-key", self.api_key.as_str()));
                headers.push(("anthropic-version", "2023-06-01"));
            }
            WirelessProvider::OpenAi
            | WirelessProvider::OpenRouter
            | WirelessProvider::DeepSeek => {
                headers.push(("authorization", bearer.as_str()));
            }
        }

        let configuration = Configuration {
            buffer_size: Some(4 * 1024),
            buffer_size_tx: Some(4 * 1024),
            timeout: Some(Duration::from_secs(180)),
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        let mut client = HttpClient::wrap(
            EspHttpConnection::new(&configuration)
                .map_err(|error| format!("initialize HTTPS: {error}"))?,
        );
        let mut request = client
            .request(Method::Post, endpoint, &headers)
            .map_err(|error| format!("create model request: {error}"))?;
        request
            .write_all(body.as_bytes())
            .map_err(|error| format!("write model request: {error}"))?;
        request
            .flush()
            .map_err(|error| format!("flush model request: {error}"))?;
        let mut response = request
            .submit()
            .map_err(|error| format!("send model request: {error}"))?;
        let status = response.status();
        let mut decoder = match self.provider {
            WirelessProvider::Anthropic => ProviderStream::Anthropic(Default::default()),
            WirelessProvider::DeepSeek => {
                ProviderStream::Chat(openai_chat::Stream::new(openai_chat::Dialect::DeepSeek))
            }
            WirelessProvider::OpenAi | WirelessProvider::OpenRouter => {
                ProviderStream::Chat(Default::default())
            }
        };
        let mut pending = Vec::with_capacity(4 * 1024);
        let mut chunk = [0u8; 2 * 1024];
        loop {
            let count = response
                .read(&mut chunk)
                .map_err(|error| format!("read model stream: {error}"))?;
            if count == 0 {
                break;
            }
            pending.extend_from_slice(&chunk[..count]);
            if (200..300).contains(&status) {
                drain_sse_lines(&mut pending, &mut decoder, on_event)?;
            }
        }
        if !(200..300).contains(&status) {
            return Err(format!(
                "{} returned HTTP {status}: {}",
                self.provider.id(),
                String::from_utf8_lossy(&pending)
                    .chars()
                    .take(400)
                    .collect::<String>()
            ));
        }
        if !pending.is_empty() {
            pending.push(b'\n');
            drain_sse_lines(&mut pending, &mut decoder, on_event)?;
        }
        decoder.finish()
    }
}

fn drain_sse_lines(
    pending: &mut Vec<u8>,
    decoder: &mut ProviderStream,
    on_event: &mut dyn FnMut(ModelStreamEvent),
) -> Result<(), String> {
    while let Some(end) = pending.iter().position(|byte| *byte == b'\n') {
        let mut line = pending.drain(..=end).collect::<Vec<_>>();
        while matches!(line.last(), Some(b'\n' | b'\r')) {
            line.pop();
        }
        let line = core::str::from_utf8(&line)
            .map_err(|error| format!("model SSE line UTF-8: {error}"))?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim_start();
        if !data.is_empty() && data != "[DONE]" {
            for event in decoder.push(data)? {
                on_event(event);
            }
        }
    }
    Ok(())
}

enum ProviderStream {
    Chat(openai_chat::Stream),
    Anthropic(anthropic_messages::Stream),
}

impl ProviderStream {
    fn push(&mut self, data: &str) -> Result<Vec<ModelStreamEvent>, String> {
        match self {
            Self::Chat(stream) => stream.push(data),
            Self::Anthropic(stream) => stream.push(data),
        }
    }

    fn finish(self) -> Result<String, String> {
        match self {
            Self::Chat(stream) => stream.finish(),
            Self::Anthropic(stream) => stream.finish(),
        }
    }
}
