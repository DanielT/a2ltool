# a2ltool
A tool to edit, merge and update a2l files

## Features of a2ltool
 - update the addresses of measurement variables and tunable parameters (characteristics) based on the elf file containing the compiled embedded application
 - merge multiple a2l files into a single file
 - add new measurements or characteristics based on the elf file
 - display XCP connection parameters embedded in the a2l file, if any exist
 - maintain the formatting and ordering of items in the a2l file during manipulation, so that the diff between the original and the updated/modified file is as small as possible
 - Supports files up to a2l version 1.71 (current)

## About a2l Files
A2l files describe measurement variables and tunable parameters of an embedded device (typically: an automotive ECU).
The consumer of the a2l file typically allows online calibraction over a protocol such as XCP and/or offline tuning by generating flashable parameter sets. There are several commercial tools for this purpose.
The a2l file format is specified by ASAM and is formally called ASAM MCD-2 MC.

## Project Status
All planned features are implemented; once the tool has had more use and testing it will be called version 1.0
