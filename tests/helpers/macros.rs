#[path = "../../src/helpers/macros.rs"]
mod source_macros;

use source_macros::concat_bytes;

#[test]
fn concatenates_two_byte_slices() {
    const OUT: [u8; 5] = concat_bytes!(b"ab", b"cde");

    assert_eq!(OUT, *b"abcde");
}

#[test]
fn concatenates_more_than_two_byte_slices() {
    const OUT: [u8; 6] = concat_bytes!(b"ab", b"cd", b"ef");

    assert_eq!(OUT, *b"abcdef");
}

#[test]
fn returns_single_byte_slice_unchanged() {
    const OUT: &[u8; 3] = concat_bytes!(b"abc");

    assert_eq!(OUT, b"abc");
}
