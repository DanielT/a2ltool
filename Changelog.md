# Changelog

## Version 2.4.0

- Document and validate the ability to use exe/DWARF2+ files, which are produced by MinGW gcc.
  In practice support for these files has always existed because the `object` crate transparently supports getting sections from exe files.
- Allow greater control over the a2l update process
  - The option `--update` can now take an argument `FULL` or `ADDRESSES` (if the argument is not provided it defaults to `FULL`)
    Mode `FULL` updates addresses, datatypes, upper and lower limits, etc. It is a comprehensive update of all information in the a2l file.
    The mode `ADDRESSES` only updates the addresses, leaving everything else untouched.
  - The option `--upate-preserve` has been removed. Instead there is not `--update-mode`, which works in combination with `--update`
    Update mode `STRICT` verifies all information in the al2 file. If anything other than the addresses would be modified, then a2lfile reports an error.
    Update mode `PRESERVE` PRESERVE behaves like --update-preserve, and keeps unknown objects.
    Update mode `DEFAULT` (or omitting the option entirely) allows incorrect information, and discards invalid blocks.
- Upgrade to a2lfile 2.3.2
  - Bugfix: Escape codes \r, \n and \t in quoted strings were not handled correctly while writing files
- Bugfix: support the case where a compiler omits type information after the volatile tag for "volatile void*"

## Version 2.3.0

- Bugfix: Handle the case of incomplete DWARF type info
  C allows incomplete type declarations. For example "struct foo;" without a definition only tells the compiler that his type exists, and results in a stub type entry with the attribute DW_AT_declaration in the DWARF data.
- upgrade to a2lfile 2.3.1
  - change the behavior of merge for GROUPS and FUNCTION
    Previously, if two GROUPS in a merge had the same name, then the incoming group was renamed with the suffix ".MERGE".
    Now the content for GROUPs/FUNCTIONs with the same name is combined.
  - a2l checker improvements: fix several bugs, and handle some additional cases
- In strict mode, any problems detected by `--check` now cause a2ltool to exit with an error code.
  Outside of strict mode, `--check` remains informational.

## Version 2.2.0

- upgrade to a2lfile 2.2.0
  - Bugfix: during merges, some items might be renamed to prevent name collisions, but not all references to renamed items were updated inside of TYPEDEF_*
  - Successfully parse non-standard a2l files which contain hex-encoded integers where the parser expects a float value. (by @louiscaron)
- Add the ability to remove items by regex. This will remove any CHARACTERISTIC, MEASUREMENT or INSTANCE whose name matches.
  Removal happens before insertion, if both are done in the same run.
- Change the behavior of insertion by regex: the regex must match the whole variable name, instead of any substring inside it.
  This makes insertion more precise, but some regexes that worked previously will need to be extended with `.*`

## Version 2.1.1

- Bugfix: Don't fail to read DWARF type info if it contains any of the attributes packed, atomic, restrict, or immutable.
- During update, set ECU_ADDRESSes that were "0" to hexadecimal display mode

## Version 2.1.0

- Enable the use of response files on the command line, using an `@filename` argument
- display XCPplus parameters in `--show-xcp`

## Version 2.0.2

- update to a2lfile 2.1.0
  - add handling for `/include` inside A2ML (by @louiscaron)
  - fix multi-level `/include` inside A2L

## Version 2.0.1

- Fix issue #30: don't remove the BIT_MASK from elements during update (by @louiscaron)
- Fix issue #32: the COMPU_METHOD must be taken into account while updating the data limits
- Fix: Performance regression from version 2.0.0

## Version 2.0.0

- upgrade to a2lfile 2.0.0
  - fix the definition of AR_COMPONENT
  - don't remove valid elements during cleanup
- Create and update INSTANCEs and TYPEDEF_MEASUREMENTs if the file version is 1.7.1 and `--enable-structures` is set
- Insert whole arrays of MEASUREMENTs and CHARACTERISTICs instead of separate items for each element if the array elements have a simple datatype
- Items can now be inserted based on the containing elf section
- Debug data reader improvements - extracted information should now be better and more complete
  With assistance and fixes by @oleid - Thanks!
- Support XCP IF_DATA up to version 1.4 (previously only version 1.2 was supported)
- Use new array notation if the file version is 1.7.0 or newer - "[x]" instead of ".\_x\_"
- Fix the BIT_MASK attribute for big-endian targets
- Remove a stray debug print that caused message spam while inserting CHARACTERISTICs

## Version 1.6.0

- Upgrade to a2lfile version 1.5.0
  - Ensure that the components of a RECORD_LAYOUT are written in the correct order
  - fix the definitions if the OVERWRITE and REF_MEMORY_SEGMENT elements
  - be more strict about a2l versions, and reject unknown ones
  - improved error handling for invalid identifiers
- Bugfix: handle inherited members of C++ classes correctly
- Correctly read array information from the DWARF debug data even if it does not have a size attribute
  Contributed by @oleid
- reduce clap and regex versions in order to be compatible with rustc 1.63 on Debian stable
  Contributed by @oleid
- add an option to change the a2l file version. This option deletes any elements that are unsupported in the target version.

## Version 1.5.0

Upgrade to the a2lfile crate version 1.4.0
Allow a2ltool to load and merge a2l fragments. An a2l fragment is a file that contains only the content of a MODULE, but none of the surrounding elements.
Upgrade all dependencies; one of these (rustix, an indirect dependency) had a vulnerability that is fixed in the latest version.

## Version 1.4.4

Upgrade to the a2lfile crate version 1.3.4, to get a fix in the a2l parser.
Previous versions were unable to handle some strings with double "" escapes, e.g. "some ""text"" here"

## Version 1.4.3

Improve the formatting of the `--help` message by

- enabling color
- enabling automatic wrapping of the descriptons

Add basic usage examples to the README, since some people seemed confused

## Version 1.4.2

Upgrade to the a2lfile crate version 1.3.3. This brings:

- The double quotes around filenames in `/include` are no longer mandatory; quotes are only required if the path contains spaces.
  Fixed by @jl-rbpt
- The a2ml parser had a bug that prevented the datatype uint64 from being recognized
- Handling of USER_RIGHTS block during merging is improved, so that duplicate blocks will not be created any more
- Extra spaces will no longer be added to A2ML blocks during writing

## Version 1.4.1

Version 1.4.1 contains one bug fix compared to 1.4.0:

- C++ Symbol demangling was incorrectly applied to both variable names and names of struct members.
  A trivial example is that "c" can be demangled to "const", so "somestruct.c" would be demangled to "somestruct.const" and then updating / inserting would fail.
  The handling of name demangling has been changed completely and should make much more sense now.

## Version 1.4.0

Changes since version 1.3:

- upgrade to a2lfile 1.3.2, which fixes a mistake in the parsing of REF_UNIT
- upgrade clap from 2.34 to 4.0. The layout and look of the --help text changes, but all functionality should remain unchanged
- bug fix for one case where it was possible to create duplicate measurements or characteristics

## Version 1.3.0

Upgrade to the a2lfile crate version 1.3.0. This brings:

- perfect support for all of a2l version 1.7.1
- a bug fix in the tokenizer. It didn't handle strings that end in \\" correctly and files that had such strings could not be loaded

## Version 1.2.0

- add `--cleanup` wich cleans up unused or useless items in the file
   It removes empty groups and functions, as well as unused compu_methods, compu_tabs, record_layouts and units.
- add `--target-group` which allows new items created by `--measurement[...]` and `--characteristic[...]` to be directly added to a group
- minor formatting improvements

## Version 1.1.0

- rename `--insert-characteristic` to `--characteristic` and `--insert-measurement` to `--measurement`.
   The old names remain as aliases, though they are not shown by `--help`.
- add `--measurement-range` and `--characteristic-range`. Each of these takes a start address and
   an end address and inserts all variables found in this range into the a2l file.
- add `--measurement-regex` and `--characteristic-regex`. Each of these takes a regex pattern.
   Any variable matching the pattern will be inserted into the a2l file.
   Example: `[...] --characteristic-regex "TuningData" [...]` would insert TuningData1 and TuningData2, and also DefaultTuningData
   Example: `[...] --measurement-regex "^TestVar\._0_.*" [...]` would insert TestVar.\_0_.member, but not TestVar.\_1_.member
- Bugfix: the output path is no longer restricted to valid utf-8

## Version 1.0.1

- fix a bug where referring to array elements using angle brackets (array[0]) did not work corrctly
- fix a bug in `--insert-characteristic` and `--insert-measurement` where these could only reference variables, but not array elements or struct members
- allow creating new a2l files using the option --create

## Version 1.0.0

- initial stable release
