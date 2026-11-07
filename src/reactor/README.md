# WASI reactor commands

Commands `execveatinit`/`execveatwait` run (see `execveat.rs`), out of the utilities built for wasm
(`feat_wasm`). A command not listed in `process.rs` returns `ErrNotFound`.

Supported commands run through their `uumain_with` once stdin is exhausted, except those that stream,
which write their output as each chunk of stdin arrives.

| command   | supported    | reason |
|-----------|--------------|--------|
| arch      | yes          |        |
| b2sum     | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| base32    | yes          |        |
| base64    | yes          |        |
| basename  | yes          |        |
| basenc    | yes          |        |
| cat       | yes          |        |
| cksum     | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| comm      | yes          |        |
| cp        | yes          | prompts read their answers from stdin, which must already hold them; the prompt itself goes to the reactor's stderr |
| csplit    | yes          |        |
| cut       | no           | fast paths need a real file descriptor (splice, `AsFd`, terminal checks) |
| date      | yes          |        |
| dd        | no           | operates on file descriptors directly |
| dir       | no           | layout and colour depend on the terminal |
| dircolors | yes          |        |
| dirname   | yes          |        |
| echo      | yes          |        |
| expand    | yes          |        |
| expr      | yes          |        |
| factor    | yes          |        |
| false     | no           | shell builtin |
| fmt       | yes          |        |
| fold      | yes          |        |
| head      | yes          |        |
| hostid    | yes          |        |
| join      | yes          |        |
| link      | yes          |        |
| ln        | no           | not ported yet |
| ls        | no           | layout and colour depend on the terminal |
| md5sum    | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| mkdir     | yes          |        |
| mktemp    | yes          |        |
| mv        | yes          | prompts read their answers from stdin, which must already hold them; the prompt itself goes to the reactor's stderr |
| nice      | no           | runs another program |
| nl        | yes          |        |
| nproc     | yes          |        |
| numfmt    | no           | fast paths need a real file descriptor (splice, `AsFd`, terminal checks) |
| od        | no           | fast paths need a real file descriptor (splice, `AsFd`, terminal checks) |
| paste     | yes          |        |
| pathchk   | no           | not ported yet |
| pr        | yes          |        |
| printenv  | yes          |        |
| printf    | yes          |        |
| ptx       | yes          |        |
| pwd       | yes          |        |
| readlink  | yes          |        |
| realpath  | yes          |        |
| rm        | no           | interactive prompts read stdin |
| rmdir     | yes          |        |
| seq       | yes          |        |
| sha1sum   | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| sha224sum | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| sha256sum | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| sha384sum | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| sha512sum | no           | not ported yet, stdin/stdout are used deep in uucore's checksum module |
| shred     | no           | operates on file descriptors directly |
| shuf      | yes          |        |
| sleep     | yes          |        |
| sort      | no           | `--compress-program` runs another program |
| split     | no           | `--filter` runs another program |
| sum       | yes          |        |
| tac       | yes          |        |
| tail      | yes          | `-f` and `-F` fail: a process can not wait for more input |
| tee       | yes, streams |        |
| test      | no           | shell builtin; `-t` needs `isatty` |
| touch     | no           | not ported yet |
| tr        | yes          |        |
| true      | no           | shell builtin |
| truncate  | yes          |        |
| tsort     | yes          |        |
| tty       | no           | needs a terminal |
| uname     | yes          |        |
| unexpand  | yes          |        |
| uniq      | yes          |        |
| unlink    | yes          |        |
| vdir      | no           | layout and colour depend on the terminal |
| wc        | yes          |        |
| yes       | no           | fast paths need a real file descriptor (splice, `AsFd`, terminal checks) |
