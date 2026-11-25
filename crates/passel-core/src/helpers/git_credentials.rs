//! Credentials for git remotes.
//!
//! libgit2 asks for credentials whenever a remote wants them (push, the
//! setup wizard's remote check). What to answer depends on the platform:
//!
//! - The desktop answers from the user's own setup, like `git` in a
//!   terminal: the running ssh-agent, then git's configured credential
//!   helpers. That is [`SystemCredentials`], the default.
//! - A phone has neither, so the app keeps credentials itself (an HTTPS
//!   personal access token, or an SSH key generated on the device, sealed
//!   in the Android Keystore or iOS Keychain) and hands them over through
//!   [`StoredCredentials`], installed with [`install_credential_provider`].

use std::{
    collections::HashMap,
    sync::{OnceLock, RwLock},
};

use zeroize::Zeroizing;

/// One way to authenticate to a remote.
#[derive(Clone)]
pub enum Credential {
    /// Keys loaded in the running ssh-agent. Falls back to git's
    /// credential helpers when no agent answers.
    SshAgent { username: String },
    /// git's configured credential helpers (`credential.helper`).
    CredentialHelper,
    /// An SSH private key held in memory (OpenSSH or PEM format).
    SshKey {
        username: String,
        private_key: Zeroizing<String>,
        passphrase: Option<Zeroizing<String>>,
    },
    /// HTTPS basic authentication: a username and a password or, for
    /// most hosts, a personal access token.
    UserPassword {
        username: String,
        password: Zeroizing<String>,
    },
}

// Never print key material or tokens, even in debug logs.
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Credential::SshAgent { .. } => "Credential::SshAgent",
            Credential::CredentialHelper => "Credential::CredentialHelper",
            Credential::SshKey { .. } => "Credential::SshKey(<redacted>)",
            Credential::UserPassword { .. } => "Credential::UserPassword(<redacted>)",
        })
    }
}

/// What libgit2 is asking for.
#[derive(Debug, Clone, Copy)]
pub struct CredentialRequest<'a> {
    /// The remote's address, as configured.
    pub url: &'a str,
    /// The user in the address (`git` in `git@github.com:me/vault`).
    pub username_from_url: Option<&'a str>,
    /// The remote accepts an SSH key.
    pub ssh_key: bool,
    /// The remote accepts a username and password (HTTPS).
    pub user_password: bool,
    /// 0 for the first request of an operation; higher when the remote
    /// rejected what was given before.
    pub attempt: u32,
}

/// Answers libgit2's credential requests. Called on the thread running
/// the git operation.
pub trait CredentialProvider: Send + Sync {
    /// The credential to try, or `None` to give up (the operation then
    /// fails as access denied).
    fn credential(&self, request: &CredentialRequest<'_>) -> Option<Credential>;
}

/// The desktop's credentials: ssh-agent for SSH remotes, git's credential
/// helpers otherwise. Retried as is: the agent may hold several keys.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCredentials;

impl CredentialProvider for SystemCredentials {
    fn credential(&self, request: &CredentialRequest<'_>) -> Option<Credential> {
        match request.username_from_url {
            Some(username) if request.ssh_key => Some(Credential::SshAgent {
                username: username.to_string(),
            }),
            _ => Some(Credential::CredentialHelper),
        }
    }
}

/// Credentials the app saved, one per remote host (`github.com`,
/// `gitlab.example.org`…). Each is offered once per operation: a rejected
/// credential is not retried, so a revoked token fails fast instead of
/// looping.
#[derive(Default)]
pub struct StoredCredentials {
    by_host: RwLock<HashMap<String, Credential>>,
}

impl StoredCredentials {
    pub fn new() -> Self {
        Self::default()
    }

    /// Saves (or replaces) the credential for `host`.
    pub fn set(&self, host: &str, credential: Credential) {
        self.write().insert(host.to_ascii_lowercase(), credential);
    }

    /// Forgets the credential for `host`.
    pub fn remove(&self, host: &str) {
        self.write().remove(&host.to_ascii_lowercase());
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, Credential>> {
        self.by_host
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CredentialProvider for StoredCredentials {
    fn credential(&self, request: &CredentialRequest<'_>) -> Option<Credential> {
        if request.attempt > 0 {
            return None;
        }
        let host = remote_host(request.url)?;
        let by_host = self
            .by_host
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let credential = by_host.get(&host)?;
        let usable = match credential {
            Credential::SshAgent { .. } | Credential::SshKey { .. } => request.ssh_key,
            Credential::UserPassword { .. } => request.user_password,
            Credential::CredentialHelper => true,
        };
        usable.then(|| credential.clone())
    }
}

static PROVIDER: OnceLock<Box<dyn CredentialProvider>> = OnceLock::new();

/// Sets how git operations authenticate, for the whole process. Call it
/// once at startup, before the first push; it fails (returning the
/// provider) if one is already in use. Without it, [`SystemCredentials`]
/// is used.
pub fn install_credential_provider(
    provider: Box<dyn CredentialProvider>,
) -> Result<(), Box<dyn CredentialProvider>> {
    PROVIDER.set(provider)
}

pub(crate) fn credential_provider() -> &'static dyn CredentialProvider {
    PROVIDER
        .get_or_init(|| Box::new(SystemCredentials))
        .as_ref()
}

/// The lowercase host of a remote address: `https://host/…`,
/// `ssh://user@host:port/…` or scp-like `user@host:path`. `None` for
/// local paths and `file://` addresses.
pub fn remote_host(url: &str) -> Option<String> {
    let authority = if let Some((scheme, rest)) = url.split_once("://") {
        if scheme.eq_ignore_ascii_case("file") {
            return None;
        }
        rest.split('/').next()?
    } else {
        // scp-like syntax: a colon before any slash.
        let (authority, _) = url.split_once(':')?;
        if authority.contains('/') {
            return None;
        }
        authority
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(bracketed) = host.strip_prefix('[') {
        bracketed.split_once(']')?.0
    } else {
        host.split_once(':').map_or(host, |(host, _port)| host)
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(url: &str, ssh_key: bool, attempt: u32) -> CredentialRequest<'_> {
        CredentialRequest {
            url,
            username_from_url: ssh_key.then_some("git"),
            ssh_key,
            user_password: !ssh_key,
            attempt,
        }
    }

    #[test]
    fn finds_the_host_of_remote_addresses() {
        for (url, host) in [
            ("https://github.com/me/vault.git", Some("github.com")),
            (
                "https://me:token@GitLab.example.org:8443/me/vault",
                Some("gitlab.example.org"),
            ),
            (
                "ssh://git@codeberg.org:2222/me/vault.git",
                Some("codeberg.org"),
            ),
            ("git@github.com:me/vault.git", Some("github.com")),
            ("ssh://git@[::1]:22/vault", Some("::1")),
            ("file:///home/me/vault.git", None),
            ("/home/me/vault.git", None),
            ("../vault.git", None),
        ] {
            assert_eq!(remote_host(url).as_deref(), host, "{url}");
        }
    }

    #[test]
    fn system_credentials_use_the_agent_for_ssh_and_helpers_otherwise() {
        let ssh = SystemCredentials.credential(&request("git@github.com:me/v", true, 2));
        assert!(matches!(ssh, Some(Credential::SshAgent { username }) if username == "git"));
        let https = SystemCredentials.credential(&request("https://github.com/me/v", false, 0));
        assert!(matches!(https, Some(Credential::CredentialHelper)));
    }

    #[test]
    fn stored_credentials_match_host_and_kind_once() {
        let stored = StoredCredentials::new();
        stored.set(
            "GitHub.com",
            Credential::UserPassword {
                username: "me".into(),
                password: Zeroizing::new("ghp_token".into()),
            },
        );
        let https = request("https://github.com/me/vault", false, 0);
        assert!(matches!(
            stored.credential(&https),
            Some(Credential::UserPassword { password, .. }) if password.as_str() == "ghp_token"
        ));
        // Rejected once: not offered again.
        assert!(stored
            .credential(&request("https://github.com/me/vault", false, 1))
            .is_none());
        // A token is no answer to an SSH remote, nor to another host.
        assert!(stored
            .credential(&request("git@github.com:me/vault", true, 0))
            .is_none());
        assert!(stored
            .credential(&request("https://gitlab.com/me/vault", false, 0))
            .is_none());

        stored.remove("github.com");
        assert!(stored.credential(&https).is_none());
    }

    #[test]
    fn debug_output_hides_secrets() {
        let credential = Credential::SshKey {
            username: "git".into(),
            private_key: Zeroizing::new("-----BEGIN OPENSSH PRIVATE KEY-----".into()),
            passphrase: Some(Zeroizing::new("hunter2".into())),
        };
        let printed = format!("{credential:?}");
        assert!(!printed.contains("OPENSSH") && !printed.contains("hunter2"));
    }
}
