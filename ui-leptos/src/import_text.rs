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
    // Format 4 files can carry AI findings; older ones can't, and leave the
    // reader's findings alone.
    if preview.has_findings {
        if preview.findings_in_file > 0 {
            lines.push(format!(
                "{}: {} new, {} you already have.",
                plural(preview.findings_in_file, "AI finding"),
                preview.new_findings,
                preview.findings_already_present
            ));
        } else {
            lines.push("No AI findings.".to_string());
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

/// Whether the add-or-replace choice is offered: the file holds something
/// to add, or to replace the reader's with (bookmarks, highlights, notes
/// or AI findings).
pub fn offers_choice(preview: &ImportPreview) -> bool {
    preview.bookmarks_in_file > 0
        || preview.highlights_in_file > 0
        || preview.notes_in_file > 0
        || preview.findings_in_file > 0
}

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// What replacing removes, a collection each: its name, and how many of the
/// reader's it removes ("bookmarks", "3 current bookmarks"). A file's
/// format says which collections it replaces; AI findings are named only
/// when there are some here to remove.
fn replaced(preview: &ImportPreview) -> Vec<(String, String)> {
    let mut replaced = vec![(
        "bookmarks".to_string(),
        plural(preview.current_bookmarks, "current bookmark"),
    )];
    if preview.has_highlights {
        replaced.push((
            "highlights".to_string(),
            plural(preview.current_highlights, "highlighted verse"),
        ));
    }
    if preview.has_notes {
        replaced.push(("notes".to_string(), plural(preview.current_notes, "note")));
    }
    if preview.has_findings && preview.current_findings > 0 {
        replaced.push((
            "AI findings".to_string(),
            plural(preview.current_findings, "AI finding"),
        ));
    }
    replaced
}

/// The collections replacing removes, by name: "bookmarks, highlights and
/// notes".
fn replaced_names(preview: &ImportPreview) -> String {
    let names: Vec<String> = replaced(preview).into_iter().map(|(n, _)| n).collect();
    and_list(&names)
}

/// How many of each replacing removes: "3 current bookmarks and 5
/// highlighted verses".
fn replaced_counts(preview: &ImportPreview) -> String {
    let counts: Vec<String> = replaced(preview).into_iter().map(|(_, c)| c).collect();
    and_list(&counts)
}

/// The add-or-replace choice's words: its legend, and its two options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub legend: String,
    pub merge: String,
    pub replace: String,
}

pub fn choice(preview: &ImportPreview) -> Choice {
    let replace = format!(
        "Replace all my {} with the file’s (removes my {})",
        replaced_names(preview),
        replaced_counts(preview)
    );
    if !preview.has_highlights {
        return Choice {
            legend: "Bookmarks".to_string(),
            merge: format!(
                "Add them to my bookmarks ({} new; any I already have stay as they are)",
                preview.new_bookmarks
            ),
            replace,
        };
    }
    let mut names = vec!["bookmarks".to_string(), "highlights".to_string()];
    let mut added = vec![
        plural(preview.new_bookmarks, "new bookmark"),
        plural(preview.new_highlights, "new highlighted verse"),
    ];
    if preview.has_notes {
        names.push("notes".to_string());
        added.push(plural(preview.new_notes, "new note"));
    }
    if preview.has_findings && (preview.findings_in_file > 0 || preview.current_findings > 0) {
        names.push("AI findings".to_string());
    }
    if preview.has_findings && preview.findings_in_file > 0 {
        added.push(plural(preview.new_findings, "new AI finding"));
    }
    let legend = and_list(&names);
    let mut legend_chars = legend.chars();
    Choice {
        legend: legend_chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + legend_chars.as_str())
            .unwrap_or_default(),
        merge: format!(
            "Add them to mine ({}; anything I already have stays as it is)",
            added.join(", ")
        ),
        replace,
    }
}

/// The warning shown while Replace is chosen, and the confirm button's
/// label for the choice. Both name everything replacing removes.
pub fn mode(preview: &ImportPreview, replace: bool) -> (String, String) {
    (
        format!(
            "This deletes your {} and keeps only the file’s. If you might want yours back, cancel and export them first.",
            replaced_counts(preview)
        ),
        if replace {
            format!("Replace {} and import", replaced_names(preview))
        } else {
            "Import".to_string()
        },
    )
}

/// The option to also set "Let AI agents save findings" as the file has it:
/// its label, and whether it starts ticked. Offered only when the file has
/// the switch and it differs from the reader's. It starts unticked when it
/// would turn saving on, so that's only ever the reader's choice, and
/// ticked when it would turn it off.
pub fn permission(preview: &ImportPreview) -> Option<(String, bool)> {
    let on = preview
        .ai_findings_enabled
        .filter(|&on| preview.has_findings && on != preview.current_ai_findings_enabled)?;
    Some((
        format!(
            "Also turn {} “Let AI agents save findings”, as in the file",
            if on { "on" } else { "off" }
        ),
        !on,
    ))
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
    if preview.bookmarks_in_file > 0 || (replace && result.removed > 0) {
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
    if preview.has_findings
        && (preview.findings_in_file > 0 || (replace && result.findings_removed > 0))
    {
        parts.push(if replace {
            format!(
                "Replaced {} with the file’s {}.",
                plural(result.findings_removed, "AI finding"),
                plural(result.findings_added, "AI finding")
            )
        } else {
            let had = if result.findings_already_present > 0 {
                format!(" ({} you already had)", result.findings_already_present)
            } else {
                String::new()
            };
            format!(
                "Added {}{had}.",
                plural(result.findings_added, "AI finding")
            )
        });
    }
    if let Some(on) = result.findings_permission {
        parts.push(format!(
            "Turned {} “Let AI agents save findings”.",
            if on { "on" } else { "off" }
        ));
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

    /// A format 4 file with three AI findings, where the reader has four.
    fn with_findings() -> ImportPreview {
        ImportPreview {
            has_findings: true,
            findings_in_file: 3,
            new_findings: 2,
            findings_already_present: 1,
            current_findings: 4,
            ..with_everything()
        }
    }

    #[test]
    fn ai_findings_are_named_wherever_the_import_touches_them() {
        assert_eq!(
            summary(&with_findings(), true)[4],
            "3 AI findings: 2 new, 1 you already have."
        );
        let c = choice(&with_findings());
        assert_eq!(c.legend, "Bookmarks, highlights, notes and AI findings");
        assert_eq!(
            c.merge,
            "Add them to mine (1 new bookmark, 1,200 new highlighted verses, 1 new note, 2 new AI findings; anything I already have stays as it is)"
        );
        assert_eq!(
            c.replace,
            "Replace all my bookmarks, highlights, notes and AI findings with the file’s (removes my 3 current bookmarks, 5 highlighted verses, 1 note and 4 AI findings)"
        );
        let (warning, button) = mode(&with_findings(), true);
        assert_eq!(
            warning,
            "This deletes your 3 current bookmarks, 5 highlighted verses, 1 note and 4 AI findings and keeps only the file’s. If you might want yours back, cancel and export them first."
        );
        assert_eq!(
            button,
            "Replace bookmarks, highlights, notes and AI findings and import"
        );

        // A file with none, replacing the reader's: they're named as removed.
        let empty = ImportPreview {
            findings_in_file: 0,
            new_findings: 0,
            findings_already_present: 0,
            ..with_findings()
        };
        assert_eq!(summary(&empty, true)[4], "No AI findings.");
        assert!(
            mode(&empty, true)
                .0
                .contains(", 1 note and 4 AI findings and keeps")
        );
        assert!(!choice(&empty).merge.contains("AI finding"), "none to add");

        // None here to remove: replacing names the rest, as before.
        let none_here = ImportPreview {
            current_findings: 0,
            ..with_findings()
        };
        assert_eq!(
            mode(&none_here, true).1,
            "Replace bookmarks, highlights and notes and import"
        );
        assert!(!choice(&none_here).replace.contains("AI finding"));
        assert!(choice(&none_here).merge.contains("2 new AI findings"));
    }

    #[test]
    fn a_file_with_only_findings_offers_the_choice() {
        let only = ImportPreview {
            has_highlights: true,
            has_notes: true,
            has_findings: true,
            findings_in_file: 1,
            new_findings: 1,
            ..ImportPreview::default()
        };
        assert!(offers_choice(&only));
        assert!(!offers_choice(&ImportPreview {
            findings_in_file: 0,
            ..only
        }));
        assert!(offers_choice(&bookmarks_only()));
    }

    #[test]
    fn the_permission_is_offered_only_when_the_file_would_change_it() {
        let file = |file: Option<bool>, here: bool| ImportPreview {
            ai_findings_enabled: file,
            current_ai_findings_enabled: here,
            ..with_findings()
        };
        assert_eq!(
            permission(&file(Some(true), false)),
            Some((
                "Also turn on “Let AI agents save findings”, as in the file".to_string(),
                false
            )),
            "turning saving on starts unticked"
        );
        assert_eq!(
            permission(&file(Some(false), true)),
            Some((
                "Also turn off “Let AI agents save findings”, as in the file".to_string(),
                true
            )),
            "turning it off starts ticked"
        );
        assert_eq!(permission(&file(Some(true), true)), None);
        assert_eq!(permission(&file(Some(false), false)), None);
        assert_eq!(permission(&file(None, true)), None);
        assert_eq!(permission(&file(None, false)), None);
    }

    #[test]
    fn how_it_went_with_findings_and_the_permission() {
        let merged = ImportResult {
            findings_added: 2,
            findings_already_present: 1,
            findings_permission: Some(true),
            ..ImportResult::default()
        };
        let parts = result_parts(&with_findings(), false, &merged);
        assert!(parts.contains(&"Added 2 AI findings (1 you already had).".to_string()));
        assert!(parts.contains(&"Turned on “Let AI agents save findings”.".to_string()));

        // Replacing with a file without findings reports the removal; so does
        // replacing with one without bookmarks.
        let only_findings = ImportPreview {
            bookmarks_in_file: 0,
            sample: Vec::new(),
            findings_in_file: 0,
            ..with_findings()
        };
        let replaced = ImportResult {
            removed: 3,
            findings_removed: 4,
            findings_permission: Some(false),
            ..ImportResult::default()
        };
        let parts = result_parts(&only_findings, true, &replaced);
        assert!(parts.contains(&"Replaced 3 bookmarks with the file’s 0 bookmarks.".to_string()));
        assert!(
            parts.contains(&"Replaced 4 AI findings with the file’s 0 AI findings.".to_string())
        );
        assert!(parts.contains(&"Turned off “Let AI agents save findings”.".to_string()));

        // Left alone: nothing said about the switch.
        let untouched = result_parts(&with_findings(), false, &ImportResult::default());
        assert!(!untouched.iter().any(|p| p.contains("Let AI agents")));
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
