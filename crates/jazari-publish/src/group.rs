//! Group flattening: `DOMGroup` members are lifted into the parent's element
//! list, raw shapes first.
//!
//! # Matrix semantics (settled by the `groups` fixture)
//!
//! A group member's `<matrix>` is **absolute**: it already includes every
//! enclosing group transform and maps the member's local coordinates straight
//! into the space of the timeline that holds the outermost group. JPEXS never
//! writes a `DOMGroup` matrix and always writes member matrices; applied as
//! they are, they reproduce the edge bounds Flash 8 baked into the SWF to
//! within one twip, and the instance matrices equal Flash's PlaceObject2
//! matrices (`tests/fixtures/MANIFEST.md`, groups). So member matrices are
//! **never** multiplied by group matrices.
//!
//! fla-viewer's rule for other writers is the same for members that have a
//! matrix, while a member *without* one inherits the composed matrix of its
//! enclosing groups. The model cannot tell an absent member matrix from an
//! explicit identity, so [`member_matrix`] approximates that rule: an
//! identity member inside a group that carries a non-identity matrix takes
//! the group's composed matrix. JPEXS groups have no matrix, so this never
//! applies to them.
//!
//! # Baked transforms
//!
//! Flash 8 bakes group and drawing-object transforms into the edges and
//! places the merged shape with the identity matrix. [`bake_shape`] does the
//! same for shapes inside groups and for drawing objects (edges, fill
//! matrices and stroke widths), so the raw content of a keyframe becomes one
//! shape character, as in Flash, and anti-aliasing matches Flash's output.
//! A stroke cannot keep an anisotropic scale once baked, so shapes with
//! scaling strokes under a non-uniform matrix keep their matrix (they are
//! then placed with it, which draws them exactly as authored). Merge-mode
//! shapes at the top of a keyframe keep their matrix (the identity in every
//! Flash-made file): placing them with it draws the same.
//!
//! # Drawing order
//!
//! Flash draws the merge-mode raw shape of a frame or group (a `DOMShape`
//! that is not a drawing object) **underneath** the groups, drawing objects
//! and instances beside it, whatever its position in the XFL (JPEXS lists it
//! last). [`flatten`] therefore moves raw shapes to the back of every element
//! list (keeping their relative order) and keeps document order, back to
//! front, for everything else.

use jazari_as2::Origin;
use jazari_xfl::model::{Element, FillStyle, Matrix, PathCmd, Pt, ScaleMode, Shape};

use crate::context::{Ctx, MAX_NESTING};

/// Returns `elements` with every group replaced by its members, recursively
/// and in drawing order (see the module docs). Groups nested deeper than
/// [`MAX_NESTING`] are reported at `origin` and left out.
pub fn flatten(ctx: &mut Ctx<'_>, elements: &[Element], origin: &Origin) -> Vec<Element> {
    let mut out = Vec::with_capacity(elements.len());
    let mut too_deep = false;
    // Explicit stack of (ordered elements, next index, inherited matrix,
    // inside a group): no recursion, so hostile nesting cannot overflow the
    // stack.
    let mut stack: Vec<(Vec<&Element>, usize, Option<Matrix>, bool)> =
        vec![(drawing_order(elements), 0, None, false)];
    while let Some((list, i, inherited, grouped)) = stack.pop() {
        let Some(&el) = list.get(i) else {
            continue;
        };
        stack.push((list, i + 1, inherited, grouped));
        match el {
            Element::Group(g) => {
                if stack.len() > MAX_NESTING {
                    too_deep = true;
                    continue;
                }
                let m = compose(inherited.as_ref(), g.matrix.as_ref());
                stack.push((drawing_order(&g.members), 0, m, true));
            }
            other => out.push(with_inherited(other, inherited.as_ref(), grouped)),
        }
    }
    if too_deep {
        ctx.warn(
            origin.clone(),
            format!("groups nested deeper than {MAX_NESTING} levels were left out"),
        );
    }
    out
}

/// The elements of one list in drawing order, back to front: merge-mode raw
/// shapes first, then everything else in document order.
pub fn drawing_order(elements: &[Element]) -> Vec<&Element> {
    let (raw, rest): (Vec<&Element>, Vec<&Element>) =
        elements.iter().partition(|e| is_raw_shape(e));
    let mut out = raw;
    out.extend(rest);
    out
}

/// Whether an element is a merge-mode raw shape (drawn under its siblings).
pub fn is_raw_shape(el: &Element) -> bool {
    matches!(el, Element::Shape(s) if !s.is_drawing_object)
}

/// The composed matrix of a group's enclosing groups, used only for members
/// that have no matrix of their own (see the module docs): `inherited ×
/// group`, or `None` while no group on the path carries a matrix.
pub fn compose(inherited: Option<&Matrix>, group: Option<&Matrix>) -> Option<Matrix> {
    match (inherited, group) {
        (Some(p), Some(g)) => Some(p.multiply(g)),
        (Some(p), None) => Some(*p),
        (None, Some(g)) => Some(*g),
        (None, None) => None,
    }
}

/// The matrix a member is published with: its own (absolute) matrix, unless
/// it is the identity inside a transformed group, where it stands for an
/// absent matrix and takes the groups' composed matrix.
pub fn member_matrix(own: &Matrix, inherited: Option<&Matrix>) -> Matrix {
    match inherited {
        Some(m) if *own == Matrix::IDENTITY => *m,
        _ => *own,
    }
}

/// A copy of `el` with its published matrix; grouped shapes and drawing
/// objects are baked when possible.
fn with_inherited(el: &Element, inherited: Option<&Matrix>, grouped: bool) -> Element {
    let mut el = el.clone();
    match &mut el {
        Element::Shape(s) => {
            s.matrix = member_matrix(&s.matrix, inherited);
            if grouped || s.is_drawing_object {
                bake_shape(s);
            }
        }
        Element::Instance(i) => i.matrix = member_matrix(&i.matrix, inherited),
        Element::Bitmap(b) => b.matrix = member_matrix(&b.matrix, inherited),
        Element::Text(t) => t.matrix = member_matrix(&t.matrix, inherited),
        Element::Group(_) | Element::Unsupported { .. } => {}
    }
    el
}

/// Bakes `shape.matrix` into its edges, fill matrices and stroke widths and
/// sets the matrix to the identity, as Flash 8 publishes grouped shapes and
/// drawing objects. Returns `false` (and changes nothing) when the matrix is
/// already the identity, is not finite, or would scale a stroke unevenly.
pub fn bake_shape(shape: &mut Shape) -> bool {
    let m = shape.matrix;
    if m == Matrix::IDENTITY || !m.is_finite() {
        return false;
    }
    let Some(stroke_scale) = stroke_scale(shape, &m) else {
        return false;
    };
    // Edges are in twips; the matrix translation is in pixels.
    let tw = Matrix {
        tx: m.tx * 20.0,
        ty: m.ty * 20.0,
        ..m
    };
    let map = |p: &mut Pt| *p = tw.transform_point(*p);
    for edge in &mut shape.edges {
        for cmd in &mut edge.path {
            match cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => map(p),
                PathCmd::QuadTo(c, p) => {
                    map(c);
                    map(p);
                }
            }
        }
    }
    for fill in &mut shape.fills {
        bake_fill(fill, &m);
    }
    for stroke in &mut shape.strokes {
        bake_fill(&mut stroke.fill, &m);
        if !stroke.hairline && stroke.scale_mode == ScaleMode::Normal {
            stroke.weight *= stroke_scale;
        }
    }
    shape.matrix = Matrix::IDENTITY;
    true
}

/// The factor a baked shape's scaling strokes are multiplied by, or `None`
/// when baking cannot keep them as drawn: a matrix that is not a similarity
/// (uniform scale, rotation, mirror) under a scaling stroke, or a stroke
/// scale mode other than normal or none.
fn stroke_scale(shape: &Shape, m: &Matrix) -> Option<f64> {
    let scale = m.determinant().abs().sqrt();
    let mut scaling = false;
    for st in &shape.strokes {
        match st.scale_mode {
            _ if st.hairline => {}
            ScaleMode::None => {}
            ScaleMode::Normal => scaling = true,
            ScaleMode::Horizontal | ScaleMode::Vertical => return None,
        }
    }
    if !scaling {
        return Some(1.0);
    }
    // A similarity has orthogonal columns of equal length.
    let (len_x, len_y) = (m.a.hypot(m.b), m.c.hypot(m.d));
    let tolerance = 1e-6 * len_x.max(len_y).max(1e-12);
    let similar = (len_x - len_y).abs() <= tolerance && (m.a * m.c + m.b * m.d).abs() <= tolerance;
    similar.then_some(scale)
}

/// Moves a fill's own matrix (gradient or bitmap space to shape space) into
/// the space `m` maps the shape to.
fn bake_fill(fill: &mut FillStyle, m: &Matrix) {
    match fill {
        FillStyle::Linear(g) | FillStyle::Radial(g) => g.matrix = m.multiply(&g.matrix),
        FillStyle::Bitmap(b) => b.matrix = m.multiply(&b.matrix),
        FillStyle::Solid(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jazari_xfl::model::{
        BitmapFill, Edge, Gradient, Group, Rgba, Shape, StrokeStyle, SymbolInstance,
    };

    fn ctx_parts() -> (
        jazari_xfl::Document,
        jazari_text::FontLibrary,
        jazari_as2::Compiler,
    ) {
        (
            jazari_xfl::Document::default(),
            jazari_text::FontLibrary::embedded_only(),
            jazari_as2::Compiler::new(Default::default()),
        )
    }

    fn instance(name: &str, m: Matrix) -> Element {
        Element::Instance(SymbolInstance {
            library_item: name.into(),
            matrix: m,
            ..SymbolInstance::default()
        })
    }

    /// A shape with one solid fill whose red channel tags it, and one edge
    /// from (0, 0) to (100, 0) twips.
    fn shape(tag: u8, m: Matrix, drawing_object: bool) -> Shape {
        Shape {
            matrix: m,
            fills: vec![FillStyle::Solid(Rgba::new(tag, 0, 0, 255))],
            edges: vec![Edge {
                fill1: 1,
                path: vec![
                    PathCmd::MoveTo(Pt::new(0.0, 0.0)),
                    PathCmd::LineTo(Pt::new(100.0, 0.0)),
                ],
                ..Edge::default()
            }],
            is_drawing_object: drawing_object,
            ..Shape::default()
        }
    }

    /// The tag of a shape made by [`shape`], or an instance's library item.
    fn tag(el: &Element) -> String {
        match el {
            Element::Shape(s) => match s.fills.first() {
                Some(FillStyle::Solid(c)) => c.r.to_string(),
                _ => "?".into(),
            },
            Element::Instance(i) => i.library_item.clone(),
            _ => "?".into(),
        }
    }

    /// The end point of the first edge of a shape made by [`shape`].
    fn end_point(el: &Element) -> Option<Pt> {
        match el {
            Element::Shape(s) => match s.edges.first()?.path.get(1)? {
                PathCmd::LineTo(p) => Some(*p),
                _ => None,
            },
            _ => None,
        }
    }

    #[test]
    fn member_matrices_are_absolute_and_never_composed() {
        // The groups fixture: members carry matrices that already include
        // the group transforms; JPEXS writes no group matrix.
        let (doc, fonts, as2) = ctx_parts();
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let n3n2n1 = Matrix::new(1.00017, 0.317642, -0.707352, 0.775162, 227.0, 179.0);
        let inner = Element::Group(Group {
            matrix: None,
            members: vec![instance("Marker", n3n2n1)],
        });
        let raw = shape(1, Matrix::translate(-31.9, 71.05), false);
        let outer = Element::Group(Group {
            matrix: None,
            members: vec![inner, Element::Shape(raw)],
        });
        let flat = flatten(&mut ctx, &[outer], &Origin::default());
        assert_eq!(flat.len(), 2);
        // The raw shape is drawn first, its translation baked into the edge.
        assert_eq!(flat[0].matrix(), Some(&Matrix::IDENTITY));
        let p = end_point(&flat[0]).expect("a line");
        assert!((p.x - (100.0 - 638.0)).abs() < 1e-9, "{p:?}");
        assert!((p.y - 1421.0).abs() < 1e-9, "{p:?}");
        assert_eq!(flat[1].matrix(), Some(&n3n2n1));
    }

    #[test]
    fn a_group_matrix_does_not_move_members_with_their_own_matrix() {
        let (doc, fonts, as2) = ctx_parts();
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let g = Element::Group(Group {
            matrix: Some(Matrix::translate(10.0, 0.0)),
            members: vec![instance("a", Matrix::scale(2.0, 2.0))],
        });
        let flat = flatten(&mut ctx, &[g], &Origin::default());
        assert_eq!(flat[0].matrix(), Some(&Matrix::scale(2.0, 2.0)));
    }

    #[test]
    fn identity_members_of_transformed_groups_inherit_the_composed_matrix() {
        let (doc, fonts, as2) = ctx_parts();
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let inner = Element::Group(Group {
            matrix: Some(Matrix::scale(2.0, 2.0)),
            members: vec![instance("b", Matrix::IDENTITY)],
        });
        let outer = Element::Group(Group {
            matrix: Some(Matrix::translate(10.0, 0.0)),
            members: vec![instance("a", Matrix::IDENTITY), inner],
        });
        let flat = flatten(&mut ctx, &[outer], &Origin::default());
        assert_eq!(flat[0].matrix(), Some(&Matrix::translate(10.0, 0.0)));
        assert_eq!(
            flat[1].matrix(),
            Some(&Matrix::new(2.0, 0.0, 0.0, 2.0, 10.0, 0.0))
        );
    }

    #[test]
    fn raw_shapes_go_to_the_back_of_every_list() {
        // The groups fixture's `stack` layer: XFL order Ga, Gb, drawing
        // object, raw rect; Flash draws the raw rect first.
        let (doc, fonts, as2) = ctx_parts();
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let ga = Element::Group(Group {
            matrix: None,
            members: vec![
                instance("in_ga", Matrix::IDENTITY),
                Element::Shape(shape(2, Matrix::IDENTITY, false)),
            ],
        });
        let gb = Element::Group(Group {
            matrix: None,
            members: vec![Element::Shape(shape(3, Matrix::IDENTITY, false))],
        });
        let elements = [
            ga,
            gb,
            Element::Shape(shape(4, Matrix::IDENTITY, true)),
            Element::Shape(shape(5, Matrix::IDENTITY, false)),
        ];
        let flat = flatten(&mut ctx, &elements, &Origin::default());
        let order: Vec<String> = flat.iter().map(tag).collect();
        assert_eq!(order, ["5", "2", "in_ga", "3", "4"]);
        assert!(is_raw_shape(&flat[0]));
        assert!(!is_raw_shape(&flat[4]));
    }

    #[test]
    fn only_grouped_shapes_and_drawing_objects_are_baked() {
        let (doc, fonts, as2) = ctx_parts();
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let m = Matrix::translate(5.0, 0.0);
        let flat = flatten(
            &mut ctx,
            &[
                Element::Shape(shape(1, m, false)),
                Element::Shape(shape(2, m, true)),
            ],
            &Origin::default(),
        );
        assert_eq!(flat[0].matrix(), Some(&m));
        assert_eq!(flat[1].matrix(), Some(&Matrix::IDENTITY));
        assert_eq!(end_point(&flat[1]), Some(Pt::new(200.0, 0.0)));
    }

    #[test]
    fn baking_moves_edges_and_fill_matrices_into_parent_space() {
        let rot = Matrix::new(0.0, 1.0, -1.0, 0.0, 10.0, 20.0); // 90 degrees
        let mut s = shape(1, rot, true);
        s.fills.push(FillStyle::Linear(Gradient {
            matrix: Matrix::scale(0.5, 0.5),
            ..Gradient::default()
        }));
        s.fills.push(FillStyle::Bitmap(BitmapFill {
            library_item: "b".into(),
            matrix: Matrix::new(20.0, 0.0, 0.0, 20.0, 1.0, 2.0),
            clipped: true,
        }));
        s.strokes.push(StrokeStyle::default());
        assert!(bake_shape(&mut s));
        assert_eq!(s.matrix, Matrix::IDENTITY);
        // (100, 0) twips turned by 90 degrees, plus (200, 400) twips.
        assert_eq!(s.edges[0].path[1], PathCmd::LineTo(Pt::new(200.0, 500.0)));
        let FillStyle::Linear(g) = &s.fills[1] else {
            panic!("fill changed kind");
        };
        assert_eq!(g.matrix, rot.multiply(&Matrix::scale(0.5, 0.5)));
        let FillStyle::Bitmap(b) = &s.fills[2] else {
            panic!("fill changed kind");
        };
        assert_eq!(b.matrix, Matrix::new(0.0, 20.0, -20.0, 0.0, 8.0, 21.0));
        assert_eq!(s.strokes[0].weight, StrokeStyle::default().weight);
    }

    #[test]
    fn uniform_scales_scale_strokes_and_uneven_ones_are_not_baked() {
        let stroked = |m: Matrix| Shape {
            matrix: m,
            strokes: vec![StrokeStyle {
                weight: 2.0,
                ..StrokeStyle::default()
            }],
            ..Shape::default()
        };
        let mut uniform = stroked(Matrix::scale(1.5, 1.5));
        assert!(bake_shape(&mut uniform));
        assert_eq!(uniform.strokes[0].weight, 3.0);

        let mut uneven = stroked(Matrix::scale(1.5, 0.75));
        assert!(!bake_shape(&mut uneven));
        assert_eq!(uneven.matrix, Matrix::scale(1.5, 0.75));

        let mut fixed = stroked(Matrix::scale(1.5, 0.75));
        fixed.strokes[0].scale_mode = ScaleMode::None;
        assert!(bake_shape(&mut fixed));
        assert_eq!(fixed.strokes[0].weight, 2.0);

        let mut broken = stroked(Matrix::new(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0));
        assert!(!bake_shape(&mut broken));
    }

    #[test]
    fn absurd_nesting_is_cut_without_overflow() {
        let (doc, fonts, as2) = ctx_parts();
        let mut ctx = Ctx::new(&doc, crate::BuildMode::Release, &fonts, &as2);
        let mut el = Element::Shape(Shape::default());
        for _ in 0..10_000 {
            el = Element::Group(Group {
                matrix: None,
                members: vec![el],
            });
        }
        let flat = flatten(&mut ctx, std::slice::from_ref(&el), &Origin::default());
        assert!(flat.is_empty());
        assert_eq!(ctx.take_diagnostics().len(), 1);
        // Drop the deep tree iteratively to keep the test's own stack safe.
        let mut cur = el;
        while let Element::Group(mut g) = cur {
            cur = g
                .members
                .pop()
                .unwrap_or(Element::Unsupported { tag: String::new() });
        }
    }
}
