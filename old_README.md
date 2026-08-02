# `liquid-assets`

`liquid-assets` is an assets pipeline for embedded Rust. It has two parts:

- `liquid-assets-deflate` is used in build.rs compress source images into binaries.
- `liquid-assets-inflate` provides a macro which loads these images and provides easy methods for decompressing them.

## What problem does it solve?

You could use a python script to convert the images into `.bin` files, and then load each binary into a slice:

```rust
const ANIMATION_DATA: &[&[u8]] = &[
    include_bytes!("path/frame1.bin"),
    include_bytes!("path/frame2.bin"),
    include_bytes!("path/frame3.bin"),
    include_bytes!("path/frame4.bin"),
    include_bytes!("path/frame5.bin"),
    ... // Rinse and repeat many times
];
```

This is very repetitive, and if the assets need to be changed then this need to be rewritten.

`liquid_assets` provides a pipeline which automates the compression and decompression of assets.

`liquid_assets_deflate` provides the `build_assets` function which can be placed in `build.rs`, and will compress assets into .bin files. The user has to implement the `Compressor` trait, to implement a compression library. Some example implementations of this trait are included in /example/build.rs.

`liquid_assets_inflate` provides the `include_assets` macro, which automates adding 

## Quick Example

Add `build_assets` to build.rs, providing a compression implementation and the source/target directories.

```rust
use liquid_assets_deflate::build_assets;
struct MinizOxideCompressor {}
impl Compressor for MinizOxideCompressor {
    // The compression is infallible
    type Error = ();

    fn compress(&mut self, input_bytes: &[u8]) -> Result<Vec<u8>, Self::Error> {
        const COMPRESSION_LEVEL: u8 = 5;
        Ok(miniz_oxide::deflate::compress_to_vec(
            input_bytes,
            COMPRESSION_LEVEL,
        ))
    }
}
fn main() {
    let mut compressor = MinizOxideCompressor {};
    build_assets(
        "./assets", // Source directory for assets
        "./asset-binaries", // Target directory for binaries
        TargetColorFormat::Rgb565, // Images will be converted to this colour format
        &mut compressor, // Pass a reference to the compressor
    );
    ... // Rest of build.rs
```

This will rebuild the assets whenever the assets source directory changes, or if the user runs `REBUILD_ASSETS=1 cargo build`.

Next, to load these animations in the embedded Rust code, call the `include_assets!` macro which is provided by `liquid_assets_inflate`.

```rust
use liquid_assets_inflate::include_assets;
const BUFFER_SIZE: usize = 135 * 135 * 2;
include_assets!("asset-binaries", BUFFER_SIZE);

fn main() {
    ... // Embedded setup here

    // Decompress a static asset
    let DecompressedData {
        bytes_wrote,
        width,
        height,
    } = assets::ESPRESSIF
        .decompress(&mut buffer, &decompressor)
        .unwrap();
    // You can now access the image as a slice of the buffer
    let data = buffer[..bytes_wrote];
    // It's up to the user to convert this into something that the display driver can use

    // You can also decompress animations as an iterator
    for (frame_index, frame) in assets::GITHUB.as_iter().enumerate() {
        let DecompressedData {
            bytes_wrote,
            width,
            height,
        } = frame.decompress(&mut buffer, &decompressor).unwrap();
        // Then display the frame
        // Then add a delay to maintain a steady framerate
    }
    ...
}
```

The `include_assets` macro will expand to something like this:

```rust
pub mod assets {
    use liquid_assets_inflate::Decompressor;
    pub enum Error<DecompressionError> {
        Decompression(DecompressionError),
        UnexpectedSize,
        FrameOutOfRange,
    }
    pub struct DecompressedData {
        pub bytes_wrote: usize,
        pub width: u16,
        pub height: u16,
    }
    pub struct StaticAsset {
        data: &'static [u8],
        width: u16,
        height: u16,
    }
    impl StaticAsset {
        /// Get the compressed data as a slice
        pub const fn get_comressed_data(&self) -> &'static [u8] { /* ... */ }
        /// Get the width of the image in pixels
        pub const fn width(&self) -> u16 { self.width }
        /// Get the height of the image in pixels
        pub const fn height(&self) -> u16 { self.height }
        /// Decompress the asset to the buffer by passing a Decompressor
        pub fn decompress<const N: usize, D: Decompressor>(
            &self,
            buffer: &mut [u8; N],
            decompressor: &D,
        ) -> Result<DecompressedData, Error<<D as Decompressor>::Error>> { /* ... */ }
    }
    ///An animated asset, which is a collection of frames (images) with the same dimensions
    pub struct AnimatedAsset<const N: usize> {
        frames: &'static [&'static [u8]],
        width: u16,
        height: u16,
    }
    impl<const N: usize> AnimatedAsset<N> {
        /// Get the total number of frames in the animation
        pub const fn get_number_of_frames(&self) -> usize { /* ... */ }
        /// Get the width of the frames in pixels
        pub fn width(&self) -> u16 { self.width }
        /// Get the height of the frames in pixels
        pub fn height(&self) -> u16 { self.height }
        /// Decompress a single frame into a buffer by passing a Decompressor. Returns an error if the frame is out of range
        pub fn decompress_frame<D: Decompressor>(
            &self,
            frame_number: usize,
            buffer: &mut [u8; N],
            decompressor: &D,
        ) -> Result<usize, Error<<D as Decompressor>::Error>> { /* ... */ }
        /// Get the compressed data for a frame. Retuns error if the frame is out of range
        pub fn get_compressed_frame_data(
            &self,
            frame_number: usize,
        ) -> Result<&'static [u8], Error<()>> { /* ... */ }
        /// Copy the compressed frame data into the buffer. Returns an error if the frame is out of range. On success, returns the number of bytes wrote
        pub fn copy_compressed_frame_data_to_buffer<D: Decompressor>(
            &self,
            frame_number: usize,
            buffer: &mut [u8; N],
        ) -> { /* ... */ }
        /// Access the animation as a FrameIterator (this method uses references so doesn't duplicate data)
        pub fn as_iter(&self) -> FrameIterator { /* ... */ }
    }
    pub struct FrameIterator {
        frames: &'static [&'static [u8]],
        width: u16,
        height: u16,
        current_frame: usize,
    }
    impl FrameIterator { /* ... */ }
    impl Iterator for FrameIterator { /* ... */ }

    // All your assets will then be defined. Here we only have two examples:

    pub const COMPANY_LOGO: StaticAsset = StaticAsset {
        data: include_bytes!("assets_directory/company_logo.bin")
        width: 128,
        height: 128,
    };
    pub const LOADING_ANIMATION: AnimatedAsset<{ super::BUFFER_SIZE }> = AnimatedAsset {
        frames: &[
            include_bytes!("assets_directory/loading_animation/frame1.bin"),
            include_bytes!("assets_directory/loading_animation/frame2.bin"),
            /* ... plus all the rest of the frames */
        ],
        width: 135,
        height: 135,
    };

    pub const fn get_all_static_assets() -> &'static [&'static StaticAsset] { /*...*/ }
    pub const fn get_all_animated_assets() -> &'static [&'static AnimatedAsset<{ super::BUFFER_SIZE }>] { /*...*/ }
}
```

Note that in the Cargo.toml `liquid-assets-inflate` is added to `[dependencies]` and `liquid-assets-deflate` is added to `[build-dependencies]`.

```
[dependencies]
liquid-assets-inflate = { git = "git@github.com:tom-flaherty/liquid-assets.git", version = "0.3.0" }
[build-dependencies]
liquid-assets-deflate = { git = "git@github.com:tom-flaherty/liquid-assets.git", version = "0.3.0" }
```

## Assets Directory Format

In the following example, `espressif` is a static asset, whereas `github` and `loading` are animations. Images must already be the desired size. Frames must be named `snake_case` with a numeric suffix starting with 1.

```
assets
├── espressif.png
├── github
│   ├── frame_0001.png
│   ├── frame_0002.png
│   └── ...
└── loading
    ├── frame_0001.png
    ├── frame_0002.png
    └── ...
```

To use `liquid-assets`, first add `liquid-assets-deflate` to the `[dev-dependencies]` of your Cargo.toml.

# Long Term TODO

- The compression code could be refactored to use more structs, which may improve readablility
- Remove "... as u16" from compression code
- Support for displays other using colour formats other than RGB565
- Support for transparency
- Add a way to build assets without adding to build.rs
- Support for bitmaps?
- Prevent unwanted rebuilds
- Only rebuild the specific assets that changed (this would add a lot of complexity)

## Licence

This software is provided under the MIT Licence (see LICENCE file). If you find this project helpful, please give the repo a star.

## Contributing

Please raise an issue on github to discuss changes.

## Other notes

Only tested on Linux.

You can convert a gif to frames using:

`ffmpeg -i mygif.gif frame_%04d.png`

Or to also resize:

`ffmpeg -i mygif.gif -vf scale=128:128 frame_%04d.png`
