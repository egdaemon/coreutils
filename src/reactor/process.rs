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
#[cfg(feature = "nice")]
use uucore::{
    display::Quotable,
    error::{FromIo, USimpleError},
};

/// The `uumain_with` of a utility.
pub type UUMain = fn(std::vec::IntoIter<OsString>, Box<dyn Read>, &mut dyn Write) -> UResult<()>;

/// A utility that consumes its stdin a chunk at a time as it arrives, so its output streams.
pub trait Stream {
    /// Consumes a chunk of stdin. An error ends the utility with its exit code, which is how a
    /// utility that needs no more of stdin finishes early.
    fn write(&mut self, chunk: &[u8], stdout: &mut dyn Write) -> Result<(), i32>;
    /// The exit code once stdin is exhausted.
    fn finish(self: Box<Self>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32;
}

/// Starts a [`Stream`] from its argv, a utility that fails to start reports to stderr and
/// returns its exit code.
pub type Spawn = fn(Vec<OsString>, &mut dyn Write) -> Result<Box<dyn Stream>, i32>;

/// A utility that writes without reading stdin, a chunk at a time, so its output is only produced
/// as fast as it is read.
pub trait Source {
    /// Writes the next chunk of output. An error ends the utility with its exit code.
    fn read(&mut self, stdout: &mut dyn Write) -> Result<(), i32>;
}

/// Starts a [`Source`] from its argv, a utility that fails to start reports to stderr and
/// returns its exit code.
pub type Open = fn(Vec<OsString>, &mut dyn Write) -> Result<Box<dyn Source>, i32>;

/// How the reactor runs a utility. A utility with a stream or source entry point is streamed,
/// the others run through their `uumain_with` once stdin is exhausted.
#[derive(Clone, Copy)]
pub enum Utility {
    Main(UUMain),
    #[allow(dead_code)] // unused when no streaming utility is enabled
    Stream(Spawn),
    #[allow(dead_code)] // unused when no source is enabled
    Source(Open),
}

/// The `uumain_with` of the utilities that have one, each when its feature is enabled.
macro_rules! mains {
    ($util:expr, $($name:literal => $krate:ident),* $(,)?) => {
        match $util {
            $(
                #[cfg(feature = $name)]
                $name => Some(::$krate::uumain_with),
            )*
            _ => None,
        }
    };
}

/// The utilities available as processes.
pub fn lookup(util: &str) -> Option<Utility> {
    match util {
        #[cfg(feature = "head")]
        "head" => Some(Utility::Stream(head)),
        #[cfg(feature = "nice")]
        "nice" => Some(Utility::Main(nice)),
        #[cfg(feature = "tee")]
        "tee" => Some(Utility::Stream(tee)),
        #[cfg(feature = "yes")]
        "yes" => Some(Utility::Source(yes)),
        util => main(util).map(Utility::Main),
    }
}

/// The `uumain_with` of a utility, which runs it to completion over the whole of its stdin.
fn main(util: &str) -> Option<UUMain> {
    mains!(util,
        "arch" => arch,
        "b2sum" => b2sum,
        "base32" => base32,
        "base64" => base64,
        "basename" => basename,
        "basenc" => basenc,
        "cat" => cat,
        "cksum" => cksum,
        "comm" => comm,
        "cp" => cp,
        "csplit" => csplit,
        "cut" => cut,
        "date" => date,
        "dd" => dd,
        "dir" => dir,
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
        "ln" => ln,
        "ls" => ls,
        "md5sum" => md5sum,
        "mkdir" => mkdir,
        "mktemp" => mktemp,
        "mv" => mv,
        "nl" => nl,
        "nproc" => nproc,
        "numfmt" => numfmt,
        "od" => od,
        "paste" => paste,
        "pathchk" => pathchk,
        "pr" => pr,
        "printenv" => printenv,
        "printf" => printf,
        "ptx" => ptx,
        "pwd" => pwd,
        "readlink" => readlink,
        "realpath" => realpath,
        "rm" => rm,
        "rmdir" => rmdir,
        "seq" => seq,
        "sha1sum" => sha1sum,
        "sha224sum" => sha224sum,
        "sha256sum" => sha256sum,
        "sha384sum" => sha384sum,
        "sha512sum" => sha512sum,
        "shred" => shred,
        "shuf" => shuf,
        "sleep" => sleep,
        "sort" => sort,
        "split" => split,
        "sum" => sum,
        "tac" => tac,
        "tail" => tail,
        "test" => uu_test,
        "touch" => touch,
        "tr" => tr,
        "true" => r#true,
        "truncate" => truncate,
        "tsort" => tsort,
        "tty" => tty,
        "uname" => uname,
        "unexpand" => unexpand,
        "uniq" => uniq,
        "unlink" => unlink,
        "vdir" => vdir,
        "wc" => wc,
    )
}

#[cfg(feature = "tee")]
impl Stream for ::tee::Tee {
    fn write(&mut self, chunk: &[u8], stdout: &mut dyn Write) -> Result<(), i32> {
        ::tee::Tee::write(self, chunk, stdout).map_err(|()| 1)
    }

    fn finish(self: Box<Self>, _: &mut dyn Write, _: &mut dyn Write) -> i32 {
        ::tee::Tee::finish(*self)
    }
}

/// A utility that runs once stdin is exhausted, for a utility that streams only some of its
/// invocations.
struct Buffered {
    main: UUMain,
    argv: Vec<OsString>,
    stdin: Vec<u8>,
}

impl Stream for Buffered {
    fn write(&mut self, chunk: &[u8], _: &mut dyn Write) -> Result<(), i32> {
        self.stdin.extend_from_slice(chunk);
        Ok(())
    }

    fn finish(self: Box<Self>, stdout: &mut dyn Write, stderr: &mut dyn Write) -> i32 {
        run(
            self.main,
            self.argv,
            Box::new(io::Cursor::new(self.stdin)),
            stdout,
            stderr,
        )
    }
}

#[cfg(feature = "head")]
impl Stream for ::head::Head {
    fn write(&mut self, chunk: &[u8], stdout: &mut dyn Write) -> Result<(), i32> {
        match Self::write(self, chunk, stdout) {
            Ok(false) => Ok(()),
            // head took all it wants.
            Ok(true) => Err(0),
            Err(_) => Err(1),
        }
    }

    fn finish(self: Box<Self>, _: &mut dyn Write, _: &mut dyn Write) -> i32 {
        0
    }
}

/// Streams head when it takes a prefix of stdin, so it finishes without waiting for the rest.
#[cfg(feature = "head")]
fn head(argv: Vec<OsString>, stderr: &mut dyn Write) -> Result<Box<dyn Stream>, i32> {
    match ::head::Head::new(argv.clone().into_iter()) {
        Ok(Some(h)) => Ok(Box::new(h)),
        Ok(None) => Ok(Box::new(Buffered {
            main: ::head::uumain_with,
            argv,
            stdin: Vec::new(),
        })),
        Err(err) => Err(report(err.as_ref(), stderr)),
    }
}

#[cfg(feature = "yes")]
impl Source for ::yes::Yes {
    fn read(&mut self, stdout: &mut dyn Write) -> Result<(), i32> {
        stdout.write_all(self.chunk()).map_err(|_| 1)
    }
}

#[cfg(feature = "yes")]
fn yes(argv: Vec<OsString>, stderr: &mut dyn Write) -> Result<Box<dyn Source>, i32> {
    match ::yes::Yes::new(argv.into_iter()) {
        Ok(y) => Ok(Box::new(y)),
        Err(err) => Err(report(err.as_ref(), stderr)),
    }
}

#[cfg(feature = "tee")]
fn tee(argv: Vec<OsString>, stderr: &mut dyn Write) -> Result<Box<dyn Stream>, i32> {
    match ::tee::Tee::new(argv.into_iter()) {
        Ok(t) => Ok(Box::new(t)),
        Err(err) => Err(report(err.as_ref(), stderr)),
    }
}

/// Runs nice's command as another utility of the reactor, at the reactor's niceness: a process
/// has no niceness of its own to adjust, and the reactor runs no other programs.
#[cfg(feature = "nice")]
fn nice(
    argv: std::vec::IntoIter<OsString>,
    stdin: Box<dyn Read>,
    stdout: &mut dyn Write,
) -> UResult<()> {
    let ::nice::Invocation::Run { argv, .. } = ::nice::parse(argv)? else {
        writeln!(stdout, "0")?;
        return Ok(());
    };
    let util = std::path::Path::new(&argv[0])
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(main) = main(&util) else {
        let err = io::Error::from(io::ErrorKind::NotFound)
            .map_err_context(|| argv[0].quote().to_string());
        return Err(USimpleError::new(127, err.to_string()));
    };

    super::invocation(&util, &argv[0]).map_err(|e| USimpleError::new(125, e))?;
    main(argv.into_iter(), stdin, stdout)
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

    /// Runs a utility as `execveatwait` does, after preparing uucore for it like `Environment::enter`.
    /// The invocation is global to uucore and the reactor runs one utility at a time, so the tests
    /// run them one at a time too.
    fn exec(args: &[&str], stdin: &[u8]) -> (i32, Vec<u8>, Vec<u8>) {
        static EXEC: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _exec = EXEC
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let argv: Vec<OsString> = args.iter().map(OsString::from).collect();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        super::super::invocation(args[0], &argv[0]).expect("an invocation");
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
                    Ok(()) => s.finish(&mut stdout, &mut stderr),
                    Err(code) => code,
                },
                Err(code) => code,
            },
            Utility::Source(_) => panic!("a source never finishes"),
        };
        (code, stdout, stderr)
    }

    #[test]
    fn unknown_utility_is_not_a_process() {
        assert!(lookup("bogus").is_none());
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
        assert_eq!(
            (s.finish(&mut Vec::new(), &mut Vec::new()), stdout),
            (0, b"abc".to_vec())
        );
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

    /// A directory of its own for a test, removed when it drops.
    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = env::temp_dir().join(format!("reactor-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self, name: &str) -> String {
            self.0.join(name).to_str().unwrap().to_string()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
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
        assert_eq!(
            exec(&["tty"], b""),
            (1, b"not a tty\n".to_vec(), Vec::new())
        );
        assert_eq!(exec(&["tty", "-s"], b""), (1, Vec::new(), Vec::new()));
    }

    #[cfg(feature = "pathchk")]
    #[test]
    fn pathchk_accepts_a_portable_path() {
        assert_eq!(
            exec(&["pathchk", "-p", "a/b.c"], b""),
            (0, Vec::new(), Vec::new())
        );
    }

    #[cfg(feature = "touch")]
    #[test]
    fn touch_creates_a_file() {
        let dir = Scratch::new("touch");
        let file = dir.0.join("f");

        assert_eq!(
            exec(&["touch", file.to_str().unwrap()], b""),
            (0, Vec::new(), Vec::new())
        );
        assert!(file.is_file());
    }

    #[cfg(feature = "touch")]
    #[test]
    fn touch_stops_at_stdout() {
        let dir = Scratch::new("touch-stdout");
        let (before, after) = (dir.0.join("before"), dir.0.join("after"));

        let (code, stdout, stderr) = exec(
            &[
                "touch",
                before.to_str().unwrap(),
                "-",
                after.to_str().unwrap(),
            ],
            b"",
        );
        assert_eq!((code, stdout), (1, Vec::new()));
        assert!(String::from_utf8(stderr).unwrap().contains("'-'"));
        assert!(before.is_file());
        assert!(!after.exists());
    }

    #[cfg(feature = "ln")]
    #[test]
    fn ln_reports_the_links_it_makes() {
        let dir = Scratch::new("ln");
        let link = dir.0.join("link");

        let (code, stdout, stderr) = exec(&["ln", "-sv", "target", link.to_str().unwrap()], b"");
        let expected = format!("'{}' -> 'target'\n", link.display());
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from("target"));
    }

    #[cfg(feature = "ln")]
    #[test]
    fn ln_asks_stdin_before_replacing() {
        let dir = Scratch::new("ln-interactive");
        let link = dir.0.join("link");
        std::os::unix::fs::symlink("old", &link).unwrap();
        let ln = |answer: &[u8]| exec(&["ln", "-sfi", "new", link.to_str().unwrap()], answer).0;

        assert_eq!(ln(b"n\n"), 1);
        assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from("old"));
        assert_eq!(ln(b"y\n"), 0);
        assert_eq!(std::fs::read_link(&link).unwrap(), PathBuf::from("new"));
    }

    #[cfg(feature = "nice")]
    #[test]
    fn nice_writes_the_niceness() {
        assert_eq!(exec(&["nice"], b""), (0, b"0\n".to_vec(), Vec::new()));
    }

    #[cfg(all(feature = "nice", feature = "seq"))]
    #[test]
    fn nice_runs_another_utility() {
        assert_eq!(
            exec(&["nice", "-n", "5", "seq", "2"], b""),
            (0, b"1\n2\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "nice")]
    #[test]
    fn nice_runs_no_other_programs() {
        let (code, stdout, stderr) = exec(&["nice", "missing"], b"");
        assert_eq!((code, stdout), (127, Vec::new()));
        assert!(String::from_utf8(stderr).unwrap().contains("'missing'"));
    }

    #[cfg(feature = "cut")]
    #[test]
    fn cut_selects_fields_of_stdin() {
        assert_eq!(
            exec(&["cut", "-d", ",", "-f", "2"], b"a,b,c\nd,e,f\n"),
            (0, b"b\ne\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "cut")]
    #[test]
    fn cut_selects_bytes_of_a_file_and_stdin() {
        let dir = env::temp_dir().join(format!("reactor-cut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"abc\n").unwrap();

        let (code, stdout, stderr) =
            exec(&["cut", "-b", "1-2", file.to_str().unwrap(), "-"], b"xyz\n");
        assert_eq!(
            (code, stdout, stderr),
            (0, b"ab\nxy\n".to_vec(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "numfmt")]
    #[test]
    fn numfmt_converts_the_lines_of_stdin() {
        assert_eq!(
            exec(&["numfmt", "--to=iec"], b"1024\n2048\n"),
            (0, b"1.0K\n2.0K\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "numfmt")]
    #[test]
    fn numfmt_converts_its_arguments() {
        assert_eq!(
            exec(&["numfmt", "--from=si", "1K", "2M"], b""),
            (0, b"1000\n2000000\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "od")]
    #[test]
    fn od_dumps_stdin() {
        assert_eq!(
            exec(&["od", "-An", "-tx1"], b"AB"),
            (0, b" 41 42\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "od")]
    #[test]
    fn od_skips_into_stdin_after_a_file() {
        let dir = env::temp_dir().join(format!("reactor-od-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"ab").unwrap();

        let (code, stdout, stderr) = exec(
            &["od", "-An", "-c", "-j", "3", file.to_str().unwrap(), "-"],
            b"xyz",
        );
        assert_eq!(
            (code, stdout, stderr),
            (0, b"   y   z\n".to_vec(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "sort")]
    #[test]
    fn sort_sorts_stdin() {
        assert_eq!(
            exec(&["sort", "-n"], b"10\n9\n100\n"),
            (0, b"9\n10\n100\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "sort")]
    #[test]
    fn sort_reads_stdin_once_among_files() {
        let dir = env::temp_dir().join(format!("reactor-sort-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"b\nd\n").unwrap();

        let (code, stdout, stderr) = exec(
            &["sort", "-u", file.to_str().unwrap(), "-", "-"],
            b"c\na\nb\n",
        );
        assert_eq!(
            (code, stdout, stderr),
            (0, b"a\nb\nc\nd\n".to_vec(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "sort")]
    #[test]
    fn sort_refuses_a_compress_program() {
        let (code, stdout, stderr) = exec(&["sort", "--compress-program=gzip"], b"b\na\n");
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(!stderr.is_empty());
    }

    #[cfg(feature = "split")]
    #[test]
    fn split_writes_a_chunk_of_stdin_to_stdout() {
        assert_eq!(
            exec(&["split", "-n", "2/2"], b"abcd"),
            (0, b"cd".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "split")]
    #[test]
    fn split_writes_files_and_reports_them() {
        let dir = env::temp_dir().join(format!("reactor-split-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let prefix = dir.join("x");

        let (code, stdout, stderr) = exec(
            &[
                "split",
                "-l",
                "1",
                "--verbose",
                "-",
                prefix.to_str().unwrap(),
            ],
            b"a\nb\n",
        );
        // the message is localized, so only the report of each file is checked.
        assert_eq!((code, stderr), (0, Vec::new()));
        assert_eq!(String::from_utf8(stdout).unwrap().lines().count(), 2);
        assert_eq!(std::fs::read(dir.join("xaa")).unwrap(), b"a\n");
        assert_eq!(std::fs::read(dir.join("xab")).unwrap(), b"b\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "split")]
    #[test]
    fn split_refuses_a_filter() {
        let (code, stdout, stderr) = exec(&["split", "--filter=cat"], b"a\n");
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(!stderr.is_empty());
    }

    #[cfg(feature = "b2sum")]
    #[test]
    fn b2sum_digests_stdin() {
        assert_eq!(
            exec(&["b2sum"], b"abc"),
            (0, b"ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923  -\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "md5sum")]
    #[test]
    fn md5sum_digests_stdin() {
        assert_eq!(
            exec(&["md5sum"], b"abc"),
            (
                0,
                b"900150983cd24fb0d6963f7d28e17f72  -\n".to_vec(),
                Vec::new()
            )
        );
    }

    #[cfg(feature = "sha1sum")]
    #[test]
    fn sha1sum_digests_stdin() {
        assert_eq!(
            exec(&["sha1sum"], b"abc"),
            (
                0,
                b"a9993e364706816aba3e25717850c26c9cd0d89d  -\n".to_vec(),
                Vec::new()
            )
        );
    }

    #[cfg(feature = "sha224sum")]
    #[test]
    fn sha224sum_digests_stdin() {
        assert_eq!(
            exec(&["sha224sum"], b"abc"),
            (
                0,
                b"23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7  -\n".to_vec(),
                Vec::new()
            )
        );
    }

    #[cfg(feature = "sha256sum")]
    #[test]
    fn sha256sum_digests_stdin() {
        assert_eq!(
            exec(&["sha256sum"], b"abc"),
            (
                0,
                b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  -\n".to_vec(),
                Vec::new()
            )
        );
    }

    #[cfg(feature = "sha384sum")]
    #[test]
    fn sha384sum_digests_stdin() {
        assert_eq!(
            exec(&["sha384sum"], b"abc"),
            (0, b"cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  -\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "sha512sum")]
    #[test]
    fn sha512sum_digests_stdin() {
        assert_eq!(
            exec(&["sha512sum"], b"abc"),
            (0, b"ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f  -\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "cksum")]
    #[test]
    fn cksum_checksums_stdin() {
        assert_eq!(
            exec(&["cksum"], b"abc"),
            (0, b"1219131554 3\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "cksum")]
    #[test]
    fn cksum_checks_tagged_lines_from_stdin() {
        let dir = env::temp_dir().join(format!("reactor-cksum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"abc").unwrap();

        let list = format!(
            "SHA256 ({}) = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n",
            file.display()
        );
        let (code, stdout, stderr) = exec(&["cksum", "--check"], list.as_bytes());
        let expected = format!("{}: OK\n", file.display());
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "sha256sum")]
    #[test]
    fn sha256sum_checks_a_list_from_stdin() {
        let dir = env::temp_dir().join(format!("reactor-sha256sum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"abc").unwrap();

        let list = format!(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  {}\n",
            file.display()
        );
        let (code, stdout, stderr) = exec(&["sha256sum", "-c"], list.as_bytes());
        let expected = format!("{}: OK\n", file.display());
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(feature = "md5sum")]
    #[test]
    fn md5sum_reports_a_mismatch() {
        let dir = env::temp_dir().join(format!("reactor-md5sum-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("f");
        std::fs::write(&file, b"abd").unwrap();

        let list = format!("900150983cd24fb0d6963f7d28e17f72  {}\n", file.display());
        let (code, stdout, _) = exec(&["md5sum", "-c"], list.as_bytes());
        let expected = format!("{}: FAILED\n", file.display());
        assert_eq!((code, stdout), (1, expected.into_bytes()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A directory holding the files `b` and `a` and the directory `sub`, removed on drop.
    #[cfg(any(feature = "ls", feature = "dir", feature = "vdir"))]
    struct Listing(std::path::PathBuf);

    #[cfg(any(feature = "ls", feature = "dir", feature = "vdir"))]
    impl Listing {
        fn new(name: &str) -> Self {
            let dir = env::temp_dir().join(format!("reactor-{name}-{}", std::process::id()));
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            std::fs::write(dir.join("b"), b"").unwrap();
            std::fs::write(dir.join("a"), b"").unwrap();
            Self(dir)
        }

        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }

    #[cfg(any(feature = "ls", feature = "dir", feature = "vdir"))]
    impl Drop for Listing {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(feature = "ls")]
    #[test]
    fn ls_lists_one_entry_per_line() {
        let dir = Listing::new("ls");
        assert_eq!(
            exec(&["ls", dir.path()], b""),
            (0, b"a\nb\nsub\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "ls")]
    #[test]
    fn ls_lists_in_columns_of_the_given_width() {
        let dir = Listing::new("ls-columns");
        assert_eq!(
            exec(&["ls", "-C", "-w", "8", dir.path()], b""),
            (0, b"a  sub\nb\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "ls")]
    #[test]
    fn ls_classifies_only_when_asked_to_always() {
        let dir = Listing::new("ls-classify");
        assert_eq!(
            exec(&["ls", "--classify=auto", dir.path()], b""),
            (0, b"a\nb\nsub\n".to_vec(), Vec::new())
        );
        assert_eq!(
            exec(&["ls", "--classify=always", dir.path()], b""),
            (0, b"a\nb\nsub/\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "ls")]
    #[test]
    fn ls_colours_only_when_asked_to_always() {
        let dir = Listing::new("ls-color");
        let (code, stdout, _) = exec(&["ls", "--color=auto", dir.path()], b"");
        assert_eq!((code, stdout), (0, b"a\nb\nsub\n".to_vec()));
        let (code, stdout, _) = exec(&["ls", "--color=always", dir.path()], b"");
        assert_eq!(code, 0);
        assert!(stdout.contains(&0x1b));
    }

    #[cfg(feature = "dir")]
    #[test]
    fn dir_lists_in_columns() {
        let dir = Listing::new("dir");
        assert_eq!(
            exec(&["dir", "-w", "80", dir.path()], b""),
            (0, b"a  b  sub\n".to_vec(), Vec::new())
        );
    }

    #[cfg(feature = "vdir")]
    #[test]
    fn vdir_lists_in_long_format() {
        let dir = Listing::new("vdir");
        let (code, stdout, stderr) = exec(&["vdir", dir.path()], b"");
        let stdout = String::from_utf8(stdout).unwrap();
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!((code, stderr), (0, Vec::new()));
        assert!(lines[0].starts_with("total "), "{stdout}");
        assert!(
            lines[1].starts_with('-') && lines[1].ends_with(" a"),
            "{stdout}"
        );
        assert!(
            lines[2].starts_with('-') && lines[2].ends_with(" b"),
            "{stdout}"
        );
        assert!(
            lines[3].starts_with('d') && lines[3].ends_with(" sub"),
            "{stdout}"
        );
        assert_eq!(lines.len(), 4, "{stdout}");
    }

    #[cfg(feature = "cp")]
    #[test]
    fn cp_reports_each_copy_on_stdout() {
        let dir = Scratch::new("cp-verbose");
        let (src, dst) = (dir.path("src"), dir.path("dst"));
        std::fs::write(&src, b"x").unwrap();

        let (code, stdout, stderr) = exec(&["cp", "-v", &src, &dst], b"");
        let expected = format!("'{src}' -> '{dst}'\n");
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        assert_eq!(std::fs::read(&dst).unwrap(), b"x");
    }

    #[cfg(feature = "cp")]
    #[test]
    fn cp_reads_its_prompt_answer_from_stdin() {
        let dir = Scratch::new("cp-interactive");
        let (src, dst) = (dir.path("src"), dir.path("dst"));
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(&dst, b"old").unwrap();

        let (code, _, _) = exec(&["cp", "-i", &src, &dst], b"y\n");
        assert_eq!(code, 0);
        assert_eq!(std::fs::read(&dst).unwrap(), b"new");
    }

    #[cfg(feature = "mv")]
    #[test]
    fn mv_reports_each_rename_on_stdout() {
        let dir = Scratch::new("mv-verbose");
        let (src, dst) = (dir.path("src"), dir.path("dst"));
        std::fs::write(&src, b"x").unwrap();

        let (code, stdout, stderr) = exec(&["mv", "-v", &src, &dst], b"");
        let expected = format!("renamed '{src}' -> '{dst}'\n");
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        assert_eq!(std::fs::read(&dst).unwrap(), b"x");
    }

    #[cfg(feature = "mv")]
    #[test]
    fn mv_reads_its_prompt_answer_from_stdin() {
        let dir = Scratch::new("mv-interactive");
        let (src, dst) = (dir.path("src"), dir.path("dst"));
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(&dst, b"old").unwrap();

        let (code, _, _) = exec(&["mv", "-i", &src, &dst], b"y\n");
        assert_eq!(code, 0);
        assert_eq!(std::fs::read(&dst).unwrap(), b"new");
        assert!(!std::path::Path::new(&src).exists());
    }

    #[cfg(feature = "rm")]
    #[test]
    fn rm_reports_each_removal_on_stdout() {
        let dir = Scratch::new("rm-verbose");
        let file = dir.path("f");
        std::fs::write(&file, b"x").unwrap();

        let (code, stdout, stderr) = exec(&["rm", "-v", &file], b"");
        let expected = format!("removed '{file}'\n");
        assert_eq!(
            (code, stdout, stderr),
            (0, expected.into_bytes(), Vec::new())
        );
        assert!(!std::path::Path::new(&file).exists());
    }

    #[cfg(feature = "rm")]
    #[test]
    fn rm_reads_its_prompt_answer_from_stdin() {
        let dir = Scratch::new("rm-interactive");
        let file = dir.path("f");
        std::fs::write(&file, b"x").unwrap();

        let (code, _, _) = exec(&["rm", "-i", &file], b"y\n");
        assert_eq!(code, 0);
        assert!(!std::path::Path::new(&file).exists());
    }

    #[cfg(feature = "dd")]
    #[test]
    fn dd_copies_blocks_of_stdin() {
        let (code, stdout, _) = exec(
            &[
                "dd",
                "bs=2",
                "skip=1",
                "count=2",
                "conv=ucase",
                "status=none",
            ],
            b"abcdefgh",
        );
        assert_eq!((code, stdout), (0, b"CDEF".to_vec()));
    }

    #[cfg(feature = "dd")]
    #[test]
    fn dd_pads_its_stdout_to_seek() {
        let (code, stdout, _) = exec(&["dd", "bs=1", "seek=2", "status=none"], b"x");
        assert_eq!((code, stdout), (0, b"\0\0x".to_vec()));
    }

    #[cfg(feature = "dd")]
    #[test]
    fn dd_copies_a_file_to_stdout_and_stdin_to_a_file() {
        let dir = Scratch::new("dd");
        let (src, dst) = (dir.path("src"), dir.path("dst"));
        std::fs::write(&src, b"file").unwrap();

        let (code, stdout, _) = exec(&["dd", &format!("if={src}"), "status=none"], b"");
        assert_eq!((code, stdout), (0, b"file".to_vec()));

        let (code, stdout, _) = exec(&["dd", &format!("of={dst}"), "status=none"], b"stdin");
        assert_eq!((code, stdout), (0, Vec::new()));
        assert_eq!(std::fs::read(&dst).unwrap(), b"stdin");
    }

    #[cfg(feature = "shred")]
    #[test]
    fn shred_overwrites_a_file_with_zeros() {
        let dir = Scratch::new("shred-zero");
        let file = dir.path("f");
        std::fs::write(&file, b"secret").unwrap();

        let (code, stdout, stderr) = exec(&["shred", "-n", "0", "-z", "-x", &file], b"");
        assert_eq!((code, stdout, stderr), (0, Vec::new(), Vec::new()));
        assert_eq!(std::fs::read(&file).unwrap(), b"\0\0\0\0\0\0");
    }

    #[cfg(feature = "shred")]
    #[test]
    fn shred_removes_a_file() {
        let dir = Scratch::new("shred-remove");
        let file = dir.path("f");
        std::fs::write(&file, b"secret").unwrap();

        let (code, _, _) = exec(&["shred", "-n", "1", "-u", &file], b"");
        assert_eq!(code, 0);
        assert!(!std::path::Path::new(&file).exists());
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_finishes_once_it_took_its_lines() {
        let Some(Utility::Stream(spawn)) = lookup("head") else {
            panic!("head streams");
        };
        let mut s = spawn(
            vec!["head".into(), "-n".into(), "2".into()],
            &mut Vec::new(),
        )
        .unwrap();
        let mut stdout = Vec::new();
        assert_eq!(s.write(b"a\nb", &mut stdout), Ok(()));
        assert_eq!(s.write(b"\nc\n", &mut stdout), Err(0));
        assert_eq!(stdout, b"a\nb\n");
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_finishes_once_it_took_its_bytes() {
        let Some(Utility::Stream(spawn)) = lookup("head") else {
            panic!("head streams");
        };
        let mut s = spawn(
            vec!["head".into(), "-c".into(), "3".into()],
            &mut Vec::new(),
        )
        .unwrap();
        let mut stdout = Vec::new();
        assert_eq!(s.write(b"ab", &mut stdout), Ok(()));
        assert_eq!(s.write(b"cd", &mut stdout), Err(0));
        assert_eq!(stdout, b"abc");
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_takes_no_lines() {
        assert_eq!(
            exec(&["head", "-n", "0"], b"a\n"),
            (0, Vec::new(), Vec::new())
        );
    }

    #[cfg(feature = "head")]
    #[test]
    fn head_reports_a_bad_count_before_it_starts() {
        let (code, stdout, stderr) = exec(&["head", "-n", "x"], b"");
        assert_eq!((code, stdout), (1, Vec::new()));
        assert!(!stderr.is_empty());
    }

    #[cfg(feature = "yes")]
    #[test]
    fn yes_writes_a_bounded_chunk_at_a_time() {
        let Some(Utility::Source(open)) = lookup("yes") else {
            panic!("yes is a source");
        };
        let mut s = open(vec!["yes".into(), "ab".into()], &mut Vec::new()).unwrap();
        let mut stdout = Vec::new();
        s.read(&mut stdout).unwrap();
        let chunk = stdout.len();
        assert!(chunk > 0 && chunk <= 64 * 1024, "{chunk}");
        assert!(stdout.chunks(3).all(|line| line == b"ab\n"));

        s.read(&mut stdout).unwrap();
        assert_eq!(stdout.len(), 2 * chunk);
    }

    #[cfg(feature = "yes")]
    #[test]
    fn yes_reports_a_bad_option() {
        let Some(Utility::Source(open)) = lookup("yes") else {
            panic!("yes is a source");
        };
        assert_eq!(
            open(vec!["yes".into(), "--bogus".into()], &mut Vec::new()).err(),
            Some(1)
        );
    }

    #[cfg(all(feature = "nice", feature = "head"))]
    #[test]
    fn nice_runs_a_streaming_utility_to_completion() {
        assert_eq!(
            exec(&["nice", "head", "-n", "1"], b"a\nb\n"),
            (0, b"a\n".to_vec(), Vec::new())
        );
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
