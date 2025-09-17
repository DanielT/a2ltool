# Usage of a2ltool

## Basic options

#### `--create`

Create a new A2L file instead of loading an existing one

#### `-o <A2LFILE>`, `--output <A2LFILE>`

Write to the given output file. If this flag is not present, no output will be written.

#### `-s`, `--strict`

Parse all input in strict mode. An error wil be reported if the file has any inconsistency.

#### `-v`, `--verbose`

Display additional information

#### `-e <ELFFILE>`, `--elffile <ELFFILE>`
  
Elf file containing symbols and address information in DWARF2+ format.
An exe file produced by MinGW with DWARF2 debug info can also be used.

#### `--pdbfile <PDBFILE>`

PDB file containig debugging information in Microsoft's Program Database format.
This is used for programs that are compiled by Visual Studio

#### `-h`, `--help`

Print the built in help message and exit.

#### `-V`, `--version`

Print the version and exit.


## Merge and Update

#### `-m <A2LFILE>`, `--merge <A2LFILE>`

Merge another a2l file on the MODULE level.
The input file and the merge file must each contain exactly one MODULE.
The contents will be merged so that there is one merged MODULE in the output.

#### `--merge-preference <PREF>`

Choose how to handle conflicts when merging MODULES. PREF can be one of:

- `EXISTING`: Keep the existing item
- `NEW`: Use the new item from the merge file.
- `BOTH`: keep both items, renaming the new item if necessary (default)

#### `-i`, `--merge-includes`

Merge the content of all included files. The output file will contain no /include commands.

#### `--merge-project <A2LFILE>`

Merge another a2l file on the PROJECT level.
If the input file contains m MODULES and the merge file contains n MODULES, then there will be m + n MODULEs in the output.

Warning: Unless you know exactly what you're doing, this option is not what you want.

#### `-u [<UPDATE_TYPE>]`, `--update [<UPDATE_TYPE>]`

Update the A2L file based on the elf file. The update type can omitted to perform a full update or it can be one of:

- `FULL`: Update the address and type info of all items. This is the default.
- `ADDRESSES`: Update only the addresses.

The arg --elffile must be present in order to perform an update.

#### ´--update-mode [<UPDATE_MODE>]`

Update the A2L file based on the elf file. Action can be one of:

- `DEFAULT`: Unknown objects are removed, invalid settings are updated.
- `STRICT`: Unknown objects or invalid settings trigger an error.
- `PRESERVE`: Unknown objects are preserved, with the address set to zero.

The arg --update must be present.


## Creating and Removing Items

#### `-C <VAR>`, `--characteristic <VAR>`

Insert a CHARACTERISTIC based on a variable in the elf file. The variable name can be complex, e.g. var.element[0].subelement

#### `--characteristic-regex <REGEX>`

Compare all symbol names in the elf file to the given regex. All matching ones will be inserted as CHARACTERISTICs

#### `--characteristic-range <ADDR> <ADDR>`

Insert multiple CHARACTERISTICs. All variables whose address is inside the given range will be inserted as CHARACTERISTICs.
This is useful in order to add all variables from a tuning data section with fixed addresses.

Example: `--characteristic-range 0x1000 0x2000`

#### `--characteristic-section <SECTION>`

Insert all variables from the given section as CHARACTERISTICs.

#### `-M <VAR>`, `--measurement <VAR>`

Insert a MEASUREMENT based on a variable in the elf file. The variable name can be complex, e.g. `var.element[0].subelement`

#### `--measurement-regex <REGEX>`

Compare all symbol names in the elf file to the given regex. All matching ones will be inserted as MEASUREMENTs

#### `--measurement-range <ADDR> <ADDR>`

Insert multiple MEASUREMENTs. All variables whose address is inside the given range will be inserted as MEASUREMENTs.
Example: `--measurement-range 0x1000 0x2000`

#### `--measurement-section <SECTION>`

Insert all variables from the given section as MEASUREMENTs.

#### `--target-group <GROUP>`

When inserting items, put them into the group named in this option. The group will be created if it doe not exist.

#### `--from-source <SOURCE_FILE>`

Create elements in the a2l file based on special comments in a source file. Argument can be a filename or pattern (e.g. *.c).

#### `-t`, `--enable-structures`

Enable the the use of INSTANCE, TYPEDEF_STRUCTURE & co. for all operations. Requires a2l version 1.7.1
The use of structurs is not supported in many other tools. You should check if the consumer of your a2l files can handle such data before using this option.

#### `--old-arrays`

Force the use of old array notation (e.g. .\_2\_) even when the a2l version allows the use of new array notation (e.g. [2]).

#### `-R <REGEX>`, `--remove <REGEX>`

Remove any CHARACTERISTICs, MEASUREMENTs, AXIS_PTS and INSTANCEs whose name matches the given regex.

#### `--remove-range <ADDR> <ADDR>`

Remove any CHARACTERISTICs, MEASUREMENTs, AXIS_PTS and INSTANCEs whose address is inside the given range.

#### `-c`, `--cleanup`

Remove empty or unreferenced items.

#### `--ifdata-cleanup`

Remove all IF_DATA blocks that cannot be parsed according to A2ML


## Other

#### `--check`

Perform additional consistency checks

#### `--sort`

Sort all the elements in the file first by item type and then alphabetically.
This gives the file a deterministic order.

#### `-a <A2L_VERSION>`, `--a2lversion <A2L_VERSION>`

Convert the input file to the given version (e.g. "1.5.1", "1.6.0", etc.).
It can upgrade or downgrade the version of an a2l file.
When downgrading this is a lossy operation, which deletes incompatible information.

Permitted versions are `1.5.0`, `1.5.1`, `1.6.0`, `1.6.1`, `1.7.0`, `1.7.1`

#### `--debug-print`

Display internal data for debugging / troubleshooting.

#### `--show-xcp`

Display the XCP settings in the a2l file, if they exist
