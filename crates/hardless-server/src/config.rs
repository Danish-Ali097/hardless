//! Server configuration (FR-21): `hardless.config.json`, then environment variables, then CLI
//! flags, each overriding the one before.

use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use hardless_core::DeviceId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The config file read from the working directory when no `--config` is given.
pub const DEFAULT_CONFIG_FILE: &str = "hardless.config.json";

/// A device to create at start-up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceSpec {
    /// Device kind, e.g. `printer-escpos`.
    pub kind: String,
    /// Device ID; generated if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<DeviceId>,
    /// Device config.
    #[serde(default)]
    pub config: Value,
}

/// The contents of `hardless.config.json`. Every field is optional.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigFile {
    /// Address to bind.
    pub host: Option<IpAddr>,
    /// HTTP port.
    pub port: Option<u16>,
    /// HTTPS port.
    pub https_port: Option<u16>,
    /// Serve HTTPS (default on).
    pub https: Option<bool>,
    /// Folder for the local certificate authority (default: the per-user data folder).
    pub cert_dir: Option<PathBuf>,
    /// Extra host names or addresses for the HTTPS certificate.
    pub cert_hosts: Option<Vec<String>>,
    /// Bearer token required by the API and bridge.
    pub token: Option<String>,
    /// Devices to create at start-up. Omitted: one printer and one drawer.
    pub devices: Option<Vec<DeviceSpec>>,
}

/// Values from CLI flags; `None` means the flag was not given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// `--config`
    pub config: Option<PathBuf>,
    /// `--host`
    pub host: Option<IpAddr>,
    /// `--port`
    pub port: Option<u16>,
    /// `--https-port`
    pub https_port: Option<u16>,
    /// `--token`
    pub token: Option<String>,
}

/// The resolved configuration.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    /// Address to bind (default `127.0.0.1`, NFR-01).
    pub host: IpAddr,
    /// HTTP port for the dashboard, API and bridge (default 8080, FR-22).
    pub port: u16,
    /// HTTPS port (default 8443, FR-22).
    pub https_port: u16,
    /// Serve HTTPS on `https_port` (default on; FR-33).
    pub https: bool,
    /// Folder for the local certificate authority; `None` is the per-user data folder
    /// (see [`crate::tls::default_cert_dir`]).
    pub cert_dir: Option<PathBuf>,
    /// Extra host names or addresses for the HTTPS certificate, e.g. a LAN host name.
    pub cert_hosts: Vec<String>,
    /// Bearer token (NFR-02).
    pub token: Option<String>,
    /// Devices to create at start-up.
    pub devices: Vec<DeviceSpec>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8080,
            https_port: 8443,
            https: true,
            cert_dir: None,
            cert_hosts: Vec::new(),
            token: None,
            devices: default_devices(),
        }
    }
}

/// `printer-1` (ESC/POS over Web Serial) with `drawer-1` attached on pin 2, `scanner-1`
/// (keyboard wedge), `local_printer` (HTTP, the ePOS-Print default device ID) and `nfc-1`
/// (NFC reader): enough to print a first receipt by either route, scan and tap without
/// writing any config.
pub fn default_devices() -> Vec<DeviceSpec> {
    let id = |s: &str| s.parse::<DeviceId>().ok();
    vec![
        DeviceSpec {
            kind: "printer-escpos".into(),
            id: id("printer-1"),
            config: json!({}),
        },
        DeviceSpec {
            kind: "cash-drawer".into(),
            id: id("drawer-1"),
            config: json!({ "attachedTo": "printer-1" }),
        },
        DeviceSpec {
            kind: "scanner-wedge".into(),
            id: id("scanner-1"),
            config: json!({}),
        },
        DeviceSpec {
            kind: "printer-escpos".into(),
            id: id("local_printer"),
            config: json!({ "transport": "http" }),
        },
        DeviceSpec {
            kind: "nfc-reader".into(),
            id: id("nfc-1"),
            config: json!({}),
        },
    ]
}

/// Why the configuration could not be loaded.
#[derive(Debug)]
pub struct LoadError(pub String);

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LoadError {}

impl ServerConfig {
    /// Resolves the configuration: defaults, then the config file (`--config`,
    /// `HARDLESS_CONFIG`, or `hardless.config.json` if it exists), then `HARDLESS_HOST`,
    /// `HARDLESS_PORT`, `HARDLESS_HTTPS_PORT`, `HARDLESS_HTTPS`, `HARDLESS_CERT_DIR`,
    /// `HARDLESS_CERT_HOSTS` and `HARDLESS_TOKEN`, then CLI flags.
    ///
    /// `env` looks up an environment variable; pass `|k| std::env::var(k).ok()`.
    ///
    /// ```
    /// use hardless_server::config::{Overrides, ServerConfig};
    /// let env = |k: &str| (k == "HARDLESS_PORT").then(|| "9000".to_owned());
    /// let flags = Overrides { port: Some(9100), ..Default::default() };
    /// assert_eq!(ServerConfig::load(&env, &Overrides::default()).unwrap().port, 9000);
    /// assert_eq!(ServerConfig::load(&env, &flags).unwrap().port, 9100);
    /// ```
    pub fn load(
        env: &dyn Fn(&str) -> Option<String>,
        flags: &Overrides,
    ) -> Result<Self, LoadError> {
        let mut config = ServerConfig::default();

        let path = flags
            .config
            .clone()
            .or_else(|| env("HARDLESS_CONFIG").map(PathBuf::from))
            .or_else(|| {
                let default = PathBuf::from(DEFAULT_CONFIG_FILE);
                default.exists().then_some(default)
            });
        if let Some(path) = path {
            config.apply_file(&read_file(&path)?);
        }

        if let Some(v) = env("HARDLESS_HOST") {
            config.host = v
                .parse()
                .map_err(|_| LoadError(format!("HARDLESS_HOST is not an IP address: {v}")))?;
        }
        if let Some(v) = env("HARDLESS_PORT") {
            config.port = v
                .parse()
                .map_err(|_| LoadError(format!("HARDLESS_PORT is not a port: {v}")))?;
        }
        if let Some(v) = env("HARDLESS_HTTPS_PORT") {
            config.https_port = v
                .parse()
                .map_err(|_| LoadError(format!("HARDLESS_HTTPS_PORT is not a port: {v}")))?;
        }
        if let Some(v) = env("HARDLESS_HTTPS") {
            config.https = !matches!(
                v.to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            );
        }
        if let Some(v) = env("HARDLESS_CERT_DIR") {
            config.cert_dir = Some(PathBuf::from(v)).filter(|p| !p.as_os_str().is_empty());
        }
        if let Some(v) = env("HARDLESS_CERT_HOSTS") {
            config.cert_hosts = v
                .split(',')
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .map(str::to_owned)
                .collect();
        }
        if let Some(v) = env("HARDLESS_TOKEN") {
            config.token = Some(v).filter(|t| !t.is_empty());
        }

        if let Some(host) = flags.host {
            config.host = host;
        }
        if let Some(port) = flags.port {
            config.port = port;
        }
        if let Some(port) = flags.https_port {
            config.https_port = port;
        }
        if let Some(token) = &flags.token {
            config.token = Some(token.clone()).filter(|t| !t.is_empty());
        }
        Ok(config)
    }

    fn apply_file(&mut self, file: &ConfigFile) {
        if let Some(host) = file.host {
            self.host = host;
        }
        if let Some(port) = file.port {
            self.port = port;
        }
        if let Some(port) = file.https_port {
            self.https_port = port;
        }
        if let Some(https) = file.https {
            self.https = https;
        }
        if let Some(dir) = &file.cert_dir {
            self.cert_dir = Some(dir.clone());
        }
        if let Some(hosts) = &file.cert_hosts {
            self.cert_hosts = hosts.clone();
        }
        if let Some(token) = &file.token {
            self.token = Some(token.clone()).filter(|t| !t.is_empty());
        }
        if let Some(devices) = &file.devices {
            self.devices = devices.clone();
        }
    }

    /// Whether the server is reachable only from this machine.
    pub fn is_loopback(&self) -> bool {
        self.host.is_loopback()
    }
}

fn read_file(path: &Path) -> Result<ConfigFile, LoadError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| LoadError(format!("cannot read {}: {e}", path.display())))?;
    serde_json::from_str(&text).map_err(|e| LoadError(format!("invalid {}: {e}", path.display())))
}
