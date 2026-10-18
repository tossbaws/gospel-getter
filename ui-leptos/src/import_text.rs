//! The import preview's words: the summary of a file, the choice between
//! adding and replacing, the warning, and how it went. The pure part of
//! what `ui/index.html`'s import script composed (`openImportPreview`,
//! `updateImportMode`, `describePreferences` and the confirm handler),
//! kept free of the DOM so it is tested natively (`cargo test`). Numbers
//! read as the page wrote them: `plural` groups thousands, the bare counts
//! do not.

use crate::dto::{ImportPreview, ImportResult, Preferences};
use crate::text::{plural, plural_as};

/// The import dialog's title, for a file that can be imported.
pub const TITLE: &str = "Import bookmarks, highlights, notes and settings";

/// Its title for one that can't.
pub const INVALID_TITLE: &str = "This file can’t be imported";

/// "bookmarks.json — exported 2026-10-04 by Gospel Getter 2.2.0".
pub fn file_line(preview: &ImportPreview) -> String {
    let exported: String = preview.exported_at.chars().take(10).collect();
    let exported = if exported.is_empty() {
        "at an unknown time".to_string()
    } else {
        exported
    };
    format!(
        "{} — exported {exported} by Gospel Getter {}",
        preview.file_name, preview.app_version
    )
}

fn repeated(n: u64) -> String {
    format!(
        " ({} in the file will be skipped.)",
        plural_as(n, "repeated entry", "repeated entries")
    )
}

/// What the file holds and what importing it would do, a line each.
/// `has_preferences` is whether any of its display settings can be named.
pub fn summary(preview: &ImportPreview, has_preferences: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if preview.bookmarks_in_file > 0 {
        let mut line = format!(
            "{}: {} new, {} you already have.",
            plural(preview.bookmarks_in_file, "bookmark"),
            preview.new_bookmarks,
            preview.already_present
        );
        if preview.duplicates_in_file > 0 {
            line.push_str(&repeated(preview.duplicates_in_file));
        }
        lines.push(line);
        let more = if preview.bookmarks_in_file > preview.sample.len() as u64 {
            ", …"
        } else {
            ""
        };
        lines.push(format!("Including {}{more}", preview.sample.join(", ")));
    } else {
        lines.push("No bookmarks.".to_string());
    }
    // Format 2 files can carry highlights; format 1 files can't, and leave
    // the reader's highlights alone.
    if preview.has_highlights {
        if preview.highlights_in_file > 0 {
            let mut line = format!(
                "{}: {} new, {} you’ve already highlighted (those keep your colors).",
                plural(preview.highlights_in_file, "highlighted verse"),
                preview.new_highlights,
                preview.already_highlighted
            );
            if preview.highlight_duplicates_in_file > 0 {
                line.push_str(&repeated(preview.highlight_duplicates_in_file));
            }
            lines.push(line);
        } else {
            lines.push("No highlights.".to_string());
        }
    }
    // Format 3 files can carry notes; older ones can't, and leave the
    // reader's notes alone.
    if preview.has_notes {
        if preview.notes_in_file > 0 {
            let mut line = format!(
                "{}: {} new, {} you already have.",
                plural(preview.notes_in_file, "note"),
                preview.new_notes,
                preview.notes_already_present
            );
            if preview.note_duplicates_in_file > 0 {
                line.push_str(&repeated(preview.note_duplicates_in_file));
            }
            lines.push(line);
        } else {
            lines.push("No notes.".to_string());
        }
    }
    if preview.position.is_none() {
        lines.push("No reading position.".to_string());
    }
    if !has_preferences {
        lines.push("No display settings.".to_string());
    }
    // Settings from a later version are skipped; the backend names them.
    if let Some(notice) = &preview.skipped_preferences_notice
        && !notice.is_empty()
    {
        lines.push(notice.clone());
    }
    lines
}

/// The add-or-replace choice's words: its legend, and its two options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub legend: String,
    pub merge: String,
    pub replace: String,
}

pub fn choice(preview: &ImportPreview) -> Choice {
    if preview.has_notes {
        Choice {
            legend: "Bookmarks, highlights and notes".to_string(),
            merge: format!(
                "Add them to mine ({}, {}, {}; anything I already have stays as it is)",
                plural(preview.new_bookmarks, "new bookmark"),
                plural(preview.new_highlights, "new highlighted verse"),
                plural(preview.new_notes, "new note")
            ),
            replace: format!(
                "Replace all my bookmarks, highlights and notes with the file’s (removes my {}, {} and {})",
                plural(preview.current_bookmarks, "current bookmark"),
                plural(preview.current_highlights, "highlighted verse"),
                plural(preview.current_notes, "note")
            ),
        }
    } else if preview.has_highlights {
        Choice {
            legend: "Bookmarks and highlights".to_string(),
            merge: format!(
                "Add them to mine ({}, {}; anything I already have stays as it is)",
                plural(preview.new_bookmarks, "new bookmark"),
                plural(preview.new_highlights, "new highlighted verse")
            ),
            replace: format!(
                "Replace all my bookmarks and highlights with the file’s (removes my {} and {})",
                plural(preview.current_bookmarks, "current bookmark"),
                plural(preview.current_highlights, "highlighted verse")
            ),
        }
    } else {
        Choice {
            legend: "Bookmarks".to_string(),
            merge: format!(
                "Add them to my bookmarks ({} new; any I already have stay as they are)",
                preview.new_bookmarks
            ),
            replace: format!(
                "Replace all my bookmarks with the file’s (removes my {})",
                plural(preview.current_bookmarks, "current bookmark")
            ),
        }
    }
}

/// The warning shown while Replace is chosen, and the confirm button's
/// label for the choice.
pub fn mode(preview: &ImportPreview, replace: bool) -> (String, &'static str) {
    let current = plural(preview.current_bookmarks, "current bookmark");
    let tail =
        "and keeps only the file’s. If you might want yours back, cancel and export them first.";
    if preview.has_notes {
        (
            format!(
                "This deletes your {current}, {} and {} {tail}",
                plural(preview.current_highlights, "highlighted verse"),
                plural(preview.current_notes, "note")
            ),
            if replace {
                "Replace bookmarks, highlights and notes and import"
            } else {
                "Import"
            },
        )
    } else if preview.has_highlights {
        (
            format!(
                "This deletes your {current} and {} {tail}",
                plural(preview.current_highlights, "highlighted verse")
            ),
            if replace {
                "Replace bookmarks and highlights and import"
            } else {
                "Import"
            },
        )
    } else {
        (
            format!("This deletes your {current} {tail}"),
            if replace {
                "Replace bookmarks and import"
            } else {
                "Import"
            },
        )
    }
}

/// The file's display settings, named as Aa names them: `label(select,
/// value)` is the text of the option `value` in Aa's `<select>` `select`
/// (or `value` itself, if it has no such option). "Matrix theme, large
/// text, …".
pub fn describe_preferences(
    prefs: &Preferences,
    label: impl Fn(&str, &str) -> String,
) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(theme) = prefs.theme.as_deref().filter(|v| !v.is_empty()) {
        parts.push(format!("{} theme", label("theme-select", theme)));
    }
    let lowered = |select: &str, value: &Option<String>, what: &str| {
        value
            .as_deref()
            .filter(|v| !v.is_empty())
            .map(|v| format!("{} {what}", label(select, v).to_lowercase()))
    };
    parts.extend(lowered("text-size-select", &prefs.text_size, "text"));
    parts.extend(lowered(
        "line-spacing-select",
        &prefs.line_spacing,
        "line spacing",
    ));
    parts.extend(lowered("width-select", &prefs.width, "width"));
    parts.extend(lowered("layout-select", &prefs.layout, "layout"));
    if let Some(compare) = prefs.compare {
        parts.push(format!("compare {}", if compare { "on" } else { "off" }));
    }
    if let Some(with) = prefs.compare_with.as_deref().filter(|v| !v.is_empty()) {
        parts.push(format!("compare with the {}", with.to_uppercase()));
    }
    if let Some(reading_mode) = prefs.reading_mode {
        parts.push(format!(
            "reading mode {}",
            if reading_mode { "on" } else { "off" }
        ));
    }
    parts
}

/// How a confirmed import went, before the display settings' part: a
/// sentence per kind of data it touched, the position it went to, and any
/// settings it skipped.
pub fn result_parts(preview: &ImportPreview, replace: bool, result: &ImportResult) -> Vec<String> {
    let mut parts = Vec::new();
    if preview.bookmarks_in_file > 0 {
        parts.push(if replace {
            format!(
                "Replaced {} with the file’s {}.",
                plural(result.removed, "bookmark"),
                plural(result.added, "bookmark")
            )
        } else {
            let had = if result.already_present > 0 {
                format!(" ({} you already had)", result.already_present)
            } else {
                String::new()
            };
            format!("Added {}{had}.", plural(result.added, "bookmark"))
        });
    }
    if preview.has_highlights
        && (preview.highlights_in_file > 0 || (replace && result.highlights_removed > 0))
    {
        parts.push(if replace {
            format!(
                "Replaced {} with the file’s {}.",
                plural(result.highlights_removed, "highlighted verse"),
                plural(result.highlights_added, "highlighted verse")
            )
        } else {
            let kept = if result.highlights_already_present > 0 {
                format!(
                    " ({} you’d already highlighted kept your colors)",
                    result.highlights_already_present
                )
            } else {
                String::new()
            };
            format!(
                "Added {}{kept}.",
                plural(result.highlights_added, "highlighted verse")
            )
        });
    }
    if preview.has_notes && (preview.notes_in_file > 0 || (replace && result.notes_removed > 0)) {
        parts.push(if replace {
            format!(
                "Replaced {} with the file’s {}.",
                plural(result.notes_removed, "note"),
                plural(result.notes_added, "note")
            )
        } else {
            let had = if result.notes_already_present > 0 {
                format!(" ({} you already had)", result.notes_already_present)
            } else {
                String::new()
            };
            format!("Added {}{had}.", plural(result.notes_added, "note"))
        });
    }
    if let Some(position) = &result.position {
        parts.push(format!("Went to {}.", position.reference));
    }
    if let Some(notice) = &preview.skipped_preferences_notice
        && !notice.is_empty()
    {
        parts.push(notice.clone());
    }
    parts
}

/// What applying the file's display settings came to.
pub fn preferences_part(applied: Result<bool, String>) -> String {
    match applied {
        Ok(true) => "Display settings applied.".to_string(),
        Ok(false) => "Display settings applied, but they couldn’t be saved here, so they’ll reset when the app restarts.".to_string(),
        Err(why) => format!("Display settings weren’t applied: {why}"),
    }
}

/// The import's closing line, for the notice and the toast.
pub fn complete(parts: &[String]) -> String {
    format!("Import complete. {}", parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::PositionPreview;

    fn bookmarks_only() -> ImportPreview {
        ImportPreview {
            file_name: "data.json".to_string(),
            exported_at: "2026-10-04T12:00:00.000Z".to_string(),
            app_version: "2.2.0".to_string(),
            bookmarks_in_file: 2,
            new_bookmarks: 1,
            already_present: 1,
            current_bookmarks: 3,
            sample: vec!["John 3:16".to_string(), "Psalms 23:1–4".to_string()],
            position: Some(PositionPreview {
                translation_code: "web".to_string(),
                book_id: 45,
                chapter: 8,
                reference: "Romans 8 (WEB)".to_string(),
            }),
            ..ImportPreview::default()
        }
    }

    fn with_everything() -> ImportPreview {
        ImportPreview {
            has_highlights: true,
            highlights_in_file: 1234,
            new_highlights: 1200,
            already_highlighted: 34,
            highlight_duplicates_in_file: 2,
            current_highlights: 5,
            has_notes: true,
            notes_in_file: 1,
            new_notes: 1,
            current_notes: 1,
            duplicates_in_file: 1,
            ..bookmarks_only()
        }
    }

    #[test]
    fn the_file_line_names_the_export() {
        assert_eq!(
            file_line(&bookmarks_only()),
            "data.json — exported 2026-10-04 by Gospel Getter 2.2.0"
        );
        let unknown = ImportPreview {
            exported_at: String::new(),
            ..bookmarks_only()
        };
        assert!(file_line(&unknown).contains("exported at an unknown time by"));
    }

    #[test]
    fn a_bookmark_file_summarised() {
        assert_eq!(
            summary(&bookmarks_only(), true),
            [
                "2 bookmarks: 1 new, 1 you already have.",
                "Including John 3:16, Psalms 23:1–4"
            ]
        );
        let more = ImportPreview {
            bookmarks_in_file: 9,
            ..bookmarks_only()
        };
        assert_eq!(
            summary(&more, true)[1],
            "Including John 3:16, Psalms 23:1–4, …"
        );
    }

    #[test]
    fn everything_summarised_with_grouped_and_bare_numbers() {
        assert_eq!(
            summary(&with_everything(), false),
            [
                "2 bookmarks: 1 new, 1 you already have. (1 repeated entry in the file will be skipped.)",
                "Including John 3:16, Psalms 23:1–4",
                "1,234 highlighted verses: 1200 new, 34 you’ve already highlighted (those keep your colors). (2 repeated entries in the file will be skipped.)",
                "1 note: 1 new, 0 you already have.",
                "No display settings.",
            ]
        );
    }

    #[test]
    fn an_empty_file_says_what_it_lacks() {
        let empty = ImportPreview {
            has_highlights: true,
            has_notes: true,
            skipped_preferences_notice: Some("Skipped: a theme.".to_string()),
            ..ImportPreview::default()
        };
        assert_eq!(
            summary(&empty, false),
            [
                "No bookmarks.",
                "No highlights.",
                "No notes.",
                "No reading position.",
                "No display settings.",
                "Skipped: a theme.",
            ]
        );
    }

    #[test]
    fn the_choice_by_format() {
        let b = choice(&bookmarks_only());
        assert_eq!(b.legend, "Bookmarks");
        assert_eq!(
            b.merge,
            "Add them to my bookmarks (1 new; any I already have stay as they are)"
        );
        assert_eq!(
            b.replace,
            "Replace all my bookmarks with the file’s (removes my 3 current bookmarks)"
        );
        let all = choice(&with_everything());
        assert_eq!(all.legend, "Bookmarks, highlights and notes");
        assert_eq!(
            all.merge,
            "Add them to mine (1 new bookmark, 1,200 new highlighted verses, 1 new note; anything I already have stays as it is)"
        );
    }

    #[test]
    fn the_warning_and_the_button() {
        let (warning, button) = mode(&bookmarks_only(), true);
        assert_eq!(
            warning,
            "This deletes your 3 current bookmarks and keeps only the file’s. If you might want yours back, cancel and export them first."
        );
        assert_eq!(button, "Replace bookmarks and import");
        assert_eq!(mode(&bookmarks_only(), false).1, "Import");
        assert_eq!(
            mode(&with_everything(), true).1,
            "Replace bookmarks, highlights and notes and import"
        );
    }

    #[test]
    fn display_settings_named_as_aa_names_them() {
        let prefs = Preferences {
            theme: Some("matrix".to_string()),
            text_size: Some("large".to_string()),
            line_spacing: Some("relaxed".to_string()),
            reading_mode: Some(false),
            compare: Some(true),
            compare_with: Some("bsb".to_string()),
            ..Preferences::default()
        };
        let label = |_: &str, value: &str| {
            let mut c = value.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        };
        assert_eq!(
            describe_preferences(&prefs, label),
            [
                "Matrix theme",
                "large text",
                "relaxed line spacing",
                "compare on",
                "compare with the BSB",
                "reading mode off"
            ]
        );
        assert!(describe_preferences(&Preferences::default(), label).is_empty());
    }

    #[test]
    fn how_it_went() {
        let result = ImportResult {
            added: 2,
            removed: 3,
            ..ImportResult::default()
        };
        assert_eq!(
            complete(&result_parts(&bookmarks_only(), true, &result)),
            "Import complete. Replaced 3 bookmarks with the file’s 2 bookmarks."
        );
        let merged = ImportResult {
            added: 1,
            already_present: 1,
            position: bookmarks_only().position,
            ..ImportResult::default()
        };
        assert_eq!(
            result_parts(&bookmarks_only(), false, &merged),
            [
                "Added 1 bookmark (1 you already had).",
                "Went to Romans 8 (WEB)."
            ]
        );
        assert_eq!(preferences_part(Ok(true)), "Display settings applied.");
        assert!(preferences_part(Ok(false)).contains("couldn’t be saved here"));
        assert_eq!(
            preferences_part(Err("x".into())),
            "Display settings weren’t applied: x"
        );
    }
}
