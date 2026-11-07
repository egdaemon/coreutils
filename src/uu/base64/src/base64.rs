// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use clap::Command;
use uu_base32::base_common;
use uucore::translate;
use uucore::{encoding::Format, error::UResult};

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let config = base_common::parse_base_cmd_args(args, uu_app())?;
    let mut input = base_common::get_input(&config)?;
    base_common::handle_input(&mut input, Format::Base64, config)
}

/// Runs base64 like [`uumain`], isolated from the process's standard streams.
pub fn uumain_with(
    args: impl uucore::Args,
    stdin: Box<dyn std::io::Read>,
    stdout: &mut dyn std::io::Write,
) -> UResult<()> {
    let config = base_common::parse_base_cmd_args(args, uu_app())?;
    let mut input = base_common::get_input_from(&config, stdin)?;
    base_common::handle_input_to(&mut input, Format::Base64, config, stdout)
}

pub fn uu_app() -> Command {
    base_common::base_app(translate!("base64-about"), translate!("base64-usage")).name("base64")
}
