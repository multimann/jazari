//! The document, its library and its settings.

use std::path::PathBuf;

use indexmap::IndexMap;

use super::{Rect, Rgba, Timeline};

/// An XFL document in memory: the root of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Stage width in pixels (`DOMDocument@width`, default 550).
    pub width: f64,
    /// Stage height in pixels (`DOMDocument@height`, default 400).
    pub height: f64,
    /// Frames per second (`DOMDocument@frameRate`, default 24).
    pub frame_rate: f64,
    /// Stage colour (`DOMDocument@backgroundColor`, default white).
    pub background: Rgba,
    /// `DOMDocument@xflVersion` as written ("2.0" = CS5, "2.2" = CS6,
    /// "24.0" = Animate 2024). Compare it as a number, never as text.
    pub xfl_version: String,
    /// One timeline per `DOMTimeline`, in document order. Each is a scene.
    pub scenes: Vec<Timeline>,
    /// Library items.
    pub library: Library,
    /// Settings from `PublishSettings.xml`, with defaults when absent.
    pub publish: PublishSettings,
    /// Where the document was read from.
    pub source: SourceInfo,
}

impl Default for Document {
    /// A Flash-default stage (550 × 400 px, 24 fps, white) with no scenes.
    fn default() -> Self {
        Self {
            width: 550.0,
            height: 400.0,
            frame_rate: 24.0,
            background: Rgba::WHITE,
            xfl_version: String::new(),
            scenes: Vec::new(),
            library: Library::default(),
            publish: PublishSettings::default(),
            source: SourceInfo::default(),
        }
    }
}

/// The document library.
///
/// Keys are full library paths using `/` (`"folder/Name"`), taken from the
/// `name` attribute inside each item, never from file names.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Library {
    /// Symbols, keyed by `DOMSymbolItem@name`, in document order.
    pub symbols: IndexMap<String, Symbol>,
    /// Bitmaps, keyed by `DOMBitmapItem@name`.
    pub bitmaps: IndexMap<String, BitmapItem>,
    /// Font items, keyed by `DOMFontItem@name`.
    pub fonts: IndexMap<String, FontItem>,
    /// Library folders (`DOMFolderItem@name`), in document order.
    pub folders: Vec<String>,
}

/// A library symbol: a movie clip, graphic or button with its own timeline.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Symbol {
    /// Full library path (`DOMSymbolItem@name`).
    pub name: String,
    /// Symbol type.
    pub kind: SymbolKind,
    /// The symbol's timeline. For a button, frames 0..=3 are Up, Over, Down
    /// and Hit.
    pub timeline: Timeline,
    /// Linkage for ActionScript, if exported.
    pub linkage: Option<Linkage>,
    /// 9-slice scaling grid in symbol pixels, if set.
    pub scale9: Option<Rect>,
}

/// The type of a symbol or of a symbol instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SymbolKind {
    /// Movie clip (the XFL default when `symbolType` is absent).
    #[default]
    MovieClip,
    /// Graphic symbol: flattened into its parent timeline when published.
    Graphic,
    /// Button symbol.
    Button,
}

/// ActionScript linkage of a library item.
#[derive(Debug, Clone, PartialEq)]
pub struct Linkage {
    /// `linkageIdentifier`: the AS2 `attachMovie` id (SWF ExportAssets name).
    pub identifier: String,
    /// `linkageExportForAS`.
    pub export_for_as: bool,
    /// `linkageExportInFirstFrame` (default true).
    pub export_in_first_frame: bool,
    /// `linkageClassName`: an AS2 class registered with `Object.registerClass`.
    pub class_name: Option<String>,
}

impl Default for Linkage {
    fn default() -> Self {
        Self {
            identifier: String::new(),
            export_for_as: false,
            export_in_first_frame: true,
            class_name: None,
        }
    }
}

/// A bitmap library item (`DOMBitmapItem`) with its decoded data.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BitmapItem {
    /// Full library path.
    pub name: String,
    /// Width in pixels (`frameRight / 20`).
    pub width: u32,
    /// Height in pixels (`frameBottom / 20`).
    pub height: u32,
    /// The pixels, or the JPEG bytes to pass through to the SWF.
    pub data: BitmapData,
    /// `allowSmoothing`.
    pub smoothing: bool,
    /// Linkage for ActionScript, if exported.
    pub linkage: Option<Linkage>,
}

/// Bitmap contents.
#[derive(Debug, Clone, PartialEq)]
pub enum BitmapData {
    /// Decoded pixels: `width × height × 4` bytes, row-major, R, G, B, A order.
    ///
    /// Lossless `bin/*.dat` bitmaps are kept premultiplied exactly as Flash
    /// stores them: reordered to A, R, G, B, they are the plane Flash 8 writes
    /// into DefineBitsLossless2 (byte-identical in the `bitmaps` fixture).
    /// Relative to the imported image, Flash's plane holds alpha `a + 1` and
    /// colours `floor(c × a / 256)` for semi-transparent pixels, so do not
    /// "fix" it before publishing. PNG files and 8-bit palette bitmaps
    /// are straight.
    Rgba {
        /// Whether the colour channels are premultiplied by alpha (no channel
        /// exceeds alpha).
        premultiplied: bool,
        /// The pixel bytes.
        pixels: Vec<u8>,
    },
    /// JPEG data, published as DefineBitsJPEG2 (or DefineBitsJPEG3 with alpha).
    Jpeg {
        /// The JPEG stream from SOI to EOI, without the XFL bounds trailer.
        bytes: Vec<u8>,
        /// Optional alpha plane, one byte per pixel, uncompressed.
        alpha: Option<Vec<u8>>,
    },
}

impl Default for BitmapData {
    /// An empty, unpremultiplied pixel buffer.
    fn default() -> Self {
        BitmapData::Rgba {
            premultiplied: false,
            pixels: Vec::new(),
        }
    }
}

/// A font library item (`DOMFontItem`): a request to embed glyphs of a face.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FontItem {
    /// Full library path (`DOMFontItem@name`).
    pub name: String,
    /// The font face (`DOMFontItem@font`).
    pub face: String,
    /// `DOMFontItem@size` in points.
    pub size: f64,
    /// Embedded character range ids (`embedRanges="1|2|3"`).
    pub embed_ranges: Vec<u32>,
    /// Extra characters to embed (`embeddedCharacters`).
    pub embedded_characters: String,
    /// Linkage for ActionScript, if exported.
    pub linkage: Option<Linkage>,
}

/// Publish settings from `PublishSettings.xml` (the current Flash profile).
///
/// Missing files and empty values fall back to the [`Default`] values, which
/// are those of a Flash 8 / AS2 document. The frame rate is not here: it is
/// [`Document::frame_rate`].
#[derive(Debug, Clone, PartialEq)]
pub struct PublishSettings {
    /// Target player version (`Version`, default 8). Al-Jazari always
    /// publishes SWF 8 and reports other values.
    pub flash_version: u8,
    /// `ActionScriptVersion` (default 2).
    pub actionscript_version: u8,
    /// `CompressMovie`: zlib-compressed (`CWS`) output (default true).
    pub compress: bool,
    /// `Quality`: JPEG quality 0..=100 (default 80).
    pub jpeg_quality: u8,
    /// `InvisibleLayer`: publish hidden layers (default true).
    pub publish_hidden_layers: bool,
    /// `OmitTraceActions` (default false).
    pub omit_trace: bool,
    /// `Protect`: set the SWF Protect tag (default false).
    pub protect: bool,
    /// `DebuggingPermitted` (default false).
    pub debugging_permitted: bool,
    /// `UseNetwork`: the FileAttributes use-network bit (default false).
    pub use_network: bool,
    /// `PackagePaths`: the AS2 classpath, relative to the document folder.
    pub package_paths: Vec<String>,
}

impl Default for PublishSettings {
    fn default() -> Self {
        Self {
            flash_version: 8,
            actionscript_version: 2,
            compress: true,
            jpeg_quality: 80,
            publish_hidden_layers: true,
            omit_trace: false,
            protect: false,
            debugging_permitted: false,
            use_network: false,
            package_paths: Vec::new(),
        }
    }
}

/// Where a document came from.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceInfo {
    /// The path the user opened (folder, `.xfl` stub or `.fla`).
    pub path: PathBuf,
    /// Container kind.
    pub container: ContainerKind,
    /// Base directory for `#include`, class paths and `fonts/` lookup: the
    /// folder that holds the document (for a `.fla`, the folder of the file).
    pub base_dir: PathBuf,
}

/// The kind of container a document was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ContainerKind {
    /// Built in code or read from memory; there are no files on disk.
    #[default]
    InMemory,
    /// An uncompressed XFL folder (with `DOMDocument.xml` and a `.xfl` stub).
    Folder,
    /// A CS5+ `.fla` ZIP archive.
    Zip,
}
