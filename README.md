# Gnome Vault

GTK4 + Adwaita app for [pass](https://www.passwordstore.org/).
Written in Rust ❤️

Uses:
- Adwaita
- Gtk-rs
- blueprint

## Requirements (build or dev)

- rust or rustup
- meson

Use rustup if you require multiples rust env (similarly to other tools like pyenv, nvm, rbenv, …)

## Usage

- User must have GPG Key in their wallet.
  Use Gnome `Seahorse` to create one if none appears.
- User should install `pass` cli tool before,
  although not required.

If user want to sync, it requires a private git remote project, with working git access (git-credential configured, or ssh access).
Project will default on `~/.password-store`, yet configurable with env var `PASSWORD_STORE_DIR`.

Consider using `pass` cli for a guaranteed functional git env

## Install

```bash
meson setup build
meson compile -C build
meson install -C build
```

meson install not tested.

## Dev

Run only once!

```bash
meson setup build
```

### Compile

```bash
cd build;
meson compile
```

### Run

Dev-only for now:
```bash
cd build;
./target/debug/gnome-vault
```

### Test

```bash
cd build
cargo test
```

### Doc

```bash
cd build
cargo doc
```


## Thanks

- zx2c4 for the great standard unix password
  manager
- The Gnome Project ❤️
