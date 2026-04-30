/// Compile-time concatenation of byte slice constants into a `[u8; N]`.
///
/// The macro recurses through its argument list pairwise: a call with
/// `n` slices expands to `n - 1` nested invocations. Rust's default
/// `recursion_limit` is 128, so this comfortably handles any realistic
/// charset combination (this crate currently uses up to 4 slices per
/// call). A larger composition would need `#![recursion_limit]` bumped.
macro_rules! concat_bytes {
    ($a:expr) => { $a };

    ($a:expr, $b:expr $(, $rest:expr)+) => {
        concat_bytes!(
            concat_bytes!($a, $b)
            $(, $rest)+
        )
    };

    ($a:expr, $b:expr) => {{
        const OUT: [u8; $a.len() + $b.len()] = {
            let mut out = [0u8; $a.len() + $b.len()];

            let mut i = 0;
            while i < $a.len() {
                out[i] = $a[i];
                i += 1;
            }

            let mut j = 0;
            while j < $b.len() {
                out[$a.len() + j] = $b[j];
                j += 1;
            }

            out
        };
        OUT
    }};
}

pub(crate) use concat_bytes;

#[cfg(test)]
mod tests {
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
}
