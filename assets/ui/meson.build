
subdir('fields')

entry_view_ui = custom_target(
  'entry-view-ui',
  input: 'entry_view.blp',
  output: 'entry_view.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

generate_password_view_ui = custom_target(
  'generate-password-view-ui',
  input: 'generate_password_view.blp',
  output: 'generate_password_view.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

vault_view_ui = custom_target(
  'vault-view-ui',
  input: 'vault_view.blp',
  output: 'vault_view.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

changes_view_ui = custom_target(
  'changes-view-ui',
  input: 'changes_view.blp',
  output: 'changes_view.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

preferences_dialog_ui = custom_target(
  'preferences-dialog-ui',
  input: 'preferences_dialog.blp',
  output: 'preferences_dialog.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)

window_ui = custom_target(
  'window-ui',
  input: 'window.blp',
  output: 'window.ui',
  command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
)
