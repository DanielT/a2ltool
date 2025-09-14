## Creating items from debug information

### Creating single named items

Use the command line options `--characteristic` or `--measurement` to create a
single MEASUREMENT or CHARACTERISTIC element in the a2l file for the named global variable or struct member.

The variable name can be a struct member or array element, so `myStruct.member[3].x` would work, provided this is a valid global name in the ECU software.

Local variables (allocated on the stack) cannot be added.

#### Example

    a2ltool input.a2l --elffile sw.elf --measurement globalVar --output out.a2l

    a2ltool input.a2l --elffile sw.elf --characteristic myStruct.member[3].x --output out.a2l

### Creating multiple named items using regex

Use the command line options `--characteristic-regex` or `--measurement-regex` to insert multiple named items at once using regular expressions.
a2ltool matches every variable name against the provided regexes and creates `MEASUREMENT`s or `CHARACTERISTIC`s for every match.

For example, the regex `some.*` would match `someFoo` and `someBar`, but not `bla_some`.

#### Example

    a2ltool input.a2l --elffile sw.elf --measurement-regex "thing\d+.member.[xy]" --output out.a2l

### Creating multiple items based on an address range

The command line options `--characteristic-range` or `--measurement-range` take a pair of addresses.
Any global variable located in this range is added to the a2l file.

#### Example

Assume that the ECU software has the following variables:

- `time` at address 0x1200
- `space` at address 0x1300

The following command makes both of them available in the a2l file:

    a2ltool input.a2l --elffile sw.elf --characteristic-range 0x1000 0x2000 --output out.a2l

### Creating multiple items based on the section

Elements of the embedded software can be placed into distinct named sections.
The options `--characteristic-section` and `--measurement-section` can look up the address range of each section in the elf file header.
After that all items in the memory range of the named section are inserted in the a2l file.

### Group assignment

By default, items created from debug info are not assigned to any group. This can be changed by setting the `--target-group` option.
The named group will be extended or created as required.
