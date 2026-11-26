# Privacy policy

Passel is a password manager for Linux, Windows and Android. This policy
covers every version of the app, including the one on Google Play.

**Passel collects no data.** It has no account, no analytics, no
advertising, no crash reporting and no server of its own. Its authors
receive nothing about you or your use of the app.

## What stays on your device

- Your vaults: your entries, encrypted with your OpenPGP key.
- Your OpenPGP key. On Android it is kept in the app's private storage,
  protected by its passphrase.
- The access tokens of your git remotes. On Android they are encrypted
  with a key held by the Android Keystore.

On Android, the app's data is excluded from device backups.

## What leaves your device

Only what you ask for: when you sync a vault, Passel connects to the git
remote you configured (for example GitHub, GitLab or your own server) and
exchanges the vault's encrypted files with it, using the token or SSH
access you gave it. That remote is chosen and run by you or your
provider, under their own privacy policy. Passel connects to nothing
else.

## Permissions

On Android, Passel only asks for network access, to sync with your git
remotes.

## Contact

Questions about this policy: open an issue at
<https://github.com/Pierrotws/passel/issues>.

Changes to this policy are published in this file, with their history in
the repository.
