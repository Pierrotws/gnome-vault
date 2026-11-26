# Privacy policy

Passel is a password manager for Linux, Windows and Android. This policy
covers every version of the app, including the one on Google Play.

**Passel sends nothing to its developers.** It has no account, no
analytics, no advertising, no crash reporting and no server of its own.
Its authors receive nothing about you or your use of the app.

## What stays on your device

- Your vaults: your entries, encrypted with your OpenPGP key.
- Your OpenPGP key. On Android it is kept in the app's private storage,
  protected by its passphrase.
- The access tokens of your git remotes. On Android they are encrypted
  with a key held by the Android Keystore.

On Android, the app's data is excluded from device backups. A vault kept
only on the device sends nothing anywhere.

## What leaves your device

Only when you sync a vault: Passel then connects to the git remote you
configured (for example GitHub, GitLab or your own server) and exchanges
the vault with it, using the token or SSH access you gave it. Passel
connects to nothing else.

What that remote receives:

- The contents of your entries (passwords, notes, fields), encrypted
  with your OpenPGP key: the remote cannot read them.
- The names of your entries and folders (for example
  `github.com/alice`), which are **not encrypted**: like `pass`, Passel
  stores them as file and folder names, so the remote's operator can
  read them.

On Android, sync only uses HTTPS, so all of this is encrypted in
transit. That remote is chosen and run by you or your provider, under
their own privacy policy.

## Permissions

On Android, Passel only asks for network access, to sync with your git
remotes.

## Contact

Questions about this policy: open an issue at
<https://github.com/Pierrotws/passel/issues>.

Changes to this policy are published in this file, with their history in
the repository.
