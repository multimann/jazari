//! Publishes every fixture (release and stage builds), reads the output back
//! with the `swf` crate, and compares it with Flash 8's `flash8.swf` where
//! the comparison does not depend on rendering.

mod common;

use std::collections::BTreeMap;

use common::{
    CharInfo, Placed, all_fixtures, errors, flash8, publish_fixture, root_states, with_parsed,
};
use jazari_publish::BuildMode;
use swf::{CharacterId, Tag};

fn header_frames(bytes: &[u8]) -> u16 {
    with_parsed(bytes, |s| s.header.num_frames())
}

#[test]
fn every_fixture_publishes_and_reads_back() {
    let names = all_fixtures();
    assert!(names.len() >= 8, "fixtures not found: {names:?}");
    for name in &names {
        for mode in [BuildMode::Release, BuildMode::Stage] {
            let out = publish_fixture(name, mode);
            assert!(
                errors(&out).is_empty(),
                "{name} {mode:?}: {:#?}",
                errors(&out)
            );
            let swf = out.swf.unwrap_or_else(|| panic!("{name} {mode:?}: no SWF"));
            let frames = header_frames(&swf);
            assert_eq!(usize::from(frames), out.frame_map.len(), "{name} {mode:?}");
            assert_eq!(
                frames,
                header_frames(&flash8(name)),
                "{name} {mode:?}: frame count"
            );
            let first = with_parsed(&swf, |s| format!("{:?}", s.tags.first()));
            assert!(first.contains("FileAttributes"), "{name}: {first}");
        }
    }
}

#[test]
fn publishing_is_deterministic() {
    for name in all_fixtures() {
        let a = publish_fixture(&name, BuildMode::Release);
        let b = publish_fixture(&name, BuildMode::Release);
        assert_eq!(a.swf, b.swf, "{name}");
        assert_eq!(a.diagnostics, b.diagnostics, "{name}");
    }
}

/// Stub modules warn that something is not published yet. Besides those,
/// the only expected warnings are font substitutions: the tests publish with
/// the embedded fonts only, so the text fixture's Arial, Times New Roman and
/// Tahoma are replaced by the Noto fonts (tests/text.rs checks the text
/// fixture with the installed fonts).
#[test]
fn only_the_stub_modules_warn_on_fixtures() {
    for name in all_fixtures() {
        let out = publish_fixture(&name, BuildMode::Release);
        for d in &out.diagnostics {
            let substitution = d.message.contains("is not installed;")
                && d.message.contains("in (put the font in a 'fonts' folder");
            // The masks fixture puts several elements on one mask layer
            // (Flash 8 publishes the backmost only).
            let mask_note = name == "masks"
                && d.message
                    .contains("only the backmost element of a mask layer");
            assert!(
                d.message.contains("not published yet") || substitution || mask_note,
                "{name}: unexpected diagnostic {d}"
            );
        }
    }
}

fn sprites_by_depth(
    frame: &BTreeMap<u16, Placed>,
    chars: &BTreeMap<CharacterId, CharInfo>,
) -> Vec<Placed> {
    frame
        .values()
        .filter(|p| chars.get(&p.char) == Some(&CharInfo::Sprite))
        .cloned()
        .collect()
}

/// The largest difference in twips between two matrices, counting the
/// linear part in twips over a 100 px arm (2000 twips).
fn matrix_error(a: &swf::Matrix, b: &swf::Matrix) -> f64 {
    let lin = [
        (a.a.to_f64() - b.a.to_f64()),
        (a.b.to_f64() - b.b.to_f64()),
        (a.c.to_f64() - b.c.to_f64()),
        (a.d.to_f64() - b.d.to_f64()),
    ]
    .iter()
    .map(|d| d.abs() * 2000.0)
    .fold(0.0, f64::max);
    let t = f64::from(
        (a.tx.get() - b.tx.get())
            .abs()
            .max((a.ty.get() - b.ty.get()).abs()),
    );
    lin.max(t)
}

/// The motion tweens: the same sprites in the same layer order at every
/// frame, with matrices within 1 twip of Flash 8's (the linear part counted
/// over a 100 px arm) and the same colour transforms. `tween_oracle.rs`
/// checks every tween property layer by layer.
#[test]
fn tweens_follow_flash_8() {
    let ours = publish_fixture("tweens", BuildMode::Release)
        .swf
        .expect("swf");
    let (our_frames, our_chars) = root_states(&ours);
    let (f8_frames, f8_chars) = root_states(&flash8("tweens"));
    assert_eq!(our_frames.len(), f8_frames.len());
    let mut worst = 0.0f64;
    let mut worst_at = (0, 0);
    for (f, (o, e)) in our_frames.iter().zip(&f8_frames).enumerate() {
        let o = sprites_by_depth(o, &our_chars);
        let e = sprites_by_depth(e, &f8_chars);
        assert_eq!(o.len(), e.len(), "frame {}", f + 1);
        for (i, (a, b)) in o.iter().zip(&e).enumerate() {
            let err = matrix_error(&a.matrix, &b.matrix);
            if err > worst {
                worst = err;
                worst_at = (f + 1, i);
            }
            assert_eq!(
                a.cxform,
                b.cxform,
                "frame {} sprite {i}: colour transform",
                f + 1
            );
        }
    }
    eprintln!(
        "tweens: worst matrix error {worst:.1} twips at frame {} sprite {}",
        worst_at.0, worst_at.1
    );
    assert!(
        worst <= 1.0,
        "worst matrix error {worst} twips at {worst_at:?}"
    );
}

/// The flattened `Counter` graphic shows the same bar (width and position)
/// as Flash 8 at every frame, for loop, play-once and single-frame
/// instances.
#[test]
fn graphic_symbols_flatten_like_flash_8() {
    let ours = publish_fixture("tweens", BuildMode::Release)
        .swf
        .expect("swf");
    let bars = |bytes: &[u8]| -> Vec<Vec<(i32, i32)>> {
        let (frames, chars) = root_states(bytes);
        frames
            .iter()
            .map(|f| {
                let mut v: Vec<(i32, i32)> = f
                    .values()
                    .filter_map(|p| match chars.get(&p.char) {
                        Some(CharInfo::Shape(b)) => Some((p.matrix.tx.get(), b.width().get())),
                        _ => None,
                    })
                    .collect();
                v.sort_unstable();
                v
            })
            .collect()
    };
    let o = bars(&ours);
    let e = bars(&flash8("tweens"));
    assert_eq!(o.len(), e.len());
    for (f, (a, b)) in o.iter().zip(&e).enumerate() {
        assert_eq!(a, b, "frame {}", f + 1);
    }
}

/// The smoke fixture's linear tween matches Flash 8 to the twip.
#[test]
fn linear_tween_matches_flash_8_exactly() {
    let ours = publish_fixture("smoke", BuildMode::Release)
        .swf
        .expect("swf");
    let (o, oc) = root_states(&ours);
    let (e, ec) = root_states(&flash8("smoke"));
    assert_eq!(o.len(), e.len());
    for (f, (a, b)) in o.iter().zip(&e).enumerate() {
        let a: Vec<_> = a.values().filter(|p| oc.contains_key(&p.char)).collect();
        let b: Vec<_> = b.values().filter(|p| ec.contains_key(&p.char)).collect();
        assert_eq!(a.len(), b.len(), "frame {}", f + 1);
        for (x, y) in a.iter().zip(&b) {
            assert!(
                matrix_error(&x.matrix, &y.matrix) <= 1.0,
                "frame {}: {:?} vs {:?}",
                f + 1,
                x.matrix,
                y.matrix
            );
        }
    }
}

/// Named instances get Flash 8's names, colour transforms, filters, blend
/// modes and cacheAsBitmap, with PlaceObject3 only where needed.
#[test]
fn symbols_place_like_flash_8() {
    let ours = publish_fixture("symbols", BuildMode::Release)
        .swf
        .expect("swf");
    let collect = |bytes: &[u8]| -> BTreeMap<String, String> {
        with_parsed(bytes, |s| {
            let mut out = BTreeMap::new();
            for t in &s.tags {
                if let Tag::PlaceObject(p) = t
                    && let Some(n) = p.name
                {
                    out.insert(
                        n.to_str_lossy(swf::UTF_8).into_owned(),
                        format!(
                            "v{} m={:?} c={:?} f={:?} b={:?} cache={:?}",
                            p.version,
                            p.matrix,
                            p.color_transform,
                            p.filters,
                            p.blend_mode,
                            p.is_bitmap_cached
                        ),
                    );
                }
            }
            out
        })
    };
    let o = collect(&ours);
    let e = collect(&flash8("symbols"));
    assert_eq!(o.keys().collect::<Vec<_>>(), e.keys().collect::<Vec<_>>());
    for (k, v) in &e {
        assert_eq!(&o[k], v, "{k}");
    }
}

fn actions_per_frame(bytes: &[u8]) -> Vec<usize> {
    with_parsed(bytes, |s| {
        let mut out = vec![0];
        for t in &s.tags {
            match t {
                Tag::DoAction(_) => *out.last_mut().expect("frame") += 1,
                Tag::ShowFrame => out.push(0),
                _ => {}
            }
        }
        out.pop();
        out
    })
}

/// Frame scripts land on the same frames as in Flash 8, with the same
/// labels, button handlers and clip events.
#[test]
fn as2_scripts_are_placed_like_flash_8() {
    let ours = publish_fixture("as2", BuildMode::Release).swf.expect("swf");
    let f8 = flash8("as2");
    assert_eq!(actions_per_frame(&ours), actions_per_frame(&f8));
    let shape = |bytes: &[u8]| -> Vec<String> {
        with_parsed(bytes, |s| {
            let mut v = Vec::new();
            for t in &s.tags {
                match t {
                    Tag::FrameLabel(l) => {
                        v.push(format!("label {}", l.label.to_str_lossy(swf::UTF_8)))
                    }
                    Tag::DefineButton2(b) => v.push(format!(
                        "button records={} conditions={:?}",
                        b.records.len(),
                        b.actions.iter().map(|a| a.conditions).collect::<Vec<_>>()
                    )),
                    Tag::PlaceObject(p) if p.clip_actions.is_some() => {
                        let ca = p.clip_actions.as_ref().expect("checked");
                        v.push(format!(
                            "clip {:?} {:?}",
                            ca.all_event_flags,
                            ca.records.iter().map(|r| r.events).collect::<Vec<_>>()
                        ));
                    }
                    _ => {}
                }
            }
            v
        })
    };
    assert_eq!(shape(&ours), shape(&f8));
}

/// The stage build: no user scripts, `stop()` on frame 1 of every sprite,
/// and the `jz_goto` hook on root frame 1.
#[test]
fn stage_build_has_only_the_hook_and_stops() {
    let hook = jazari_as2::stage_goto_hook().expect("hook");
    for name in all_fixtures() {
        let swf = publish_fixture(&name, BuildMode::Stage).swf.expect("swf");
        with_parsed(&swf, |s| {
            let mut root_frame = 0;
            for t in &s.tags {
                match t {
                    Tag::ShowFrame => root_frame += 1,
                    Tag::DoAction(a) => {
                        assert_eq!(root_frame, 0, "{name}: root DoAction after frame 1");
                        assert_eq!(*a, hook.as_slice(), "{name}");
                    }
                    Tag::DoInitAction { .. } => panic!("{name}: DoInitAction in a stage build"),
                    Tag::DefineButton2(b) => {
                        assert!(b.actions.is_empty(), "{name}: button actions")
                    }
                    Tag::PlaceObject(p) => {
                        assert!(p.clip_actions.is_none(), "{name}: clip actions")
                    }
                    Tag::DefineSprite(sp) if sp.num_frames > 0 => {
                        let mut frame = 0;
                        let mut stops = 0;
                        for c in &sp.tags {
                            match c {
                                Tag::ShowFrame => frame += 1,
                                Tag::DoAction(a) => {
                                    assert_eq!(frame, 0, "{name}: sprite DoAction after frame 1");
                                    assert_eq!(*a, &[0x07u8, 0x00][..]);
                                    stops += 1;
                                }
                                Tag::PlaceObject(p) => {
                                    assert!(p.clip_actions.is_none(), "{name}: clip actions");
                                }
                                _ => {}
                            }
                        }
                        assert_eq!(stops, 1, "{name}: sprite {}", sp.id);
                    }
                    _ => {}
                }
            }
        });
    }
}
