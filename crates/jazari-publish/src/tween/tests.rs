//! Unit tests of the tween module, with numbers Flash 8 published.

use jazari_xfl::model::{ColorEffect, Matrix, MotionTween, Pt, RotateMode, SymbolInstance};

use super::matrix::{decompose, interpolate, rotation_delta};
use super::{interpolate_color, tweened_state};

fn tween(rotate: RotateMode, times: u32) -> MotionTween {
    MotionTween {
        rotate,
        rotate_times: times,
        ..MotionTween::default()
    }
}

fn fx(v: f64) -> i64 {
    (v * 65536.0).round() as i64
}

fn tw(v: f64) -> i64 {
    (v * 20.0).round() as i64
}

const O: Pt = Pt { x: 0.0, y: 0.0 };

fn instance(matrix: Matrix) -> SymbolInstance {
    SymbolInstance {
        matrix,
        ..SymbolInstance::default()
    }
}

#[test]
fn clockwise_one_turn_matches_the_tweens_fixture() {
    // rotate_cw: (60, 50) -> (260, 50), CW x1 over 23 frames. Flash 8's
    // frame 2: a=63083 b=17555 tx=1374 twips.
    let t = tween(RotateMode::Clockwise, 1);
    let a = Matrix::new(1.0, 0.0, 0.0, 1.0, 60.0, 50.0);
    let b = Matrix::new(1.0, 0.0, 0.0, 1.0, 260.0, 50.0);
    let s = tweened_state(&t, &instance(a), &instance(b), 1, 23);
    let m = s.matrix;
    assert_eq!(
        (fx(m.a), fx(m.b), fx(m.c), fx(m.d)),
        (63083, 17555, -17555, 63083)
    );
    assert_eq!((tw(m.tx), tw(m.ty)), (1374, 1000));
}

#[test]
fn rotate_modes_turn_the_right_way() {
    let cw = tween(RotateMode::Clockwise, 1);
    assert_eq!(rotation_delta(&cw, 0.0, 0.0), 360.0);
    let ccw = tween(RotateMode::CounterClockwise, 2);
    assert_eq!(rotation_delta(&ccw, 0.0, 0.0), -720.0);
    let cw0 = tween(RotateMode::Clockwise, 0);
    assert_eq!(rotation_delta(&cw0, 0.0, -90.0), 270.0);
    let ccw0 = tween(RotateMode::CounterClockwise, 0);
    assert_eq!(rotation_delta(&ccw0, 10.0, 20.0), -350.0);
    let auto = tween(RotateMode::Auto, 0);
    assert_eq!(rotation_delta(&auto, 170.0, -170.0), 20.0);
    // Rotate "none" does not turn (tween-probes/matrices, layer none_rot).
    let none = tween(RotateMode::None, 3);
    assert_eq!(rotation_delta(&none, 0.0, 90.0), 0.0);
}

#[test]
fn mirrored_keyframes_keep_positive_scales() {
    // tween-probes/matrices, layer mirror: (-1, 0, 0, 1) -> 90 degrees.
    let p = decompose(&Matrix::new(-1.0, 0.0, 0.0, 1.0, 0.0, 0.0));
    assert_eq!((p.sx, p.sy, p.rx, p.ry), (1.0, 1.0, 0.0, 180.0));
    let t = tween(RotateMode::Auto, 0);
    let a = Matrix::new(-1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
    let b = Matrix::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0);
    let m = interpolate(&t, &a, &b, O, O, 0.5);
    // Both axes meet at 135 and 45 degrees: a = cos 135, d = cos 45.
    assert_eq!(
        (fx(m.a), fx(m.b), fx(m.c), fx(m.d)),
        (-46341, 46341, -46341, 46341)
    );
}

#[test]
fn the_transformation_point_moves_in_a_straight_line() {
    // tween-probes/tpoint, layer tp_corner_rot: Corner (transformation
    // point (35, 10)) at (25, 150) -> rotated 90 degrees at (220, 185) over
    // 30 frames. Flash 8's frame 16: tx=2347 ty=3163 twips.
    let t = tween(RotateMode::Auto, 0);
    let tp = Pt { x: 35.0, y: 10.0 };
    let a = Matrix::new(1.0, 0.0, 0.0, 1.0, 25.0, 150.0);
    let b = Matrix::new(0.0, 1.0, -1.0, 0.0, 220.0, 185.0);
    let q = (15.0f64 * 65536.0 / 30.0).round() / 65536.0;
    let m = interpolate(&t, &a, &b, tp, tp, q);
    assert!((tw(m.tx) - 2347).abs() <= 1, "{m:?}");
    assert!((tw(m.ty) - 3163).abs() <= 1, "{m:?}");
    // With the point at the registration point, tx and ty simply lerp.
    let m = interpolate(&t, &a, &b, O, O, 0.5);
    assert_eq!((tw(m.tx), tw(m.ty)), (2450, 3350));
}

#[test]
fn scale_off_keeps_the_start_scale() {
    let t = MotionTween {
        scale: false,
        ..tween(RotateMode::Auto, 0)
    };
    let a = Matrix::scale(1.5, 1.5);
    let b = Matrix::scale(0.5, 0.5);
    let m = interpolate(&t, &a, &b, O, O, 0.5);
    assert_eq!((fx(m.a), fx(m.d)), (98304, 98304));
}

#[test]
fn end_points_reproduce_the_keyframes() {
    let t = tween(RotateMode::Auto, 0);
    let a = Matrix::new(1.0, 0.0, 0.0, 1.0, 330.0, 50.0);
    let b = Matrix::new(0.0, 1.0, -1.0, 0.0, 500.0, 50.0);
    assert!(interpolate(&t, &a, &b, O, O, 0.0).approx_eq(&a, 1e-9));
    assert!(interpolate(&t, &a, &b, O, O, 1.0).approx_eq(&b, 1e-9));
}

#[test]
fn colours_interpolate_in_swf_units() {
    // tween-probes/color, layer c_tint_tint: tint #FF0000 50 % -> #0000FF
    // 80 %; Flash 8's frame 1 is mult 124, add (122, 0, 10).
    let a = ColorEffect {
        mult: [0.5, 0.5, 0.5, 1.0],
        add: [127.5, 0.0, 0.0, 0.0],
    };
    let b = ColorEffect {
        mult: [0.2, 0.2, 0.2, 1.0],
        add: [0.0, 0.0, 204.0, 0.0],
    };
    let q = 3277.0 / 65536.0;
    let c = interpolate_color(&a, &b, q);
    assert_eq!(c.mult[0] * 256.0, 124.0);
    assert_eq!(c.add, [122.0, 0.0, 10.0, 0.0]);
    // c_adv: green offset -50 -> 200 at frame 3 is -13 (rounded half up
    // from -12.5015).
    let a = ColorEffect {
        mult: [1.0; 4],
        add: [0.0, -50.0, 0.0, 0.0],
    };
    let b = ColorEffect {
        mult: [1.0; 4],
        add: [0.0, 200.0, 0.0, 0.0],
    };
    let c = interpolate_color(&a, &b, 9830.0 / 65536.0);
    assert_eq!(c.add[1], -13.0);
}

#[test]
fn hostile_input_does_not_panic() {
    let t = tween(RotateMode::Clockwise, u32::MAX);
    let a = Matrix::new(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0);
    let b = Matrix::new(1e308, 1e308, -1e308, 1e308, 1e308, -1e308);
    let s = tweened_state(&t, &instance(a), &instance(b), 1, 2);
    assert!(s.matrix.a.is_nan());
    let s = tweened_state(&t, &instance(b), &instance(a), 1, 2);
    assert_eq!(s.matrix, b);
    let s = tweened_state(&t, &instance(Matrix::IDENTITY), &instance(b), 1, 3);
    assert!(s.matrix.is_finite());
    let s = tweened_state(&t, &instance(Matrix::IDENTITY), &instance(b), 0, 0);
    assert_eq!(s.matrix, Matrix::IDENTITY);
}
