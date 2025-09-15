# Merging A2L Files

## Motivation

An embedded software project often generates multiple separate A2L files for different parts of the overall system.
For example, there might be hand-crafted files as well as files generated for individual components by calibration data management tools, AUTOSAR RTE generators, etc.

All these files must be merged into a single file so that measurement and calibration tools can utilize the information.

## Basic Example

You can use a2ltool as follows:

    a2ltool main.a2l --merge first.a2l --merge second.a2l --output out.a2l

The `--merge` option can be repeated as many times as needed to merge multiple input files in a single command.

## Merge Modes

Ideally, the information in your input files should be completely disjoint. In other words, no two files should define an object with the same name.

For example, if one file defines a measurement variable `speed` as an integer, but another file defines `speed` as a floating point value, then it is impossible to know which definition is correct.
By default, the merge keeps both definitions, renaming the second one to `speed.MERGE`. This allows you to select the correct variant in the measurement tool.

You can change this behavior by setting a merge preference.


a2ltool supports three different ways to handle merges:

- `--merge-preference EXISTING`: Prefer existing items, discarding any items from the merge that have conflicting names.
- `--merge-preference NEW`: Prefer new items, overwriting any existing items with conflicting names.
- `--merge-preference BOTH`: Keep both, renaming conflicts as described above. This is the default if no merge preference is set.

The merge preference applies to all merge operations in the same a2ltool invocation.
Call a2ltool multiple times if you want to use different preferences for different files.


### Example

    a2ltool main.a2l --merge other.a2l --merge-preference NEW --output out.a2l

## Merging includes


A2L files can include other A2L files using the directive `/include "file.a2l"`.
This form is not widely supported by measurement and calibration tools, so included files are usually merged into a single file.

a2ltool can perform this merging step using the `--merge-includes` command-line option.

### Example

    a2ltool file_with_includes.a2l --merge-includes --output file_without_includes.a2l
