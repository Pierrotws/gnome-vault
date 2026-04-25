use gnome_vault::{
    helpers::parser::{format_entry, parse_entry},
    pass::model::{EntryData, EntryField},
};

#[test]
fn parses_plain_array_and_literal_multiline_fields() {
    let entry = parse_entry(
        "secret\nusername: test\nnotes: |\n  Test1\n  test2\n  Multiline\ntags:\n  - elem1\n  - elem2\n",
    )
    .unwrap();

    assert_eq!(entry.password, EntryField::Password("secret".into()));
    assert_eq!(
        entry.fields,
        vec![
            ("username".into(), EntryField::Plain("test".into())),
            (
                "notes".into(),
                EntryField::Multiline("Test1\ntest2\nMultiline".into())
            ),
            (
                "tags".into(),
                EntryField::Array(vec!["elem1".into(), "elem2".into()])
            ),
        ]
    );
}

#[test]
fn parses_folded_multiline_fields() {
    let entry = parse_entry("secret\nnotes: >\n  Test1\n  test2\n  Multiline\n").unwrap();

    assert_eq!(
        entry.fields,
        vec![(
            "notes".into(),
            EntryField::Multiline("Test1 test2 Multiline".into())
        )]
    );
}

#[test]
fn writes_array_and_multiline_fields_as_yaml() {
    let entry = EntryData {
        password: EntryField::Password("secret".into()),
        fields: vec![
            ("username".into(), EntryField::Plain("test".into())),
            (
                "notes".into(),
                EntryField::Multiline("Test1\ntest2\nMultiline".into()),
            ),
            (
                "tags".into(),
                EntryField::Array(vec!["elem1".into(), "elem2".into()]),
            ),
        ],
    };

    assert_eq!(
        format_entry(&entry),
        "secret\nusername: test\nnotes: |\n  Test1\n  test2\n  Multiline\ntags:\n  - elem1\n  - elem2\n"
    );
}
