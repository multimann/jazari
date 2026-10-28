//! Headless tests on the software adapter (WARP on Windows). They never use
//! a hardware GPU: `OffscreenRenderer` only accepts `DeviceType::Cpu`.
//!
//! Tests that need the Flash 8 reference movies skip themselves (with a
//! message) when the files are not on this machine.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use eframe::egui;
use jazari_preview::{
    LogKind, LogLine, OffscreenRenderer, PlayerSettings, PlayerStatus, PreviewPlayer,
    render_offscreen, run_headless,
};
use ruffle_core::swf::avm1::types::{Action, DefineFunction, Jump, Push, Value};
use ruffle_core::swf::avm1::write::Writer;
use ruffle_core::swf::{
    Color, Compression, EditText, FileAttributes, Fixed8, Header, Matrix, PlaceObject,
    PlaceObjectAction, Rectangle, SwfStr, Tag, Twips,
};

const SHAPES_BASIC: &str = r"C:\dev\samples\fixtures\shapes-basic\shapes-basic.swf";
const SMOKE: &str = r"C:\dev\samples\f8\smoke.swf";

/// One software device for every test in this file.
fn renderer() -> &'static OffscreenRenderer {
    static RENDERER: OnceLock<OffscreenRenderer> = OnceLock::new();
    RENDERER.get_or_init(|| {
        let renderer = OffscreenRenderer::new().expect("a software adapter (WARP) is required");
        assert_eq!(renderer.adapter_info().device_type, wgpu::DeviceType::Cpu);
        renderer
    })
}

fn fixture(path: &str) -> Option<Vec<u8>> {
    if Path::new(path).exists() {
        Some(std::fs::read(path).expect("readable fixture"))
    } else {
        eprintln!("skipping: {path} is not on this machine");
        None
    }
}

fn traces(lines: &[LogLine]) -> Vec<&str> {
    lines
        .iter()
        .filter(|l| l.kind == LogKind::Trace)
        .map(|l| l.text.as_str())
        .collect()
}

/// Asserts that a pixel is within `tolerance` of `rgb` on every channel.
fn assert_pixel(image: &image::RgbaImage, x: u32, y: u32, rgb: [u8; 3], tolerance: u8) {
    let p = image.get_pixel(x, y).0;
    let close = (0..3).all(|i| p[i].abs_diff(rgb[i]) <= tolerance);
    assert!(close, "pixel ({x}, {y}) is {p:?}, expected about {rgb:?}");
}

// ---------------------------------------------------------------------------
// Flash 8 reference movies

#[test]
fn shapes_basic_renders_like_the_manifest_describes() {
    let Some(swf) = fixture(SHAPES_BASIC) else {
        return;
    };
    let out = renderer().render(&swf, &[1], 1.0).expect("render");
    let image = &out.images[0];
    assert_eq!(image.dimensions(), (550, 400));
    assert_pixel(image, 540, 10, [0xFF, 0xFF, 0xFF], 0); // background
    assert_pixel(image, 340, 50, [0x33, 0x66, 0xCC], 2); // `solid`: blue rect
    assert_pixel(image, 440, 50, [0xFF, 0xCC, 0x00], 2); // `solid`: yellow rect
    assert_pixel(image, 440, 130, [0x99, 0xD4, 0x99], 3); // 50% #33AA33 over white
    assert_pixel(image, 310, 230, [0x00, 0x80, 0x80], 2); // `holes`: donut ring
    assert_pixel(image, 350, 230, [0xFF, 0xFF, 0xFF], 0); // donut hole
    assert_pixel(image, 270, 380, [0xFF, 0x00, 0x00], 2); // `back`: red rect
    assert_pixel(image, 450, 345, [0x00, 0x00, 0xFF], 2); // `front`: blue rounded rect
    assert_eq!(traces(&out.log), ["shapes-basic: frame 1"]);
    assert!(!out.halted);
}

#[test]
fn smoke_moves_the_box_and_traces_once() {
    let Some(swf) = fixture(SMOKE) else {
        return;
    };
    let out = renderer().render(&swf, &[20, 1], 1.0).expect("render");
    // Images come back in the order asked for.
    let (last, first) = (&out.images[0], &out.images[1]);
    let red = [0xCC, 0x33, 0x33];
    assert_pixel(first, 100, 100, red, 2); // the box starts at (100, 100)
    assert_pixel(first, 400, 250, [0xFF, 0xFF, 0xFF], 0);
    assert_pixel(last, 400, 250, red, 2); // and tweens to (400, 250)
    assert_pixel(last, 100, 100, [0xFF, 0xFF, 0xFF], 0);
    assert_eq!(traces(&out.log), ["hello from Flash 8"]);
}

#[test]
fn headless_runs_collect_traces_without_a_renderer() {
    let Some(swf) = fixture(SMOKE) else {
        return;
    };
    // 20 frames, no stop(): frame 1 runs again at frame 21.
    let log = run_headless(&swf, 21).expect("run");
    assert_eq!(traces(&log), ["hello from Flash 8", "hello from Flash 8"]);
}

#[test]
fn render_offscreen_scales_the_stage() {
    let Some(swf) = fixture(SMOKE) else {
        return;
    };
    let images = render_offscreen(&swf, &[1], 0.5).expect("render");
    assert_eq!(images[0].dimensions(), (275, 200));
}

#[test]
fn frame_zero_is_rejected() {
    let swf = movie(&[actions(&[])]);
    let err = renderer().render(&swf, &[0], 1.0).unwrap_err();
    assert!(err.to_string().contains("1-based"), "{err}");
}

#[test]
fn garbage_is_not_a_movie() {
    let (tx, _rx) = mpsc::channel();
    let err = PreviewPlayer::new(
        renderer().descriptors(),
        b"not a swf".to_vec(),
        (100, 100),
        1.0,
        tx,
    )
    .err()
    .expect("must fail");
    assert!(err.to_string().contains("not a playable SWF"), "{err}");
}

// ---------------------------------------------------------------------------
// PreviewPlayer on the software device, with generated movies

const BACKGROUND: Color = Color {
    r: 0x20,
    g: 0x40,
    b: 0x60,
    a: 0xFF,
};

/// AVM1 bytecode for a frame, ending with `End`.
fn actions(list: &[Action<'_>]) -> Vec<u8> {
    let mut bytes = body(list);
    bytes.push(0); // ActionEnd
    bytes
}

/// AVM1 bytecode without the final `End` (a function body).
fn body(list: &[Action<'_>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut writer = Writer::new(&mut bytes, 8);
    for action in list {
        writer.write_action(action).expect("encodable action");
    }
    bytes
}

fn s(text: &str) -> &SwfStr {
    SwfStr::from_utf8_str(text)
}

fn push(values: Vec<Value<'_>>) -> Action<'_> {
    Action::Push(Push { values })
}

fn trace(text: &str) -> Vec<u8> {
    actions(&[push(vec![Value::Str(s(text))]), Action::Trace])
}

/// A 400x300 SWF 8 with `frames` frames and the given tags after
/// FileAttributes and SetBackgroundColor.
fn write_movie(frames: usize, background: Color, body: Vec<Tag<'_>>) -> Vec<u8> {
    let header = Header {
        compression: Compression::Zlib,
        version: 8,
        stage_size: Rectangle {
            x_min: Twips::ZERO,
            x_max: Twips::from_pixels(400.0),
            y_min: Twips::ZERO,
            y_max: Twips::from_pixels(300.0),
        },
        frame_rate: Fixed8::from_f32(12.0),
        num_frames: u16::try_from(frames).expect("few frames"),
    };
    let mut tags = vec![
        Tag::FileAttributes(FileAttributes::empty()),
        Tag::SetBackgroundColor(background),
    ];
    tags.extend(body);
    let mut out = Vec::new();
    ruffle_core::swf::write_swf(&header, &tags, &mut out).expect("writable SWF");
    out
}

/// A 400x300 SWF 8 with one DoAction per frame.
fn movie(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut tags = Vec::new();
    for frame in frames {
        tags.push(Tag::DoAction(frame));
        tags.push(Tag::ShowFrame);
    }
    write_movie(frames.len(), BACKGROUND, tags)
}

/// A one-frame movie on white with an HTML dynamic text field showing
/// `html`, which names its font with `<font face=...>` (a device font).
fn text_movie(html: &str) -> Vec<u8> {
    let field = EditText::new()
        .with_id(1)
        .with_bounds(Rectangle {
            x_min: Twips::ZERO,
            x_max: Twips::from_pixels(400.0),
            y_min: Twips::ZERO,
            y_max: Twips::from_pixels(100.0),
        })
        .with_default_font()
        .with_is_html(true)
        .with_is_read_only(true)
        .with_initial_text(Some(s(html)));
    let place = PlaceObject {
        version: 2,
        action: PlaceObjectAction::Place(1),
        depth: 1,
        matrix: Some(Matrix::IDENTITY),
        color_transform: None,
        ratio: None,
        name: None,
        clip_depth: None,
        class_name: None,
        filters: None,
        background_color: None,
        blend_mode: None,
        clip_actions: None,
        has_image: false,
        is_bitmap_cached: None,
        is_visible: None,
        amf_data: None,
    };
    let white = Color {
        r: 0xFF,
        g: 0xFF,
        b: 0xFF,
        a: 0xFF,
    };
    write_movie(
        1,
        white,
        vec![
            Tag::DefineEditText(Box::new(field)),
            Tag::PlaceObject(Box::new(place)),
            Tag::ShowFrame,
        ],
    )
}

fn dark_pixels(image: &image::RgbaImage) -> usize {
    image.pixels().filter(|p| p.0[0] < 128).count()
}

#[test]
fn arabic_device_font_is_registered() {
    let swf = text_movie(
        "<p><font face=\"Noto Sans Arabic\" size=\"40\" color=\"#000000\">سلام عليكم</font></p>",
    );
    let with = renderer().render(&swf, &[1], 1.0).expect("render");
    let without = renderer()
        .render_with(
            &swf,
            &[1],
            1.0,
            &PlayerSettings {
                arabic_device_font: false,
                ..PlayerSettings::default()
            },
        )
        .expect("render");
    let unknown = |out: &jazari_preview::OffscreenOutput| {
        out.log
            .iter()
            .any(|l| l.text.contains("Unknown device font") && l.text.contains("Noto Sans Arabic"))
    };
    assert!(!unknown(&with), "{:?}", with.log);
    assert!(unknown(&without), "{:?}", without.log);
    // For review by eye: Ruffle neither shapes nor reorders device text.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    for (name, out) in [("with", &with), ("without", &without)] {
        let path = dir.join(format!("arabic-device-font-{name}.png"));
        out.images[0].save(&path).expect("writable PNG");
        eprintln!("wrote {}", path.display());
    }
    // Noto Sans Arabic has the glyphs; Ruffle's Latin fallback does not.
    let (drawn, fallback) = (
        dark_pixels(&with.images[0]),
        dark_pixels(&without.images[0]),
    );
    assert!(drawn > 200, "only {drawn} dark pixels with the Arabic font");
    assert!(drawn > fallback * 2, "{drawn} vs {fallback} dark pixels");
}

/// What a stage build puts on root frame 1: `stop();` and
/// `flash.external.ExternalInterface.addCallback("jz_goto", null,
/// function (f) { _root.gotoAndStop(f); });` (plus a trace for the test).
fn stage_build_frame_1() -> Vec<u8> {
    let goto = body(&[
        push(vec![Value::Str(s("f"))]),
        Action::GetVariable,
        push(vec![Value::Int(1), Value::Str(s("_root"))]),
        Action::GetVariable,
        push(vec![Value::Str(s("gotoAndStop"))]),
        Action::CallMethod,
        Action::Pop,
    ]);
    actions(&[
        Action::Stop,
        Action::DefineFunction(DefineFunction {
            name: s(""),
            params: vec![s("f")],
            actions: &goto,
        }),
        push(vec![
            Value::Null,
            Value::Str(s("jz_goto")),
            Value::Int(3),
            Value::Str(s("flash")),
        ]),
        Action::GetVariable,
        push(vec![Value::Str(s("external"))]),
        Action::GetMember,
        push(vec![Value::Str(s("ExternalInterface"))]),
        Action::GetMember,
        push(vec![Value::Str(s("addCallback"))]),
        Action::CallMethod,
        Action::Trace, // traces addCallback's result: "true"
        push(vec![Value::Str(s("f1"))]),
        Action::Trace,
    ])
}

fn player(swf: Vec<u8>, settings: PlayerSettings) -> (PreviewPlayer, Receiver<LogLine>) {
    let (tx, rx) = mpsc::channel();
    let player =
        PreviewPlayer::with_settings(renderer().descriptors(), swf, (400, 300), 1.0, tx, settings)
            .expect("player");
    (player, rx)
}

fn drain(rx: &Receiver<LogLine>) -> Vec<LogLine> {
    rx.try_iter().collect()
}

#[test]
fn goto_and_stop_uses_the_stage_hook() {
    let swf = movie(&[stage_build_frame_1(), trace("f2"), trace("f3"), trace("f4")]);
    let (mut player, rx) = player(swf, PlayerSettings::default());
    assert_eq!(traces(&drain(&rx)), ["true", "f1"]);
    assert_eq!(player.current_frame(), Some(1));

    player.goto_and_stop(3);
    assert_eq!(traces(&drain(&rx)), ["f3"]);
    assert_eq!(player.current_frame(), Some(3));
    assert!(!player.is_playing());

    player.goto_and_stop(2); // backwards
    assert_eq!(traces(&drain(&rx)), ["f2"]);
    assert_eq!(player.current_frame(), Some(2));

    // Paused: ticking does not move the playhead.
    player.tick(Duration::from_secs(1));
    assert_eq!(player.current_frame(), Some(2));
    assert_eq!(player.status(), PlayerStatus::Running);
}

#[test]
fn goto_frame_zero_is_refused() {
    let swf = movie(&[stage_build_frame_1(), trace("f2")]);
    let (mut player, rx) = player(swf, PlayerSettings::default());
    player.goto_and_stop(2);
    drain(&rx);
    player.goto_and_stop(0);
    let log = drain(&rx);
    assert!(traces(&log).is_empty(), "{log:?}");
    assert!(
        log.iter()
            .any(|l| l.kind == LogKind::Preview && l.text.contains("1-based")),
        "{log:?}"
    );
    assert_eq!(player.current_frame(), Some(2));
}

#[test]
fn a_movie_without_the_hook_is_reported_once() {
    let swf = movie(&[trace("f1"), trace("f2")]);
    let (mut player, rx) = player(swf, PlayerSettings::default());
    drain(&rx);
    player.goto_and_stop(2);
    player.goto_and_stop(2);
    let preview: Vec<_> = drain(&rx)
        .into_iter()
        .filter(|l| l.kind == LogKind::Preview)
        .collect();
    assert_eq!(preview.len(), 1, "{preview:?}");
    assert!(preview[0].text.contains("jz_goto"));
}

#[test]
fn playback_renders_into_the_texture() {
    let swf = movie(&[trace("f1"), trace("f2"), trace("f3")]);
    let (mut player, rx) = player(swf, PlayerSettings::default());
    assert!(player.is_playing());
    assert_eq!(player.repaint_after(), Some(Duration::ZERO));
    player.render();
    let image = player.capture_image().expect("readback");
    assert_eq!(image.dimensions(), (400, 300));
    assert_pixel(
        &image,
        200,
        150,
        [BACKGROUND.r, BACKGROUND.g, BACKGROUND.b],
        0,
    );

    // 12 fps: a quarter of a second runs about three frames, capped at the
    // movie's end, where it loops.
    for _ in 0..3 {
        player.tick(Duration::from_millis(84));
    }
    assert_eq!(traces(&drain(&rx)), ["f1", "f2", "f3", "f1"]);
    player.set_playing(false);
    player.render();
    assert_eq!(player.repaint_after(), None);
}

#[test]
fn resizing_recreates_and_clamps_the_texture() {
    let swf = movie(&[trace("f1")]);
    let (mut player, _rx) = player(swf, PlayerSettings::default());
    let first = player.texture_generation();
    player.resize(200, 150, 1.0);
    assert_ne!(player.texture_generation(), first);
    assert_eq!(player.size_px(), (200, 150));
    player.render();
    let image = player.capture_image().expect("readback");
    assert_eq!(image.dimensions(), (200, 150));

    // Same size again: nothing is recreated.
    let second = player.texture_generation();
    player.resize(200, 150, 1.0);
    assert_eq!(player.texture_generation(), second);

    let max = renderer().descriptors().limits.max_texture_dimension_2d;
    player.resize(max + 1000, 0, 1.0);
    assert_eq!(player.size_px(), (max, 1));
}

#[test]
fn a_script_timeout_halts_and_is_reported() {
    // Frame 1: an endless loop (a jump to itself).
    let endless = actions(&[Action::Jump(Jump { offset: -5 })]);
    let swf = movie(&[endless, trace("f2")]);
    let settings = PlayerSettings {
        max_execution_duration: Duration::from_millis(100),
        ..PlayerSettings::default()
    };
    let (mut player, rx) = player(swf, settings);
    assert!(player.is_halted());
    assert_eq!(player.status(), PlayerStatus::Halted);
    let log = drain(&rx);
    assert!(
        log.iter()
            .any(|l| l.kind == LogKind::Ruffle && l.text.contains("taken too long")),
        "{log:?}"
    );
    assert!(log.iter().any(|l| l.kind == LogKind::Preview), "{log:?}");

    // The timeline still plays, but frame 2's trace never runs.
    player.step_frame();
    assert!(traces(&drain(&rx)).is_empty());

    // A restart rebuilds the player (and halts again on the same movie).
    player.restart().expect("restart");
    assert_eq!(player.status(), PlayerStatus::Halted);
}

#[test]
fn pointer_input_reaches_the_player() {
    let swf = movie(&[trace("f1")]);
    let (mut player, _rx) = player(swf, PlayerSettings::default());
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
    player.handle_events(
        &[
            egui::Event::PointerMoved(egui::pos2(10.0, 10.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(10.0, 10.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: egui::pos2(10.0, 10.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerMoved(egui::pos2(900.0, 10.0)),
        ],
        rect,
    );
    player.set_focused(true);
    player.handle_event(
        &egui::Event::Key {
            key: egui::Key::ArrowLeft,
            physical_key: Some(egui::Key::ArrowLeft),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        },
        rect,
    );
    player.set_focused(false);
    assert_eq!(player.status(), PlayerStatus::Running);
    assert_eq!(player.cursor_icon(), egui::CursorIcon::Default);
}

#[test]
fn players_share_one_device() {
    let desc = renderer().descriptors();
    let swf = movie(&[trace("f1")]);
    let (tx, _rx) = mpsc::channel();
    let a =
        PreviewPlayer::new(Arc::clone(&desc), swf.clone(), (64, 48), 1.0, tx.clone()).expect("a");
    let b = PreviewPlayer::new(desc, swf, (64, 48), 1.0, tx).expect("b");
    assert_ne!(a.texture_generation(), b.texture_generation());
    assert_eq!(a.adapter_info().device_type, wgpu::DeviceType::Cpu);
}
