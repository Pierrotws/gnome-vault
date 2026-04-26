schema_files = files('io.pierrotws.GnomeVault.gschema.xml')

install_data(
  schema_files,
  install_dir: datadir / 'glib-2.0' / 'schemas',
)

gnome.compile_schemas()
