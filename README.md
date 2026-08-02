# `liquid-assets`

`liquid-assets` is an assets pipeline for embedded Rust. It has two parts:

- `liquid-assets-deflate` is used in build.rs to convert the images into the required image format and then compress source images into binaries.
- `liquid-assets-inflate` provides a macro which loads these images and provides easy methods for decompressing them.

## Assets directory structure

All the assets must be placed inside a directory and organised correctly.
All image files must already be the correct resolution (but do not have to be in the correct colour format).
These conditions must be met:

- All assets should be named in snake case. This is because a Rust variable will be generated with the name of the asset.
- Images for static assets can be placed directly in the assets directory.
- For each animation, a directory should be created in the assets directory and named with the animation name. The animation directory should contain the frame images.
- The naming of the frame images is pretty flexible, but must end with the frame number.
- The first frame must be frame 1 (not frame 0!).
- There cannot be any missing frame numbers.
- All animation frames must have the same resolution

Example layout with 4 static assets and 2 animations:

```
├── company_logo.png
├── warning.png
├── connected.png
├── disconnected.png
├── loading
│   ├── frame_0001.png
│   ├── frame_0002.png
│   ├── frame_0003.png
│   ├── frame_0004.png
│   ├── frame_0005.png
│   └── frame_0006.png
└── connection_success
    ├── frame_0001.png
    ├── frame_0002.png
    ├── frame_0003.png
    ├── frame_0004.png
    ├── frame_0005.png
    ├── frame_0006.png
    ├── frame_0007.png
    └── frame_0008.png
```

## Compressing assets using `liquid-assets-deflate`

Firstly, a compression method must be implemented in `build.rs` using the `Compressor` trait.
There are some [example implementations](https://github.com/tom-flaherty/liquid-assets/blob/master/example/build.rs) provided for the [miniz oxide](https://crates.io/crates/miniz_oxide) compression library, the [lzss](https://crates.io/crates/lzss) compression library, and a no-compression implementation.
In most cases 
The trait simply wraps a compression library.

```rust
// Example implementatino of the compressor trait for miniz oxide
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
```

Note that as this is running in `build.rs` it can use the standard library as it does not run on-target.

Also note that depending on the speed of the target flash memory, load times may not be better when using no compression.
In some circumstances it will be quicker to load a small amount of memory and decompress it than to load a large amount of uncompressed memory.

Finally, call `build_assets` in `build.rs`. Here is an example:

```rust
use liquid_assets_deflate::{Compressor, TargetColorFormat, build_assets};

struct MinizOxideCompressor {}
impl Compressor for MinizOxideCompressor { ... }

fn main() {
    let mut compressor = MinizOxideCompressor {};

    build_assets(
        "./assets", // The assets source directory
        "./asset-binaries", // The destination for compiled asset binaries
        TargetColorFormat::Rgb565, // The colour format of the display (currently only RGB565 is supported)
        &mut compressor, // Mutable reference to the compressor
    );
    // ... the rest of the build file
}
```

When the source directory is changed the assets will be rebuilt automatically.
The assets will also be rebuilt if another part of `build.rs` needs to be rerun, which may increase compilation times.
The user can also force the assets to be rebuilt: `REBUILD_ASSETS=1 cargo run`

## Decompressing assets using `liquid-assets-inflate`

The biggest timesaver when using `liquid-assets` is in the decompression of the assets.

First, the user must implement the `Decompressor` trait, using the same compression library as for the `Compressor` trait implementation.
There are some [example](https://github.com/tom-flaherty/liquid-assets/blob/master/example/src/decompressors.rs) implementation of the `Decompressor` traits, which can be copied for other projects.

```rust
pub struct MinizOxideDecompressor {}
impl Decompressor for MinizOxideDecompressor {
    // Wraps to the compression library error
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
```

A fixed length decompression buffer should be created, which must be big enough to contain the largest asset.

Invoke the `include_assets` macro, providing the directory for the assets binaries which were created by `liquid-assets-deflate`, and the size of the decompression buffer.

```rust
const BUFFER_SIZE: usize = 135 * 135 * 2;
liquid_assets_inflate::include_assets!("asset-binaries", BUFFER_SIZE);
```

This macro will generate a module called `assets`, which contains all the compressed data stored as `const`.
To see what this will module will expand to, please see the appendix.

In summary, the module contains the following:

| Item                      | Type       | Usage                                                                                                                           |
|---------------------------|------------|---------------------------------------------------------------------------------------------------------------------------------|
| `Error`                   | `enum`     | Error type for decompression methods. Can map to the compression crate error type.                                              |
| `DecompressedData`        | `struct`   | Returned by decompression methods on success. Stores the number of bytes written to the buffer, and the asset width and height. |
| `StaticAsset`             | `struct`   | Stores data relating to a static asset. See the section below for more detail.                                                  |
| `AnimatedAsset`           | `struct`   | Stores data relating to an animation. See the section below for more detail.                                                    |
| `FrameIterator`           | `struct`   | Can be obtained from an `AnimatedAsset` to iterate over the frames in an animation.                                             |
| `get_all_static_assets`   | `const fn` | Returns a slice of all `StaticAsset`s, which may be useful when running benchmarks.                                             |
| `get_all_animated_assets` | `const fn` | Returns a slice of all `AnimatedAsset`s, which may be useful when running benchmarks.                                           |

### `StaticAsset` overview

The following methods are implemented for `StaticAsset`:

```rust
pub const fn get_comressed_data(&self) -> &'static [u8] { ... }
```

Get a static reference to the compressed data.

```rust
pub const fn width(&self) -> u16 { self.width }
```

Get the width of the image in pixels.

```rust
pub const fn height(&self) -> u16 { self.height }
```

Get the height of the image in pixels.

```rust
pub fn decompress<const N: usize, D: Decompressor>(
    &self,
    buffer: &mut [u8; N],
    decompressor: &D,
) -> Result<DecompressedData, Error<<D as Decompressor>::Error>> { /* ... */ }
```

Decompress the static asset into the the decompression buffer, using a reference to the struct implementing the `Decompressor` trait.
If successful it will return a `DecompressedData` struct, which contains the number of bytes written to the buffer (starting from the first byte), the image width and the image height.

<!-- | Method                         | Parameters                                                                                                    | Return Type                                                  | Usage                                                                                      |
|--------------------------------|---------------------------------------------------------------------------------------------------------------|--------------------------------------------------------------|--------------------------------------------------------------------------------------------|
| `const fn get_compressed_data` | N/A                                                                                                           | `&'static [u8]`                                              | Returns slice of bytes for the compressed data for the asset                               |
| `const fn width`               | N/A                                                                                                           | `u16`                                                        | Returns the width of the image in pixels                                                   |
| `const fn height`              | N/A                                                                                                           | `u16`                                                        | Returns the height of the image in pixels                                                  |
| `fn decompress`                | Mutable reference to the decompression buffer, reference to the struct implementing the `Decompressor` trait` | `Result<DecompressedData, Error<<D as Decompressor>::Error>` | Attempts to decompresses the asset and return a `DecompressedData`. May return an `Error`` | -->

### `AnimatedAsset` overview

The following methods are implemented for `AnimatedAsset`:

```rust
pub const fn get_number_of_frames(&self) -> usize { self.frames.len() }
```

Get the number of frames in the animation.

```rust
pub const fn width(&self) -> u16 { self.width }
```

Get the width of the animation frames in pixels.

```rust
pub const fn height(&self) -> u16 { self.height }
```

Get the height of the animation frames in pixels.

```rust
pub fn decompress_frame<D: Decompressor>(
    &self,
    frame_number: usize,
    buffer: &mut [u8; N],
    decompressor: &D,
) -> Result<usize, Error<<D as Decompressor>::Error>> { ... }
```

Decompress a specific animation frame into the decompression buffer.
The first frame is 0.
Requires a mutable reference to the decompression buffer, and a reference to the struct implementing the `Decompressor` trait.

```rust
pub fn get_compressed_frame_data(
    &self,
    frame_number: usize,
) -> Result<&'static [u8], Error<()>> { ... }
```

Get the compressed data for a single frame.
The first frame is 0.

```rust
pub fn copy_compressed_frame_data_to_buffer<D: Decompressor>(
    &self,
    frame_number: usize,
    buffer: &mut [u8; N],
) -> Result<usize, Error<<D as Decompressor>::Error>> { ... }
```

Copy the compressed frame data into the buffer.
If successful, returns the number of bytes written to the buffer.

```rust
pub fn as_iter(&self) -> FrameIterator { ... }
```

Access the animation frames as a `FrameIterator`.

### Example

You can see a proper [example](https://github.com/tom-flaherty/liquid-assets/tree/master/example) on the Github page.
A rough example can be seen below:

```rust
#![no_std]
const BUFFER_SIZE: usize = 135 * 135 * 2; // width * height * bytes per pixel
liquid_assets_inflate::include_assets!("asset-binaries", BUFFER_SIZE);
pub struct MinizOxideDecompressor {}
impl Decompressor for MinizOxideDecompressor { ... }

fn example() {
    let decompressor = MinizOxideDecompressor {};

    // Setup the display here (see the Github example)

    // Decompress a static asset
    let DecompressedData {
        bytes_wrote,
        width,
        height: _,
    } = assets::COMPANY_LOGO
        .decompress(&mut buffer, &decompressor)
        .expect("Decompression failed");
    // The data is now in the buffer. It's up to the user to push this data to the display driver
    // Some display drivers support using embedded_graphics::image::Image
    let image_raw = embedded_graphics::image::ImageRaw::<Rgb565>::new(
        &frame_buffer[0..bytes_wrote],
        width as u32,
    );
    let image = embedded_graphics::image::Image::new(&image_raw, Point { x: 0, y: 0 });
    image.draw(&mut display).unwrap();

    delay.delay(Duration::from_secs(1));

    // Loop over an animation
    for frame_number in 0..assets::LOADING.get_number_of_frames() {
        let DecompressedData { .. } = assets::LOADING
            .decompress_frame(frame_number, &mut buffer, &decompressor)
            .expect("Decompression failed");
        
        // The data can be displayed
        
        // Note that the decompression time is unpredicatable, so for smoother animations
        // don't use a fixed delay like this
        delay.delay(Duration::from_millis(50));
    }

    // Iterate over an animation using a FrameIterator
    for (frame_number, frame) in assets::LOADING.as_iter().enumerate() {
        let frame_start_time = Instant::now();
        rprint!("Frame no. {} ", frame_number);

        let decompression_start_time = Instant::now();
        let DecompressedData {
            bytes_wrote, width, ..
        } = frame.decompress(&mut frame_buffer, &decompressor).unwrap();
        rprint!("Decomp. in {} ", decompression_start_time.elapsed());

        // Now it's up to the user to display the decompressed data
        // The mipidsi driver used in this example is compatible with embedded_graphics::Image

        let image_raw = embedded_graphics::image::ImageRaw::<Rgb565>::new(
            &frame_buffer[0..bytes_wrote],
            width as u32,
        );
        let image = embedded_graphics::image::Image::new(&image_raw, Point { x: 0, y: 0 });

        let draw_start = Instant::now();
        image.draw(&mut display).unwrap();
        rprint!("Draw time {} ", draw_start.elapsed());

        // Delay to maintain framerate
        delay.delay(
            desired_frame_time
                .checked_sub(frame_start_time.elapsed())
                .unwrap_or(Duration::from_millis(0)),
        );

        rprintln!("Frame Time {}", frame_start_time.elapsed());
    }
}

```

## What are the advantages of using this library?

- It's easy to add and remove assets, and compressing them does not require explicitly calling another script.
- As macros are used to include the compiled asset binaries, the user doesn't need to manually update the `include_bytes!()` calls every time (huge timesaver!). The macros do lots of work to make this easier.

## What are the disadvantages of using this library?

- If another part of build.rs needs to be reran then all the assets will be recompiled, which adds to compile time.
- For projects with lots of assets, it's better to use external flash memory rather than including the assets in the main binary, as the binary size will bloat and cause long flash times.

## What about text?

This crate doesn't support text as there are already crates which do this effectively.
The `embedded-graphics` library includes some mono-space fonts.
For non-mono text, [minitype](https://crates.io/crates/minitype) can be used to generate bitmaps from font files.

## Appendix

### `include_assets` macro expansion

Some repetitive parts have been replaced with `...`

```rust
pub mod assets {
    use liquid_assets_inflate::Decompressor;
    ///Errors which may be returned by decompression methods. Errors may originate from the compression crate
    pub enum Error<DecompressionError> {
        Decompression(DecompressionError),
        UnexpectedSize,
        FrameOutOfRange,
    }
    ///Returned by decompression functions
    pub struct DecompressedData {
        ///The number of bytes wrote to the buffer
        pub bytes_wrote: usize,
        ///The width of the image
        pub width: u16,
        ///The height of the image
        pub height: u16,
    }
    ///A static (non-animated) asset
    pub struct StaticAsset {
        data: &'static [u8],
        width: u16,
        height: u16,
    }
    impl StaticAsset {
        /// Get the compressed data as a slice
        pub const fn get_compressed_data(&self) -> &'static [u8] {
            self.data
        }
        /// Get the width of the image in pixels
        pub const fn width(&self) -> u16 {
            self.width
        }
        /// Get the height of the image in pixels
        pub const fn height(&self) -> u16 {
            self.height
        }
        /// Decompress the asset to the buffer by passing a Decompressor
        pub fn decompress<const N: usize, D: Decompressor>(
            &self,
            buffer: &mut [u8; N],
            decompressor: &D,
        ) -> Result<DecompressedData, Error<<D as Decompressor>::Error>> {
            let bytes_wrote = decompressor
                .decompress(buffer, self.data)
                .map_err(|e| Error::Decompression(e))?;
            const BYTES_PER_PIXEL: usize = 2;
            if bytes_wrote
                != (self.width as usize) * (self.height as usize) * BYTES_PER_PIXEL
            {
                return Err(Error::UnexpectedSize);
            }
            Ok(DecompressedData {
                bytes_wrote,
                width: self.width,
                height: self.height,
            })
        }
    }
    /// An animated asset, which is a collection of frames (images) with the same dimensions
    pub struct AnimatedAsset<const N: usize> {
        frames: &'static [&'static [u8]],
        width: u16,
        height: u16,
    }
    impl<const N: usize> AnimatedAsset<N> {
        /// Get the total number of frames in the animation
        pub const fn get_number_of_frames(&self) -> usize {
            self.frames.len()
        }
        /// Get the width of the frames in pixels
        pub const fn width(&self) -> u16 {
            self.width
        }
        /// Get the height of the frames in pixels
        pub const fn height(&self) -> u16 {
            self.height
        }
        /// Decompress a single frame into a buffer by passing a Decompressor. Returns an error if the frame is out of range
        pub fn decompress_frame<D: Decompressor>(
            &self,
            frame_number: usize,
            buffer: &mut [u8; N],
            decompressor: &D,
        ) -> Result<DecompressedData, Error<<D as Decompressor>::Error>> {
            if frame_number >= self.frames.len() {
                return Err(Error::FrameOutOfRange);
            }
            let bytes_wrote = decompressor
                .decompress(buffer, self.frames[frame_number])
                .map_err(|e| Error::Decompression(e))?;
            const BYTES_PER_PIXEL: usize = 2;
            if bytes_wrote
                != (self.width as usize) * (self.height as usize) * BYTES_PER_PIXEL
            {
                return Err(Error::UnexpectedSize);
            }
            Ok(DecompressedData {
                bytes_wrote,
                width: self.width,
                height: self.height,
            })
        }
        /// Get the compressed data for a frame. Retuns error if the frame is out of range
        pub fn get_compressed_frame_data(
            &self,
            frame_number: usize,
        ) -> Result<&'static [u8], Error<()>> {
            if frame_number < self.frames.len() {
                Ok(self.frames[frame_number])
            } else {
                Err(Error::FrameOutOfRange)
            }
        }
        /// Copy the compressed frame data into the buffer. Returns an error if the frame is out of range. On success, returns the number of bytes wrote
        pub fn copy_compressed_frame_data_to_buffer<D: Decompressor>(
            &self,
            frame_number: usize,
            buffer: &mut [u8; N],
        ) -> Result<usize, Error<<D as Decompressor>::Error>> {
            if frame_number < self.frames.len() {
                let source_bytes = self.frames[frame_number as usize];
                buffer[..source_bytes.len()].copy_from_slice(source_bytes);
                Ok(source_bytes.len())
            } else {
                Err(Error::FrameOutOfRange)
            }
        }
        /// Access the animation as a FrameIterator (this method uses references so doesn't duplicate data)
        pub fn as_iter(&self) -> FrameIterator {
            FrameIterator::new(self.frames, self.width, self.height)
        }
    }
    /// Access the animation as a FrameIterator. This returns each frame in the animation as a static asset. Can be used with the syntax `for frame in assets::ANIMATION.as_iter() {...}`
    pub struct FrameIterator {
        frames: &'static [&'static [u8]],
        width: u16,
        height: u16,
        current_frame: usize,
    }
    impl FrameIterator {
        pub fn new(frames: &'static [&'static [u8]], width: u16, height: u16) -> Self {
            Self {
                frames,
                width,
                height,
                current_frame: 0,
            }
        }
    }
    impl Iterator for FrameIterator {
        type Item = StaticAsset;
        fn next(&mut self) -> Option<Self::Item> {
            if self.current_frame < self.frames.len() {
                let data = self.frames[self.current_frame];
                self.current_frame += 1;
                Some(StaticAsset {
                    data,
                    width: self.width,
                    height: self.height,
                })
            } else {
                None
            }
        }
    }
    pub const CONNECTION_SUCCESS: AnimatedAsset<{ super::BUFFER_SIZE }> = AnimatedAsset {
        frames: &[
            include_bytes(...).as_slice(),
            include_bytes(...).as_slice(),
            include_bytes(...).as_slice(),
            ...
        ],
        width: 135u16,
        height: 135u16,
    };
    pub const LOADING: AnimatedAsset<{ super::BUFFER_SIZE }> = AnimatedAsset {
        frames: &[
            include_bytes(...).as_slice(),
            include_bytes(...).as_slice(),
            include_bytes(...).as_slice(),
            ...
        ],
        width: 135u16,
        height: 135u16,
    };
    pub const COMPANY_LOGO: StaticAsset = StaticAsset {
        data: include_bytes(...).as_slice(),
        width: 128u16,
        height: 128u16,
    };
    pub const WARNING: StaticAsset = StaticAsset {
        data: include_bytes(...).as_slice(),
        width: 128u16,
        height: 128u16,
    };
    pub const CONNECTED: StaticAsset = StaticAsset {
        data: include_bytes(...).as_slice(),
        width: 128u16,
        height: 128u16,
    };
    pub const DISCONNECTED: StaticAsset = StaticAsset {
        data: include_bytes(...).as_slice(),
        width: 128u16,
        height: 128u16,
    };
    /// Retuns a slice containing all StaticAsset structs defined in the assets module
    pub const fn get_all_static_assets() -> &'static [&'static StaticAsset] {
        &[&COMPANY_LOGO, &WARNING, &CONNECTED, &DISCONNECTED].as_slice()
    }
    /// Returns a slice containing all AnimatedAsset structs defined in the assets module
    pub const fn get_all_animated_assets() -> &'static [&'static AnimatedAsset<
        { super::BUFFER_SIZE },
    >] {
        &[&CONNECTED, &LOADING].as_slice()
    }
}
```
