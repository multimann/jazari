use super::*;
use crate::BuildMode;
use crate::context::Ctx;
use jazari_xfl::Document;
use jazari_xfl::model::{GradientStop, Matrix, Rgba};

fn with_ctx<R>(f: impl FnOnce(&mut Ctx<'_>) -> R) -> R {
    let doc = Document::default();
    let fonts = jazari_text::FontLibrary::embedded_only();
    let as2 = jazari_as2::Compiler::new(Default::default());
    let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
    f(&mut ctx)
}

fn pt(x: f64, y: f64) -> Pt {
    Pt::new(x, y)
}

fn edge(fill0: u32, fill1: u32, stroke: u32, path: Vec<PathCmd>) -> Edge {
    Edge {
        fill0,
        fill1,
        stroke,
        path,
    }
}

/// A closed square of side `s` twips at (x, y) as one edge.
fn square(x: f64, y: f64, s: f64, fill1: u32, stroke: u32) -> Edge {
    edge(
        0,
        fill1,
        stroke,
        vec![
            PathCmd::MoveTo(pt(x, y)),
            PathCmd::LineTo(pt(x + s, y)),
            PathCmd::LineTo(pt(x + s, y + s)),
            PathCmd::LineTo(pt(x, y + s)),
            PathCmd::LineTo(pt(x, y)),
        ],
    )
}

fn solid(r: u8) -> FillStyle {
    FillStyle::Solid(Rgba::opaque(r, 0, 0))
}

fn build(shapes: &[&Shape]) -> (Option<BuiltShape>, Vec<jazari_as2::Diagnostic>) {
    with_ctx(|ctx| {
        let b = build_shape(ctx, shapes, &Origin::default());
        (b, ctx.take_diagnostics())
    })
}

fn style_changes(s: &swf::Shape) -> Vec<&StyleChangeData> {
    s.shape
        .iter()
        .filter_map(|r| match r {
            ShapeRecord::StyleChange(sc) => Some(&**sc),
            _ => None,
        })
        .collect()
}

#[test]
fn a_filled_square_is_a_v3_shape_with_one_style_change() {
    let s = Shape {
        fills: vec![solid(255)],
        edges: vec![square(200.0, 400.0, 1000.0, 1, 0)],
        ..Shape::default()
    };
    let built = build(&[&s]).0.expect("shape").shape;
    assert_eq!(built.version, 3);
    let sc = style_changes(&built);
    assert_eq!(sc.len(), 1);
    assert_eq!(
        sc[0].move_to,
        Some(Point::new(Twips::new(200), Twips::new(400)))
    );
    assert_eq!(
        (sc[0].fill_style_0, sc[0].fill_style_1, sc[0].line_style),
        (Some(0), Some(1), Some(0))
    );
    assert_eq!(built.shape.len(), 5);
    assert_eq!(built.shape_bounds.x_min.get(), 200);
    assert_eq!(built.shape_bounds.y_max.get(), 1400);
    tags::encode(&Tag::DefineShape(Box::new(built))).expect("round trip");
}

#[test]
fn no_empty_style_change_is_written() {
    // Two edges with the same styles, the second starting where the first
    // ends (as XFL writes a contour split across Edge elements).
    let s = Shape {
        fills: vec![solid(1)],
        edges: vec![
            edge(
                0,
                1,
                0,
                vec![
                    PathCmd::MoveTo(pt(0.0, 0.0)),
                    PathCmd::LineTo(pt(100.0, 0.0)),
                ],
            ),
            edge(
                0,
                1,
                0,
                vec![
                    PathCmd::MoveTo(pt(100.0, 0.0)),
                    PathCmd::LineTo(pt(100.0, 100.0)),
                ],
            ),
            edge(
                0,
                1,
                0,
                vec![
                    PathCmd::MoveTo(pt(100.0, 100.0)),
                    PathCmd::LineTo(pt(0.0, 0.0)),
                ],
            ),
        ],
        ..Shape::default()
    };
    let built = build(&[&s]).0.expect("shape").shape;
    for sc in style_changes(&built) {
        assert!(
            sc.move_to.is_some()
                || sc.fill_style_0.is_some()
                || sc.fill_style_1.is_some()
                || sc.line_style.is_some()
                || sc.new_styles.is_some(),
            "empty StyleChange"
        );
    }
    // The first record sets the selectors; the pen never needs to move.
    assert_eq!(style_changes(&built).len(), 1);
}

#[test]
fn every_block_restates_all_selectors_with_new_styles() {
    let a = Shape {
        fills: vec![solid(1)],
        edges: vec![square(0.0, 0.0, 100.0, 1, 0)],
        ..Shape::default()
    };
    let b = Shape {
        fills: vec![solid(2), solid(3)],
        strokes: vec![StrokeStyle::default()],
        edges: vec![square(0.0, 0.0, 100.0, 2, 1)],
        ..Shape::default()
    };
    let built = build(&[&a, &b]).0.expect("shape").shape;
    assert_eq!(built.version, 4, "a stroke makes it DefineShape4");
    let sc = style_changes(&built);
    let at = sc
        .iter()
        .position(|s| s.new_styles.is_some())
        .expect("new styles");
    let n = sc[at];
    assert_eq!(
        (n.fill_style_0, n.fill_style_1, n.line_style),
        (Some(0), Some(0), Some(0))
    );
    assert_eq!(n.new_styles.as_ref().map(|s| s.fill_styles.len()), Some(2));
    let next = sc[at + 1];
    assert_eq!(
        (next.fill_style_0, next.fill_style_1, next.line_style),
        (Some(0), Some(2), Some(1))
    );
    assert!(sc.iter().filter(|s| s.new_styles.is_some()).count() == 1);
    assert!(built.flags.contains(ShapeFlag::HAS_SCALING_STROKES));
    tags::encode(&Tag::DefineShape(Box::new(built))).expect("round trip");
}

#[test]
fn bad_selectors_and_invisible_edges_are_cleaned() {
    let s = Shape {
        fills: vec![solid(1)],
        edges: vec![
            // Out of range fill and stroke: nothing left, dropped.
            square(0.0, 0.0, 10.0, 9, 9),
            // Same fill on both sides: dropped.
            edge(
                1,
                1,
                0,
                vec![PathCmd::MoveTo(pt(0.0, 0.0)), PathCmd::LineTo(pt(5.0, 5.0))],
            ),
        ],
        ..Shape::default()
    };
    assert!(build(&[&s]).0.is_none(), "nothing drawable");
}

#[test]
fn long_edges_are_split_within_the_delta_limit() {
    let s = Shape {
        fills: vec![solid(1)],
        edges: vec![edge(
            0,
            1,
            0,
            vec![
                PathCmd::MoveTo(pt(0.0, 0.0)),
                PathCmd::LineTo(pt(200_000.0, -70_000.0)),
                PathCmd::QuadTo(pt(400_000.0, 300_000.0), pt(0.0, 0.0)),
            ],
        )],
        ..Shape::default()
    };
    let built = build(&[&s]).0.expect("shape").shape;
    let mut x = 0i64;
    let mut y = 0i64;
    for r in &built.shape {
        match r {
            ShapeRecord::StraightEdge { delta } => {
                assert!(
                    delta.dx.get().abs() <= MAX_EDGE_DELTA
                        && delta.dy.get().abs() <= MAX_EDGE_DELTA
                );
                x += i64::from(delta.dx.get());
                y += i64::from(delta.dy.get());
            }
            ShapeRecord::CurvedEdge {
                control_delta,
                anchor_delta,
            } => {
                for d in [
                    control_delta.dx,
                    control_delta.dy,
                    anchor_delta.dx,
                    anchor_delta.dy,
                ] {
                    assert!(d.get().abs() <= MAX_EDGE_DELTA);
                }
                x += i64::from(control_delta.dx.get() + anchor_delta.dx.get());
                y += i64::from(control_delta.dy.get() + anchor_delta.dy.get());
            }
            ShapeRecord::StyleChange(_) => {}
        }
    }
    assert_eq!((x, y), (0, 0), "the contour still closes");
    tags::encode(&Tag::DefineShape(Box::new(built))).expect("round trip");
}

#[test]
fn absolute_positions_are_rounded_before_deltas() {
    // Three steps of 0.4 twips: rounding each delta would give 0, 0, 0.
    let s = Shape {
        strokes: vec![StrokeStyle::default()],
        edges: vec![edge(
            0,
            0,
            1,
            vec![
                PathCmd::MoveTo(pt(0.0, 0.0)),
                PathCmd::LineTo(pt(0.4, 0.0)),
                PathCmd::LineTo(pt(0.8, 0.0)),
                PathCmd::LineTo(pt(1.2, 0.0)),
            ],
        )],
        ..Shape::default()
    };
    let built = build(&[&s]).0.expect("shape").shape;
    let total: i32 = built
        .shape
        .iter()
        .map(|r| match r {
            ShapeRecord::StraightEdge { delta } => delta.dx.get(),
            _ => 0,
        })
        .sum();
    assert_eq!(total, 1);
}

#[test]
fn bounds_include_curve_extrema_and_half_strokes() {
    let stroke = StrokeStyle {
        weight: 10.0,
        ..StrokeStyle::default()
    };
    let s = Shape {
        fills: vec![solid(1)],
        strokes: vec![stroke],
        edges: vec![
            // A curve bulging to y = -500 (control at -1000).
            edge(
                0,
                1,
                0,
                vec![
                    PathCmd::MoveTo(pt(0.0, 0.0)),
                    PathCmd::QuadTo(pt(500.0, -1000.0), pt(1000.0, 0.0)),
                    PathCmd::LineTo(pt(0.0, 0.0)),
                ],
            ),
            // A stroked line far right: grows by 100 twips (10 px / 2).
            edge(
                0,
                0,
                1,
                vec![
                    PathCmd::MoveTo(pt(2000.0, 0.0)),
                    PathCmd::LineTo(pt(3000.0, 0.0)),
                ],
            ),
        ],
        ..Shape::default()
    };
    let built = build(&[&s]).0.expect("shape").shape;
    assert_eq!(built.edge_bounds.y_min.get(), -500);
    assert_eq!(built.edge_bounds.x_max.get(), 3000);
    assert_eq!(
        built.shape_bounds.y_min.get(),
        -500,
        "the fill is not grown"
    );
    assert_eq!(built.shape_bounds.x_max.get(), 3100);
    assert_eq!(built.shape_bounds.y_max.get(), 100);
}

#[test]
fn gradients_are_capped_at_fifteen_records_and_focal_needs_v4() {
    let stops: Vec<GradientStop> = (0..20)
        .map(|i| GradientStop {
            ratio: f64::from(i) / 19.0,
            color: Rgba::opaque(i as u8, 0, 0),
        })
        .collect();
    let g = Gradient {
        matrix: Matrix::scale(0.05, 0.05),
        stops,
        focal_point: 0.70588,
        ..Gradient::default()
    };
    let s = Shape {
        fills: vec![FillStyle::Radial(g)],
        edges: vec![square(0.0, 0.0, 100.0, 1, 0)],
        ..Shape::default()
    };
    let (built, diags) = build(&[&s]);
    let built = built.expect("shape").shape;
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(built.version, 4);
    match &built.styles.fill_styles[0] {
        SwfFill::FocalGradient {
            gradient,
            focal_point,
        } => {
            assert_eq!(gradient.records.len(), MAX_GRADIENT_RECORDS);
            assert_eq!(gradient.records.last().map(|r| r.ratio), Some(255));
            assert_eq!(focal_point.get(), 180, "Flash 8 wrote 180/256");
        }
        other => panic!("{other:?}"),
    }
    tags::encode(&Tag::DefineShape(Box::new(built))).expect("round trip");
}

#[test]
fn line_style_two_carries_caps_joins_and_scaling() {
    let st = StrokeStyle {
        weight: 12.0,
        caps: Caps::None,
        joints: Joints::Miter,
        miter_limit: 1.5,
        scale_mode: ScaleMode::None,
        ..StrokeStyle::default()
    };
    let hair = StrokeStyle {
        hairline: true,
        ..StrokeStyle::default()
    };
    let s = Shape {
        strokes: vec![st, hair],
        edges: vec![
            edge(
                0,
                0,
                1,
                vec![
                    PathCmd::MoveTo(pt(0.0, 0.0)),
                    PathCmd::LineTo(pt(100.0, 0.0)),
                ],
            ),
            edge(
                0,
                0,
                2,
                vec![
                    PathCmd::MoveTo(pt(0.0, 50.0)),
                    PathCmd::LineTo(pt(100.0, 50.0)),
                ],
            ),
        ],
        ..Shape::default()
    };
    let built = build(&[&s]).0.expect("shape").shape;
    assert!(built.flags.contains(ShapeFlag::HAS_NON_SCALING_STROKES));
    let expected = LineStyle::new()
        .with_width(Twips::new(240))
        .with_start_cap(LineCapStyle::None)
        .with_end_cap(LineCapStyle::None)
        .with_join_style(LineJoinStyle::Miter(Fixed8::from_bits(384)))
        .with_allow_scale_x(false)
        .with_allow_scale_y(false)
        .with_color(swf::Color::BLACK);
    assert_eq!(built.styles.line_styles[0], expected);
    tags::encode(&Tag::DefineShape(Box::new(built))).expect("round trip");
}

#[test]
fn identical_shapes_are_defined_once() {
    let s = Shape {
        fills: vec![solid(9)],
        edges: vec![square(0.0, 0.0, 100.0, 1, 0)],
        ..Shape::default()
    };
    with_ctx(|ctx| {
        let a = define_shapes(ctx, &[&s], &Origin::default());
        let b = define_shapes(ctx, &[&s.clone()], &Origin::default());
        assert!(a.is_some());
        assert_eq!(a, b);
    });
}

#[test]
fn hostile_coordinates_do_not_panic() {
    let s = Shape {
        fills: vec![solid(1)],
        strokes: vec![StrokeStyle {
            weight: 1e12,
            ..StrokeStyle::default()
        }],
        edges: vec![edge(
            0,
            1,
            1,
            vec![
                PathCmd::LineTo(pt(f64::NAN, 1e300)),
                PathCmd::QuadTo(pt(-1e300, f64::INFINITY), pt(3.0, 4.0)),
                PathCmd::MoveTo(pt(f64::NEG_INFINITY, 0.0)),
                PathCmd::LineTo(pt(1e18, -1e18)),
            ],
        )],
        ..Shape::default()
    };
    let (built, diags) = build(&[&s]);
    assert!(!diags.is_empty(), "the stroke weight is reported");
    let built = built.expect("shape").shape;
    tags::encode(&Tag::DefineShape(Box::new(built))).expect("round trip");
}
