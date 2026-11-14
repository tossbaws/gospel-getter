//! The JSON the commands send for the shapes the page declares too, pinned
//! to literals (`ui-leptos/src/dto.rs`'s tests read the same literals).

use crate::commands::{
    BibleEdgeDto, BookDto, BookStartDto, CopyFormatsDto, FindingDto, FindingRequestDto, GoToDto,
    NoteLinkDto, SearchDto, SearchResultDto, TranslationDto, XrefDto,
};
use crate::notes_export::{Commentary, CommentaryBook, CommentaryChapter, CommentaryNote};
use crate::release_notes::{Block, Span};
use crate::start_here::StartHereDto;

#[track_caller]
fn sends<T: serde::Serialize>(value: &T, json: &str) {
    assert_eq!(serde_json::to_string(value).expect("writes"), json);
}

fn note() -> CommentaryNote {
    CommentaryNote {
        verses: "16–18".into(),
        body: "**Grace**, not works.".into(),
    }
}

fn chapter() -> CommentaryChapter {
    CommentaryChapter {
        heading: "Chapter 3".into(),
        notes: vec![note()],
    }
}

fn book() -> CommentaryBook {
    CommentaryBook {
        name: "John".into(),
        chapters: vec![chapter()],
    }
}

fn plain() -> Span {
    Span {
        text: "Fixed ".into(),
        strong: false,
        code: false,
    }
}

fn marked() -> Span {
    Span {
        text: "trunk".into(),
        strong: true,
        code: true,
    }
}

fn result() -> SearchResultDto {
    SearchResultDto {
        book_id: 43,
        chapter: 3,
        verse: 16,
        reference: "John 3:16".into(),
        text: "the verse's words".into(),
        marks: vec![(4, 9)],
    }
}

fn go_to() -> GoToDto {
    GoToDto {
        book_id: 19,
        chapter: 1,
        reference: "Psalms 1".into(),
    }
}

fn finding() -> FindingDto {
    FindingDto {
        id: 7,
        book_id: 43,
        book_name: "John".into(),
        chapter: 11,
        verse_start: 35,
        verse_end: 35,
        reference: "John 11:35".into(),
        agent: "Claude".into(),
        request: "grief".into(),
        text: "Found by Claude for your request “grief”.".into(),
        created_at: "2026-11-07 09:30:00".into(),
        created_at_display: "Nov 7, 2026".into(),
    }
}

#[test]
fn commentary_note() {
    sends(
        &note(),
        r#"{"verses":"16–18","body":"**Grace**, not works."}"#,
    );
}

#[test]
fn commentary_chapter() {
    sends(
        &chapter(),
        r#"{"heading":"Chapter 3","notes":[{"verses":"16–18","body":"**Grace**, not works."}]}"#,
    );
}

#[test]
fn commentary_book() {
    sends(
        &book(),
        r#"{"name":"John","chapters":[{"heading":"Chapter 3","notes":[{"verses":"16–18","body":"**Grace**, not works."}]}]}"#,
    );
}

#[test]
fn commentary() {
    let commentary = Commentary {
        title: "My notes".into(),
        exported: "Nov 14, 2026".into(),
        written: None,
        book_headings: true,
        books: vec![book()],
    };
    sends(
        &commentary,
        r#"{"title":"My notes","exported":"Nov 14, 2026","written":null,"bookHeadings":true,"books":[{"name":"John","chapters":[{"heading":"Chapter 3","notes":[{"verses":"16–18","body":"**Grace**, not works."}]}]}]}"#,
    );
}

#[test]
fn span() {
    sends(&plain(), r#"{"text":"Fixed "}"#);
    sends(&marked(), r#"{"text":"trunk","strong":true,"code":true}"#);
}

#[test]
fn block() {
    sends(
        &Block::Heading {
            spans: vec![plain()],
        },
        r#"{"kind":"heading","spans":[{"text":"Fixed "}]}"#,
    );
    let paragraph = Block::Paragraph {
        spans: vec![plain(), marked()],
    };
    sends(
        &paragraph,
        r#"{"kind":"paragraph","spans":[{"text":"Fixed "},{"text":"trunk","strong":true,"code":true}]}"#,
    );
    let item = Block::Item {
        depth: 1,
        spans: vec![marked()],
    };
    sends(
        &item,
        r#"{"kind":"item","depth":1,"spans":[{"text":"trunk","strong":true,"code":true}]}"#,
    );
}

#[test]
fn note_link() {
    let link = NoteLinkDto {
        start: 4,
        end: 14,
        book_id: 45,
        chapter: 5,
        verse_start: Some(8),
        verse_end: None,
        reference: "Romans 5:8".into(),
    };
    sends(
        &link,
        r#"{"start":4,"end":14,"bookId":45,"chapter":5,"verseStart":8,"verseEnd":null,"reference":"Romans 5:8"}"#,
    );
}

#[test]
fn search_result() {
    sends(
        &result(),
        r#"{"bookId":43,"chapter":3,"verse":16,"reference":"John 3:16","text":"the verse's words","marks":[[4,9]]}"#,
    );
}

#[test]
fn go_to_chapter() {
    sends(
        &go_to(),
        r#"{"bookId":19,"chapter":1,"reference":"Psalms 1"}"#,
    );
}

#[test]
fn search() {
    sends(&SearchDto::Empty, r#"{"kind":"empty"}"#);
    let passage = SearchDto::Passage {
        book_id: 43,
        chapter: 3,
        verse_start: Some(16),
        verse_end: None,
        reference: "John 3:16".into(),
    };
    sends(
        &passage,
        r#"{"kind":"passage","bookId":43,"chapter":3,"verseStart":16,"verseEnd":null,"reference":"John 3:16"}"#,
    );
    let invalid = SearchDto::Invalid {
        message: "John has 21 chapters.".into(),
    };
    sends(
        &invalid,
        r#"{"kind":"invalid","message":"John has 21 chapters."}"#,
    );
    let unavailable = SearchDto::Unavailable {
        message: "Search isn't ready yet.".into(),
    };
    sends(
        &unavailable,
        r#"{"kind":"unavailable","message":"Search isn't ready yet."}"#,
    );
    let text = SearchDto::Text {
        terms: vec!["love".into()],
        total: 1,
        offset: 0,
        page_size: 50,
        results: vec![result()],
        go_to: None,
    };
    sends(
        &text,
        r#"{"kind":"text","terms":["love"],"total":1,"offset":0,"pageSize":50,"results":[{"bookId":43,"chapter":3,"verse":16,"reference":"John 3:16","text":"the verse's words","marks":[[4,9]]}]}"#,
    );
    let book = SearchDto::Text {
        terms: vec!["psalms".into()],
        total: 0,
        offset: 0,
        page_size: 50,
        results: vec![],
        go_to: Some(go_to()),
    };
    sends(
        &book,
        r#"{"kind":"text","terms":["psalms"],"total":0,"offset":0,"pageSize":50,"results":[],"goTo":{"bookId":19,"chapter":1,"reference":"Psalms 1"}}"#,
    );
}

#[test]
fn translation() {
    let dated = TranslationDto {
        code: "KJV".into(),
        name: "King James Version".into(),
        date: Some("1611/1769".into()),
        licence: "Public domain".into(),
        credit: "King James Version (1611/1769), public domain.".into(),
    };
    sends(
        &dated,
        r#"{"code":"KJV","name":"King James Version","date":"1611/1769","licence":"Public domain","credit":"King James Version (1611/1769), public domain."}"#,
    );
    let undated = TranslationDto {
        code: "WEB".into(),
        name: "World English Bible".into(),
        date: None,
        licence: "Public domain".into(),
        credit: "World English Bible, public domain.".into(),
    };
    sends(
        &undated,
        r#"{"code":"WEB","name":"World English Bible","date":null,"licence":"Public domain","credit":"World English Bible, public domain."}"#,
    );
}

#[test]
fn xref() {
    let verse = XrefDto {
        book_id: 45,
        chapter: 5,
        verse: 8,
        end_verse: None,
        citation: "Rom 5:8".into(),
    };
    sends(
        &verse,
        r#"{"bookId":45,"chapter":5,"verse":8,"endVerse":null,"citation":"Rom 5:8"}"#,
    );
    let range = XrefDto {
        book_id: 62,
        chapter: 4,
        verse: 9,
        end_verse: Some(10),
        citation: "1 John 4:9-10".into(),
    };
    sends(
        &range,
        r#"{"bookId":62,"chapter":4,"verse":9,"endVerse":10,"citation":"1 John 4:9-10"}"#,
    );
}

#[test]
fn bible_edge() {
    let end = BibleEdgeDto {
        title: "The end of the Bible".into(),
        detail: None,
    };
    sends(&end, r#"{"title":"The end of the Bible","detail":null}"#);
    let start = BibleEdgeDto {
        title: "The Old Testament".into(),
        detail: Some("Genesis · 50 chapters".into()),
    };
    sends(
        &start,
        r#"{"title":"The Old Testament","detail":"Genesis · 50 chapters"}"#,
    );
}

#[test]
fn book_start() {
    let start = BookStartDto {
        book_id: 2,
        chapter: 1,
        book_name: "Exodus".into(),
    };
    sends(&start, r#"{"bookId":2,"chapter":1,"bookName":"Exodus"}"#);
}

#[test]
fn copy_formats() {
    let formats = CopyFormatsDto {
        reference: "John 3:16 (KJV)".into(),
        numbered: "16 the verse's words".into(),
        markdown: "> the verse's words".into(),
        plain: "the verse's words".into(),
    };
    sends(
        &formats,
        r#"{"reference":"John 3:16 (KJV)","numbered":"16 the verse's words","markdown":"> the verse's words","plain":"the verse's words"}"#,
    );
}

#[test]
fn finding_dto() {
    sends(
        &finding(),
        r#"{"id":7,"bookId":43,"bookName":"John","chapter":11,"verseStart":35,"verseEnd":35,"reference":"John 11:35","agent":"Claude","request":"grief","text":"Found by Claude for your request “grief”.","createdAt":"2026-11-07 09:30:00","createdAtDisplay":"Nov 7, 2026"}"#,
    );
}

#[test]
fn finding_request() {
    let request = FindingRequestDto {
        request: "grief".into(),
        agents: "Claude".into(),
        verse_count: 1,
        latest_display: "Nov 7, 2026".into(),
        findings: vec![finding()],
    };
    sends(
        &request,
        r#"{"request":"grief","agents":"Claude","verseCount":1,"latestDisplay":"Nov 7, 2026","findings":[{"id":7,"bookId":43,"bookName":"John","chapter":11,"verseStart":35,"verseEnd":35,"reference":"John 11:35","agent":"Claude","request":"grief","text":"Found by Claude for your request “grief”.","createdAt":"2026-11-07 09:30:00","createdAtDisplay":"Nov 7, 2026"}]}"#,
    );
}

#[test]
fn book_dto() {
    let genesis = BookDto {
        id: 1,
        name: "Genesis".into(),
        chapter_count: 50,
    };
    sends(&genesis, r#"{"id":1,"name":"Genesis","chapterCount":50}"#);
}

#[test]
fn start_here() {
    let row = StartHereDto {
        book_id: 43,
        chapter: 1,
        title: "John".into(),
        line: "Begin with the life of Jesus.".into(),
    };
    sends(
        &row,
        r#"{"bookId":43,"chapter":1,"title":"John","line":"Begin with the life of Jesus."}"#,
    );
}
