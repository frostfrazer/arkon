//! ACME (Let's Encrypt) TLS certificate provisioner.
//!
//! Called by the SSH target after first deploy when `tls = true` in the target config.
//! Uses the `instant-acme` crate to run the ACME HTTP-01 or DNS-01 challenge,
//! obtain a certificate, and write it to the remote server via SSH.
//!
//! Auto-renewal: the daemon schedules a daily check and renews if the cert
//! expires within 30 days. Renewed certs are pushed via rsync + `nginx -s reload`.

use arkon_core::error::{ArkonError, Result};
use instant_acme::{
    Account, AccountCredentials, ChallengeType, Identifier, LetsEncrypt,
    NewAccount, NewOrder, OrderStatus,
};
use std::time::Duration;
use tokio::time;
use tracing::{debug, info, warn};

/// Certificate bundle returned after successful provisioning.
#[derive(Debug, Clone)]
pub struct CertBundle {
    /// PEM-encoded certificate chain.
    pub cert_pem: String,
    /// PEM-encoded private key.
    pub key_pem: String,
    /// Domain the cert was issued for.
    pub domain: String,
    /// Expiry as days remaining (approximate).
    pub expires_in_days: u32,
}

/// Full ACME provisioner.
pub struct AcmeProvisioner {
    domain:      String,
    email:       String,
    /// Path where ACME account credentials are cached.
    creds_path:  std::path::PathBuf,
    /// Whether to use Let's Encrypt staging (for testing).
    staging:     bool,
}

impl AcmeProvisioner {
    pub fn new(domain: impl Into<String>, email: impl Into<String>, staging: bool) -> Self {
        let domain = domain.into();
        let creds_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(".arkon")
            .join("acme")
            .join(format!("{}.json", sanitize(&domain)));

        Self { domain, email: email.into(), creds_path, staging }
    }

    /// Provision a new certificate or return cached credentials.
    pub async fn provision(&self, http_challenge_dir: &str) -> Result<CertBundle> {
        info!(domain = %self.domain, staging = %self.staging, "starting ACME provisioning");

        let directory_url = if self.staging {
            LetsEncrypt::Staging.url().to_string()
        } else {
            LetsEncrypt::Production.url().to_string()
        };

        // Load or create ACME account
        let account = self.get_or_create_account(directory_url).await?;

        // Create order
        let identifiers = vec![Identifier::Dns(self.domain.clone())];
        let mut order = account
            .new_order(&NewOrder { identifiers: &identifiers })
            .await
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("ACME new order: {e}")))?;

        let authorizations = order
            .authorizations()
            .await
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("ACME authorizations: {e}")))?;

        // Complete HTTP-01 challenges
        for authz in &authorizations {
            let challenge = authz
                .challenges
                .iter()
                .find(|c| c.r#type == ChallengeType::Http01)
                .ok_or_else(|| ArkonError::Other(anyhow::anyhow!("HTTP-01 challenge not offered")))?;

            let key_authorization = order.key_authorization(challenge);

            // Write challenge file to HTTP server's well-known dir
            let challenge_path = format!(
                "{http_challenge_dir}/.well-known/acme-challenge/{}", challenge.token
            );
            if let Some(parent) = std::path::Path::new(&challenge_path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&challenge_path, key_authorization.as_str())?;
            info!(path = %challenge_path, "HTTP-01 challenge file written");

            // Notify ACME server we're ready
            order.set_challenge_ready(&challenge.url)
                .await
                .map_err(|e| ArkonError::Other(anyhow::anyhow!("set challenge ready: {e}")))?;
        }

        // Poll for order readiness (max 60s)
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            time::sleep(Duration::from_secs(2)).await;
            let state = order.refresh().await
                .map_err(|e| ArkonError::Other(anyhow::anyhow!("order refresh: {e}")))?;

            match state.status {
                OrderStatus::Ready => break,
                OrderStatus::Invalid => {
                    return Err(ArkonError::Other(anyhow::anyhow!(
                        "ACME challenge validation failed for {}", self.domain
                    )));
                }
                _ => {
                    if std::time::Instant::now() > deadline {
                        return Err(ArkonError::Other(anyhow::anyhow!("ACME challenge timed out")));
                    }
                    debug!(status = ?state.status, "waiting for challenge validation");
                }
            }
        }

        // Generate key pair and CSR
        let mut key_params = rcgen::CertificateParams::new(vec![self.domain.clone()]);
        key_params.distinguished_name = rcgen::DistinguishedName::new();
        let key_pair = rcgen::KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("key gen: {e}")))?;
        key_params.key_pair = Some(key_pair);
        let cert = rcgen::Certificate::from_params(key_params)
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("cert params: {e}")))?;
        let csr_der = cert.serialize_request_der()
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("CSR serialize: {e}")))?;
        let key_pem = cert.get_key_pair().serialize_pem();

        // Finalize order
        order.finalize(&csr_der)
            .await
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("finalize: {e}")))?;

        // Download certificate
        let cert_chain_pem = loop {
            time::sleep(Duration::from_secs(1)).await;
            match order.certificate().await {
                Ok(Some(cert)) => break cert,
                Ok(None) => {
                    debug!("certificate not ready yet, retrying");
                }
                Err(e) => return Err(ArkonError::Other(anyhow::anyhow!("cert download: {e}"))),
            }
        };

        info!(domain = %self.domain, "certificate issued by Let's Encrypt");

        Ok(CertBundle {
            cert_pem: cert_chain_pem,
            key_pem,
            domain: self.domain.clone(),
            expires_in_days: 90, // LE certs are always 90 days
        })
    }

    async fn get_or_create_account(
        &self,
        directory_url: String,
    ) -> Result<Account> {
        if self.creds_path.exists() {
            let raw = std::fs::read_to_string(&self.creds_path)?;
            let creds: AccountCredentials = serde_json::from_str(&raw)
                .map_err(|e| ArkonError::Other(e.into()))?;
            let account = Account::from_credentials(creds)
                .await
                .map_err(|e| ArkonError::Other(anyhow::anyhow!("load ACME account: {e}")))?;
            info!("loaded existing ACME account");
            return Ok(account);
        }

        let new_account = NewAccount {
            contact:              &[&format!("mailto:{}", self.email)],
            terms_of_service_agreed: true,
            only_return_existing:    false,
        };

        let (account, credentials) = Account::create(&new_account, &directory_url, None)
            .await
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("create ACME account: {e}")))?;

        // Persist credentials
        if let Some(parent) = self.creds_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let creds_json = serde_json::to_string(&credentials)
            .map_err(|e| ArkonError::Other(e.into()))?;
        std::fs::write(&self.creds_path, creds_json)?;
        info!(path = %self.creds_path.display(), "ACME account credentials saved");

        Ok(account)
    }

    /// Renewal scheduler: checks daily, renews if cert expires within 30 days.
    /// Spawn with `tokio::spawn`.
    pub async fn run_renewal_scheduler(
        domain:              String,
        email:               String,
        http_challenge_dir:  String,
        staging:             bool,
        on_renewed:          impl Fn(CertBundle) + Send + 'static,
    ) {
        let mut interval = time::interval(Duration::from_secs(86_400));
        interval.tick().await; // skip first immediate tick

        loop {
            interval.tick().await;
            let provisioner = AcmeProvisioner::new(&domain, &email, staging);
            match provisioner.needs_renewal(&domain).await {
                Ok(true) => {
                    info!(domain = %domain, "certificate nearing expiry — renewing");
                    match provisioner.provision(&http_challenge_dir).await {
                        Ok(bundle) => {
                            info!(domain = %domain, "certificate renewed");
                            on_renewed(bundle);
                        }
                        Err(e) => warn!(domain = %domain, error = %e, "renewal failed"),
                    }
                }
                Ok(false) => debug!(domain = %domain, "certificate still valid, no renewal needed"),
                Err(e)    => warn!(domain = %domain, error = %e, "renewal check failed"),
            }
        }
    }

    /// Check if the cert for a domain expires within 30 days.
    /// Reads the locally cached PEM cert and parses the x509 NotAfter field directly.
    async fn needs_renewal(&self, domain: &str) -> Result<bool> {
        use x509_parser::prelude::*;

        let cert_cache = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(".arkon")
            .join("certs")
            .join(format!("{}.pem", sanitize(domain)));

        if !cert_cache.exists() {
            return Ok(true); // No cert on disk — provision needed
        }

        let pem_data = std::fs::read(&cert_cache)?;

        // Parse PEM → DER → x509
        let pem_decoded = ::pem::parse(&pem_data)
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("PEM parse: {e}")))?;
        let (_, cert) = X509Certificate::from_der(pem_decoded.contents())
            .map_err(|e| ArkonError::Other(anyhow::anyhow!("x509 parse: {e}")))?;

        let not_after = cert.validity().not_after.timestamp();
        let now       = chrono::Utc::now().timestamp();
        let days_left = (not_after - now) / 86_400;

        let needs = days_left < 30;
        if needs {
            info!(domain = %domain, days_left = %days_left, "certificate renewal required");
        } else {
            debug!(domain = %domain, days_left = %days_left, "certificate valid");
        }
        Ok(needs)
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}
