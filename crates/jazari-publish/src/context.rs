//! The publishing context shared by every module: the document, the build
//! mode, the diagnostics, and the character table with its lazy emission.
//!
//! # Characters
//!
//! A character is defined once ([`Ctx::define`]) and gets an id at once, but
//! its tags are written into the root tag stream only the first time it is
//! placed ([`Ctx::emit_definition`]), after the characters it depends on.
//! Definitions therefore always sit at root level, just before first use, as
//! Flash writes them. Characters that are never placed are never written.
//!
//! # Diagnostics
//!
//! Every diagnostic carries an [`Origin`] built from a [`Scope`] (scene or
//! symbol), a layer, a 0-based frame and, for instances or elements, a
//! description. Identical diagnostics are reported once.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use jazari_as2::{Compiler, Diagnostic, Origin, Severity};
use jazari_text::FontLibrary;
use jazari_xfl::Document;
use swf::CharacterId;

use crate::BuildMode;
use crate::error::PublishError;
use crate::tags::RawTag;
use crate::timeline::{Expanded, TimelinePlan};
use crate::{bitmap, mask, text};

/// Highest character id we hand out. Flash uses 65535 as a placeholder
/// bitmap id, so it is kept free.
pub const MAX_CHARACTER_ID: u16 = 65534;

/// Deepest symbol nesting we follow (movie clips, graphics, buttons and
/// groups each count). Deeper content is reported and left out, which also
/// bounds recursion.
pub const MAX_NESTING: usize = 64;

/// What kind of character an id names. Only [`CharKind::Shape`] characters
/// may be swapped in place with a PlaceObject2 *Replace*.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)] // some kinds are defined only by the text and bitmap modules
pub enum CharKind {
    /// DefineShape3/4 (including bitmap instances drawn as shapes).
    Shape,
    /// DefineSprite.
    Sprite,
    /// DefineButton2.
    Button,
    /// DefineBits*.
    Bitmap,
    /// DefineFont2/3.
    Font,
    /// DefineText/DefineText2.
    Text,
    /// DefineEditText.
    EditText,
    /// Anything else.
    Other,
}

/// A defined character.
#[derive(Debug)]
struct CharDef {
    kind: CharKind,
    /// The definition tags, in order (the definition itself first, then for
    /// example ExportAssets or CSMTextSettings).
    tags: Vec<RawTag>,
    /// Characters that must be written before this one.
    deps: Vec<CharacterId>,
    emitted: bool,
}

/// The key of a DefineButton2: symbol, track-as-menu flag, and the
/// instance script with the scene it was compiled for.
pub(crate) type ButtonKey = (String, bool, Option<(String, Option<usize>)>);

/// Names the timeline a diagnostic or a script belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    /// The display name: the scene name (`Scene 1`) or the symbol's library
    /// path.
    pub timeline: String,
    /// Whether this is a graphic symbol flattened into a parent (its scripts
    /// and labels are dropped, as Flash does).
    pub flattened: bool,
    /// The scene's index when this is a scene (root) timeline: its frame
    /// scripts and button handlers resolve scene gotos at compile time.
    pub scene: Option<usize>,
}

impl Scope {
    /// A scope for a scene or a symbol.
    pub fn new(timeline: impl Into<String>) -> Self {
        Self {
            timeline: timeline.into(),
            flattened: false,
            scene: None,
        }
    }

    /// A scope for scene `index` of the document.
    pub fn scene(timeline: impl Into<String>, index: usize) -> Self {
        Self {
            scene: Some(index),
            ..Self::new(timeline)
        }
    }

    /// An origin in this scope.
    pub fn origin(&self, layer: Option<&str>, frame: Option<u32>) -> Origin {
        Origin {
            timeline: Some(self.timeline.clone()),
            layer: layer.map(str::to_owned),
            frame,
            instance: None,
            file: None,
        }
    }

    /// An origin for an element or instance, described by `what` (the
    /// instance name, or for example `text element 3`).
    pub fn element_origin(&self, layer: &str, frame: u32, what: impl Into<String>) -> Origin {
        Origin {
            instance: Some(what.into()),
            ..self.origin(Some(layer), Some(frame))
        }
    }
}

/// The state of one publish.
pub struct Ctx<'a> {
    /// The document being published.
    pub doc: &'a Document,
    /// Release or stage build.
    pub mode: BuildMode,
    /// Fonts for static text.
    #[allow(dead_code)] // used by the text module
    pub fonts: &'a FontLibrary,
    /// The AS2 compiler.
    pub as2: &'a Compiler,
    diagnostics: Vec<Diagnostic>,
    seen: HashSet<String>,
    chars: Vec<CharDef>,
    out_of_ids_reported: bool,
    /// Shape bodies (with id 0) to their character, for deduplication.
    pub(crate) shape_dedup: HashMap<Vec<u8>, CharacterId>,
    /// Movie clip symbols already built, by library name (`None` = failed).
    pub(crate) sprites: HashMap<String, Option<CharacterId>>,
    /// Button definitions, by symbol, track-as-menu flag and script (with
    /// the scene its gotos were compiled for).
    pub(crate) buttons: HashMap<ButtonKey, Option<CharacterId>>,
    /// Symbols being built right now (recursion guard).
    pub(crate) building: Vec<String>,
    /// Expanded keyframes, by the address of their `Frame`.
    pub(crate) expanded: HashMap<usize, Rc<Expanded>>,
    /// Layer plans and depth bands, by the address of their `Timeline`.
    pub(crate) plans: HashMap<usize, Rc<TimelinePlan>>,
    /// Depth needs of graphic symbols, by library name.
    pub(crate) needs: HashMap<String, u32>,
    /// Graphic symbols whose depth need is being computed (recursion guard).
    pub(crate) need_stack: Vec<String>,
    /// State owned by the text module.
    #[allow(dead_code)]
    pub text: text::TextState,
    /// State owned by the bitmap module.
    #[allow(dead_code)]
    pub bitmap: bitmap::BitmapState,
    /// State owned by the mask module.
    #[allow(dead_code)]
    pub mask: mask::MaskState,
}

impl<'a> Ctx<'a> {
    /// A fresh context.
    pub fn new(
        doc: &'a Document,
        mode: BuildMode,
        fonts: &'a FontLibrary,
        as2: &'a Compiler,
    ) -> Self {
        Self {
            doc,
            mode,
            fonts,
            as2,
            diagnostics: Vec::new(),
            seen: HashSet::new(),
            chars: Vec::new(),
            out_of_ids_reported: false,
            shape_dedup: HashMap::new(),
            sprites: HashMap::new(),
            buttons: HashMap::new(),
            building: Vec::new(),
            expanded: HashMap::new(),
            plans: HashMap::new(),
            needs: HashMap::new(),
            need_stack: Vec::new(),
            text: text::TextState::default(),
            bitmap: bitmap::BitmapState::default(),
            mask: mask::MaskState::default(),
        }
    }

    /// Whether user scripts are compiled (release build).
    pub fn scripts_enabled(&self) -> bool {
        self.mode == BuildMode::Release
    }

    /// Adds a diagnostic (identical ones are kept once).
    pub fn report(&mut self, severity: Severity, origin: Origin, message: impl Into<String>) {
        self.push(Diagnostic {
            severity,
            message: message.into(),
            origin,
            line: 0,
            column: 0,
        });
    }

    /// Adds a warning: something was approximated or left out.
    pub fn warn(&mut self, origin: Origin, message: impl Into<String>) {
        self.report(Severity::Warning, origin, message);
    }

    /// Adds an error: something was skipped.
    pub fn error(&mut self, origin: Origin, message: impl Into<String>) {
        self.report(Severity::Error, origin, message);
    }

    /// Adds an informational note.
    pub fn info(&mut self, origin: Origin, message: impl Into<String>) {
        self.report(Severity::Info, origin, message);
    }

    /// Adds diagnostics produced elsewhere (the AS2 compiler).
    pub fn extend(&mut self, diags: impl IntoIterator<Item = Diagnostic>) {
        for d in diags {
            self.push(d);
        }
    }

    fn push(&mut self, d: Diagnostic) {
        let key = format!("{d}");
        if self.seen.insert(key) {
            self.diagnostics.push(d);
        }
    }

    /// Takes the diagnostics collected so far.
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Defines a character. `build` receives the new id and returns the
    /// encoded definition tags; `deps` are characters that must be written
    /// first. On failure the id is still consumed (ids stay deterministic),
    /// an error is reported at `origin`, and `None` is returned.
    pub fn define(
        &mut self,
        kind: CharKind,
        deps: Vec<CharacterId>,
        origin: &Origin,
        build: impl FnOnce(CharacterId) -> Result<Vec<RawTag>, PublishError>,
    ) -> Option<CharacterId> {
        let id = self.reserve_id(origin)?;
        match build(id) {
            Ok(tags) => {
                self.set_definition(id, kind, tags, deps);
                Some(id)
            }
            Err(e) => {
                self.error(origin.clone(), format!("left out: {e}"));
                None
            }
        }
    }

    /// Reserves the next character id, for definitions that need their id
    /// before their tags exist (sprites). Fill it with
    /// [`set_definition`](Self::set_definition). Reports and returns `None`
    /// when ids run out.
    pub fn reserve_id(&mut self, origin: &Origin) -> Option<CharacterId> {
        let next = self.chars.len() + 1;
        match u16::try_from(next) {
            Ok(id) if id <= MAX_CHARACTER_ID => {
                self.chars.push(CharDef {
                    kind: CharKind::Other,
                    tags: Vec::new(),
                    deps: Vec::new(),
                    emitted: false,
                });
                Some(id)
            }
            _ => {
                if !self.out_of_ids_reported {
                    self.out_of_ids_reported = true;
                    self.error(origin.clone(), PublishError::OutOfIds.to_string());
                }
                None
            }
        }
    }

    /// Sets the tags of a reserved id. An unknown id is ignored.
    pub fn set_definition(
        &mut self,
        id: CharacterId,
        kind: CharKind,
        tags: Vec<RawTag>,
        deps: Vec<CharacterId>,
    ) {
        if let Some(def) = self.def_mut(id) {
            def.kind = kind;
            def.tags = tags;
            def.deps = deps;
        }
    }

    /// Appends tags to a character's definition (for example ExportAssets
    /// after a sprite).
    pub fn append_definition_tags(&mut self, id: CharacterId, tags: Vec<RawTag>) {
        if let Some(def) = self.def_mut(id) {
            def.tags.extend(tags);
        }
    }

    /// The kind of a character, if it exists.
    pub fn kind_of(&self, id: CharacterId) -> Option<CharKind> {
        self.def(id).map(|d| d.kind)
    }

    /// Writes the definition of `id` (and, first, of everything it depends
    /// on) into `out`, unless it was written already.
    pub fn emit_definition(&mut self, id: CharacterId, out: &mut Vec<RawTag>) {
        // Iterative depth-first walk, so deep dependency chains cannot
        // overflow the stack. A character is marked as written before its
        // dependencies are visited, which also stops cycles.
        let mut stack: Vec<(CharacterId, bool)> = vec![(id, false)];
        while let Some((cur, deps_done)) = stack.pop() {
            if deps_done {
                if let Some(def) = self.def_mut(cur) {
                    out.extend(def.tags.iter().cloned());
                }
                continue;
            }
            let Some(def) = self.def_mut(cur) else {
                continue;
            };
            if def.emitted {
                continue;
            }
            def.emitted = true;
            let deps = def.deps.clone();
            stack.push((cur, true));
            for d in deps.into_iter().rev() {
                stack.push((d, false));
            }
        }
    }

    fn def(&self, id: CharacterId) -> Option<&CharDef> {
        self.chars.get(usize::from(id).checked_sub(1)?)
    }

    fn def_mut(&mut self, id: CharacterId) -> Option<&mut CharDef> {
        self.chars.get_mut(usize::from(id).checked_sub(1)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::RawTag;

    fn tag(n: u8) -> RawTag {
        RawTag {
            code: 2,
            body: vec![n],
        }
    }

    #[test]
    fn definitions_are_emitted_once_after_their_dependencies() {
        let doc = Document::default();
        let fonts = FontLibrary::embedded_only();
        let as2 = Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let o = Origin::default();
        let a = ctx.define(CharKind::Shape, vec![], &o, |_| Ok(vec![tag(1)]));
        let b = ctx.define(CharKind::Shape, vec![], &o, |_| Ok(vec![tag(2)]));
        let (a, b) = (a.expect("a"), b.expect("b"));
        let c = ctx
            .define(CharKind::Sprite, vec![b, a], &o, |_| Ok(vec![tag(3)]))
            .expect("c");
        let mut out = Vec::new();
        ctx.emit_definition(c, &mut out);
        assert_eq!(out, vec![tag(2), tag(1), tag(3)]);
        ctx.emit_definition(a, &mut out);
        assert_eq!(out.len(), 3, "already written");
    }

    #[test]
    fn ids_run_out_with_one_error() {
        let doc = Document::default();
        let fonts = FontLibrary::embedded_only();
        let as2 = Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let o = Origin::default();
        for _ in 0..MAX_CHARACTER_ID {
            assert!(ctx.reserve_id(&o).is_some());
        }
        assert!(ctx.reserve_id(&o).is_none());
        assert!(ctx.reserve_id(&o).is_none());
        assert_eq!(ctx.take_diagnostics().len(), 1);
    }

    #[test]
    fn identical_diagnostics_are_kept_once() {
        let doc = Document::default();
        let fonts = FontLibrary::embedded_only();
        let as2 = Compiler::new(Default::default());
        let mut ctx = Ctx::new(&doc, BuildMode::Release, &fonts, &as2);
        let o = Scope::new("Scene 1").origin(Some("a"), Some(0));
        ctx.warn(o.clone(), "x");
        ctx.warn(o, "x");
        assert_eq!(ctx.take_diagnostics().len(), 1);
    }
}
