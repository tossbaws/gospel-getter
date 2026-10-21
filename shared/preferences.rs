//! Gospel Getter's saved settings, in one place: what each is called, where
//! the page keeps it (a local storage key and an `<html>` data attribute),
//! the values it may have and what Aa calls them, its value when nothing
//! (or nothing valid) is saved, and its name in an export file.
//!
//! This one file is compiled into both crates that need it, as their
//! `preferences` module (`#[path]`): the backend (`src-tauri`), whose
//! `reader_data` reads and writes an export file's display settings as the
//! enums here, and the page's Rust pieces (`ui-leptos`), whose store loads,
//! checks, applies and saves the settings and whose Aa offers them. The
//! page's pre-paint script, which applies the saved look before the first
//! frame, carries a table generated from this one ([`prepaint_script`]); a
//! test (`src-tauri/tests/frontend_markup.rs`) keeps it the same, byte for
//! byte. The keys and values are the ones readers already have saved and
//! export files already carry: they never change.

/// One of a setting's values, and what Aa calls it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

/// Defines one of the settings Aa offers as an enum, as an export file
/// carries it (its kebab-case value), with its [`Choice`]s in Aa's order.
macro_rules! choices {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $value:literal, $label:literal;)+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $($(#[$variant_meta])* #[serde(rename = $value)] $variant,)+
        }

        impl $name {
            /// Every value, in Aa's order, with what it's called.
            pub const CHOICES: &'static [Choice] = &[$(Choice { value: $value, label: $label },)+];
        }
    };
}

choices! {
    /// The look: colours and type (Aa's Theme row).
    Theme {
        ChiRho = "chi-rho", "Chi Rho";
        Vellum = "vellum", "Vellum";
        /// Chi Rho or Vellum, following the computer's light or dark setting.
        System = "system", "System";
        Vaporwave = "vaporwave", "Vaporwave";
        ClassicDark = "classic-dark", "Classic Dark";
        ClassicLight = "classic-light", "Classic Light";
        Matrix = "matrix", "Matrix";
        BeastSlayer = "beast-slayer", "Beast Slayer";
        HotPink = "hot-pink", "Hot Pink";
    }
}

choices! {
    /// The reading text's size (Aa's Text size stepper).
    TextSize {
        Small = "small", "Small";
        Medium = "medium", "Medium";
        Large = "large", "Large";
        XLarge = "x-large", "Extra large";
    }
}

choices! {
    /// Aa's Line spacing row.
    LineSpacing {
        Compact = "compact", "Compact";
        Normal = "normal", "Normal";
        Relaxed = "relaxed", "Relaxed";
    }
}

choices! {
    /// The reading column's width (Aa's Width row).
    Width {
        Narrow = "narrow", "Narrow";
        Normal = "normal", "Normal";
        Wide = "wide", "Wide";
    }
}

choices! {
    /// How a chapter is set (Aa's Layout row): a verse to a line, or the
    /// verses flowing on as prose. Presentation only: the text is the same.
    Layout {
        Verses = "verses", "Verses";
        Flowing = "flowing", "Flowing";
    }
}

/// What a setting may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Values {
    /// One of Aa's choices.
    Choices(&'static [Choice]),
    /// One of these words.
    Words(&'static [&'static str]),
    /// A translation's code: one to sixteen lowercase letters or digits.
    /// Which translations there are is the backend's to say, so the code is
    /// checked against them where it's used.
    Code,
}

impl Values {
    /// Whether `value` is one of them.
    pub fn allow(self, value: &str) -> bool {
        match self {
            Self::Choices(choices) => choices.iter().any(|c| c.value == value),
            Self::Words(words) => words.contains(&value),
            Self::Code => is_code(value),
        }
    }

    /// Every value, in order (none for a code).
    pub fn list(self) -> Vec<&'static str> {
        match self {
            Self::Choices(choices) => choices.iter().map(|c| c.value).collect(),
            Self::Words(words) => words.to_vec(),
            Self::Code => Vec::new(),
        }
    }
}

/// A translation's code: one to sixteen lowercase ASCII letters or digits.
pub fn is_code(value: &str) -> bool {
    (1..=16).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// One saved setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preference {
    /// The page's name for it, which is also its `<html>` data attribute
    /// by its `dataset` name (`textSize` is `data-text-size`).
    pub name: &'static str,
    /// Its local storage key.
    pub key: &'static str,
    /// Its value when nothing valid is saved: the page's look before the
    /// setting existed. It shows as no data attribute at all.
    pub fallback: &'static str,
    pub values: Values,
    /// Saved values that were renamed, and their new names: a reader who
    /// saved the old one keeps it under the new name.
    pub renamed: &'static [(&'static str, &'static str)],
    /// The hidden `<select>` that records it for the page's other scripts
    /// (Aa's settings have one), if any.
    pub select: Option<&'static str>,
    /// Its name in an export file; none for a setting that's this
    /// computer's (the sidebar, the launch check), which isn't exported.
    pub file_name: Option<&'static str>,
}

impl Preference {
    /// Whether `value` is one it may have.
    pub fn allows(&self, value: &str) -> bool {
        self.values.allow(value)
    }

    /// `value`'s new name, if it was renamed.
    pub fn renamed_to(&self, value: &str) -> Option<&'static str> {
        self.renamed
            .iter()
            .find(|(old, _)| *old == value)
            .map(|(_, new)| *new)
    }

    /// What Aa calls `value`, if it's one of Aa's choices.
    pub fn label(&self, value: &str) -> Option<&'static str> {
        match self.values {
            Values::Choices(choices) => choices.iter().find(|c| c.value == value).map(|c| c.label),
            _ => None,
        }
    }

    /// Exported as `true` or `false` ("on" or "off" here).
    pub fn on_off(&self) -> bool {
        self.values == ON_OFF
    }
}

const ON_OFF: Values = Values::Words(&["off", "on"]);

pub const THEME: Preference = Preference {
    name: "theme",
    key: "gospel-getter-theme",
    fallback: "chi-rho",
    values: Values::Choices(Theme::CHOICES),
    renamed: &[("monster-hunter", "beast-slayer")],
    select: Some("theme-select"),
    file_name: Some("theme"),
};

pub const TEXT_SIZE: Preference = Preference {
    name: "textSize",
    key: "gospel-getter-text-size",
    fallback: "medium",
    values: Values::Choices(TextSize::CHOICES),
    renamed: &[],
    select: Some("text-size-select"),
    file_name: Some("text_size"),
};

pub const LINE_SPACING: Preference = Preference {
    name: "lineSpacing",
    key: "gospel-getter-line-spacing",
    fallback: "normal",
    values: Values::Choices(LineSpacing::CHOICES),
    renamed: &[],
    select: Some("line-spacing-select"),
    file_name: Some("line_spacing"),
};

/// The reading column's width (and reading mode's); Normal is the width
/// it always had.
pub const WIDTH: Preference = Preference {
    name: "width",
    key: "gospel-getter-width",
    fallback: "normal",
    values: Values::Choices(Width::CHOICES),
    renamed: &[],
    select: Some("width-select"),
    file_name: Some("width"),
};

/// A verse to a line, as it always was, or flowing on as prose. Compare
/// keeps a verse to a row whichever is chosen.
pub const LAYOUT: Preference = Preference {
    name: "layout",
    key: "gospel-getter-layout",
    fallback: "verses",
    values: Values::Choices(Layout::CHOICES),
    renamed: &[],
    select: Some("layout-select"),
    file_name: Some("layout"),
};

pub const READER_MODE: Preference = Preference {
    name: "readerMode",
    key: "gospel-getter-reader-mode",
    fallback: "off",
    values: ON_OFF,
    renamed: &[],
    select: None,
    file_name: Some("reading_mode"),
};

/// Compare translations side by side.
pub const COMPARE: Preference = Preference {
    name: "compare",
    key: "gospel-getter-compare",
    fallback: "off",
    values: ON_OFF,
    renamed: &[],
    select: None,
    file_name: Some("compare"),
};

/// The translation compare shows beside the one being read, once the
/// reader has chosen one.
pub const COMPARE_WITH: Preference = Preference {
    name: "compareWith",
    key: "gospel-getter-compare-with",
    fallback: "",
    values: Values::Code,
    renamed: &[],
    select: None,
    file_name: Some("compare_with"),
};

/// Whether the book and chapter picker shows as a sidebar on a wide
/// window.
pub const SIDEBAR: Preference = Preference {
    name: "sidebar",
    key: "gospel-getter-sidebar",
    fallback: "open",
    values: Values::Words(&["open", "closed"]),
    renamed: &[],
    select: None,
    file_name: None,
};

/// Check for updates when the app starts.
pub const UPDATE_CHECK: Preference = Preference {
    name: "updateCheck",
    key: "gospel-getter-update-check",
    fallback: "on",
    values: Values::Words(&["on", "off"]),
    renamed: &[],
    select: None,
    file_name: None,
};

/// Every saved setting, in the order they're applied (and exported).
pub const PREFERENCES: [Preference; 10] = [
    THEME,
    TEXT_SIZE,
    LINE_SPACING,
    WIDTH,
    LAYOUT,
    READER_MODE,
    COMPARE,
    COMPARE_WITH,
    SIDEBAR,
    UPDATE_CHECK,
];

/// The setting the page calls `name`.
pub fn by_name(name: &str) -> Option<&'static Preference> {
    PREFERENCES.iter().find(|p| p.name == name)
}

/// The setting an export file calls `file_name`.
pub fn by_file_name(file_name: &str) -> Option<&'static Preference> {
    PREFERENCES.iter().find(|p| p.file_name == Some(file_name))
}

/// The window width from which the book and chapter picker can be a
/// sidebar, as a media query.
pub const WIDE_QUERY: &str = "(min-width: 1200px)";

/// The page's pre-paint script: the inline `<script>` at the top of
/// `ui/index.html`'s `<head>`, from its opening tag to its closing one,
/// as it stands there (indented for its place). It applies what's saved
/// as each setting's data attribute on `<html>` before the first frame
/// (renamed values under their new names, anything else unknown as the
/// default), then the picker's shape: `data-wide` on a wide window, and
/// `data-picker="sidebar"` while the sidebar shows there. It only reads.
pub fn prepaint_script() -> String {
    let quoted = |s: &str| format!("'{s}'");
    let rows: String = PREFERENCES
        .iter()
        .map(|p| {
            let values = match p.values {
                Values::Code => "null".to_string(),
                values => format!(
                    "[{}]",
                    values
                        .list()
                        .into_iter()
                        .map(quoted)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            };
            let renamed = p
                .renamed
                .iter()
                .map(|(old, new)| format!(" {}: {} ", quoted(old), quoted(new)))
                .collect::<String>();
            format!(
                "                [{}, {}, {}, {values}, {{{renamed}}}],\n",
                quoted(p.key),
                quoted(p.name),
                quoted(p.fallback),
            )
        })
        .collect();
    format!(
        r"    <script>
        // The saved look, before the first frame: each saved setting as its
        // data attribute on <html> (none for its default), which the CSS
        // keys off, then the book and chapter picker's shape, so the page
        // is laid out for it from the start. Only that: the page's Rust
        // pieces (ui-leptos/src/aa.rs) keep the settings from then on, and
        // without them the Bible text still shows in the saved look. The
        // table is generated from shared/preferences.rs (prepaint_script);
        // a test keeps the two the same.
        (function () {{
            'use strict';
            var data = document.documentElement.dataset;
            // Local storage key, data attribute, default, the values it
            // may have (null: a translation's code), renamed values.
            var SAVED = [
{rows}            ];
            SAVED.forEach(function (pref) {{
                var value = null;
                try {{
                    value = localStorage.getItem(pref[0]);
                }} catch (e) {{}}
                if (value && Object.prototype.hasOwnProperty.call(pref[4], value)) value = pref[4][value];
                var valid = pref[3] ? pref[3].indexOf(value) !== -1 : typeof value === 'string' && /^[a-z0-9]{{1,16}}$/.test(value);
                if (valid && value !== pref[2]) data[pref[1]] = value;
                else delete data[pref[1]];
            }});
            // A sidebar on a window this wide, unless the reader hid it;
            // never in reading mode.
            var wide = !!(window.matchMedia && window.matchMedia('{WIDE_QUERY}').matches);
            if (wide) data.wide = '';
            else delete data.wide;
            if (wide && data.sidebar !== 'closed' && data.readerMode !== 'on') data.picker = 'sidebar';
            else delete data.picker;
        }})();
    </script>
"
    )
}
