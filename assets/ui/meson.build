
subdir('fields')

if use_blueprint
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

  group_view_ui = custom_target(
    'group-view-ui',
    input: 'group_view.blp',
    output: 'group_view.ui',
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

  new_entry_dialog_ui = custom_target(
    'new-entry-dialog-ui',
    input: 'new_entry_dialog.blp',
    output: 'new_entry_dialog.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )

  window_ui = custom_target(
    'window-ui',
    input: 'window.blp',
    output: 'window.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )
else
  entry_view_ui = configure_file(
    input: 'entry_view.ui',
    output: 'entry_view.ui',
    copy: true,
  )

  generate_password_view_ui = configure_file(
    input: 'generate_password_view.ui',
    output: 'generate_password_view.ui',
    copy: true,
  )

  vault_view_ui = configure_file(
    input: 'vault_view.ui',
    output: 'vault_view.ui',
    copy: true,
  )

  group_view_ui = configure_file(
    input: 'group_view.ui',
    output: 'group_view.ui',
    copy: true,
  )

  changes_view_ui = configure_file(
    input: 'changes_view.ui',
    output: 'changes_view.ui',
    copy: true,
  )

  preferences_dialog_ui = configure_file(
    input: 'preferences_dialog.ui',
    output: 'preferences_dialog.ui',
    copy: true,
  )

  new_entry_dialog_ui = configure_file(
    input: 'new_entry_dialog.ui',
    output: 'new_entry_dialog.ui',
    copy: true,
  )

  window_ui = configure_file(
    input: 'window.ui',
    output: 'window.ui',
    copy: true,
  )
endif
