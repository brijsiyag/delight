//! Icons: [Lucide](https://lucide.dev) v1.48.0 (ISC, see
//! `assets/icons/LICENSE-lucide`), embedded at compile time and served to
//! GPUI by [`Assets`]. One list generates both the embedded files and
//! [`IconName`], so a name can't point at a missing file.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

/// Delight's logo (SVG) — the one copy every part of the app uses.
pub const LOGO_SVG: &[u8] = include_bytes!("../assets/logo.svg");

/// The app's asset source: install it with `Application::with_assets(Assets)`.
pub struct Assets;

macro_rules! icons {
    ($($name:ident => $file:literal),* $(,)?) => {
        /// A built-in icon; draw it with [`crate::Icon`].
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum IconName {
            $($name),*
        }

        impl IconName {
            pub fn path(self) -> SharedString {
                match self {
                    $(IconName::$name => concat!("icons/", $file, ".svg")),*
                }
                .into()
            }
        }

        impl IconName {
            /// The icon with this [Lucide](https://lucide.dev) file name (`"terminal"`),
            /// for a name that comes from data, such as a plugin's permission.
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($file => Some(IconName::$name),)*
                    _ => None,
                }
            }
        }

        const ICONS: &[(&str, &[u8])] = &[
            $((concat!("icons/", $file, ".svg"), include_bytes!(concat!("../assets/icons/", $file, ".svg")))),*
        ];
    };
}

icons![
    ArrowDownAZ => "arrow-down-a-z",
    Check => "check",
    CircleCheck => "circle-check",
    CircleX => "circle-x",
    Copy => "copy",
    File => "file",
    Folder => "folder",
    Globe => "globe",
    Grid => "grid-2x2",
    History => "history",
    IndentIncrease => "list-indent-increase",
    Info => "info",
    Lightbulb => "lightbulb",
    Moon => "moon",
    Plus => "plus",
    Puzzle => "puzzle",
    RefreshCw => "refresh-cw",
    Search => "search",
    Settings => "settings",
    Sparkles => "sparkles",
    Sun => "sun",
    Terminal => "terminal",
    Trash => "trash-2",
    TriangleAlert => "triangle-alert",
    X => "x",
    Zap => "zap",
];

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS.iter().find(|(p, _)| *p == path).map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS.iter().map(|(p, _)| *p).filter(|p| p.starts_with(path)).map(SharedString::from).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_name_loads() {
        for name in [IconName::Check, IconName::Settings, IconName::Trash, IconName::Zap] {
            assert!(Assets.load(&name.path()).unwrap().is_some(), "{name:?}");
        }
        assert_eq!(Assets.list("icons/").unwrap().len(), ICONS.len());
        assert_eq!(IconName::from_name("terminal"), Some(IconName::Terminal));
        assert_eq!(IconName::from_name("nope"), None);
    }
}
