blueprint = find_program('blueprint-compiler')

entry_view_ui = custom_target(
  'entry-view-ui',
  input: 'blp/entry_view.blp',
  output: 'entry_view.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

generate_password_view_ui = custom_target(
  'generate-password-view-ui',
  input: 'blp/generate_password_view.blp',
  output: 'generate_password_view.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

window_ui = custom_target(
  'window-ui',
  input: 'blp/window.blp',
  output: 'window.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

resources = gnome.compile_resources(
  'resources',
  'resources.gresource.xml',
  gresource_bundle: true,
  source_dir: meson.current_build_dir(),
  dependencies: [entry_view_ui, generate_password_view_ui, window_ui],
  install: true,
  install_dir: pkgdatadir,
)
