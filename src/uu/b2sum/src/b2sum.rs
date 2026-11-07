// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

// spell-checker:ignore (ToDO) algo

use clap::Command;
use std::io::{Read, Write};

use uu_checksum_common::{standalone_checksum_app_with_length, standalone_with_length_main};

use uucore::checksum::{AlgoKind, BlakeLength, Streams, parse_blake_length};
use uucore::error::UResult;
use uucore::translate;

#[uucore::main]
pub fn uumain(args: impl uucore::Args) -> UResult<()> {
    b2sum(args, &mut Streams::process())
}

/// Runs b2sum like [`uumain`], isolated from the process's standard streams.
pub fn uumain_with(
    args: impl uucore::Args,
    stdin: Box<dyn Read>,
    stdout: &mut dyn Write,
) -> UResult<()> {
    b2sum(args, &mut Streams::given(stdin, stdout))
}

fn b2sum(args: impl uucore::Args, streams: &mut Streams) -> UResult<()> {
    let calculate_blake2b_length =
        |s: &str| parse_blake_length(AlgoKind::Blake2b, BlakeLength::String(s));
    standalone_with_length_main(
        AlgoKind::Blake2b,
        uu_app(),
        args,
        calculate_blake2b_length,
        true,
        streams,
    )
}

#[inline]
pub fn uu_app() -> Command {
    standalone_checksum_app_with_length(translate!("b2sum-about"), translate!("b2sum-usage"))
        .name("b2sum")
}
