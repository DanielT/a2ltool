# Merging a2l Files

## Motivation

An embedded software project often generates multiple separate a2l files for different parts of the overall system.
For example, there might be hand-crafted files as well as files generated for individual components by calibration data mangement tools, Autosar RTE generators, etc.

They must all be merged into a single file so that a measurement/calibration tool can make use of the information.

## Basic Example

With a2ltool you can do it like this:

    a2ltool main.a2l --merge first.a2l --merge second.a2l --output out.a2l

the option `--merge` can be repeated as often as necessary to merge multiple inputs in a single call.

## Merge Modes

Ideally, the information in your inputs should be fully disjoint. In other words, no two files should define an object with the same name.

For example, if one file defines a measurment variable `speed` as integar, but another file defines `speed` as a floating point value, then it impossible to know which definition is correct.
By default, the merge keeps both, renaming the second one to `speed.MERGE`. This allows the user to select the correct variant in the measurement tool.

You can modify this behavior by setting a merge preference.

a2ltool supports three different ways to handle merges:

- `--merge-preference EXISTING`: Prefer existing items, discarding any items from the merge that have conflicting names
- `--merge-preference NEW`: Prefer new items, overwriting any existing items have conflicting names
- `--merge-preference BOTH` Keep both, renaing conflicts as described above. This is the default if no merge preference is set.

The merge preference applies to all merge operations in the same a2ltool invocation.
a2ltool should be called multiple times to use different preferences with different files.

### Example

    a2ltool main.a2l --merge other.a2l --merge-preference NEW --output.a2l

## Merging includes

A2l files can include other a2l files using the directive `/include "file.a2l"`.
This form is not widely supported in measurement/calibration tools, and is usually merged into a single file.

a2ltool can handle this merging step with the `--merge-includes` command line option.

### Example

    a2ltool file_with_includes.a2l --merge-includes --output file_without_includes.a2l
