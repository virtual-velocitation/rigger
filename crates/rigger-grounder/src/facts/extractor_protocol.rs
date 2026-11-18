//! THE EXTRACTOR PROTOCOL's client (spec 125): the session one declared extractor runs in
//! ([`Session`]), the request line rigger writes it ([`request_line`]) and the rules an answer line
//! is accepted or refused by ([`symbols_answered`]). [`ProjectExtractor`](super::ProjectExtractor)
//! composes them; the facts the accepted symbols become are the parent module's.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

use rigger_domain::contextgraph::reference_candidate_kinds;
use rigger_process::subprocess;
use serde::{Deserialize, Serialize};

use super::CodeSymbols;
use crate::grounder::symbols::model::{self, SymRef};

/// One running declared extractor: the child and the ends of its input and output rigger holds.
#[derive(Debug)]
pub(super) struct Session {
    child: Child,
    requests: ChildStdin,
    answers: BufReader<ChildStdout>,
}

impl Session {
    /// The extractor `run` started in `dir` through the host's shell
    /// ([`subprocess::shell_command`]), its input and output piped to rigger and its stderr
    /// rigger's own; why not when it cannot start.
    pub(super) fn start(run: &str, dir: &Path) -> Result<Self, String> {
        let mut child = subprocess::shell_command(run, dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("start: {e}"))?;
        let requests = child.stdin.take().expect("the extractor's input is piped");
        let answers = child
            .stdout
            .take()
            .expect("the extractor's output is piped");
        Ok(Self {
            child,
            requests,
            answers: BufReader::new(answers),
        })
    }

    /// The extractor's answer to the request line `request`, written and flushed: the next line
    /// of its output, its line ending dropped; why not when the request cannot be written, its
    /// output ends first or the answer is not text.
    pub(super) fn ask(&mut self, request: &str) -> Result<String, String> {
        self.requests
            .write_all(format!("{request}\n").as_bytes())
            .and_then(|()| self.requests.flush())
            .map_err(|e| format!("write the request: {e}"))?;
        let mut answer = String::new();
        match self.answers.read_line(&mut answer) {
            Ok(0) => Err("end of output".to_string()),
            Ok(_) => Ok(answer.trim_end_matches(['\n', '\r']).to_string()),
            Err(e) => Err(format!("read the answer: {e}")),
        }
    }

    /// Ends the session: closes the extractor's input, so one reading it sees its input end, and
    /// its output, so one still writing fails its next write, then waits for it to exit. Its exit
    /// status is no part of the protocol, so a wait that fails leaves nothing more to do.
    pub(super) fn close(self) {
        let Self {
            mut child,
            requests,
            answers,
        } = self;
        drop(requests);
        drop(answers);
        let _ = child.wait();
    }
}

/// One request of THE EXTRACTOR PROTOCOL: the file's root-relative path, `/`-separated, and its
/// text.
#[derive(Serialize)]
struct Request<'a> {
    path: &'a str,
    text: &'a str,
}

/// The request line for the file at `path` holding `text`: one line of JSON, every line break of
/// the text escaped.
pub(super) fn request_line(path: &str, text: &str) -> String {
    serde_json::to_string(&Request { path, text }).expect("two strings serialize")
}

/// One answer of THE EXTRACTOR PROTOCOL as it parses: the path it is for, and the error the
/// extractor met or the file's definitions and references in the bundled extraction's own forms,
/// with whether they cover only part of the file and the lines of the file's own doc comment.
#[derive(Deserialize)]
struct Answer {
    path: String,
    error: Option<String>,
    defs: Option<Vec<model::Def>>,
    refs: Option<Vec<SymRef>>,
    #[serde(default)]
    partial: bool,
    #[serde(default)]
    doc: Option<(u32, u32)>,
}

/// The symbols the answer line `line` gives the request for the file at `path` holding `text`,
/// or why THE EXTRACTOR PROTOCOL refuses it. Checked in order: the line parses as an answer; it is
/// for `path`; it holds no error; it holds both lists; the file's doc span, when it has one, is
/// within the text's lines ([`outside`]) - from 1 to the last, one more than the `\n` the text
/// holds; each def has a name and a path, its line and end line within them, its doc span, when it
/// has one, within them, and none of the out-of-line module fields only the bundled Rust grammar
/// sets; each ref has a name, a qualifier, when it has one, that is not empty, a line from 1 to the
/// last and, when it has a kind, a tagged mention kind, one whose references
/// [`reference_candidate_kinds`] resolves onto definitions; and no def path leads back to itself
/// through the parents ([`parent_cycle`]). The why quotes each text the extractor wrote into it -
/// the answer's path, its error, a ref's kind, the def path a cycle runs through - so a line break
/// or terminal escape there is escaped and the line saying the why stays one line. The symbols
/// hold the doc spans, the file's and each def's, as the answer writes them (spec 117, DOC FACTS).
pub(super) fn symbols_answered(path: &str, text: &str, line: &str) -> Result<CodeSymbols, String> {
    let answer: Answer =
        serde_json::from_str(line).map_err(|e| format!("the answer does not parse: {e}"))?;
    if answer.path != path {
        return Err(format!("the answer is for {:?}", answer.path));
    }
    if let Some(error) = answer.error {
        return Err(format!("the answer is an error: {error:?}"));
    }
    let (Some(defs), Some(refs)) = (answer.defs, answer.refs) else {
        return Err("the answer holds no error and lacks defs or refs".to_string());
    };
    let last = text.matches('\n').count() as u32 + 1;
    if let Some(why) = answer.doc.and_then(|doc| outside(doc, last, DOC_BOUNDS)) {
        return Err(format!("the file's {why}"));
    }
    for (i, def) in defs.iter().enumerate() {
        if def.name.is_empty() {
            return Err(format!("defs[{i}]: the name is empty"));
        }
        if def.path.is_empty() {
            return Err(format!("defs[{i}]: the path is empty"));
        }
        if let Some(why) = outside((def.line, def.end_line), last, ["line", "end_line"])
            .or_else(|| def.doc.and_then(|doc| outside(doc, last, DOC_BOUNDS)))
        {
            return Err(format!("defs[{i}]: {why}"));
        }
        if def.is_out_of_line_module
            || def.path_override.is_some()
            || def.enclosing_inline_module_path.is_some()
        {
            return Err(format!(
                "defs[{i}]: an out-of-line module is not in the protocol"
            ));
        }
    }
    for (i, r) in refs.iter().enumerate() {
        if r.name.is_empty() {
            return Err(format!("refs[{i}]: the name is empty"));
        }
        if r.qualifier.as_ref().is_some_and(|q| q.name.is_empty()) {
            return Err(format!("refs[{i}]: the qualifier is empty"));
        }
        if !(1..=last).contains(&r.line) {
            return Err(format!("refs[{i}]: line {} is outside 1..={last}", r.line));
        }
        if let Some(kind) = &r.ref_kind {
            if reference_candidate_kinds(kind).is_empty() {
                return Err(format!(
                    "refs[{i}]: ref_kind {kind:?} is not a tagged mention kind"
                ));
            }
        }
    }
    if let Some(through) = parent_cycle(&defs) {
        return Err(format!(
            "the defs' parents form a cycle through {through:?}"
        ));
    }
    Ok(CodeSymbols {
        defs,
        refs,
        partial: answer.partial,
        doc: answer.doc,
    })
}

/// What [`outside`] names a doc span's first and last line by.
const DOC_BOUNDS: [&str; 2] = ["doc start", "doc end"];

/// Why the 1-based, inclusive line span `(start, end)` lies outside a text whose last line is
/// `last`, the span's first and last line named `start_name` and `end_name`: a start outside 1 to
/// the last line, or else an end outside the start to the last line. None when the span is within
/// the text.
fn outside(
    (start, end): (u32, u32),
    last: u32,
    [start_name, end_name]: [&str; 2],
) -> Option<String> {
    if !(1..=last).contains(&start) {
        return Some(format!("{start_name} {start} is outside 1..={last}"));
    }
    (!(start..=last).contains(&end))
        .then(|| format!("{end_name} {end} is outside {start}..={last}"))
}

/// The def path a walk of `defs` from def path to parent returns to, when one does:
/// [`rigger_domain::cycle::first_cycle`] over the paths in the order the defs give them, each path
/// stepping to the parent each def of that path names, and a parent no def is at ends that step.
/// None when the parents form no cycle.
fn parent_cycle(defs: &[model::Def]) -> Option<&str> {
    let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
    for def in defs {
        let named = parents.entry(def.path.as_str()).or_default();
        named.extend(def.parent.as_deref());
    }
    rigger_domain::cycle::first_cycle(defs.iter().map(|def| def.path.as_str()), |path| {
        parents.get(path).into_iter().flatten().copied()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounder::symbols::model::Kind;
    use crate::symbol_index::qualifier;

    /// A def an answer carries: a function named `name` at `path` over `line..=end_line`, its
    /// container `parent`.
    fn answered_def(
        name: &str,
        path: &str,
        (line, end_line): (u32, u32),
        parent: Option<&str>,
        is_test: bool,
    ) -> model::Def {
        model::Def {
            line,
            end_line,
            path: path.to_string(),
            parent: parent.map(str::to_string),
            is_test,
            ..crate::symbol_index::def(Kind::Function, name)
        }
    }

    /// A ref an answer carries: `name` at `line`, inside the def at `enclosing`, of `ref_kind`.
    fn answered_ref(
        name: &str,
        line: u32,
        enclosing: Option<&str>,
        ref_kind: Option<&str>,
        is_test: bool,
    ) -> SymRef {
        SymRef {
            ref_kind: ref_kind.map(str::to_string),
            ..crate::symbol_index::sym_ref(name, line, enclosing, is_test)
        }
    }

    /// An answer for `a.zig` carrying the defs `defs` (JSON objects, comma-separated) and no refs.
    fn answer_with_defs(defs: &str) -> String {
        format!(r#"{{"path":"a.zig","defs":[{defs}],"refs":[]}}"#)
    }

    /// An answer for `a.zig` carrying no defs and the refs `refs` (JSON objects, comma-separated).
    fn answer_with_refs(refs: &str) -> String {
        format!(r#"{{"path":"a.zig","defs":[],"refs":[{refs}]}}"#)
    }

    /// THE EXTRACTOR PROTOCOL, the answer: over a request for `a.zig`, an answer holding defs and
    /// refs within the text's lines - the last one more than the `\n` the text holds - gives them,
    /// each ref's tagged mention kind and qualifier, its partial flag and the doc spans, the
    /// file's and each def's, as written; every answer the protocol refuses gives the exact why:
    /// one that does not parse, that is for another path, that is an error - its text quoted, a
    /// line break or terminal escape in it escaped - that lacks a list, a
    /// def or ref outside the lines or with an empty name, a def with an empty path, an end line
    /// outside its line and the last, or an out-of-line module field, a doc span, the file's or a
    /// def's, starting outside the first line and the last or ending outside its start and the
    /// last, a ref with an empty qualifier or a kind that is no tagged mention kind (`call`, the
    /// empty kind, `module`), and a cycle among the def paths through their parents.
    #[test]
    fn an_answer_gives_its_symbols_or_why_the_protocol_refuses_it() {
        const TEXT: &str = "a\nb\nc\n";
        let def_on = |line: u32, end_line: u32| {
            format!(
                r#"{{"kind":"Function","name":"f","line":{line},"end_line":{end_line},"path":"f"}}"#
            )
        };
        let parented = |path: &str, parent: &str| {
            format!(
                r#"{{"kind":"Function","name":"f","line":1,"end_line":1,"path":"{path}","parent":"{parent}"}}"#
            )
        };
        let unparented = |path: &str| {
            format!(r#"{{"kind":"Type","name":"t","line":1,"end_line":1,"path":"{path}"}}"#)
        };
        let empty = || CodeSymbols {
            defs: Vec::new(),
            refs: Vec::new(),
            partial: false,
            doc: None,
        };
        let refused = |why: &str| Err(why.to_string());
        // (the request's text, the answer line, its symbols or why it is refused)
        let mut rows: Vec<(&str, String, Result<CodeSymbols, String>)> = vec![
            (
                TEXT,
                r#"{"path":"a.zig","defs":[],"refs":[]}"#.into(),
                Ok(empty()),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","defs":[],"refs":[],"partial":true}"#.into(),
                Ok(CodeSymbols {
                    partial: true,
                    ..empty()
                }),
            ),
            (
                TEXT,
                concat!(
                    r#"{"path":"a.zig","defs":["#,
                    r#"{"kind":"Function","name":"a","line":1,"end_line":1,"path":"a","parent":"c"},"#,
                    r#"{"kind":"Function","name":"b","line":4,"end_line":4,"path":"b","parent":"c"},"#,
                    r#"{"kind":"Function","name":"c","line":1,"end_line":4,"path":"c","parent":"gone"},"#,
                    r#"{"kind":"Function","name":"a","line":2,"end_line":3,"path":"a","parent":"c","is_test":true}"#,
                    r#"],"refs":["#,
                    r#"{"name":"x","line":1},"#,
                    r#"{"name":"y","line":4,"enclosing":"a","is_test":true},"#,
                    r#"{"name":"C","line":2,"ref_kind":"const"},"#,
                    r#"{"name":"T","line":3,"ref_kind":"type"},"#,
                    r#"{"name":"u","line":4,"ref_kind":"use"}"#,
                    r#"]}"#
                )
                .into(),
                Ok(CodeSymbols {
                    defs: vec![
                        answered_def("a", "a", (1, 1), Some("c"), false),
                        answered_def("b", "b", (4, 4), Some("c"), false),
                        answered_def("c", "c", (1, 4), Some("gone"), false),
                        answered_def("a", "a", (2, 3), Some("c"), true),
                    ],
                    refs: vec![
                        answered_ref("x", 1, None, None, false),
                        answered_ref("y", 4, Some("a"), None, true),
                        answered_ref("C", 2, None, Some("const"), false),
                        answered_ref("T", 3, None, Some("type"), false),
                        answered_ref("u", 4, None, Some("use"), false),
                    ],
                    partial: false,
                    doc: None,
                }),
            ),
            ("", answer_with_defs(&def_on(1, 1)), {
                Ok(CodeSymbols {
                    defs: vec![answered_def("f", "f", (1, 1), None, false)],
                    ..empty()
                })
            }),
            (
                TEXT,
                r#"{"path":"a.zig","defs":[{"kind":"Function","name":"f","line":2,"end_line":2,"path":"f","doc":[1,1]}],"refs":[],"doc":[1,1]}"#
                    .into(),
                Ok(CodeSymbols {
                    defs: vec![model::Def {
                        doc: Some((1, 1)),
                        ..answered_def("f", "f", (2, 2), None, false)
                    }],
                    doc: Some((1, 1)),
                    ..empty()
                }),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":"field"}"#),
                Ok(CodeSymbols {
                    refs: vec![answered_ref("x", 1, None, Some("field"), false)],
                    ..empty()
                }),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":"variant"}"#),
                Ok(CodeSymbols {
                    refs: vec![answered_ref("x", 1, None, Some("variant"), false)],
                    ..empty()
                }),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":"impl"}"#),
                Ok(CodeSymbols {
                    refs: vec![answered_ref("x", 1, None, Some("impl"), false)],
                    ..empty()
                }),
            ),
            (
                TEXT,
                answer_with_refs(concat!(
                    r#"{"name":"area","line":1,"qualifier":{"name":"Shape","path":true}},"#,
                    r#"{"name":"area","line":2,"qualifier":{"name":"s","path":false}}"#
                )),
                Ok(CodeSymbols {
                    refs: vec![
                        SymRef {
                            qualifier: Some(qualifier("Shape", true)),
                            ..answered_ref("area", 1, None, None, false)
                        },
                        SymRef {
                            qualifier: Some(qualifier("s", false)),
                            ..answered_ref("area", 2, None, None, false)
                        },
                    ],
                    ..empty()
                }),
            ),
            (
                "",
                answer_with_defs(&def_on(2, 2)),
                refused("defs[0]: line 2 is outside 1..=1"),
            ),
            ("a\nb", answer_with_defs(&def_on(1, 2)), {
                Ok(CodeSymbols {
                    defs: vec![answered_def("f", "f", (1, 2), None, false)],
                    ..empty()
                })
            }),
            (
                "a\nb",
                answer_with_defs(&def_on(1, 3)),
                refused("defs[0]: end_line 3 is outside 1..=2"),
            ),
            (
                TEXT,
                "not json".into(),
                refused("the answer does not parse: expected ident at line 1 column 2"),
            ),
            (
                TEXT,
                r#"{"defs":[],"refs":[]}"#.into(),
                refused("the answer does not parse: missing field `path` at line 1 column 21"),
            ),
            (
                TEXT,
                r#"{"path":"c.zig","defs":[],"refs":[]}"#.into(),
                refused(r#"the answer is for "c.zig""#),
            ),
            (
                TEXT,
                r#"{"path":"c.zig","error":"no parser"}"#.into(),
                refused(r#"the answer is for "c.zig""#),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","error":"no parser"}"#.into(),
                refused(r#"the answer is an error: "no parser""#),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","error":"no parser","defs":[],"refs":[]}"#.into(),
                refused(r#"the answer is an error: "no parser""#),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","error":"no\nparser\u001b[31m"}"#.into(),
                refused(r#"the answer is an error: "no\nparser\u{1b}[31m""#),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","refs":[]}"#.into(),
                refused("the answer holds no error and lacks defs or refs"),
            ),
            (
                TEXT,
                r#"{"path":"a.zig","defs":[]}"#.into(),
                refused("the answer holds no error and lacks defs or refs"),
            ),
            (
                TEXT,
                r#"{"path":"a.zig"}"#.into(),
                refused("the answer holds no error and lacks defs or refs"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Function","name":"","line":1,"end_line":1,"path":"f"}"#,
                ),
                refused("defs[0]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_defs(&format!(
                    r#"{},{{"kind":"Function","name":"","line":1,"end_line":1,"path":"g"}}"#,
                    def_on(1, 1)
                )),
                refused("defs[1]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Function","name":"f","line":1,"end_line":1,"path":""}"#,
                ),
                refused("defs[0]: the path is empty"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(0, 1)),
                refused("defs[0]: line 0 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(5, 5)),
                refused("defs[0]: line 5 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(2, 1)),
                refused("defs[0]: end_line 1 is outside 2..=4"),
            ),
            (
                TEXT,
                answer_with_defs(&def_on(2, 5)),
                refused("defs[0]: end_line 5 is outside 2..=4"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Module","name":"m","line":1,"end_line":1,"path":"m","is_out_of_line_module":true}"#,
                ),
                refused("defs[0]: an out-of-line module is not in the protocol"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Module","name":"m","line":1,"end_line":1,"path":"m","path_override":"x.zig"}"#,
                ),
                refused("defs[0]: an out-of-line module is not in the protocol"),
            ),
            (
                TEXT,
                answer_with_defs(
                    r#"{"kind":"Module","name":"m","line":1,"end_line":1,"path":"m","enclosing_inline_module_path":"n"}"#,
                ),
                refused("defs[0]: an out-of-line module is not in the protocol"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"","line":1}"#),
                refused("refs[0]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1},{"name":"","line":1}"#),
                refused("refs[1]: the name is empty"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":0}"#),
                refused("refs[0]: line 0 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":5}"#),
                refused("refs[0]: line 5 is outside 1..=4"),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":"call"}"#),
                refused(r#"refs[0]: ref_kind "call" is not a tagged mention kind"#),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":""}"#),
                refused(r#"refs[0]: ref_kind "" is not a tagged mention kind"#),
            ),
            (
                TEXT,
                answer_with_refs(r#"{"name":"x","line":1,"ref_kind":"module"}"#),
                refused(r#"refs[0]: ref_kind "module" is not a tagged mention kind"#),
            ),
            (
                TEXT,
                answer_with_refs(concat!(
                    r#"{"name":"x","line":1,"qualifier":{"name":"Q","path":true}},"#,
                    r#"{"name":"x","line":1,"qualifier":{"name":"","path":false}}"#
                )),
                refused("refs[1]: the qualifier is empty"),
            ),
            (
                TEXT,
                answer_with_defs(&parented("a", "a")),
                refused(r#"the defs' parents form a cycle through "a""#),
            ),
            (
                TEXT,
                answer_with_defs(&format!("{},{}", parented("a", "b"), parented("b", "a"))),
                refused(r#"the defs' parents form a cycle through "a""#),
            ),
            (
                TEXT,
                answer_with_defs(&format!(
                    "{},{},{}",
                    parented("a", "b"),
                    parented("b", "c"),
                    parented("c", "b")
                )),
                refused(r#"the defs' parents form a cycle through "b""#),
            ),
            (
                TEXT,
                answer_with_defs(&format!(
                    "{},{},{}",
                    unparented("a"),
                    parented("a", "b"),
                    parented("b", "a")
                )),
                refused(r#"the defs' parents form a cycle through "a""#),
            ),
        ];
        // (a doc span over TEXT, why the protocol refuses it or none): each is answered as a def's
        // span and as the file's, the def's why led by its index and the file's by "the file's".
        let doc_spans = [
            ((0, 1), Some("doc start 0 is outside 1..=4")),
            ((1, 4), None),
            ((4, 4), None),
            ((5, 5), Some("doc start 5 is outside 1..=4")),
            ((3, 2), Some("doc end 2 is outside 3..=4")),
            ((3, 3), None),
            ((2, 5), Some("doc end 5 is outside 2..=4")),
        ];
        for ((start, end), why) in doc_spans {
            rows.push((
                TEXT,
                answer_with_defs(&format!(
                    r#"{{"kind":"Function","name":"f","line":1,"end_line":1,"path":"f","doc":[{start},{end}]}}"#
                )),
                match why {
                    Some(why) => refused(&format!("defs[0]: {why}")),
                    None => Ok(CodeSymbols {
                        defs: vec![model::Def {
                            doc: Some((start, end)),
                            ..answered_def("f", "f", (1, 1), None, false)
                        }],
                        ..empty()
                    }),
                },
            ));
            rows.push((
                TEXT,
                format!(r#"{{"path":"a.zig","defs":[],"refs":[],"doc":[{start},{end}]}}"#),
                match why {
                    Some(why) => refused(&format!("the file's {why}")),
                    None => Ok(CodeSymbols {
                        doc: Some((start, end)),
                        ..empty()
                    }),
                },
            ));
        }
        for (text, answer, symbols) in rows {
            assert_eq!(
                symbols_answered("a.zig", text, &answer),
                symbols,
                "{text:?}: {answer}"
            );
        }
    }

    /// THE EXTRACTOR PROTOCOL, a session: a started extractor answers a request with its line, its
    /// line ending dropped; one whose output ends, whose answer is not text, or that is gone before
    /// the request is written says why; and one that cannot start in its directory says why.
    /// Each session then closes within 5 s, the one whose extractor writes on without end after
    /// its answer included, since closing drops its output before it waits.
    #[cfg(unix)]
    #[test]
    fn a_session_answers_a_request_with_its_line_or_says_why_it_cannot() {
        let kept = tempfile::tempdir().expect("tempdir");
        // (the extractor's run, whether it has ended before the request, its answer or why not)
        let rows = [
            (r#"read -r line; printf '%s\n' "$line""#, false, Ok("ping")),
            (r#"read -r line; printf 'x\n'; yes"#, false, Ok("x")),
            (r#"read -r line; printf 'pong\r\n'"#, false, Ok("pong")),
            (r#"read -r line; printf 'pong'"#, false, Ok("pong")),
            ("read -r line", false, Err("end of output".to_string())),
            (
                r#"read -r line; printf '\377\n'"#,
                false,
                Err("read the answer: stream did not contain valid UTF-8".to_string()),
            ),
            (
                "exit 0",
                true,
                // EPIPE: the extractor's end of the pipe closed with it.
                Err(format!(
                    "write the request: {}",
                    std::io::Error::from_raw_os_error(32)
                )),
            ),
        ];
        for (run, ended, answer) in rows {
            let mut session = Session::start(run, kept.path()).expect("start the session");
            if ended {
                session.child.wait().expect("wait for the extractor");
            }
            assert_eq!(session.ask("ping"), answer.map(str::to_string), "{run}");
            let (closed, close) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                session.close();
                let _ = closed.send(());
            });
            assert_eq!(
                close.recv_timeout(std::time::Duration::from_secs(5)),
                Ok(()),
                "{run}: the session closes"
            );
        }
        let gone = kept.path().join("gone");
        assert_eq!(
            Session::start("true", &gone).map(|session| session.close()),
            Err(format!("start: {}", std::io::Error::from_raw_os_error(2))),
            "a session cannot start in a directory that is not there"
        );
    }
}
