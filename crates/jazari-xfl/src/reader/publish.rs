//! `PublishSettings.xml`: the current Flash profile's publish properties.
//!
//! Every value is a string that may be empty, so the leading integer is read
//! and empty or missing values keep the Flash 8 / AS2 defaults (F32).

use crate::WarningKind;
use crate::model::PublishSettings;

use super::ctx::Ctx;
use super::values::{excerpt, parse_leading_int};
use super::xml::Node;

/// Reads the `<flash_profiles>` root of `PublishSettings.xml`.
pub(crate) fn read_publish_settings(ctx: &mut Ctx, root: &Node) -> PublishSettings {
    let mut settings = PublishSettings::default();
    let profiles: Vec<&Node> = if root.name == "flash_profile" {
        vec![root]
    } else {
        root.children_named("flash_profile").collect()
    };
    let profile = profiles
        .iter()
        .copied()
        .find(|p| p.attr("current") == Some("true"))
        .or_else(|| profiles.first().copied());
    let Some(props) = profile.and_then(|p| p.child("PublishFlashProperties")) else {
        return settings;
    };

    let int = |name: &str| -> Option<i64> {
        props
            .child(name)
            .map(|n| n.text.trim())
            .filter(|t| !t.is_empty())
            .and_then(parse_leading_int)
    };
    let flag = |name: &str, default: bool| int(name).map_or(default, |v| v != 0);

    if let Some(text) = props
        .child("Version")
        .map(|n| n.text.trim())
        .filter(|t| !t.is_empty())
    {
        match parse_leading_int(text) {
            Some(v) => {
                settings.flash_version = u8::try_from(v.clamp(0, 255)).unwrap_or(8);
                if v != 8 {
                    ctx.warn(
                        WarningKind::Unsupported,
                        format!("the document targets Flash Player {v}; Al-Jazari publishes SWF 8"),
                    );
                }
            }
            None => ctx.warn(
                WarningKind::Unsupported,
                format!(
                    "the document targets \"{}\"; Al-Jazari publishes SWF 8",
                    excerpt(text)
                ),
            ),
        }
    }
    if let Some(v) = int("ActionScriptVersion") {
        settings.actionscript_version = u8::try_from(v.clamp(0, 255)).unwrap_or(2);
        if v != 2 {
            ctx.warn(
                WarningKind::Unsupported,
                format!("the document uses ActionScript {v}; Al-Jazari compiles ActionScript 2"),
            );
        }
    }
    settings.compress = flag("CompressMovie", settings.compress);
    if let Some(q) = int("Quality") {
        settings.jpeg_quality = u8::try_from(q.clamp(0, 100)).unwrap_or(80);
    }
    settings.publish_hidden_layers = flag("InvisibleLayer", settings.publish_hidden_layers);
    settings.omit_trace = flag("OmitTraceActions", settings.omit_trace);
    settings.protect = flag("Protect", settings.protect);
    settings.debugging_permitted = flag("DebuggingPermitted", settings.debugging_permitted);
    settings.use_network = flag("UseNetwork", settings.use_network);
    if let Some(paths) = props.child("PackagePaths") {
        settings.package_paths = paths
            .text
            .split(';')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect();
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::xml::parse_str;

    fn read(xml: &str) -> (PublishSettings, usize) {
        let mut ctx = Ctx::default();
        let s = read_publish_settings(&mut ctx, &parse_str(xml).unwrap());
        (s, ctx.report.len())
    }

    #[test]
    fn current_profile_values() {
        let (s, w) = read(
            r#"<flash_profiles>
              <flash_profile name="Other" current="false"><PublishFlashProperties><Version>6</Version></PublishFlashProperties></flash_profile>
              <flash_profile name="Default" current="true"><PublishFlashProperties enabled="true">
                <Version>8</Version><ActionScriptVersion>2</ActionScriptVersion><CompressMovie>0</CompressMovie>
                <Quality>65</Quality><InvisibleLayer>0</InvisibleLayer><OmitTraceActions>1</OmitTraceActions>
                <Protect>1</Protect><DebuggingPermitted/><UseNetwork>1</UseNetwork>
                <PackagePaths>./classes; ../shared</PackagePaths>
              </PublishFlashProperties></flash_profile>
            </flash_profiles>"#,
        );
        assert_eq!(w, 0);
        assert_eq!((s.flash_version, s.actionscript_version), (8, 2));
        assert!(!s.compress && !s.publish_hidden_layers);
        assert!(s.omit_trace && s.protect && s.use_network && !s.debugging_permitted);
        assert_eq!(s.jpeg_quality, 65);
        assert_eq!(s.package_paths, ["./classes", "../shared"]);
    }

    #[test]
    fn empty_values_keep_defaults_and_other_targets_warn() {
        let (s, w) = read(
            r#"<flash_profiles><flash_profile><PublishFlashProperties>
              <Version>FlashLite40</Version><ActionScriptVersion>3</ActionScriptVersion><CompressMovie/>
            </PublishFlashProperties></flash_profile></flash_profiles>"#,
        );
        assert_eq!(w, 2);
        assert_eq!(s.flash_version, 8);
        assert_eq!(s.actionscript_version, 3);
        assert!(s.compress);
        let (s, w) = read("<flash_profiles/>");
        assert_eq!((s, w), (PublishSettings::default(), 0));
    }
}
