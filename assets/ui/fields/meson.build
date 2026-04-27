if use_blueprint
  plain_field_row_ui = custom_target(
    'plain-field-row-ui',
    input: 'plain_field_row.blp',
    output: 'plain_field_row.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )

  password_field_row_ui = custom_target(
    'password-field-row-ui',
    input: 'password_field_row.blp',
    output: 'password_field_row.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )

  multiline_field_row_ui = custom_target(
    'multiline-field-row-ui',
    input: 'multiline_field_row.blp',
    output: 'multiline_field_row.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )

  array_field_row_ui = custom_target(
    'array-field-row-ui',
    input: 'array_field_row.blp',
    output: 'array_field_row.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )

  otp_field_row_ui = custom_target(
    'otp-field-row-ui',
    input: 'otp_field_row.blp',
    output: 'otp_field_row.ui',
    command: [blueprint, 'compile', '--output', '@OUTPUT@', '@INPUT@'],
  )
else
  plain_field_row_ui = configure_file(
    input: 'plain_field_row.ui',
    output: 'plain_field_row.ui',
    copy: true,
  )

  password_field_row_ui = configure_file(
    input: 'password_field_row.ui',
    output: 'password_field_row.ui',
    copy: true,
  )

  multiline_field_row_ui = configure_file(
    input: 'multiline_field_row.ui',
    output: 'multiline_field_row.ui',
    copy: true,
  )

  array_field_row_ui = configure_file(
    input: 'array_field_row.ui',
    output: 'array_field_row.ui',
    copy: true,
  )

  otp_field_row_ui = configure_file(
    input: 'otp_field_row.ui',
    output: 'otp_field_row.ui',
    copy: true,
  )
endif
