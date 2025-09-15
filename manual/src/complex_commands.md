# Complex Command Lines

## Order of Operations

All options supported by a2ltool can be combined in a single command line.

When combining options, it is useful to understand the overall order of operations:

1. Load the input A2L or create a new file (`--create`)
2. Convert the file version (`--a2lversion`)
3. Merge additional A2L files (`--merge`)
4. Merge includes (`--merge-includes`)
5. Remove items (`--remove` and `--remove-range`)
6. Insert items generated from source comments (`--from-source`)
7. Update addresses and other settings (`--update`)
8. Insert items based on debug data (e.g., `--characteristic`, `--measurement`, etc.)
9. Clean-up (`--cleanup`)
10. If-data clean-up (`--ifdata-cleanup`)
11. Sort all elements (`--sort`)
12. Check consistency (`--check`)
13. Write the output file (`--output`)

## Response files

 a2ltool supports many options, several of which may be used multiple times. This can cause command lines to become very long, making them unmanageable or even exceeding system-imposed command line length limits.

To address this, a2ltool supports response files—files that contain command line options.
When a response file is included in the command line, a2ltool loads the file and reads additional options from it.

A response file must be prefixed with `@` on the command line.


#### Example

Example response file (`Response.rsp`):

    --elffile sw.elf
    --update ADDRESSES
    --update-mode PRESERVE
    --cleanup

Example command using a response file:

    a2lfile input.a2l @Response.rsp --output out.a2l
