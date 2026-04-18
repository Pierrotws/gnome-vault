cargo = find_program('cargo')

cargo_options = [
  '--manifest-path', meson.project_source_root() / 'Cargo.toml',
  '--target-dir', meson.project_build_root() / 'target',
]

if get_option('buildtype') == 'release'
  cargo_options += ['--release']
endif

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
    cargo,
    'build',
    cargo_options,
  ],
)
