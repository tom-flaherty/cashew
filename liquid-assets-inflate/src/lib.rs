#![no_std]

pub use liquid_assets_inflate_macros::include_assets;

/// The Decompressor trait should be implemented on a struct and can be passed
/// to assets to decompress into a buffer
pub trait Decompressor {
    type Error;

    fn decompress<const N: usize>(
        &self,
        buffer: &mut [u8; N],
        compressed_data: &[u8],
    ) -> Result<usize, Self::Error>;
}
