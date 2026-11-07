// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//! Utilities the reactor runs as processes (see `execveat`).
//!
//! Every utility is run through its `uumain_with`, which takes the command's stdin and stdout
//! instead of the process's standard streams, so commands stay isolated from each other.

use std::env;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use uucore::error::{UError, UResult};

/// The `uumain_with` of a utility.
pub type UUMain = fn(std::vec::IntoIter<OsString>, Box<dyn Read>, &mut dyn Write) -> UResult<()>;

/// A utility that consumes its stdin a chunk at a time as it arrives, so its output streams.
pub trait Stream {
    /// Consumes a chunk of stdin, an error is the utility's exit code.
    fn write(&mut self, chunk: &[u8], stdout: &mut dyn Write) -> Result<(), i32>;
    /// The exit code once stdin is exhausted.
    fn finish(self: Box<Self>) -> i32;
}

/// Starts a [`Stream`] from its argv, a utility that fails to start reports to stderr and
/// returns its exit code.
pub type Spawn = fn(Vec<OsString>, &mut dyn Write) -> Result<Box<dyn Stream>, i32>;

/// How the reactor runs a utility. A utility with a stream entry point is streamed, the others
/// run through their `uumain_with` once stdin is exhausted.
#[derive(Clone, Copy)]
pub enum Utility {
    Main(UUMain),
    #[allow(dead_code)] // unused when no streaming utility is enabled
    Stream(Spawn),
}

/// The utilities that run through their `uumain_with`, each when its feature is enabled.
macro_rules! mains {
    ($util:expr, $($name:literal => $krate:ident),* $(,)?) => {
        match $util {
            $(
                #[cfg(feature = $name)]
                $name => Some(Utility::Main(::$krate::uumain_with)),
            )*
            _ => None,
        }
    };
}

/// The utilities available as processes.
pub fn lookup(util: &str) -> Option<Utility> {
    match util {
        #[cfg(feature = "tee")]
        "tee" => Some(Utility::Stream(tee)),
        util => mains!(util,
            "arch" => arch,
            "base32" => base32,
            "base64" => base64,
            "basename" => basename,
            "basenc" => basenc,
            "cat" => cat,
            "comm" => comm,
            "csplit" => csplit,
            "date" => date,
            "dircolors" => dircolors,
            "dirname" => dirname,
            "echo" => echo,
            "expand" => expand,
            "expr" => expr,
            "factor" => factor,
            "false" => r#false,
            "fmt" => fmt,
            "fold" => fold,
            "head" => head,
            "hostid" => hostid,
            "join" => join,
            "link" => link,
            "mkdir" => mkdir,
            "mktemp" => mktemp,
            "nl" => nl,
            "nproc" => nproc,
            "paste" => paste,
            "pr" => pr,
            "printenv" => printenv,
            "printf" => printf,
            "ptx" => ptx,
            "pwd" => pwd,
            "readlink" => readlink,
            "realpath" => realpath,
            "rmdir" => rmdir,
            "seq" => seq,
            "shuf" => shuf,
            "sleep" => sleep,
            "sum" => sum,
            "tac" => tac,
            "tail" => tail,
            "test" => uu_test,
            "tr" => tr,
            "truncate" => truncate,
            "true" => r#true,
            "tsort" => tsort,
            "tty" => tty,
            "uname" => uname,
            "unexpand" => unexpand,
            "uniq" => uniq,
            "unlink" => unlink,
            "wc" => wc,
        ),
    }
}

#[cfg(feature = "tee")]
impl Stream for ::tee::Tee {
    fn write(&mut self, chunk: &[u8], stdout: &mut dyn Write) -> Result<(), i32> {
        ::tee::Tee::write(self, chunk, stdout).map_err(|()| 1)
    }

    fn finish(self: Box<Self>) -> i32 {
        ::tee::Tee::finish(*self)
    }
}

#[cfg(feature = "tee")]
fn tee(argv: Vec<OsString>, stderr: &mut dyn Write) -> Result<Box<dyn Stream>, i32> {
    match ::tee::Tee::new(argv.into_iter()) {
        Ok(t) => Ok(Box::new(t)),
        Err(err) => Err(report(err.as_ref(), stderr)),
    }
}

/// Runs a utility to completion, returning its exit code. A failure is reported to stderr the
/// way `uucore::main` does.
pub fn run(
    main: UUMain,
    argv: Vec<OsString>,
    stdin: Box<dyn Read>,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    match main(argv.into_iter(), stdin, stdout) {
        Ok(()) => uucore::error::get_exit_code(),
        Err(err) => report(err.as_ref(), stderr),
    }
}

fn report(err: &dyn UError, stderr: &mut dyn Write) -> i32 {
    let msg = err.to_string();
    if !msg.is_empty() {
        let _ = writeln!(stderr, "{}: {msg}", uucore::util_name());
    }
    if err.usage() {
        let _ = writeln!(
            stderr,
            "Try '{} --help' for more information.",
            uucore::execution_phrase()
        );
    }
    err.code()
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
/// The environment and working directory of a command. The reactor is shared by every command,
/// so a command's environment is only applied while the reactor calls into its process.
pub struct Environment {
    pub util: String,
    pub argv0: OsString,
    pub vars: Vec<(OsString, OsString)>,
    pub cwd: PathBuf,
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
impl Environment {
    /// Applies the environment until the returned guard drops, which restores the previous one.
    pub fn enter(&self) -> io::Result<Restore> {
        let restore = Restore {
            vars: env::vars_os().collect(),
            cwd: env::current_dir()?,
        };
        replace(&self.vars);
        chdir(&self.cwd)?;
        super::invocation(&self.util, &self.argv0).map_err(io::Error::other)?;
        Ok(restore)
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
pub struct Restore {
    vars: Vec<(OsString, OsString)>,
    cwd: PathBuf,
}

impl Drop for Restore {
    fn drop(&mut self) {
        replace(&self.vars);
        let _ = chdir(&self.cwd);
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
fn replace(vars: &[(OsString, OsString)]) {
    // SAFETY: the reactor is single threaded (wasm32-wasip1), nothing reads the environment
    // concurrently.
    unsafe {
        for (k, _) in env::vars_os() {
            env::remove_var(k);
        }
        for (k, v) in vars {
            env::set_var(k, v);
        }
    }
}

#[cfg_attr(not(target_os = "wasi"), allow(dead_code))] // used by execveat
/// chdir through libc, which tracks the working directory of a wasi module.
fn chdir(dir: &std::path::Path) -> io::Result<()> {
    let dir = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes())?;
    // SAFETY: dir is a NUL terminated string that outlives the call.
    if unsafe { libc::chdir(dir.as_ptr()) } == 0 {
        return Ok(());
    }
    Err(io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(args: &[&str], stdin: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
        let argv = args.iter().map(OsString::from).collect();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = match lookup(args[0]).expect("a process") {
            Utility::Main(main) => run(
                main,
                argv,
                Box::new(io::Cursor::new(stdin.to_vec())),
                &mut stdout,
                &mut stderr,
            ),
            Utility::Stream(spawn) => match spawn(argv, &mut stderr) {
                Ok(mut s) => match s.write(stdin, &mut stdout) {
                    Ok(()) => s.finish(),
                    Err(code) => code,
                },
                Err(code) => code,
            },
        };
        (code, stdout, stderr)
    }

    #[test]
    fn unknown_utility_is_not_a_process() {
        assert!(lookup("ls").is_none());
    }

    #[cfg(feature = "echo")]
    #[test]
    fn echo_writes_its_arguments() {
        assert_eq!(
            exec(&["echo", "-n", "a", "b"], b""),
            (0, b"a b".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "echo")]
    #[test]
    fn echo_interprets_escapes() {
        assert_eq!(
            exec(&["echo", "-e", r"a\tb"], b""),
            (0, b"a\tb\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "tee")]
    #[test]
    fn tee_copies_stdin_to_stdout_and_files() {
        let dir = env::temp_dir().join(format!("reactor-tee-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("out");

        let (code, stdout, stderr) = exec(&["tee", file.to_str().unwrap()], b"abc");
        assert_eq!((code, stdout, stderr), (0, b"abc".to_vec(), Vec::new()));
        assert_eq!(std::fs::read(&file).unwrap(), b"abc");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "tee")]
    #[test]
    fn tee_streams_each_chunk_before_stdin_is_exhausted() {
        let Some(Utility::Stream(spawn)) = lookup("tee") else {
            panic!("tee streams");
        };
        let mut s = spawn(vec!["tee".into()], &mut Vec::new()).unwrap();
        let mut stdout = Vec::new();
        s.write(b"ab", &mut stdout).unwrap();
        assert_eq!(stdout, b"ab");
        s.write(b"c", &mut stdout).unwrap();
        assert_eq!((s.finish(), stdout), (0, b"abc".to_vec()));
    }

    #[cfg(feature = "tee")]
    #[test]
    fn tee_rejects_unknown_options() {
        let (code, stdout, _) = exec(&["tee", "--bogus"], b"abc");
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
    }

    #[cfg(feature = "tr")]
    #[test]
    fn tr_translates_its_input() {
        assert_eq!(
            exec(&["tr", "a-z", "A-Z"], b"hello\n"),
            (0, b"HELLO\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "tr")]
    #[test]
    fn tr_reports_a_missing_operand() {
        let (code, stdout, stderr) = exec(&["tr"], b"");
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(!stderr.is_empty());
    }

    #[cfg(feature = "seq")]
    #[test]
    fn seq_counts() {
        assert_eq!(
            exec(&["seq", "1", "3"], b""),
            (0, b"1\n2\n3\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "basename")]
    #[test]
    fn basename_strips_the_directory_and_suffix() {
        assert_eq!(
            exec(&["basename", "/a/b.txt", ".txt"], b""),
            (0, b"b\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "factor")]
    #[test]
    fn factor_reads_numbers_from_stdin() {
        assert_eq!(
            exec(&["factor"], b"12\n"),
            (0, b"12: 2 2 3\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "uniq")]
    #[test]
    fn uniq_filters_stdin() {
        assert_eq!(
            exec(&["uniq"], b"a\na\nb\n"),
            (0, b"a\nb\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "paste")]
    #[test]
    fn paste_shares_stdin_between_operands() {
        assert_eq!(
            exec(&["paste", "-", "-"], b"a\nb\nc\nd\n"),
            (0, b"a\tb\nc\td\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_takes_the_first_lines_of_stdin() {
        assert_eq!(
            exec(&["head", "-n", "2"], b"a\nb\nc\n"),
            (0, b"a\nb\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_drops_the_last_bytes_of_stdin() {
        assert_eq!(
            exec(&["head", "-c", "-2"], b"abcd"),
            (0, b"ab".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_names_each_file_it_reads() {
        let dir = env::temp_dir().join(format!("reactor-head-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"1\n2\n").unwrap();

        let (code, stdout, stderr) =
            exec(&["head", "-n", "1", file.to_str().unwrap(), "-"], b"x\ny\n");
        let expected = format!(
            "==> {} <==\n1\n\n==> 'standard input' <==\nx\n",
            file.display()
        );
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "wc")]
    #[test]
    fn wc_counts_stdin() {
        assert_eq!(
            exec(&["wc"], b"a b\nc\n"),
            (0, b"      2       3       6\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "wc")]
    #[test]
    fn wc_counts_the_bytes_of_stdin() {
        assert_eq!(
            exec(&["wc", "-c"], b"abcd"),
            (0, b"4\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "wc")]
    #[test]
    fn wc_reads_file_names_from_stdin() {
        let dir = env::temp_dir().join(format!("reactor-wc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"1\n2\n").unwrap();

        let (code, stdout, stderr) = exec(
            &["wc", "-l", "--files0-from=-"],
            format!("{}\0", file.display()).as_bytes(),
        );
        let expected = format!("2 {}\n", file.display());
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "cat")]
    #[test]
    fn cat_copies_stdin() {
        assert_eq!(exec(&["cat"], b"a\nb"), (0, b"a\nb".to_vec(), Vec::new()));
    }

    #[cfg(feature = "cat")]
    #[test]
    fn cat_numbers_a_file_and_stdin_in_turn() {
        let dir = env::temp_dir().join(format!("reactor-cat-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"x\n").unwrap();

        let (code, stdout, stderr) = exec(&["cat", "-n", file.to_str().unwrap(), "-"], b"y\tz\r");
        assert_eq!(
            (code, stdout, stderr),
            (0, b"     1\tx\n     2\ty\tz\r".to_vec(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "cat")]
    #[test]
    fn cat_shows_ends_and_tabs() {
        assert_eq!(
            exec(&["cat", "-A"], b"a\tb\n"),
            (0, b"a^Ib$\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "tail")]
    #[test]
    fn tail_takes_the_last_lines_of_stdin() {
        assert_eq!(
            exec(&["tail", "-n", "2"], b"a\nb\nc\n"),
            (0, b"b\nc\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "tail")]
    #[test]
    fn tail_starts_at_a_line_of_stdin() {
        assert_eq!(
            exec(&["tail", "-n", "+2"], b"a\nb\nc\n"),
            (0, b"b\nc\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "tail")]
    #[test]
    fn tail_names_each_file_it_reads() {
        let dir = env::temp_dir().join(format!("reactor-tail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"1\n2\n").unwrap();

        let (code, stdout, stderr) =
            exec(&["tail", "-c", "2", file.to_str().unwrap(), "-"], b"xyz");
        let expected = format!(
            "==> {} <==\n2\n\n==> 'standard input' <==\nyz",
            file.display()
        );
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "tail")]
    #[test]
    fn tail_refuses_to_follow() {
        let (code, stdout, stderr) = exec(&["tail", "-f", "/dev/null"], b"");
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(!stderr.is_empty());
    }

    #[cfg(feature = "true")]
    #[test]
    fn true_succeeds() {
        assert_eq!(exec(&["true"], b""), (0, Vec::new(), Vec::new()));
    }

    #[cfg(feature = "true")]
    #[test]
    fn true_writes_its_version() {
        let (code, stdout, stderr) = exec(&["true", "--version"], b"");
        assert_eq!((code, stderr), (0, Vec::new()));
        assert!(stdout.starts_with(b"true "));
    }

    #[cfg(feature = "false")]
    #[test]
    fn false_fails() {
        assert_eq!(exec(&["false"], b""), (1, Vec::new(), Vec::new()));
    }

    #[cfg(feature = "test")]
    #[test]
    fn test_evaluates_its_expression() {
        assert_eq!(exec(&["test", "a", "=", "a"], b"").0, 0);
        assert_eq!(exec(&["test", "a", "=", "b"], b"").0, 1);
    }

    #[cfg(feature = "test")]
    #[test]
    fn test_has_no_terminals() {
        assert_eq!(exec(&["test", "-t", "0"], b""), (1, Vec::new(), Vec::new()));
        assert_eq!(exec(&["test", "-t", "1"], b""), (1, Vec::new(), Vec::new()));
    }

    #[cfg(feature = "tty")]
    #[test]
    fn tty_is_not_a_terminal() {
        // the reactor sets up the utility's messages before running it.
        super::super::invocation("tty", &"tty".into()).unwrap();
        assert_eq!(
            exec(&["tty"], b""),
            (1, b"not a tty\n".to_vec(), Vec::new())
        );
        assert_eq!(exec(&["tty", "-s"], b""), (1, Vec::new(), Vec::new()));
    }

    #[cfg(feature = "pwd")]
    #[test]
    fn pwd_writes_the_working_directory() {
        let mut expected = env::current_dir()
            .unwrap()
            .into_os_string()
            .into_encoded_bytes();
        expected.push(b'\n');
        assert_eq!(exec(&["pwd", "-P"], b""), (0, expected, Vec::new()));
    }
}
