//! Hosting services offering free private repositories, with the steps
//! to create an empty one for a vault. Shown on the wizard's online-copy
//! page to users who have never used git hosting.
//!
//! Codeberg is left out on purpose: it does not offer private hosting.
//! The SSH host keys of these services are accepted without a prior
//! `known_hosts` entry (see `helpers::git::KNOWN_HOST_KEYS`).

#[derive(Debug)]
pub struct Host {
    pub name: &'static str,
    /// One line shown under the service picker.
    pub note: &'static str,
    /// Host names that identify the service in an address (SSH, then web).
    pub hosts: &'static [&'static str],
    pub new_repo_url: &'static str,
    /// Where the user pastes their public SSH key.
    pub ssh_keys_url: &'static str,
    /// How to create an SSH key, for users who have none.
    pub ssh_help_url: &'static str,
    pub address_example: &'static str,
    /// What to pick on the service's "new repository" page.
    pub checklist: &'static [&'static str],
    /// The service only accepts RSA SSH keys.
    pub rsa_only: bool,
}

pub const HOSTS: &[Host] = &[
    Host {
        name: "GitHub",
        note: "Free. The most widely used.",
        hosts: &["github.com"],
        new_repo_url: "https://github.com/new",
        ssh_keys_url: "https://github.com/settings/keys",
        ssh_help_url: "https://docs.github.com/en/authentication/connecting-to-github-with-ssh/generating-a-new-ssh-key-and-adding-it-to-the-ssh-agent",
        address_example: "git@github.com:you/passwords.git",
        checklist: &[
            "Name it, for example “passwords”",
            "Choose Private",
            "Leave README, .gitignore and license off",
            "Create it, then copy its SSH address",
        ],
        rsa_only: false,
    },
    Host {
        name: "GitLab",
        note: "Free.",
        hosts: &["gitlab.com"],
        new_repo_url: "https://gitlab.com/projects/new#blank_project",
        ssh_keys_url: "https://gitlab.com/-/user_settings/ssh_keys",
        ssh_help_url: "https://docs.gitlab.com/user/ssh/",
        address_example: "git@gitlab.com:you/passwords.git",
        checklist: &[
            "Name it, for example “passwords”",
            "Set Visibility to Private",
            "Untick “Initialize repository with a README”",
            "Create it, then copy the address under Code › Clone with SSH",
        ],
        rsa_only: false,
    },
    Host {
        name: "Bitbucket",
        note: "Free for up to 5 users.",
        hosts: &["bitbucket.org"],
        new_repo_url: "https://bitbucket.org/repo/create",
        ssh_keys_url: "https://bitbucket.org/account/settings/ssh-keys/",
        ssh_help_url: "https://support.atlassian.com/bitbucket-cloud/docs/set-up-personal-ssh-keys-on-linux/",
        address_example: "git@bitbucket.org:workspace/passwords.git",
        checklist: &[
            "Name it, for example “passwords”",
            "Keep “Private repository” ticked",
            "Set “Include a README?” to No",
            "Create it, then copy the SSH address shown under Clone",
        ],
        rsa_only: false,
    },
    Host {
        name: "Azure DevOps",
        note: "Free for up to 5 users. Needs an RSA SSH key.",
        hosts: &["ssh.dev.azure.com", "dev.azure.com"],
        new_repo_url: "https://dev.azure.com/",
        ssh_keys_url: "https://learn.microsoft.com/azure/devops/repos/git/use-ssh-keys-to-authenticate",
        ssh_help_url: "https://learn.microsoft.com/azure/devops/repos/git/use-ssh-keys-to-authenticate",
        address_example: "git@ssh.dev.azure.com:v3/org/passwords/passwords",
        checklist: &[
            "Create a private project, for example “passwords”",
            "Open Repos: the project’s repository starts empty",
            "Click Clone, choose SSH, and copy the address",
        ],
        rsa_only: true,
    },
];

/// The service an address points to, if it is one of [`HOSTS`].
pub fn detect(address: &str) -> Option<&'static Host> {
    let host = host_of(address.trim())?;
    HOSTS.iter().find(|candidate| {
        candidate
            .hosts
            .iter()
            .any(|known| host.eq_ignore_ascii_case(known))
    })
}

/// Host name of an `scheme://[user@]host[:port]/path` URL or an
/// scp-like `[user@]host:path` address.
fn host_of(address: &str) -> Option<&str> {
    let authority = match address.split_once("://") {
        Some((_, rest)) => rest.split('/').next()?,
        None => {
            let (before_colon, _) = address.split_once(':')?;
            if before_colon.contains('/') {
                // A local path such as `./a:b`.
                return None;
            }
            before_colon
        }
    };
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_service_of_ssh_and_https_addresses() {
        let name = |address| detect(address).map(|host| host.name);
        assert_eq!(name("git@github.com:you/passwords.git"), Some("GitHub"));
        assert_eq!(
            name(" https://github.com/you/passwords.git "),
            Some("GitHub")
        );
        assert_eq!(
            name("ssh://git@gitlab.com:22/you/passwords.git"),
            Some("GitLab")
        );
        assert_eq!(
            name("git@bitbucket.org:team/passwords.git"),
            Some("Bitbucket")
        );
        assert_eq!(
            name("git@ssh.dev.azure.com:v3/org/project/repo"),
            Some("Azure DevOps")
        );
        assert_eq!(
            name("https://org@dev.azure.com/org/project/_git/repo"),
            Some("Azure DevOps")
        );
        assert_eq!(name("git@github.com.evil.example:you/x.git"), None);
        assert_eq!(name("git@git.example.org:you/passwords.git"), None);
        assert_eq!(name("/media/usb/passwords.git"), None);
        assert_eq!(name(""), None);
    }
}
