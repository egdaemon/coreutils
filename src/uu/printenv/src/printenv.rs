// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::env;
use std::io::Write;

use clap::{Arg, ArgAction, Command};

use uucore::display::{OsWrite, write_all_env_vars};
use uucore::error::UResult;
use uucore::line_ending::LineEnding;
use uucore::{format_usage, translate};

static OPT_NULL: &str = "null";

static ARG_VARIABLES: &str = "variables";

#[uucore::main(no_signals)]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    printenv(args, &mut std::io::stdout().lock())
}

/// Runs printenv like [`uumain`], isolated from the process's standard streams.
pub fn uumain_with(
    args: impl uucore::Args,
    _stdin: &mut dyn std::io::Read,
    stdout: &mut dyn Write,
) -> UResult<()> {
    printenv(args, stdout)
}

fn printenv(args: impl uucore::Args, stdout: &mut dyn Write) -> UResult<()> {
    let matches = uucore::clap_localization::handle_clap_result_with_exit_code(uu_app(), args, 2)?;

    let variables: Vec<String> = matches
        .get_many::<String>(ARG_VARIABLES)
        .map(|v| v.map(ToString::to_string).collect())
        .unwrap_or_default();

    let separator = LineEnding::from_zero_flag(matches.get_flag(OPT_NULL));

    if variables.is_empty() {
        write_all_env_vars(stdout, separator)?;
        return Ok(());
    }

    let mut error_found = false;
    for env_var in variables {
        // we silently ignore a=b as variable but we trigger an error
        if env_var.contains('=') {
            error_found = true;
            continue;
        }
        if let Some(var) = env::var_os(env_var) {
            stdout.write_all_os(&var)?;
            write!(stdout, "{separator}")?;
        } else {
            error_found = true;
        }
    }

    if error_found { Err(1.into()) } else { Ok(()) }
}

pub fn uu_app() -> Command {
    let cmd = Command::new("printenv")
        .version(uucore::crate_version!())
        .about(translate!("printenv-about"))
        .override_usage(format_usage(&translate!("printenv-usage")))
        .infer_long_args(true);
    uucore::clap_localization::configure_localized_command(cmd)
        .arg(
            Arg::new(OPT_NULL)
                .short('0')
                .long(OPT_NULL)
                .help(translate!("printenv-help-null"))
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new(ARG_VARIABLES)
                .action(ArgAction::Append)
                .num_args(1..),
        )
}
