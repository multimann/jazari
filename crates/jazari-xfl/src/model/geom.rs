//! Geometry and colour primitives shared by the whole model.

use std::fmt;
use std::ops::Mul;

/// A point.
///
/// Its unit depends on where it is used: pixels in most of the model, twips in
/// shape edges ([`super::PathCmd`]) and em units in glyph outlines.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pt {
    /// Horizontal coordinate (grows to the right).
    pub x: f64,
    /// Vertical coordinate (grows downwards).
    pub y: f64,
}

impl Pt {
    /// The point (0, 0).
    pub const ORIGIN: Pt = Pt { x: 0.0, y: 0.0 };

    /// Creates a point.
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub left: f64,
    /// Top edge.
    pub top: f64,
    /// Right edge.
    pub right: f64,
    /// Bottom edge.
    pub bottom: f64,
}

impl Rect {
    /// Creates a rectangle from its edges.
    pub const fn new(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Width (`right - left`); negative if the rectangle is inverted.
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    /// Height (`bottom - top`); negative if the rectangle is inverted.
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// An 8-bit colour with straight (not premultiplied) alpha.
///
/// The default is opaque black, the XFL default for fills and text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha: 0 is transparent, 255 opaque.
    pub a: u8,
}

impl Rgba {
    /// Opaque black.
    pub const BLACK: Rgba = Rgba::opaque(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Rgba = Rgba::opaque(255, 255, 255);
    /// Fully transparent black.
    pub const TRANSPARENT: Rgba = Rgba::new(0, 0, 0, 0);

    /// Creates a colour from its four components.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates an opaque colour.
    pub const fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    /// Returns this colour with alpha set from a 0..1 ratio (the XFL `alpha`
    /// attribute). Values outside 0..1 are clamped; NaN gives 0.
    pub fn with_alpha_ratio(self, alpha: f64) -> Self {
        let a = if alpha.is_nan() {
            0.0
        } else {
            (alpha.clamp(0.0, 1.0) * 255.0).round()
        };
        // `a` is in 0..=255 after the clamp, so the cast cannot truncate.
        Self { a: a as u8, ..self }
    }

    /// Alpha as a 0..1 ratio.
    pub fn alpha_ratio(&self) -> f64 {
        f64::from(self.a) / 255.0
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Self::BLACK
    }
}

impl fmt::Display for Rgba {
    /// Formats as `#RRGGBB`, followed by `/alpha` when not opaque.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "/{}", self.a)?;
        }
        Ok(())
    }
}

/// A 2D affine matrix in the Flash convention.
///
/// A point maps as `x' = a·x + c·y + tx` and `y' = b·x + d·y + ty`, that is
/// the column-vector matrix `[a c tx; b d ty; 0 0 1]`. `a..d` are unitless and
/// `tx`/`ty` are pixels (XFL stores them that way; SWF stores twips). All
/// maths is done in `f64`; rounding to twips and Fixed16 happens once, when
/// publishing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    /// X scale and rotation component.
    pub a: f64,
    /// Y skew and rotation component.
    pub b: f64,
    /// X skew and rotation component.
    pub c: f64,
    /// Y scale and rotation component.
    pub d: f64,
    /// X translation in pixels.
    pub tx: f64,
    /// Y translation in pixels.
    pub ty: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Matrix {
    /// The identity matrix (the XFL default when `<matrix>` is absent).
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// Creates a matrix from its six components.
    pub const fn new(a: f64, b: f64, c: f64, d: f64, tx: f64, ty: f64) -> Self {
        Self { a, b, c, d, tx, ty }
    }

    /// A translation.
    pub const fn translate(tx: f64, ty: f64) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    /// A scale about the origin.
    pub const fn scale(sx: f64, sy: f64) -> Self {
        Self::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    /// A rotation about the origin. Positive angles turn clockwise on screen,
    /// because the y axis points down.
    pub fn rotate(radians: f64) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::new(cos, sin, -sin, cos, 0.0, 0.0)
    }

    /// Returns `self × rhs`: the transform that applies `rhs` first and then
    /// `self`. For a child placed inside a parent,
    /// `world = parent.multiply(&child)`. Also available as `self * rhs`.
    pub fn multiply(&self, rhs: &Matrix) -> Matrix {
        Matrix {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            tx: self.a * rhs.tx + self.c * rhs.ty + self.tx,
            ty: self.b * rhs.tx + self.d * rhs.ty + self.ty,
        }
    }

    /// Determinant of the linear part (`a·d − b·c`). Negative for a mirrored
    /// transform, zero for one that collapses to a line or a point.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// The inverse, or `None` when the matrix is singular or not finite.
    pub fn invert(&self) -> Option<Matrix> {
        let det = self.determinant();
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let a = self.d / det;
        let b = -self.b / det;
        let c = -self.c / det;
        let d = self.a / det;
        let inverse = Matrix {
            a,
            b,
            c,
            d,
            tx: -(a * self.tx + c * self.ty),
            ty: -(b * self.tx + d * self.ty),
        };
        inverse.is_finite().then_some(inverse)
    }

    /// Maps a point through this matrix.
    pub fn transform_point(&self, p: Pt) -> Pt {
        Pt {
            x: self.a * p.x + self.c * p.y + self.tx,
            y: self.b * p.x + self.d * p.y + self.ty,
        }
    }

    /// Whether all six components are finite.
    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|v| v.is_finite())
    }

    /// Whether every component differs from `other`'s by at most `eps`.
    pub fn approx_eq(&self, other: &Matrix, eps: f64) -> bool {
        let lhs = [self.a, self.b, self.c, self.d, self.tx, self.ty];
        let rhs = [other.a, other.b, other.c, other.d, other.tx, other.ty];
        lhs.iter()
            .zip(rhs.iter())
            .all(|(l, r)| (l - r).abs() <= eps)
    }

    /// Splits the matrix into scales, skew angles and translation, the
    /// quantities a classic tween interpolates (see [`Decomposed`]).
    ///
    /// A mirrored matrix (negative determinant) gets a negative `scale_x`, so
    /// that `skew_x` and `skew_y` stay close and a mirrored instance keeps
    /// turning like a rotation. [`Decomposed::recompose`] gives back the
    /// original matrix up to floating-point rounding.
    pub fn decompose(&self) -> Decomposed {
        let mut scale_x = self.a.hypot(self.b);
        let scale_y = self.c.hypot(self.d);
        let skew_x = (-self.c).atan2(self.d);
        let skew_y = if self.determinant() < 0.0 {
            scale_x = -scale_x;
            (-self.b).atan2(-self.a)
        } else {
            self.b.atan2(self.a)
        };
        Decomposed {
            scale_x,
            scale_y,
            skew_x,
            skew_y,
            tx: self.tx,
            ty: self.ty,
        }
    }
}

impl Mul for Matrix {
    type Output = Matrix;

    /// Same as [`Matrix::multiply`]: `rhs` applies first.
    fn mul(self, rhs: Matrix) -> Matrix {
        self.multiply(&rhs)
    }
}

/// A matrix split into the components Flash tweens: scales, skew angles and
/// translation.
///
/// The relation to the matrix is
/// `a = scale_x·cos(skew_y)`, `b = scale_x·sin(skew_y)`,
/// `c = −scale_y·sin(skew_x)`, `d = scale_y·cos(skew_x)`.
/// When `skew_x == skew_y` the transform is a plain rotation by that angle
/// plus scaling. Angles are radians, in −π..π as returned by
/// [`Matrix::decompose`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Decomposed {
    /// Scale along the (skewed) x axis; negative when mirrored.
    pub scale_x: f64,
    /// Scale along the (skewed) y axis.
    pub scale_y: f64,
    /// Angle of the y axis from vertical, in radians.
    pub skew_x: f64,
    /// Angle of the x axis from horizontal, in radians (the rotation).
    pub skew_y: f64,
    /// X translation in pixels.
    pub tx: f64,
    /// Y translation in pixels.
    pub ty: f64,
}

impl Decomposed {
    /// Builds the matrix back from its components.
    pub fn recompose(&self) -> Matrix {
        let (sin_y, cos_y) = self.skew_y.sin_cos();
        let (sin_x, cos_x) = self.skew_x.sin_cos();
        Matrix {
            a: self.scale_x * cos_y,
            b: self.scale_x * sin_y,
            c: -self.scale_y * sin_x,
            d: self.scale_y * cos_x,
            tx: self.tx,
            ty: self.ty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_6, PI};

    const EPS: f64 = 1e-9;

    fn assert_pt(p: Pt, x: f64, y: f64) {
        assert!(
            (p.x - x).abs() < EPS && (p.y - y).abs() < EPS,
            "{p:?} != ({x}, {y})"
        );
    }

    #[test]
    fn identity_is_neutral() {
        let m = Matrix::new(2.0, 0.5, -0.25, 3.0, 10.0, -4.0);
        assert_eq!(Matrix::IDENTITY.multiply(&m), m);
        assert_eq!(m.multiply(&Matrix::IDENTITY), m);
        assert_eq!(Matrix::default(), Matrix::IDENTITY);
    }

    #[test]
    fn multiply_applies_rhs_first() {
        let m = Matrix::translate(10.0, 0.0) * Matrix::scale(2.0, 3.0);
        assert_pt(m.transform_point(Pt::new(1.0, 1.0)), 12.0, 3.0);
        let n = Matrix::scale(2.0, 3.0) * Matrix::translate(10.0, 0.0);
        assert_pt(n.transform_point(Pt::new(1.0, 1.0)), 22.0, 3.0);
    }

    #[test]
    fn rotation_is_clockwise_on_screen() {
        let m = Matrix::rotate(FRAC_PI_2);
        assert_pt(m.transform_point(Pt::new(1.0, 0.0)), 0.0, 1.0);
    }

    #[test]
    fn invert_round_trips() {
        let m = Matrix::new(1.5, 0.3, -0.7, 0.8, 120.0, -35.5);
        let inv = m.invert().expect("invertible");
        assert!(m.multiply(&inv).approx_eq(&Matrix::IDENTITY, EPS));
        assert!(inv.multiply(&m).approx_eq(&Matrix::IDENTITY, EPS));
        let p = Pt::new(7.0, -3.0);
        let back = inv.transform_point(m.transform_point(p));
        assert_pt(back, p.x, p.y);
    }

    #[test]
    fn singular_and_non_finite_matrices_do_not_invert() {
        assert_eq!(Matrix::scale(0.0, 1.0).invert(), None);
        assert_eq!(Matrix::new(1.0, 2.0, 2.0, 4.0, 0.0, 0.0).invert(), None);
        assert_eq!(
            Matrix::new(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0).invert(),
            None
        );
    }

    #[test]
    fn decompose_rotation_and_scale() {
        let m = Matrix::translate(5.0, 6.0) * Matrix::rotate(FRAC_PI_6) * Matrix::scale(2.0, 0.5);
        let d = m.decompose();
        assert!((d.scale_x - 2.0).abs() < EPS);
        assert!((d.scale_y - 0.5).abs() < EPS);
        assert!((d.skew_x - FRAC_PI_6).abs() < EPS);
        assert!((d.skew_y - FRAC_PI_6).abs() < EPS);
        assert!((d.tx - 5.0).abs() < EPS && (d.ty - 6.0).abs() < EPS);
        assert!(d.recompose().approx_eq(&m, EPS));
    }

    #[test]
    fn decompose_mirror_gives_negative_x_scale() {
        let d = Matrix::scale(-1.0, 1.0).decompose();
        assert!((d.scale_x + 1.0).abs() < EPS);
        assert!((d.scale_y - 1.0).abs() < EPS);
        assert!(d.skew_x.abs() < EPS && d.skew_y.abs() < EPS);

        let mirrored_turn = Matrix::rotate(0.4) * Matrix::scale(-2.0, 1.0);
        let d = mirrored_turn.decompose();
        assert!(d.scale_x < 0.0);
        assert!((d.skew_x - d.skew_y).abs() < EPS);
        assert!(d.recompose().approx_eq(&mirrored_turn, EPS));
    }

    #[test]
    fn decompose_recompose_round_trips() {
        let cases = [
            Matrix::IDENTITY,
            Matrix::new(1.0, 0.0, 0.5, 1.0, 0.0, 0.0),
            Matrix::new(0.2, 1.3, -0.9, 0.4, -12.5, 99.0),
            Matrix::new(-0.7, 0.1, 0.3, -1.2, 3.0, 4.0),
            Matrix::new(0.0, 0.0, 0.0, 1.0, 1.0, 2.0),
            Matrix::rotate(PI),
            Matrix::scale(0.05, 0.05),
        ];
        for m in cases {
            let back = m.decompose().recompose();
            assert!(back.approx_eq(&m, EPS), "{m:?} -> {back:?}");
        }
    }

    #[test]
    fn rgba_alpha_ratio() {
        assert_eq!(Rgba::WHITE.with_alpha_ratio(0.5).a, 128);
        assert_eq!(Rgba::WHITE.with_alpha_ratio(2.0).a, 255);
        assert_eq!(Rgba::WHITE.with_alpha_ratio(f64::NAN).a, 0);
        assert_eq!(Rgba::new(255, 0, 16, 255).to_string(), "#FF0010");
        assert_eq!(Rgba::new(255, 0, 16, 10).to_string(), "#FF0010/10");
    }
}
