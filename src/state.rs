//! Application state and persisted configuration.
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub user: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub last_model: String,
    #[serde(default)]
    pub understand_model: String,
    #[serde(default)]
    pub output_dir: String,
}

fn default_timeout() -> u64 {
    600
}

impl Default for Config {
    fn default() -> Self {
        Config {
            api_key: String::new(),
            base_url: crate::api::DEFAULT_BASE.to_string(),
            user: String::new(),
            timeout_secs: default_timeout(),
            last_model: String::new(),
            understand_model: "google/gemini-2.5-flash".to_string(),
            output_dir: crate::util::default_output_dir().to_string_lossy().to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Config {
        let path = Self::path();
        let mut cfg = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Config>(&s).ok())
            .unwrap_or_default();
        if cfg.base_url.trim().is_empty() {
            cfg.base_url = crate::api::DEFAULT_BASE.to_string();
        }
        if cfg.timeout_secs == 0 {
            cfg.timeout_secs = default_timeout();
        }
        if let Some(env_key) = crate::util::env_api_key() {
            cfg.api_key = env_key;
        }
        cfg
    }

    pub fn path() -> std::path::PathBuf {
        crate::util::config_dir().join("config.json")
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir failed: {e}"))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| format!("write failed: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}

pub struct AppState {
    pub config: RefCell<Config>,
    pub client: RefCell<reqwest::blocking::Client>,
}

impl AppState {
    pub fn new() -> AppState {
        let config = Config::load();
        let client = crate::api::make_client(config.timeout_secs)
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        AppState {
            config: RefCell::new(config),
            client: RefCell::new(client),
        }
    }

    pub fn api_key(&self) -> String {
        self.config.borrow().api_key.clone()
    }

    pub fn base_url(&self) -> String {
        self.config.borrow().base_url.clone()
    }

    pub fn has_key(&self) -> bool {
        !self.config.borrow().api_key.trim().is_empty()
    }

    pub fn client(&self) -> reqwest::blocking::Client {
        self.client.borrow().clone()
    }

    pub fn set_key(&self, key: &str) {
        self.config.borrow_mut().api_key = key.trim().to_string();
    }

    pub fn rebuild_client(&self) {
        let t = self.config.borrow().timeout_secs;
        if let Ok(c) = crate::api::make_client(t) {
            *self.client.borrow_mut() = c;
        }
    }

    pub fn save(&self) {
        let cfg = self.config.borrow().clone();
        if let Err(e) = cfg.save() {
            eprintln!("warning: could not save config: {e}");
        }
    }

    pub fn user(&self) -> Option<String> {
        let u = self.config.borrow().user.clone();
        if u.trim().is_empty() {
            None
        } else {
            Some(u)
        }
    }
}
