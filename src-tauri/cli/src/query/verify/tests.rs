//! `verify_quote` over a real prepared database, and its comparison on its own.
//! The real text comes from `get_passage`, which `query::tests` compares with
//! the bundled text, so a quote here is never a retyped copy of a verse.

use gospel_getter_core::db::test_support::{TempDb, bundled, fresh_database, prepared_database};
use gospel_getter_core::db::{self, Db};
use serde_json::{Value, json};

use super::*;

/// A prepared database, and the one open library the checks read it through.
struct Install {
    _db: TempDb,
    pool: Db,
    library: Library,
}

impl Install {
    async fn indexed(label: &str) -> Self {
        let (db, pool) = fresh_database(label).await;
        let library = Library::open(&db.path()).await.expect("the install");
        Self {
            _db: db,
            pool,
            library,
        }
    }

    async fn close(self) {
        self.library.close().await;
        self.pool.close().await;
    }

    async fn answer(
        &self,
        quote: &str,
        reference: Option<&str>,
        translation: &str,
    ) -> anyhow::Result<Value> {
        let args = VerifyQuoteArgs {
            text: quote.to_string(),
            reference: reference.map(str::to_string),
            translation: Some(translation.to_string()),
        };
        let out = self.library.verify_quote(args).await?;
        Ok(serde_json::to_value(out).expect("JSON"))
    }

    async fn check(&self, quote: &str, reference: Option<&str>, translation: &str) -> Value {
        self.answer(quote, reference, translation)
            .await
            .expect("an answer")
    }

    async fn refused(&self, quote: &str, reference: Option<&str>, translation: &str) -> String {
        let refusal = self.answer(quote, reference, translation).await;
        format!("{:#}", refusal.expect_err("a refusal"))
    }

    /// What `get_passage` gives for a reference, its verses joined by one space.
    async fn real_text(&self, reference: &str, translation: &str) -> String {
        let translation = self.library.translation(Some(translation)).await;
        let translation = translation.expect("a translation");
        let passage = self.library.passage_in(translation, reference).await;
        let passage = passage.expect("a passage");
        let verses: Vec<&str> = passage.verses.iter().map(|v| v.text.as_str()).collect();
        verses.join(" ")
    }

    /// The core promise, for any quote at all: whatever the answer says about
    /// the text, the characters bear out, and what each reference names is the
    /// text given with it.
    async fn assert_it_keeps_its_promise(&self, quote: &str, answer: &Value) {
        let trimmed = quote.trim();
        let code = answer["translation"].as_str().expect("a translation");
        let text = answer["text"].as_str().unwrap_or_default();
        match answer["status"].as_str().expect("a status") {
            "exact" => assert_eq!(text, trimmed, "{answer}"),
            "exact_part" => assert!(text.contains(trimmed) && text != trimmed, "{answer}"),
            "differs" => assert!(!text.contains(trimmed), "{answer}"),
            "elsewhere" => {
                assert!(!text.contains(trimmed), "{answer}");
                assert!(answer["found_at"].as_array().is_some_and(|p| !p.is_empty()));
            }
            "not_found" => {
                assert!(answer.get("reference").is_none(), "{answer}");
                assert!(answer.get("text").is_none(), "{answer}");
                assert!(answer.get("found_at").is_none(), "{answer}");
                return;
            }
            status => panic!("no such status: {status}"),
        }
        let named = answer["reference"].as_str().expect("a reference");
        assert_eq!(text, self.real_text(named, code).await, "{answer}");
        for place in answer["found_at"].as_array().into_iter().flatten() {
            let there = place["text"].as_str().expect("a place's text");
            let named = place["reference"].as_str().expect("a place's reference");
            assert!(there.contains(trimmed), "{answer}");
            assert_eq!(there, self.real_text(named, code).await, "{answer}");
        }
    }
}

// ---- Each status.

/// The whole verse, and the whole of a passage of several, its verses joined
/// by one space: exact.
#[tokio::test]
async fn the_whole_passage_is_exact() {
    let install = Install::indexed("verify_exact").await;
    let verse = install.real_text("John 3:16", "kjv").await;
    assert_eq!(
        install.check(&verse, Some("John 3:16"), "kjv").await,
        json!({
            "status": "exact",
            "translation": "kjv",
            "reference": "John 3:16",
            "text": verse,
            "message": "This is exactly John 3:16 in the KJV.",
        })
    );

    let next = install.real_text("John 3:17", "kjv").await;
    let both = format!("{verse} {next}");
    let answer = install.check(&both, Some("jn 3:16-17"), "kjv").await;
    assert_eq!(answer["status"], "exact");
    assert_eq!(answer["reference"], "John 3:16\u{2013}17");
    assert_eq!(answer["text"], both.as_str());

    // Joined any other way, it isn't the text.
    let length = verse.chars().count();
    for (joined, position) in [
        (format!("{verse}  {next}"), length + 2),
        (format!("{verse}\n{next}"), length + 1),
    ] {
        let answer = install.check(&joined, Some("John 3:16-17"), "kjv").await;
        assert_eq!(answer["status"], "differs");
        assert_eq!(answer["difference"]["position"], position);
    }
    install.close().await;
}

/// A fragment of a verse, and a stretch across verses inside the passage.
#[tokio::test]
async fn a_contiguous_part_is_exact_part() {
    let install = Install::indexed("verify_part").await;
    let fragment = "For God so loved the world";
    let answer = install.check(fragment, Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "exact_part");
    assert_eq!(
        answer["text"],
        install.real_text("John 3:16", "kjv").await.as_str()
    );
    assert_eq!(
        answer["message"],
        "This is an exact part of John 3:16 in the KJV."
    );
    assert!(answer.get("difference").is_none());

    let across = "everlasting life. For God sent not his Son";
    let answer = install.check(across, Some("John 3:16-18"), "kjv").await;
    assert_eq!(answer["status"], "exact_part");
    assert_eq!(answer["reference"], "John 3:16\u{2013}18");
    install.close().await;
}

/// One wrong letter: where it is, in characters and words, what was quoted
/// from there, and what the text has from there, with the whole real text.
#[tokio::test]
async fn one_changed_letter_differs_and_says_where() {
    let install = Install::indexed("verify_letter").await;
    let verse = install.real_text("John 3:16", "kjv").await;
    let quote = verse.replacen("loved", "lovad", 1);
    let answer = install.check(&quote, Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(answer["reference"], "John 3:16");
    assert_eq!(answer["text"], verse.as_str());
    assert_eq!(
        answer["difference"],
        json!({
            "position": 15,
            "word": 4,
            "quoted": "ad the world, that he gave his only bego",
            "actual": "ed the world, that he gave his only bego",
        })
    );
    assert_eq!(
        answer["message"],
        "This differs from John 3:16 in the KJV at word 4, character 15."
    );
    assert!(answer.get("found_at").is_none());
    install.close().await;
}

/// Case, a missing comma, an extra word, a doubled or otherwise different
/// space: each is a difference, at the right place. The wrong apostrophe too.
#[tokio::test]
async fn every_kind_of_small_change_differs() {
    let install = Install::indexed("verify_small").await;
    for (quote, position, word) in [
        ("For god so loved the world", 5, 2),
        ("FOR GOD SO LOVED", 2, 1),
        ("For God so loved the world that", 27, 6),
        ("For God so dearly loved the world", 12, 4),
        ("For God so  loved the world", 12, 3),
        ("For God\nso loved the world", 8, 2),
    ] {
        let answer = install.check(quote, Some("John 3:16"), "kjv").await;
        assert_eq!(answer["status"], "differs", "{quote}: {answer}");
        assert_eq!(answer["difference"]["position"], position, "{quote}");
        assert_eq!(answer["difference"]["word"], word, "{quote}");
        install.assert_it_keeps_its_promise(quote, &answer).await;
    }
    // The first letter wrong: no alignment starts at it, so the first difference
    // is against the best the rule finds. Still only ever a difference.
    let answer = install
        .check("for God so loved the world", Some("John 3:16"), "kjv")
        .await;
    assert_eq!(answer["status"], "differs", "{answer}");

    // The BSB's apostrophes are curly: a straight one isn't the same character.
    let verse = install.real_text("Luke 2:49", "bsb").await;
    let straight = verse.replace('\u{2019}', "'");
    assert_ne!(straight, verse);
    let answer = install.check(&straight, Some("Luke 2:49"), "bsb").await;
    assert_eq!(answer["status"], "differs", "{answer}");
    install.close().await;
}

/// Longer than the text: the difference is where the text ends.
#[tokio::test]
async fn a_quote_running_past_the_text_differs_at_its_end() {
    let install = Install::indexed("verify_longer").await;
    let answer = install
        .check("Jesus wept. And so", Some("John 11:35"), "kjv")
        .await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(
        answer["difference"],
        json!({"position": 12, "word": 2, "quoted": " And so", "actual": ""})
    );
    install.close().await;
}

/// Verbatim in another verse than the one named: where it is, and the
/// difference against the one named, whose text it stays.
#[tokio::test]
async fn the_wrong_reference_finds_it_elsewhere() {
    let install = Install::indexed("verify_elsewhere").await;
    let answer = install.check("Jesus wept.", Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "elsewhere");
    assert_eq!(answer["reference"], "John 3:16");
    assert_eq!(
        answer["text"],
        install.real_text("John 3:16", "kjv").await.as_str()
    );
    assert_eq!(
        answer["found_at"],
        json!([{"reference": "John 11:35", "text": "Jesus wept."}])
    );
    assert_eq!(answer["difference"]["position"], 1);
    assert_eq!(
        answer["message"],
        "This isn't in John 3:16 in the KJV as quoted, but it is in John 11:35."
    );
    install
        .assert_it_keeps_its_promise("Jesus wept.", &answer)
        .await;

    // A quote running on into the next verse isn't in the first verse alone.
    let both = install.real_text("John 3:16-17", "kjv").await;
    let answer = install.check(&both, Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "elsewhere");
    assert_eq!(answer["found_at"][0]["reference"], "John 3:16\u{2013}17");
    assert_eq!(answer["found_at"][0]["text"], both.as_str());
    install.close().await;
}

/// Invented words: said plainly, with no reference and no guess. Given a
/// reference, the passage is known, so it differs instead.
#[tokio::test]
async fn invented_text_is_not_found() {
    let install = Install::indexed("verify_invented").await;
    let invented = "Blessed are the pickle makers, for they shall inherit the jar.";
    for (code, label) in [("kjv", "KJV"), ("bsb", "BSB"), ("web", "WEB")] {
        assert_eq!(
            install.check(invented, None, code).await,
            json!({
                "status": "not_found",
                "translation": code,
                "message": format!("This text isn't in the {label} as quoted."),
            })
        );
    }
    // Short, and of words common enough: as plain, since what's rare in it
    // would have led to the verse.
    for invented in [
        "the LORD loves pickles",
        "God helps those who help themselves",
    ] {
        let answer = install.check(invented, None, "kjv").await;
        assert_eq!(answer["status"], "not_found", "{answer}");
        assert_eq!(answer["message"], "This text isn't in the KJV as quoted.");
    }
    let answer = install.check(invented, Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "differs");
    install.assert_it_keeps_its_promise(invented, &answer).await;
    install.close().await;
}

/// No reference: a phrase search finds the true one, the fragment of a verse
/// or the whole of it.
#[tokio::test]
async fn no_reference_finds_the_real_one() {
    let install = Install::indexed("verify_finds").await;
    let verse = install.real_text("John 3:16", "kjv").await;
    assert_eq!(
        install
            .check("For God so loved the world", None, "kjv")
            .await,
        json!({
            "status": "exact_part",
            "translation": "kjv",
            "reference": "John 3:16",
            "text": verse,
            "message": "This is an exact part of John 3:16 in the KJV.",
        })
    );
    let answer = install.check(&verse, None, "kjv").await;
    assert_eq!(answer["status"], "exact");
    assert_eq!(answer["reference"], "John 3:16");
    install.close().await;
}

/// Across verses, with no reference: the verses it's in. The index is by verse,
/// so what makes these hard is a verse ending among a quote's first words.
#[tokio::test]
async fn no_reference_finds_a_quote_across_verses() {
    let install = Install::indexed("verify_across").await;
    let whole = install.real_text("John 3:16-17", "kjv").await;
    let answer = install.check(&whole, None, "kjv").await;
    assert_eq!(answer["status"], "exact");
    assert_eq!(answer["reference"], "John 3:16\u{2013}17");

    for (quote, reference) in [
        // Eight words later, past the verse's end.
        (
            "everlasting life. For God sent not his Son into the world",
            "John 3:16\u{2013}17",
        ),
        // Too few words after the verse's end for that, but before it enough.
        (
            "that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. For God",
            "John 3:16\u{2013}17",
        ),
        ("everlasting life. For", "John 3:16\u{2013}17"),
        // Few words either side, and the first place of two, the earlier verses
        // ending the same way.
        ("life. For God", "John 3:15\u{2013}16"),
        // Past a verse too short to hold eight words.
        (
            "see. Jesus wept. Then said the Jews",
            "John 11:34\u{2013}36",
        ),
        // Verses each too short to hold eight words: a run in the middle.
        (
            "Enosh, Kenan, Mahalaleel, Jered, Henoch,",
            "1 Chronicles 1:1\u{2013}3",
        ),
    ] {
        let answer = install.check(quote, None, "kjv").await;
        assert_eq!(answer["status"], "exact_part", "{quote}: {answer}");
        assert_eq!(answer["reference"], reference, "{quote}");
        install.assert_it_keeps_its_promise(quote, &answer).await;
    }
    install.close().await;
}

/// The same words in another case or punctuation are no match: the first verse
/// with them, its difference, and a message saying it's only the closest.
#[tokio::test]
async fn the_same_words_otherwise_written_are_only_the_closest() {
    let install = Install::indexed("verify_closest").await;
    let answer = install
        .check("For god so loved the world that he", None, "kjv")
        .await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(answer["reference"], "John 3:16");
    assert_eq!(
        answer["text"],
        install.real_text("John 3:16", "kjv").await.as_str()
    );
    assert_eq!(answer["difference"]["position"], 5);
    assert_eq!(
        answer["message"],
        "This isn't in the KJV as quoted. The closest verse is John 3:16, not a match: it \
         differs at word 2, character 5."
    );

    // Long enough to run on, it's compared with as many verses as it takes, so
    // the difference is where it really departs.
    let whole = install.real_text("John 3:16-17", "kjv").await;
    let changed = whole.replacen("condemn", "CONDEMN", 1);
    let answer = install.check(&changed, None, "kjv").await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(answer["reference"], "John 3:16\u{2013}17");
    assert_eq!(answer["text"], whole.as_str());
    let position = whole.find("condemn").expect("the word") + 1;
    assert_eq!(answer["difference"]["position"], position);
    assert_eq!(
        answer["difference"]["actual"],
        "condemn the world; but that the world th"
    );
    let message = answer["message"].as_str().expect("a message");
    assert!(message.contains("closest verses are"), "{message}");
    install.close().await;
}

/// A quote in several places: all of them, in Bible order, and the one the
/// answer names is the whole of a verse if it's that anywhere.
#[tokio::test]
async fn several_places_are_all_given() {
    let install = Install::indexed("verify_several").await;
    let quote = "O LORD our Lord, how excellent is thy name in all the earth!";
    let answer = install.check(quote, None, "kjv").await;
    assert_eq!(answer["status"], "exact", "{answer}");
    assert_eq!(answer["reference"], "Psalms 8:9");
    let places: Vec<&str> = answer["found_at"]
        .as_array()
        .expect("found_at")
        .iter()
        .map(|p| p["reference"].as_str().expect("a reference"))
        .collect();
    assert_eq!(places, ["Psalms 8:1", "Psalms 8:9"]);
    assert_eq!(
        answer["message"],
        "This is exactly Psalms 8:9 in the KJV. It's in 2 places; found_at lists them."
    );
    install.assert_it_keeps_its_promise(quote, &answer).await;

    // Given a passage that has it, there's no search, and no list.
    let answer = install.check(quote, Some("Psalm 8:1"), "kjv").await;
    assert_eq!(answer["status"], "exact_part");
    assert!(answer.get("found_at").is_none());
    install.close().await;
}

/// More verses with a common phrase than are read: only the first of them are
/// reported, and the answer says it hasn't seen them all.
#[tokio::test]
async fn a_common_phrase_says_it_was_not_all_checked() {
    let install = Install::indexed("verify_common").await;
    let answer = install.check("oF the LORD", None, "kjv").await;
    assert_eq!(answer["status"], "differs", "{answer}");
    let message = answer["message"].as_str().expect("a message");
    assert!(message.ends_with("not all were checked."), "{message}");

    let answer = install.check("of the LORD", None, "kjv").await;
    assert_eq!(answer["status"], "exact_part", "{answer}");
    let places = answer["found_at"].as_array().expect("found_at");
    assert_eq!(places.len(), MAX_CANDIDATES as usize);
    let message = answer["message"].as_str().expect("a message");
    assert!(message.contains("at least 200 places"), "{message}");

    // In no one verse, with both halves too common to read every verse of: a
    // verse could end between them, so it can't plainly say it isn't there.
    let answer = install.check("and the of the", None, "kjv").await;
    assert_eq!(
        answer,
        json!({
            "status": "not_found",
            "translation": "kjv",
            "message": "This wasn't found in the KJV as quoted, but its words are too common \
                        to check every verse. Give a reference to check it there.",
        })
    );
    install.close().await;
}

#[test]
fn only_a_phrase_that_could_hide_the_quote_counts_as_too_common() {
    let quote = quote_words("a b c d");
    let missed =
        |too_common: &[&[String]]| may_have_missed(&quote, &too_common.iter().copied().collect());
    // Nothing too common; or one side of each split read in full.
    assert!(!missed(&[]));
    assert!(!missed(&[
        &quote[..1],
        &quote[1..2],
        &quote[2..3],
        &quote[3..]
    ]));
    assert!(!missed(&[&quote[..2], &quote[..1]]));
    // Both sides of one split.
    assert!(missed(&[&quote[..2], &quote[2..]]));
    // The whole quote.
    assert!(missed(&[&quote[..]]));

    // A long quote: only its leading words count.
    let long = quote_words("a b c d e f g h i j");
    let too_common = [&long[..1], &long[1..]].into_iter().collect();
    assert!(!may_have_missed(&long, &too_common));
    let too_common = [&long[..PHRASE_WORDS]].into_iter().collect();
    assert!(may_have_missed(&long, &too_common));
}

// ---- F1: the clean text, and nothing but.

/// Supplied words are plain words, without the source's braces; margin notes
/// aren't in the text, so quoting one is no match. As the source stores a verse
/// isn't what the app's Copy gives.
#[tokio::test]
async fn the_kjvs_markup_is_read_as_copy_reads_it() {
    let install = Install::indexed("verify_markup").await;
    let kjv = bundled("kjv");
    let john = &kjv[42].chapters[2];
    let stored = john[4].as_str();
    let clean = stored.replace(['{', '}'], "");
    assert_ne!(stored, clean);

    assert_eq!(
        install.check(&clean, Some("John 3:5"), "kjv").await["status"],
        "exact"
    );
    let answer = install.check(&clean, None, "kjv").await;
    assert_eq!(answer["status"], "exact");
    assert_eq!(answer["reference"], "John 3:5");
    assert_eq!(
        install
            .check("The LORD is my shepherd; I shall not want.", None, "kjv")
            .await["reference"],
        "Psalms 23:1"
    );

    let answer = install.check(stored, Some("John 3:5"), "kjv").await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(answer["text"], clean.as_str());
    let brace = stored.find('{').expect("a brace");
    assert_eq!(answer["difference"]["position"], brace + 1);

    let stored = kjv[18].chapters[118][104].as_str();
    assert!(stored.ends_with("{lamp: or, candle}"));
    let clean = install.real_text("Psalm 119:105", "kjv").await;
    assert!(!clean.contains("candle") && !clean.contains('{'));
    let answer = install.check(&clean, Some("Psalm 119:105"), "kjv").await;
    assert_eq!(answer["status"], "exact");
    let answer = install.check(stored, Some("Psalm 119:105"), "kjv").await;
    assert_eq!(answer["status"], "differs");
    install.assert_it_keeps_its_promise(stored, &answer).await;
    let answer = install.check(stored, None, "kjv").await;
    assert_ne!(answer["status"], "exact");

    // A note's words find no verse, as in the app's own search.
    let answer = install.check("lamp: or, candle", None, "kjv").await;
    assert_eq!(answer["status"], "not_found", "{answer}");
    install.close().await;
}

// ---- Unicode: positions are characters, and nothing is cut inside one.

/// Curly quotes and an em dash, in a quote and in the text.
#[tokio::test]
async fn curly_quotes_and_dashes_are_characters_like_any_other() {
    let install = Install::indexed("verify_unicode").await;
    let verse = install.real_text("John 3:3", "bsb").await;
    assert!(verse.contains('\u{201c}') && verse.ends_with('\u{201d}'));
    let answer = install.check(&verse, None, "bsb").await;
    assert_eq!(answer["status"], "exact", "{answer}");
    assert_eq!(answer["reference"], "John 3:3");

    // Straightening the last quote: its position counts characters, and the
    // opening curly quote before it is three bytes.
    let straight = verse.replace('\u{201d}', "\"");
    let answer = install.check(&straight, Some("John 3:3"), "bsb").await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(answer["difference"]["position"], verse.chars().count());
    assert_eq!(answer["difference"]["quoted"], "\"");
    assert_eq!(answer["difference"]["actual"], "\u{201d}");

    // In the WEB, a no-break space: the ordinary space isn't it.
    let verse = install.real_text("Genesis 22:16", "web").await;
    assert!(verse.contains('\u{a0}'));
    let answer = install.check(&verse, Some("Genesis 22:16"), "web").await;
    assert_eq!(answer["status"], "exact");
    let plain = verse.replace('\u{a0}', " ");
    let answer = install.check(&plain, Some("Genesis 22:16"), "web").await;
    assert_eq!(answer["status"], "differs");
    let at = verse
        .chars()
        .position(|c| c == '\u{a0}')
        .expect("the space");
    assert_eq!(answer["difference"]["position"], at + 1);

    // A curly-quoted quote against a text with none: nothing to panic on.
    let quote = "\u{201c}For God\u{2014}so loved\u{201d}";
    let answer = install.check(quote, Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "differs");
    assert_eq!(answer["difference"]["position"], 1);
    assert_eq!(answer["difference"]["quoted"], quote);
    let answer = install.check(quote, None, "kjv").await;
    assert_ne!(answer["status"], "exact_part");
    install.assert_it_keeps_its_promise(quote, &answer).await;
    install.close().await;
}

#[test]
fn a_difference_is_counted_in_characters() {
    // Each of these is more than one byte, and they fall around the 40 shown.
    let quote: String = "\u{201c}".repeat(3) + "b" + &"\u{2014}".repeat(50);
    let text: String = "\u{201c}".repeat(3) + "c" + &"\u{2019}".repeat(50);
    let difference = first_difference(&quote, &text);
    assert_eq!(difference.position, 4);
    assert_eq!(difference.word, 1);
    assert_eq!(difference.quoted.chars().count(), SHOWN_CHARS);
    assert_eq!(difference.actual.chars().count(), SHOWN_CHARS);
    assert!(difference.quoted.starts_with('b'));
    assert!(difference.actual.starts_with('c'));
}

#[test]
fn a_quote_is_aligned_where_the_text_shares_the_most_with_it() {
    // The earlier "the" shares less than the later "the world".
    let difference = first_difference("the world!", "see the sea and the world, too");
    assert_eq!(difference.position, 10);
    assert_eq!(difference.quoted, "!");
    assert_eq!(difference.actual, ", too");

    // Of two that share as much, the earliest.
    let difference = first_difference("abX", "abC abD");
    assert_eq!(difference.actual, "C abD");

    // Sharing nothing, the start of each.
    assert_eq!(
        first_difference("xyz", "abc"),
        QuoteDifference {
            position: 1,
            word: 1,
            quoted: "xyz".to_string(),
            actual: "abc".to_string(),
        }
    );

    // A space counts to the word before it.
    let difference = first_difference("one two three", "one two, three");
    assert_eq!((difference.position, difference.word), (8, 2));
}

#[test]
fn the_comparison_folds_nothing() {
    for (quote, text, status) in [
        ("a b", "a b", Some(QuoteStatus::Exact)),
        ("a", "a b", Some(QuoteStatus::ExactPart)),
        ("A b", "a b", None),
        ("a  b", "a b", None),
        ("a\nb", "a b", None),
        ("a,b", "a b", None),
        ("\u{2019}", "'", None),
        ("\u{e9}", "e\u{301}", None),
    ] {
        assert_eq!(verbatim(quote, text), status, "{quote:?} in {text:?}");
    }
}

#[test]
fn places_across_verses_name_those_verses() {
    let joined = JoinedVerses::new([
        (1, "one two".to_string()),
        (2, "three \u{2014} four".to_string()),
        (4, "five two".to_string()),
    ]);
    assert_eq!(joined.text, "one two three \u{2014} four five two");
    assert_eq!(joined.places_of("nope"), []);
    assert_eq!(
        joined.places_of("two"),
        [((1, 1), "one two"), ((4, 4), "five two")]
    );
    assert_eq!(
        joined.places_of("two three"),
        [((1, 2), "one two three \u{2014} four")]
    );
    assert_eq!(
        joined.places_of("four five"),
        [((2, 4), "three \u{2014} four five two")]
    );
    // Two in one verse are one place.
    let joined = JoinedVerses::new([(1, "a a".to_string())]);
    assert_eq!(joined.places_of("a"), [((1, 1), "a a")]);
    // Overlapping itself, across verses.
    let joined = JoinedVerses::new([
        (1, "a.".to_string()),
        (2, "a.".to_string()),
        (3, "a.".to_string()),
    ]);
    assert_eq!(
        joined.places_of("a. a."),
        [((1, 2), "a. a."), ((2, 3), "a. a.")]
    );
    assert_eq!(joined.window_from(2, 1), Some(((2, 2), "a.")));
    assert_eq!(joined.window_from(2, 4), Some(((2, 3), "a. a.")));
    assert_eq!(joined.window_from(2, 400), Some(((2, 3), "a. a.")));
    assert_eq!(joined.window_from(9, 1), None);
}

#[test]
fn the_phrases_start_with_the_quotes_first_words() {
    let phrases = |count: usize| -> Vec<Vec<String>> {
        let words: Vec<String> = (0..count).map(|i| i.to_string()).collect();
        phrases_of(&words).map(<[String]>::to_vec).collect()
    };
    assert!(phrases(0).is_empty());
    assert_eq!(phrases(1), [["0"]]);
    assert_eq!(phrases(2), [vec!["0", "1"], vec!["0"], vec!["1"]]);
    assert_eq!(
        phrases(3),
        [
            vec!["0", "1", "2"],
            vec!["0", "1"],
            vec!["1", "2"],
            vec!["0"],
            vec!["1"],
            vec!["2"],
        ]
    );
    let many = phrases(PHRASE_STARTS + PHRASE_WORDS);
    assert_eq!(many[0].len(), PHRASE_WORDS);
    assert_eq!(many[0][0], "0");
    assert_eq!(many[1][0], "1");
    assert_eq!(many[1].len(), PHRASE_WORDS);
    assert_eq!(many[PHRASE_STARTS - 1][0], (PHRASE_STARTS - 1).to_string());
    assert_eq!(many[PHRASE_STARTS - 1].len(), PHRASE_WORDS);
    assert_eq!(many[PHRASE_STARTS].len(), PHRASE_WORDS - 1);
    assert_eq!(many.last().expect("a phrase").len(), 3);
}

// ---- What it won't do.

/// Only the leading and trailing whitespace of the whole quote goes.
#[tokio::test]
async fn only_the_quotes_ends_are_trimmed() {
    let install = Install::indexed("verify_trim").await;
    let answer = install
        .check("\n  Jesus wept.\t\n", Some("John 11:35"), "kjv")
        .await;
    assert_eq!(answer["status"], "exact");
    for inner in ["Jesus  wept.", "Jesus\nwept.", "Jesus\twept."] {
        let answer = install.check(inner, Some("John 11:35"), "kjv").await;
        assert_eq!(answer["status"], "differs", "{inner:?}");
    }
    install.close().await;
}

/// The core promise, hard: one-character changes at every place in real verses,
/// with a reference and without. Whatever comes back, the characters bear out;
/// and no change to a verse is called the whole of it.
#[tokio::test]
async fn no_change_to_a_verse_is_ever_called_exact_in_it() {
    let install = Install::indexed("verify_promise").await;
    for (reference, code) in [("John 11:35", "kjv"), ("Luke 17:32", "web")] {
        let verse = install.real_text(reference, code).await;
        let unchanged = install.check(&verse, Some(reference), code).await;
        assert_eq!(unchanged["status"], "exact");

        let chars: Vec<char> = verse.chars().collect();
        for at in 0..chars.len() {
            let edit = |middle: &[char], skip: usize| -> String {
                chars[..at]
                    .iter()
                    .chain(middle)
                    .chain(&chars[at + skip..])
                    .collect()
            };
            let swapped: Vec<char> = if chars[at].is_uppercase() {
                chars[at].to_lowercase().collect()
            } else {
                chars[at].to_uppercase().collect()
            };
            for quote in [
                edit(&swapped, 1),
                edit(&[], 1),
                edit(&[',', chars[at]], 1),
                edit(&[' ', chars[at]], 1),
            ] {
                if quote.trim() == verse || quote.trim().is_empty() {
                    continue;
                }
                let found = install.check(&quote, Some(reference), code).await;
                let whole = (found["status"].as_str(), found["reference"].as_str());
                assert_ne!(whole, (Some("exact"), Some(reference)), "{quote:?}");
                install.assert_it_keeps_its_promise(&quote, &found).await;

                if !quote_words(&quote).is_empty() {
                    let found = install.check(&quote, None, code).await;
                    install.assert_it_keeps_its_promise(&quote, &found).await;
                }
            }
        }
    }
    install.close().await;
}

/// The same refusals as `get_passage`, in the same words.
#[tokio::test]
async fn a_reference_that_cannot_be_read_is_refused_as_get_passage_does() {
    let install = Install::indexed("verify_bad_reference").await;
    for (reference, code) in [
        ("Hezekiah 3:16", "kjv"),
        ("John 22:1", "kjv"),
        ("for God so loved", "kjv"),
        ("Matthew 17:21", "bsb"),
    ] {
        let translation = install.library.translation(Some(code)).await.unwrap();
        let refusal = install.library.passage_in(translation, reference).await;
        let message = format!("{:#}", refusal.unwrap_err());
        assert_eq!(
            install.refused("Jesus wept.", Some(reference), code).await,
            message
        );
    }
    let message = install.refused("Jesus wept.", None, "niv").await;
    assert!(message.contains("niv") && message.contains("kjv, bsb, web"));
    install.close().await;
}

/// An empty quote, and one too long: refused, plainly. The limit counts
/// characters, not bytes, and doesn't count what's trimmed.
#[tokio::test]
async fn empty_and_oversized_quotes_are_refused() {
    let install = Install::indexed("verify_size").await;
    for empty in ["", " \n\t "] {
        let message = install.refused(empty, None, "kjv").await;
        assert!(message.contains("empty"), "{message}");
        let message = install.refused(empty, Some("John 3:16"), "kjv").await;
        assert!(message.contains("empty"), "{message}");
    }
    let over = "x".repeat(MAX_QUOTE_CHARS + 1);
    assert_eq!(
        install.refused(&over, None, "kjv").await,
        "That's 10001 characters; at most 10000 per quotation. Split it."
    );
    let wide = "\u{2014}x".repeat(MAX_QUOTE_CHARS / 2);
    let answer = install.check(&wide, Some("John 3:16"), "kjv").await;
    assert_eq!(answer["status"], "differs");
    let padded = format!("  {}  ", "x".repeat(MAX_QUOTE_CHARS));
    assert_eq!(
        install.check(&padded, None, "kjv").await["status"],
        "not_found"
    );
    install.close().await;
}

/// With no words to search for and no passage to look in, there's nothing to do.
#[tokio::test]
async fn a_quote_with_no_words_needs_a_reference() {
    let install = Install::indexed("verify_no_words").await;
    for quote in ["...", "\"", "\u{2014}"] {
        let message = install.refused(quote, None, "kjv").await;
        assert!(message.contains("no words"), "{message}");
    }
    assert_eq!(
        install.check(";", Some("Psalm 23:1"), "kjv").await["status"],
        "exact_part"
    );
    assert_eq!(
        install.check("!?", Some("Psalm 23:1"), "kjv").await["status"],
        "differs"
    );
    install.close().await;
}

/// Whatever is in a quote goes to the search as words, never as query syntax.
#[tokio::test]
async fn a_quote_is_never_read_as_search_syntax() {
    let install = Install::indexed("verify_syntax").await;
    for quote in [
        "\"For God\" OR \"so loved\"",
        "For* NEAR(God so) AND loved",
        "text : \"God\" ^ { } ( ) - + * ' \u{201c}",
        "'' OR 1=1 --",
    ] {
        let answer = install.check(quote, None, "kjv").await;
        assert_ne!(answer["status"], "exact", "{quote}");
        assert_ne!(answer["status"], "exact_part", "{quote}");
        install.assert_it_keeps_its_promise(quote, &answer).await;
    }
    install.close().await;
}

// ---- One translation (B4), the default (F2).

/// A quote from one translation isn't looked for in another, even with the very
/// words in the very verse named.
#[tokio::test]
async fn only_the_translation_named_is_looked_in() {
    let install = Install::indexed("verify_translation").await;
    let kjv = install.real_text("John 3:16", "kjv").await;
    assert_eq!(install.check(&kjv, None, "kjv").await["status"], "exact");
    for code in ["bsb", "web"] {
        let answer = install.check(&kjv, Some("John 3:16"), code).await;
        assert_eq!(answer["status"], "differs", "{answer}");
        assert_eq!(answer["translation"], code);
        assert_eq!(
            answer["text"],
            install.real_text("John 3:16", code).await.as_str()
        );
        assert!(answer.get("found_at").is_none());
        assert_ne!(install.check(&kjv, None, code).await["status"], "exact");
    }
    let only_kjv = "that whosoever believeth in him should not perish";
    assert_eq!(
        install.check(only_kjv, None, "kjv").await["status"],
        "exact_part"
    );
    assert_eq!(
        install.check(only_kjv, None, "web").await["status"],
        "not_found"
    );
    install.close().await;
}

/// None named: the reader's saved one, else the KJV.
#[tokio::test]
async fn the_default_translation_is_the_readers() {
    let install = Install::indexed("verify_default").await;
    let verify = async |quote: &str| {
        let args = VerifyQuoteArgs {
            text: quote.to_string(),
            reference: None,
            translation: None,
        };
        let out = install.library.verify_quote(args).await.expect("an answer");
        serde_json::to_value(out).expect("JSON")
    };
    let kjv = install.real_text("John 3:16", "kjv").await;
    let web = install.real_text("John 3:16", "web").await;
    let answer = verify(&kjv).await;
    assert_eq!(
        (&answer["translation"], &answer["status"]),
        (&json!("kjv"), &json!("exact"))
    );

    let id: i64 = sqlx::query_scalar("SELECT id FROM translations WHERE code = 'web'")
        .fetch_one(install.pool.read())
        .await
        .expect("a translation");
    db::save_reading_position_at(install.pool.write(), id, 43, 3, None)
        .await
        .expect("save the position");
    let answer = verify(&web).await;
    assert_eq!(
        (&answer["translation"], &answer["status"]),
        (&json!("web"), &json!("exact"))
    );
    assert_ne!(verify(&kjv).await["status"], "exact");
    install.close().await;
}

// ---- F3.

/// With no search index, a passage that has the quote is still checked; all
/// that needs a search says to open the app.
#[tokio::test]
async fn only_what_needs_a_search_needs_the_index() {
    let (db, pool) = prepared_database("verify_no_index").await;
    let library = Library::open(&db.path()).await.expect("the install");
    let install = Install {
        _db: db,
        pool,
        library,
    };
    let answer = install
        .check("Jesus wept.", Some("John 11:35"), "kjv")
        .await;
    assert_eq!(answer["status"], "exact");
    let answer = install.check("Jesus", Some("John 11:35"), "kjv").await;
    assert_eq!(answer["status"], "exact_part");
    for reference in [None, Some("John 3:16")] {
        let message = install.refused("Jesus wept.", reference, "kjv").await;
        assert!(message.contains("search index"), "{message}");
        assert!(
            message.contains("Open Gospel Getter once first"),
            "{message}"
        );
    }
    install.close().await;
}
