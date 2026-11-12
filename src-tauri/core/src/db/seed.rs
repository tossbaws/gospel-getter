//! The bundled translations and cross-references, embedded in the binary,
//! and seeding the database with whichever of them it doesn't have yet.

use anyhow::Context;
use serde::Deserialize;

use crate::db::WritePool;

/// How the app lists one bundled translation (its text is in `TEXTS`).
pub struct TranslationSource {
    pub code: &'static str,
    pub name: &'static str,
    /// When it was made, as the chooser shows it ("1611/1769"); `None` for
    /// one still being revised (the WEB).
    pub date: Option<&'static str>,
    /// Its licence, as the chooser shows it.
    pub licence: &'static str,
    /// Where it comes from, as About credits it.
    pub credit: &'static str,
}

/// The list of bundled translations, in the order Aa lists them. **To add
/// a new one**: drop a JSON file into `data/` shaped like the existing ones
/// (an array of `{"name", "testament", "chapters"}` objects, one per book,
/// in canonical Genesis-to-Revelation order — see `data/kjv.json`; a verse
/// the translation doesn't number is `null` (or `""`, see `numbered`),
/// and footnotes kept apart from
/// the text go in each book's `footnotes`, as in `data/bsb.json`), then add
/// one entry here and one in `TEXTS`. Only include translations you have the right to
/// redistribute (public domain, or under a license that permits
/// bulk/offline copies) — see the README for why NIV and ESV aren't bundled
/// this way.
///
/// They're seeded in `SEED_ORDER`, which never changes, so a translation's
/// database id is the same on every install, new or upgraded. A change to
/// one that installs have already seeded is a step in `DATA_STEPS`.
pub const TRANSLATIONS: &[TranslationSource] = &[
    TranslationSource {
        code: "kjv",
        name: "King James Version",
        date: Some("1611/1769"),
        licence: "public domain",
        // Ten chapters (and Revelation 13:1) from eBible.org's USFM, since
        // 2.29.2 (`sources/kjv`), the rest from thiagobodruk/bible.
        credit: "King James Version (KJV), public domain, from the thiagobodruk/bible dataset \
                 and eBible.org",
    },
    TranslationSource {
        code: "bsb",
        name: "Berean Standard Bible",
        date: Some("2023"),
        licence: "public domain",
        credit: "Berean Standard Bible (BSB), public domain, berean.bible",
    },
    TranslationSource {
        code: "web",
        name: "World English Bible",
        date: None,
        licence: "public domain",
        credit: "World English Bible (WEB), public domain, from TehShrike/world-english-bible (via ebible.org)",
    },
];

/// Each bundled translation's raw text, embedded into the binary so the app
/// is fully self-contained and works offline. Apart from [`TRANSLATIONS`], so
/// that a program that only lists them (`gospel-getter-cli`) doesn't carry
/// their text: the linker keeps only what's used. A `static`, not a `const`: a
/// const's data is copied into every crate that instantiates what uses it, and
/// `seed_missing` is instantiated in the app too, which doubled its size.
static TEXTS: [(&str, &str); 3] = [
    ("kjv", include_str!("../../../data/kjv.json")),
    ("bsb", include_str!("../../../data/bsb.json")),
    ("web", include_str!("../../../data/web.json")),
];

/// The order translations are seeded in: each new one at the end.
const SEED_ORDER: [&str; 3] = ["kjv", "web", "bsb"];

/// Where `code` comes in [`TRANSLATIONS`] (unknown codes last).
pub fn rank(code: &str) -> usize {
    TRANSLATIONS
        .iter()
        .position(|t| t.code == code)
        .unwrap_or(TRANSLATIONS.len())
}

/// The bundled translation `code`.
pub fn source(code: &str) -> Option<&'static TranslationSource> {
    TRANSLATIONS.iter().find(|t| t.code == code)
}

/// A verse's text as its bundled file has it, or `None` for a verse the
/// translation doesn't number: `null` there (the BSB's 16), or no text at
/// all. The WEB's file has five verses with no text, ones the WEB doesn't
/// number (Luke 17:36, Acts 8:37, 15:34, 24:7, Romans 16:25): each is an
/// empty string where the other translations number a verse, and is read
/// the same as `null`. A verse with any text is the translation's,
/// character for character.
fn numbered(text: &Option<String>) -> Option<&str> {
    text.as_deref().filter(|text| !text.is_empty())
}

#[derive(Deserialize)]
struct RawBook {
    name: String,
    testament: String,
    /// `None` (or empty, see [`numbered`]) for a verse the translation
    /// doesn't number.
    chapters: Vec<Vec<Option<String>>>,
    #[serde(default)]
    footnotes: Vec<RawFootnote>,
}

/// A footnote kept apart from its verse's text (see `domain::usfm_note`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFootnote {
    chapter: i64,
    verse: i64,
    at: i64,
    usfm: String,
}

/// Populate `translations`, `books`, and `verses` from the bundled
/// translations — but only whatever is actually missing. This is what
/// makes the app self-initializing on a fresh checkout *and* lets an
/// already-running install pick up a newly-added translation after a
/// rebuild: each translation in `TRANSLATIONS` is seeded independently, on
/// its own presence check (`SELECT id FROM translations WHERE code = ...`),
/// not gated by a single "is the database empty" flag. This only fills in
/// what's absent: a change to a translation that's already seeded is a
/// step in `DATA_STEPS`, which [`upgrade_bundled_data`] makes. Each
/// translation seeded here is recorded at its latest data version, past
/// every step.
pub async fn seed_missing(writer: &WritePool) -> anyhow::Result<()> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start seed transaction")?;

    // Book structure (names, testament, chapter counts) is shared across
    // translations, so it's only seeded once, from whichever translation
    // seeds first.
    let (book_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM books")
        .fetch_one(&mut *tx)
        .await
        .context("Failed to check book count")?;
    let mut books_seeded = book_count > 0;

    let (max_translation_id,): (Option<i64>,) = sqlx::query_as("SELECT MAX(id) FROM translations")
        .fetch_one(&mut *tx)
        .await
        .context("Failed to check existing translations")?;
    let mut next_translation_id = max_translation_id.unwrap_or(0) + 1;

    for code in SEED_ORDER {
        let source = source(code).expect("every seeded translation is listed");
        let already_seeded: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM translations WHERE code = $1")
                .bind(source.code)
                .fetch_optional(&mut *tx)
                .await
                .context(format!("Failed to check translation {}", source.code))?;

        if already_seeded.is_some() {
            continue;
        }

        let translation_id = next_translation_id;
        next_translation_id += 1;

        sqlx::query("INSERT INTO translations (id, code, name) VALUES ($1, $2, $3)")
            .bind(translation_id)
            .bind(source.code)
            .bind(source.name)
            .execute(&mut *tx)
            .await
            .context(format!("Failed to insert translation {}", source.code))?;

        // Whatever the search index held for this id (from a translation
        // since cleared out) is stale now; `ensure_search_index` rebuilds it.
        sqlx::query("DELETE FROM search_index_state WHERE translation_id = $1")
            .bind(translation_id)
            .execute(&mut *tx)
            .await
            .context(format!(
                "Failed to reset search index state for {}",
                source.code
            ))?;

        // Seeded as bundled now: past every step (any record left for this
        // id by a translation since cleared out is replaced).
        record_data_version(&mut tx, translation_id, data_version(source.code))
            .await
            .context(format!(
                "Failed to record the data version of {}",
                source.code
            ))?;

        let books = bundled_books(source.code)?;
        if !books_seeded {
            for (book_index, book) in books.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO books (id, name, testament, chapter_count) VALUES ($1, $2, $3, $4)",
                )
                .bind(book_index as i64 + 1)
                .bind(&book.name)
                .bind(&book.testament)
                .bind(book.chapters.len() as i64)
                .execute(&mut *tx)
                .await
                .context(format!("Failed to insert book {}", book.name))?;
            }
        }
        insert_bundled_rows(&mut tx, translation_id, source.code, &books).await?;

        books_seeded = true;
        tracing::info!("Seeded {} ({} books)", source.name, books.len());
    }

    tx.commit()
        .await
        .context("Failed to commit seed transaction")?;

    Ok(())
}

/// The bundled translation `code`, parsed.
fn bundled_books(code: &str) -> anyhow::Result<Vec<RawBook>> {
    let (_, text) = TEXTS
        .iter()
        .find(|(listed, _)| *listed == code)
        .with_context(|| format!("No bundled text for {code}"))?;
    serde_json::from_str(text).context(format!("Failed to parse bundled {code} data"))
}

/// Insert `books`, the bundled translation `code`, as translation
/// `translation_id`'s rows: every verse it numbers, then its footnotes,
/// numbered in order within each verse (any left by a translation since
/// cleared out are gone first). How [`seed_missing`] seeds a translation,
/// and how a [`DataStep::Reseed`] seeds it again. Returns how many verses
/// it inserted.
async fn insert_bundled_rows(
    conn: &mut sqlx::SqliteConnection,
    translation_id: i64,
    code: &str,
    books: &[RawBook],
) -> anyhow::Result<usize> {
    let mut inserted = 0;
    for (book_index, book) in books.iter().enumerate() {
        let book_id = book_index as i64 + 1;
        for (chapter_index, verses) in book.chapters.iter().enumerate() {
            let chapter = chapter_index as i64 + 1;
            for (verse_index, text) in verses.iter().enumerate() {
                let verse = verse_index as i64 + 1;
                let Some(text) = numbered(text) else { continue };
                sqlx::query(
                    "INSERT INTO verses (translation_id, book_id, chapter, verse, text) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(translation_id)
                .bind(book_id)
                .bind(chapter)
                .bind(verse)
                .bind(text)
                .execute(&mut *conn)
                .await
                .context(format!(
                    "Failed to insert {code} {} {chapter}:{verse}",
                    book.name
                ))?;
                inserted += 1;
            }
        }
    }

    sqlx::query("DELETE FROM footnotes WHERE translation_id = $1")
        .bind(translation_id)
        .execute(&mut *conn)
        .await
        .context(format!("Failed to clear footnotes for {code}"))?;
    for (book_index, book) in books.iter().enumerate() {
        let mut ordinal = 0;
        let mut last = (0, 0);
        for note in &book.footnotes {
            ordinal = if (note.chapter, note.verse) == last {
                ordinal + 1
            } else {
                0
            };
            last = (note.chapter, note.verse);
            sqlx::query(
                "INSERT INTO footnotes \
                 (translation_id, book_id, chapter, verse, ordinal, position, usfm) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(translation_id)
            .bind(book_index as i64 + 1)
            .bind(note.chapter)
            .bind(note.verse)
            .bind(ordinal)
            .bind(note.at)
            .bind(&note.usfm)
            .execute(&mut *conn)
            .await
            .context(format!(
                "Failed to insert a footnote on {code} {} {}:{}",
                book.name, note.chapter, note.verse
            ))?;
        }
    }
    Ok(inserted)
}

/// A change to a bundled translation's rows that an install seeded before
/// the change needs, made there once, on the first launch after upgrading
/// ([`upgrade_bundled_data`]). A new install never makes it: it seeds the
/// translation as bundled now, past every step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DataStep {
    /// Take out the verses stored with no text: ones the translation
    /// doesn't number, which [`seed_missing`] no longer stores (see
    /// [`numbered`]). Their search index rows go with them. Nothing else
    /// is touched: the reader's records on those verses keep their
    /// coordinates, as they do on any verse a translation doesn't number.
    RemoveUnnumbered,
    /// Seed the translation again: clear its verses, footnotes and search
    /// index record, and insert its bundled rows as [`seed_missing`] does
    /// (`ensure_search_index` then indexes it again). Its id, its name and
    /// the books are kept, and nothing of the reader's is touched: every
    /// record is keyed by coordinates, never by a verse's row, so each
    /// stays where it was, beside the verse now numbered there.
    Reseed,
}

/// Each bundled translation's steps, in order. A translation's rows as
/// first seeded (by any version of the app before its first step) are its
/// data version 1, and its step n brings them from version n to n + 1;
/// [`seed_missing`] records a translation it seeds at version (steps + 1),
/// its latest. A translation not listed has no steps: version 1 is its
/// latest.
///
/// A step is added at the end of its translation's list, never anywhere
/// else, and never changed once released. Re-seeding one (the KJV's text
/// replaced by a better source, say) is a step like any other: one that
/// clears its verses, footnotes and search index record and inserts its
/// bundled rows as `seed_missing` does, in the same transaction.
const DATA_STEPS: &[(&str, &[DataStep])] = &[
    // 2.29.1: the WEB's five verses with no text (Luke 17:36, Acts 8:37,
    // 15:34, 24:7, Romans 16:25), seeded until then as empty verses.
    ("web", &[DataStep::RemoveUnnumbered]),
    // 2.29.2: the KJV's ten chapters its source numbered wrongly (1 Samuel
    // 20, 1 Kings 22, Matthew 2, 22, 26, Mark 4, 7, 8, 3 John, Revelation
    // 12) and Revelation 13:1, taken from eBible.org's USFM instead
    // (`sources/kjv`). Every other verse is as before.
    ("kjv", &[DataStep::Reseed]),
];

/// The verses a bundled translation numbered before one of its
/// `DATA_STEPS` took them away, and that none numbers now, as (book,
/// chapter, verse): the last verses of the four chapters 2.29.2's KJV step
/// made a verse shorter, where its earlier source had split a verse in two
/// (1 Samuel 20:43, 1 Kings 22:54, 3 John 1:15, Revelation 12:18). A record
/// of the reader's there is still one: it can be saved again (an undo, a
/// backup imported) as it could before, and is shown as on any verse a
/// translation doesn't number. Nothing is ever taken off this list.
pub const RETIRED_VERSES: [(i64, i64, i64); 4] =
    [(9, 20, 43), (11, 22, 54), (64, 1, 15), (66, 12, 18)];

/// The last of [`RETIRED_VERSES`] in `book_id`'s `chapter`, if it has one.
pub fn last_retired_verse(book_id: i64, chapter: i64) -> Option<i64> {
    RETIRED_VERSES
        .iter()
        .filter(|&&(b, c, _)| (b, c) == (book_id, chapter))
        .map(|&(_, _, v)| v)
        .max()
}

/// The steps of the translation `code`.
fn data_steps(code: &str) -> &'static [DataStep] {
    DATA_STEPS
        .iter()
        .find(|(listed, _)| *listed == code)
        .map_or(&[], |(_, steps)| steps)
}

/// The latest data version of the translation `code`: past all its steps.
fn data_version(code: &str) -> i64 {
    data_steps(code).len() as i64 + 1
}

/// Record that the rows of translation `translation_id` are its data
/// version `version`.
async fn record_data_version(
    conn: &mut sqlx::SqliteConnection,
    translation_id: i64,
    version: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO translation_data_version (translation_id, version) VALUES ($1, $2) \
         ON CONFLICT (translation_id) DO UPDATE SET version = excluded.version",
    )
    .bind(translation_id)
    .bind(version)
    .execute(conn)
    .await?;
    Ok(())
}

/// A translation whose rows aren't its latest data version yet: its id and
/// code, and the version they are.
#[derive(Debug)]
struct Behind {
    translation_id: i64,
    code: String,
    version: i64,
}

/// The seeded translations whose rows are behind their latest data version.
/// Rows with no version recorded were seeded before 2.29.1, version 1. Rows
/// past it (a newer version of the app has made a step this one doesn't
/// know) are left as they are, as `migrate` leaves a newer schema.
async fn behind(
    conn: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
) -> anyhow::Result<Vec<Behind>> {
    let seeded: Vec<(i64, String, i64)> = sqlx::query_as(
        "SELECT t.id, t.code, COALESCE(d.version, 1) FROM translations t \
         LEFT JOIN translation_data_version d ON d.translation_id = t.id ORDER BY t.id",
    )
    .fetch_all(conn)
    .await
    .context("Failed to read the translations' data versions")?;
    Ok(seeded
        .into_iter()
        .filter(|(_, code, version)| *version < data_version(code))
        .map(|(translation_id, code, version)| Behind {
            translation_id,
            code,
            version,
        })
        .collect())
}

/// Bring every seeded translation's rows to its latest data version, making
/// the steps in `DATA_STEPS` it hasn't had yet, and say which translations
/// it brought up to which version: none at all on a normal launch.
///
/// A quick look first, so a normal launch takes no lock. Then every step,
/// and the versions they reach, in one transaction that starts with the
/// write lock (`begin_write`) and looks again once it's held: another copy
/// of the app may have made them since. It's made whole or not at all: if
/// anything fails, every row is as it was, the version too, and the next
/// launch tries again. No table of the reader's (notes, bookmarks,
/// highlights, findings, the reading place, settings) is touched.
pub async fn upgrade_bundled_data(writer: &WritePool) -> anyhow::Result<Vec<(String, i64)>> {
    if behind(writer.pool()).await?.is_empty() {
        return Ok(Vec::new());
    }
    let started = std::time::Instant::now();
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start the bundled data upgrade transaction")?;
    let mut upgraded = Vec::new();
    let mut made = Vec::new();
    for translation in behind(&mut *tx).await? {
        let steps = data_steps(&translation.code);
        let from = usize::try_from(translation.version - 1).unwrap_or(0);
        for (index, step) in steps.iter().enumerate().skip(from) {
            let what = make_step(&mut tx, &translation, *step)
                .await
                .with_context(|| {
                    format!(
                        "Failed to bring {} to data version {}",
                        translation.code,
                        index + 2
                    )
                })?;
            made.push(what);
        }
        let version = data_version(&translation.code);
        record_data_version(&mut tx, translation.translation_id, version)
            .await
            .with_context(|| {
                format!("Failed to record the data version of {}", translation.code)
            })?;
        upgraded.push((translation.code, version));
    }
    tx.commit()
        .await
        .context("Failed to commit the bundled data upgrade")?;
    for what in &made {
        tracing::info!("{what}");
    }
    for (code, version) in &upgraded {
        tracing::info!("Brought {code}'s stored data to version {version}");
    }
    tracing::info!("The bundled data upgrade took {:.0?}", started.elapsed());
    Ok(upgraded)
}

/// Make one step on `translation`'s rows, in the upgrade's transaction, and
/// say what it did, for the log once the transaction is committed.
async fn make_step(
    conn: &mut sqlx::SqliteConnection,
    translation: &Behind,
    step: DataStep,
) -> anyhow::Result<String> {
    match step {
        DataStep::RemoveUnnumbered => {
            let unnumbered: Vec<(i64, i64, i64)> = sqlx::query_as(
                "SELECT book_id, chapter, verse FROM verses \
                 WHERE translation_id = $1 AND text = '' ORDER BY book_id, chapter, verse",
            )
            .bind(translation.translation_id)
            .fetch_all(&mut *conn)
            .await
            .context("Failed to find the verses stored with no text")?;
            crate::db::search::forget_verses(conn, translation.translation_id, &unnumbered).await?;
            let removed = sqlx::query("DELETE FROM verses WHERE translation_id = $1 AND text = ''")
                .bind(translation.translation_id)
                .execute(&mut *conn)
                .await
                .context("Failed to remove the verses stored with no text")?
                .rows_affected();
            anyhow::ensure!(
                usize::try_from(removed).is_ok_and(|removed| removed == unnumbered.len()),
                "Removed {removed} verses with no text, not the {} found",
                unnumbered.len()
            );
            Ok(format!(
                "Took out {removed} verses of {} stored with no text, which it doesn't \
                 number: (book, chapter, verse) {unnumbered:?}",
                translation.code
            ))
        }
        DataStep::Reseed => {
            let books = bundled_books(&translation.code)?;
            let removed = sqlx::query("DELETE FROM verses WHERE translation_id = $1")
                .bind(translation.translation_id)
                .execute(&mut *conn)
                .await
                .context("Failed to clear the verses")?
                .rows_affected();
            // Its footnotes are cleared as its rows are inserted.
            sqlx::query("DELETE FROM search_index_state WHERE translation_id = $1")
                .bind(translation.translation_id)
                .execute(&mut *conn)
                .await
                .context("Failed to reset the search index state")?;
            let inserted =
                insert_bundled_rows(conn, translation.translation_id, &translation.code, &books)
                    .await?;
            Ok(format!(
                "Seeded {} again from its bundled text: {removed} verses replaced by \
                 {inserted}; its search index is built again",
                translation.code
            ))
        }
    }
}

const CROSS_REFERENCES_JSON: &str = include_str!("../../../data/cross_references.json");

#[derive(Deserialize)]
struct RawCrossRefBook {
    #[allow(dead_code)] // provenance only, not used at runtime
    code: String,
    chapters: Vec<Vec<Vec<RawCrossRef>>>,
}

#[derive(Deserialize)]
struct RawCrossRef {
    book: i64,
    chapter: i64,
    verse: i64,
    #[serde(rename = "endVerse")]
    end_verse: Option<i64>,
    score: i64,
}

/// Populate `cross_references` from the bundled dataset, but only if it's
/// currently empty. A quick look first, so a normal launch doesn't parse
/// the dataset; then `seed_cross_references`, which looks again with the
/// write lock held, so two copies of the app starting at once (both finding
/// it empty here) seed it once between them.
pub async fn seed_cross_references_if_empty(writer: &WritePool) -> anyhow::Result<()> {
    if cross_references_present(writer.pool()).await? {
        return Ok(());
    }
    let books = bundled_cross_references()?;
    seed_cross_references(writer, &books).await?;
    Ok(())
}

/// Whether `cross_references` has any rows.
async fn cross_references_present(
    conn: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
) -> anyhow::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM cross_references)")
        .fetch_one(conn)
        .await
        .context("Failed to check for cross-references")
}

/// The bundled cross-reference dataset, parsed.
fn bundled_cross_references() -> anyhow::Result<Vec<RawCrossRefBook>> {
    serde_json::from_str(CROSS_REFERENCES_JSON)
        .context("Failed to parse bundled cross-reference data")
}

/// Insert `books`' cross-references, in one transaction that starts with the
/// write lock, only if the table is still empty once that's held: another
/// copy of the app may have seeded it since this one looked. Whether this
/// seeded it.
async fn seed_cross_references(
    writer: &WritePool,
    books: &[RawCrossRefBook],
) -> anyhow::Result<bool> {
    let mut tx = crate::db::begin_write(writer)
        .await
        .context("Failed to start cross-reference seed transaction")?;
    if cross_references_present(&mut *tx).await? {
        return Ok(false);
    }

    for (book_index, book) in books.iter().enumerate() {
        let book_id = book_index as i64 + 1;
        for (chapter_index, verses) in book.chapters.iter().enumerate() {
            let chapter = chapter_index as i64 + 1;
            for (verse_index, refs) in verses.iter().enumerate() {
                let verse = verse_index as i64 + 1;
                for r in refs {
                    sqlx::query(
                        "INSERT INTO cross_references \
                         (book_id, chapter, verse, ref_book_id, ref_chapter, ref_verse, ref_end_verse, score) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    )
                    .bind(book_id)
                    .bind(chapter)
                    .bind(verse)
                    .bind(r.book)
                    .bind(r.chapter)
                    .bind(r.verse)
                    .bind(r.end_verse)
                    .bind(r.score)
                    .execute(&mut *tx)
                    .await
                    .context("Failed to insert cross reference")?;
                }
            }
        }
    }

    tx.commit()
        .await
        .context("Failed to commit cross-reference seed transaction")?;
    tracing::info!("Seeded cross-reference data");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_support::prepared_database;

    async fn count(writer: &WritePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM cross_references")
            .fetch_one(writer.pool())
            .await
            .expect("count")
    }

    /// Two copies of the app start at once on a database with no
    /// cross-references: each looks and finds none, then each seeds as the
    /// write lock lets it, one after the other. One dataset results, as
    /// one seed makes it.
    #[tokio::test]
    async fn two_seeders_both_past_their_first_look_seed_the_cross_references_once() {
        let (db, one) = prepared_database("xref_two_seeders").await;
        let one_dataset = count(one.write()).await;
        assert!(one_dataset > 0);
        sqlx::query("DELETE FROM cross_references")
            .execute(one.write().pool())
            .await
            .expect("empty it, as on a first launch");
        let two = db.connect().await;

        assert!(!cross_references_present(one.write().pool()).await.unwrap());
        assert!(!cross_references_present(two.write().pool()).await.unwrap());
        let books = bundled_cross_references().unwrap();

        assert!(seed_cross_references(one.write(), &books).await.unwrap());
        assert!(!seed_cross_references(two.write(), &books).await.unwrap());
        assert_eq!(count(one.write()).await, one_dataset);
        assert_eq!(count(two.write()).await, one_dataset);

        // And a later launch's whole step does nothing.
        seed_cross_references_if_empty(two.write()).await.unwrap();
        assert_eq!(count(one.write()).await, one_dataset);
        two.close().await;
        one.close().await;
    }
}
