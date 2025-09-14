# Removing Elements

## Overview

You may need to remove elements from an A2L file to protect sensitive information when sharing files with clients, or to prevent unauthorized users from modifying critical settings.

a2ltool supports removing elements in several ways:

- Obsolete elements that are no longer present in the ECU software can be deleted during an update.  
  This is described in the [update chapter](update.md).
- Elements can be deleted by name or by matching a regular expression.
- Elements can be deleted based on their address.
- Supporting elements such as `COMPU_METHOD`s and `RECORD_LAYOUT`s can be automatically removed if they are unreferenced.

## Removing Elements by Name

The `--remove` option deletes any `CHARACTERISTIC`, `MEASUREMENT`, `AXIS_PTS`, `INSTANCE`, or `BLOB` that matches the specified name or regular expression.

You can pass this option multiple times in a single a2ltool command to remove several elements at once.

It is recommended to use `--cleanup` as well ([see below](#cleaning-up-unused-items)).

### Example

    a2ltool input.a2l --remove SecretValue --remove DangerousTuning.* --cleanup --output out.a2l

## Removing Elements by Address

The `--remove-range` option deletes any `CHARACTERISTIC`, `MEASUREMENT`, `AXIS_PTS`, `INSTANCE`, or `BLOB` located within the specified address range.

This is useful for removing entire categories of elements, as the address space of an embedded controller is typically divided into distinct regions. Hardware usually places RAM, ROM, and device registers in separate memory regions, and embedded software often further subdivides these regions.

It is recommended to use `--cleanup` as well ([see below](#cleaning-up-unused-items)).

### Example

Given a hardware and project-specific memory map like:

    ...
    0x10008000 - 0x10009FFF: safety-critical parameters
    0x1000A000 - 0x1000FFFF: non-critical parameters
    ...
    0x20000000 - 0x2000FFFF: RAM region for safety-critical tasks
    0x20010000 - 0x2001FFFF: RAM region for non-critical tasks
    ...

You could run a2ltool to remove all A2L elements containing safety-critical data at once:

    a2ltool input.a2l --remove-range 0x10008000 0x10009FFF --remove-range 0x20000000 0x2000FFFF --cleanup --output out.a2l

## Cleaning up Unused Items

An A2L file contains many "supporting" items: `COMPU_METHOD`, `COMPU_TAB`, `RECORD_LAYOUT`, `GROUP`, etc.  
These support the definition of main elements (`CHARACTERISTIC`, `MEASUREMENT`, `AXIS_PTS`, `INSTANCE`, and `BLOB`), but are not useful on their own.

When main elements are removed, some supporting items may become unused.

The `--cleanup` option checks each supporting item to determine if it is still referenced, and deletes those that are no longer needed.

### Example

    a2ltool input.a2l --remove ".*" --cleanup --output nearly_empty.a2l
