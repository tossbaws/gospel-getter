//! AI-flagged findings: where an AI agent, asked by the reader ("note every
//! verse that deals with grief so I can read them"), marked a verse, so the
//! reader can go through them and read each one for themselves. See the
//! governing doc's C1-C10 (`2026-11-04-ai-access.md`).
//!
//! A finding is a marker, never commentary (C3): who found the verse
//! (`agent`) and for what (`request`), and nothing else. The table has no
//! column for free text, so no words about a verse can be stored beside it;
//! what the app shows is built from those two fields alone ([`finding_text`]).
//! A finding is never the reader's note either (C2, C9): it lives in its own
//! table, and only the reader turns one into a note ([`keep_as_mine`], C10).
//!
//! Agents only ever add findings (C4), through [`FindingsWriter`], the one
//! thing beside the app that writes to its database, and only while the
//! reader has turned it on ([`ENABLED_KEY`], off until then: C6). Removing
//! them is the reader's, in the app.

use anyhow::Context;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{FromRow, SqliteConnection, SqlitePool};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use super::notes::{Note, NoteBodyError, NoteError, Passage, grouped, insert_note, normalize_body};
use super::{BUSY_TIMEOUT, BookmarkError, WRITE_QUEUE_TIMEOUT, WritePool};

/// The most characters an agent's name keeps; a longer one is cut short
/// there. The `ai_findings` table's CHECK says the same.
pub const MAX_AGENT_CHARS: usize = 60;

/// The most characters a request may have: a short topic label ("grief", "the
/// sun"), never room for words about a verse (C3). The `ai_findings` table's
/// CHECK says the same.
pub const MAX_REQUEST_CHARS: usize = 60;

/// What a request can't hold, besides line breaks and characters that can't
/// be seen (`breaks_or_hides`): the punctuation that would start a sentence
/// about the verse ("grief: here Jesus shows…"), not a label.
pub const NOT_IN_A_REQUEST: [char; 4] = [':', ';', '\u{2014}', '\u{2013}'];

/// Whether `c` breaks a line or can't be seen, so a request or an agent's
/// name holding it isn't one visible line: a control character (Cc: a line
/// break, a tab…), a line or paragraph separator (Zl, Zp: U+2028, U+2029), or
/// a format character (Cf: U+200B zero width space, U+200C and U+200D zero
/// width non-joiner and joiner, U+2060 word joiner, U+FEFF, the bidi controls
/// such as U+202E…), which would make "grief" and "grief\u{200B}" two
/// requests that look the same. Only a joiner inside a word or an emoji is
/// let be ([`hidden_chars`]).
fn breaks_or_hides(c: char) -> bool {
    use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
    matches!(
        c.general_category(),
        GeneralCategory::Control
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
            | GeneralCategory::Format
    )
}

/// The zero width non-joiner and joiner (U+200C, U+200D).
const JOINERS: [char; 2] = ['\u{200C}', '\u{200D}'];

/// Each character of `text`, and whether it breaks a line or can't be seen
/// there ([`breaks_or_hides`]). A zero width non-joiner or joiner
/// ([`JOINERS`]) with a visible non-ASCII character on each side is part of
/// how that's written, so it doesn't hide: in a Persian word
/// ("می\u{200C}خواهم"), an Indic conjunct, or an emoji joined from several
/// ("👨\u{200D}👩\u{200D}👧"). Anywhere else (at either end, beside an ASCII
/// character or a space, or beside another invisible character, as when
/// doubled) it hides, as any format character does.
fn hidden_chars(text: &str) -> impl Iterator<Item = (char, bool)> {
    let chars: Vec<char> = text.chars().collect();
    let visible_non_ascii = |c: Option<&char>| {
        c.is_some_and(|&c| !c.is_ascii() && !c.is_whitespace() && !breaks_or_hides(c))
    };
    (0..chars.len()).map(move |i| {
        let c = chars[i];
        let joins = JOINERS.contains(&c)
            && visible_non_ascii(i.checked_sub(1).and_then(|before| chars.get(before)))
            && visible_non_ascii(chars.get(i + 1));
        (c, breaks_or_hides(c) && !joins)
    })
}

/// How every refusal of a request starts: what a request is.
const A_REQUEST_IS: &str =
    "The request is a short topic label, such as \u{201c}grief\u{201d} or \u{201c}the sun\u{201d}";

/// The most AI findings Gospel Getter keeps, every request's together, as
/// [`MAX_NOTES`](super::notes::MAX_NOTES) is for notes: a backup holds them
/// all, and it bounds a backup's size (`reader_data::MAX_FILE_BYTES` in the
/// app). Enforced where an agent's save is (`insert_findings`), and only
/// there: the app's Undo ([`restore_findings`]) and an import put back the
/// reader's own records, which a cap must never refuse (decision AF1), and
/// an install from before the cap (2.26.0) may hold more.
pub const MAX_FINDINGS: usize = 10_000;

/// Who a finding is said to be by when its agent gives no name.
pub const DEFAULT_AGENT: &str = "AI agent";

/// The `app_settings` key of the reader's switch, "Let AI agents save
/// findings": `"on"` when it's on. Absent, or anything else, is off (C6).
pub const ENABLED_KEY: &str = "ai_findings_enabled";

/// What [`ENABLED_KEY`] holds when the switch is on.
const ON: &str = "on";

/// Where the reader turns the switch on, as every refusal says it.
pub const HOW_TO_TURN_ON: &str = "Saving findings is turned off in Gospel Getter. To turn it \
    on, open Gospel Getter, choose Aa in the top bar, and under AI agents turn on \u{201c}Let AI \
    agents save findings\u{201d}. Then try again.";

/// What a database too old to hold findings is answered with: the app, at
/// 2.26.0 or newer, adds the tables the first time it opens it.
pub const NOT_READY: &str = "Open Gospel Getter 2.26.0 or newer once, then try again.";

/// The words the app shows for a finding (C2, C3): who found it and for what
/// request, never anything about the verse.
pub fn finding_text(agent: &str, request: &str) -> String {
    format!("Found by {agent} for your request \u{201c}{request}\u{201d}.")
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Finding {
    pub id: i64,
    pub book_id: i64,
    pub chapter: i64,
    pub verse_start: i64,
    pub verse_end: i64,
    /// Who found it: "Claude".
    pub agent: String,
    /// What the reader asked for: "grief".
    pub request: String,
    /// UTC, ISO 8601 with milliseconds — sorts chronologically as text.
    pub created_at: String,
}

impl Finding {
    pub fn passage(&self) -> Passage {
        Passage {
            book_id: self.book_id,
            chapter: self.chapter,
            verse_start: self.verse_start,
            verse_end: self.verse_end,
        }
    }

    /// The words the app shows for it ([`finding_text`]).
    pub fn text(&self) -> String {
        finding_text(&self.agent, &self.request)
    }
}

/// Why a request can't be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestError {
    /// Nothing but whitespace.
    Empty,
    /// More than [`MAX_REQUEST_CHARS`] characters, after trimming.
    TooLong { chars: usize },
    /// One of [`NOT_IN_A_REQUEST`], a line break, a tab, or another control,
    /// separator or invisible format character (a zero width non-joiner or
    /// joiner outside a word or an emoji): more than a label, or more than can
    /// be seen.
    NotALabel,
}

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "{A_REQUEST_IS}; this one is empty."),
            Self::TooLong { chars } => write!(
                f,
                "{A_REQUEST_IS}: at most {MAX_REQUEST_CHARS} characters, and this one has \
                 {chars}."
            ),
            Self::NotALabel => write!(
                f,
                "{A_REQUEST_IS}, so it can't have a colon, a semicolon, a dash (\u{2014} or \
                 \u{2013}), a line break or a character that can't be seen."
            ),
        }
    }
}

impl std::error::Error for RequestError {}

/// A request as it's stored: trimmed, then a short topic label (C3): 1 to
/// [`MAX_REQUEST_CHARS`] characters on one visible line, none of them one of
/// [`NOT_IN_A_REQUEST`].
pub fn normalize_request(raw: &str) -> Result<String, RequestError> {
    let request = raw.trim();
    if request.is_empty() {
        return Err(RequestError::Empty);
    }
    if hidden_chars(request).any(|(c, hidden)| hidden || NOT_IN_A_REQUEST.contains(&c)) {
        return Err(RequestError::NotALabel);
    }
    let chars = request.chars().count();
    if chars > MAX_REQUEST_CHARS {
        return Err(RequestError::TooLong { chars });
    }
    Ok(request.to_string())
}

/// An agent's name as it's stored: control, separator and invisible format
/// characters (`hidden_chars`: a joiner inside a word or an emoji is kept) as
/// spaces, trimmed, cut short at [`MAX_AGENT_CHARS`] characters, never ending
/// in a space or a joiner, or [`DEFAULT_AGENT`] if that leaves nothing.
pub fn normalize_agent(raw: &str) -> String {
    let spaced: String = hidden_chars(raw)
        .map(|(c, hidden)| if hidden { ' ' } else { c })
        .collect();
    let cut: String = spaced.trim().chars().take(MAX_AGENT_CHARS).collect();
    let agent = cut.trim_end_matches(|c: char| c.is_whitespace() || JOINERS.contains(&c));
    if agent.is_empty() {
        DEFAULT_AGENT.to_string()
    } else {
        agent.to_string()
    }
}

/// The label a finding from a backup gets when nothing of its own label is
/// left that today's rule allows ([`recover_request`]).
pub const EARLIER_REQUEST: &str = "Earlier request";

/// A label read back from the reader's own records, as it's stored now, and
/// whether it had to be tidied up to get there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    pub label: String,
    /// It broke today's rule, so it isn't the label the record held.
    pub adjusted: bool,
}

/// A request label from a backup, as today's rule ([`normalize_request`])
/// allows it: unchanged if the rule accepts it, otherwise cleaned
/// (`recover_label`), or [`EARLIER_REQUEST`] if nothing is left.
///
/// A restore cleans a label instead of refusing it (decision AF1 of the
/// 2026-11-19 audit fix chain): the reader's own backup must always
/// restore, and a finding saved under an older, looser rule (2.26.0 to
/// 2.29.2 let a request hold a line or paragraph separator and invisible
/// format characters, which 2.29.3 refused) would otherwise make the whole
/// file, notes and all, refuse to import. A new agent write never comes
/// through here: it keeps the strict rule and its refusal
/// ([`FindingsWriter::add`]). Two findings of the same passage whose labels
/// clean to the same request are then one finding, as two alike in a file
/// always were: the database keeps one per passage and request, and the
/// import counts the rest as repeats.
pub fn recover_request(raw: &str) -> Recovered {
    recover_label(
        raw,
        &NOT_IN_A_REQUEST,
        MAX_REQUEST_CHARS,
        normalize_request,
        EARLIER_REQUEST,
    )
}

/// `raw` as `rule` stores it, if `rule` accepts it; otherwise cleaned until
/// `rule` does: each character it can't hold (one of `refused`, or one that
/// breaks the line or can't be seen, `hidden_chars`) as a space, runs of
/// spaces as one, trimmed, cut to `max_chars` without ending in a space or a
/// joiner. Cleaning again until nothing changes, since a space put in can
/// leave a joiner beside it that no longer joins. `fallback` if that leaves
/// nothing `rule` accepts. Deterministic, and never a label `rule` refuses
/// unless `fallback` is one. The one shape for every label restored from
/// the reader's records, so each kind's rule is the only thing that differs.
fn recover_label<E>(
    raw: &str,
    refused: &[char],
    max_chars: usize,
    rule: impl Fn(&str) -> Result<String, E>,
    fallback: &str,
) -> Recovered {
    if let Ok(label) = rule(raw) {
        return Recovered {
            label,
            adjusted: false,
        };
    }
    let mut label = raw.to_string();
    let label = loop {
        let mut cleaned = String::with_capacity(label.len());
        for (c, hidden) in hidden_chars(&label) {
            let c = if hidden || refused.contains(&c) {
                ' '
            } else {
                c
            };
            if !(c == ' ' && cleaned.ends_with(' ')) {
                cleaned.push(c);
            }
        }
        let cut: String = cleaned.trim().chars().take(max_chars).collect();
        let cut = cut
            .trim_end_matches(|c: char| c.is_whitespace() || JOINERS.contains(&c))
            .to_string();
        if let Ok(accepted) = rule(&cut) {
            break accepted;
        }
        if cut == label || cut.is_empty() {
            break fallback.to_string();
        }
        label = cut;
    };
    Recovered {
        label,
        adjusted: true,
    }
}

/// A finding to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFinding {
    pub passage: Passage,
    /// As given: it's normalized as it's saved ([`normalize_agent`]).
    pub agent: String,
    /// As given: it's checked as it's saved ([`normalize_request`]).
    pub request: String,
    /// When it was found, in the database's format, to keep (a finding
    /// restored, or imported); `None` for "now".
    pub created_at: Option<String>,
}

/// What saving findings came to, in the order given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SavedFindings {
    /// Saved now.
    pub saved: Vec<Passage>,
    /// Already saved for the same request: left as they were.
    pub already: Vec<Passage>,
}

/// Why findings couldn't be saved. Nothing was.
#[derive(Debug)]
pub enum FindingsError {
    /// The database has no `ai_findings` or `app_settings` table: it was last
    /// opened by Gospel Getter 2.25.x or older ([`NOT_READY`]).
    NotReady,
    /// The reader hasn't turned it on ([`HOW_TO_TURN_ON`]).
    Off,
    Request(RequestError),
    /// A passage that doesn't exist (the same checks as bookmarks).
    Passage(BookmarkError),
    /// Saving them would take the reader past [`MAX_FINDINGS`] findings.
    Limit,
    Database(anyhow::Error),
}

impl fmt::Display for FindingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReady => f.write_str(NOT_READY),
            Self::Off => f.write_str(HOW_TO_TURN_ON),
            Self::Request(e) => write!(f, "{e}"),
            Self::Passage(e) => write!(f, "{e}"),
            Self::Limit => write!(
                f,
                "Gospel Getter holds at most {} AI findings; remove some in the Library first.",
                grouped(MAX_FINDINGS)
            ),
            Self::Database(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for FindingsError {}

impl From<RequestError> for FindingsError {
    fn from(e: RequestError) -> Self {
        Self::Request(e)
    }
}

impl From<BookmarkError> for FindingsError {
    fn from(e: BookmarkError) -> Self {
        match e {
            BookmarkError::Database(e) => Self::Database(e),
            other => Self::Passage(other),
        }
    }
}

impl From<anyhow::Error> for FindingsError {
    fn from(e: anyhow::Error) -> Self {
        Self::Database(e)
    }
}

impl From<sqlx::Error> for FindingsError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.into())
    }
}

/// The columns a `Finding` is read from.
macro_rules! finding_columns {
    () => {
        "id, book_id, chapter, verse_start, verse_end, agent, request, created_at"
    };
}

/// The order findings are shown in: Bible order, then oldest first, then by
/// id (the order they were saved).
macro_rules! bible_order {
    () => {
        " ORDER BY book_id, chapter, verse_start, verse_end, created_at, id"
    };
}

/// Whether the database has the tables findings need (2.26.0 and newer
/// create them).
pub async fn findings_ready(conn: &mut SqliteConnection) -> anyhow::Result<bool> {
    let found: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
         AND name IN ('ai_findings', 'app_settings')",
    )
    .fetch_one(&mut *conn)
    .await
    .context("Failed to look for the findings' tables")?;
    Ok(found == 2)
}

/// Whether the reader has turned on "Let AI agents save findings" (C6).
async fn enabled_on(conn: &mut SqliteConnection) -> anyhow::Result<bool> {
    let value: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = $1")
        .bind(ENABLED_KEY)
        .fetch_optional(&mut *conn)
        .await
        .context("Failed to read the AI findings setting")?;
    Ok(value.as_deref() == Some(ON))
}

/// Whether the reader has turned on "Let AI agents save findings" (C6).
pub async fn findings_enabled(pool: &SqlitePool) -> anyhow::Result<bool> {
    let mut conn = pool
        .acquire()
        .await
        .context("Failed to read the AI findings setting")?;
    enabled_on(&mut conn).await
}

/// Turn "Let AI agents save findings" on or off.
pub async fn set_findings_enabled(writer: &WritePool, on: bool) -> anyhow::Result<()> {
    let mut conn = writer
        .pool()
        .acquire()
        .await
        .context("Failed to save the AI findings setting")?;
    set_enabled_on(&mut conn, on).await
}

/// [`set_findings_enabled`] on `conn` (in an import's transaction, say).
pub(crate) async fn set_enabled_on(conn: &mut SqliteConnection, on: bool) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES ($1, $2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(ENABLED_KEY)
    .bind(if on { ON } else { "off" })
    .execute(&mut *conn)
    .await
    .context("Failed to save the AI findings setting")?;
    Ok(())
}

/// How many findings there are, every request's.
async fn finding_count(conn: &mut SqliteConnection) -> anyhow::Result<usize> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_findings")
        .fetch_one(&mut *conn)
        .await
        .context("Failed to count the findings")?;
    usize::try_from(count).context("negative finding count")
}

/// Each finding's passage checked, and the finding saved unless the same
/// verses are already saved for the same request, all on `conn`, which is in
/// a transaction that writes. One that can't be saved stops them all, and so
/// does one that would make more than `cap` in all ([`FindingsError::Limit`]):
/// all or none, as every caller rolls its transaction back on an error, so a
/// save that would pass the limit saves nothing. One already saved for its
/// request adds nothing, so it never counts against the limit: at the limit,
/// saving the same verses again still answers that they're already saved.
/// `cap` is `Some(MAX_FINDINGS)` for what an agent saves and `None` for an
/// import (see [`MAX_FINDINGS`]).
pub(crate) async fn insert_findings(
    conn: &mut SqliteConnection,
    findings: &[NewFinding],
    cap: Option<usize>,
) -> Result<SavedFindings, FindingsError> {
    let mut checked: HashMap<(i64, i64), (i64, i64)> = HashMap::new();
    let mut saved = SavedFindings::default();
    let mut count = match cap {
        Some(_) => finding_count(&mut *conn).await?,
        None => 0,
    };
    for finding in findings {
        let request = normalize_request(&finding.request)?;
        let agent = normalize_agent(&finding.agent);
        check_finding_passage(&mut *conn, &mut checked, &finding.passage).await?;
        let p = finding.passage;
        let inserted = sqlx::query(
            "INSERT INTO ai_findings \
                 (book_id, chapter, verse_start, verse_end, agent, request, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, \
                 COALESCE($7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))) \
             ON CONFLICT (book_id, chapter, verse_start, verse_end, request) DO NOTHING",
        )
        .bind(p.book_id)
        .bind(p.chapter)
        .bind(p.verse_start)
        .bind(p.verse_end)
        .bind(&agent)
        .bind(&request)
        .bind(finding.created_at.as_deref())
        .execute(&mut *conn)
        .await
        .context("Failed to save a finding")?
        .rows_affected();
        if inserted > 0 {
            count += 1;
            if cap.is_some_and(|cap| count > cap) {
                return Err(FindingsError::Limit);
            }
            saved.saved.push(p);
        } else {
            saved.already.push(p);
        }
    }
    Ok(saved)
}

/// The same checks as a bookmark's passage (`bookmarks::check_passage`),
/// each book's and chapter's limits read once however many findings are in
/// it: `checked` holds each (book, chapter)'s chapter count and last verse.
async fn check_finding_passage(
    conn: &mut SqliteConnection,
    checked: &mut HashMap<(i64, i64), (i64, i64)>,
    passage: &Passage,
) -> Result<(), BookmarkError> {
    let Passage {
        book_id,
        chapter,
        verse_start,
        verse_end,
    } = *passage;
    if verse_start < 1 || verse_end < verse_start {
        return Err(BookmarkError::InvalidRange {
            verse_start,
            verse_end,
        });
    }
    let (chapter_count, last_verse) = match checked.get(&(book_id, chapter)) {
        Some(&limits) => limits,
        None => {
            let chapter_count: Option<i64> =
                sqlx::query_scalar("SELECT chapter_count FROM books WHERE id = $1")
                    .bind(book_id)
                    .fetch_optional(&mut *conn)
                    .await
                    .context("Failed to look up book")?;
            let last_verse =
                crate::db::queries::chapter_last_verse(&mut *conn, book_id, chapter).await?;
            let limits = (chapter_count.unwrap_or(0), last_verse);
            checked.insert((book_id, chapter), limits);
            limits
        }
    };
    if !(1..=chapter_count).contains(&chapter) {
        return Err(BookmarkError::NoSuchChapter { book_id, chapter });
    }
    if verse_end > last_verse {
        return Err(BookmarkError::NoSuchVerse {
            book_id,
            chapter,
            verse: verse_end,
            last_verse,
        });
    }
    Ok(())
}

/// Save findings as an agent's save does ([`FindingsWriter::add`]'s rules
/// and its [`MAX_FINDINGS`] limit), all or none, but through the app's own
/// connection and without asking the switch: the test bridges seed findings
/// with it. The app's Undo puts back what it removed with
/// [`restore_findings`] instead.
pub async fn add_findings(
    writer: &WritePool,
    findings: &[NewFinding],
) -> Result<SavedFindings, FindingsError> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start saving findings")?;
    let saved = insert_findings(&mut tx, findings, Some(MAX_FINDINGS)).await?;
    tx.commit().await.context("Failed to save findings")?;
    Ok(saved)
}

/// A removed finding to put back as it was ([`restore_findings`]): its
/// passage, agent, request and date, but not its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedFinding {
    pub passage: Passage,
    pub agent: String,
    pub request: String,
    pub created_at: String,
}

impl From<Finding> for RemovedFinding {
    fn from(f: Finding) -> Self {
        Self {
            passage: f.passage(),
            agent: f.agent,
            request: f.request,
            created_at: f.created_at,
        }
    }
}

/// Put back findings the reader removed (the app's Undo of
/// [`remove_finding`] or [`remove_request`]), each exactly as it was: its
/// agent, request and date byte for byte, all or none. How many came back;
/// one whose passage and request an agent saved again meanwhile is already
/// there and is left as it is.
///
/// Undo puts back a record, it doesn't make a new one, so none of a new
/// write's rules apply: not today's label rule, which a finding saved under
/// an older rule may break (decision AF1; see [`recover_request`]), and not
/// [`MAX_FINDINGS`], which an agent may have reached meanwhile. Only the
/// table's own constraints do. It takes a [`WritePool`], which only the app
/// has: nothing outside it can write a finding this way (see
/// [`FindingsWriter`]).
pub async fn restore_findings(
    writer: &WritePool,
    findings: &[RemovedFinding],
) -> anyhow::Result<usize> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start restoring findings")?;
    let mut restored = 0;
    for f in findings {
        let p = f.passage;
        restored += sqlx::query(
            "INSERT INTO ai_findings \
                 (book_id, chapter, verse_start, verse_end, agent, request, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (book_id, chapter, verse_start, verse_end, request) DO NOTHING",
        )
        .bind(p.book_id)
        .bind(p.chapter)
        .bind(p.verse_start)
        .bind(p.verse_end)
        .bind(&f.agent)
        .bind(&f.request)
        .bind(&f.created_at)
        .execute(&mut *tx)
        .await
        .context("Failed to restore a finding")?
        .rows_affected();
    }
    tx.commit().await.context("Failed to restore findings")?;
    usize::try_from(restored).context("restored count out of range")
}

/// The findings on one chapter, in the order they're shown.
pub async fn list_findings(
    pool: &SqlitePool,
    book_id: i64,
    chapter: i64,
) -> anyhow::Result<Vec<Finding>> {
    sqlx::query_as::<_, Finding>(concat!(
        "SELECT ",
        finding_columns!(),
        " FROM ai_findings WHERE book_id = $1 AND chapter = $2",
        bible_order!()
    ))
    .bind(book_id)
    .bind(chapter)
    .fetch_all(pool)
    .await
    .context("Failed to read the chapter's findings")
}

/// Every finding, in Bible order.
pub async fn all_findings(pool: &SqlitePool) -> anyhow::Result<Vec<Finding>> {
    sqlx::query_as::<_, Finding>(concat!(
        "SELECT ",
        finding_columns!(),
        " FROM ai_findings",
        bible_order!()
    ))
    .fetch_all(pool)
    .await
    .context("Failed to read the findings")
}

/// One request's findings, as the Library groups them (C4: a request's
/// findings are listed and removed together).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingRequest {
    pub request: String,
    /// Who found them, each once, in the order they first did.
    pub agents: Vec<String>,
    /// How many verses they mark in all, each verse once however many of
    /// them mark it.
    pub verse_count: i64,
    /// When the latest of them was found.
    pub latest: String,
    /// In Bible order.
    pub findings: Vec<Finding>,
}

/// Every request's findings, the latest request first ([`group_requests`]).
pub async fn list_requests(pool: &SqlitePool) -> anyhow::Result<Vec<FindingRequest>> {
    Ok(group_requests(all_findings(pool).await?))
}

/// `findings` (in Bible order) grouped by request, exactly as it was saved
/// (as the table's uniqueness counts it: "grief" and "Grief" are two): each
/// group's findings in that order, every one of them kept, its agents in the
/// order they first found one, the verses they mark counted once each
/// however many of its findings mark them, the groups the latest found
/// first (then by request).
pub fn group_requests(findings: Vec<Finding>) -> Vec<FindingRequest> {
    let mut groups: Vec<FindingRequest> = Vec::new();
    for finding in findings {
        let at = match groups.iter().position(|g| g.request == finding.request) {
            Some(at) => at,
            None => {
                groups.push(FindingRequest {
                    request: finding.request.clone(),
                    agents: Vec::new(),
                    verse_count: 0,
                    latest: String::new(),
                    findings: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[at];
        if finding.created_at > group.latest {
            group.latest.clone_from(&finding.created_at);
        }
        group.findings.push(finding);
    }
    for group in &mut groups {
        group.verse_count = verses_marked(&group.findings);
        let mut by_age: Vec<&Finding> = group.findings.iter().collect();
        by_age.sort_by(|a, b| (&a.created_at, a.id).cmp(&(&b.created_at, b.id)));
        for finding in by_age {
            if !group.agents.contains(&finding.agent) {
                group.agents.push(finding.agent.clone());
            }
        }
    }
    groups.sort_by(|a, b| {
        b.latest
            .cmp(&a.latest)
            .then_with(|| a.request.cmp(&b.request))
    });
    groups
}

/// How many verses `findings` mark: each verse of each book and chapter
/// once, however many of the findings' ranges cover it (John 11:33-35 and
/// John 11:35 mark three), and the same verse number in another chapter or
/// book as another verse.
fn verses_marked(findings: &[Finding]) -> i64 {
    let mut ranges: Vec<(i64, i64, i64, i64)> = findings
        .iter()
        .map(|f| (f.book_id, f.chapter, f.verse_start, f.verse_end))
        .collect();
    ranges.sort_unstable();
    let mut count = 0;
    // The run of overlapping ranges being joined: its book, chapter, first
    // and last verse.
    let mut run: Option<(i64, i64, i64, i64)> = None;
    for (book, chapter, start, end) in ranges {
        match &mut run {
            Some((b, c, _, last)) if *b == book && *c == chapter && start <= *last => {
                *last = (*last).max(end);
            }
            _ => {
                if let Some((_, _, first, last)) = run {
                    count += last - first + 1;
                }
                run = Some((book, chapter, start, end));
            }
        }
    }
    if let Some((_, _, first, last)) = run {
        count += last - first + 1;
    }
    count
}

/// Remove the finding with `id` and `created_at` (see
/// [`super::notes::update_note`] for why both), returning it as it was (to
/// bring back with [`restore_findings`]), or `None` if it was already gone.
pub async fn remove_finding(
    writer: &WritePool,
    id: i64,
    created_at: &str,
) -> anyhow::Result<Option<Finding>> {
    sqlx::query_as::<_, Finding>(concat!(
        "DELETE FROM ai_findings WHERE id = $1 AND created_at = $2 RETURNING ",
        finding_columns!()
    ))
    .bind(id)
    .bind(created_at)
    .fetch_optional(writer.pool())
    .await
    .context("Failed to remove the finding")
}

/// Remove every finding of one request (C4), returning them as they were
/// (to bring back with [`restore_findings`]), in Bible order.
pub async fn remove_request(writer: &WritePool, request: &str) -> anyhow::Result<Vec<Finding>> {
    let mut removed = sqlx::query_as::<_, Finding>(concat!(
        "DELETE FROM ai_findings WHERE request = $1 RETURNING ",
        finding_columns!()
    ))
    .bind(request)
    .fetch_all(writer.pool())
    .await
    .context("Failed to remove the request's findings")?;
    removed.sort_by(|a, b| {
        (
            a.book_id,
            a.chapter,
            a.verse_start,
            a.verse_end,
            &a.created_at,
            a.id,
        )
            .cmp(&(
                b.book_id,
                b.chapter,
                b.verse_start,
                b.verse_end,
                &b.created_at,
                b.id,
            ))
    });
    Ok(removed)
}

/// Why "Keep as mine" didn't make a note.
#[derive(Debug)]
pub enum KeepError {
    /// There's no such finding (it was removed meanwhile).
    FindingGone,
    /// The note couldn't be saved, for the reasons any note can't.
    Note(NoteError),
}

impl fmt::Display for KeepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FindingGone => f.write_str("That finding is no longer there."),
            Self::Note(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for KeepError {}

impl From<NoteError> for KeepError {
    fn from(e: NoteError) -> Self {
        Self::Note(e)
    }
}

impl From<NoteBodyError> for KeepError {
    fn from(e: NoteBodyError) -> Self {
        Self::Note(e.into())
    }
}

impl From<anyhow::Error> for KeepError {
    fn from(e: anyhow::Error) -> Self {
        Self::Note(e.into())
    }
}

/// "Keep as mine" (C10): the reader's note `body` on the verses of the
/// finding with `id` and `created_at` (see [`super::notes::update_note`] for
/// why both), saved as any note is (the same checks, the same limits, no
/// mark of the AI's), and the finding removed, in one transaction: the
/// finding goes only if the note is saved. A verse that already has notes
/// gets another one; none is ever changed.
pub async fn keep_as_mine(
    writer: &WritePool,
    id: i64,
    created_at: &str,
    body: &str,
) -> Result<Note, KeepError> {
    let body = normalize_body(body)?;
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start keeping the finding")?;
    let finding = sqlx::query_as::<_, Finding>(concat!(
        "SELECT ",
        finding_columns!(),
        " FROM ai_findings WHERE id = $1 AND created_at = $2"
    ))
    .bind(id)
    .bind(created_at)
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to read the finding")?
    .ok_or(KeepError::FindingGone)?;
    let note = insert_note(&mut tx, &finding.passage(), &body, None).await?;
    sqlx::query("DELETE FROM ai_findings WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("Failed to remove the kept finding")?;
    tx.commit()
        .await
        .context("Failed to finish keeping the finding")?;
    Ok(note)
}

/// The one way a program beside the app writes to its database: it adds AI
/// findings, while the reader has that turned on, and nothing else (C4, C6).
///
/// It isn't a [`WritePool`], so nothing in `db` that writes anything else
/// can be handed it, and it never gives out its connection: its only write
/// is [`FindingsWriter::add`]'s insert into `ai_findings`. It never creates
/// a table or a database. It connects as the app does (write-ahead logging,
/// `synchronous = NORMAL`, waiting up to [`BUSY_TIMEOUT`] for a lock, taking
/// the write lock at the start of its one transaction), so a running app and
/// this take turns rather than fail. Open it for one save and close it.
///
/// Handing it to something that writes notes doesn't compile (a
/// `FindingsWriter` isn't a `WritePool`: E0308):
///
/// ```compile_fail,E0308
/// use gospel_getter_core::db::{FindingsWriter, Passage, notes};
///
/// async fn try_to_write_a_note(writer: &FindingsWriter, passage: &Passage) {
///     let _ = notes::add_note(writer, passage, "Not the reader's words.", None).await;
/// }
/// ```
///
/// nor does reaching for its connection (a private field: E0616):
///
/// ```compile_fail,E0616
/// use gospel_getter_core::db::FindingsWriter;
///
/// fn connection(writer: &FindingsWriter) -> &sqlx::SqlitePool {
///     &writer.pool
/// }
/// ```
#[derive(Debug)]
pub struct FindingsWriter {
    pool: SqlitePool,
}

impl FindingsWriter {
    /// Open the app's database at `path` to save findings to it. It must
    /// already be there.
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(WRITE_QUEUE_TIMEOUT)
            .connect_with(options)
            .await
            .with_context(|| format!("Failed to open {} to save findings", path.display()))?;
        Ok(Self { pool })
    }

    /// Save one finding by `agent` for `request` on each of `passages`, all
    /// or none, once the database has the tables and the reader has turned
    /// it on, both checked in the same transaction. The same verses already
    /// saved for the same request are left as they were. Nothing is saved if
    /// it would make more than [`MAX_FINDINGS`] ([`FindingsError::Limit`]).
    pub async fn add(
        &self,
        agent: &str,
        request: &str,
        passages: &[Passage],
    ) -> Result<SavedFindings, FindingsError> {
        let request = normalize_request(request)?;
        let findings: Vec<NewFinding> = passages
            .iter()
            .map(|&passage| NewFinding {
                passage,
                agent: agent.to_string(),
                request: request.clone(),
                created_at: None,
            })
            .collect();
        let mut tx = self
            .pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .context("Failed to start saving findings")?;
        if !findings_ready(&mut tx).await? {
            return Err(FindingsError::NotReady);
        }
        if !enabled_on(&mut tx).await? {
            return Err(FindingsError::Off);
        }
        let saved = insert_findings(&mut tx, &findings, Some(MAX_FINDINGS)).await?;
        tx.commit().await.context("Failed to save findings")?;
        Ok(saved)
    }

    /// Close the connection.
    pub async fn close(self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(id: i64, verse: i64, request: &str, agent: &str, at: &str) -> Finding {
        Finding {
            id,
            book_id: 43,
            chapter: 11,
            verse_start: verse,
            verse_end: verse,
            agent: agent.to_string(),
            request: request.to_string(),
            created_at: at.to_string(),
        }
    }

    #[test]
    fn the_shown_text_names_only_the_agent_and_the_request() {
        assert_eq!(
            finding_text("Claude", "grief"),
            "Found by Claude for your request \u{201c}grief\u{201d}."
        );
        let f = finding(1, 35, "grief", "Claude", "2026-11-07T00:00:00.000Z");
        assert_eq!(f.text(), finding_text("Claude", "grief"));
    }

    #[test]
    fn a_request_is_trimmed_and_checked() {
        assert_eq!(normalize_request("  grief \n"), Ok("grief".to_string()));
        assert_eq!(normalize_request(" \t "), Err(RequestError::Empty));
        // A label, in a few words: these are all one.
        for label in [
            "the sun",
            "every reference to the sun",
            "born-again",
            "Lazarus's tomb",
            "grief (and loss)",
        ] {
            assert_eq!(normalize_request(label), Ok(label.to_string()), "{label}");
        }
        // More than a label: refused, whatever comes after.
        for not_one in [
            "grief: here Jesus shows that he shares our sorrow",
            "grief; and loss",
            "grief \u{2014} his tears",
            "grief \u{2013} loss",
            "grief\nand loss",
            "grief\r\nand loss",
            "grief\tloss",
        ] {
            assert_eq!(
                normalize_request(not_one),
                Err(RequestError::NotALabel),
                "{not_one:?}"
            );
        }
        let longest = "\u{1d11e}".repeat(MAX_REQUEST_CHARS);
        assert_eq!(normalize_request(&longest), Ok(longest.clone()));
        assert_eq!(
            normalize_request(&format!("{longest}x")),
            Err(RequestError::TooLong {
                chars: MAX_REQUEST_CHARS + 1
            })
        );
    }

    /// A request is one line a reader can see all of (H-02): no line or
    /// paragraph separator, and no invisible format character (a zero-width
    /// space, a joiner outside a word, the word joiner, a byte-order mark,
    /// bidi controls), which would make "grief" and "grief\u{200B}" two
    /// requests that look the same.
    #[test]
    fn a_request_is_one_visible_line() {
        for label in [
            "grief\u{2028}Jesus weeps here",
            "grief\u{2029}x",
            "grief\u{200B}",
            "\u{2060}grief",
            "\u{FEFF}grief",
            "gr\u{200C}ief",
            "grief\u{200D}",
            "\u{202E}feirg",
            "grief\u{2066}x\u{2069}",
            "grief\u{AD}",
        ] {
            assert_eq!(
                normalize_request(label),
                Err(RequestError::NotALabel),
                "{label:?}"
            );
        }
        // Labels of any script, with marks, symbols and emoji, are still
        // labels, as they were given.
        for label in [
            "the sun",
            "caf\u{e9}",
            "cafe\u{301}",
            "λύπη",
            "אבל",
            "حزن",
            "悲伤",
            "दुःख",
            "🙏 prayer",
            "❤\u{fe0f} love",
            "grief 😢",
        ] {
            assert_eq!(normalize_request(label), Ok(label.to_string()), "{label:?}");
        }
    }

    /// An agent's name is one visible line too: a line or paragraph separator
    /// or an invisible character becomes a space, as a control character does.
    #[test]
    fn an_agent_is_one_visible_line() {
        assert_eq!(normalize_agent("Claude\u{2028}x"), "Claude x");
        assert_eq!(normalize_agent("Claude\u{2029}x"), "Claude x");
        assert_eq!(normalize_agent("Claude\u{200B}Code"), "Claude Code");
        assert_eq!(normalize_agent("\u{FEFF}Claude\u{2060}"), "Claude");
        assert_eq!(normalize_agent("\u{202E}Claude"), "Claude");
        assert_eq!(normalize_agent("\u{200B}\u{2028}"), DEFAULT_AGENT);
        assert_eq!(
            normalize_agent("Caf\u{e9} \u{1f916}"),
            "Caf\u{e9} \u{1f916}"
        );
    }

    /// A zero width non-joiner or joiner between two visible non-ASCII
    /// characters is part of how a word or an emoji is written: a request or
    /// an agent's name keeps it as it was given. Anywhere else it's refused in
    /// a request, and a space in a name, as other invisible characters are.
    #[test]
    fn a_joiner_is_kept_only_inside_a_word_or_an_emoji() {
        let persian = "می\u{200C}خواهم";
        let family = "👨\u{200D}👩\u{200D}👧 family";
        let conjunct = "क्\u{200D}ष";
        for label in [persian, family, conjunct] {
            assert_eq!(normalize_request(label), Ok(label.to_string()), "{label:?}");
            assert_eq!(normalize_agent(label), label, "{label:?}");
        }
        for label in [
            "gr\u{200C}ief",
            "grief\u{200D}",
            "\u{200D}grief",
            "می\u{200C}\u{200C}خواهم",
            "\u{200C}می",
            "می\u{200D}",
            "می \u{200C}خواهم",
            "👨\u{200D}\u{200B}👩",
        ] {
            assert_eq!(
                normalize_request(label),
                Err(RequestError::NotALabel),
                "{label:?}"
            );
        }
        assert_eq!(normalize_agent("gr\u{200C}ief"), "gr ief");
        assert_eq!(normalize_agent("Claude\u{200D}Code"), "Claude Code");
        assert_eq!(normalize_agent("grief\u{200D}"), "grief");
        assert_eq!(normalize_agent("\u{200D}grief"), "grief");
        assert_eq!(normalize_agent("می\u{200C}\u{200C}خواهم"), "می  خواهم");
        // Cut short just after a joiner, a name doesn't end in one.
        let long = format!("{}👨\u{200D}👩", "a".repeat(MAX_AGENT_CHARS - 2));
        assert_eq!(
            normalize_agent(&long),
            format!("{}👨", "a".repeat(MAX_AGENT_CHARS - 2))
        );
    }

    #[test]
    fn an_agent_is_trimmed_and_cut_short_or_named_for_it() {
        assert_eq!(normalize_agent("  Claude Code "), "Claude Code");
        assert_eq!(normalize_agent("Claude\u{0}\nCode"), "Claude  Code");
        assert_eq!(normalize_agent(" \n"), DEFAULT_AGENT);
        let long = "a".repeat(MAX_AGENT_CHARS + 20);
        assert_eq!(normalize_agent(&long).chars().count(), MAX_AGENT_CHARS);
        // Cut short at a space, it doesn't end in one.
        let spaced = format!("{} b", "a".repeat(MAX_AGENT_CHARS - 1));
        assert_eq!(normalize_agent(&spaced), "a".repeat(MAX_AGENT_CHARS - 1));
    }

    /// Each refusal says plainly what a request is.
    #[test]
    fn a_refused_request_is_told_it_is_a_short_topic_label() {
        assert_eq!(MAX_REQUEST_CHARS, 60);
        let a_label = "The request is a short topic label, such as \u{201c}grief\u{201d} or \u{201c}the sun\u{201d}";
        assert_eq!(
            RequestError::Empty.to_string(),
            format!("{a_label}; this one is empty.")
        );
        assert_eq!(
            RequestError::TooLong { chars: 75 }.to_string(),
            format!("{a_label}: at most 60 characters, and this one has 75.")
        );
        assert_eq!(
            RequestError::NotALabel.to_string(),
            format!(
                "{a_label}, so it can't have a colon, a semicolon, a dash (\u{2014} or \u{2013}), \
                 a line break or a character that can't be seen."
            )
        );
    }

    /// A label that older versions saved and today's rule refuses comes back
    /// cleaned, one fixture per kind of character the rule came to refuse
    /// (GG32-01, AF1).
    #[test]
    fn a_label_an_older_rule_saved_is_cleaned_not_refused() {
        for (stored, cleaned) in crate::db::test_support::OLD_RULE_REQUESTS {
            assert!(normalize_request(stored).is_err(), "{stored:?}");
            assert_eq!(
                recover_request(stored),
                Recovered {
                    label: cleaned.to_string(),
                    adjusted: true
                },
                "{stored:?}"
            );
        }
    }

    /// A label today's rule accepts is kept exactly, joiners in words and
    /// emoji included, and isn't counted as tidied.
    #[test]
    fn a_label_the_rule_accepts_is_unchanged() {
        for label in [
            "grief",
            "  the sun ",
            "\u{645}\u{6CC}\u{200C}\u{62E}\u{648}\u{627}\u{647}\u{645}",
            "👨\u{200D}👩\u{200D}👧",
            &"\u{1d11e}".repeat(MAX_REQUEST_CHARS),
        ] {
            assert_eq!(
                recover_request(label),
                Recovered {
                    label: label.trim().to_string(),
                    adjusted: false
                }
            );
        }
    }

    /// Whatever a label held, what's left is cleaned of it: the rule's own
    /// punctuation, runs of spaces, the length, a joiner left beside a space
    /// put in, and nothing left at all.
    #[test]
    fn cleaning_keeps_what_the_rule_allows() {
        let tidied = |raw: &str| {
            let recovered = recover_request(raw);
            assert!(recovered.adjusted, "{raw:?}");
            recovered.label
        };
        assert_eq!(tidied("grief: here Jesus wept"), "grief here Jesus wept");
        assert_eq!(tidied("a ;\u{2014}\u{2013} b"), "a b");
        assert_eq!(tidied(&"x".repeat(MAX_REQUEST_CHARS + 5)), "x".repeat(60));
        let long = format!("{}\u{200B}{}", "a".repeat(58), "b".repeat(10));
        assert_eq!(tidied(&long), format!("{} b", "a".repeat(58)));
        // A space at the cut is dropped, not kept at the end.
        let spaced = format!("{}:b", "a".repeat(MAX_REQUEST_CHARS - 1));
        assert_eq!(tidied(&spaced), "a".repeat(MAX_REQUEST_CHARS - 1));
        // The non-joiner joined Persian letter and dash; once the dash is a
        // space, it joins nothing, and goes too.
        assert_eq!(tidied("\u{628}\u{200C}\u{2014}x"), "\u{628} x");
        for nothing in ["", "   ", ":", "\u{200B}", "\u{2014} ; \u{2028}\t"] {
            assert_eq!(tidied(nothing), EARLIER_REQUEST, "{nothing:?}");
        }
        assert_eq!(
            normalize_request(EARLIER_REQUEST).as_deref(),
            Ok(EARLIER_REQUEST)
        );
    }

    /// Whatever a label holds, recovering it gives a label today's rule
    /// accepts as it is, and the same one every time: labels made of the
    /// characters that matter here, at random from a fixed seed.
    #[test]
    fn a_recovered_label_always_passes_the_rule() {
        let palette = [
            'a',
            'Z',
            '7',
            ' ',
            ' ',
            '\t',
            '\n',
            ':',
            ';',
            '\u{2014}',
            '\u{2013}',
            '\u{200B}',
            '\u{200C}',
            '\u{200D}',
            '\u{2060}',
            '\u{FEFF}',
            '\u{202E}',
            '\u{AD}',
            '\u{2028}',
            '\u{2029}',
            '\u{E0041}',
            '\u{A0}',
            '\u{3000}',
            '\u{628}',
            '\u{645}',
            '👨',
            '👩',
            '\u{1d11e}',
            '\u{0301}',
            '\u{7f}',
            '\u{85}',
        ];
        let mut seed: u64 = 0x2026_1119;
        let mut next = |below: usize| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as usize % below
        };
        for _ in 0..20_000 {
            let len = next(90);
            let raw: String = (0..len).map(|_| palette[next(palette.len())]).collect();
            let recovered = recover_request(&raw);
            assert_eq!(
                normalize_request(&recovered.label).as_deref(),
                Ok(recovered.label.as_str()),
                "{raw:?}"
            );
            assert_eq!(
                recovered.adjusted,
                normalize_request(&raw).is_err(),
                "{raw:?}"
            );
            assert_eq!(recover_request(&raw), recovered, "{raw:?}");
        }
    }

    /// The limits here are the ones the table's CHECKs enforce.
    #[test]
    fn the_length_limits_match_the_table() {
        let migrate = include_str!("migrate.rs");
        for check in [
            format!("CHECK (length(agent) BETWEEN 1 AND {MAX_AGENT_CHARS})"),
            format!("CHECK (length(request) BETWEEN 1 AND {MAX_REQUEST_CHARS})"),
        ] {
            assert!(migrate.contains(&check), "{check}");
        }
    }

    #[test]
    fn requests_group_latest_first_with_their_agents_and_verses() {
        let findings = vec![
            finding(1, 33, "grief", "Claude", "2026-11-05T00:00:00.000Z"),
            finding(2, 35, "grief", "Codex", "2026-11-07T00:00:00.000Z"),
            finding(3, 35, "tears", "Claude", "2026-11-06T00:00:00.000Z"),
            finding(4, 37, "grief", "Claude", "2026-11-06T00:00:00.000Z"),
        ];
        let groups = group_requests(findings);
        let requests: Vec<&str> = groups.iter().map(|g| g.request.as_str()).collect();
        assert_eq!(requests, ["grief", "tears"]);
        assert_eq!(groups[0].agents, ["Claude", "Codex"]);
        assert_eq!(groups[0].verse_count, 3);
        assert_eq!(groups[0].latest, "2026-11-07T00:00:00.000Z");
        let ids: Vec<i64> = groups[0].findings.iter().map(|f| f.id).collect();
        assert_eq!(ids, [1, 2, 4]);
        assert_eq!(groups[1].verse_count, 1);
    }

    fn range(id: i64, at: (i64, i64), verses: (i64, i64), request: &str) -> Finding {
        Finding {
            book_id: at.0,
            chapter: at.1,
            verse_start: verses.0,
            verse_end: verses.1,
            ..finding(id, verses.0, request, "Claude", "2026-11-07T00:00:00.000Z")
        }
    }

    /// A verse two of a request's findings mark is counted once, and every
    /// finding is kept, each with its own range.
    #[test]
    fn a_request_counts_each_verse_once_however_many_findings_mark_it() {
        let findings = vec![
            range(1, (43, 11), (33, 35), "grief"),
            range(2, (43, 11), (35, 35), "grief"),
            range(3, (43, 11), (34, 36), "grief"),
            range(4, (43, 11), (40, 41), "grief"),
            range(5, (43, 11), (33, 35), "tears"),
        ];
        let groups = group_requests(findings.clone());
        let counts: Vec<(&str, i64)> = groups
            .iter()
            .map(|g| (g.request.as_str(), g.verse_count))
            .collect();
        // 33-36 and 40-41: six verses, not 3 + 1 + 3 + 2.
        assert_eq!(counts, [("grief", 6), ("tears", 3)]);
        let kept: Vec<&Finding> = groups.iter().flat_map(|g| &g.findings).collect();
        assert_eq!(kept.len(), findings.len(), "every finding is kept");
        for f in &findings {
            assert!(kept.contains(&f), "{f:?}");
        }
    }

    /// The same verse numbers in another chapter, or another book, are
    /// other verses.
    #[test]
    fn the_same_verse_number_elsewhere_is_another_verse() {
        let groups = group_requests(vec![
            range(1, (43, 11), (35, 35), "grief"),
            range(2, (43, 12), (35, 35), "grief"),
            range(3, (42, 11), (35, 35), "grief"),
            range(4, (42, 11), (34, 35), "grief"),
            range(5, (1, 1), (1, 3), "grief"),
            range(6, (1, 1), (4, 5), "grief"),
        ]);
        // John 11:35, John 12:35, Luke 11:34-35, Genesis 1:1-5.
        assert_eq!(groups[0].verse_count, 1 + 1 + 2 + 5);
    }

    /// Requests are told apart exactly as they were saved, as the table's
    /// uniqueness tells them apart.
    #[test]
    fn requests_differing_only_in_case_are_two_requests() {
        let groups = group_requests(vec![
            range(1, (43, 11), (35, 35), "grief"),
            range(2, (43, 11), (35, 35), "Grief"),
        ]);
        let requests: Vec<(&str, i64, usize)> = groups
            .iter()
            .map(|g| (g.request.as_str(), g.verse_count, g.findings.len()))
            .collect();
        assert_eq!(requests, [("Grief", 1, 1), ("grief", 1, 1)]);
    }
}
