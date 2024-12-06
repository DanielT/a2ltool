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

## Usage examples

### Merge two a2l files

`a2ltool file1.a2l --merge file2.a2l --output merged.a2l`

### Merge multiple a2l files

`a2ltool file1.a2l --merge file2.a2l --merge file3.a2l --merge file4.a2l --output merged.a2l`

### Merge all included files into the main file

`a2ltool file1.a2l --merge-includes --output flat.a2l`

### Update the addresses and other data in an a2l file

`a2ltool input.a2l --elffile input.elf --update --output updated.a2l`

### Update the addresses and other data in an a2l file, while keeping invalid elements

`a2ltool input.a2l --elffile input.elf --update --update-mode PRESERVE --output updated.a2l`

### Update only the addresses in an a2l file, and exit with an error if any other a2l elements are incorrect

`a2ltool input.a2l --elffile input.elf --update ADDRESSES --update-mode STRICT --output updated.a2l`

### Create a new a2l file and add a characteristic from an elf file to it

`a2ltool --create --elffile input.elf --characteristic my_var --output newfile.a2l`

### Create a new a2l file and add multiple measurements from an elf file to it using a regular expression

`a2ltool --create --elffile input.elf --measurement-regex ".*name_pattern\d\d+*" --output newfile.a2l`

### Create a new a2l file and add multiple measurements from an elf file to it using an address range

`a2ltool --create --elffile input.elf --measurement-range 0x1000 0x3000 --output newfile.a2l`

### Change the version of an a2l file, while deleting any incompatible elements

`a2ltool input.a2l --a2lversion 1.5.1 --output downgraded.a2l`

### Check an a2lfile for consistency

`a2ltool input.a2l --check --strict`

### Use response files containing command arguments

Assume that the file `a2ltool.rsp` exists and contains valid arguments for `a2ltool`.

`a2ltool @a2ltool.rsp`

## About a2l Files

A2l files describe measurement variables and tunable parameters of an embedded device (typically: an automotive ECU).

The consumer of the a2l file typically allows online calibraction over a protocol such as XCP and/or offline tuning by generating flashable parameter sets. There are several commercial tools for this purpose.

The a2l file format is specified by ASAM and is formally called ASAM MCD-2 MC.
