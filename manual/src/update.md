
# Updating Files

## Motivation

Programs that load A2L files depend on the memory addresses specified within those files to correctly read and write data. In most workflows, these addresses are initially set to zero.

To resolve this, the update process must locate each item defined in the A2L file within the debug data of the compiled software, then update the A2L file with the correct address.

By default, `a2ltool` removes obsolete elements and updates all other information that can be derived from debug data, including:
- Data type
- Array dimensions
- Upper and lower limits
- Bit masks for bit fields
- Enumerators for types declared as enums

This enables `a2ltool` to update existing A2L files that were originally created for different versions of ECU software.

### Basic Example

    a2lfile input.a2l --elffile ecu_software.elf --update --output updated.a2l

## Update Modes

`a2lfile` can update all settings in an A2L file or restrict updates to addresses only. The operation is controlled by the `--update` parameter:

- `--update` or `--update FULL`: Update addresses and type information for all items.
- `--update ADDRESSES`: Update only the addresses.

You can also specify how to handle invalid or unknown items using the `--update-mode` parameter:

- `--update-mode DEFAULT`: Default behavior: unknown objects are removed and invalid settings are updated.
- `--update-mode STRICT`: Unknown objects or invalid settings will cause an error.
- `--update-mode PRESERVE`: Unknown objects are preserved, but their addresses are set to zero.

## Debug Data Formats

Most embedded development toolchains produce ELF files containing debug data in the DWARF2 (or 3/4/5) format.

In some cases, parts of an embedded program may also be compiled using PC compilers in order to target simulation or "virtual ECU" environments. These builds may produce EXE or DLL files with debug information in the PDB format.

To support these scenarios, `a2lfile` includes parsers for both DWARF2 and PDB formats.

Use the `--elffile` option to load a file containing DWARF2 debug info, or use `--pdbfile` to load debug information in the PDB format.

An interesting special case is compilation on Windows using MinGW, which produces EXE files with DWARF2 debug info.
These files can also be loaded using the `--elffile` option.

One of these options must be provided to perform any update.

## Examples

Update all information and preserve unknown items:

    a2lfile input.a2l --elffile ecu_software.elf --update --update-mode PRESERVE --output updated.a2l

Update addresses only, and abort if the file contains invalid items:

    a2lfile input.a2l --elffile ecu_software.elf --update ADDRESSES --update-mode STRICT --output updated.a2l
