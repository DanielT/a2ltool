# a2ltool

A tool to edit, merge and update a2l files

## Features of a2ltool

- update the addresses of measurement variables and tunable parameters (characteristics) based on the elf file containing the compiled embedded application
- merge multiple a2l files into a single file
- add new measurements or characteristics based on the elf file
- display XCP connection parameters embedded in the a2l file, if any exist
- maintain the formatting and ordering of items in the a2l file during manipulation, so that the diff between the original and the updated/modified file is as small as possible
- Supports files up to a2l version 1.71 (current)

## Usage examples

### Merge two a2l files

`a2ltool file1.a2l --merge file2.a2l --output merged.a2l`

### Update the addresses in an a2l file

`a2ltool input.a2l --elffile input.elf --update --output updated.a2l`

### Create a new a2lfile and add a characteristic from an elf file to it

`a2ltool --create --ellfile input.elf --characteristic my_var --output newfile.a2l`

## About a2l Files

A2l files describe measurement variables and tunable parameters of an embedded device (typically: an automotive ECU).

The consumer of the a2l file typically allows online calibraction over a protocol such as XCP and/or offline tuning by generating flashable parameter sets. There are several commercial tools for this purpose.

The a2l file format is specified by ASAM and is formally called ASAM MCD-2 MC.

## Project Status

With a2ltool version 1.0 all initially planned features were fully implemented.
Further improvements have been made as needed since then - see the changelog.
