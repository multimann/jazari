//! Classic text (`DOMStaticText`, `DOMDynamicText`, `DOMInputText`) and the
//! TLF fallback (`DOMTLFText` read as static text).

use crate::WarningKind;
use crate::model::{Align, LineType, Rgba, TextAttrs, TextElement, TextKind, TextRun};

use super::ctx::Ctx;
use super::values::{excerpt, normalize_characters, parse_color, parse_f64};
use super::xml::Node;

/// Children a classic text element may have.
const TEXT_CHILDREN: [&str; 5] = [
    "matrix",
    "transformationPoint",
    "textRuns",
    "filters",
    "persistentData",
];

/// Reads a classic text element.
pub(crate) fn read_text(ctx: &mut Ctx, node: &Node) -> TextElement {
    ctx.check_children(node, &TEXT_CHILDREN);
    let kind = match node.name.as_str() {
        "DOMDynamicText" => TextKind::Dynamic,
        "DOMInputText" => TextKind::Input,
        _ => TextKind::Static,
    };
    let line_type = match node.attr("lineType") {
        None | Some("single line") => LineType::SingleLine,
        Some("multiline") => LineType::Multiline,
        Some("multiline no wrap") => LineType::MultilineNoWrap,
        Some("password") => LineType::Password,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!(
                    "lineType=\"{}\" is unknown; single line is used",
                    excerpt(other)
                ),
            );
            LineType::SingleLine
        }
    };
    let orientation = node.attr("orientation").unwrap_or("horizontal").to_owned();
    if orientation != "horizontal" {
        ctx.warn(
            WarningKind::Unsupported,
            format!("{orientation} text is laid out horizontally"),
        );
    }
    if node
        .child("filters")
        .is_some_and(|f| !f.children.is_empty())
    {
        ctx.warn(
            WarningKind::Unsupported,
            "filters on text fields are not published",
        );
    }

    let mut runs = Vec::new();
    if let Some(container) = node.child("textRuns") {
        for run in &container.children {
            if run.name != "DOMTextRun" {
                ctx.warn(
                    WarningKind::UnknownElement,
                    format!("unknown <{}> inside <textRuns> was skipped", run.name),
                );
                continue;
            }
            let text = run
                .child("characters")
                .map(|c| normalize_characters(&c.text))
                .unwrap_or_default();
            let attrs = match run.child("textAttrs").and_then(|t| t.child("DOMTextAttrs")) {
                Some(a) => read_attrs(ctx, a),
                None => TextAttrs::default(),
            };
            runs.push(TextRun { text, attrs });
        }
    }

    TextElement {
        kind,
        matrix: ctx.matrix(node).unwrap_or_default(),
        left: ctx.f64(node, "left", 0.0),
        top: ctx.f64(node, "top", 0.0),
        width: ctx.f64(node, "width", 0.0),
        height: ctx.f64(node, "height", 0.0),
        runs,
        auto_expand: ctx.bool(node, "autoExpand", false),
        line_type,
        selectable: ctx.bool(node, "isSelectable", true),
        name: Ctx::text(node, "name"),
        variable: Ctx::text(node, "variableName"),
        max_chars: ctx.opt_u32(node, "maxCharacters").filter(|&n| n > 0),
        border: ctx.bool(node, "border", false),
        render_as_html: ctx.bool(node, "renderAsHTML", false),
        orientation,
    }
}

/// Reads `DOMTextAttrs`.
fn read_attrs(ctx: &mut Ctx, node: &Node) -> TextAttrs {
    let align = match node.attr("alignment") {
        None | Some("left") => Align::Left,
        Some("right") => Align::Right,
        Some("center") => Align::Center,
        Some("justify") => Align::Justify,
        Some(other) => {
            ctx.warn(
                WarningKind::BadValue,
                format!("alignment=\"{}\" is unknown; left is used", excerpt(other)),
            );
            Align::Left
        }
    };
    match node.attr("characterPosition") {
        None | Some("normal") => {}
        Some(other) => ctx.warn(
            WarningKind::Unsupported,
            format!("{} text is laid out as normal text", excerpt(other)),
        ),
    }
    TextAttrs {
        face: node.attr("face").unwrap_or_default().to_owned(),
        size: ctx.f64(node, "size", 12.0),
        fill: ctx.color_alpha(node, "fillColor", "alpha", Rgba::BLACK),
        align,
        indent: ctx.f64(node, "indent", 0.0),
        left_margin: ctx.f64(node, "leftMargin", 0.0),
        right_margin: ctx.f64(node, "rightMargin", 0.0),
        line_spacing: ctx.f64(node, "lineSpacing", 2.0),
        letter_spacing: ctx.f64(node, "letterSpacing", 0.0),
        auto_kern: ctx.bool(node, "autoKern", true),
        url: Ctx::text(node, "url"),
        target: Ctx::text(node, "target"),
    }
}

/// Reads a `DOMTLFText` as static text from the spans of its TextFlow.
///
/// TLF needs AS3 and Flash Player 10, so it cannot be published to SWF 8;
/// paragraphs become CR-separated runs and span fonts, sizes and colours are
/// kept.
pub(crate) fn read_tlf(ctx: &mut Ctx, node: &Node) -> TextElement {
    ctx.warn(
        WarningKind::Unsupported,
        "TLF text is imported as static text (TLF needs AS3 and Flash Player 10)",
    );
    let mut runs: Vec<TextRun> = Vec::new();
    if let Some(flow) = node.descendant("TextFlow") {
        let base = span_attrs(flow, &TextAttrs::default());
        let paragraphs = collect(flow, "p");
        let count = paragraphs.len();
        for (i, p) in paragraphs.into_iter().enumerate() {
            let p_attrs = span_attrs(p, &base);
            let before = runs.len();
            for span in collect(p, "span") {
                runs.push(TextRun {
                    text: normalize_characters(&span.text),
                    attrs: span_attrs(span, &p_attrs),
                });
            }
            if runs.len() == before && !p.text.is_empty() {
                runs.push(TextRun {
                    text: normalize_characters(&p.text),
                    attrs: p_attrs.clone(),
                });
            }
            if i + 1 < count {
                if runs.len() > before {
                    if let Some(last) = runs.last_mut() {
                        last.text.push('\r');
                    }
                } else {
                    runs.push(TextRun {
                        text: "\r".to_owned(),
                        attrs: p_attrs,
                    });
                }
            }
        }
    }
    TextElement {
        kind: TextKind::Static,
        matrix: ctx.matrix(node).unwrap_or_default(),
        runs,
        selectable: false,
        ..TextElement::default()
    }
}

/// All descendants named `name`, not descending into matches.
fn collect<'a>(node: &'a Node, name: &str) -> Vec<&'a Node> {
    let mut out = Vec::new();
    for c in &node.children {
        if c.name == name {
            out.push(c);
        } else {
            out.extend(collect(c, name));
        }
    }
    out
}

/// TLF character and paragraph attributes on top of `base`.
fn span_attrs(node: &Node, base: &TextAttrs) -> TextAttrs {
    let mut attrs = base.clone();
    if let Some(face) = node.attr("fontFamily") {
        attrs.face = face.to_owned();
    }
    if let Some(size) = node.attr("fontSize").and_then(parse_f64) {
        attrs.size = size;
    }
    if let Some(color) = node.attr("color").and_then(parse_color) {
        attrs.fill = color.with_alpha_ratio(attrs.fill.alpha_ratio());
    }
    if let Some(alpha) = node.attr("textAlpha").and_then(parse_f64) {
        attrs.fill = attrs.fill.with_alpha_ratio(alpha);
    }
    if let Some(spacing) = node.attr("trackingRight").and_then(parse_f64) {
        attrs.letter_spacing = spacing;
    }
    match node.attr("textAlign") {
        Some("right" | "end") => attrs.align = Align::Right,
        Some("center") => attrs.align = Align::Center,
        Some("justify") => attrs.align = Align::Justify,
        Some("left" | "start") => attrs.align = Align::Left,
        _ => {}
    }
    attrs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    #[test]
    fn static_arabic_text() {
        let mut ctx = Ctx::default();
        let n = parse_str(
            r##"<DOMStaticText autoExpand="true" height="33.8" isSelectable="false" left="0" orientation="horizontal" top="0" width="187.85">
              <matrix><Matrix a="1" b="0" c="0" d="1" tx="22" ty="202"/></matrix>
              <textRuns><DOMTextRun><characters>مرحبا بالعالم</characters>
                <textAttrs><DOMTextAttrs aliasText="false" alignment="left" alpha="1" autoKern="true" bitmapSize="560" face="Tahoma" fillColor="#000000" lineSpacing="2" size="28"/></textAttrs>
              </DOMTextRun></textRuns>
            </DOMStaticText>"##,
        )
        .unwrap();
        let t = read_text(&mut ctx, &n);
        assert!(ctx.report.is_empty());
        assert_eq!(t.kind, TextKind::Static);
        assert_eq!(t.runs[0].text, "مرحبا بالعالم");
        assert_eq!(t.runs[0].attrs.face, "Tahoma");
        assert_eq!(t.runs[0].attrs.size, 28.0);
        assert_eq!((t.matrix.tx, t.width), (22.0, 187.85));
        assert!(t.auto_expand && !t.selectable);
    }

    #[test]
    fn dynamic_and_input_fields() {
        let mut ctx = Ctx::default();
        let n = parse_str(
            r##"<DOMInputText name="name_txt" variableName="_root.who" lineType="password" maxCharacters="20" border="true">
              <textRuns><DOMTextRun><characters>a&#13;b&#xA;c</characters>
                <textAttrs><DOMTextAttrs face="_sans" size="12.5" fillColor="#333333" alpha="0.5" alignment="center" url="http://x" target=""/></textAttrs>
              </DOMTextRun></textRuns>
            </DOMInputText>"##,
        )
        .unwrap();
        let t = read_text(&mut ctx, &n);
        assert!(ctx.report.is_empty());
        assert_eq!(t.kind, TextKind::Input);
        assert_eq!(t.line_type, LineType::Password);
        assert_eq!(t.name.as_deref(), Some("name_txt"));
        assert_eq!(t.variable.as_deref(), Some("_root.who"));
        assert_eq!(t.max_chars, Some(20));
        assert!(t.border);
        let r = &t.runs[0];
        assert_eq!(r.text, "a\rb\rc");
        assert_eq!(r.attrs.size, 12.5);
        assert_eq!(r.attrs.fill, Rgba::new(0x33, 0x33, 0x33, 128));
        assert_eq!(r.attrs.align, Align::Center);
        assert_eq!(r.attrs.url.as_deref(), Some("http://x"));
        assert_eq!(r.attrs.target, None);
    }

    #[test]
    fn empty_names_are_none() {
        let mut ctx = Ctx::default();
        let n =
            parse_str(r#"<DOMDynamicText name="" variableName="" maxCharacters="0"/>"#).unwrap();
        let t = read_text(&mut ctx, &n);
        assert_eq!((t.name, t.variable, t.max_chars), (None, None, None));
    }

    #[test]
    fn tlf_text_becomes_static_runs() {
        let mut ctx = Ctx::default();
        let n = parse_str(
            r##"<DOMTLFText><matrix><Matrix tx="5"/></matrix><markup><tlfTextObject>
              <TextFlow xmlns="http://ns.adobe.com/textLayout/2008" fontFamily="Arial" fontSize="20">
                <p textAlign="right"><span color="#FF0000">مرحبا</span><span> world</span></p>
                <p><span fontSize="10">second</span></p>
              </TextFlow></tlfTextObject></markup></DOMTLFText>"##,
        )
        .unwrap();
        let t = read_tlf(&mut ctx, &n);
        assert_eq!(ctx.report.len(), 1);
        assert_eq!(t.matrix.tx, 5.0);
        let texts: Vec<&str> = t.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["مرحبا", " world\r", "second"]);
        assert_eq!(t.runs[0].attrs.fill, Rgba::opaque(255, 0, 0));
        assert_eq!(t.runs[0].attrs.align, Align::Right);
        assert_eq!(t.runs[1].attrs.size, 20.0);
        assert_eq!(t.runs[2].attrs.size, 10.0);
        assert_eq!(t.runs[2].attrs.face, "Arial");
    }
}
