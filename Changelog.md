# Changelog

## Version 1.4.3

Improve the formatting of the --help message by
- enabling color
- enabling automatic wrapping of the descriptons

Add basic usage examples to the README, since some people seemed confused

## Version 1.4.2

Upgrade to the a2lfile crate version 1.3.3. This brings:

- The double quotes around filenames in /include are no longer mandatory; quotes are only required if the path contains spaces.
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

- add --cleanup wich cleans up unused or useless items in the file
   It removes empty groups and functions, as well as unused compu_methods, compu_tabs, record_layouts and units.
- add --target-group which allows new items created by --measurement[...] and --characteristic[...] to be directly added to a group
- minor formatting improvements

## Version 1.1.0

- rename --insert-characteristic to --characteristic and --insert-measurement to --measurement.
   The old nemaes remain as aliases, though they are not shown by --help
- add --measurement-range and --characteristic-range. Each of these takes a start address and
   an end address and inserts all variables found in this range into the a2l file.
- add --measurement-regex and --characteristic-regex. Each of these takes a regex pattern.
   Any variable matching the pattern will be inserted into the a2l file.
   Example: [...] --characteristic-regex "TuningData" [...] would insert TuningData1 and TuningData2, and also DefaultTuningData
   Example: [...] --measurement-regex "^TestVar\\.\_0\_.*" [...] would insert TestVar.\_0_.member, but not TestVar.\_1_.member
- Bugfix: the output path is no longer restricted to valid utf-8

## Version 1.0.1

- fix a bug where referring to array elements using angle brackets (array[0]) did not work corrctly
- fix a bug in --insert-characteristic and --insert-measurement where these could only reference variables, but not array elements or struct members
- allow creating new a2l files using the option --create

## Version 1.0.0

- initial stable release
