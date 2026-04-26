# Gnome Vault

GTK4 + Adwaita app for [pass](https://www.passwordstore.org/).
Written in Rust ❤️

## Uses

- Gtk-rs
- blueprint

## Install

```bash
meson setup build
meson compile -C build
meson install -C build
```

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

## Thanks

zx2c4 for the great standard unix password manager
