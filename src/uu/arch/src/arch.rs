// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::Command;
use platform_info::{PlatformInfo, PlatformInfoAPI, UNameAPI};
use std::io::Write;
use uucore::error::{UResult, USimpleError};
use uucore::translate;

#[uucore::main(no_signals)]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    arch(args, &mut std::io::stdout().lock())
}

/// Runs arch like [`uumain`], isolated from the process's standard streams.
pub fn uumain_with(
    args: impl uucore::Args,
    _stdin: Box<dyn std::io::Read>,
    stdout: &mut dyn Write,
) -> UResult<()> {
    arch(args, stdout)
}

fn arch(args: impl uucore::Args, stdout: &mut dyn Write) -> UResult<()> {
    uucore::clap_localization::handle_clap_result(uu_app(), args)?;

    let uts =
        PlatformInfo::new().map_err(|_| USimpleError::new(1, translate!("cannot-get-system")))?;
    // machine is a valid unicode
    let machine = uts.machine().as_encoded_bytes().trim_ascii();
    stdout.write_all(machine)?;
    Ok(stdout.write_all(b"\n")?)
}

pub fn uu_app() -> Command {
    Command::new("arch")
        .version(uucore::crate_version!())
        .help_template(uucore::localized_help_template("arch"))
        .about(translate!("arch-about"))
        .after_help(translate!("arch-after-help"))
        .override_usage(translate!("arch-usage"))
        .infer_long_args(true)
}
