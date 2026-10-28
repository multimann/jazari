use super::*;
use crate::BuildMode;
use jazari_as2::ClipHandler;
use swf::{Fixed8, Twips};

fn ident(slot: u32, what: IdentKind) -> Ident {
    vec![IdentPart {
        layer: 0,
        slot,
        what,
    }]
}

fn sym(item: &str) -> IdentKind {
    IdentKind::Symbol {
        item: item.to_owned(),
        kind: 0,
        name: None,
        script: None,
        track_as_menu: false,
    }
}

fn at(depth: u16, char: CharacterId, what: IdentKind, tx: i32) -> DisplayItem {
    let mut m = Matrix::IDENTITY;
    m.tx = Twips::new(tx);
    DisplayItem::new(depth, ident(0, what), char, m)
}

#[test]
fn first_frame_places_everything_with_a_matrix() {
    let mut dl = DisplayList::default();
    let (rm, places) = dl.diff(vec![at(1, 5, sym("a"), 0), at(3, 6, IdentKind::Shape, 20)]);
    assert!(rm.is_empty());
    assert_eq!(places.len(), 2);
    assert_eq!(places[0].action, PlaceObjectAction::Place(5));
    assert_eq!(
        places[0].matrix,
        Some(Matrix::IDENTITY),
        "Flash always writes the matrix"
    );
    assert_eq!(places[0].cxform, None);
}

#[test]
fn unchanged_items_write_nothing_and_changes_write_only_the_change() {
    let mut dl = DisplayList::default();
    dl.diff(vec![at(1, 5, sym("a"), 0)]);
    let (rm, places) = dl.diff(vec![at(1, 5, sym("a"), 0)]);
    assert!(rm.is_empty() && places.is_empty());
    let mut moved = at(1, 5, sym("a"), 40);
    moved.cxform.a_multiply = Fixed8::from_bits(128);
    let (_, places) = dl.diff(vec![moved]);
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].action, PlaceObjectAction::Modify);
    assert!(places[0].matrix.is_some() && places[0].cxform.is_some());
    assert!(places[0].filters.is_none() && places[0].name.is_none());
    // Back to the identity colour transform: written explicitly.
    let (_, places) = dl.diff(vec![at(1, 5, sym("a"), 40)]);
    assert_eq!(places[0].cxform, Some(ColorTransform::IDENTITY));
    assert_eq!(places[0].matrix, None);
}

#[test]
fn another_symbol_in_the_slot_is_removed_and_placed() {
    let mut dl = DisplayList::default();
    dl.diff(vec![at(1, 5, sym("a"), 0)]);
    let (rm, places) = dl.diff(vec![at(1, 7, sym("b"), 0)]);
    assert_eq!(rm, vec![1]);
    assert_eq!(places[0].action, PlaceObjectAction::Place(7));
}

#[test]
fn shapes_in_the_same_slot_are_replaced() {
    let mut dl = DisplayList::default();
    let mut a = at(2, 5, IdentKind::Shape, 0);
    a.replaceable = true;
    dl.diff(vec![a]);
    let mut b = at(2, 9, IdentKind::Shape, 0);
    b.replaceable = true;
    let (rm, places) = dl.diff(vec![b]);
    assert!(rm.is_empty());
    assert_eq!(places[0].action, PlaceObjectAction::Replace(9));
    assert_eq!(places[0].matrix, None);
}

#[test]
fn a_new_name_or_clip_depth_re_places() {
    let mut dl = DisplayList::default();
    let mut a = at(1, 5, sym("a"), 0);
    a.name = Some("one".into());
    dl.diff(vec![a]);
    let mut b = at(1, 5, sym("a"), 0);
    b.name = Some("two".into());
    let (rm, places) = dl.diff(vec![b.clone()]);
    assert_eq!(rm, vec![1]);
    assert_eq!(places[0].name.as_deref(), Some("two"));
    let mut c = b;
    c.clip_depth = Some(4);
    let (rm, places) = dl.diff(vec![c]);
    assert_eq!(rm, vec![1]);
    assert_eq!(places[0].clip_depth, Some(4));
}

#[test]
fn vanished_depths_are_removed_in_order() {
    let mut dl = DisplayList::default();
    dl.diff(vec![
        at(3, 1, sym("a"), 0),
        at(1, 2, sym("b"), 0),
        at(2, 3, sym("c"), 0),
    ]);
    let (rm, places) = dl.diff(vec![at(2, 3, sym("c"), 0)]);
    assert_eq!(rm, vec![1, 3]);
    assert!(places.is_empty());
}

#[test]
fn place_object_three_only_when_needed() {
    let plain = place(&at(1, 5, sym("a"), 0));
    assert_eq!(place_tag(&plain).version, 2);
    let mut blended = at(1, 5, sym("a"), 0);
    blended.blend = BlendMode::Multiply;
    let op = place(&blended);
    assert_eq!(place_tag(&op).version, 3);
    let mut cached = at(1, 5, sym("a"), 0);
    cached.cache = true;
    assert_eq!(place_tag(&place(&cached)).version, 3);
}

fn encode_ok(op: &PlaceOp) {
    tags::encode(&Tag::PlaceObject(Box::new(place_tag(op)))).expect("round trip");
}

#[test]
fn place_objects_round_trip() {
    let mut full = at(9, 5, sym("a"), -123);
    full.name = Some("اسم_عربي".into());
    full.cxform.r_add = -255;
    full.filters = vec![Filter::BlurFilter(Box::new(swf::BlurFilter {
        blur_x: swf::Fixed16::from_bits(10 << 16),
        blur_y: swf::Fixed16::from_bits(4 << 16),
        flags: swf::BlurFilterFlags::from_passes(2),
    }))];
    full.blend = BlendMode::Screen;
    full.cache = true;
    full.clip_actions = Some(Rc::new(vec![
        ClipHandler {
            events: ClipEventFlag::LOAD,
            key_code: None,
            actions: vec![0x07, 0x00],
        },
        ClipHandler {
            events: ClipEventFlag::KEY_PRESS | ClipEventFlag::ENTER_FRAME,
            key_code: Some(13),
            actions: vec![0x00],
        },
    ]));
    let op = place(&full);
    encode_ok(&op);
    let tag = place_tag(&op);
    let ca = tag.clip_actions.as_ref().expect("clip actions");
    assert_eq!(
        ca.all_event_flags,
        ClipEventFlag::LOAD | ClipEventFlag::KEY_PRESS | ClipEventFlag::ENTER_FRAME
    );
    // Modify, Replace, clearing filters, and a clip depth.
    let mut dl = DisplayList::default();
    let mut a = at(4, 5, IdentKind::Shape, 0);
    a.replaceable = true;
    a.filters = full.filters.clone();
    dl.diff(vec![a]);
    let mut b = at(4, 6, IdentKind::Shape, 99);
    b.replaceable = true;
    let (_, places) = dl.diff(vec![b]);
    assert_eq!(places[0].filters, Some(Vec::new()));
    encode_ok(&places[0]);
    let mut masked = at(2, 8, IdentKind::Unique(8), 0);
    masked.clip_depth = Some(16383);
    encode_ok(&place(&masked));
}

#[test]
fn frames_encode_in_flash_order() {
    let doc = jazari_xfl::Document::default();
    let fonts = jazari_text::FontLibrary::embedded_only();
    let as2 = jazari_as2::Compiler::new(Default::default());
    let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
    let o = Origin::default();
    let shape = ctx
        .define(crate::context::CharKind::Shape, vec![], &o, |id| {
            let s = swf::Shape {
                version: 3,
                id,
                shape_bounds: swf::Rectangle::ZERO,
                edge_bounds: swf::Rectangle::ZERO,
                flags: swf::ShapeFlag::HAS_NON_SCALING_STROKES,
                styles: swf::ShapeStyles {
                    fill_styles: vec![],
                    line_styles: vec![],
                },
                shape: vec![],
            };
            Ok(vec![tags::encode(&Tag::DefineShape(Box::new(s)))?])
        })
        .expect("shape");
    let ops = FrameOps {
        removes: vec![2],
        actions: vec![vec![0x07, 0x00]],
        labels: vec![("loop".into(), false), ("anchor".into(), true)],
        places: vec![place(&at(1, shape, IdentKind::Shape, 0))],
    };
    let mut out = Vec::new();
    encode_frame(&mut ctx, &ops, &mut DefSink::Root, &mut out, &o);
    let codes: Vec<u16> = out.iter().map(|t| t.code).collect();
    // RemoveObject2, DoAction, FrameLabel x2, DefineShape3, PlaceObject2.
    assert_eq!(codes, vec![28, 12, 43, 43, 32, 26]);
    assert!(ctx.take_diagnostics().is_empty());
    // A sprite collects the dependency instead of writing it.
    let mut deps = Vec::new();
    let mut out = Vec::new();
    encode_frame(
        &mut ctx,
        &ops,
        &mut DefSink::Sprite(&mut deps),
        &mut out,
        &o,
    );
    assert_eq!(deps, vec![shape]);
    assert!(!out.iter().any(|t| t.code == 32));
}
