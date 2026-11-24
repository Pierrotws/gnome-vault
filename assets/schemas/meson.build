schema_files = files('io.pierrotws.Passel.gschema.xml')

install_data(
  schema_files,
  install_dir: datadir / 'glib-2.0' / 'schemas',
)

gnome.compile_schemas(depend_files: schema_files)
