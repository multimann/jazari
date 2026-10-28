//! Instance filters (SWF 8 filters and Flash's Adjust Color).
//!
//! Units follow XFL: blur and distance in pixels, angles in degrees, strength
//! as a ratio (1 = 100 %).

use super::{GradientStop, Rgba};

/// A filter on a symbol instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// `isEnabled`: a disabled filter is kept but not published.
    pub enabled: bool,
    /// The filter and its parameters.
    pub kind: FilterKind,
}

/// The filter types.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterKind {
    /// `DropShadowFilter`.
    DropShadow(DropShadowFilter),
    /// `BlurFilter`.
    Blur(BlurFilter),
    /// `GlowFilter`.
    Glow(GlowFilter),
    /// `BevelFilter`.
    Bevel(BevelFilter),
    /// `GradientGlowFilter`.
    GradientGlow(GradientFilter),
    /// `GradientBevelFilter`.
    GradientBevel(GradientFilter),
    /// `AdjustColorFilter`: published as a colour matrix.
    AdjustColor(AdjustColorFilter),
    /// `ColorMatrixFilter`.
    ColorMatrix(ColorMatrixFilter),
    /// `ConvolutionFilter`.
    Convolution(ConvolutionFilter),
}

/// Drop shadow parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropShadowFilter {
    /// Horizontal blur in pixels.
    pub blur_x: f64,
    /// Vertical blur in pixels.
    pub blur_y: f64,
    /// Blur passes, 1..=3.
    pub quality: u8,
    /// Direction in degrees.
    pub angle: f64,
    /// Offset in pixels.
    pub distance: f64,
    /// Strength ratio.
    pub strength: f64,
    /// Shadow colour with alpha.
    pub color: Rgba,
    /// Knock out the object.
    pub knockout: bool,
    /// Inner shadow.
    pub inner: bool,
    /// Hide the object, show only the shadow.
    pub hide_object: bool,
}

impl Default for DropShadowFilter {
    fn default() -> Self {
        Self {
            blur_x: 5.0,
            blur_y: 5.0,
            quality: 1,
            angle: 45.0,
            distance: 5.0,
            strength: 1.0,
            color: Rgba::BLACK,
            knockout: false,
            inner: false,
            hide_object: false,
        }
    }
}

/// Blur parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlurFilter {
    /// Horizontal blur in pixels.
    pub blur_x: f64,
    /// Vertical blur in pixels.
    pub blur_y: f64,
    /// Blur passes, 1..=3.
    pub quality: u8,
}

impl Default for BlurFilter {
    fn default() -> Self {
        Self {
            blur_x: 5.0,
            blur_y: 5.0,
            quality: 1,
        }
    }
}

/// Glow parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlowFilter {
    /// Horizontal blur in pixels.
    pub blur_x: f64,
    /// Vertical blur in pixels.
    pub blur_y: f64,
    /// Blur passes, 1..=3.
    pub quality: u8,
    /// Strength ratio.
    pub strength: f64,
    /// Glow colour with alpha. The default is disputed (`#000000` in the
    /// schema); `#FF0000` is used until a fixture decides.
    pub color: Rgba,
    /// Knock out the object.
    pub knockout: bool,
    /// Inner glow.
    pub inner: bool,
}

impl Default for GlowFilter {
    fn default() -> Self {
        Self {
            blur_x: 5.0,
            blur_y: 5.0,
            quality: 1,
            strength: 1.0,
            color: Rgba::opaque(255, 0, 0),
            knockout: false,
            inner: false,
        }
    }
}

/// Where a bevel is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BevelPlacement {
    /// Inside the object.
    #[default]
    Inner,
    /// Outside the object.
    Outer,
    /// Both.
    Full,
}

/// Bevel parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BevelFilter {
    /// Horizontal blur in pixels.
    pub blur_x: f64,
    /// Vertical blur in pixels.
    pub blur_y: f64,
    /// Blur passes, 1..=3.
    pub quality: u8,
    /// Light direction in degrees.
    pub angle: f64,
    /// Offset in pixels.
    pub distance: f64,
    /// Strength ratio.
    pub strength: f64,
    /// Shadow colour with alpha.
    pub shadow_color: Rgba,
    /// Highlight colour with alpha.
    pub highlight_color: Rgba,
    /// Knock out the object.
    pub knockout: bool,
    /// `type`: inner, outer or full.
    pub placement: BevelPlacement,
}

impl Default for BevelFilter {
    fn default() -> Self {
        Self {
            blur_x: 5.0,
            blur_y: 5.0,
            quality: 1,
            angle: 45.0,
            distance: 5.0,
            strength: 1.0,
            shadow_color: Rgba::BLACK,
            highlight_color: Rgba::WHITE,
            knockout: false,
            placement: BevelPlacement::Inner,
        }
    }
}

/// Gradient glow and gradient bevel parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientFilter {
    /// Horizontal blur in pixels.
    pub blur_x: f64,
    /// Vertical blur in pixels.
    pub blur_y: f64,
    /// Blur passes, 1..=3.
    pub quality: u8,
    /// Direction in degrees.
    pub angle: f64,
    /// Offset in pixels.
    pub distance: f64,
    /// Strength ratio.
    pub strength: f64,
    /// Colour stops (`GradientEntry` children).
    pub stops: Vec<GradientStop>,
    /// Knock out the object.
    pub knockout: bool,
    /// `type`: inner, outer or full.
    pub placement: BevelPlacement,
}

impl Default for GradientFilter {
    fn default() -> Self {
        Self {
            blur_x: 5.0,
            blur_y: 5.0,
            quality: 1,
            angle: 45.0,
            distance: 5.0,
            strength: 1.0,
            stops: Vec::new(),
            knockout: false,
            placement: BevelPlacement::Inner,
        }
    }
}

/// Flash's Adjust Color filter, before conversion to a colour matrix.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AdjustColorFilter {
    /// −100..100.
    pub brightness: f64,
    /// −100..100.
    pub contrast: f64,
    /// −100..100.
    pub saturation: f64,
    /// −180..180 degrees.
    pub hue: f64,
}

/// A 4 × 5 colour matrix, row-major (R, G, B, A rows; the fifth column is an
/// offset in 0..255 units).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorMatrixFilter {
    /// The 20 coefficients.
    pub matrix: [f64; 20],
}

impl Default for ColorMatrixFilter {
    /// The identity matrix.
    fn default() -> Self {
        let mut matrix = [0.0; 20];
        for row in 0..4 {
            matrix[row * 5 + row] = 1.0;
        }
        Self { matrix }
    }
}

/// Convolution parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvolutionFilter {
    /// Matrix columns.
    pub matrix_x: u8,
    /// Matrix rows.
    pub matrix_y: u8,
    /// `matrix_x × matrix_y` coefficients, row-major.
    pub matrix: Vec<f64>,
    /// Divisor.
    pub divisor: f64,
    /// Bias.
    pub bias: f64,
    /// Leave alpha unchanged.
    pub preserve_alpha: bool,
    /// Clamp edge pixels instead of using `color`.
    pub clamp: bool,
    /// Colour used outside the image when not clamping.
    pub color: Rgba,
}

impl Default for ConvolutionFilter {
    fn default() -> Self {
        Self {
            matrix_x: 3,
            matrix_y: 3,
            matrix: vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            divisor: 1.0,
            bias: 0.0,
            preserve_alpha: true,
            clamp: true,
            color: Rgba::TRANSPARENT,
        }
    }
}
