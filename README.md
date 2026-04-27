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

Optional for UI development:
- blueprint-compiler

Use rustup if you require multiples rust env (similarly to other tools like pyenv, nvm, rbenv, …)

`blueprint-compiler` is only required when editing `.blp` UI files. Release tarballs ship generated `.ui` files, so they can be built without it.

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
meson setup build --prefix="$HOME/.local"
meson compile -C build
meson install -C build
```

To force a build that does not use `blueprint-compiler`:

```bash
meson setup build --prefix="$HOME/.local" -Dblueprint=disabled
meson compile -C build
meson install -C build
```

## Dev

Run only once!

```bash
meson setup build
```

Install `blueprint-compiler` for development so changes to `assets/ui/**/*.blp` are compiled into the GTK templates.

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
