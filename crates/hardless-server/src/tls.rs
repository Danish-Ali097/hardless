//! HTTPS on port 8443 (FR-22, FR-33, NFR-05): a local certificate authority, made on first
//! run and kept in the user's data folder, signs a fresh server certificate at each start.
//!
//! Trusting the CA once on a phone or tablet is enough: new server certificates (a new LAN
//! address, a renewal) are trusted automatically. iOS only offers full trust for a CA, which
//! is why there are two certificates rather than one self-signed server certificate. The CA
//! key never leaves this machine and is never committed or shipped in an image.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

/// The CA certificate file in the certificate folder.
pub const CA_CERT_FILE: &str = "hardless-ca.pem";
/// The CA private key file in the certificate folder.
pub const CA_KEY_FILE: &str = "hardless-ca-key.pem";
/// Where the dashboard and devices download the CA certificate.
pub const CA_DOWNLOAD_PATH: &str = "/hardless-ca.crt";

const CA_NAME: &str = "Hardless Local CA";
/// Apple accepts server certificates valid for at most 398 days.
const SERVER_CERT_DAYS: i64 = 397;
const CA_DAYS: i64 = 10 * 365;

/// The default certificate folder: the OS's per-user data folder, so one CA serves every
/// project on this machine.
pub fn default_cert_dir(env: &dyn Fn(&str) -> Option<String>) -> PathBuf {
    let base = if cfg!(windows) {
        env("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        env("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map_or_else(
        || PathBuf::from(".hardless").join("certs"),
        |b| b.join("hardless").join("certs"),
    )
}

/// HTTPS, ready to serve.
#[derive(Clone)]
pub struct Tls {
    /// The rustls configuration with the server certificate.
    pub config: Arc<rustls::ServerConfig>,
    /// The CA certificate (PEM), for devices to trust.
    pub ca_pem: String,
    /// The names and addresses the server certificate covers.
    pub hosts: Vec<String>,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tls")
            .field("hosts", &self.hosts)
            .finish_non_exhaustive()
    }
}

/// Why HTTPS could not be set up.
#[derive(Debug)]
pub struct TlsError(pub String);

impl std::fmt::Display for TlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TlsError {}

fn err(context: &str) -> impl Fn(rcgen::Error) -> TlsError + '_ {
    move |e| TlsError(format!("{context}: {e}"))
}

/// The names the server certificate should cover: this machine, the address the server is
/// bound to, its LAN address when bound to all interfaces, and any `extra` names.
pub fn certificate_hosts(bind: IpAddr, extra: &[String]) -> Vec<String> {
    let mut hosts = vec![
        "localhost".to_owned(),
        "127.0.0.1".to_owned(),
        "::1".to_owned(),
    ];
    if bind.is_unspecified() {
        if let Some(lan) = lan_address() {
            hosts.push(lan.to_string());
        }
    } else if !bind.is_loopback() {
        hosts.push(bind.to_string());
    }
    for host in extra {
        let host = host.trim();
        if !host.is_empty() && !hosts.iter().any(|h| h == host) {
            hosts.push(host.to_owned());
        }
    }
    hosts
}

/// The LAN address of the default route. Connecting a UDP socket sends nothing (NFR-04); it
/// only asks the OS which local address it would use. 192.0.2.1 is a documentation address
/// that is never routed.
fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

fn ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, CA_NAME);
    name.push(DnType::OrganizationName, "Hardless (development only)");
    params.distinguished_name = name;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
}

/// Loads the CA from `dir`, creating it on first run.
fn load_or_create_ca(dir: &Path) -> Result<(String, KeyPair), TlsError> {
    let cert_path = dir.join(CA_CERT_FILE);
    let key_path = dir.join(CA_KEY_FILE);
    if let (Ok(cert), Ok(key)) = (
        fs::read_to_string(&cert_path),
        fs::read_to_string(&key_path),
    ) {
        if let Ok(key) = KeyPair::from_pem(&key) {
            return Ok((cert, key));
        }
    }
    fs::create_dir_all(dir)
        .map_err(|e| TlsError(format!("cannot create {}: {e}", dir.display())))?;
    let key = KeyPair::generate().map_err(err("cannot make the CA key"))?;
    let mut params = ca_params();
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(CA_DAYS);
    let cert = params
        .self_signed(&key)
        .map_err(err("cannot make the CA certificate"))?;
    write_private(&key_path, &key.serialize_pem())?;
    fs::write(&cert_path, cert.pem())
        .map_err(|e| TlsError(format!("cannot write {}: {e}", cert_path.display())))?;
    Ok((cert.pem(), key))
}

/// Writes a private key readable only by the current user (on Unix).
fn write_private(path: &Path, contents: &str) -> Result<(), TlsError> {
    fs::write(path, contents)
        .map_err(|e| TlsError(format!("cannot write {}: {e}", path.display())))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| TlsError(format!("cannot protect {}: {e}", path.display())))?;
    }
    Ok(())
}

/// Sets up HTTPS: the CA from `dir` (made on first run) and a server certificate for
/// `hosts`, valid for 397 days from now.
pub fn setup(dir: &Path, hosts: Vec<String>) -> Result<Tls, TlsError> {
    let (ca_pem, ca_key) = load_or_create_ca(dir)?;
    // The issuer only needs the CA's name and key; the leaf names the CA by both.
    let issuer = Issuer::new(ca_params(), ca_key);

    let key = KeyPair::generate().map_err(err("cannot make the server key"))?;
    let mut params =
        CertificateParams::new(hosts.clone()).map_err(err("invalid certificate host"))?;
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "Hardless");
    params.distinguished_name = name;
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    params.use_authority_key_identifier_extension = true;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(SERVER_CERT_DAYS);
    let cert = params
        .signed_by(&key, &issuer)
        .map_err(err("cannot sign the server certificate"))?;

    let ca_der = rustls::pki_types::pem::PemObject::from_pem_slice(ca_pem.as_bytes())
        .map_err(|e: rustls::pki_types::pem::Error| TlsError(format!("bad CA certificate: {e}")))?;
    let chain: Vec<CertificateDer<'static>> = vec![cert.der().clone(), ca_der];
    let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| TlsError(format!("TLS setup failed: {e}")))?
        .with_no_client_auth()
        .with_single_cert(chain, private)
        .map_err(|e| TlsError(format!("TLS setup failed: {e}")))?;
    // HTTP/1.1 only, so WebSocket upgrades (the bridge over wss://) work as on port 8080.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Tls {
        config: Arc::new(config),
        ca_pem,
        hosts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dir_is_per_user() {
        let env = |k: &str| match k {
            "APPDATA" => Some("C:\\Users\\a\\AppData\\Roaming".to_owned()),
            "HOME" => Some("/home/a".to_owned()),
            _ => None,
        };
        let dir = default_cert_dir(&env);
        assert!(
            dir.ends_with(Path::new("hardless").join("certs")),
            "{dir:?}"
        );
        let none = default_cert_dir(&|_| None);
        assert_eq!(none, PathBuf::from(".hardless").join("certs"));
    }

    #[test]
    fn hosts_cover_loopback_the_bound_address_and_extras() {
        let hosts = certificate_hosts("192.168.1.20".parse().unwrap(), &["pos.local".into()]);
        assert_eq!(
            hosts,
            ["localhost", "127.0.0.1", "::1", "192.168.1.20", "pos.local"]
        );
        let local = certificate_hosts(IpAddr::V4(Ipv4Addr::LOCALHOST), &[]);
        assert_eq!(local, ["localhost", "127.0.0.1", "::1"]);
    }
}
