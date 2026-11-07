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
            "b2sum" => b2sum,
            "base32" => base32,
            "base64" => base64,
            "basename" => basename,
            "basenc" => basenc,
            "cat" => cat,
            "cksum" => cksum,
            "comm" => comm,
            "csplit" => csplit,
            "date" => date,
            "dircolors" => dircolors,
            "dirname" => dirname,
            "echo" => echo,
            "expand" => expand,
            "expr" => expr,
            "factor" => factor,
            "fmt" => fmt,
            "fold" => fold,
            "head" => head,
            "hostid" => hostid,
            "join" => join,
            "link" => link,
            "md5sum" => md5sum,
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
            "sha1sum" => sha1sum,
            "sha224sum" => sha224sum,
            "sha256sum" => sha256sum,
            "sha384sum" => sha384sum,
            "sha512sum" => sha512sum,
            "shuf" => shuf,
            "sleep" => sleep,
            "sum" => sum,
            "tac" => tac,
            "tail" => tail,
            "tr" => tr,
            "truncate" => truncate,
            "tsort" => tsort,
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
