cargo = find_program('cargo')
sh = find_program('sh')

cargo_manifest = meson.project_source_root() / 'Cargo.toml'
cargo_target_dir = meson.project_build_root() / 'target'
# Treat every meson buildtype except 'debug' as a release-profile cargo
# build. arch-meson defaults to 'plain', which used to drop us into
# cargo's debug profile and ship an unoptimized binary in distro
# packages.
cargo_profile = 'debug'
cargo_release_arg = ''
if get_option('buildtype') != 'debug'
  cargo_profile = 'release'
  cargo_release_arg = '--release'
endif

cargo_binary = cargo_target_dir / cargo_profile / meson.project_name()

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depends: resources,
  env: {
    'APP_ID': application_id,
    'RESOURCES_FILE': pkgdatadir / 'resources.gresource',
  },
  command: [
    sh,
    '-c',
    'set -eu; if [ -n "$4" ]; then "$1" build --manifest-path "$2" --target-dir "$3" "$4"; else "$1" build --manifest-path "$2" --target-dir "$3"; fi; cp "$5" "$6"',
    'cargo-build',
    cargo,
    cargo_manifest,
    cargo_target_dir,
    cargo_release_arg,
    cargo_binary,
    '@OUTPUT@',
  ],
  install_mode: 'rwxr-xr-x',
)
