# a2ltool User Manual

a2ltool is a command line utility that helps you work with a2l files.

A2l files describe measurement variables and tunable parameters of an embedded device (typically: an automotive ECU).

## Features

You can:

- create a2l files from scatch
- merge multiple a2l files
- update the information in an a2l file based debug info of the software
- delete existing elements
- check the consistency of a file

## Installation

a2ltool binaries are available on the [github releases page](https://github.com/DanielT/a2ltool/releases).
You can get pre-built binaries for Windows (x64) and Linux (x64) there.

For any other platforms you can clone the git repository and compile it as ususal, using `cargo build --release`.

## Quick start

Every call of a2ltool needs either an input file name, or the parameter `--create` to start with an empty file.
Any time you want to write some output to a file you need the parameter `--output`.
As a result, a minimal call to a2ltool that doesn't do anything by itself looks like this:

    a2ltool input.a2l --output output.a2l

or

    a2ltool --create --output output.a2l

You can add all other parameters you might need to this basic call.

## Note

a2ltool is silent by default. Only errors are printed to your console.

While you are getting started with a2ltool you will find it useful to set the `--verbose` (or `-v`) option to enable additional output.

## Basic examples

### Merging

    a2ltool input.a2l --merge other.a2l --output output.a2l

### Updating

    a2ltool input.a2l --elffile ecu_software.elf --update -v --output output.a2l

## Support

If you encounter any bugs, you can report the bug in the [issue tracker](https://github.com/DanielT/a2ltool/issues) on Github.
