# Checking A2L File Consistency

## Motivation

Syntactically valid a2l files can still contain incorrect or inconsistent content.

This can occur in several ways:

- Deleting an item that is referenced elsewhere in the file.
  For example, an axis may use a `MEASUREMENT` as an input. a2ltool can delete a referenced `MEASUREMENT` using the `--remove` command-line option.
- Adding items without including the elements they reference.
  With a2ltool, this may happen when using the `--from-source` option, as correctness depends on the input provided by the user.
- Other tools that generate a2l files may have bugs.
- The a2l file may be manually edited in a text editor, introducing errors.

## Checks

With the `--check` option, a2ltool performs the following checks:

- Verifies all cross-references between elements.
- Ensures that various elements have mandatory or forbidden sub-elements depending on their type.
  For example, an `AXIS_DESCR` of type `COM_AXIS` requires an `AXIS_PTS_REF`. Similarly, a `COMPU_METHOD` of type `TAB_VERB` must have a `COMPU_TAB_REF`.
- Checks that `CHARACTERISTIC`s have the correct number of `AXIS_DESCR`s based on their type.
- Validates that the lower and upper limits of elements are within the outer limits defined by their `COMPU_METHOD` and data type.
- Checks the group hierarchy: it must start with a group marked as `ROOT` and must not contain any cycles.

## Usage notes

By itself, the option `--check` only prints a list of warnings about incorrect a2l file elements.

When used in automation, such as a Makefile or CI job, it is recommended to also use `--strict`, which turns warnings into errors and sets a non-zero exit status to signal the error to the environment.

The check only considers information within the a2l file. To also compare the content to the ECU software, use `--update --update-mode STRICT`, which raises an error if there is a mismatch between the a2l file and the debug information.

## Examples

    a2ltool input.a2l --verbose --check --strict

    a2ltool input.a2l --elffile sw.elf --update --update-mode STRICT --check --strict
