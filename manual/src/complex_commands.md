# Complex Command Lines

## Order of Operations

All options supported by a2ltool can be combined in a single command line.

When doing so, it is useful to know the overall order of operations:

1. Load the input A2L or create a new file (`--create`)
2. Convert the file version (`--a2lversion`)
3. Merge other A2L files (`--merge`)
4. Merge includes (`--merge-includes`)
5. Remove items (`--remove` and `--remove-range`)
6. Insert items created from source comments (`--from-source`)
7. Update the addresses and other settings (`--update`)
8. Insert items based on debug data (`--characteristic`, `--measurement`, etc.)
9. Clean-up (`--cleanup`)
10. If-data clean-up (`--ifdata-cleanup`)
11. Sort all elements (`--sort`)
12. Check consistency (`--check`)
13. Write the output file (`--output`)

## Response files

a2ltool support many options, many of which may be used multiple times. This allows command lines to grow very long, until they become unmanageable or even hit system-imposed command line length limits.

To help with this, a2ltool supports response files, which are files that contain command line options.
When a response file is part of the command line, a2ltool loads the file and reads further options from it.

A response file must have the prefix `@` on the command line.

#### Example

Response.rsp:

    --elffile sw.elf
    --update ADDRESSES
    --update-mode PRESERVE
    --cleanup

a2lfile command:

    a2lfile input.a2l @Response.rsp --output out.a2l
