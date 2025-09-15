# Creating items from debug information

## Creating single named items

Use the command line options `--characteristic` or `--measurement` to create a
single MEASUREMENT or CHARACTERISTIC element in the A2L file for the named global variable or struct member.

The variable name can refer to a struct member or array element. For example, `myStruct.member[3].x` works, provided it is a valid global name in the ECU software.

Local variables (allocated on the stack) cannot be added to the A2L file.

#### Example

    a2ltool input.a2l --elffile sw.elf --measurement globalVar --output out.a2l

    a2ltool input.a2l --elffile sw.elf --characteristic myStruct.member[3].x --output out.a2l

## Creating multiple named items using regex

Use the command line options `--characteristic-regex` or `--measurement-regex` to insert multiple named items at once using regular expressions.
a2ltool matches each variable name against the provided regular expressions and creates a `MEASUREMENT` or `CHARACTERISTIC` for every match.

For example, the regular expression `some.*` matches `someFoo` and `someBar`, but not `bla_some`.

#### Example

    a2ltool input.a2l --elffile sw.elf --measurement-regex "thing\d+.member.[xy]" --output out.a2l

## Creating multiple items based on an address range

The command line options `--characteristic-range` or `--measurement-range` take a pair of addresses.
Any global variable located within this range is added to the A2L file.

#### Example

Assume that the ECU software has the following variables:

- `time` at address 0x1200
- `space` at address 0x1300

The following command adds both variables to the A2L file:

    a2ltool input.a2l --elffile sw.elf --characteristic-range 0x1000 0x2000 --output out.a2l

## Creating multiple items based on the section

Elements of the embedded software may be placed into distinct named sections.
The options `--characteristic-section` and `--measurement-section` look up the address range of each section in the ELF file header.
All items within the memory range of the specified section are then inserted into the A2L file.

## Group assignment

By default, items created from debug information are not assigned to any group. You can change this by using the `--target-group` option.
The specified group will be extended or created as needed.
