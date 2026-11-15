//! THE EXTRACTOR PROTOCOL's stub extractor (spec 125, criterion 8): a POSIX shell script a test
//! declares as an extractor's `run`. It answers each request line with the one line a test wrote
//! for the request's path, and records the directory it started in, every request it read and
//! each time its input ended, so a test pins what the code store asked and how many sessions it
//! started and closed. It lives in its own directory, outside the project it serves, so no walk
//! of the project reads it. It runs under `sh`, so only a unix test starts it. Beside it lie the
//! request line it reads for a file, the one-function answer line a test writes for it, the
//! physical root it records as its start and the identity facts a declared file holds when it
//! holds no symbols.

use std::path::{Path, PathBuf};

use rigger_domain::code_facts::{FileFacts, Half};

/// The stub's script, run as `sh <script> <dir>`. It appends the physical directory it started
/// in to `<dir>/starts`; for each request line it appends the line to `<dir>/requests` and prints
/// `<dir>/answers/<the request's path>`, ending without a line when that file is missing; and
/// when its input ends it appends a line to `<dir>/ends`. The request's path is read off the
/// line's head, `{"path":"`, up to the next `"`.
const STUB_SCRIPT: &str = r#"dir=$1
pwd -P >> "$dir/starts"
while IFS= read -r request; do
  printf '%s\n' "$request" >> "$dir/requests"
  path=${request#'{"path":"'}
  path=${path%%'"'*}
  cat "$dir/answers/$path" 2>/dev/null || exit 1
done
echo ended >> "$dir/ends"
"#;

/// A stub extractor kept in one directory: its script, the answers a test wrote for it and what
/// it recorded.
pub struct StubExtractor {
    dir: PathBuf,
}

impl StubExtractor {
    /// The stub kept in `dir`, its script written there.
    pub fn new(dir: &Path) -> Self {
        std::fs::write(dir.join("stub.sh"), STUB_SCRIPT).expect("write the stub's script");
        Self {
            dir: dir.to_path_buf(),
        }
    }

    /// The command a declaration runs to start the stub.
    pub fn run(&self) -> String {
        format!(
            "sh '{}' '{}'",
            self.dir.join("stub.sh").display(),
            self.dir.display()
        )
    }

    /// The `extractors:` flow mapping declaring the stub as the extractor `name` of the flow
    /// sequence `extensions` (`[zig]`, say).
    pub fn declaring(&self, name: &str, extensions: &str) -> String {
        format!(
            "{{ {name}: {{ extensions: {extensions}, run: {:?} }} }}",
            self.run()
        )
    }

    /// Makes the stub answer a request for `path` with the line `answer`.
    pub fn answers(&self, path: &str, answer: &str) {
        let file = self.dir.join("answers").join(path);
        std::fs::create_dir_all(file.parent().expect("an answer file lies in a directory"))
            .expect("create the answers directory");
        std::fs::write(file, format!("{answer}\n")).expect("write the answer");
    }

    /// The physical directory of each start of the stub, in start order.
    pub fn starts(&self) -> Vec<String> {
        self.recorded("starts")
    }

    /// Every request line the stub read, in the order it read them.
    pub fn requests(&self) -> Vec<String> {
        self.recorded("requests")
    }

    /// One line for each session of the stub whose input ended, in the order they ended.
    pub fn ends(&self) -> Vec<String> {
        self.recorded("ends")
    }

    /// The lines of the stub's record `name`, none before the stub first writes it.
    fn recorded(&self, name: &str) -> Vec<String> {
        match std::fs::read_to_string(self.dir.join(name)) {
            Ok(text) => text.lines().map(str::to_string).collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => panic!("read the stub's {name}: {e}"),
        }
    }
}

/// The request line the stub reads for the file `path` holding `text`, a text holding no character
/// JSON escapes but the line end.
pub fn request_for(path: &str, text: &str) -> String {
    format!(
        r#"{{"path":"{path}","text":"{}"}}"#,
        text.replace('\n', "\\n")
    )
}

/// The answer line giving the file `path` one file-level function `leaf` on its first line.
pub fn answer_defining(path: &str, leaf: &str) -> String {
    answer_defining_at(path, leaf, 1, 1)
}

/// The answer line giving the file `path` one file-level function `leaf` over the lines
/// `line..=end_line`.
pub fn answer_defining_at(path: &str, leaf: &str, line: u32, end_line: u32) -> String {
    format!(
        r#"{{"path":"{path}","defs":[{{"kind":"Function","name":"{leaf}","line":{line},"end_line":{end_line},"path":"{leaf}"}}],"refs":[]}}"#
    )
}

/// The physical directory of the project at `root`, as the stub records where it started.
pub fn root_dir_of(root: &Path) -> String {
    root.canonicalize()
        .expect("the root exists")
        .display()
        .to_string()
}

/// The facts a declared extraction gives a file it holds no symbols for: its identity, the
/// code half and the declared name as its language.
pub fn declared_identity(path: &str, blob: &str, lang: &str) -> FileFacts {
    FileFacts {
        path: path.to_string(),
        blob: blob.to_string(),
        lang: lang.to_string(),
        half: Half::Code,
        ..FileFacts::default()
    }
}
