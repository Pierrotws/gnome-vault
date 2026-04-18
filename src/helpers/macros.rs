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
