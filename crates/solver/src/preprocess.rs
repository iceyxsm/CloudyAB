//! Image preprocessing utilities for captcha solver models.
//!
//! Handles decoding, resizing, normalization, and tensor conversion
//! for feeding images into ONNX models.

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, RgbImage};
use ndarray::Array4;

use crate::traits::SolverError;

/// Standard normalization mean for ImageNet-pretrained models.
const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];

/// Standard normalization std for ImageNet-pretrained models.
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// Decode PNG/JPEG bytes into a DynamicImage.
pub fn decode_image(bytes: &[u8]) -> Result<DynamicImage, SolverError> {
    image::load_from_memory(bytes)
        .map_err(|e| SolverError::ImageProcessing(format!("Failed to decode image: {e}")))
}

/// Convert image to grayscale, resize, and produce a normalized [1, 1, H, W] tensor.
pub fn to_grayscale_tensor(
    img: &DynamicImage,
    height: u32,
    width: u32,
) -> Result<Array4<f32>, SolverError> {
    let gray: GrayImage =
        image::imageops::resize(&img.to_luma8(), width, height, FilterType::Lanczos3);

    let mut tensor = Array4::<f32>::zeros((1, 1, height as usize, width as usize));
    for y in 0..height as usize {
        for x in 0..width as usize {
            let pixel = gray.get_pixel(x as u32, y as u32).0[0];
            tensor[[0, 0, y, x]] = pixel as f32 / 255.0;
        }
    }

    Ok(tensor)
}

/// Convert image to RGB, resize, and produce an ImageNet-normalized [1, 3, H, W] tensor.
pub fn to_rgb_tensor(
    img: &DynamicImage,
    height: u32,
    width: u32,
) -> Result<Array4<f32>, SolverError> {
    let rgb: RgbImage =
        image::imageops::resize(&img.to_rgb8(), width, height, FilterType::Lanczos3);

    let mut tensor = Array4::<f32>::zeros((1, 3, height as usize, width as usize));
    for y in 0..height as usize {
        for x in 0..width as usize {
            let pixel = rgb.get_pixel(x as u32, y as u32).0;
            for c in 0..3 {
                tensor[[0, c, y, x]] =
                    (pixel[c] as f32 / 255.0 - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
            }
        }
    }

    Ok(tensor)
}

/// Split an image into a grid of tiles and return each as an RGB tensor.
pub fn split_into_tiles(
    img: &DynamicImage,
    rows: u32,
    cols: u32,
    tile_height: u32,
    tile_width: u32,
) -> Result<Vec<Array4<f32>>, SolverError> {
    let resized = img.resize_exact(cols * tile_width, rows * tile_height, FilterType::Lanczos3);

    let mut tiles = Vec::with_capacity((rows * cols) as usize);
    for row in 0..rows {
        for col in 0..cols {
            let tile =
                resized.crop_imm(col * tile_width, row * tile_height, tile_width, tile_height);
            let tensor = to_rgb_tensor(&tile, tile_height, tile_width)?;
            tiles.push(tensor);
        }
    }

    Ok(tiles)
}

/// Compute horizontal edge energy map using Sobel-X filter.
/// Returns a 1D vector of column-wise edge sums for slot detection.
pub fn horizontal_edge_profile(img: &DynamicImage) -> Vec<f32> {
    let gray = img.to_luma8();
    let (width, height) = gray.dimensions();
    let mut profile = vec![0.0f32; width as usize];

    for y in 1..(height - 1) {
        for x in 1..(width - 1) {
            // Sobel-X kernel
            let gx = -(gray.get_pixel(x - 1, y - 1).0[0] as f32)
                + (gray.get_pixel(x + 1, y - 1).0[0] as f32)
                - 2.0 * (gray.get_pixel(x - 1, y).0[0] as f32)
                + 2.0 * (gray.get_pixel(x + 1, y).0[0] as f32)
                - (gray.get_pixel(x - 1, y + 1).0[0] as f32)
                + (gray.get_pixel(x + 1, y + 1).0[0] as f32);

            profile[x as usize] += gx.abs();
        }
    }

    profile
}

/// Find the peak position in an edge profile (slot location for slider puzzles).
/// Ignores the leftmost region where the puzzle piece starts.
pub fn find_slot_offset(profile: &[f32], skip_left_fraction: f32) -> i32 {
    let skip = (profile.len() as f32 * skip_left_fraction) as usize;
    let search_region = &profile[skip..];

    let max_idx = search_region
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(idx, _)| idx)
        .unwrap_or(0);

    (skip + max_idx) as i32
}
