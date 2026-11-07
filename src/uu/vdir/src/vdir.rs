// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::Command;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use uu_ls::{Config, ProgramMode, options};
use uucore::error::UResult;
use uucore::{format_usage, translate};

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    vdir(args, None)
}

/// Runs vdir like [`uumain`], isolated from the process's standard streams.
pub fn uumain_with(
    args: impl uucore::Args,
    _stdin: Box<dyn Read>,
    stdout: &mut dyn Write,
) -> UResult<()> {
    vdir(args, Some(stdout))
}

/// Lists to `stdout`, or to the process's standard output without one.
fn vdir(args: impl uucore::Args, stdout: Option<&mut dyn Write>) -> UResult<()> {
    let command = uu_app();
    // The arguments are kept for the caret in SIZE diagnostics, which echoes
    // the command line.
    let (matches, diag_args) =
        uucore::clap_localization::handle_clap_result_with_diagnostics(command, args.collect(), 2)?;

    let locs = matches
        .get_many::<OsString>(options::PATHS)
        .map_or_else(|| vec![Path::new(".")], |v| v.map(Path::new).collect());

    match stdout {
        None => uu_ls::list(locs, &Config::from_vdir(&matches, diag_args.as_deref())?),
        Some(stdout) => uu_ls::list_to(
            locs,
            &Config::isolated(&matches, diag_args.as_deref(), ProgramMode::Vdir)?,
            stdout,
        ),
    }
}

// To avoid code duplication, we reuse ls uu_app function which has the same
// arguments. However, coreutils won't compile if one of the utils is missing
// an uu_app function, so we need this dummy one.
pub fn uu_app() -> Command {
    uu_ls::uu_app()
        .name("vdir")
        .override_usage(format_usage(&translate!("vdir-usage")))
        .about(translate!("vdir-about"))
}
