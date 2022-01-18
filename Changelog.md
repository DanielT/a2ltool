# Changelog
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
