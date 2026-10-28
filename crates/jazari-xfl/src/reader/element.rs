//! Elements of a keyframe or group: dispatch on the element name, groups,
//! symbol instances, bitmap instances and instance scripts.

use crate::WarningKind;
use crate::model::{
    BitmapInstance, BlendMode, ColorEffect, Element, GraphicPlayback, Group, LoopMode,
    SymbolInstance, SymbolKind,
};

use super::color::read_color_effect;
use super::ctx::Ctx;
use super::filter::read_filters;
use super::shape::{read_oval, read_rectangle, read_shape};
use super::text::{read_text, read_tlf};
use super::values::{excerpt, normalize_script};
use super::xml::Node;

/// Reads the children of an `<elements>` or `<members>` container. Each
/// element's index is added to the location's element path while it is read.
pub(crate) fn read_elements(ctx: &mut Ctx, container: Option<&Node>) -> Vec<Element> {
    let Some(container) = container else {
        return Vec::new();
    };
    container
        .children
        .iter()
        .enumerate()
        .map(|(i, node)| ctx.with_element(i, |ctx| read_element(ctx, node)))
        .collect()
}

/// Reads one element. Unknown elements become [`Element::Unsupported`].
fn read_element(ctx: &mut Ctx, node: &Node) -> Element {
    match node.name.as_str() {
        "DOMShape" => Element::Shape(read_shape(ctx, node)),
        "DOMRectangleObject" => Element::Shape(read_rectangle(ctx, node)),
        "DOMOvalObject" => Element::Shape(read_oval(ctx, node)),
        "DOMGroup" => Element::Group(read_group(ctx, node)),
        "DOMSymbolInstance" => Element::Instance(read_instance(ctx, node)),
        "DOMBitmapInstance" => Element::Bitmap(read_bitmap_instance(ctx, node)),
        "DOMStaticText" | "DOMDynamicText" | "DOMInputText" => Element::Text(read_text(ctx, node)),
        "DOMTLFText" => Element::Text(read_tlf(ctx, node)),
        "DOMVideoInstance" | "DOMComponentInstance" | "DOMCompiledClipInstance" => {
            ctx.warn(
                WarningKind::Unsupported,
                format!("<{}> is not supported and is left out", node.name),
            );
            Element::Unsupported {
                tag: node.name.clone(),
            }
        }
        other => {
            ctx.warn(
                WarningKind::UnknownElement,
                format!("unknown element <{other}> is left out"),
            );
            Element::Unsupported {
                tag: other.to_owned(),
            }
        }
    }
}

/// Reads a `DOMGroup`.
fn read_group(ctx: &mut Ctx, node: &Node) -> Group {
    ctx.check_children(node, &["matrix", "transformationPoint", "members"]);
    Group {
        matrix: ctx.matrix(node),
        members: read_elements(ctx, node.child("members")),
    }
}

/// Parses a `symbolType` value. `"ik container"` is read as a graphic.
pub(crate) fn symbol_kind(ctx: &mut Ctx, value: Option<&str>) -> SymbolKind {
    match value {
        None | Some("movie clip") => SymbolKind::MovieClip,
        Some("graphic") => SymbolKind::Graphic,
        Some("button") => SymbolKind::Button,
        Some("ik container") => {
            ctx.warn(
                WarningKind::AnimateFeature,
                "an IK container symbol is read as a graphic symbol",
            );
            SymbolKind::Graphic
        }
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "symbolType=\"{}\" is unknown; movie clip is used",
                    excerpt(other)
                ),
            );
            SymbolKind::MovieClip
        }
    }
}

/// Children a symbol instance may have.
const INSTANCE_CHILDREN: [&str; 8] = [
    "matrix",
    "transformationPoint",
    "Actionscript",
    "filters",
    "color",
    "MatteColor",
    "persistentData",
    "dataBindingXML",
];

/// Reads a `DOMSymbolInstance`.
fn read_instance(ctx: &mut Ctx, node: &Node) -> SymbolInstance {
    ctx.check_children(node, &INSTANCE_CHILDREN);
    let library_item = node.attr("libraryItemName").unwrap_or_default().to_owned();
    if library_item.is_empty() {
        ctx.warn(
            WarningKind::MissingItem,
            "a symbol instance names no library item",
        );
    }
    let kind = symbol_kind(ctx, node.attr("symbolType"));
    let color = node
        .child("color")
        .and_then(|c| c.child("Color"))
        .map(|c| read_color_effect(ctx, c))
        .unwrap_or(ColorEffect::IDENTITY);
    let filters = node
        .child("filters")
        .map(|f| read_filters(ctx, f))
        .unwrap_or_default();
    let graphic = (kind == SymbolKind::Graphic).then(|| graphic_playback(ctx, node));
    warn_animate_instance_attrs(ctx, node);
    SymbolInstance {
        library_item,
        kind,
        name: Ctx::text(node, "name"),
        matrix: ctx.matrix(node).unwrap_or_default(),
        transformation_point: ctx.transformation_point(node),
        color,
        filters,
        blend_mode: blend_mode(ctx, node),
        cache_as_bitmap: ctx.bool(node, "cacheAsBitmap", false),
        graphic,
        track_as_menu: ctx.bool(node, "trackAsMenu", false),
        script: read_script(ctx, node),
    }
}

/// `loop` and `firstFrame` of a graphic instance.
fn graphic_playback(ctx: &mut Ctx, node: &Node) -> GraphicPlayback {
    let mode = match node.attr("loop") {
        None | Some("loop") => LoopMode::Loop,
        Some("play once") => LoopMode::PlayOnce,
        Some("single frame") => LoopMode::SingleFrame,
        Some("loop reverse") => {
            ctx.warn(
                WarningKind::AnimateFeature,
                "loop reverse is played as loop",
            );
            LoopMode::Loop
        }
        Some("play once reverse") => {
            ctx.warn(
                WarningKind::AnimateFeature,
                "play once reverse is played as play once",
            );
            LoopMode::PlayOnce
        }
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!("loop=\"{}\" is unknown; loop is used", excerpt(other)),
            );
            LoopMode::Loop
        }
    };
    if node.has_attr("lastFrame") {
        ctx.warn(
            WarningKind::AnimateFeature,
            "the last frame of a graphic loop (Animate) is ignored",
        );
    }
    GraphicPlayback {
        mode,
        first_frame: ctx.u32(node, "firstFrame", 0),
    }
}

/// `blendMode` (SWF 8 blend modes).
fn blend_mode(ctx: &mut Ctx, node: &Node) -> BlendMode {
    match node.attr("blendMode") {
        None | Some("normal") => BlendMode::Normal,
        Some("layer") => BlendMode::Layer,
        Some("multiply") => BlendMode::Multiply,
        Some("screen") => BlendMode::Screen,
        Some("lighten") => BlendMode::Lighten,
        Some("darken") => BlendMode::Darken,
        Some("difference") => BlendMode::Difference,
        Some("add") => BlendMode::Add,
        Some("subtract") => BlendMode::Subtract,
        Some("invert") => BlendMode::Invert,
        Some("alpha") => BlendMode::Alpha,
        Some("erase") => BlendMode::Erase,
        Some("overlay") => BlendMode::Overlay,
        Some("hardlight") => BlendMode::HardLight,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "blendMode=\"{}\" is unknown; normal is used",
                    excerpt(other)
                ),
            );
            BlendMode::Normal
        }
    }
}

/// Warns about instance attributes that SWF 8 cannot express.
fn warn_animate_instance_attrs(ctx: &mut Ctx, node: &Node) {
    if !ctx.bool(node, "isVisible", true) {
        ctx.warn(
            WarningKind::Unsupported,
            "an invisible instance (isVisible=false) is published visible",
        );
    }
    let three_d = [
        "rotationX",
        "rotationY",
        "rotationZ",
        "centerPoint3DX",
        "centerPoint3DY",
        "centerPoint3DZ",
    ];
    let has_3d = three_d.iter().any(|a| {
        node.attr(a)
            .is_some_and(|v| v.trim().parse::<f64>().is_ok_and(|x| x != 0.0))
    }) || node.has_attr("matrix3D");
    if has_3d {
        ctx.warn(
            WarningKind::AnimateFeature,
            "3D transforms are ignored; only the 2D matrix is used",
        );
    }
}

/// The `on()` / `onClipEvent()` or frame source in `<Actionscript><script>`,
/// with CR and CRLF normalised to LF. `None` when there is no code.
pub(crate) fn read_script(ctx: &mut Ctx, node: &Node) -> Option<String> {
    let mut source = String::new();
    for block in node.children_named("Actionscript") {
        if let Some(href) = block.attr("href").filter(|h| !h.is_empty()) {
            ctx.warn(
                WarningKind::Unsupported,
                format!("the external script file \"{}\" is not read", excerpt(href)),
            );
        }
        for script in block.children_named("script") {
            source.push_str(&script.text);
        }
    }
    let source = normalize_script(&source);
    (!source.trim().is_empty()).then_some(source)
}

/// Reads a `DOMBitmapInstance`.
fn read_bitmap_instance(ctx: &mut Ctx, node: &Node) -> BitmapInstance {
    let library_item = node.attr("libraryItemName").unwrap_or_default().to_owned();
    if library_item.is_empty() {
        ctx.warn(
            WarningKind::MissingItem,
            "a bitmap instance names no library item",
        );
    }
    BitmapInstance {
        library_item,
        matrix: ctx.matrix(node).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    fn elements(xml: &str) -> (Vec<Element>, crate::ImportReport) {
        let mut ctx = Ctx::default();
        let n = parse_str(xml).unwrap();
        let e = read_elements(&mut ctx, Some(&n));
        (e, ctx.report)
    }

    #[test]
    fn instances_buttons_and_graphics() {
        let (e, report) = elements(
            r##"<elements>
              <DOMSymbolInstance libraryItemName="Btn" name="my_btn" symbolType="button" blendMode="screen" cacheAsBitmap="true" trackAsMenu="true">
                <matrix><Matrix tx="110" ty="220"/></matrix>
                <transformationPoint><Point x="40" y="15"/></transformationPoint>
                <Actionscript><script><![CDATA[on (release) {]]>&#13;&#10;<![CDATA[	trace("x");]]>&#13;}</script></Actionscript>
                <color><Color alphaMultiplier="0.5"/></color>
              </DOMSymbolInstance>
              <DOMSymbolInstance libraryItemName="Counter" symbolType="graphic" loop="play once" firstFrame="3" name=""/>
              <DOMSymbolInstance libraryItemName="Clip"/>
            </elements>"##,
        );
        assert!(report.is_empty(), "{:?}", report);
        let Element::Instance(b) = &e[0] else {
            panic!()
        };
        assert_eq!(b.kind, SymbolKind::Button);
        assert_eq!(b.name.as_deref(), Some("my_btn"));
        assert_eq!(b.blend_mode, BlendMode::Screen);
        assert!(b.cache_as_bitmap && b.track_as_menu);
        assert_eq!((b.transformation_point.x, b.matrix.ty), (40.0, 220.0));
        assert_eq!(
            b.script.as_deref(),
            Some("on (release) {\n\ttrace(\"x\");\n}")
        );
        assert_eq!(b.color.mult[3], 0.5);
        assert_eq!(b.graphic, None);
        let Element::Instance(g) = &e[1] else {
            panic!()
        };
        assert_eq!(
            g.graphic,
            Some(GraphicPlayback {
                mode: LoopMode::PlayOnce,
                first_frame: 3
            })
        );
        assert_eq!(g.name, None);
        let Element::Instance(m) = &e[2] else {
            panic!()
        };
        assert_eq!(m.kind, SymbolKind::MovieClip, "absent symbolType");
        assert!(m.color.is_identity());
    }

    #[test]
    fn groups_nest_and_keep_optional_matrices() {
        let (e, report) = elements(
            r#"<elements><DOMGroup><members>
                <DOMShape><edges/></DOMShape>
                <DOMGroup><matrix><Matrix tx="5"/></matrix><members><DOMBitmapInstance libraryItemName="a.png"/></members></DOMGroup>
            </members></DOMGroup></elements>"#,
        );
        assert!(report.is_empty());
        let Element::Group(g) = &e[0] else { panic!() };
        assert_eq!(g.matrix, None);
        let Element::Group(inner) = &g.members[1] else {
            panic!()
        };
        assert_eq!(inner.matrix.map(|m| m.tx), Some(5.0));
        assert!(matches!(&inner.members[0], Element::Bitmap(b) if b.library_item == "a.png"));
    }

    #[test]
    fn unknown_elements_are_kept_with_a_located_warning() {
        let mut ctx = Ctx::default();
        ctx.loc.frame = Some(4);
        let n = parse_str(
            r#"<elements><DOMShape><edges/></DOMShape><DOMGroup><members><DOMHologram/></members></DOMGroup><DOMVideoInstance/></elements>"#,
        )
        .unwrap();
        let e = read_elements(&mut ctx, Some(&n));
        let Element::Group(g) = &e[1] else { panic!() };
        assert_eq!(
            g.members[0],
            Element::Unsupported {
                tag: "DOMHologram".to_owned()
            }
        );
        assert_eq!(
            e[2],
            Element::Unsupported {
                tag: "DOMVideoInstance".to_owned()
            }
        );
        let w = &ctx.report.warnings;
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].kind, WarningKind::UnknownElement);
        assert_eq!(w[0].location.element, vec![1, 0]);
        assert_eq!(w[0].location.frame, Some(4));
        assert_eq!(w[1].kind, WarningKind::Unsupported);
        assert_eq!(w[1].location.element, vec![2]);
        assert!(ctx.loc.element.is_empty(), "the path is restored");
    }

    #[test]
    fn animate_only_instance_attributes_warn() {
        let (_, report) = elements(
            r#"<elements><DOMSymbolInstance libraryItemName="A" isVisible="false" rotationY="30" symbolType="graphic" loop="loop reverse" lastFrame="4"/></elements>"#,
        );
        let kinds: Vec<WarningKind> = report.iter().map(|w| w.kind).collect();
        assert_eq!(
            kinds,
            [
                WarningKind::AnimateFeature,
                WarningKind::AnimateFeature,
                WarningKind::Unsupported,
                WarningKind::AnimateFeature
            ]
        );
    }

    #[test]
    fn empty_scripts_are_none() {
        let mut ctx = Ctx::default();
        let n =
            parse_str("<DOMFrame><Actionscript><script>  \n </script></Actionscript></DOMFrame>")
                .unwrap();
        assert_eq!(read_script(&mut ctx, &n), None);
        let n = parse_str(r#"<DOMFrame><Actionscript href="a.as"><script>x();</script></Actionscript></DOMFrame>"#).unwrap();
        assert_eq!(read_script(&mut ctx, &n).as_deref(), Some("x();"));
        assert_eq!(ctx.report.len(), 1);
    }
}
