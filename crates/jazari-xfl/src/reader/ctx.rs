//! Reading context: the import report, the current location, and attribute
//! accessors that record a warning when a value cannot be read.

use crate::model::{Matrix, Pt, Rgba};
use crate::{ImportReport, Location, WarningKind};

use super::values::{excerpt, parse_bool, parse_color, parse_f64, parse_u32};
use super::xml::Node;

/// State shared by every reading function.
#[derive(Debug, Default)]
pub(crate) struct Ctx {
    /// Warnings collected so far.
    pub report: ImportReport,
    /// Where the reader currently is; warnings are recorded here.
    pub loc: Location,
}

impl Ctx {
    /// Records a warning at the current location.
    pub fn warn(&mut self, kind: WarningKind, message: impl Into<String>) {
        self.report.warn(kind, message, self.loc.clone());
    }

    /// Runs `f` with `index` appended to the element path of the location.
    pub fn with_element<R>(&mut self, index: usize, f: impl FnOnce(&mut Self) -> R) -> R {
        self.loc.element.push(index);
        let result = f(self);
        self.loc.element.pop();
        result
    }

    /// Records that an attribute value could not be read.
    fn bad_value(&mut self, node: &Node, name: &str, value: &str) {
        self.warn(
            WarningKind::BadValue,
            format!(
                "<{}> {name}=\"{}\" cannot be read; the default is used",
                node.name,
                excerpt(value)
            ),
        );
    }

    /// A number attribute, or `None` when absent or unreadable (with a
    /// warning in the second case).
    pub fn opt_f64(&mut self, node: &Node, name: &str) -> Option<f64> {
        let value = node.attr(name)?;
        let parsed = parse_f64(value);
        if parsed.is_none() {
            self.bad_value(node, name, value);
        }
        parsed
    }

    /// A number attribute with a default.
    pub fn f64(&mut self, node: &Node, name: &str, default: f64) -> f64 {
        self.opt_f64(node, name).unwrap_or(default)
    }

    /// A non-negative whole-number attribute, or `None`.
    pub fn opt_u32(&mut self, node: &Node, name: &str) -> Option<u32> {
        let value = node.attr(name)?;
        let parsed = parse_u32(value);
        if parsed.is_none() {
            self.bad_value(node, name, value);
        }
        parsed
    }

    /// A non-negative whole-number attribute with a default.
    pub fn u32(&mut self, node: &Node, name: &str, default: u32) -> u32 {
        self.opt_u32(node, name).unwrap_or(default)
    }

    /// A boolean attribute with a default.
    pub fn bool(&mut self, node: &Node, name: &str, default: bool) -> bool {
        match node.attr(name) {
            None => default,
            Some(value) => parse_bool(value).unwrap_or_else(|| {
                self.bad_value(node, name, value);
                default
            }),
        }
    }

    /// An opaque `#RRGGBB` colour attribute with a default.
    pub fn color(&mut self, node: &Node, name: &str, default: Rgba) -> Rgba {
        match node.attr(name) {
            None => default,
            Some(value) => parse_color(value).unwrap_or_else(|| {
                self.bad_value(node, name, value);
                default
            }),
        }
    }

    /// A colour attribute combined with an alpha-ratio attribute (0..1,
    /// default 1), as XFL writes `color="#336699" alpha="0.5"`.
    pub fn color_alpha(&mut self, node: &Node, color: &str, alpha: &str, default: Rgba) -> Rgba {
        let rgb = self.color(node, color, default);
        let ratio = self.f64(node, alpha, 1.0);
        rgb.with_alpha_ratio(ratio)
    }

    /// A string attribute, `None` when absent or empty.
    pub fn text(node: &Node, name: &str) -> Option<String> {
        node.attr(name).filter(|v| !v.is_empty()).map(str::to_owned)
    }

    /// The `<matrix><Matrix .../></matrix>` child of `node`, or `None` when
    /// there is no `<matrix>`. Missing components take the identity values.
    pub fn matrix(&mut self, node: &Node) -> Option<Matrix> {
        let container = node.child("matrix")?;
        let Some(m) = container.child("Matrix") else {
            return Some(Matrix::IDENTITY);
        };
        Some(Matrix {
            a: self.f64(m, "a", 1.0),
            b: self.f64(m, "b", 0.0),
            c: self.f64(m, "c", 0.0),
            d: self.f64(m, "d", 1.0),
            tx: self.f64(m, "tx", 0.0),
            ty: self.f64(m, "ty", 0.0),
        })
    }

    /// The `<transformationPoint><Point x y/></transformationPoint>` child, or
    /// the origin.
    pub fn transformation_point(&mut self, node: &Node) -> Pt {
        match node
            .child("transformationPoint")
            .and_then(|t| t.child("Point"))
        {
            Some(p) => Pt::new(self.f64(p, "x", 0.0), self.f64(p, "y", 0.0)),
            None => Pt::ORIGIN,
        }
    }

    /// Warns about every child of `node` whose name is not in `known`.
    pub fn check_children(&mut self, node: &Node, known: &[&str]) {
        for child in &node.children {
            if !known.contains(&child.name.as_str()) {
                self.warn(
                    WarningKind::UnknownElement,
                    format!(
                        "unknown <{}> inside <{}> was skipped",
                        child.name, node.name
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    #[test]
    fn bad_values_fall_back_to_defaults_with_a_warning() {
        let n = parse_str(r##"<X w="12.5" h="abc" ok="true" bad="maybe" c="#00FF00" a="0.5"/>"##)
            .unwrap();
        let mut ctx = Ctx::default();
        assert_eq!(ctx.f64(&n, "w", 1.0), 12.5);
        assert_eq!(ctx.f64(&n, "h", 1.0), 1.0);
        assert_eq!(ctx.f64(&n, "missing", 7.0), 7.0);
        assert!(ctx.bool(&n, "ok", false));
        assert!(!ctx.bool(&n, "bad", false));
        assert_eq!(
            ctx.color_alpha(&n, "c", "a", Rgba::BLACK),
            Rgba::new(0, 255, 0, 128)
        );
        assert_eq!(ctx.report.len(), 2);
        assert!(ctx.report.warnings[0].message.contains("h=\"abc\""));
    }

    #[test]
    fn matrix_defaults_to_identity_components() {
        let n = parse_str(r#"<E><matrix><Matrix tx="10" d="2"/></matrix></E>"#).unwrap();
        let mut ctx = Ctx::default();
        assert_eq!(
            ctx.matrix(&n),
            Some(Matrix::new(1.0, 0.0, 0.0, 2.0, 10.0, 0.0))
        );
        let none = parse_str("<E/>").unwrap();
        assert_eq!(ctx.matrix(&none), None);
    }
}
