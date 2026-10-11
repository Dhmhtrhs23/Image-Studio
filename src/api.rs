//! OpenRouter Image API + chat (image understanding) client.
//!
//! Reference: https://openrouter.ai/docs/guides/overview/multimodal/image-generation
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::BufRead;
use std::time::Duration;

pub const DEFAULT_BASE: &str = "https://openrouter.ai/api/v1";

// ---------------------------------------------------------------------------
// Discovery types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Architecture {
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
}

/// Typed capability descriptor from supported_parameters.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Capability {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub values: Option<Vec<String>>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ImageModel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub architecture: Architecture,
    #[serde(default)]
    pub supported_parameters: Value,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub endpoints: Option<String>,
}

impl ImageModel {
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.id.clone())
    }
    pub fn capabilities(&self) -> HashMap<String, Capability> {
        parse_capabilities(&self.supported_parameters)
    }
    pub fn cap(&self, key: &str) -> Option<Capability> {
        self.capabilities().get(key).cloned()
    }
    pub fn has(&self, key: &str) -> bool {
        self.capabilities().contains_key(key)
    }
    pub fn max_refs(&self) -> i64 {
        self.cap("input_references").and_then(|c| c.max).unwrap_or(0)
    }
    pub fn max_n(&self) -> i64 {
        self.cap("n").and_then(|c| c.max).unwrap_or(1).max(1)
    }
    pub fn input_modalities(&self) -> String {
        if self.architecture.input_modalities.is_empty() {
            "text".into()
        } else {
            self.architecture.input_modalities.join(" + ")
        }
    }
    pub fn output_modalities(&self) -> String {
        if self.architecture.output_modalities.is_empty() {
            "image".into()
        } else {
            self.architecture.output_modalities.join(" + ")
        }
    }
}

pub fn parse_capabilities(v: &Value) -> HashMap<String, Capability> {
    let mut out = HashMap::new();
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                if let Ok(c) = serde_json::from_value::<Capability>(val.clone()) {
                    out.insert(k.clone(), c);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    if let Ok(c) = serde_json::from_value::<Capability>(item.clone()) {
                        out.insert(name.to_string(), c);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct PricingLine {
    #[serde(default)]
    pub billable: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub variant: Option<String>,
}

impl PricingLine {
    pub fn label(&self) -> String {
        let cost = self.cost_usd.unwrap_or(0.0);
        let cost_s = if cost >= 0.01 {
            format!("{:.4}", cost)
        } else {
            format!("{:.6}", cost)
        };
        let bill = self.billable.clone().unwrap_or_else(|| "?".into());
        let unit = self.unit.clone().unwrap_or_default();
        match &self.variant {
            Some(v) => format!("{} / {} ({}) = USD {}", bill, unit, v, cost_s),
            None => format!("{} / {} = USD {}", bill, unit, cost_s),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Endpoint {
    #[serde(default)]
    pub provider_name: Option<String>,
    #[serde(default)]
    pub provider_slug: Option<String>,
    #[serde(default)]
    pub provider_tag: Option<String>,
    #[serde(default)]
    pub supported_parameters: Value,
    #[serde(default)]
    pub allowed_passthrough_parameters: Vec<String>,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub pricing: Vec<PricingLine>,
}

impl Endpoint {
    pub fn capabilities(&self) -> HashMap<String, Capability> {
        parse_capabilities(&self.supported_parameters)
    }
    pub fn label(&self) -> String {
        self.provider_name
            .clone()
            .or_else(|| self.provider_slug.clone())
            .unwrap_or_else(|| "Unknown provider".into())
    }
    pub fn pin_slug(&self) -> Option<String> {
        self.provider_tag
            .clone()
            .filter(|s| !s.is_empty() && s != "null")
            .or_else(|| self.provider_slug.clone())
    }
}

#[derive(Debug, Deserialize)]
struct ModelsEnvelope {
    #[serde(default)]
    data: Vec<ImageModel>,
}

#[derive(Debug, Deserialize)]
struct EndpointsEnvelope {
    #[serde(default)]
    endpoints: Vec<Endpoint>,
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InputRef {
    #[serde(rename = "type")]
    pub kind: String,
    pub image_url: ImageUrl,
}

impl InputRef {
    pub fn from_data_uri(uri: String) -> Self {
        InputRef {
            kind: "image_url".into(),
            image_url: ImageUrl { url: uri },
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ProviderRouting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_fallbacks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
}

impl ProviderRouting {
    pub fn is_empty(&self) -> bool {
        self.only.is_none()
            && self.order.is_none()
            && self.ignore.is_none()
            && self.sort.is_none()
            && self.allow_fallbacks.is_none()
            && self.options.is_none()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct GenRequest {
    pub model: String,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_references: Option<Vec<InputRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderRouting>,
}

impl GenRequest {
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: Option<i64>,
    #[serde(default)]
    pub completion_tokens: Option<i64>,
    #[serde(default)]
    pub total_tokens: Option<i64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

impl Usage {
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(c) = self.cost {
            parts.push(format!("cost USD {:.6}", c));
        }
        if let Some(t) = self.total_tokens {
            parts.push(format!("{} tokens", t));
        }
        if parts.is_empty() {
            "usage not reported".into()
        } else {
            parts.join(" | ")
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct GeneratedImage {
    #[serde(default)]
    pub b64_json: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ImageResult {
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub data: Vec<GeneratedImage>,
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// Events emitted while a generation request is running.
#[derive(Clone, Debug)]
pub enum GenEvent {
    Partial { index: i64, b64: String, media_type: Option<String> },
    Done(Box<ImageResult>),
    Error(String),
    Status(String),
}

/// Events emitted for image-understanding (chat) requests.
#[derive(Clone, Debug)]
pub enum ChatEvent {
    Delta(String),
    Done { text: String, usage: Option<Usage> },
    Error(String),
    Status(String),
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

pub fn make_client(timeout_secs: u64) -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(30)))
        .connect_timeout(Duration::from_secs(30))
        .user_agent("OpenRouter Image Studio/0.1 (GTK4)")
        .build()
        .map_err(|e| format!("Could not create HTTP client: {e}"))
}

fn api_error(status: reqwest::StatusCode, body: &str) -> String {
    let parsed: Option<String> = serde_json::from_str::<Value>(body).ok().and_then(|v| {
        v.pointer("/error/message")
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
            .or_else(|| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
    });
    match parsed {
        Some(m) => format!("HTTP {}: {}", status.as_u16(), m),
        None => format!(
            "HTTP {}: {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        ),
    }
}

fn auth(req: reqwest::blocking::RequestBuilder, key: &str) -> reqwest::blocking::RequestBuilder {
    req.bearer_auth(key)
        .header("HTTP-Referer", "https://github.com/local/openrouter-image-studio")
        .header("X-Title", "OpenRouter Image Studio")
}

pub fn fetch_image_models(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
) -> Result<Vec<ImageModel>, String> {
    let url = format!("{}/images/models", base.trim_end_matches('/'));
    let resp = auth(client.get(&url), key)
        .send()
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    let env: ModelsEnvelope =
        serde_json::from_str(&body).map_err(|e| format!("Bad models response: {e}"))?;
    let mut models = env.data;
    models.sort_by(|a, b| a.id.to_lowercase().cmp(&b.id.to_lowercase()));
    Ok(models)
}

pub fn fetch_endpoints(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
    model_id: &str,
) -> Result<Vec<Endpoint>, String> {
    let url = format!("{}/images/models/{}/endpoints", base.trim_end_matches('/'), model_id);
    let resp = auth(client.get(&url), key)
        .send()
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    let env: EndpointsEnvelope =
        serde_json::from_str(&body).map_err(|e| format!("Bad endpoints response: {e}"))?;
    Ok(env.endpoints)
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct KeyInfo {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub usage: Option<f64>,
    #[serde(default)]
    pub limit: Option<f64>,
    #[serde(default)]
    pub limit_remaining: Option<f64>,
    #[serde(default)]
    pub is_free_tier: Option<bool>,
}

pub fn fetch_key_info(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
) -> Result<KeyInfo, String> {
    let url = format!("{}/key", base.trim_end_matches('/'));
    let resp = auth(client.get(&url), key)
        .send()
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(api_error(status, &body));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| format!("Bad key response: {e}"))?;
    let data = v.get("data").cloned().unwrap_or(Value::Null);
    serde_json::from_value(data).map_err(|e| format!("Bad key data: {e}"))
}

/// Run an image generation request. emit is called from the worker thread.
pub fn generate(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
    request: &GenRequest,
    stream: bool,
    emit: &mut dyn FnMut(GenEvent),
) {
    let url = format!("{}/images", base.trim_end_matches('/'));
    let mut req = request.clone();
    req.stream = if stream { Some(true) } else { None };
    let body = req.to_json();
    emit(GenEvent::Status(format!(
        "POST {} | model={} | stream={}\n{}",
        url,
        request.model,
        stream,
        serde_json::to_string(&body).unwrap_or_default()
    )));
    let resp = match auth(client.post(&url), key).json(&body).send() {
        Ok(r) => r,
        Err(e) => {
            emit(GenEvent::Error(format!("Network error: {e}")));
            return;
        }
    };
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        emit(GenEvent::Error(api_error(status, &body)));
        return;
    }
    if !stream {
        let text = match resp.text() {
            Ok(t) => t,
            Err(e) => {
                emit(GenEvent::Error(format!("Read error: {e}")));
                return;
            }
        };
        match serde_json::from_str::<ImageResult>(&text) {
            Ok(r) => emit(GenEvent::Done(Box::new(r))),
            Err(e) => emit(GenEvent::Error(format!(
                "Bad generation response: {e} -- {}",
                text.chars().take(300).collect::<String>()
            ))),
        }
        return;
    }
    let reader = std::io::BufReader::new(resp);
    let mut accumulated: Vec<GeneratedImage> = Vec::new();
    let mut usage: Option<Usage> = None;
    let mut created: Option<i64> = None;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                emit(GenEvent::Error(format!("Stream read error: {e}")));
                return;
            }
        };
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let data = match line.strip_prefix("data:") {
            Some(d) => d.trim().to_string(),
            None => continue,
        };
        if data == "[DONE]" {
            break;
        }
        let v: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "image_generation.partial_image" => {
                let index = v.get("partial_image_index").and_then(|i| i.as_i64()).unwrap_or(0);
                if let Some(b64) = v.get("b64_json").and_then(|b| b.as_str()) {
                    emit(GenEvent::Partial {
                        index,
                        b64: b64.to_string(),
                        media_type: v
                            .get("media_type")
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string()),
                    });
                }
            }
            "image_generation.completed" => {
                if let Some(b64) = v.get("b64_json").and_then(|b| b.as_str()) {
                    accumulated.push(GeneratedImage {
                        b64_json: Some(b64.to_string()),
                        media_type: v
                            .get("media_type")
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string()),
                        url: None,
                    });
                }
                created = v.get("created").and_then(|c| c.as_i64()).or(created);
                if let Some(u) = v.get("usage") {
                    usage = serde_json::from_value(u.clone()).ok().or(usage);
                }
            }
            "error" => {
                let msg = v
                    .pointer("/error/message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("stream error");
                emit(GenEvent::Error(msg.to_string()));
                return;
            }
            _ => {}
        }
    }
    emit(GenEvent::Done(Box::new(ImageResult {
        created,
        data: accumulated,
        usage,
    })));
}

/// Image understanding via /chat/completions.
pub fn chat_with_images(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
    model: &str,
    prompt: &str,
    images: &[String],
    system: Option<&str>,
    stream: bool,
    max_tokens: Option<u32>,
    emit: &mut dyn FnMut(ChatEvent),
) {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let mut content: Vec<Value> = vec![json!({"type": "text", "text": prompt})];
    for img in images {
        content.push(json!({"type": "image_url", "image_url": {"url": img}}));
    }
    let mut messages: Vec<Value> = Vec::new();
    if let Some(s) = system {
        if !s.trim().is_empty() {
            messages.push(json!({"role": "system", "content": s}));
        }
    }
    messages.push(json!({"role": "user", "content": content}));
    let mut payload = json!({
        "model": model,
        "messages": messages,
        "stream": stream,
    });
    if let Some(mt) = max_tokens {
        payload["max_tokens"] = json!(mt);
    }
    emit(ChatEvent::Status(format!("POST {} | model={}", url, model)));
    let resp = match auth(client.post(&url), key).json(&payload).send() {
        Ok(r) => r,
        Err(e) => {
            emit(ChatEvent::Error(format!("Network error: {e}")));
            return;
        }
    };
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        emit(ChatEvent::Error(api_error(status, &body)));
        return;
    }
    if !stream {
        let text = resp.text().unwrap_or_default();
        let v: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                emit(ChatEvent::Error(format!("Bad chat response: {e}")));
                return;
            }
        };
        let content = extract_chat_text(&v);
        let usage = v.get("usage").and_then(|u| serde_json::from_value(u.clone()).ok());
        emit(ChatEvent::Done { text: content, usage });
        return;
    }
    let reader = std::io::BufReader::new(resp);
    let mut full = String::new();
    let mut usage: Option<Usage> = None;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                emit(ChatEvent::Error(format!("Stream read error: {e}")));
                return;
            }
        };
        let line = line.trim().to_string();
        let data = match line.strip_prefix("data:") {
            Some(d) => d.trim().to_string(),
            None => continue,
        };
        if data == "[DONE]" {
            break;
        }
        let v: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(err) = v.get("error") {
            let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("stream error");
            emit(ChatEvent::Error(msg.to_string()));
            return;
        }
        if let Some(delta) = v.pointer("/choices/0/delta/content").and_then(|c| c.as_str()) {
            if !delta.is_empty() {
                full.push_str(delta);
                emit(ChatEvent::Delta(delta.to_string()));
            }
        }
        if let Some(u) = v.get("usage") {
            if !u.is_null() {
                usage = serde_json::from_value(u.clone()).ok().or(usage);
            }
        }
    }
    emit(ChatEvent::Done { text: full, usage });
}

fn extract_chat_text(v: &Value) -> String {
    if let Some(s) = v.pointer("/choices/0/message/content").and_then(|c| c.as_str()) {
        return s.to_string();
    }
    if let Some(arr) = v.pointer("/choices/0/message/content").and_then(|c| c.as_array()) {
        let mut out = String::new();
        for part in arr {
            if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    if let Some(imgs) = v.pointer("/choices/0/message/images") {
        return format!("(model returned images only) {imgs}");
    }
    "(empty response)".to_string()
}

/// Raw POST used by the CLI probe / self-test.
pub fn post_images_raw(
    client: &reqwest::blocking::Client,
    base: &str,
    key: &str,
    body: &Value,
) -> Result<(reqwest::StatusCode, String), String> {
    let url = format!("{}/images", base.trim_end_matches('/'));
    let resp = auth(client.post(&url), key)
        .json(body)
        .send()
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    Ok((status, text))
}
