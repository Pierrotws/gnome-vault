project(
  'passel',
  'rust',
  version: '0.2.0', # x-release-please-version
  meson_version: '>= 1.4'
)

gnome = import('gnome')

base_id = 'io.pierrotws.Passel'
application_id = base_id

bindir = get_option('prefix') / get_option('bindir')
datadir = get_option('prefix') / get_option('datadir')
pkgdatadir = datadir / meson.project_name()

subdir('assets')
subdir('src')

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
