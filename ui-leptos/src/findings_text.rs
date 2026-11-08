//! AI-flagged findings' rules and markup, free of the DOM (`findings` and
//! `margin` put them on the page). A finding marks where a verse is: an AI
//! agent, asked by the reader ("note every verse that deals with grief"),
//! found it. It shows who found it and for which request, never anything
//! about the verse (C3), and always in its own look, the violet "AI" badge,
//! so it can't be taken for the reader's own note or for scripture (C2).
//!
//! In the margin each verse's findings are one entry beside it, after the
//! reader's own notes there (C9): collapsed as the badge and the verse (C7),
//! "AI ×2" when there are two; expanded, each finding's words with "Keep as
//! mine" and "Remove". A narrow window has a marker after the verse instead,
//! after the reader's pencil.

use crate::dto::Finding;
use crate::reader_text::escape_html;

/// The badge's word.
pub const BADGE: &str = "AI";

/// The action that turns a finding into the reader's own note (C10).
pub const KEEP: &str = "Keep as mine";

/// The action that removes one finding.
pub const REMOVE: &str = "Remove";

/// The Library's action that removes a request's findings together (C4).
pub const REMOVE_ALL: &str = "Remove all";

/// The Library's heading over the reader's own notes, when there are
/// findings beside them.
pub const MY_NOTES: &str = "My notes";

/// The Library's heading over the findings, grouped by request.
pub const FOUND_BY_AI: &str = "Found by AI agents";

/// Said when the switch can't be read or saved.
pub const SWITCH_FAILED: &str = "The AI agents setting couldn\u{2019}t be saved.";

/// One verse's findings (the same verses): what the margin shows as one
/// entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingGroup {
    pub verse_start: i64,
    pub verse_end: i64,
    /// "John 11:35".
    pub reference: String,
    /// In the order the backend gave them (oldest first).
    pub findings: Vec<Finding>,
}

impl FindingGroup {
    /// What names the group on the page: its verses, "35-35".
    pub fn key(&self) -> String {
        format!("{}-{}", self.verse_start, self.verse_end)
    }
}

/// `findings` (as the backend ordered them) grouped by their verses, the
/// groups in the order their first finding came.
pub fn groups(findings: Vec<Finding>) -> Vec<FindingGroup> {
    let mut groups: Vec<FindingGroup> = Vec::new();
    for finding in findings {
        match groups
            .iter_mut()
            .find(|g| g.verse_start == finding.verse_start && g.verse_end == finding.verse_end)
        {
            Some(group) => group.findings.push(finding),
            None => groups.push(FindingGroup {
                verse_start: finding.verse_start,
                verse_end: finding.verse_end,
                reference: finding.reference.clone(),
                findings: vec![finding],
            }),
        }
    }
    groups
}

/// The badge: "AI", or "AI ×2" for two findings on the verse.
pub fn badge(count: usize) -> String {
    if count > 1 {
        format!("{BADGE} \u{d7}{count}")
    } else {
        BADGE.to_string()
    }
}

/// What a group is called, for a screen reader and as a marker's tooltip.
pub fn label(group: &FindingGroup) -> String {
    match group.findings.len() {
        1 => format!("AI finding on {}", group.reference),
        n => format!("{n} AI findings on {}", group.reference),
    }
}

/// One finding's words, as markup: "Found by Claude for your request
/// “grief”." (the backend's `text`, its agent and request marked out).
pub fn words_html(finding: &Finding) -> String {
    format!(
        "Found by <span class=\"finding-agent\">{}</span> for your request <span \
         class=\"finding-request\">\u{201c}{}\u{201d}</span>.",
        escape_html(&finding.agent),
        escape_html(&finding.request)
    )
}

/// One finding with its two actions. `tag` is `li` in a list, or `span`
/// inside a verse (a marker's).
fn item_html(finding: &Finding, tag: &str) -> String {
    let id = finding.id;
    let reference = escape_html(&finding.reference);
    let request = escape_html(&finding.request);
    format!(
        r#"<{tag} class="finding-item" data-finding-id="{id}"><span class="finding-text">{words}</span> <span class="finding-actions"><button type="button" class="finding-keep note-link" data-finding-id="{id}" aria-label="Keep the finding on {reference} for &#8220;{request}&#8221; as your own note">{KEEP}</button> <button type="button" class="finding-remove note-link" data-finding-id="{id}" aria-label="Remove the finding on {reference} for &#8220;{request}&#8221;">{REMOVE}</button></span></{tag}>"#,
        words = words_html(finding),
    )
}

/// A group's findings, expanded: each with its actions, hidden until the
/// toggle `id` belongs to opens it.
fn body_html(group: &FindingGroup, id: &str) -> String {
    let items: String = group.findings.iter().map(|f| item_html(f, "li")).collect();
    format!(r#"<ul class="finding-list" id="{id}" hidden>{items}</ul>"#)
}

/// One verse's findings in the margin (C7): a toggle showing the badge and
/// `shown` (the reference, or the notes column's "35"), collapsed, and the
/// findings it expands to. It opens collapsed every time it's drawn.
fn entry_html(group: &FindingGroup, shown: &str, where_: &str) -> String {
    let key = group.key();
    let body = format!("finding-body-{where_}-{key}");
    format!(
        r#"<article class="margin-finding" data-finding-key="{key}" aria-label="{label}"><button type="button" class="margin-finding-head" data-finding-key="{key}" aria-expanded="false" aria-controls="{body}"><span class="ai-badge">{badge}</span> <span class="margin-finding-ref">{shown}</span></button>{items}</article>"#,
        label = escape_html(&label(group)),
        badge = badge(group.findings.len()),
        shown = escape_html(shown),
        items = body_html(group, &body),
    )
}

/// One verse's findings in the margin.
pub fn margin_html(group: &FindingGroup) -> String {
    entry_html(group, &group.reference, "margin")
}

/// One verse's findings in Compare's notes column, or under its verse,
/// labelled with its verses as the column labels the reader's notes.
pub fn column_html(group: &FindingGroup, where_: &str) -> String {
    entry_html(
        group,
        &crate::margin_text::verses_label(group.verse_start, group.verse_end),
        where_,
    )
}

/// A verse's findings where there's no room for the margin (C9): a small
/// violet marker after the verse, after the reader's pencil, that opens
/// the findings in the text, after it.
pub fn marker_html(group: &FindingGroup) -> String {
    let key = group.key();
    let label = escape_html(&label(group));
    let items: String = group
        .findings
        .iter()
        .map(|f| item_html(f, "span"))
        .collect();
    format!(
        r#"<button type="button" class="margin-marker finding-marker" data-finding-key="{key}" aria-expanded="false" aria-controls="finding-inline-{key}" aria-label="{label}" title="{label}"><span class="ai-badge">{badge}</span></button><span class="finding-inline" id="finding-inline-{key}" data-finding-key="{key}" hidden>{items}</span>"#,
        badge = badge(group.findings.len()),
    )
}

/// What the toast says once one finding is removed.
pub fn removed(finding: &Finding) -> String {
    format!(
        "Removed the AI finding on {} for \u{201c}{}\u{201d}.",
        finding.reference, finding.request
    )
}

/// What the toast says once a request's findings are removed.
pub fn removed_all(count: usize, request: &str) -> String {
    match count {
        1 => format!("Removed 1 AI finding for \u{201c}{request}\u{201d}."),
        n => format!("Removed {n} AI findings for \u{201c}{request}\u{201d}."),
    }
}

/// What the toast says once Undo has brought them back.
pub fn restored(count: usize) -> String {
    match count {
        1 => "The AI finding is back.".to_string(),
        n => format!("The {n} AI findings are back."),
    }
}

/// "1 verse", "38 verses".
pub fn verse_count(n: i64) -> String {
    if n == 1 {
        "1 verse".to_string()
    } else {
        format!("{n} verses")
    }
}

/// A request's line in the Library: "Found by Claude · 38 verses · Nov 7, 2026".
pub fn request_meta(agents: &str, verses: i64, date: &str) -> String {
    format!(
        "Found by {agents} \u{b7} {} \u{b7} {date}",
        verse_count(verses)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: i64, verse: i64, request: &str) -> Finding {
        Finding {
            id,
            book_id: 43,
            book_name: "John".to_string(),
            chapter: 11,
            verse_start: verse,
            verse_end: verse,
            reference: format!("John 11:{verse}"),
            agent: "Claude".to_string(),
            request: request.to_string(),
            text: format!("Found by Claude for your request \u{201c}{request}\u{201d}."),
            created_at: "2026-11-07T00:00:00.000Z".to_string(),
            created_at_display: "Nov 7, 2026".to_string(),
        }
    }

    /// The text without its tags, as the page reads it.
    fn plain(html: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => out.push(c),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn findings_on_one_verse_are_one_group_in_order() {
        let groups = groups(vec![
            finding(1, 33, "grief"),
            finding(2, 35, "grief"),
            finding(3, 35, "tears"),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[1].reference, "John 11:35");
        assert_eq!(groups[1].key(), "35-35");
        let ids: Vec<i64> = groups[1].findings.iter().map(|f| f.id).collect();
        assert_eq!(ids, [2, 3]);
    }

    #[test]
    fn the_badge_counts_the_findings_on_a_verse() {
        assert_eq!(badge(1), "AI");
        assert_eq!(badge(2), "AI \u{d7}2");
        let two = &groups(vec![finding(1, 37, "grief"), finding(2, 37, "death")])[0];
        assert_eq!(label(two), "2 AI findings on John 11:37");
    }

    /// The words shown are the backend's, character for character: who found
    /// it and for what, nothing about the verse.
    #[test]
    fn a_findings_words_are_who_found_it_and_for_what() {
        let f = finding(1, 35, "grief & <loss>");
        assert_eq!(
            plain(&words_html(&f))
                .replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">"),
            f.text
        );
        assert!(
            words_html(&f).contains("grief &amp; &lt;loss&gt;"),
            "escaped"
        );
    }

    #[test]
    fn an_entry_opens_collapsed_with_its_badge_and_verse() {
        let group = &groups(vec![finding(9, 35, "grief")])[0];
        let html = margin_html(group);
        assert!(html.contains(r#"aria-expanded="false""#));
        assert!(
            html.contains(r#"<ul class="finding-list" id="finding-body-margin-35-35" hidden>"#)
        );
        assert!(plain(&html).starts_with("AI John 11:35"));
        assert!(html.contains(KEEP) && html.contains(REMOVE));
        assert!(
            column_html(group, "column").contains(r#"<span class="margin-finding-ref">35</span>"#)
        );
        let marker = marker_html(group);
        assert!(marker.contains(r#"class="margin-marker finding-marker""#));
        assert!(marker.contains("hidden><span class=\"finding-item\""));
    }

    #[test]
    fn the_library_and_the_toasts_say_it_plainly() {
        assert_eq!(
            request_meta("Claude", 38, "Nov 7, 2026"),
            "Found by Claude \u{b7} 38 verses \u{b7} Nov 7, 2026"
        );
        assert_eq!(verse_count(1), "1 verse");
        assert_eq!(
            removed_all(2, "grief"),
            "Removed 2 AI findings for \u{201c}grief\u{201d}."
        );
        assert_eq!(
            removed(&finding(1, 35, "grief")),
            "Removed the AI finding on John 11:35 for \u{201c}grief\u{201d}."
        );
        assert_eq!(restored(1), "The AI finding is back.");
    }
}
