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
  dependencies: [plain_field_row_ui, password_field_row_ui, multiline_field_row_ui, otp_field_row_ui, entry_view_ui, generate_password_view_ui, vault_view_ui, group_view_ui, changes_view_ui, preferences_dialog_ui, new_entry_dialog_ui, setup_wizard_ui, window_ui],
  install: true,
  install_dir: pkgdatadir,
)

# Launcher entry and icons (assets/icons/make-icons.py makes the icons
# from logo.svg). The file names match the application id, so the
# desktop shell ties the running window to them. Windows has no
# freedesktop launcher: the installer makes the Start menu shortcut
# (packaging/windows).
if host_machine.system() != 'windows'
  i18n.merge_file(
    input: 'io.pierrotws.Passel.desktop.in',
    output: 'io.pierrotws.Passel.desktop',
    type: 'desktop',
    po_dir: meson.project_source_root() / 'po',
    install: true,
    install_dir: datadir / 'applications',
  )
endif
foreach size : [16, 24, 32, 48, 64, 128, 256, 512]
  icon_dir = 'icons' / 'hicolor' / '@0@x@0@'.format(size) / 'apps'
  install_data(
    icon_dir / 'io.pierrotws.Passel.png',
    install_dir: datadir / icon_dir,
  )
endforeach
