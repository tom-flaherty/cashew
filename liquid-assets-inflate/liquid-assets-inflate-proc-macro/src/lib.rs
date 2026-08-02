use proc_macro_error::{abort_call_site, proc_macro_error};
use quote::quote;
use serde::Deserialize;
use std::{
    ffi::OsStr,
    fs::{self, DirEntry},
    path::{Path, PathBuf},
};
use syn::{Expr, LitInt, LitStr, Token, parse::Parse};

enum BufferSizeParam {
    Expr(Expr),
    LitInt(usize),
}

#[derive(Deserialize, Debug)]
struct JsonData {
    image_width: u16,
    image_height: u16,
}

struct MacroArgs {
    assets_path: String,
    buffer_size: BufferSizeParam,
}

impl Parse for MacroArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lit_str: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let buffer_size = match input.parse::<LitInt>() {
            Ok(lit_int) => BufferSizeParam::LitInt(lit_int.base10_parse()?),
            Err(_e) => BufferSizeParam::Expr(input.parse::<Expr>()?),
        };

        Ok(MacroArgs {
            assets_path: lit_str.value(),
            buffer_size,
        })
    }
}

struct AssetDefinitions {
    asset_definition: proc_macro2::TokenStream,
    asset_name: proc_macro2::TokenStream,
}

/// Generate an assets module containing the compressed image data, organised into
/// StaticAsset and AnimatedAsset structs. The input is the path to the asset binaries
/// directory, relative to the Cargo.toml
#[proc_macro]
#[proc_macro_error]
pub fn include_assets(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let MacroArgs {
        assets_path,
        buffer_size,
    } = syn::parse_macro_input!(input as MacroArgs);

    let mut target_dir: PathBuf = match std::env::var("CARGO_MANIFEST_DIR") {
        Ok(path_str) => path_str,
        Err(e) => abort_call_site!(
            "Could not read environment variable CARGO_MANIFEST_DIR: {:?}",
            e
        ),
    }
    .into();
    target_dir.push(Path::new(&assets_path));

    let target_dir_read = match fs::read_dir(&target_dir) {
        Ok(read_dir) => read_dir,
        Err(e) => {
            abort_call_site!("Failed to read directory {:?}: {:?}", target_dir, e)
        }
    };

    let mut asset_definitions: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut static_asset_names: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut animated_asset_names: Vec<proc_macro2::TokenStream> = Vec::new();
    for item in target_dir_read {
        let item = item.unwrap();
        let file_type = item.file_type().unwrap();
        if file_type.is_dir() {
            let AssetDefinitions {
                asset_definition,
                asset_name,
            } = process_animated_asset(item, &buffer_size);
            asset_definitions.push(asset_definition);
            animated_asset_names.push(asset_name);
        } else if file_type.is_file() {
            if item.path().extension().unwrap_or(OsStr::new("")) == "bin" {
                let AssetDefinitions {
                    asset_definition,
                    asset_name,
                } = process_static_asset(item);
                asset_definitions.push(asset_definition);
                static_asset_names.push(asset_name);
            }
        }
    }

    let struct_definitions = define_module_types();
    let get_all_methods =
        define_get_all_methods(static_asset_names, animated_asset_names, buffer_size);

    quote! {
        pub mod assets {
            use liquid_assets_inflate::Decompressor;
            #struct_definitions
            #(#asset_definitions)*
            #get_all_methods
        }
    }
    .into()
}

fn process_static_asset(asset: DirEntry) -> AssetDefinitions {
    let asset_name = process_asset_name(&asset);

    let path_to_bin: String = match asset.path().to_str() {
        Some(path_as_str) => path_as_str,
        None => abort_call_site!("Unable to convert path to string: {:?}", asset.path()),
    }
    .to_owned();

    // Load metadata from json
    let json_data =
        fs::read(asset.path().with_extension("json")).expect("Could not find json file for `{}`");
    let json_data: JsonData = serde_json::from_slice(json_data.as_slice()).expect(
        format!(
            "Failed to parse json file for static asset `{}`",
            asset_name
        )
        .as_str(),
    );
    let width = json_data.image_width;
    let height = json_data.image_height;

    AssetDefinitions {
        asset_definition: quote! {
            pub const #asset_name: StaticAsset = StaticAsset {
                data: include_bytes!(#path_to_bin).as_slice(),
                width: #width,
                height: #height,
            };
        },
        asset_name,
    }
}

fn process_animated_asset(asset: DirEntry, buffer_size: &BufferSizeParam) -> AssetDefinitions {
    let asset_name = process_asset_name(&asset);

    let total_frames = match fs::read_dir(asset.path()) {
        Ok(read_dir) => read_dir,
        Err(e) => abort_call_site!(
            "Could not read animated asset directory {:?}: {:?}",
            asset.path(),
            e
        ),
    }
    .count();

    // Load metadata from json
    let json_data =
        fs::read(asset.path().with_extension("json")).expect("Could not find json file for `{}`");
    let json_data: JsonData = serde_json::from_slice(json_data.as_slice()).expect(
        format!(
            "Failed to parse json file for static asset `{}`",
            asset_name
        )
        .as_str(),
    );
    let width = json_data.image_width;
    let height = json_data.image_height;

    // Data for each frame is added with include bytes
    let mut frame_data: Vec<proc_macro2::TokenStream> = Vec::new();

    for frame_number in 1..(total_frames + 1) {
        let frame_dir = asset.path().clone();

        let frame_bin_path = frame_dir.join(Path::new(&format!("FRAME{}.bin", frame_number)));

        let frame_bin_path_str = match frame_bin_path.to_str() {
            Some(frame_path_str) => frame_path_str,
            None => abort_call_site!("Failed to convert frame path to str"),
        };

        // Surround the path with quote marks
        let frame_bin_path_str = format!("\"{}\"", frame_bin_path_str);

        let frame_path = match frame_bin_path_str.parse::<proc_macro2::TokenStream>() {
            Ok(frame_path_token_stream) => frame_path_token_stream,
            Err(e) => abort_call_site!(
                "Failed to convert frame path str into token stream: {:?}",
                e
            ),
        };

        frame_data.push(quote! {
            include_bytes!(#frame_path).as_slice(),
        });
    }

    // The AnimatedAsset struct requires a size at compile time. The user may have entered
    // a integer literal (which is used directly) or an expression which is accessed with super::
    let buffer_size = match buffer_size {
        BufferSizeParam::Expr(expr) => quote! {{ super::#expr }},
        BufferSizeParam::LitInt(lit_int) => quote! {#lit_int},
    };

    AssetDefinitions {
        asset_definition: quote! {
            pub const #asset_name: AnimatedAsset<#buffer_size> = AnimatedAsset {
                frames: &[
                    #(#frame_data)*
                ],
                width: #width,
                height: #height,
            };
        },
        asset_name,
    }
}

fn process_asset_name(asset_dir_entry: &DirEntry) -> proc_macro2::TokenStream {
    let asset_path = asset_dir_entry.path();
    let asset_file_stem = match asset_path.file_stem() {
        Some(file_stem) => file_stem,
        None => abort_call_site!(
            "Unable to get file stem for asset {:?}",
            asset_dir_entry.path()
        ),
    };
    let asset_name = match asset_file_stem.to_str() {
        Some(asset_name) => asset_name,
        None => abort_call_site!(
            "Unable to convert file stem into str {:?}",
            asset_dir_entry.path()
        ),
    }
    .to_string();

    if !is_snake_case(&asset_name) {
        abort_call_site!("Asset name is not valid snake_case: {:?}", asset_name);
    }

    let asset_name = asset_name.to_ascii_uppercase();

    asset_name.parse().unwrap()
}

fn define_module_types() -> proc_macro2::TokenStream {
    quote! {
        #[doc = "Errors which may be returned by decompression methods. Errors may originate from the compression crate"]
        #[derive(Debug)]
        pub enum Error<DecompressionError> {
            Decompression(DecompressionError),
            UnexpectedSize,
            FrameOutOfRange,
        }

        #[doc = "Returned by decompression functions"]
        pub struct DecompressedData {
            #[doc = "The number of bytes wrote to the buffer"]
            pub bytes_wrote: usize,
            #[doc = "The width of the image"]
            pub width: u16,
            #[doc = "The height of the image"]
            pub height: u16,
        }

        #[doc = "A static (non-animated) asset"]
        pub struct StaticAsset {
            data: &'static [u8],
            width: u16,
            height: u16,
        }
        impl StaticAsset {
            #[doc = "Get the compressed data as a slice"]
            pub const fn get_compressed_data(&self) -> &'static [u8] {
                self.data
            }
            #[doc = "Get the width of the image in pixels"]
            pub const fn width(&self) -> u16 {
                self.width
            }
            #[doc = "Get the height of the image in pixels"]
            pub const fn height(&self) -> u16 {
                self.height
            }
            #[doc = "Decompress the asset to the buffer by passing a Decompressor"]
            pub fn decompress<const N: usize, D: Decompressor>(
                &self,
                buffer: &mut [u8; N],
                decompressor: &D,
            ) -> Result<DecompressedData, Error<<D as Decompressor>::Error>> {
                let bytes_wrote = decompressor
                    .decompress(buffer, self.data)
                    .map_err(|e| Error::Decompression(e))?;
                // TODO this is only valid for RGB565
                const BYTES_PER_PIXEL: usize = 2;
                if bytes_wrote != (self.width as usize) * (self.height as usize) * BYTES_PER_PIXEL {
                    return Err(Error::UnexpectedSize);
                }
                Ok(DecompressedData {
                    bytes_wrote,
                    width: self.width,
                    height: self.height,
                })
            }
        }

        #[doc = "An animated asset, which is a collection of frames (images) with the same dimensions"]
        pub struct AnimatedAsset<const N: usize> {
            frames: &'static [&'static [u8]],
            width: u16,
            height: u16,
        }
        impl<const N: usize> AnimatedAsset<N> {
            #[doc = "Get the total number of frames in the animation"]
            pub const fn get_number_of_frames(&self) -> usize {
                self.frames.len()
            }
            #[doc = "Get the width of the frames in pixels"]
            pub const fn width(&self) -> u16 {
                self.width
            }
            #[doc = "Get the height of the frames in pixels"]
            pub const fn height(&self) -> u16 {
                self.height
            }
            #[doc = "Decompress a single frame into a buffer by passing a Decompressor. Returns an error if the frame is out of range"]
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
                // TODO this is only valid for RGB565
                const BYTES_PER_PIXEL: usize = 2;
                if bytes_wrote != (self.width as usize) * (self.height as usize) * BYTES_PER_PIXEL {
                    return Err(Error::UnexpectedSize);
                }
                Ok(DecompressedData {
                    bytes_wrote,
                    width: self.width,
                    height: self.height,
                })
            }
            #[doc = "Get the compressed data for a frame. Retuns error if the frame is out of range"]
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
            #[doc = "Copy the compressed frame data into the buffer. Returns an error if the frame is out of range. On success, returns the number of bytes wrote"]
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
            #[doc = "Access the animation as a FrameIterator (this method uses references so doesn't duplicate data)"]
            pub fn as_iter(&self) -> FrameIterator {
                FrameIterator::new(self.frames, self.width, self.height)
            }
        }

        #[doc = "Access the animation as a FrameIterator. This returns each frame in the animation as a static asset. Can be used with the syntax `for frame in assets::ANIMATION.as_iter() {...}`"]
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
    }
}

fn define_get_all_methods(
    static_asset_names: Vec<proc_macro2::TokenStream>,
    animated_asset_names: Vec<proc_macro2::TokenStream>,
    buffer_size: BufferSizeParam,
) -> proc_macro2::TokenStream {
    let mut static_asset_processed: Vec<proc_macro2::TokenStream> = Vec::new();
    for asset_name in static_asset_names {
        static_asset_processed.push(quote! {&#asset_name, })
    }

    let mut animated_assets_processed: Vec<proc_macro2::TokenStream> = Vec::new();
    for asset_name in animated_asset_names {
        animated_assets_processed.push(quote! {&#asset_name, })
    }

    // TODO remove this duplicate code
    let buffer_size = match buffer_size {
        BufferSizeParam::Expr(expr) => quote! {{ super::#expr }},
        BufferSizeParam::LitInt(lit_int) => quote! {#lit_int},
    };

    quote! {
        #[doc = "Retuns a slice containing all StaticAsset structs defined in the assets module"]
        pub const fn get_all_static_assets() -> &'static [&'static StaticAsset] {
            &[#(#static_asset_processed)*].as_slice()
        }
        #[doc = "Returns a slice containing all AnimatedAsset structs defined in the assets module"]
        pub const fn get_all_animated_assets() -> &'static [&'static AnimatedAsset<#buffer_size>] {
            &[#(#animated_assets_processed)*].as_slice()
        }
    }
}

fn is_snake_case(str: &String) -> bool {
    if str.is_empty() {
        false
    } else {
        str.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !str.starts_with(|c: char| c.is_ascii_digit())
            && !str.contains("__")
            && !str.ends_with('_')
    }
}
