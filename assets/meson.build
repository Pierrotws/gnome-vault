blueprint_option = get_option('blueprint')
blueprint = find_program('blueprint-compiler', required: blueprint_option.enabled())
use_blueprint = blueprint.found() and not blueprint_option.disabled()

subdir('schemas')
subdir('ui')

resources = gnome.compile_resources(
  'resources',
  'resources.gresource.xml',
  gresource_bundle: true,
  source_dir: meson.current_build_dir(),
  dependencies: [plain_field_row_ui, password_field_row_ui, multiline_field_row_ui, array_field_row_ui, otp_field_row_ui, entry_view_ui, generate_password_view_ui, vault_view_ui, group_view_ui, changes_view_ui, preferences_dialog_ui, new_entry_dialog_ui, window_ui],
  install: true,
  install_dir: pkgdatadir,
)
