pub trait FromLe {
    fn from_le_bytes(b: &[u8]) -> Self;
}

macro_rules! impl_from_le {
    ($type:ty, $size:expr) => {
        doc_comment! {
            "Converts unsigned integer bytes to a Rust integer.",
            impl FromLe for $type {
                fn from_le_bytes(byte: &[u8]) -> Self {
                    let mut b: [u8; $size] = Default::default();
                    b.copy_from_slice(&byte[0..$size]);
                    Self::from_le_bytes(b)
                }
            }
        }
    };
}

impl_from_le!(u8, 1);
impl_from_le!(u16, 2);
impl_from_le!(u32, 4);
impl_from_le!(i32, 4);
impl_from_le!(u64, 8);
impl_from_le!(i64, 8);
impl_from_le!(f32, 4);
impl_from_le!(f64, 8);
