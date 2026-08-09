# Changelog

## Version 3.4.1

- Allow insertion of bitfield values using `--measurement`  / ` --characteristic` . It was already possible to insert them using regexes; this change restores parity.
- Fix: when creating items with ` --from-source`, `MAIN_GROUP` is now used as the default group for any items if there is no `GROUP` statement (@lennart24)
- Fix: ignore constexpr members in C++ classes/structs while loading debug info instead of pretending they're at offset 0
- Fix: C++ allows inheritance on structs too; the dwarf debug reader now handles this case
- Fix: correctly update `MATRIX_DIM` of a `CHARACTERISTIC` that represents a struct member selected by `FNC_VALUES`
- Fix: Verify `CHARACTERISTIC` bit masks in strict update mode
- Fix: 64-bit enums with negative values were not detected as signed

## Version 3.4.0

- a2ltool can now be installed as a Python wheel using `pip install a2ltool` (by zariiii9003)

**a2lfile**
- update to a2lfile 3.5.0
  - Don't crash when removing the last item from a list
  - Don't crash during check if CHARACTERISTIC has 6 or more AXIS_DESCR
  - Don't crash on a cycle of /include directives
  - Don't crash on a cycle of A2ML /include directives
  - Fix infinite loop when parsing empty TaggedStruct/TaggedUnion in IF_DATA
  - Reject signed hex numerical constants like "-0x20" in the tokenizer
  - Fix TYPEDEF_* and INSTANCE-related module merging bugs
  - Fix bad error messages for CrossReferenceError during check
  - Fix merge of FUNCTION.DEF_CHARACTERISTIC and FUNCTION.REF_CHARACTERISTIC, which were previously ignored
  - Fix: STATUS_STRING_REF target can be deleted by cleanup
  - Fix: off by one in CRLF detection at the end of A2ML blocks
  - merge-includes in IF_DATA now recurses into Array/Sequence

**Creator (`--from-source`)**
- Fix: the creator no longer crashes on recursive structure definitions; these are now reported as an error, since a recursive struct definition cannot occur in a real program
- Fix: a single comment can now contain multiple a2l creator commands
- Fix: incorrect handling of `VAR_CHARACTERISTIC` in the creator
- Fix: the creator no longer crashes when a `VAR_CRITERION` has no values
- Fix: the creator no longer crashes on malformed input
- Fix: the creator no longer creates duplicate item names
- Fix: curve/map items could not be changed using `overwrite`
- Fix: use the correct compu_tab for a compu_method if the target's name had to be changed to avoid a naming conflict
- Prevent arithmetic errors while creating an axis with a fixed range
- Improve handling of invalid split templates
- Fix: correct the position tracking in the comment scanner, which could cause incorrect error locations
- Prevent a `u16` overflow when creating a string object or flattening matrix dimensions during version downgrade; values are saturated instead
- The `ALIAS` of an `INSTANCE` definition is now also applied without `--enable-structures`

**Insert**
- Fix: newly inserted characteristics no longer use the raw input string as the item name; they are now cleaned up the same way as inserted measurements
- Complex C++ symbol names are now fully cleaned up when inserting items. Previously only `::` was removed, so characters like `<` and `>` could remain, producing invalid a2l identifiers
- Fix: `--old-arrays` was ignored when inserting new items
- Fix: create `REF_CHARACTERISTIC` instead of `REF_MEASUREMENT` for calibration-type `INSTANCE`s

**Update**
- Fix: the update result for `INSTANCE`s and `VAR_CHARACTERISTIC`s is now correctly counted as a strict-mode error
- Fix: reduce unnecessary rebuilding of types while updating `INSTANCE`s and `TYPEDEF`s
- Fix: possible record layout name conflicts when new record layouts are created during an update
- Fix a potential crash while updating `VAR_ADDRESS`
- Fix: the displayed update count for `VAR_CHARACTERISTIC` was always zero
- Fix: `IF_DATA CANAPE_EXT` entries were incorrectly zeroed out during an address-only update
- Fix: the size field in the `IF_DATA ASAP1B_CCP` block was incorrectly zeroed out during an address-only update
- Fix a potential crash while updating typedefs
- Fix: cleanup of unused typedefs could stop too early
- Fix a potential crash during typedef update for 64-bit bitfields
- Fix: typedef-nested arrays could get their inner dimension encoded twice
- Fix: `SymbolNotFound` errors were logged twice

**Remove**
- Fix: a2ltool no longer crashes if the regex given to `--remove` is invalid
- Fix: remove ranges are now upper-exclusive, as documented

**Debug info (DWARF/PDB)**
- Fix a crash when DWARF data contains an array of zero-sized elements
- Fix: PDB multi-dimensional arrays had the wrong strides
- Fix a crash on invalid PDB files; this is now reported as an error instead
- Fix: PDB type info for global variables could be overwritten by type info of a static variable with the same name
- Fix: derived class member types could be overwritten with base class info
- Handle array dimensions more carefully: incomplete debug info could produce arrays without a dimension, and dimensions larger than `u16` were not handled correctly

**General**
- Fix regex matching for patterns containing `|` (alternatives), used by `--insert` and `--remove` by name. Unanchored patterns without `^`/`$` were previously extended incorrectly, so `a|b` became `^a|b$`; they are now wrapped in a group, e.g. `^(?:a|b)$`
- Fix: allow `--target-group` to be combined with both `--from-source` and `--characteristic-section`, as intended
- Fix the mangled-symbol fallback path when the original Symbol Link contains extra info such as `{Namespace:Global}`
- Don't fail to set the version if the file has no existing `ASAP2_VERSION`
- Fix: upper limits of signed bitfields were off by one
- Fix limit calculation for linear conversions with negative factors
- Prevent division by zero when recalculating `RAT_FUNC` limits

## Version 3.3.0

- Allow the INSTANCE definition in source comments to contain multiple groups and an alias.
- Fix: Take the symbol offset into account when using CANAPE_EXT/LINK_MAP to identify a symbol
- Fix: specifiying an invalid file name with `--from-source` is an error. When specifiying multiple files or patterns at least one file must be valid and readable.
- Fix: Correct some errors in the documented grammar of the comment syntax used by `--from-source`

## Version 3.2.2

- Fix: Comments at the top level of a2l files are preserved during all operations

## Version 3.2.1

- Fix: repeatedly resolve type references until an actual type is found. This fixed nested type references in PDB debug info (by Louis Caron)
- Fix new clippy warnings
- update to a2lfile 3.3.2
  - Fix: Allow fallback parser to skip comments in unknown IF_DATA blocks (Akhil T Thomas)
  - Fix: Non-strict parsing should not fail if a string was expected, but comment + identifier was found. Previously the comment prevented the error recovery, where an identifier is accepted in place of a string. (Louis Caron)

## Version 3.2.0

- Fix: creating new TYPEDEF_CHARACTERISTICs from the command line was broken
- Fix: Handle of the offset value in SYMBOL_LINK correctly.
- DWARF type reader: get names from DebugLineStrRef too
- estimate sizes for structures created using `--from-source`

## Version 3.1.0

- Add the `--old-arrays` option. It allows the use of old array notation (e.g. ".\_3\_") to be forced,
  even if the file version is 1.7.0 or 1.7.1 where new array notation (e.g. [2]) is possible.
- Add the `--insert-a2ml` option. When this option is set, a2ltool's built-in A2ML defintion is inserted into the a2l file if it doesn't contain an one already.
- Bugfix: In some cases, depending on the debug info generated by the compiler, integer types were treated as bitfields
- Bugfix: The upper limit of bitfield values is no longer off by one
- Bugfix: prevent infinite recursion in the DWARF debug data reader if the element type of an array is the array itself
- Workaround: Some compilers claim that an enum is unsigned even if it contains enumerators with negative values. In this case the datatype is switched to a signed type.
- ignore case in parameters of command line options. For example `--update ADDRESSES` can also be written as `--update addresses` now.
- Correctly support the combination of `--enable-structures` with `--from-source`
- Stop with an error message if there are problems while processing definitions in source files

## Version 3.0.0

- Implement compatibility with Vector's ASAP2 Creator and allow creation of a2l elements using the same special comment syntax
- Implement additional modes for file merges.
  The standard so far was to keep both conflicting elements. Now the `--merge-preference` option can be used to always use the existing version or the new version instead.
- Add a manual for a2ltool.
- Support non-standard `SYMBOL_LINK` notation, where the compile unit name is used as a prefix of the real symbol name
- Implement the ability to update `VAR_ADDRESS` info of `VAR_CHARACTERISTIC`
- Handle negative bit-offsets in DWARF2 debug info correctly instead of panicing
- Increase the size of the containing type for bitfields that extend past the end of their nominal containing type.
- Add more plausibility checks to `--check`

## Version 2.7.0

- support C++ references in the DWARF debug data reader
- update to a2lfile 3.0.0
  - Comments are now preserved in some cases. Specifically, any comment that occurs in a position where a block is permitted
    can be can be represented in the data, and is written to the output file. Sorting a2l files erases comments.
  - The values of BIT_MASK and error mask are u64 instead of u32, and parsing no longer fails for values that don't fit in u32.
  - Allow multiple CALIBRATION_HANDLEs inside of CALIBRATION_METHOD (by Rainer Zaiser)

## Version 2.6.0

- handle offset values other than zero in SYMBOL_LINK
  Offset values are still restricted to only point to the beginning of values, and cannot refer to e.g. the second byte of a 32-bit value.
- update to a2lfile 2.5.0, which brings some bug fixes:
  - improvements for the merge function:
    - TYPEDEF_CHARACTERISTIC may reference a MEASUREMENT, but this reference was not updated during merges
    - USER_RIGHTS can no longer get duplicate user entries as a result of the merge
    - all items are merged in the order in which they appear in the merge file. Previously they were merged in reverse order
  - During checking, accept references from an AXIS_PRS_REF or CURVE_AXIS_REF of a TYPEDEF_CHARACTERISTIC
    to a structure component of the containing structure using the notation THIS.component_name.

## Version 2.5.0

- Support program information in PDB format
  The new option `--pdbfile` can be used instead of `--elffile` in order to read address and data type information from a PDB file. This supports the "virtual ECU" use case, where some parts of a microcontroller application are built on Windows and run inside a simulation environment.
- Improve the limit adjustment logic
  If an object with a 64-bit float data type had a compu-method, the limit calculation could produce +-INF.
  This has been prevented.
- Update to a2lfile 2.4.0, which brings the following improvements:
  - The handling of /include normlizes the path separators, so that Windows paths work on Linux (by @AkhilTThomas)
  - a2l checker improvements:
    - The RECORD_LAYOUT used by a CHARACTERISTIC must contain FNC_VALUES
    - A RECORD_LAYOUT must contain an AXIS_PTS_X/Y/Z for each internal axis
    - There should be a specific number of AXIS_DESCR depending on the type of RECORD_LAYOUT
    - The lower and upper limits of CHARACTERISTICs, MEASUREMENTs and AXIS_PTS
      are checked for plausibility based on the data type of the item and its COMPU_METHOD.
- Bugfix: in some cases IF_DATA of newly merged files was not updated
- Bugfix: For both DWARF and PDB input the signedness of enums is now read and used.
  Previously all enums were treated as unsigned.
- Expanded and improved unit tests
- a new relase workflow automatically builds binaries for Windows and Linux

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
