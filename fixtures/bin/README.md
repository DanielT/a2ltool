# Binary fixtures for a2ltool tests

## debugdata

The source code for the various debugdata_* files is located inside the archive debugdata_src.zip.
This code is not intended to perform any sort of useful function, but only to produce interesting debug information when it is compiled.

### debugdata_gcc*.elf

Compiled with gcc 13.3 (as distributed by ARM) for bare-metal ARM; basic compile command:

`arm-none-eabi-gcc -mcpu=cortex-m7 -mthumb "-specs=nano.specs" "-specs=nosys.specs" -mfloat-abi=hard -nostdlib -g3 ...`

For the _dw3 variant, the command line option `-gdwarf3` is addded, to force gcc to generate DWARF3 output.

### debugdata_clang*.elf

Compiled with clang 19 for bare-metal ARM; basic compile command:

`clang --target=armv7m-none-eabi -mcpu=cortex-m7 -mfloat-abi=hard -nostdlib -g3 ...`

The _dw4 variant uses the additional command line option `-gdwarf-4` to force clang to generate DWARF4 instead of DWARF5.

### _dwz files

These files were run through DWZ (<https://sourceware.org/dwz/>).
The DWARF format is extremely inefficient and often contains many repetitions of essentially the same data.
DWZ aims to improve this, but it's not clear DWZ is doing much for these test files, since they are probably not complicated enough.
In any case, testing with these files has not revealed any new edge cases so far, so these files might be removed in the future.

### debugdata_cl.pdb

The debugdata example source was compiled with MSVC 2022. Compile command:

`cl ... /Qspectre- /Zi -o debugdata_cl.exe`

Only the resulting PDB file was added to git, since the exe file is not relevant for testing.

### debugdata_clang.pdb

This file was create by clang 19, using the clang-cl command:

`clang-cl.exe ... /Zi -o debugdata_clang.exe`

The exe file was discarded, since only the pdb is used for testing.

### debugdata_gcc.exe

This file was compiled with gcc 14.2 for windows created by the MSYS2 project.
MINGW / MSYS2 gcc produces .exe files that contain DWARF debug information.

Compile command:

`gcc -g3 -o debugdata_gcc.exe ...`

## update_test

The update_test files were built from the single source file update_test.c; they are used for various tests of the a2l update code.

update_test.elf
update_test.exe

## update_typedef_test

This is used for the test cases that are specific to the code creating and updating TYPEDEF_STRUCTUREs and INSTANCEs

## software_a.elf and software_b.elf

These are obfuscated embedded applications.
All code and data sections have been removed from each of these files, so they are no longer executable.
They only contain debug section, in which all strings have been randomized.

These files are used by the release pipeline to perform a PGO build.
