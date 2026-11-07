# WASI reactor commands

Commands `execveatinit`/`execveatwait` run (see `execveat.rs`), out of the utilities built for wasm
(`feat_wasm`). A command not listed in `process.rs` returns `ErrNotFound`.

Supported commands run through their `uumain_with` once stdin is exhausted, except those that stream,
which write their output as each chunk of stdin arrives.

| command   | supported    | reason |
|-----------|--------------|--------|
| arch      | yes          |        |
| b2sum     | yes          |        |
| base32    | yes          |        |
| base64    | yes          |        |
| basename  | yes          |        |
| basenc    | yes          |        |
| cat       | yes          |        |
| cksum     | yes          |        |
| comm      | yes          |        |
| cp        | yes          | prompts read their answers from stdin, which must already hold them; the prompt itself goes to the reactor's stderr |
| csplit    | yes          |        |
| cut       | yes          |        |
| date      | yes          |        |
| dd        | yes          | its statistics go to the reactor's stderr |
| dir       | yes          | columns are 80 wide unless `-w` or `COLUMNS` says otherwise, `--color=auto`, `--classify=auto` and `--hyperlink=auto` see no terminal |
| dircolors | yes          |        |
| dirname   | yes          |        |
| echo      | yes          |        |
| expand    | yes          |        |
| expr      | yes          |        |
| factor    | yes          |        |
| false     | yes          |        |
| fmt       | yes          |        |
| fold      | yes          |        |
| head      | yes          |        |
| hostid    | yes          |        |
| join      | yes          |        |
| link      | yes          |        |
| ln        | yes          | `-i` reads its answers from stdin |
| ls        | yes          | one entry per line by default, `--color=auto`, `--classify=auto` and `--hyperlink=auto` see no terminal; `-C` and `-x` are 80 columns wide unless `-w` or `COLUMNS` says otherwise |
| md5sum    | yes          |        |
| mkdir     | yes          |        |
| mktemp    | yes          |        |
| mv        | yes          | prompts read their answers from stdin, which must already hold them; the prompt itself goes to the reactor's stderr |
| nice      | yes          | runs only a command the reactor runs through `uumain_with`, at niceness 0 |
| nl        | yes          |        |
| nproc     | yes          |        |
| numfmt    | yes          |        |
| od        | yes          |        |
| paste     | yes          |        |
| pathchk   | yes          |        |
| pr        | yes          |        |
| printenv  | yes          |        |
| printf    | yes          |        |
| ptx       | yes          |        |
| pwd       | yes          |        |
| readlink  | yes          |        |
| realpath  | yes          |        |
| rm        | yes          | prompts read their answers from stdin, which must already hold them; the prompt itself goes to the reactor's stderr |
| rmdir     | yes          |        |
| seq       | yes          |        |
| sha1sum   | yes          |        |
| sha224sum | yes          |        |
| sha256sum | yes          |        |
| sha384sum | yes          |        |
| sha512sum | yes          |        |
| shred     | yes          | `-` fails: the caller's stdout is a stream, not a file to overwrite |
| shuf      | yes          |        |
| sleep     | yes          |        |
| sort      | yes          | `--compress-program` fails: there are no other programs; `-c` and `-m` fail on wasi: they read on threads |
| split     | yes          | `--filter` fails: there are no other programs |
| sum       | yes          |        |
| tac       | yes          |        |
| tail      | yes          | `-f` and `-F` fail: a process can not wait for more input |
| tee       | yes, streams |        |
| test      | yes          | `-t` is false: a process has no terminal |
| touch     | yes          | `-` fails: stdout is the guest's socket |
| tr        | yes          |        |
| true      | yes          |        |
| truncate  | yes          |        |
| tsort     | yes          |        |
| tty       | yes          | not a tty: a process has no terminal |
| uname     | yes          |        |
| unexpand  | yes          |        |
| uniq      | yes          |        |
| unlink    | yes          |        |
| vdir      | yes          | `--color=auto`, `--classify=auto` and `--hyperlink=auto` see no terminal |
| wc        | yes          |        |
| yes       | no           | never exits: the reactor buffers a utility's whole output before writing it |
