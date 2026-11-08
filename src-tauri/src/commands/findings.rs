//! AI-flagged findings, as the app shows them (see `db::findings` and the
//! governing doc's C1-C10): a chapter's findings for the margin, every
//! request's for the Library, removing them (one, or a request's, C4) and
//! bringing them back (Undo), "Keep as mine" (C10), and the reader's switch,
//! "Let AI agents save findings" (C6). The app never adds a finding of its
//! own: an agent does, through `gospel-getter-cli`, while the switch is on.
//!
//! The app reads what the CLI saved whenever it reads findings: the
//! chapter's each time one is shown (and after any change to the notes),
//! the Library's each time it opens or its notes reload. Nothing is pushed
//! to it while it's open.

use anyhow::Context;
use serde::{Deserialize, Serialize};

use super::notes::{NoteDto, note_dto, note_error_message};
use super::{AppState, book_name, failed};
use crate::db::{Finding, FindingRequest, KeepError, NewFinding, Passage};
use crate::passage_reference::passage_reference;
use crate::reader_data;

/// One finding.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingDto {
    pub id: i64,
    pub book_id: i64,
    pub book_name: String,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// "John 11:35".
    pub reference: String,
    /// Who found it: "Claude".
    pub agent: String,
    /// What the reader asked for: "grief".
    pub request: String,
    /// What the app shows for it, built from `agent` and `request` alone:
    /// "Found by Claude for your request “grief”." (C3).
    pub text: String,
    pub created_at: String,
    /// The day it was found, as the app shows dates ("Nov 7, 2026").
    pub created_at_display: String,
}

/// One request's findings, as the Library groups them.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingRequestDto {
    pub request: String,
    /// Who found them: "Claude", or "Claude and Codex".
    pub agents: String,
    /// How many verses they mark.
    pub verse_count: i64,
    /// The day the latest was found ("Nov 7, 2026").
    pub latest_display: String,
    /// In Bible order.
    pub findings: Vec<FindingDto>,
}

/// A removed finding to bring back (Undo): as it was, but for its id.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoredFinding {
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    pub agent: String,
    pub request: String,
    pub created_at: String,
}

fn finding_dto(state: &AppState, finding: Finding) -> FindingDto {
    let book_name = book_name(state, finding.book_id).unwrap_or("?").to_string();
    FindingDto {
        reference: passage_reference(
            &book_name,
            finding.chapter,
            Some((finding.verse_start, finding.verse_end)),
        ),
        text: finding.text(),
        created_at_display: crate::notes_export::stored_date(&finding.created_at, &chrono::Local),
        id: finding.id,
        book_id: finding.book_id,
        book_name,
        chapter: finding.chapter,
        verse_start: finding.verse_start,
        verse_end: finding.verse_end,
        agent: finding.agent,
        request: finding.request,
        created_at: finding.created_at,
    }
}

/// "Claude", "Claude and Codex", "Claude, Codex and Gemini".
pub fn agents_label(agents: &[String]) -> String {
    match agents {
        [] => crate::db::findings::DEFAULT_AGENT.to_string(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn request_dto(state: &AppState, group: FindingRequest) -> FindingRequestDto {
    FindingRequestDto {
        agents: agents_label(&group.agents),
        verse_count: group.verse_count,
        latest_display: crate::notes_export::stored_date(&group.latest, &chrono::Local),
        request: group.request,
        findings: group
            .findings
            .into_iter()
            .map(|f| finding_dto(state, f))
            .collect(),
    }
}

/// The findings on one chapter, in the order they're shown.
pub async fn chapter_findings(
    state: &AppState,
    book_id: i64,
    chapter: i64,
) -> Result<Vec<FindingDto>, String> {
    let findings = state
        .store
        .chapter_findings(book_id, chapter)
        .await
        .context("Failed to list the chapter's findings")
        .map_err(failed("The AI findings couldn't be loaded."))?;
    Ok(findings
        .into_iter()
        .map(|f| finding_dto(state, f))
        .collect())
}

/// Every request's findings, the latest request first.
pub async fn finding_requests(state: &AppState) -> Result<Vec<FindingRequestDto>, String> {
    let groups = state
        .store
        .finding_requests()
        .await
        .context("Failed to list the findings")
        .map_err(failed("The AI findings couldn't be loaded."))?;
    Ok(groups.into_iter().map(|g| request_dto(state, g)).collect())
}

/// Remove one finding, returning it as it was (for Undo), or `None` if it
/// was already gone.
pub async fn finding_remove(state: &AppState, id: i64) -> Result<Option<FindingDto>, String> {
    state
        .store
        .remove_finding(id)
        .await
        .map(|f| f.map(|f| finding_dto(state, f)))
        .with_context(|| format!("Failed to remove finding {id}"))
        .map_err(failed("The finding couldn't be removed."))
}

/// Remove every finding of one request (C4), returning them as they were.
pub async fn finding_request_remove(
    state: &AppState,
    request: &str,
) -> Result<Vec<FindingDto>, String> {
    state
        .store
        .remove_finding_request(request)
        .await
        .map(|removed| removed.into_iter().map(|f| finding_dto(state, f)).collect())
        .context("Failed to remove a request's findings")
        .map_err(failed("The findings couldn't be removed."))
}

/// Bring back removed findings (Undo): the same verses, agent, request and
/// dates. How many came back.
pub async fn findings_restore(
    state: &AppState,
    findings: Vec<RestoredFinding>,
) -> Result<usize, String> {
    let mut back = Vec::with_capacity(findings.len());
    for f in findings {
        let Some(created_at) = reader_data::normalize_timestamp(&f.created_at) else {
            return Err("That finding's date isn't valid, so it couldn't be restored.".to_string());
        };
        back.push(NewFinding {
            passage: Passage {
                book_id: f.book_id,
                chapter: f.chapter,
                verse_start: f.verse_start,
                verse_end: f.verse_end,
            },
            agent: f.agent,
            request: f.request,
            created_at: Some(created_at),
        });
    }
    match state.store.add_findings(&back).await {
        Ok(saved) => Ok(saved.saved.len()),
        Err(e) => {
            tracing::error!("Failed to restore findings: {e}");
            Err("The findings couldn't be restored.".to_string())
        }
    }
}

/// "Keep as mine" (C10): `body` saved as the reader's own note on the
/// finding's verses, and the finding removed, in one transaction.
pub async fn finding_keep(state: &AppState, id: i64, body: &str) -> Result<NoteDto, String> {
    match state.store.keep_finding_as_note(id, body).await {
        Ok(note) => Ok(note_dto(state, note)),
        Err(KeepError::FindingGone) => {
            Err("That finding is no longer there; it may have been removed.".to_string())
        }
        Err(KeepError::Note(e)) => Err(note_error_message(state, e)),
    }
}

/// Whether "Let AI agents save findings" is on.
pub async fn findings_switch(state: &AppState) -> Result<bool, String> {
    state
        .store
        .findings_enabled()
        .await
        .context("Failed to read the AI findings setting")
        .map_err(failed("The AI agents setting couldn't be read."))
}

/// Turn "Let AI agents save findings" on or off; what it is now.
pub async fn set_findings_switch(state: &AppState, enabled: bool) -> Result<bool, String> {
    state
        .store
        .set_findings_enabled(enabled)
        .await
        .context("Failed to save the AI findings setting")
        .map_err(failed("The AI agents setting couldn't be saved."))?;
    Ok(enabled)
}

#[tauri::command]
pub async fn list_findings(
    state: tauri::State<'_, AppState>,
    book_id: i64,
    chapter: i64,
) -> Result<Vec<FindingDto>, String> {
    chapter_findings(&state, book_id, chapter).await
}

#[tauri::command]
pub async fn list_finding_requests(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<FindingRequestDto>, String> {
    finding_requests(&state).await
}

#[tauri::command]
pub async fn remove_finding(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<Option<FindingDto>, String> {
    finding_remove(&state, id).await
}

#[tauri::command]
pub async fn remove_finding_request(
    state: tauri::State<'_, AppState>,
    request: String,
) -> Result<Vec<FindingDto>, String> {
    finding_request_remove(&state, &request).await
}

#[tauri::command]
pub async fn restore_findings(
    state: tauri::State<'_, AppState>,
    findings: Vec<RestoredFinding>,
) -> Result<usize, String> {
    findings_restore(&state, findings).await
}

#[tauri::command]
pub async fn keep_finding_as_note(
    state: tauri::State<'_, AppState>,
    id: i64,
    body: String,
) -> Result<NoteDto, String> {
    finding_keep(&state, id, &body).await
}

#[tauri::command]
pub async fn get_ai_findings_enabled(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    findings_switch(&state).await
}

#[tauri::command]
pub async fn set_ai_findings_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    set_findings_switch(&state, enabled).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::tests::app_state;

    #[test]
    fn several_agents_are_named_in_a_list() {
        let names = |n: &[&str]| agents_label(&n.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(names(&["Claude"]), "Claude");
        assert_eq!(names(&["Claude", "Codex"]), "Claude and Codex");
        assert_eq!(
            names(&["Claude", "Codex", "Gemini"]),
            "Claude, Codex and Gemini"
        );
    }

    #[tokio::test]
    async fn a_finding_shows_who_found_it_for_what_and_keeps_as_the_readers_note() {
        let (_db, state) = app_state("cmd_findings").await;
        let at = |verse| NewFinding {
            passage: Passage {
                book_id: 43,
                chapter: 11,
                verse_start: verse,
                verse_end: verse,
            },
            agent: "Claude".to_string(),
            request: "grief".to_string(),
            created_at: None,
        };
        state.store.add_findings(&[at(33), at(35)]).await.unwrap();
        let shown = chapter_findings(&state, 43, 11).await.unwrap();
        assert_eq!(shown.len(), 2);
        assert_eq!(shown[1].reference, "John 11:35");
        assert_eq!(
            shown[1].text,
            "Found by Claude for your request \u{201c}grief\u{201d}."
        );
        let groups = finding_requests(&state).await.unwrap();
        assert_eq!(
            (groups[0].agents.as_str(), groups[0].verse_count),
            ("Claude", 2)
        );

        let note = finding_keep(&state, shown[1].id, "  Mine now. ")
            .await
            .unwrap();
        assert_eq!(
            (note.body.as_str(), note.reference.as_str()),
            ("Mine now.", "John 11:35")
        );
        assert_eq!(chapter_findings(&state, 43, 11).await.unwrap().len(), 1);
        assert_eq!(
            finding_keep(&state, shown[1].id, "Again.")
                .await
                .unwrap_err(),
            "That finding is no longer there; it may have been removed."
        );
        assert_eq!(
            finding_keep(&state, shown[0].id, "  ").await.unwrap_err(),
            "A note can't be empty."
        );

        let removed = finding_request_remove(&state, "grief").await.unwrap();
        assert_eq!(removed.len(), 1);
        let back = removed
            .iter()
            .map(|f| RestoredFinding {
                book_id: f.book_id,
                chapter: f.chapter,
                verse_start: f.verse_start,
                verse_end: f.verse_end,
                agent: f.agent.clone(),
                request: f.request.clone(),
                created_at: f.created_at.clone(),
            })
            .collect();
        assert_eq!(findings_restore(&state, back).await.unwrap(), 1);
        assert_eq!(
            chapter_findings(&state, 43, 11).await.unwrap()[0].created_at,
            removed[0].created_at
        );

        assert!(!findings_switch(&state).await.unwrap());
        assert!(set_findings_switch(&state, true).await.unwrap());
        assert!(findings_switch(&state).await.unwrap());
    }
}
