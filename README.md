# a2ltool

[![Github Actions](https://github.com/DanielT/a2ltool/actions/workflows/CI.yml/badge.svg)](https://github.com/DanielT/a2ltool/actions)

A tool to edit, merge and update a2l files

## Features of a2ltool

- update the addresses of measurement variables and tunable parameters (characteristics) based on the elf file containing the compiled embedded application
- merge multiple a2l files into a single file
- add new measurements or characteristics based on the elf file
- check a2l files for consistency
- display XCP connection parameters embedded in the a2l file, if any exist
- maintain the formatting and ordering of items in the a2l file during manipulation, so that the diff between the original and the updated/modified file is as small as possible
- Supports files up to a2l version 1.71 (current)

## Installation

a2ltool binaries are available on the [releases page](https://github.com/DanielT/a2ltool/releases).
You can get pre-built binaries for Windows (x64) and Linux (x64) there.

For any other platform you can compile a2ltool using `cargo build --release`.

## Usage

Refer to [the manual](https://danielt.github.io/a2ltool/) for a detailed description of the features and options of a2ltool.

## Examples

The following examples show how to use a2ltool for common use cases:

#### Merge two A2L files

`a2ltool file1.a2l --merge file2.a2l --output merged.a2l`

#### Merge multiple A2L files

`a2ltool file1.a2l --merge file2.a2l --merge file3.a2l --merge file4.a2l --output merged.a2l`

#### Merge all included files into the main file

`a2ltool file1.a2l --merge-includes --output flat.a2l`

#### Update the addresses and other data in an A2L file

`a2ltool input.a2l --elffile input.elf --update --output updated.a2l`

#### Update the addresses and other data in an A2L file, while keeping invalid elements

`a2ltool input.a2l --elffile input.elf --update --update-mode PRESERVE --output updated.a2l`

#### Update only the addresses in an A2L file, and exit with an error if any other A2L elements are incorrect

`a2ltool input.a2l --elffile input.elf --update ADDRESSES --update-mode STRICT --output updated.a2l`

#### Create a new A2L file and add a characteristic from an ELF file to it

`a2ltool --create --elffile input.elf --characteristic my_var --output newfile.a2l`

#### Create a new A2L file and add multiple measurements from an ELF file to it using a regular expression

`a2ltool --create --elffile input.elf --measurement-regex ".*name_pattern\d\d+*" --output newfile.a2l`

#### Create a new A2L file and add multiple measurements from an ELF file to it using an address range

`a2ltool --create --elffile input.elf --measurement-range 0x1000 0x3000 --output newfile.a2l`

### Change the version of an A2L file, while deleting any incompatible elements

`a2ltool input.a2l --a2lversion 1.5.1 --output downgraded.a2l`

#### Check an A2L file for consistency

`a2ltool input.a2l --check --strict`

#### Use response files containing command arguments

Assume that the file `a2ltool.rsp` exists and contains valid arguments for `a2ltool`.

`a2ltool @a2ltool.rsp`

## About A2L Files

A2L files describe measurement variables and tunable parameters of an embedded device (typically, an automotive ECU).

The consumer of the A2L file typically allows online calibration over a protocol such as XCP and/or offline tuning by generating flashable parameter sets. Several commercial tools are available for this purpose.

The A2L file format is specified by ASAM and is formally called ASAM MCD-2 MC.

## License

a2ltool is dual-licensed under the [MIT](LICENSE-MIT) and [Apache2](LICENSE-APACHE) licenses.
