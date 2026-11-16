//! THE LOGIN START: the host's facility for starting a program as its user logs in, behind one
//! interface, [`LoginStart`]. A run whose launch opted in (`rigger run --relaunch-at-login`)
//! registers `<rigger> run --at-login <root>` under its project's key ([`run_key`]) once its
//! driver holds the step lock, and removes the entry as that driver ends; a driver that dies
//! with the machine, by a reboot or a power loss, leaves it, so the next login relaunches the run.
//!
//! Each OS has one writer, a file under the directory its own facility reads at login, given as
//! the writer's one field so a test drives the writer in a directory of its own: on Linux a
//! systemd user unit and its `default.target.wants` link ([`SystemdUserUnits`], unix only, for
//! the link), on macOS a launchd agent's property list ([`LaunchAgents`]), and on Windows a
//! command file in the Startup folder ([`StartupFolder`]). [`OsLoginStart`] writes through the
//! writer of the OS the binary is built for, under the directory that OS reads; any other OS
//! answers unavailable. Every writer's `register` replaces the entry standing under its key, and
//! its `unregister` removes every file and link `register` wrote under the key, leaving the
//! directories, and is `Ok` when none stands. Every writer lists the entries standing under a key
//! [`run_key`] makes, each with the program read back from the file its `register` wrote, so
//! [`sweep`] removes an entry whose program was since uninstalled or moved.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use rigger_domain::playbooks::fnv1a_64;
use rigger_domain::sleep_hold::Unavailable;

use crate::home::{non_empty, xdg_or_home};

/// The host's facility for starting a program each time its user logs in.
pub trait LoginStart {
    /// Start `program` with `args` at every login from now on, under `key`, replacing the entry
    /// standing under it; or the reason the facility is unavailable.
    fn register(&self, key: &str, program: &str, args: &[String]) -> Result<(), Unavailable>;

    /// Start nothing under `key` at login any more, removing every file and link
    /// [`LoginStart::register`] wrote under it; `Ok` when no entry stands under it.
    fn unregister(&self, key: &str) -> Result<(), Unavailable>;

    /// Every entry standing under a key [`run_key`] makes, by key, each with the program it
    /// starts read back from the file [`LoginStart::register`] wrote, or with none when that file
    /// is not in the form this facility's register writes; empty when none stands.
    fn entries(&self) -> Result<Vec<Entry>, Unavailable>;
}

/// One login entry a facility lists ([`LoginStart::entries`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The key it stands under.
    pub key: String,
    /// The program it starts, read back from its file; `None` when that file is not in the form
    /// the facility's register writes, so it may be the user's own.
    pub program: Option<String>,
}

/// What [`sweep`] did with one entry it names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Swept {
    /// Removed, since the program it starts does not exist.
    Removed {
        /// The key it stood under.
        key: String,
        /// The program it started.
        program: String,
    },
    /// Its program does not exist, and its removal failed.
    NotRemoved {
        /// The key it stands under.
        key: String,
        /// The program it starts.
        program: String,
        /// Why the removal failed.
        why: String,
    },
    /// Left alone: its file is not in the form rigger writes, so it may be the user's.
    Foreign {
        /// The key it stands under.
        key: String,
    },
}

impl Swept {
    /// The one line that says what the sweep did with the entry.
    pub fn line(&self) -> String {
        match self {
            Swept::Removed { key, program } => format!(
                "removed the login entry {key}: the program it starts, {program}, does not exist"
            ),
            Swept::NotRemoved { key, program, why } => format!(
                "the login entry {key} starts {program}, which does not exist, and was not \
                 removed: {why}"
            ),
            Swept::Foreign { key } => {
                format!("left the login entry {key} alone: it is not in the form rigger writes")
            }
        }
    }

    /// Whether the sweep changed the entry: removed it.
    pub fn is_change(&self) -> bool {
        matches!(self, Swept::Removed { .. })
    }
}

/// THE STALE-ENTRY SWEEP, the reaper of an entry whose program was uninstalled or moved after
/// its register, which would fail at every login: each entry `start` lists whose program does not
/// exist is unregistered; an entry whose program stands, or is not known to be absent, stands;
/// and one not in the facility's own form is left alone. Answers what it did with each entry it
/// names, in the listing's key order; or why `start` could not list its entries.
pub fn sweep(start: &dyn LoginStart) -> Result<Vec<Swept>, Unavailable> {
    let _ = start;
    Ok(Vec::new())
}

/// The knob selecting the login-start adapter a run registers its relaunch with, read by the
/// composition root's `login_start` alone, beside [`crate::keep_awake::KEEP_AWAKE_ENV`] and with
/// its grammar: unset, the host's [`OsLoginStart`]; `record:<path>`, a fake appending a
/// `register <key> <program> <args...>` or `unregister <key>` line to `<path>`;
/// `unavailable:<reason>`, every call answering `<reason>`; any other value, every call answering
/// that the value names no adapter. The tests set the fake, so no test registers with its host.
pub const LOGIN_START_ENV: &str = "RIGGER_LOGIN_START";

/// The XDG variable the Linux writer's directory is read from, before the home directory.
pub const CONFIG_HOME_ENV: &str = "XDG_CONFIG_HOME";

/// The variable the Windows writer's directory, the user's Startup folder, is read from.
pub const APPDATA_ENV: &str = "APPDATA";

/// The text every key [`run_key`] makes starts with, by which a writer lists its own entries.
pub const KEY_PREFIX: &str = "rigger-run-";

/// The key a project's entry is registered under: [`KEY_PREFIX`] and the sixteen hex digits of the
/// FNV-1a hash of its canonical root's text, so every launch in one checkout replaces one entry
/// and two checkouts hold two.
pub fn run_key(root: &str) -> String {
    format!("{KEY_PREFIX}{:016x}", fnv1a_64(root.as_bytes()))
}

/// The Linux writer: under `dir`, the user unit `<key>.service` ([`systemd_unit`]) and the link
/// `default.target.wants/<key>.service` to it that enables it, so the user's systemd manager
/// starts it as it starts, at login (at boot under lingering). Unix only, for the link.
#[cfg(unix)]
pub struct SystemdUserUnits {
    /// The user unit directory the manager reads at its start.
    pub dir: PathBuf,
}

/// The directory under the user unit directory whose links enable a unit at the manager's start.
#[cfg(unix)]
const WANTS_DIR: &str = "default.target.wants";

#[cfg(unix)]
impl SystemdUserUnits {
    /// The unit file of `key`.
    fn unit(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.service"))
    }

    /// The link that enables the unit of `key`.
    fn link(&self, key: &str) -> PathBuf {
        self.dir.join(WANTS_DIR).join(format!("{key}.service"))
    }
}

#[cfg(unix)]
impl LoginStart for SystemdUserUnits {
    fn register(&self, key: &str, program: &str, args: &[String]) -> Result<(), Unavailable> {
        let link = self.link(key);
        write_entry(&self.unit(key), &systemd_unit(program, args))?;
        remove_entry(&link)?;
        let wants = self.dir.join(WANTS_DIR);
        fs::create_dir_all(&wants).map_err(|e| failed("create", &wants, e))?;
        std::os::unix::fs::symlink(Path::new("..").join(format!("{key}.service")), &link)
            .map_err(|e| failed("link", &link, e))
    }

    fn unregister(&self, key: &str) -> Result<(), Unavailable> {
        remove_entry(&self.link(key))?;
        remove_entry(&self.unit(key))
    }

    fn entries(&self) -> Result<Vec<Entry>, Unavailable> {
        Ok(Vec::new())
    }
}

/// The user unit that runs `program` with `args` once as the manager starts: `oneshot`, and kept
/// active after its command returns, so the driver the command starts in the background stays
/// in the unit's control group, where the manager would end it if the unit went inactive. Every
/// word is double-quoted ([`systemd_word`]).
pub fn systemd_unit(program: &str, args: &[String]) -> String {
    let command: Vec<String> = std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(systemd_word)
        .collect();
    format!(
        "[Unit]\nDescription=Relaunch a rigger run whose driver died\n\n[Service]\nType=oneshot\n\
         RemainAfterExit=yes\nExecStart={}\n\n[Install]\nWantedBy=default.target\n",
        command.join(" ")
    )
}

/// `word` as one `ExecStart` word: double-quoted, with the specifier sign `%` and the variable
/// sign `$` doubled, so neither expands, and a backslash, a double quote and an ASCII control
/// character escaped as C escapes, which the manager unescapes.
fn systemd_word(word: &str) -> String {
    let mut quoted = String::from('"');
    for c in word.chars() {
        match c {
            '%' => quoted.push_str("%%"),
            '$' => quoted.push_str("$$"),
            '\\' => quoted.push_str(r"\\"),
            '"' => quoted.push_str(r#"\""#),
            c if c.is_ascii_control() => quoted.push_str(&format!(r"\x{:02x}", u32::from(c))),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// The macOS writer: under `dir`, the launchd agent `<key>.plist` ([`launch_agent_plist`]), which
/// launchd loads, and so runs, as the user logs in.
pub struct LaunchAgents {
    /// The user's agent directory launchd reads at login.
    pub dir: PathBuf,
}

impl LaunchAgents {
    /// The property list of `key`.
    fn plist(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.plist"))
    }
}

impl LoginStart for LaunchAgents {
    fn register(&self, key: &str, program: &str, args: &[String]) -> Result<(), Unavailable> {
        write_entry(&self.plist(key), &launch_agent_plist(key, program, args))
    }

    fn unregister(&self, key: &str) -> Result<(), Unavailable> {
        remove_entry(&self.plist(key))
    }

    fn entries(&self) -> Result<Vec<Entry>, Unavailable> {
        Ok(Vec::new())
    }
}

/// The agent labelled `label` that runs `program` with `args` once as it loads, each an
/// XML-escaped string ([`xml_text`]), and leaves the process group of the driver its command
/// starts in the background alive when that command returns.
pub fn launch_agent_plist(label: &str, program: &str, args: &[String]) -> String {
    let arguments: String = std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(|word| format!("\t\t<string>{}</string>\n", xml_text(word)))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \
         \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\t<key>Label</key>\n\t<string>{}</string>\n\
         \t<key>ProgramArguments</key>\n\t<array>\n{arguments}\t</array>\n\
         \t<key>RunAtLoad</key>\n\t<true/>\n\t<key>AbandonProcessGroup</key>\n\t<true/>\n\
         </dict>\n</plist>\n",
        xml_text(label)
    )
}

/// `text` as XML character data: each of `&`, `<`, `>`, `"` and `'` as its entity.
fn xml_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// The Windows writer: under `dir`, the command file `<key>.cmd` ([`startup_command`]), which
/// Windows runs from the user's Startup folder as the user logs in.
pub struct StartupFolder {
    /// The user's Startup folder.
    pub dir: PathBuf,
}

impl StartupFolder {
    /// The command file of `key`.
    fn command(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.cmd"))
    }
}

impl LoginStart for StartupFolder {
    fn register(&self, key: &str, program: &str, args: &[String]) -> Result<(), Unavailable> {
        write_entry(&self.command(key), &startup_command(program, args)?)
    }

    fn unregister(&self, key: &str) -> Result<(), Unavailable> {
        remove_entry(&self.command(key))
    }

    fn entries(&self) -> Result<Vec<Entry>, Unavailable> {
        Ok(Vec::new())
    }
}

/// The one line that runs `program` with `args`, not echoed, each word double-quoted
/// ([`cmd_word`]); unavailable when a word holds what no quoted word can carry.
pub fn startup_command(program: &str, args: &[String]) -> Result<String, Unavailable> {
    let words = std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(cmd_word)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!("@{}\r\n", words.join(" ")))
}

/// `word` as one double-quoted word of a command file's line: `%` doubled, so the command
/// interpreter expands no variable, and the backslashes ending it doubled, so the program reads
/// them before its closing quote; inside the quotes `&`, `$` and spaces are themselves. A word
/// holding a double quote or a line break, which no Windows path holds, is refused, since it
/// would end the quoting.
fn cmd_word(word: &str) -> Result<String, Unavailable> {
    if word.contains(['"', '\r', '\n']) {
        return Err(Unavailable(format!(
            "a Windows startup command cannot carry {word:?}: it holds a double quote or a line \
             break"
        )));
    }
    let trailing = word.len() - word.trim_end_matches('\\').len();
    Ok(format!(
        "\"{}{}\"",
        word.replace('%', "%%"),
        "\\".repeat(trailing)
    ))
}

/// Write `text` to the entry at `path`, replacing any, its directory created; on unix readable by
/// all and writable by its owner alone, which launchd requires of an agent it loads.
fn write_entry(path: &Path, text: &str) -> Result<(), Unavailable> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| failed("create", dir, e))?;
    }
    fs::write(path, text).map_err(|e| failed("write", path, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644))
            .map_err(|e| failed("set the mode of", path, e))?;
    }
    Ok(())
}

/// Remove the file or link at `path`; `Ok` when none stands there.
fn remove_entry(path: &Path) -> Result<(), Unavailable> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(failed("remove", path, e)),
        _ => Ok(()),
    }
}

/// The facility is unavailable since `doing` `path` failed with `e`.
fn failed(doing: &str, path: &Path, e: io::Error) -> Unavailable {
    Unavailable(format!("cannot {doing} {}: {e}", path.display()))
}

/// The Linux writer's directory: `systemd/user` under the config home, the value of
/// [`CONFIG_HOME_ENV`] (`config_home`), else `.config` under the home directory; `None` when
/// neither is set.
pub fn systemd_user_dir(config_home: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    Some(
        xdg_or_home(config_home, home, &[".config"])?
            .join("systemd")
            .join("user"),
    )
}

/// The macOS writer's directory: `Library/LaunchAgents` under the home directory; `None` when no
/// home is set.
pub fn launch_agents_dir(home: Option<OsString>) -> Option<PathBuf> {
    Some(
        PathBuf::from(non_empty(home)?)
            .join("Library")
            .join("LaunchAgents"),
    )
}

/// The Windows writer's directory: the Startup folder under the roaming application data
/// directory, the value of [`APPDATA_ENV`] (`appdata`); `None` when it is not set.
pub fn startup_dir(appdata: Option<OsString>) -> Option<PathBuf> {
    Some(
        ["Microsoft", "Windows", "Start Menu", "Programs", "Startup"]
            .iter()
            .fold(PathBuf::from(non_empty(appdata)?), |dir, part| {
                dir.join(part)
            }),
    )
}

// The one spelling of the OS set: each OS with a login-start facility binds `native` to its
// writer under the directory that OS reads, and every other OS binds the `_` arm's, which
// answers unavailable naming the OS.
std::cfg_select! {
    target_os = "linux" => {
        /// The systemd user unit writer under the user's unit directory; unavailable when systemd
        /// is not the service manager or neither the config home nor the home is set.
        fn native() -> Result<Box<dyn LoginStart>, Unavailable> {
            if !Path::new("/run/systemd/system").is_dir() {
                return Err(Unavailable(
                    "systemd is not this host's service manager (/run/systemd/system is absent)"
                        .to_string(),
                ));
            }
            let dir = systemd_user_dir(std::env::var_os(CONFIG_HOME_ENV), crate::home_dir())
                .ok_or_else(|| {
                    Unavailable(format!(
                        "neither {CONFIG_HOME_ENV} nor {} is set",
                        crate::home::HOME_ENV
                    ))
                })?;
            Ok(Box::new(SystemdUserUnits { dir }))
        }
    }
    target_os = "macos" => {
        /// The launchd agent writer under the user's agent directory; unavailable when no home
        /// is set.
        fn native() -> Result<Box<dyn LoginStart>, Unavailable> {
            let dir = launch_agents_dir(crate::home_dir()).ok_or_else(|| {
                Unavailable(format!("{} is not set", crate::home::HOME_ENV))
            })?;
            Ok(Box::new(LaunchAgents { dir }))
        }
    }
    target_os = "windows" => {
        /// The Startup folder writer under the user's Startup folder; unavailable when the
        /// roaming application data directory is not set.
        fn native() -> Result<Box<dyn LoginStart>, Unavailable> {
            let dir = startup_dir(std::env::var_os(APPDATA_ENV))
                .ok_or_else(|| Unavailable(format!("{APPDATA_ENV} is not set")))?;
            Ok(Box::new(StartupFolder { dir }))
        }
    }
    _ => {
        /// No login-start facility on this OS: unavailable, naming the OS.
        fn native() -> Result<Box<dyn LoginStart>, Unavailable> {
            Err(Unavailable(format!(
                "no login-start adapter for {}",
                std::env::consts::OS
            )))
        }
    }
}

/// The login-start facility of the OS this binary runs on: on Linux a systemd user unit, on macOS
/// a launchd agent, on Windows a command file in the Startup folder; on any other OS, every call
/// answers unavailable naming the OS.
pub struct OsLoginStart;

impl LoginStart for OsLoginStart {
    fn register(&self, key: &str, program: &str, args: &[String]) -> Result<(), Unavailable> {
        native()?.register(key, program, args)
    }

    fn unregister(&self, key: &str) -> Result<(), Unavailable> {
        native()?.unregister(key)
    }

    fn entries(&self) -> Result<Vec<Entry>, Unavailable> {
        native()?.entries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// The key every writer row registers under.
    const KEY: &str = "rigger-run-1f2e3d4c5b6a7980";

    /// THE KEY IS THE ROOT'S HASH: a project's key is `rigger-run-` and the sixteen hex digits of
    /// the FNV-1a hash of its canonical root's text, on a unix root and a Windows one alike, and
    /// two roots give two keys.
    #[test]
    fn a_projects_key_is_rigger_run_and_the_sixteen_hex_digits_of_its_roots_hash() {
        // Each row's digits are the 64-bit FNV-1a of the root's UTF-8 bytes.
        let rows = [
            ("/home/dev/project", "rigger-run-32a7a6d10975f39b"),
            (r"\\?\C:\Users\dev\project", "rigger-run-2a0eae08fc7375e9"),
        ];
        for (root, key) in rows {
            assert_eq!(run_key(root), key, "{root}");
        }
        assert_ne!(
            run_key(rows[0].0),
            run_key(rows[1].0),
            "two roots, two keys"
        );
    }

    /// Every file and link under `dir`, by its path under `dir` with `/` between components: a
    /// file's text, a link's target after `-> `.
    fn entries(dir: &Path) -> BTreeMap<String, String> {
        let mut found = BTreeMap::new();
        let mut pending = vec![(dir.to_path_buf(), String::new())];
        while let Some((at, prefix)) = pending.pop() {
            let Ok(listing) = fs::read_dir(&at) else {
                continue;
            };
            for entry in listing.map(Result::unwrap) {
                let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
                let kind = entry.file_type().unwrap();
                if kind.is_symlink() {
                    let target = fs::read_link(entry.path()).unwrap();
                    found.insert(name, format!("-> {}", target.display()));
                } else if kind.is_dir() {
                    pending.push((entry.path(), format!("{name}/")));
                } else {
                    found.insert(name, fs::read_to_string(entry.path()).unwrap());
                }
            }
        }
        found
    }

    /// Every directory under `dir`, by its path under `dir`.
    fn directories(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_path_buf()];
        while let Some(at) = pending.pop() {
            for entry in fs::read_dir(&at).unwrap().map(Result::unwrap) {
                if entry.file_type().unwrap().is_dir() {
                    found.push(entry.path().strip_prefix(dir).unwrap().to_path_buf());
                    pending.push(entry.path());
                }
            }
        }
        found.sort();
        found
    }

    /// A writer row: GIVEN the writer `writer` makes over a directory not yet made, WHEN an entry
    /// for another program is registered under [`KEY`] and then `program` is registered twice with
    /// `run --at-login <root>`, THEN the directory holds exactly `expected`, each file or link by
    /// its path; every file is readable by all and writable by its owner alone on unix; an
    /// unregister then leaves no file or link and every directory the registers made; and an
    /// unregister of the key before any register and after the first are each `Ok`.
    fn registers(
        writer: fn(PathBuf) -> Box<dyn LoginStart>,
        program: &str,
        root: &str,
        expected: &[(&str, &str)],
    ) {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join("entries");
        let writer = writer(dir.clone());
        assert_eq!(writer.unregister(KEY), Ok(()), "an unregister before any");
        let stale = [
            "run".to_string(),
            "--at-login".to_string(),
            "/old".to_string(),
        ];
        assert_eq!(writer.register(KEY, "/old/rigger", &stale), Ok(()));
        let args = [
            "run".to_string(),
            "--at-login".to_string(),
            root.to_string(),
        ];
        for _ in 0..2 {
            assert_eq!(writer.register(KEY, program, &args), Ok(()));
        }
        let expected: BTreeMap<String, String> = expected
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect();
        assert_eq!(entries(&dir), expected, "the entry under {}", dir.display());
        #[cfg(unix)]
        for (path, _) in expected.iter().filter(|(_, text)| !text.starts_with("-> ")) {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join(path)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o644, "the mode of {path}");
        }
        let made = directories(&dir);
        assert_eq!(writer.unregister(KEY), Ok(()), "the unregister");
        assert_eq!(entries(&dir), BTreeMap::new(), "what the unregister left");
        assert_eq!(
            directories(&dir),
            made,
            "the directories the unregister left"
        );
        assert_eq!(writer.unregister(KEY), Ok(()), "a second unregister");
    }

    // GIVEN each OS's writer over a directory of its own, WHEN a program and a root holding a
    // space, a `%`, a `$` and a `&` are registered, THEN the writer writes exactly the entry its
    // OS's facility reads, a second register leaves one entry, and an unregister leaves no file
    // or link; the Linux writer's row runs on unix, for its link, and the others on every host.
    rigger_domain::test_cases! {
        /// Linux: the user unit, every word quoted with `%` and `$` doubled and a backslash and a
        /// double quote escaped, and its enabling link.
        #[cfg(unix)]
        a_systemd_user_unit_and_the_link_that_enables_it: registers(
            |dir| Box::new(SystemdUserUnits { dir }),
            "/home/dev/.cargo/bin/rigger",
            r#"/home/dev/a\b 50% $HOME & "co""#,
            &[
                (
                    "rigger-run-1f2e3d4c5b6a7980.service",
                    concat!(
                        "[Unit]\n",
                        "Description=Relaunch a rigger run whose driver died\n",
                        "\n",
                        "[Service]\n",
                        "Type=oneshot\n",
                        "RemainAfterExit=yes\n",
                        r#"ExecStart="/home/dev/.cargo/bin/rigger" "run" "--at-login" "#,
                        r#""/home/dev/a\\b 50%% $$HOME & \"co\"""#,
                        "\n",
                        "\n",
                        "[Install]\n",
                        "WantedBy=default.target\n",
                    ),
                ),
                (
                    "default.target.wants/rigger-run-1f2e3d4c5b6a7980.service",
                    "-> ../rigger-run-1f2e3d4c5b6a7980.service",
                ),
            ],
        );
        /// macOS: the agent's property list, every argument XML-escaped.
        a_launchd_agent_that_runs_at_load: registers(
            |dir| Box::new(LaunchAgents { dir }),
            "/Users/dev/.cargo/bin/rigger",
            r#"/Users/dev/50% $HOME & <"co's">"#,
            &[(
                "rigger-run-1f2e3d4c5b6a7980.plist",
                concat!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
                    "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
                    "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
                    "<plist version=\"1.0\">\n",
                    "<dict>\n",
                    "\t<key>Label</key>\n",
                    "\t<string>rigger-run-1f2e3d4c5b6a7980</string>\n",
                    "\t<key>ProgramArguments</key>\n",
                    "\t<array>\n",
                    "\t\t<string>/Users/dev/.cargo/bin/rigger</string>\n",
                    "\t\t<string>run</string>\n",
                    "\t\t<string>--at-login</string>\n",
                    "\t\t<string>/Users/dev/50% $HOME &amp; &lt;&quot;co&apos;s&quot;&gt;</string>\n",
                    "\t</array>\n",
                    "\t<key>RunAtLoad</key>\n",
                    "\t<true/>\n",
                    "\t<key>AbandonProcessGroup</key>\n",
                    "\t<true/>\n",
                    "</dict>\n",
                    "</plist>\n",
                ),
            )],
        );
        /// Windows: the command file's one line, every word quoted with `%` doubled.
        a_startup_folder_command_file: registers(
            |dir| Box::new(StartupFolder { dir }),
            r"C:\Users\dev\.cargo\bin\rigger.exe",
            r"\\?\C:\Users\dev\50% $HOME & co",
            &[(
                "rigger-run-1f2e3d4c5b6a7980.cmd",
                concat!(
                    r#"@"C:\Users\dev\.cargo\bin\rigger.exe" "run" "--at-login" "#,
                    r#""\\?\C:\Users\dev\50%% $HOME & co""#,
                    "\r\n",
                ),
            )],
        );
        /// Windows: a root ending in a backslash, doubled before its closing quote.
        a_startup_folder_command_file_for_a_drives_root: registers(
            |dir| Box::new(StartupFolder { dir }),
            r"C:\Users\dev\.cargo\bin\rigger.exe",
            r"\\?\D:\",
            &[(
                "rigger-run-1f2e3d4c5b6a7980.cmd",
                concat!(
                    r#"@"C:\Users\dev\.cargo\bin\rigger.exe" "run" "--at-login" "\\?\D:\\""#,
                    "\r\n",
                ),
            )],
        );
    }

    /// The key of an entry whose program a sweep row removes, ordered between [`KEY`] and
    /// [`FOREIGN`].
    const STALE: &str = "rigger-run-5a5a5a5a5a5a5a5a";

    /// The key of an entry a row writes in a form no writer writes.
    const FOREIGN: &str = "rigger-run-ffffffffffffffff";

    /// The arguments every listing row registers with.
    fn at_login_args() -> [String; 3] {
        ["run", "--at-login", "/home/dev/project"].map(str::to_string)
    }

    /// A read-back row: GIVEN the writer `writer` makes over a directory, WHEN `program` is
    /// registered under [`KEY`], `foreign` is written as the file `<FOREIGN>.<extension>`, and the
    /// registered entry is copied to a file of a name no key makes, THEN the writer lists the
    /// registered entry with `program` read back exactly and the foreign one with none, by key,
    /// and not the copy; over a directory not yet made it lists none.
    fn reads_back(
        writer: fn(PathBuf) -> Box<dyn LoginStart>,
        extension: &str,
        program: &str,
        foreign: &str,
    ) {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join("entries");
        let writer = writer(dir.clone());
        assert_eq!(writer.entries(), Ok(Vec::new()), "a directory not yet made");
        assert_eq!(writer.register(KEY, program, &at_login_args()), Ok(()));
        fs::write(dir.join(format!("{FOREIGN}.{extension}")), foreign).unwrap();
        fs::copy(
            dir.join(format!("{KEY}.{extension}")),
            dir.join(format!("users-own.{extension}")),
        )
        .unwrap();
        assert_eq!(
            writer.entries(),
            Ok(vec![
                Entry {
                    key: KEY.to_string(),
                    program: Some(program.to_string()),
                },
                Entry {
                    key: FOREIGN.to_string(),
                    program: None,
                },
            ]),
            "the entries under {}",
            dir.display()
        );
    }

    // GIVEN each OS's writer, WHEN a program holding every character its writer escapes is
    // registered beside a file under a key that is not in its form, THEN the writer reads the
    // program back exactly and the foreign file as none; the Linux writer's row runs on unix, for
    // its link, and the others on every host.
    rigger_domain::test_cases! {
        /// Linux: the first word of `ExecStart`, past a doubled `%` and `$`, an escaped backslash
        /// and double quote, and a C-escaped control character; an unquoted `ExecStart` is none.
        #[cfg(unix)]
        a_systemd_unit_reads_back_the_first_word_of_its_exec_start: reads_back(
            |dir| Box::new(SystemdUserUnits { dir }),
            "service",
            "/home/dev/a\\b 50% $HOME & \"co\"\u{7}",
            "[Unit]\nDescription=mine\n\n[Service]\nExecStart=/usr/bin/true\n",
        );
        /// macOS: the first string of `ProgramArguments`, not the label before it, past the five
        /// XML entities; a property list naming a `Program` alone is none.
        a_launchd_agent_reads_back_the_first_of_its_program_arguments: reads_back(
            |dir| Box::new(LaunchAgents { dir }),
            "plist",
            r#"/Users/dev/50% $HOME & <"co's">"#,
            "<plist version=\"1.0\">\n<dict>\n\t<key>Program</key>\n\t<string>/bin/true</string>\n\
             </dict>\n</plist>\n",
        );
        /// Windows: the first word of the line, past a doubled `%` and a doubled trailing
        /// backslash; a command file of commands of its own is none.
        a_startup_folder_command_file_reads_back_its_first_word: reads_back(
            |dir| Box::new(StartupFolder { dir }),
            "cmd",
            r"C:\Users\dev 50% $HOME & co\",
            "@echo off\r\nstart notepad\r\n",
        );
        /// Windows: two trailing backslashes, each doubled.
        a_startup_folder_command_file_reads_back_two_trailing_backslashes: reads_back(
            |dir| Box::new(StartupFolder { dir }),
            "cmd",
            r"C:\Users\dev\\",
            "\"C:\\Windows\\notepad.exe\"\r\n",
        );
    }

    /// A sweep row: GIVEN the writer `writer` makes over a directory holding an entry under
    /// [`KEY`] for a program that stands, an entry under [`STALE`] for a program since removed,
    /// `foreign` as the file `<FOREIGN>.<extension>`, and a file of a name no key makes, WHEN the
    /// entries are swept, THEN the stale entry is removed, every file and link its register wrote
    /// with it, and named with its program; the foreign one is named and left; and every other file
    /// stands. A second sweep names the foreign one alone.
    fn sweeps(writer: fn(PathBuf) -> Box<dyn LoginStart>, extension: &str, foreign: &str) {
        let scratch = tempfile::tempdir().unwrap();
        let dir = scratch.path().join("entries");
        let writer = writer(dir.clone());
        // A program path holding characters each writer escapes, every one valid in a file name
        // on every host.
        let standing = scratch.path().join("bin 50% $HOME & co's").join("rigger");
        fs::create_dir_all(standing.parent().unwrap()).unwrap();
        fs::write(&standing, "").unwrap();
        let standing = standing.to_str().unwrap();
        let gone = scratch.path().join("gone").join("rigger");
        let gone = gone.to_str().unwrap();
        assert_eq!(writer.register(KEY, standing, &at_login_args()), Ok(()));
        fs::write(dir.join(format!("{FOREIGN}.{extension}")), foreign).unwrap();
        fs::write(dir.join(format!("users-own.{extension}")), foreign).unwrap();
        let before = entries(&dir);
        assert_eq!(writer.register(STALE, gone, &at_login_args()), Ok(()));
        let foreign = Swept::Foreign {
            key: FOREIGN.to_string(),
        };
        assert_eq!(
            sweep(&*writer),
            Ok(vec![
                Swept::Removed {
                    key: STALE.to_string(),
                    program: gone.to_string(),
                },
                foreign.clone(),
            ]),
            "the first sweep"
        );
        assert_eq!(entries(&dir), before, "what the sweep left");
        assert_eq!(sweep(&*writer), Ok(vec![foreign]), "a second sweep");
    }

    // GIVEN each OS's writer over entries for a program that stands, for one since removed and
    // under a key in a form no writer writes, WHEN they are swept, THEN the writer removes the one
    // whose program is gone, keeps the one whose program stands, and names the foreign one and
    // leaves it; the Linux writer's row runs on unix, for its link, and the others on every host.
    rigger_domain::test_cases! {
        /// Linux: the unit and its enabling link, both removed.
        #[cfg(unix)]
        a_sweep_removes_a_systemd_unit_whose_program_is_gone: sweeps(
            |dir| Box::new(SystemdUserUnits { dir }),
            "service",
            "[Service]\nExecStart=/usr/bin/true\n",
        );
        /// macOS: the agent's property list.
        a_sweep_removes_a_launchd_agent_whose_program_is_gone: sweeps(
            |dir| Box::new(LaunchAgents { dir }),
            "plist",
            "<plist version=\"1.0\">\n<dict>\n</dict>\n</plist>\n",
        );
        /// Windows: the command file.
        a_sweep_removes_a_startup_folder_command_file_whose_program_is_gone: sweeps(
            |dir| Box::new(StartupFolder { dir }),
            "cmd",
            "@echo off\r\n",
        );
    }

    /// A facility listing one entry, under [`STALE`] for the program `gone`, whose every
    /// unregister answers `why`.
    struct Unremovable {
        gone: String,
        why: &'static str,
    }

    impl LoginStart for Unremovable {
        fn register(&self, _: &str, _: &str, _: &[String]) -> Result<(), Unavailable> {
            Ok(())
        }

        fn unregister(&self, _: &str) -> Result<(), Unavailable> {
            Err(Unavailable(self.why.to_string()))
        }

        fn entries(&self) -> Result<Vec<Entry>, Unavailable> {
            Ok(vec![Entry {
                key: STALE.to_string(),
                program: Some(self.gone.clone()),
            }])
        }
    }

    /// A SWEEP SAYS WHAT IT COULD NOT REMOVE: GIVEN an entry whose program is gone and whose
    /// facility refuses its removal, WHEN it is swept, THEN the sweep names it, its program and why,
    /// and every line a sweep says names the entry's key and, for a stale one, its program.
    #[test]
    fn a_sweep_names_an_entry_it_could_not_remove_and_each_line_names_the_key() {
        let scratch = tempfile::tempdir().unwrap();
        let gone = scratch.path().join("gone").to_str().unwrap().to_string();
        let start = Unremovable {
            gone: gone.clone(),
            why: "permission denied",
        };
        let not_removed = Swept::NotRemoved {
            key: STALE.to_string(),
            program: gone.clone(),
            why: "permission denied".to_string(),
        };
        assert_eq!(sweep(&start), Ok(vec![not_removed.clone()]));
        let rows = [
            (
                Swept::Removed {
                    key: STALE.to_string(),
                    program: gone.clone(),
                },
                format!(
                    "removed the login entry {STALE}: the program it starts, {gone}, does not \
                     exist"
                ),
                true,
            ),
            (
                not_removed,
                format!(
                    "the login entry {STALE} starts {gone}, which does not exist, and was not \
                     removed: permission denied"
                ),
                false,
            ),
            (
                Swept::Foreign {
                    key: FOREIGN.to_string(),
                },
                format!(
                    "left the login entry {FOREIGN} alone: it is not in the form rigger writes"
                ),
                false,
            ),
        ];
        for (swept, line, change) in rows {
            assert_eq!(
                (swept.line(), swept.is_change()),
                (line, change),
                "{swept:?}"
            );
        }
    }

    /// NO COMMAND FILE ENDS ITS QUOTING EARLY: GIVEN a root holding a double quote or a line
    /// break, WHEN it is registered in the Startup folder, THEN the register answers unavailable
    /// naming the root and writes nothing.
    #[test]
    fn a_startup_folder_register_refuses_a_word_that_would_end_its_quoting_and_writes_nothing() {
        for root in [r#"C:\a"b"#, "C:\\a\r\nb", "C:\\a\nb"] {
            let scratch = tempfile::tempdir().unwrap();
            let writer = StartupFolder {
                dir: scratch.path().to_path_buf(),
            };
            let args = [
                "run".to_string(),
                "--at-login".to_string(),
                root.to_string(),
            ];
            let refused = writer.register(KEY, r"C:\rigger.exe", &args);
            assert!(
                matches!(&refused, Err(Unavailable(why)) if why.contains(&format!("{root:?}"))),
                "{root:?}: {refused:?}"
            );
            assert_eq!(entries(scratch.path()), BTreeMap::new(), "{root:?}");
        }
    }

    /// Each writer's directory from the variables it is read from: `got` against `expected`.
    fn reads_dir(got: Option<PathBuf>, expected: Option<&str>) {
        assert_eq!(got, expected.map(PathBuf::from));
    }

    // GIVEN the variables each OS's directory is read from, WHEN the writer's directory is read,
    // THEN the config home wins over the home on Linux, the home roots the macOS agents, the
    // roaming application data roots the Startup folder, and an unset or empty value roots none.
    rigger_domain::test_cases! {
        the_linux_dir_is_under_the_config_home: reads_dir(
            systemd_user_dir(Some("/home/dev/cfg".into()), Some("/home/dev".into())),
            Some("/home/dev/cfg/systemd/user"),
        );
        the_linux_dir_is_under_the_homes_config_without_a_config_home: reads_dir(
            systemd_user_dir(Some("".into()), Some("/home/dev".into())),
            Some("/home/dev/.config/systemd/user"),
        );
        the_linux_dir_is_none_with_neither: reads_dir(systemd_user_dir(None, None), None);
        the_macos_dir_is_under_the_home: reads_dir(
            launch_agents_dir(Some("/Users/dev".into())),
            Some("/Users/dev/Library/LaunchAgents"),
        );
        the_macos_dir_is_none_with_an_empty_home: reads_dir(launch_agents_dir(Some("".into())), None);
        the_windows_dir_is_the_startup_folder_under_appdata: assert_eq!(
            startup_dir(Some(r"C:\Users\dev\AppData\Roaming".into())),
            Some(
                Path::new(r"C:\Users\dev\AppData\Roaming")
                    .join("Microsoft/Windows/Start Menu/Programs/Startup")
            )
        );
        the_windows_dir_is_none_without_appdata: reads_dir(startup_dir(None), None);
    }
}
