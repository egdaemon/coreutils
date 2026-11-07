// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

pub mod base_common;

use clap::Command;
use uucore::{encoding::Format, error::UResult, translate};

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    let config = base_common::parse_base_cmd_args(args, uu_app())?;
    let mut input = base_common::get_input(&config)?;
    base_common::handle_input(&mut input, Format::Base32, config)
}

/// Runs base32 like [`uumain`], isolated from the process's standard streams.
pub fn uumain_with(
    args: impl uucore::Args,
    stdin: Box<dyn std::io::Read>,
    stdout: &mut dyn std::io::Write,
) -> UResult<()> {
    let config = base_common::parse_base_cmd_args(args, uu_app())?;
    let mut input = base_common::get_input_from(&config, stdin)?;
    base_common::handle_input_to(&mut input, Format::Base32, config, stdout)
}

pub fn uu_app() -> Command {
    base_common::base_app(translate!("base32-about"), translate!("base32-usage")).name("base32")
}
