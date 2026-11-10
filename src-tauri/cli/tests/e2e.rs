//! The real program against disposable databases, prepared as the app prepares
//! its own: subcommands, the MCP server over stdio, and what they refuse. Every
//! test ends by checking nothing beside the database changed. No display is
//! set for any of it, since the program must not need one.

use gospel_getter_core::db::test_support::{TempDb, fresh_database, prepared_database};
use gospel_getter_core::db::{self, Db, HighlightColor, Passage, begin_write};
use serde_json::{Value, json};
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime};

const BIN: &str = env!("CARGO_BIN_EXE_gospel-getter-cli");
/// However slow a machine is, an answer that takes this long isn't coming.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

const JOHN: i64 = 43;

/// John 3:16, each exactly as its translation has it, and two KJV verses with
/// markup: a supplied word (John 3:5), and one with a margin note too (Psalms
/// 119:105). Typed here, not read from the same place as the program reads.
const JOHN_3_16_KJV: &str = "For God so loved the world, that he gave his only begotten Son, \
    that whosoever believeth in him should not perish, but have everlasting life.";
const JOHN_3_16_WEB: &str = "For God so loved the world, that he gave his one and only Son, \
    that whoever believes in him should not perish, but have eternal life.";
const JOHN_3_16_BSB: &str = "For God so loved the world that He gave His one and only Son, \
    that everyone who believes in Him shall not perish but have eternal life.";
const JOHN_3_5_KJV: &str = "Jesus answered, Verily, verily, I say unto thee, Except a man be \
    born of water and of the Spirit, he cannot enter into the kingdom of God.";
const PSALM_119_105_KJV: &str = "NUN. Thy word is a lamp unto my feet, and a light unto my path.";

// ---- The read-only proof.

/// A file's length, modification time and contents.
#[derive(Debug, PartialEq, Eq)]
struct FileState {
    length: u64,
    modified: SystemTime,
    contents: u64,
}

fn file_state(path: &Path) -> FileState {
    let metadata = std::fs::metadata(path).expect("the file's metadata");
    let mut hasher = DefaultHasher::new();
    std::fs::read(path).expect("the file").hash(&mut hasher);
    FileState {
        length: metadata.len(),
        modified: metadata.modified().expect("the file's time"),
        contents: hasher.finish(),
    }
}

/// Every file in the database's folder. `-shm` only by name: it's SQLite's
/// index of a log in use, where every reader takes its turn, so a read through
/// it changes it (and only it), as any reader of a running app's database does.
fn folder_state(db: &TempDb) -> BTreeMap<String, Option<FileState>> {
    let path = db.path();
    std::fs::read_dir(path.parent().expect("a folder"))
        .expect("list the folder")
        .map(|entry| {
            let entry = entry.expect("an entry");
            let name = entry.file_name().to_string_lossy().into_owned();
            let state = (!name.ends_with("-shm")).then(|| file_state(&entry.path()));
            (name, state)
        })
        .collect()
}

/// Run `body`, then check that no file in the database's folder changed and
/// none was added: the database's bytes and time, and any `-wal`'s too.
fn unchanged<T>(db: &TempDb, body: impl FnOnce() -> T) -> T {
    let before = folder_state(db);
    let result = body();
    assert_eq!(folder_state(db), before, "the folder changed");
    result
}

// ---- Running the program.

fn command(db: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .arg("--db")
        .arg(db)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY");
    command
}

fn run(db: &Path, args: &[&str]) -> Output {
    command(db, args).output().expect("run the program")
}

/// The JSON of a subcommand that succeeds, which is all it prints on stdout.
fn answer(db: &Path, args: &[&str]) -> Value {
    let output = run(db, args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "{args:?}: nothing on stderr");
    serde_json::from_slice(&output.stdout).expect("JSON on stdout")
}

/// The error of a subcommand that fails: plain text on stderr, nothing on
/// stdout, a non-zero exit.
fn refusal(db: &Path, args: &[&str]) -> String {
    let output = run(db, args);
    assert_eq!(output.status.code(), Some(1), "{args:?}");
    assert!(output.stdout.is_empty(), "{args:?}: nothing on stdout");
    String::from_utf8(output.stderr).expect("UTF-8 on stderr")
}

/// A closed database with some of the reader's data in it, as the app leaves
/// one on quitting: no `-wal`, no `-shm`.
async fn closed_install(label: &str) -> TempDb {
    let (db, pool) = fresh_database(label).await;
    add_readers_data(&pool).await;
    pool.close().await;
    let files = folder_state(&db);
    assert_eq!(files.len(), 1, "{:?}", files.keys());
    db
}

async fn add_readers_data(pool: &Db) {
    db::bookmarks::add_bookmark(pool.write(), JOHN, 3, 16, 17)
        .await
        .expect("a bookmark");
    db::highlights::set_highlights(pool.write(), JOHN, 3, 5, 5, HighlightColor::Yellow)
        .await
        .expect("a highlight");
    let passage = Passage {
        book_id: 50,
        chapter: 4,
        verse_start: 4,
        verse_end: 4,
    };
    db::notes::add_note(pool.write(), &passage, "Rejoice, always.", None)
        .await
        .expect("a note");
}

// ---- Subcommands.

/// The verses, exactly, in all three translations; the KJV's markup read as
/// Copy reads it. The shape is pinned once in full.
#[tokio::test]
async fn subcommands_give_the_exact_text() {
    let db = closed_install("e2e_text").await;
    let path = db.path();
    unchanged(&db, || {
        assert_eq!(
            answer(&path, &["passage", "John 3:16", "--translation", "kjv"]),
            json!({
                "reference": "John 3:16",
                "translation": "kjv",
                "translation_name": "King James Version",
                "book": "John",
                "chapter": 3,
                "verses": [{"verse": 16, "text": JOHN_3_16_KJV}],
            })
        );
        for (code, text) in [("web", JOHN_3_16_WEB), ("bsb", JOHN_3_16_BSB)] {
            let passage = answer(&path, &["get_passage", "John 3:16", "--translation", code]);
            assert_eq!(passage["verses"][0]["text"], text);
        }
        let supplied = answer(&path, &["passage", "John 3:5", "--translation", "kjv"]);
        assert_eq!(supplied["verses"][0]["text"], JOHN_3_5_KJV);
        assert!(supplied["verses"][0].get("footnotes").is_none());

        let noted = answer(&path, &["passage", "Psalm 119:105", "--translation", "kjv"]);
        assert_eq!(noted["verses"][0]["text"], PSALM_119_105_KJV);
        assert_eq!(
            noted["verses"][0]["footnotes"],
            json!([{"label": "a", "text": "lamp: or, candle"}])
        );
    });
}

/// Every tool, under its own name and its short one, gives its JSON.
#[tokio::test]
async fn every_subcommand_answers_in_json() {
    let db = closed_install("e2e_tools").await;
    let path = db.path();
    unchanged(&db, || {
        let translations = answer(&path, &["list_translations"]);
        assert_eq!(translations["default"], "kjv");
        assert_eq!(answer(&path, &["translations"]), translations);

        let found = answer(
            &path,
            &["search", "grace", "--book", "Romans", "--limit", "5"],
        );
        assert_eq!(
            (&found["book"], &found["limit"]),
            (&json!("Romans"), &json!(5))
        );
        assert_eq!(found["results"].as_array().map(Vec::len), Some(5));
        assert!(found["total"].as_i64() > Some(5));

        let compared = answer(
            &path,
            &["compare", "John 3:16", "--translations", "web,kjv"],
        );
        assert_eq!(compared["columns"][0]["verses"][0]["text"], JOHN_3_16_WEB);
        assert_eq!(compared["columns"][1]["verses"][0]["text"], JOHN_3_16_KJV);

        let xrefs = answer(&path, &["get_cross_references", "John 3:16"]);
        assert!(xrefs["total"].as_i64() > Some(0));
        assert_eq!(answer(&path, &["cross_references", "John 3:16"]), xrefs);

        let place = answer(&path, &["get_reading_place"]);
        assert_eq!(place, json!({"saved": false}));
        assert_eq!(answer(&path, &["reading_place"]), place);

        let bookmarks = answer(&path, &["list_bookmarks"]);
        assert_eq!(bookmarks["total"], 1);
        assert_eq!(
            bookmarks["bookmarks"][0]["reference"],
            "John 3:16\u{2013}17"
        );
        assert_eq!(
            bookmarks["bookmarks"][0]["verses"][0],
            json!({"verse": 16, "text": JOHN_3_16_KJV})
        );
        assert_eq!(answer(&path, &["bookmarks"]), bookmarks);

        let highlights = answer(&path, &["list_highlights", "--color", "yellow"]);
        assert_eq!(
            highlights["highlights"][0]["verses"][0]["text"],
            JOHN_3_5_KJV
        );
        assert_eq!(
            answer(&path, &["highlights", "--color", "pink"])["total"],
            0
        );

        let notes = answer(&path, &["notes", "--book", "Philippians"]);
        assert_eq!(notes["notes"][0]["source"], "reader_note");
        assert_eq!(notes["notes"][0]["body"], "Rejoice, always.");
        assert_eq!(notes["notes"][0]["reference"], "Philippians 4:4");
        assert_eq!(
            answer(&path, &["list_notes", "--contains", "REJOICE"])["total"],
            1
        );
        assert_eq!(answer(&path, &["list_notes", "--book", "John"])["total"], 0);
    });
}

/// `verify_quote` under both its names: exact, a difference, and what's nowhere.
#[tokio::test]
async fn verify_quote_checks_a_quotation_character_for_character() {
    let db = closed_install("e2e_verify").await;
    let path = db.path();
    unchanged(&db, || {
        let exact = &[
            "verify_quote",
            JOHN_3_16_KJV,
            "--reference",
            "John 3:16",
            "--translation",
            "kjv",
        ];
        assert_eq!(
            answer(&path, exact),
            json!({
                "status": "exact",
                "translation": "kjv",
                "reference": "John 3:16",
                "text": JOHN_3_16_KJV,
                "message": "This is exactly John 3:16 in the KJV.",
            })
        );
        let mut kebab = exact.to_vec();
        kebab[0] = "verify-quote";
        assert_eq!(answer(&path, &kebab), answer(&path, exact));

        let found = answer(
            &path,
            &[
                "verify_quote",
                "For God so loved the world",
                "--translation",
                "web",
            ],
        );
        assert_eq!(found["status"], "exact_part");
        assert_eq!(found["reference"], "John 3:16");
        assert_eq!(found["text"], JOHN_3_16_WEB);

        let misquoted = JOHN_3_16_KJV.replacen("loved", "lovad", 1);
        let differs = answer(
            &path,
            &[
                "verify_quote",
                &misquoted,
                "--reference",
                "John 3:16",
                "--translation",
                "kjv",
            ],
        );
        assert_eq!(differs["status"], "differs");
        assert_eq!(differs["text"], JOHN_3_16_KJV);
        assert_eq!(differs["difference"]["position"], 15);
        assert_eq!(differs["difference"]["word"], 4);
        assert!(differs["difference"].get("note").is_none(), "{differs}");

        // A no-break space typed as a space: still a difference, explained.
        let passage = answer(&path, &["passage", "Genesis 22:16", "--translation", "web"]);
        let verse = passage["verses"][0]["text"].as_str().unwrap();
        let plain = verse.replace('\u{a0}', " ");
        let noted = answer(
            &path,
            &[
                "verify_quote",
                &plain,
                "--reference",
                "Genesis 22:16",
                "--translation",
                "web",
            ],
        );
        assert_eq!(noted["status"], "differs");
        assert_eq!(noted["text"], verse);
        let note = "The text has a no-break space (U+00A0) where the quote has a space (U+0020).";
        assert_eq!(noted["difference"]["note"], note);
        assert!(
            noted["message"].as_str().unwrap().ends_with(note),
            "{noted}"
        );

        let elsewhere = answer(
            &path,
            &["verify_quote", JOHN_3_5_KJV, "--reference", "John 3:16"],
        );
        assert_eq!(elsewhere["status"], "elsewhere");
        assert_eq!(elsewhere["reference"], "John 3:16");
        assert_eq!(elsewhere["found_at"][0]["reference"], "John 3:5");
        assert_eq!(elsewhere["found_at"][0]["text"], JOHN_3_5_KJV);

        assert_eq!(
            answer(&path, &["verify_quote", "Blessed are the pickle makers"]),
            json!({
                "status": "not_found",
                "translation": "kjv",
                "message": "This text isn't in the KJV as quoted.",
            })
        );
    });
}

/// Every subcommand under its kebab-case names, the same as under its own.
#[tokio::test]
async fn kebab_case_names_are_the_same_subcommands() {
    let db = closed_install("e2e_kebab").await;
    let path = db.path();
    unchanged(&db, || {
        for (name, kebabs, rest) in [
            ("list_translations", &["list-translations"][..], &[][..]),
            ("get_passage", &["get-passage"], &["John 3:16"]),
            ("get_passages", &["get-passages"], &["John 3:16", "Rom 5:8"]),
            (
                "get_cross_references",
                &["get-cross-references", "cross-references"],
                &["John 3:16"],
            ),
            (
                "get_reading_place",
                &["get-reading-place", "reading-place"],
                &[],
            ),
            ("list_bookmarks", &["list-bookmarks"], &[]),
            ("list_highlights", &["list-highlights"], &[]),
            ("list_notes", &["list-notes"], &[]),
            ("verify_quote", &["verify-quote"], &["Jesus wept."]),
        ] {
            let mut args = vec![name];
            args.extend_from_slice(rest);
            let own = answer(&path, &args);
            for kebab in kebabs {
                args[0] = kebab;
                assert_eq!(answer(&path, &args), own, "{kebab}");
            }
        }
    });
}

/// Several references in one, in either front end, are `get_passages`'s answer;
/// one is `get_passage`'s, however it's written.
#[tokio::test]
async fn get_passage_takes_several_references_in_both_front_ends() {
    let db = closed_install("e2e_passage_list").await;
    let path = db.path();
    unchanged(&db, || {
        let many = answer(
            &path,
            &[
                "get_passages",
                "John 3:16",
                "Rom 5:8",
                "--translation",
                "kjv",
            ],
        );
        assert_eq!(many["passages"][0]["verses"][0]["text"], JOHN_3_16_KJV);
        assert_eq!(many["verse_count"], 2);
        for list in [
            "John 3:16, Rom 5:8",
            "John 3:16; Rom 5:8",
            "John 3:16,Rom 5:8",
        ] {
            let args = ["passage", list, "--translation", "kjv"];
            assert_eq!(answer(&path, &args), many, "{list}");
        }
        assert_eq!(
            answer(&path, &["passage", "John 3:16,", "--translation", "kjv"]),
            answer(&path, &["passage", "John 3:16", "--translation", "kjv"]),
        );
        let refused = refusal(&path, &["passage", "Psalm 119; Psalm 119; Psalm 119"]);
        assert_eq!(
            refused,
            "That's 528 verses; at most 500 per call. Split it.\n"
        );

        let mut mcp = Mcp::start(&path);
        mcp.initialize();
        let arguments = json!({"reference": "John 3:16, Rom 5:8", "translation": "kjv"});
        assert_eq!(mcp.call_for_json(2, "get_passage", arguments), many);
        let arguments = json!({"reference": "John 3:16, 18", "translation": "kjv"});
        assert_eq!(
            mcp.call_for_json(3, "get_passage", arguments),
            answer(&path, &["passage", "John 3:16, 18", "--translation", "kjv"]),
        );
        mcp.finish();
    });
}

/// `get_passages` under all its names: the passages in the order asked, as
/// `get_passage` gives each, one that fails in its place.
#[tokio::test]
async fn get_passages_gives_many_in_one_call() {
    let db = closed_install("e2e_passages").await;
    let path = db.path();
    unchanged(&db, || {
        let asked = [
            "get_passages",
            "John 3:16",
            "Hezekiah 3:16",
            "Psalm 119:105",
            "--translation",
            "kjv",
        ];
        let many = answer(&path, &asked);
        assert_eq!(many["translation"], "kjv");
        assert_eq!(many["translation_name"], "King James Version");
        assert_eq!(many["verse_count"], 2);
        assert_eq!(
            many["passages"][0],
            answer(&path, &["get_passage", "John 3:16", "--translation", "kjv"])
        );
        assert_eq!(many["passages"][0]["verses"][0]["text"], JOHN_3_16_KJV);
        assert_eq!(
            many["passages"][1],
            json!({
                "reference": "Hezekiah 3:16",
                "error": "There's no book called \u{201c}Hezekiah\u{201d}.",
            })
        );
        assert_eq!(many["passages"][2]["verses"][0]["text"], PSALM_119_105_KJV);
        for name in ["passages", "get-passages"] {
            let mut args = asked.to_vec();
            args[0] = name;
            assert_eq!(answer(&path, &args), many, "{name}");
        }

        let message = refusal(
            &path,
            &["get_passages", "Psalm 119", "Psalm 119", "Psalm 119"],
        );
        assert_eq!(
            message,
            "That's 528 verses; at most 500 per call. Split it.\n"
        );
    });
}

/// A failure is a sentence on stderr and a non-zero exit, with no JSON.
#[tokio::test]
async fn failures_are_plain_text_and_a_non_zero_exit() {
    let db = closed_install("e2e_failures").await;
    let path = db.path();
    unchanged(&db, || {
        assert!(refusal(&path, &["passage", "Hezekiah 3:16"]).contains("no book called"));
        assert!(refusal(&path, &["passage", "John 3:16", "--translation", "niv"]).contains("niv"));
        assert!(refusal(&path, &["search", "grace", "--book", "Hezekiah"]).contains("no book"));
        assert!(refusal(&path, &["list_highlights", "--color", "mauve"]).contains("mauve"));
        assert!(
            refusal(
                &path,
                &[
                    "verify_quote",
                    "Jesus wept.",
                    "--reference",
                    "Hezekiah 3:16"
                ]
            )
            .contains("no book called")
        );
        assert!(refusal(&path, &["verify_quote", "  "]).contains("empty"));
    });
}

// ---- The MCP server.

/// A running MCP server, and its stdout line by line.
struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
}

impl Mcp {
    fn start(db: &Path) -> Self {
        let mut child = command(db, &["mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the server");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("the server's stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin,
            lines,
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("the server's stdin");
        writeln!(stdin, "{message}").expect("write to the server");
        stdin.flush().expect("flush to the server");
    }

    /// Ask, and wait for the answer. Every line on stdout must be a protocol
    /// message, whatever else comes before the answer.
    fn ask(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(ANSWER_TIMEOUT)
                .expect("the server answers");
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("stdout carries only the protocol: {line:?}: {e}"));
            assert_eq!(message["jsonrpc"], "2.0", "{line}");
            if message["id"] == id {
                assert!(message.get("error").is_none(), "{method}: {message}");
                return message["result"].clone();
            }
        }
    }

    /// Ask what the server refuses, and read its protocol error.
    fn ask_error(&mut self, id: i64, method: &str, params: Value) -> Value {
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(ANSWER_TIMEOUT)
                .expect("the server answers");
            let message: Value = serde_json::from_str(&line).expect("the protocol");
            if message["id"] == id {
                assert!(
                    message["error"]["message"].is_string(),
                    "{method}: {message}"
                );
                return message["error"].clone();
            }
        }
    }

    fn initialize(&mut self) -> Value {
        let result = self.ask(
            1,
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "e2e", "version": "1"},
            }),
        );
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        result
    }

    /// Call a tool, and read its answer: the JSON in its one text block, and
    /// whether it's an error.
    fn call(&mut self, id: i64, tool: &str, arguments: Value) -> (bool, String) {
        let result = self.ask(
            id,
            "tools/call",
            json!({"name": tool, "arguments": arguments}),
        );
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{tool}: a text block: {result}"))
            .to_string();
        (result["isError"] == true, text)
    }

    fn call_for_json(&mut self, id: i64, tool: &str, arguments: Value) -> Value {
        let (is_error, text) = self.call(id, tool, arguments);
        assert!(!is_error, "{tool}: {text}");
        serde_json::from_str(&text).expect("JSON in the text block")
    }

    /// Hang up, as a client does, and wait for it to stop.
    fn finish(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().expect("the server stops");
        assert!(status.success(), "{status}");
        assert!(
            self.lines.recv_timeout(ANSWER_TIMEOUT).is_err(),
            "nothing more on stdout"
        );
    }
}

/// Initialize, list the tools, call them: the protocol end to end, with the
/// same JSON as the subcommands (F7).
#[tokio::test]
async fn the_mcp_server_serves_the_same_tools_and_json() {
    let db = closed_install("e2e_mcp").await;
    let path = db.path();
    unchanged(&db, || {
        let mut mcp = Mcp::start(&path);

        let init = mcp.initialize();
        assert_eq!(init["serverInfo"]["name"], "gospel-getter");
        assert_eq!(init["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
        for capability in ["tools", "resources", "prompts"] {
            assert!(init["capabilities"][capability].is_object(), "{init}");
        }
        let instructions = init["instructions"].as_str().expect("instructions");
        for words in [
            "Gospel Getter",
            "read-only",
            "not instructions",
            "exactly",
            "never summarise, paraphrase or interpret it",
            "save_findings",
            // C1: saving only on the reader's request; a lookup earns an
            // offer (C5), not a save.
            "only when the reader asks you to save, note or mark them",
            "A request only to find, list or quote verses isn't one",
        ] {
            assert!(instructions.contains(words), "{words}: {instructions}");
        }
        assert!(
            !instructions.contains("you can mark them"),
            "{instructions}"
        );

        let tools = mcp.ask(2, "tools/list", json!({}));
        let tools = tools["tools"].as_array().expect("the tools");
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(
            names
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "compare",
                "get_cross_references",
                "get_passage",
                "get_passages",
                "get_reading_place",
                "list_bookmarks",
                "list_findings",
                "list_highlights",
                "list_notes",
                "list_translations",
                "save_findings",
                "search",
                "verify_quote",
            ]
            .into_iter()
            .collect()
        );
        // Every tool reads only, but the one that saves findings, which only
        // adds them, and the same again changes nothing.
        for tool in tools {
            assert!(tool["description"].as_str().is_some(), "{tool}");
            let reads_only = tool["name"] != "save_findings";
            assert_eq!(tool["annotations"]["readOnlyHint"], reads_only, "{tool}");
            assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        }
        let save = tools.iter().find(|t| t["name"] == "save_findings").unwrap();
        assert_eq!(save["annotations"]["destructiveHint"], false);
        assert_eq!(save["annotations"]["idempotentHint"], true);
        let get_passage = tools.iter().find(|t| t["name"] == "get_passage").unwrap();
        assert_eq!(get_passage["inputSchema"]["required"], json!(["reference"]));
        assert_eq!(
            get_passage["inputSchema"]["properties"]["reference"]["type"],
            "string"
        );

        // The arguments are the subcommand's, and its description says what each
        // status means and the limit.
        let verify_quote = tools.iter().find(|t| t["name"] == "verify_quote").unwrap();
        let arguments = |tool: &Value| -> Vec<String> {
            let properties = tool["inputSchema"]["properties"].as_object().unwrap();
            properties.keys().cloned().collect()
        };
        assert_eq!(
            arguments(verify_quote),
            ["reference", "text", "translation"]
        );
        assert_eq!(verify_quote["inputSchema"]["required"], json!(["text"]));
        let help = run(&path, &["verify_quote", "--help"]);
        let help = String::from_utf8_lossy(&help.stdout);
        for flag in ["<TEXT>", "--reference", "--translation"] {
            assert!(help.contains(flag), "{flag}: {help}");
        }
        let description = verify_quote["description"].as_str().unwrap();
        for words in [
            "\"exact\"",
            "\"exact_part\"",
            "\"differs\"",
            "\"elsewhere\"",
            "\"not_found\"",
            "character for character",
            "whitespace around the whole quote is trimmed",
            "only look alike",
            "10,000",
        ] {
            assert!(description.contains(words), "{words}: {description}");
        }

        let get_passages = tools.iter().find(|t| t["name"] == "get_passages").unwrap();
        assert_eq!(arguments(get_passages), ["references", "translation"]);
        assert_eq!(
            get_passages["inputSchema"]["required"],
            json!(["references"])
        );
        let help = run(&path, &["get_passages", "--help"]);
        let help = String::from_utf8_lossy(&help.stdout);
        for flag in ["<REF>...", "--translation"] {
            assert!(help.contains(flag), "{flag}: {help}");
        }
        let description = get_passages["description"].as_str().unwrap();
        for words in ["50 references", "500 verses", "{reference, error}"] {
            assert!(description.contains(words), "{words}: {description}");
        }

        let passage = mcp.call_for_json(
            3,
            "get_passage",
            json!({"reference": "John 3:16", "translation": "kjv"}),
        );
        assert_eq!(passage["verses"][0]["text"], JOHN_3_16_KJV);

        let found = mcp.call_for_json(
            4,
            "search",
            json!({"query": "\"God so loved the world\"", "translation": "bsb"}),
        );
        assert_eq!(found["total"], 1);
        assert_eq!(found["results"][0]["text"], JOHN_3_16_BSB);

        let notes = mcp.call_for_json(5, "list_notes", json!({"book": "Philippians"}));
        assert_eq!(notes["notes"][0]["source"], "reader_note");
        assert_eq!(notes["notes"][0]["body"], "Rejoice, always.");

        // The same arguments, the same JSON, whichever way it's asked.
        let same: [(&str, Value, &[&str]); 12] = [
            (
                "get_passage",
                json!({"reference": "Psalm 119:105", "translation": "kjv"}),
                &["get_passage", "Psalm 119:105", "--translation", "kjv"],
            ),
            (
                "search",
                json!({"query": "grace", "book": "Romans", "limit": 3}),
                &["search", "grace", "--book", "Romans", "--limit", "3"],
            ),
            (
                "compare",
                json!({"reference": "John 3:16", "translations": ["web", "bsb"]}),
                &["compare", "John 3:16", "--translations", "web,bsb"],
            ),
            (
                "get_cross_references",
                json!({"reference": "John 3:16"}),
                &["get_cross_references", "John 3:16"],
            ),
            ("list_translations", json!({}), &["list_translations"]),
            ("get_reading_place", json!({}), &["get_reading_place"]),
            (
                "list_highlights",
                json!({"color": "yellow", "offset": 0, "limit": 5}),
                &[
                    "list_highlights",
                    "--color",
                    "yellow",
                    "--offset",
                    "0",
                    "--limit",
                    "5",
                ],
            ),
            (
                "get_passages",
                json!({"references": ["John 3:16", "Hezekiah 3:16"], "translation": "web"}),
                &[
                    "get_passages",
                    "John 3:16",
                    "Hezekiah 3:16",
                    "--translation",
                    "web",
                ],
            ),
            (
                "verify_quote",
                json!({"text": JOHN_3_16_BSB, "reference": "John 3:16", "translation": "bsb"}),
                &[
                    "verify_quote",
                    JOHN_3_16_BSB,
                    "--reference",
                    "John 3:16",
                    "--translation",
                    "bsb",
                ],
            ),
            (
                "verify_quote",
                json!({"text": "for god so loved", "reference": "John 3:16"}),
                &[
                    "verify_quote",
                    "for god so loved",
                    "--reference",
                    "John 3:16",
                ],
            ),
            (
                "verify_quote",
                json!({"text": "Remember Lot's wife!", "reference": "Luke 17:32", "translation": "web"}),
                &[
                    "verify_quote",
                    "Remember Lot's wife!",
                    "--reference",
                    "Luke 17:32",
                    "--translation",
                    "web",
                ],
            ),
            (
                "verify_quote",
                json!({"text": "Blessed are the pickle makers"}),
                &["verify_quote", "Blessed are the pickle makers"],
            ),
        ];
        for (i, (tool, arguments, args)) in same.into_iter().enumerate() {
            assert_eq!(
                mcp.call_for_json(10 + i as i64, tool, arguments),
                answer(&path, args),
                "{tool}"
            );
        }

        // A failure is the tool's error, in the subcommand's words.
        let (is_error, text) = mcp.call(40, "get_passage", json!({"reference": "Hezekiah 3:16"}));
        assert!(is_error);
        assert_eq!(
            format!("{text}\n"),
            refusal(&path, &["get_passage", "Hezekiah 3:16"])
        );
        let (is_error, text) = mcp.call(41, "verify_quote", json!({"text": ""}));
        assert!(is_error);
        assert_eq!(format!("{text}\n"), refusal(&path, &["verify_quote", ""]));
        let refs = json!({"references": ["Psalm 119", "Psalm 119", "Psalm 119"]});
        let (is_error, text) = mcp.call(43, "get_passages", refs);
        assert!(is_error);
        assert_eq!(
            format!("{text}\n"),
            refusal(
                &path,
                &["get_passages", "Psalm 119", "Psalm 119", "Psalm 119"]
            )
        );

        // No such argument: the server refuses, typed as it is.
        mcp.send(&json!({
            "jsonrpc": "2.0",
            "id": 44,
            "method": "tools/call",
            "params": {"name": "get_passage", "arguments": {"reference": "John 3:16", "body": "x"}},
        }));
        let line = mcp.lines.recv_timeout(ANSWER_TIMEOUT).expect("an answer");
        let refused: Value = serde_json::from_str(&line).expect("JSON");
        assert!(
            refused.get("error").is_some() || refused["result"]["isError"] == true,
            "{refused}"
        );

        mcp.finish();
    });
}

/// A database that can't be read: the server says so on stderr and stops
/// before any protocol, so a client's log shows why.
#[tokio::test]
async fn the_mcp_server_refuses_to_start_on_an_install_not_ready() {
    let db = TempDb::new("e2e_mcp_missing");
    let mut child = command(&db.path(), &["mcp"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the server");
    let stdout = child.stdout.take().expect("stdout");
    let (sender, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let output = child.wait_with_output();
        let _ = sender.send(output);
    });
    let output = finished
        .recv_timeout(ANSWER_TIMEOUT)
        .expect("it stops")
        .expect("its output");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Open Gospel Getter once first"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        BufReader::new(stdout).lines().count(),
        0,
        "nothing on stdout"
    );
    assert!(!db.path().exists());
}

// ---- AI-flagged findings: the one thing either front end writes.

/// Every row of the reader's own tables, as text: what saving findings must
/// never change.
async fn readers_own(path: &Path) -> Vec<String> {
    let pool = db::open_read_only(path).await.expect("open read-only");
    let mut rows = Vec::new();
    for sql in [
        "SELECT 'note ' || id || '|' || book_id || '|' || chapter || '|' || verse_start || '|' \
         || verse_end || '|' || body || '|' || created_at || '|' || updated_at FROM notes",
        "SELECT 'bookmark ' || id || '|' || book_id || '|' || chapter || '|' || verse_start \
         || '|' || verse_end || '|' || created_at FROM bookmarks",
        "SELECT 'highlight ' || book_id || '|' || chapter || '|' || verse || '|' || color \
         || '|' || created_at || '|' || updated_at FROM highlights",
    ] {
        let found: Vec<String> = sqlx::query_scalar(sql)
            .fetch_all(&pool)
            .await
            .expect("the reader's rows");
        rows.extend(found);
    }
    pool.close().await;
    rows.sort();
    rows
}

/// The reader turns on "Let AI agents save findings", in the app.
async fn turn_findings_on(db: &TempDb) {
    let pool = db.connect().await;
    db::findings::set_findings_enabled(pool.write(), true)
        .await
        .expect("turn it on");
    pool.close().await;
}

/// Off until the reader turns it on (C6), and still there: both front ends
/// say how, and nothing changes.
#[tokio::test]
async fn saving_findings_while_off_says_how_to_turn_it_on() {
    let db = closed_install("e2e_findings_off").await;
    let path = db.path();
    unchanged(&db, || {
        let message = refusal(
            &path,
            &["save-findings", "--request", "grief", "John 11:35"],
        );
        assert_eq!(message, format!("{}\n", db::findings::HOW_TO_TURN_ON));
        for words in ["Aa", "AI agents", "Let AI agents save findings"] {
            assert!(message.contains(words), "{words}: {message}");
        }
        let mut mcp = Mcp::start(&path);
        mcp.initialize();
        let (is_error, text) = mcp.call(
            2,
            "save_findings",
            json!({"request": "grief", "references": ["John 11:35"]}),
        );
        assert!(is_error);
        assert_eq!(format!("{text}\n"), message);
        let listed = mcp.call_for_json(3, "list_findings", json!({}));
        assert_eq!(listed["saving_enabled"], false);
        assert_eq!(listed["total"], 0);
        mcp.finish();
    });
}

/// Turned on: one finding per verse, what was saved, what already was and
/// what couldn't be read; over the cap, nothing; the reader's own notes,
/// bookmarks and highlights never touched.
#[tokio::test]
async fn saving_findings_says_what_was_saved_already_there_or_not_read() {
    let db = closed_install("e2e_findings_on").await;
    turn_findings_on(&db).await;
    let path = db.path();
    let own = readers_own(&path).await;

    let saved = answer(
        &path,
        &[
            "save-findings",
            "--request",
            " grief ",
            "--agent",
            "Claude",
            "--translation",
            "kjv",
            "John 11:33-35",
            "Hezekiah 3:16",
            "John 11:35",
        ],
    );
    assert_eq!(saved["request"], "grief");
    assert_eq!(saved["agent"], "Claude");
    assert_eq!(saved["translation"], "kjv");
    assert_eq!(
        (saved["saved"].as_u64(), saved["already_saved"].as_u64()),
        (Some(3), Some(0))
    );
    assert_eq!(
        saved["verses"],
        json!([
            {"reference": "John 11:33", "status": "saved"},
            {"reference": "John 11:34", "status": "saved"},
            {"reference": "John 11:35", "status": "saved"},
        ])
    );
    assert_eq!(saved["errors"][0]["reference"], "Hezekiah 3:16");
    let error = saved["errors"][0]["error"].as_str().unwrap();
    assert!(error.contains("Hezekiah"), "{error}");
    let message = saved["message"].as_str().unwrap();
    assert!(
        message.starts_with("Saved 3 findings for \u{201c}grief\u{201d}."),
        "{message}"
    );

    let again = answer(
        &path,
        &[
            "save_findings",
            "--request",
            "grief",
            "John 11:35",
            "Job 2:13",
        ],
    );
    assert_eq!(
        (again["saved"].as_u64(), again["already_saved"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(again["agent"], "AI agent", "no client, no --agent");
    assert_eq!(
        again["verses"],
        json!([
            {"reference": "John 11:35", "status": "already_saved"},
            {"reference": "Job 2:13", "status": "saved"},
        ])
    );
    assert!(again.get("errors").is_none());

    // More than 500 verses: refused whole, nothing saved.
    let chapters = [
        "Psalm 119",
        "Numbers 7",
        "Genesis 24",
        "Luke 1",
        "Psalm 78",
        "1 Chronicles 6",
    ];
    let mut args = vec!["save-findings", "--request", "everything"];
    args.extend(chapters);
    let refused = refusal(&path, &args);
    assert!(refused.contains("at most 500"), "{refused}");

    let listed = answer(&path, &["list-findings", "--request", "GRIEF"]);
    assert_eq!(listed["saving_enabled"], true);
    assert_eq!(listed["total"], 4);
    assert_eq!(
        listed["findings"][0]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        [
            "agent",
            "book",
            "chapter",
            "created_at",
            "id",
            "reference",
            "request",
            "source",
            "verse_end",
            "verse_start"
        ],
        "a marker only: no text"
    );
    let references: Vec<&str> = listed["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["reference"].as_str().unwrap())
        .collect();
    assert_eq!(
        references,
        ["Job 2:13", "John 11:33", "John 11:34", "John 11:35"]
    );
    assert_eq!(listed["findings"][0]["source"], "ai_finding");
    assert_eq!(answer(&path, &["findings", "--book", "Job"])["total"], 1);
    assert_eq!(
        answer(&path, &["findings", "--request", "everything"])["total"],
        0
    );

    assert_eq!(
        readers_own(&path).await,
        own,
        "the reader's own data is untouched"
    );
}

/// There are at most 10,000 findings in all (`MAX_FINDINGS`). A save that
/// would pass the limit saves none of its verses, and both front ends say
/// so plainly; verses already saved for the request add nothing, so they're
/// answered as before even at the limit.
#[tokio::test]
async fn saving_past_the_findings_limit_says_so_and_saves_nothing() {
    let (db, pool) = fresh_database("e2e_findings_limit").await;
    // On John 11:35, each for a request of its own.
    gospel_getter_core::db::test_support::fill_findings(
        pool.write(),
        db::findings::MAX_FINDINGS - 1,
    )
    .await;
    db::findings::set_findings_enabled(pool.write(), true)
        .await
        .expect("turn it on");
    pool.close().await;
    let path = db.path();
    let own = readers_own(&path).await;
    let limit = "Gospel Getter holds at most 10,000 AI findings; remove some in the Library first.";

    // Two new verses would be one over: neither is saved.
    let message = refusal(
        &path,
        &["save-findings", "--request", "grief", "John 11:33-34"],
    );
    assert_eq!(message, format!("{limit}\n"));
    assert_eq!(
        answer(&path, &["list-findings", "--request", "grief"])["total"],
        0
    );
    // One reaches it; the same again is already saved.
    let saved = answer(
        &path,
        &["save-findings", "--request", "grief", "John 11:33"],
    );
    assert_eq!(saved["saved"], 1);
    let again = answer(
        &path,
        &["save-findings", "--request", "grief", "John 11:33"],
    );
    assert_eq!(
        (again["saved"].as_u64(), again["already_saved"].as_u64()),
        (Some(0), Some(1))
    );

    // At the limit, a new one is refused through MCP too.
    let mut mcp = Mcp::start(&path);
    mcp.initialize();
    let (is_error, text) = mcp.call(
        2,
        "save_findings",
        json!({"request": "tears", "references": ["John 11:35"]}),
    );
    assert!(is_error);
    assert_eq!(text, limit);
    let listed = mcp.call_for_json(3, "list_findings", json!({"request": "tears"}));
    assert_eq!(listed["total"], 0, "nothing saved");
    mcp.finish();
    assert_eq!(
        readers_own(&path).await,
        own,
        "the reader's own data is untouched"
    );
}

/// A request is a short topic label, never room for words about a verse
/// (C3): one with commentary in it, or too long, is refused whole in both
/// front ends, with what a request is, and nothing is saved.
#[tokio::test]
async fn a_request_that_is_more_than_a_topic_label_is_refused() {
    let db = closed_install("e2e_findings_label").await;
    turn_findings_on(&db).await;
    let path = db.path();
    let too_long = "r".repeat(61);
    let label = "The request is a short topic label, such as \u{201c}grief\u{201d} or \u{201c}the sun\u{201d}";
    unchanged(&db, || {
        for request in [
            "grief: here Jesus shows that he shares our sorrow",
            "grief; and what it means",
            "grief \u{2014} his tears for Lazarus",
            "grief \u{2013} loss",
            too_long.as_str(),
        ] {
            let message = refusal(
                &path,
                &["save-findings", "--request", request, "John 11:35"],
            );
            assert!(message.starts_with(label), "{request}: {message}");
        }
        let mut mcp = Mcp::start(&path);
        mcp.initialize();
        let (is_error, text) = mcp.call(
            2,
            "save_findings",
            json!({"request": "grief\nJesus wept because…", "references": ["John 11:35"]}),
        );
        assert!(is_error);
        assert!(text.starts_with(label), "{text}");
        let listed = mcp.call_for_json(3, "list_findings", json!({}));
        assert_eq!(listed["total"], 0, "nothing saved");
        mcp.finish();
    });
}

/// An install the app hasn't opened since 2.25.x has no findings' tables:
/// both tools say to open the new app once, and nothing is made.
#[tokio::test]
async fn an_install_without_the_findings_tables_says_to_open_the_app_once() {
    let (db, pool) = fresh_database("e2e_findings_old").await;
    for table in ["ai_findings", "app_settings"] {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE {table}")))
            .execute(pool.write().pool())
            .await
            .expect("an install from before findings");
    }
    pool.close().await;
    let path = db.path();
    unchanged(&db, || {
        for args in [
            &["save-findings", "--request", "grief", "John 11:35"][..],
            &["list-findings"][..],
        ] {
            assert_eq!(
                refusal(&path, args),
                "Open Gospel Getter 2.26.0 or newer once, then try again.\n",
                "{args:?}"
            );
        }
        // Everything else reads as ever.
        let passage = answer(&path, &["passage", "John 3:16", "--translation", "kjv"]);
        assert_eq!(passage["verses"][0]["text"], JOHN_3_16_KJV);
    });
}

/// Over MCP: the client's name is the agent's when it gives none, the two
/// tools mirror the subcommands, and the description says what saving is
/// for, that it needs the reader to ask for it, and what it never does (C5,
/// C1, C3).
#[tokio::test]
async fn the_mcp_server_saves_findings_in_the_clients_name() {
    let db = closed_install("e2e_mcp_findings").await;
    turn_findings_on(&db).await;
    let path = db.path();
    let own = readers_own(&path).await;
    let mut mcp = Mcp::start(&path);
    mcp.initialize();

    let tools = mcp.ask(2, "tools/list", json!({}));
    let save = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "save_findings")
        .unwrap()
        .clone();
    let description = save["description"].as_str().unwrap();
    for words in [
        "note every verse that deals with grief so I can read them",
        "only when the reader asks you to save, note or mark verses",
        "A request only to find, list or quote verses isn't one: answer it, and offer to save them",
        "beside the verse",
        "marked as found by you",
        "Save only the locations: never summarise, paraphrase or explain the verses",
        "add-only",
        "Aa, then AI agents",
    ] {
        assert!(description.contains(words), "{words}: {description}");
    }
    let properties = save["inputSchema"]["properties"].as_object().unwrap();
    assert_eq!(
        properties.keys().collect::<Vec<_>>(),
        ["agent", "references", "request", "translation"]
    );
    assert_eq!(
        save["inputSchema"]["required"],
        json!(["request", "references"])
    );

    let saved = mcp.call_for_json(
        3,
        "save_findings",
        json!({"request": "weeping", "references": ["John 11:35"]}),
    );
    assert_eq!(saved["agent"], "e2e", "the client's name");
    assert_eq!(saved["saved"], 1);
    let named = mcp.call_for_json(
        4,
        "save_findings",
        json!({"request": "weeping", "references": ["Luke 19:41"], "agent": "Claude"}),
    );
    assert_eq!(named["agent"], "Claude");
    let listed = mcp.call_for_json(5, "list_findings", json!({"request": "weeping"}));
    assert_eq!(listed["total"], 2);
    mcp.finish();
    assert_eq!(
        listed,
        answer(&path, &["list_findings", "--request", "weeping"])
    );
    assert_eq!(readers_own(&path).await, own);
}

/// The one prompt is the reader's notes, never scripture; each chapter is a
/// resource with exactly `get_passage`'s text; an unknown one, a plain error.
#[tokio::test]
async fn the_mcp_server_offers_the_notes_prompt_and_each_chapter() {
    let db = closed_install("e2e_mcp_extras").await;
    let path = db.path();
    unchanged(&db, || {
        let mut mcp = Mcp::start(&path);
        mcp.initialize();

        let prompts = mcp.ask(2, "prompts/list", json!({}));
        let prompts = prompts["prompts"].as_array().expect("prompts");
        assert_eq!(prompts.len(), 1, "no other prompts: {prompts:?}");
        assert_eq!(prompts[0]["name"], "summarise-my-notes");
        assert_eq!(prompts[0]["arguments"][0]["name"], "book");
        assert_eq!(prompts[0]["arguments"][0]["required"], true);
        let prompt = mcp.ask(
            3,
            "prompts/get",
            json!({"name": "summarise-my-notes", "arguments": {"book": "Phil"}}),
        );
        assert_eq!(prompt["messages"][0]["role"], "user");
        let text = prompt["messages"][0]["content"]["text"].as_str().unwrap();
        for words in [
            "my own notes on Philippians",
            "list_notes",
            "get_passage",
            "never summarise the verses",
        ] {
            assert!(text.contains(words), "{words}: {text}");
        }
        let unknown = mcp.ask_error(
            4,
            "prompts/get",
            json!({"name": "summarise-my-notes", "arguments": {"book": "Hezekiah"}}),
        );
        assert!(
            unknown["message"].as_str().unwrap().contains("Hezekiah"),
            "{unknown}"
        );
        mcp.ask_error(5, "prompts/get", json!({"name": "study-this-passage"}));

        // Every chapter of every translation, once, page by page.
        let codes = answer(&path, &["list_translations"])["translations"]
            .as_array()
            .unwrap()
            .len();
        let mut uris = Vec::new();
        let mut cursor: Option<String> = None;
        for page in 0.. {
            assert!(page < 50, "the pages end");
            let params = cursor.as_ref().map_or(json!({}), |c| json!({"cursor": c}));
            let listed = mcp.ask(100 + page, "resources/list", params);
            for resource in listed["resources"].as_array().unwrap() {
                assert_eq!(resource["mimeType"], "application/json");
                uris.push(resource["uri"].as_str().unwrap().to_string());
            }
            cursor = listed["nextCursor"].as_str().map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(uris.len(), codes * 1189, "every chapter");
        let unique: std::collections::BTreeSet<&String> = uris.iter().collect();
        assert_eq!(unique.len(), uris.len());
        for uri in [
            "gg://kjv/genesis/1",
            "gg://bsb/song-of-solomon/2",
            "gg://web/revelation/22",
        ] {
            assert!(unique.contains(&uri.to_string()), "{uri}");
        }
        let templates = mcp.ask(6, "resources/templates/list", json!({}));
        assert_eq!(
            templates["resourceTemplates"][0]["uriTemplate"],
            "gg://{translation}/{book}/{chapter}"
        );

        // The text is get_passage's, byte for byte.
        for (i, (uri, reference, code)) in [
            ("gg://kjv/john/3", "John 3", "kjv"),
            ("gg://web/1-john/4", "1 John 4", "web"),
            ("gg://bsb/song-of-solomon/2", "Song of Solomon 2", "bsb"),
            ("gg://kjv/psalms/119", "Psalm 119", "kjv"),
        ]
        .into_iter()
        .enumerate()
        {
            let id = 10 + 2 * i as i64;
            let read = mcp.ask(id, "resources/read", json!({"uri": uri}));
            let contents = &read["contents"][0];
            assert_eq!(contents["uri"], uri);
            assert_eq!(contents["mimeType"], "application/json");
            let (is_error, text) = mcp.call(
                id + 1,
                "get_passage",
                json!({"reference": reference, "translation": code}),
            );
            assert!(!is_error, "{text}");
            assert_eq!(contents["text"].as_str().unwrap(), text, "{uri}");
        }
        for (i, bad) in [
            "gg://kjv/hezekiah/1",
            "gg://xyz/john/3",
            "gg://kjv/john/99",
            "gg://kjv/john",
            "https://example.com/kjv/john/3",
        ]
        .into_iter()
        .enumerate()
        {
            let error = mcp.ask_error(30 + i as i64, "resources/read", json!({"uri": bad}));
            assert!(!error["message"].as_str().unwrap().is_empty(), "{bad}");
        }
        mcp.finish();
    });
}

// ---- Beside a running app.

/// The app is running and holds a write transaction, open at this moment. The
/// read doesn't wait, in either front end, and sees what's committed only. Of
/// the app's files, only `-shm` (see `folder_state`) changes.
#[tokio::test]
async fn it_reads_while_the_app_holds_a_write_transaction() {
    let (db, pool) = fresh_database("e2e_open").await;
    add_readers_data(&pool).await;
    let path = db.path();

    let mut uncommitted = begin_write(pool.write()).await.expect("the write lock");
    sqlx::query(
        "INSERT INTO notes (book_id, chapter, verse_start, verse_end, body) \
         VALUES (43, 3, 16, 16, 'Not committed')",
    )
    .execute(&mut *uncommitted)
    .await
    .expect("an uncommitted note");
    sqlx::query("UPDATE books SET name = 'Uncommitted'")
        .execute(&mut *uncommitted)
        .await
        .expect("an uncommitted change");

    unchanged(&db, || {
        let started = std::time::Instant::now();
        let notes = answer(&path, &["list_notes"]);
        assert_eq!(notes["total"], 1);
        assert_eq!(notes["notes"][0]["body"], "Rejoice, always.");
        assert!(started.elapsed() < db::BUSY_TIMEOUT, "it didn't wait");

        let passage = answer(&path, &["passage", "John 3:16", "--translation", "kjv"]);
        assert_eq!(passage["verses"][0]["text"], JOHN_3_16_KJV);
        assert!(answer(&path, &["search", "grace"])["total"].as_i64() > Some(0));

        let mut mcp = Mcp::start(&path);
        mcp.initialize();
        let notes = mcp.call_for_json(2, "list_notes", json!({}));
        assert_eq!(notes["total"], 1);
        mcp.finish();
    });

    uncommitted.rollback().await.expect("roll back");
    pool.close().await;
}

// ---- Installs not ready.

/// No database: a plain message, a non-zero exit, and nothing created, not
/// even the folder it was to be in.
#[test]
fn a_missing_database_says_to_open_the_app() {
    let db = TempDb::new("e2e_missing");
    let folder = db.path().with_file_name("not_yet");
    let path = folder.join("gospel_getter.db");
    for args in [&["list_translations"][..], &["search", "grace"][..]] {
        let message = refusal(&path, args);
        assert!(
            message.contains("Open Gospel Getter once first"),
            "{message}"
        );
    }
    assert!(!folder.exists());
}

/// An older schema: refused, and not migrated.
#[tokio::test]
async fn an_older_schema_says_to_open_the_app() {
    let (db, pool) = fresh_database("e2e_old").await;
    sqlx::query("PRAGMA user_version = 0")
        .execute(pool.write().pool())
        .await
        .expect("an older version");
    pool.close().await;
    let path = db.path();
    unchanged(&db, || {
        for args in [&["passage", "John 3:16"][..], &["search", "grace"][..]] {
            let message = refusal(&path, args);
            assert!(message.contains("older Gospel Getter"), "{message}");
            assert!(
                message.contains("Open Gospel Getter once first"),
                "{message}"
            );
        }
    });
}

/// No search index: only `search` refuses, and the index isn't built.
#[tokio::test]
async fn a_missing_index_stops_only_search() {
    let (db, pool) = prepared_database("e2e_no_index").await;
    pool.close().await;
    let path = db.path();
    unchanged(&db, || {
        let message = refusal(&path, &["search", "grace"]);
        assert!(message.contains("search index"), "{message}");
        assert!(
            message.contains("Open Gospel Getter once first"),
            "{message}"
        );
        let passage = answer(&path, &["passage", "John 3:16", "--translation", "web"]);
        assert_eq!(passage["verses"][0]["text"], JOHN_3_16_WEB);
        assert_eq!(answer(&path, &["list_notes"])["total"], 0);
    });
}

/// No search index: a passage that has the quote is still checked, and what
/// has to search for it refuses, without building the index.
#[tokio::test]
async fn a_missing_index_stops_a_quote_only_where_it_must_search() {
    let (db, pool) = prepared_database("e2e_verify_no_index").await;
    pool.close().await;
    let path = db.path();
    unchanged(&db, || {
        let held = answer(
            &path,
            &["verify_quote", JOHN_3_5_KJV, "--reference", "John 3:5"],
        );
        assert_eq!(held["status"], "exact");
        for args in [
            &["verify_quote", JOHN_3_5_KJV][..],
            &["verify_quote", JOHN_3_5_KJV, "--reference", "John 3:16"][..],
        ] {
            let message = refusal(&path, args);
            assert!(message.contains("search index"), "{message}");
            assert!(
                message.contains("Open Gospel Getter once first"),
                "{message}"
            );
        }
    });
}

/// A file that isn't a database: refused, not overwritten.
#[test]
fn a_file_that_is_not_a_database_is_refused() {
    let db = TempDb::new("e2e_garbage");
    let path = db.path();
    std::fs::write(
        &path,
        "These are not the pages you are looking for, not a database.",
    )
    .expect("write the file");
    unchanged(&db, || {
        let message = refusal(&path, &["list_translations"]);
        assert!(message.contains(&path.display().to_string()), "{message}");
    });
}

// ---- The program itself.

/// All its text comes from the reader's database: the bundled translations
/// (`seed::TEXTS`) aren't in the program, so it stays small and can't answer
/// from a copy of its own.
#[test]
fn the_program_carries_no_bible_text_of_its_own() {
    let program = std::fs::read(BIN).expect("the program");
    let has = |text: &str| program.windows(text.len()).any(|w| w == text.as_bytes());
    assert!(
        has("King James Version"),
        "the list of translations is in it"
    );
    for verse in [JOHN_3_16_KJV, JOHN_3_16_WEB, JOHN_3_16_BSB] {
        assert!(!has(verse), "{verse}");
    }
}

// ---- Finding the database.

/// Without `--db`, it reads the app's database, from the app's data folder.
/// On Linux the data folder can be chosen, so it's tested here for real.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn it_finds_the_apps_database_without_being_told() {
    let db = closed_install("e2e_default_path").await;
    let home = db.path().with_file_name("data_home");
    let folder = home.join("com.tossbaws.gospel-getter");
    std::fs::create_dir_all(&folder).expect("the app's folder");
    std::fs::copy(db.path(), folder.join("gospel_getter.db")).expect("the database");

    let output = Command::new(BIN)
        .args(["passage", "John 3:16", "--translation", "web"])
        .env("XDG_DATA_HOME", &home)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the program");
    assert!(output.status.success(), "{output:?}");
    let passage: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(passage["verses"][0]["text"], JOHN_3_16_WEB);

    let empty = db.path().with_file_name("empty_home");
    let output = Command::new(BIN)
        .arg("list_translations")
        .env("XDG_DATA_HOME", &empty)
        .output()
        .expect("run the program");
    assert_eq!(output.status.code(), Some(1));
    assert!(!empty.exists(), "nothing is created");
}
