//! Pixel comparison of two rendered frames, the diff image and the
//! side-by-side composite (ours | reference | diff).

use image::{Rgba, RgbaImage};

/// The result of comparing two images.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelDiff {
    /// Pixels whose largest channel difference exceeds the threshold.
    pub differing: u64,
    /// Pixels compared: the area of the larger image in each direction.
    pub total: u64,
    /// The largest channel difference anywhere (0..=255).
    pub max_diff: u8,
    /// The threshold that was used.
    pub threshold: u8,
    /// Whether the two images had different sizes (pixels outside the
    /// smaller one count as differing by 255).
    pub size_mismatch: bool,
    /// Pixels inside ignored regions (not counted in `total`).
    pub ignored: u64,
}

/// A rectangle of pixels left out of a comparison: `x`, `y`, `width`,
/// `height` in image pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

impl Region {
    /// Whether the pixel at (`px`, `py`) is inside.
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && py >= self.y && px - self.x < self.width && py - self.y < self.height
    }

    /// The region scaled by `scale` (outward, so it still covers the
    /// same content).
    pub fn scaled(&self, scale: f32) -> Region {
        let s = f64::from(scale.max(0.0));
        let x0 = (f64::from(self.x) * s).floor();
        let y0 = (f64::from(self.y) * s).floor();
        let x1 = (f64::from(self.x.saturating_add(self.width)) * s).ceil();
        let y1 = (f64::from(self.y.saturating_add(self.height)) * s).ceil();
        Region {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0).max(0.0) as u32,
            height: (y1 - y0).max(0.0) as u32,
        }
    }
}

impl PixelDiff {
    /// The share of differing pixels, 0..=1.
    pub fn share(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.differing as f64 / self.total as f64
        }
    }
}

/// Grey of the separators between the panels of the composite.
const SEPARATOR: Rgba<u8> = Rgba([64, 64, 64, 255]);
/// Width of the separators in pixels.
const GAP: u32 = 4;

/// Compares `ours` with `reference` pixel by pixel. A pixel differs when
/// its largest channel difference (R, G, B or A) exceeds `threshold`.
///
/// The returned image shows the reference faded to light grey, pixels that
/// differ by at most `threshold` in light orange, and pixels beyond it in
/// red (brighter for larger differences).
pub fn compare(ours: &RgbaImage, reference: &RgbaImage, threshold: u8) -> (PixelDiff, RgbaImage) {
    compare_except(ours, reference, threshold, &[])
}

/// [`compare`], leaving the pixels inside `ignore` out of the counts. The
/// diff image shows them tinted blue (differences beyond the threshold in
/// strong blue), so a reviewer still sees what was left out.
pub fn compare_except(
    ours: &RgbaImage,
    reference: &RgbaImage,
    threshold: u8,
    ignore: &[Region],
) -> (PixelDiff, RgbaImage) {
    let width = ours.width().max(reference.width());
    let height = ours.height().max(reference.height());
    let mut diff_image = RgbaImage::new(width, height);
    let mut differing = 0u64;
    let mut ignored = 0u64;
    let mut max_diff = 0u8;
    for y in 0..height {
        for x in 0..width {
            let a = pixel(ours, x, y);
            let b = pixel(reference, x, y);
            let d = match (a, b) {
                (Some(a), Some(b)) => channel_diff(a, b),
                _ => 255,
            };
            if ignore.iter().any(|r| r.contains(x, y)) {
                ignored += 1;
                diff_image.put_pixel(x, y, ignored_colour(b.or(a), d, threshold));
                continue;
            }
            max_diff = max_diff.max(d);
            if d > threshold {
                differing += 1;
            }
            diff_image.put_pixel(x, y, diff_colour(b.or(a), d, threshold));
        }
    }
    let result = PixelDiff {
        differing,
        total: (u64::from(width) * u64::from(height)).saturating_sub(ignored),
        max_diff,
        threshold,
        size_mismatch: ours.dimensions() != reference.dimensions(),
        ignored,
    };
    (result, diff_image)
}

/// The diff colour of an ignored pixel: blue for a difference beyond the
/// threshold, else the faded base tinted blue.
fn ignored_colour(base: Option<Rgba<u8>>, d: u8, threshold: u8) -> Rgba<u8> {
    if d > threshold {
        return Rgba([0, 0, 128 + d / 2, 255]);
    }
    let Rgba([r, g, _, _]) = diff_colour(base, d, threshold);
    Rgba([r.saturating_sub(40), g.saturating_sub(24), 255, 255])
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> Option<Rgba<u8>> {
    if x < image.width() && y < image.height() {
        Some(*image.get_pixel(x, y))
    } else {
        None
    }
}

/// The largest absolute difference over the four channels.
pub fn channel_diff(a: Rgba<u8>, b: Rgba<u8>) -> u8 {
    a.0.iter()
        .zip(b.0.iter())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

fn diff_colour(base: Option<Rgba<u8>>, d: u8, threshold: u8) -> Rgba<u8> {
    if d > threshold {
        // 128..=255 red, so even a small excess is clearly visible.
        let r = 128 + d / 2;
        return Rgba([r, 0, 0, 255]);
    }
    if d > 0 {
        return Rgba([255, 200, 150, 255]);
    }
    let [r, g, b, _] = base.map(|p| p.0).unwrap_or([255, 255, 255, 255]);
    let lum = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
    // Fade to 192..=255 so differences stand out against it.
    let faded = 192 + (lum / 4) as u8;
    Rgba([faded, faded, faded, 255])
}

/// Puts the images next to each other, left to right, separated by grey
/// bars. Smaller images are padded with the separator colour.
pub fn side_by_side(images: &[&RgbaImage]) -> RgbaImage {
    let height = images.iter().map(|i| i.height()).max().unwrap_or(1).max(1);
    let width = images.iter().map(|i| i.width()).sum::<u32>()
        + GAP * (images.len().saturating_sub(1) as u32);
    let mut out = RgbaImage::from_pixel(width.max(1), height, SEPARATOR);
    let mut x0 = 0;
    for image in images {
        for (x, y, p) in image.enumerate_pixels() {
            out.put_pixel(x0 + x, y, *p);
        }
        x0 += image.width() + GAP;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_images_do_not_differ() {
        let a = RgbaImage::from_pixel(4, 3, Rgba([10, 20, 30, 255]));
        let (d, img) = compare(&a, &a, 0);
        assert_eq!(d.differing, 0);
        assert_eq!(d.max_diff, 0);
        assert_eq!(d.total, 12);
        assert_eq!(img.dimensions(), (4, 3));
    }

    #[test]
    fn threshold_and_largest_channel() {
        let a = RgbaImage::from_pixel(2, 1, Rgba([100, 100, 100, 255]));
        let mut b = a.clone();
        b.put_pixel(0, 0, Rgba([100, 110, 95, 255]));
        b.put_pixel(1, 0, Rgba([100, 104, 100, 255]));
        let (d, img) = compare(&a, &b, 5);
        assert_eq!(d.max_diff, 10);
        assert_eq!(d.differing, 1);
        assert!((d.share() - 0.5).abs() < 1e-9);
        assert_eq!(img.get_pixel(0, 0).0[1], 0, "beyond threshold is red");
        assert_eq!(*img.get_pixel(1, 0), Rgba([255, 200, 150, 255]));
    }

    #[test]
    fn size_mismatch_counts_missing_pixels() {
        let a = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        let b = RgbaImage::from_pixel(1, 2, Rgba([0, 0, 0, 255]));
        let (d, _) = compare(&a, &b, 16);
        assert!(d.size_mismatch);
        assert_eq!(d.differing, 2);
        assert_eq!(d.max_diff, 255);
    }

    #[test]
    fn composite_layout() {
        let a = RgbaImage::from_pixel(3, 2, Rgba([1, 1, 1, 255]));
        let b = RgbaImage::from_pixel(2, 3, Rgba([2, 2, 2, 255]));
        let c = side_by_side(&[&a, &b]);
        assert_eq!(c.dimensions(), (3 + GAP + 2, 3));
        assert_eq!(*c.get_pixel(0, 0), Rgba([1, 1, 1, 255]));
        assert_eq!(*c.get_pixel(3 + GAP, 2), Rgba([2, 2, 2, 255]));
        assert_eq!(*c.get_pixel(0, 2), SEPARATOR);
    }
}
