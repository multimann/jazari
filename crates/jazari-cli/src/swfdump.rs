//! `jazari-cli dump --swf <file>`: the tags of a SWF, one per line, with
//! sprites indented and every action block disassembled with
//! [`jazari_as2::disasm`].
//!
//! Coordinates are shown in pixels (twips / 20, so two decimals are exact);
//! matrix factors as decimals. Frames are numbered from 1.

use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};

use swf::{
    ButtonActionCondition, ClipEventFlag, Color, ColorTransform, FillStyle, Filter, Fixed8,
    Fixed16, Gradient, Matrix, PlaceObject, PlaceObjectAction, Rectangle, ShapeRecord, SwfStr, Tag,
    Twips,
};

use crate::error::{CliError, Result};

/// How much detail to print.
#[derive(Debug, Clone, Copy, Default)]
pub struct SwfDumpOptions {
    /// Also list shape records, glyph entries and font code tables.
    pub full: bool,
}

/// The parsed parts of a SWF that the dump and the other commands need.
pub struct SwfInfo {
    /// SWF version.
    pub version: u8,
    /// Frame count from the header.
    pub num_frames: u16,
    /// Stage width and height in pixels.
    pub stage_px: (f64, f64),
    /// Frame rate.
    pub frame_rate: f64,
}

/// Reads the header of `bytes` (`what` names the movie in errors).
pub fn info(bytes: &[u8], what: &str) -> Result<SwfInfo> {
    guarded(what, || info_inner(bytes, what))
}

fn info_inner(bytes: &[u8], what: &str) -> Result<SwfInfo> {
    let buf = swf::decompress_swf(bytes).map_err(|e| CliError::Swf {
        what: what.to_owned(),
        message: e.to_string(),
    })?;
    let header = &buf.header;
    let stage = header.stage_size();
    Ok(SwfInfo {
        version: header.version(),
        num_frames: header.num_frames(),
        stage_px: (
            (stage.x_max - stage.x_min).to_pixels(),
            (stage.y_max - stage.y_min).to_pixels(),
        ),
        frame_rate: header.frame_rate().to_f64(),
    })
}

/// Formats the whole tag dump of `bytes`.
pub fn dump(bytes: &[u8], what: &str, opts: SwfDumpOptions) -> Result<String> {
    guarded(what, || dump_inner(bytes, what, opts))
}

/// Runs `f`, turning a panic in the SWF parser or the disassembler (broken
/// or hostile input) into an error.
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        Err(CliError::Swf {
            what: what.to_owned(),
            message: "the SWF reader failed on this file".to_owned(),
        })
    })
}

fn dump_inner(bytes: &[u8], what: &str, opts: SwfDumpOptions) -> Result<String> {
    let buf = swf::decompress_swf(bytes).map_err(|e| CliError::Swf {
        what: what.to_owned(),
        message: e.to_string(),
    })?;
    let parsed = swf::parse_swf(&buf).map_err(|e| CliError::Swf {
        what: what.to_owned(),
        message: e.to_string(),
    })?;
    let header = &parsed.header;
    let stage = header.stage_size();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "SWF version {}, {:?} compression, {} bytes ({} uncompressed)",
        header.version(),
        header.compression(),
        bytes.len(),
        header.uncompressed_len()
    );
    let _ = writeln!(
        out,
        "stage {} x {} px, {} fps, {} frame{}, {} tag{}",
        px(stage.x_max - stage.x_min),
        px(stage.y_max - stage.y_min),
        num(header.frame_rate().to_f64()),
        header.num_frames(),
        plural(usize::from(header.num_frames())),
        parsed.tags.len(),
        plural(parsed.tags.len())
    );
    let mut d = Dumper {
        out,
        version: header.version(),
        opts,
    };
    d.tags(&parsed.tags, 0);
    Ok(d.out)
}

struct Dumper {
    out: String,
    version: u8,
    opts: SwfDumpOptions,
}

impl Dumper {
    fn line(&mut self, indent: usize, text: &str) {
        let _ = writeln!(self.out, "{:indent$}{text}", "", indent = indent * 4);
    }

    fn tags(&mut self, tags: &[Tag<'_>], indent: usize) {
        let mut frame = 1u32;
        let mut frame_started = false;
        for tag in tags {
            if !frame_started {
                self.line(indent, &format!("-- frame {frame}"));
                frame_started = true;
            }
            self.tag(tag, indent);
            if matches!(tag, Tag::ShowFrame) {
                frame += 1;
                frame_started = false;
            }
        }
    }

    fn actions(&mut self, data: &[u8], indent: usize) {
        match jazari_as2::disasm::disassemble(data, self.version) {
            Ok(list) => {
                for instruction in &list {
                    self.line(indent, &instruction.to_string());
                }
            }
            Err(e) => self.line(indent, &format!("<{e}>")),
        }
    }

    fn tag(&mut self, tag: &Tag<'_>, indent: usize) {
        match tag {
            Tag::ShowFrame => self.line(indent, "ShowFrame"),
            Tag::FileAttributes(a) => self.line(indent, &format!("FileAttributes {a:?}")),
            Tag::SetBackgroundColor(c) => {
                self.line(indent, &format!("SetBackgroundColor {}", color(c)))
            }
            Tag::DoAction(data) => {
                self.line(indent, &format!("DoAction ({} bytes)", data.len()));
                self.actions(data, indent + 1);
            }
            Tag::DoInitAction { id, action_data } => {
                self.line(
                    indent,
                    &format!("DoInitAction sprite={id} ({} bytes)", action_data.len()),
                );
                self.actions(action_data, indent + 1);
            }
            Tag::FrameLabel(l) => self.line(
                indent,
                &format!(
                    "FrameLabel {:?}{}",
                    text(l.label),
                    if l.is_anchor { " (anchor)" } else { "" }
                ),
            ),
            Tag::PlaceObject(p) => self.place(p, indent),
            Tag::RemoveObject(r) => self.line(
                indent,
                &match r.character_id {
                    Some(id) => format!("RemoveObject depth={} char={id}", r.depth),
                    None => format!("RemoveObject2 depth={}", r.depth),
                },
            ),
            Tag::DefineSprite(s) => {
                self.line(
                    indent,
                    &format!(
                        "DefineSprite id={} frames={} ({} tags)",
                        s.id,
                        s.num_frames,
                        s.tags.len()
                    ),
                );
                self.tags(&s.tags, indent + 1);
            }
            Tag::DefineShape(s) => {
                let records = s.shape.len();
                let edges = s
                    .shape
                    .iter()
                    .filter(|r| !matches!(r, ShapeRecord::StyleChange(_)))
                    .count();
                let blocks = 1 + s
                    .shape
                    .iter()
                    .filter(|r| matches!(r, ShapeRecord::StyleChange(c) if c.new_styles.is_some()))
                    .count();
                self.line(
                    indent,
                    &format!(
                        "DefineShape{} id={} bounds {} edges {} ({records} records, {edges} edges, {blocks} style block{}){}",
                        if s.version == 1 {
                            String::new()
                        } else {
                            s.version.to_string()
                        },
                        s.id,
                        rect(&s.shape_bounds),
                        rect(&s.edge_bounds),
                        plural(blocks),
                        if s.flags.is_empty() {
                            String::new()
                        } else {
                            format!(" {:?}", s.flags)
                        }
                    ),
                );
                self.styles(&s.styles, indent + 1, 1);
                let mut block = 1;
                for record in &s.shape {
                    if let ShapeRecord::StyleChange(c) = record
                        && let Some(styles) = &c.new_styles
                    {
                        block += 1;
                        self.styles(styles, indent + 1, block);
                    }
                    if self.opts.full {
                        self.line(indent + 2, &shape_record(record));
                    }
                }
            }
            Tag::DefineButton2(b) | Tag::DefineButton(b) => {
                let v = if matches!(tag, Tag::DefineButton2(_)) {
                    "2"
                } else {
                    ""
                };
                self.line(
                    indent,
                    &format!(
                        "DefineButton{v} id={}{} ({} records, {} actions)",
                        b.id,
                        if b.is_track_as_menu {
                            " track-as-menu"
                        } else {
                            ""
                        },
                        b.records.len(),
                        b.actions.len()
                    ),
                );
                for r in &b.records {
                    let mut s = format!(
                        "record {:?} char={} depth={} {}",
                        r.states,
                        r.id,
                        r.depth,
                        matrix(&r.matrix)
                    );
                    if r.color_transform != ColorTransform::IDENTITY {
                        let _ = write!(s, " {}", cxform(&r.color_transform));
                    }
                    if !r.filters.is_empty() {
                        let _ = write!(s, " filters [{}]", filters(&r.filters));
                    }
                    if r.blend_mode != swf::BlendMode::Normal {
                        let _ = write!(s, " blend={:?}", r.blend_mode);
                    }
                    self.line(indent + 1, &s);
                }
                for a in &b.actions {
                    self.line(
                        indent + 1,
                        &format!(
                            "on {}{} ({} bytes)",
                            button_conditions(a.conditions),
                            a.key_press()
                                .map(|k| format!(" key={k}"))
                                .unwrap_or_default(),
                            a.action_data.len()
                        ),
                    );
                    self.actions(a.action_data, indent + 2);
                }
            }
            Tag::DefineEditText(e) => {
                let mut s = format!("DefineEditText id={} bounds {}", e.id(), rect(e.bounds()));
                if let Some(font) = e.font_id() {
                    let _ = write!(s, " font={font}");
                }
                if let Some(class) = e.font_class() {
                    let _ = write!(s, " font-class={:?}", text(class));
                }
                if let Some(h) = e.height() {
                    let _ = write!(s, " height={}", px(h));
                }
                if let Some(c) = e.color() {
                    let _ = write!(s, " color={}", color(&c));
                }
                if let Some(m) = e.max_length() {
                    let _ = write!(s, " max={m}");
                }
                let var = text(e.variable_name());
                if !var.is_empty() {
                    let _ = write!(s, " var={var:?}");
                }
                let _ = write!(s, " {:?}", e.flags());
                self.line(indent, &s);
                if let Some(layout) = e.layout() {
                    self.line(
                        indent + 1,
                        &format!(
                            "layout {:?} margins {} {} indent {} leading {}",
                            layout.align,
                            px(layout.left_margin),
                            px(layout.right_margin),
                            px(layout.indent),
                            px(layout.leading)
                        ),
                    );
                }
                if let Some(t) = e.initial_text() {
                    self.line(indent + 1, &format!("text {:?}", text(t)));
                }
            }
            Tag::DefineText(t) | Tag::DefineText2(t) => {
                let v = if matches!(tag, Tag::DefineText2(_)) {
                    "2"
                } else {
                    ""
                };
                self.line(
                    indent,
                    &format!(
                        "DefineText{v} id={} bounds {} {} ({} records)",
                        t.id,
                        rect(&t.bounds),
                        matrix(&t.matrix),
                        t.records.len()
                    ),
                );
                for r in &t.records {
                    let mut s = String::from("record");
                    if let Some(f) = r.font_id {
                        let _ = write!(s, " font={f}");
                    }
                    if let Some(h) = r.height {
                        let _ = write!(s, " height={}", px(h));
                    }
                    if let Some(c) = &r.color {
                        let _ = write!(s, " color={}", color(c));
                    }
                    if let Some(x) = r.x_offset {
                        let _ = write!(s, " x={}", px(x));
                    }
                    if let Some(y) = r.y_offset {
                        let _ = write!(s, " y={}", px(y));
                    }
                    let _ = write!(s, " {} glyph{}", r.glyphs.len(), plural(r.glyphs.len()));
                    if self.opts.full {
                        let glyphs: Vec<String> = r
                            .glyphs
                            .iter()
                            .map(|g| format!("{}:{}", g.index, g.advance))
                            .collect();
                        let _ = write!(s, " [{}]", glyphs.join(" "));
                    }
                    self.line(indent + 1, &s);
                }
            }
            Tag::DefineFont2(f) => {
                let mut s = format!(
                    "DefineFont{} id={} name={:?} {} glyph{} {:?} lang={:?}",
                    f.version,
                    f.id,
                    text(f.name),
                    f.glyphs.len(),
                    plural(f.glyphs.len()),
                    f.flags,
                    f.language
                );
                if let Some(layout) = &f.layout {
                    let _ = write!(
                        s,
                        " ascent={} descent={} leading={} kerning={}",
                        layout.ascent,
                        layout.descent,
                        layout.leading,
                        layout.kerning.len()
                    );
                }
                self.line(indent, &s);
                if self.opts.full {
                    let codes: Vec<String> = f
                        .glyphs
                        .iter()
                        .map(|g| match char::from_u32(u32::from(g.code)) {
                            Some(c) if !c.is_control() => {
                                format!("{c}(U+{:04X}):{}", g.code, g.advance)
                            }
                            _ => format!("U+{:04X}:{}", g.code, g.advance),
                        })
                        .collect();
                    self.line(indent + 1, &format!("codes [{}]", codes.join(" ")));
                }
            }
            Tag::DefineFont(f) => self.line(
                indent,
                &format!("DefineFont id={} {} glyphs", f.id, f.glyphs.len()),
            ),
            Tag::DefineFontAlignZones { id, thickness, .. } => self.line(
                indent,
                &format!("DefineFontAlignZones font={id} {thickness:?}"),
            ),
            Tag::DefineFontName { id, name, .. } => self.line(
                indent,
                &format!("DefineFontName font={id} name={:?}", text(name)),
            ),
            Tag::CsmTextSettings(c) => self.line(
                indent,
                &format!(
                    "CSMTextSettings text={} advanced={} grid={:?} thickness={} sharpness={}",
                    c.id, c.use_advanced_rendering, c.grid_fit, c.thickness, c.sharpness
                ),
            ),
            Tag::DefineBitsLossless(b) => self.line(
                indent,
                &format!(
                    "DefineBitsLossless{} id={} {}x{} {:?} ({} bytes)",
                    if b.version == 1 {
                        String::new()
                    } else {
                        b.version.to_string()
                    },
                    b.id,
                    b.width,
                    b.height,
                    b.format,
                    b.data.len()
                ),
            ),
            Tag::DefineBitsJpeg2 { id, jpeg_data } => self.line(
                indent,
                &format!("DefineBitsJPEG2 id={id} ({} bytes)", jpeg_data.len()),
            ),
            Tag::DefineBitsJpeg3(j) => self.line(
                indent,
                &format!(
                    "DefineBitsJPEG{} id={} ({} bytes, alpha {} bytes)",
                    j.version,
                    j.id,
                    j.data.len(),
                    j.alpha_data.len()
                ),
            ),
            Tag::DefineBits { id, jpeg_data } => self.line(
                indent,
                &format!("DefineBits id={id} ({} bytes)", jpeg_data.len()),
            ),
            Tag::JpegTables(t) => self.line(indent, &format!("JPEGTables ({} bytes)", t.len())),
            Tag::ExportAssets(list) => {
                let names: Vec<String> = list
                    .iter()
                    .map(|a| format!("{}={:?}", a.id, text(a.name)))
                    .collect();
                self.line(indent, &format!("ExportAssets {}", names.join(", ")));
            }
            Tag::Protect(_) => self.line(indent, "Protect"),
            Tag::EnableDebugger(_) => self.line(indent, "EnableDebugger"),
            Tag::ScriptLimits {
                max_recursion_depth,
                timeout_in_seconds,
            } => self.line(
                indent,
                &format!(
                    "ScriptLimits recursion={max_recursion_depth} timeout={timeout_in_seconds}s"
                ),
            ),
            Tag::DefineScalingGrid { id, splitter_rect } => self.line(
                indent,
                &format!("DefineScalingGrid char={id} {}", rect(splitter_rect)),
            ),
            Tag::Unknown { tag_code, data } => self.line(
                indent,
                &format!("Unknown tag {tag_code} ({} bytes)", data.len()),
            ),
            Tag::End => self.line(indent, "End"),
            other => {
                let debug: String = format!("{other:?}").chars().take(100).collect();
                self.line(indent, &debug);
            }
        }
    }

    fn place(&mut self, p: &PlaceObject<'_>, indent: usize) {
        let mut s = format!(
            "PlaceObject{} depth={} ",
            if p.version == 1 {
                String::new()
            } else {
                p.version.to_string()
            },
            p.depth
        );
        match p.action {
            PlaceObjectAction::Place(id) => {
                let _ = write!(s, "place char={id}");
            }
            PlaceObjectAction::Modify => s.push_str("modify"),
            PlaceObjectAction::Replace(id) => {
                let _ = write!(s, "replace char={id}");
            }
        }
        if let Some(name) = p.name {
            let _ = write!(s, " name={:?}", text(name));
        }
        if let Some(m) = &p.matrix {
            let _ = write!(s, " {}", matrix(m));
        }
        if let Some(c) = &p.color_transform {
            let _ = write!(s, " {}", cxform(c));
        }
        if let Some(r) = p.ratio {
            let _ = write!(s, " ratio={r}");
        }
        if let Some(c) = p.clip_depth {
            let _ = write!(s, " clip-depth={c}");
        }
        if let Some(f) = &p.filters {
            let _ = write!(s, " filters [{}]", filters(f));
        }
        if let Some(b) = p.blend_mode {
            let _ = write!(s, " blend={b:?}");
        }
        if let Some(c) = p.is_bitmap_cached {
            let _ = write!(s, " cache-as-bitmap={c}");
        }
        if let Some(v) = p.is_visible {
            let _ = write!(s, " visible={v}");
        }
        if let Some(class) = p.class_name {
            let _ = write!(s, " class={:?}", text(class));
        }
        self.line(indent, &s);
        if let Some(actions) = &p.clip_actions {
            self.line(
                indent + 1,
                &format!(
                    "clip actions, all events {}",
                    clip_events(actions.all_event_flags)
                ),
            );
            for a in &actions.records {
                self.line(
                    indent + 1,
                    &format!(
                        "onClipEvent {}{} ({} bytes)",
                        clip_events(a.events),
                        a.key_code.map(|k| format!(" key={k}")).unwrap_or_default(),
                        a.action_data.len()
                    ),
                );
                self.actions(a.action_data, indent + 2);
            }
        }
    }

    fn styles(&mut self, styles: &swf::ShapeStyles, indent: usize, block: usize) {
        let fills: Vec<String> = styles.fill_styles.iter().map(fill).collect();
        let lines: Vec<String> = styles
            .line_styles
            .iter()
            .map(|l| {
                let mut s = format!(
                    "{} {}",
                    px(l.width()),
                    match l.fill_style() {
                        FillStyle::Color(c) => color(c),
                        other => fill(other),
                    }
                );
                if self.version >= 8 {
                    let _ = write!(
                        s,
                        " caps {:?}/{:?} join {:?}",
                        l.start_cap(),
                        l.end_cap(),
                        l.join_style()
                    );
                    if !l.allow_scale_x() || !l.allow_scale_y() {
                        let _ = write!(
                            s,
                            " scale{}{}",
                            if l.allow_scale_x() { " h" } else { "" },
                            if l.allow_scale_y() { " v" } else { "" }
                        );
                    }
                }
                s
            })
            .collect();
        self.line(
            indent,
            &format!(
                "styles {block}: fills [{}] lines [{}]",
                fills.join(", "),
                lines.join(", ")
            ),
        );
    }
}

fn shape_record(r: &ShapeRecord) -> String {
    match r {
        ShapeRecord::StyleChange(c) => {
            let mut s = String::from("style");
            if let Some(m) = &c.move_to {
                let _ = write!(s, " move {},{}", px(m.x), px(m.y));
            }
            if let Some(f) = c.fill_style_0 {
                let _ = write!(s, " fill0={f}");
            }
            if let Some(f) = c.fill_style_1 {
                let _ = write!(s, " fill1={f}");
            }
            if let Some(l) = c.line_style {
                let _ = write!(s, " line={l}");
            }
            if c.new_styles.is_some() {
                s.push_str(" new-styles");
            }
            s
        }
        ShapeRecord::StraightEdge { delta } => {
            format!("line {},{}", px(delta.dx), px(delta.dy))
        }
        ShapeRecord::CurvedEdge {
            control_delta,
            anchor_delta,
        } => format!(
            "curve {},{} {},{}",
            px(control_delta.dx),
            px(control_delta.dy),
            px(anchor_delta.dx),
            px(anchor_delta.dy)
        ),
    }
}

fn fill(f: &FillStyle) -> String {
    match f {
        FillStyle::Color(c) => format!("solid {}", color(c)),
        FillStyle::LinearGradient(g) => format!("linear {}", gradient(g)),
        FillStyle::RadialGradient(g) => format!("radial {}", gradient(g)),
        FillStyle::FocalGradient {
            gradient: g,
            focal_point,
        } => format!(
            "focal {} focal={} ({}/256)",
            gradient(g),
            num(focal_point.to_f64()),
            focal_point.get()
        ),
        FillStyle::Bitmap {
            id,
            matrix: m,
            is_smoothed,
            is_repeating,
        } => format!(
            "bitmap char={id} {} {} {}",
            matrix(m),
            if *is_smoothed { "smooth" } else { "hard" },
            if *is_repeating { "repeat" } else { "clip" }
        ),
    }
}

fn gradient(g: &Gradient) -> String {
    let stops: Vec<String> = g
        .records
        .iter()
        .map(|r| format!("{}@{}", color(&r.color), r.ratio))
        .collect();
    format!(
        "{:?} {:?} {} [{}]",
        g.spread,
        g.interpolation,
        matrix(&g.matrix),
        stops.join(" ")
    )
}

fn filters(list: &[Filter]) -> String {
    list.iter().map(filter).collect::<Vec<_>>().join(", ")
}

fn filter(f: &Filter) -> String {
    match f {
        Filter::DropShadowFilter(d) => format!(
            "drop-shadow {} blur {}x{} angle {} distance {} strength {} {:?}",
            color(&d.color),
            fx16(d.blur_x),
            fx16(d.blur_y),
            fx16(d.angle),
            fx16(d.distance),
            fx8(d.strength),
            d.flags
        ),
        Filter::BlurFilter(b) => {
            format!("blur {}x{} {:?}", fx16(b.blur_x), fx16(b.blur_y), b.flags)
        }
        Filter::GlowFilter(g) => format!(
            "glow {} blur {}x{} strength {} {:?}",
            color(&g.color),
            fx16(g.blur_x),
            fx16(g.blur_y),
            fx8(g.strength),
            g.flags
        ),
        Filter::BevelFilter(b) => format!(
            "bevel highlight {} shadow {} blur {}x{} angle {} distance {} strength {} {:?}",
            color(&b.highlight_color),
            color(&b.shadow_color),
            fx16(b.blur_x),
            fx16(b.blur_y),
            fx16(b.angle),
            fx16(b.distance),
            fx8(b.strength),
            b.flags
        ),
        Filter::GradientGlowFilter(_) => "gradient-glow".to_owned(),
        Filter::GradientBevelFilter(_) => "gradient-bevel".to_owned(),
        Filter::ConvolutionFilter(_) => "convolution".to_owned(),
        Filter::ColorMatrixFilter(_) => "color-matrix".to_owned(),
    }
}

fn button_conditions(c: ButtonActionCondition) -> String {
    let mut names = Vec::new();
    for (name, flag) in c.iter_names() {
        if flag != ButtonActionCondition::KEY_PRESS {
            names.push(name.to_lowercase());
        }
    }
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join("|")
    }
}

fn clip_events(e: ClipEventFlag) -> String {
    let names: Vec<String> = e.iter_names().map(|(n, _)| n.to_lowercase()).collect();
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join("|")
    }
}

/// A matrix as `[a b c d tx ty]`, factors as decimals and the translation
/// in pixels.
pub fn matrix(m: &Matrix) -> String {
    format!(
        "[{} {} {} {} {} {}]",
        fx16(m.a),
        fx16(m.b),
        fx16(m.c),
        fx16(m.d),
        px(m.tx),
        px(m.ty)
    )
}

fn cxform(c: &ColorTransform) -> String {
    format!(
        "cx(*{} {} {} {} +{} {} {} {})",
        fx8(c.r_multiply),
        fx8(c.g_multiply),
        fx8(c.b_multiply),
        fx8(c.a_multiply),
        c.r_add,
        c.g_add,
        c.b_add,
        c.a_add
    )
}

fn rect(r: &Rectangle<Twips>) -> String {
    format!(
        "({},{})-({},{})",
        px(r.x_min),
        px(r.y_min),
        px(r.x_max),
        px(r.y_max)
    )
}

fn color(c: &Color) -> String {
    if c.a == 255 {
        format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
    } else {
        format!("#{:02X}{:02X}{:02X}/{}", c.r, c.g, c.b, c.a)
    }
}

fn text(s: &SwfStr) -> String {
    s.to_str_lossy(swf::UTF_8).into_owned()
}

/// Twips as pixels: exact with two decimals.
fn px(t: Twips) -> String {
    num(f64::from(t.get()) / 20.0)
}

fn fx16(v: Fixed16) -> String {
    num(v.to_f64())
}

fn fx8(v: Fixed8) -> String {
    num(v.to_f64())
}

/// A number with at most 5 decimals and no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.5}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_trimmed() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(0.75), "0.75");
        assert_eq!(num(-0.000001), "0");
        assert_eq!(px(Twips::new(9327)), "466.35");
    }

    #[test]
    fn rejects_garbage() {
        assert!(dump(b"not a swf", "x", SwfDumpOptions::default()).is_err());
        assert!(info(b"", "x").is_err());
    }
}
