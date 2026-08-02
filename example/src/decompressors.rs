use liquid_assets_inflate::Decompressor;

#[allow(unused)]
pub struct MinizOxideDecompressor;
impl Decompressor for MinizOxideDecompressor {
    type Error = miniz_oxide::inflate::TINFLStatus;

    fn decompress<const N: usize>(
        &self,
        buffer: &mut [u8; N],
        compressed_data: &[u8],
    ) -> Result<usize, Self::Error> {
        miniz_oxide::inflate::decompress_slice_iter_to_slice(
            buffer,
            core::iter::once(compressed_data),
            false,
            false,
        )
    }
}

#[allow(unused)]
pub struct LzssDecompressor;
impl Decompressor for LzssDecompressor {
    type Error = lzss::LzssError<void::Void, lzss::SliceWriteError>;

    fn decompress<const N: usize>(
        &self,
        buffer: &mut [u8; N],
        compressed_data: &[u8],
    ) -> Result<usize, Self::Error> {
        use lzss::{Lzss, SliceReader, SliceWriter};
        type LzssDecoder = Lzss<10, 4, 0x20, { 1 << 10 }, { 2 << 10 }>;
        LzssDecoder::decompress_stack(
            SliceReader::new(compressed_data),
            SliceWriter::new(buffer.as_mut_slice()),
        )
    }
}

#[allow(unused)]
pub struct NoDecompressor;
impl Decompressor for NoDecompressor {
    type Error = ();

    fn decompress<const N: usize>(
        &self,
        buffer: &mut [u8; N],
        compressed_data: &[u8],
    ) -> Result<usize, Self::Error> {
        buffer[..compressed_data.len()].copy_from_slice(compressed_data);
        Ok(compressed_data.len())
    }
}
