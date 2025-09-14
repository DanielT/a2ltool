# Additional Examples

## Merge

#### Merge two a2l files

`a2ltool file1.a2l --merge file2.a2l --output merged.a2l`

#### Merge multiple a2l files

`a2ltool file1.a2l --merge file2.a2l --merge file3.a2l --merge file4.a2l --output merged.a2l`

#### Merge all included files into the main file

`a2ltool file1.a2l --merge-includes --output flat.a2l`

## Update

#### Update the addresses and other data in an a2l file

`a2ltool input.a2l --elffile input.elf --update --output updated.a2l`

#### Update the addresses and other data in an a2l file, while keeping invalid elements

`a2ltool input.a2l --elffile input.elf --update --update-mode PRESERVE --output updated.a2l`

#### Update only the addresses in an a2l file, and exit with an error if any other a2l elements are incorrect

`a2ltool input.a2l --elffile input.elf --update ADDRESSES --update-mode STRICT --output updated.a2l`

## Create

#### Create a new a2l file and add a characteristic from an elf file to it

`a2ltool --create --elffile input.elf --characteristic my_var --output newfile.a2l`

#### Create a new a2l file and add multiple measurements from an elf file to it using a regular expression

`a2ltool --create --elffile input.elf --measurement-regex ".*name_pattern\d\d+*" --output newfile.a2l`

#### Create a new a2l file and add multiple measurements from an elf file to it using an address range

`a2ltool --create --elffile input.elf --measurement-range 0x1000 0x3000 --output newfile.a2l`

## A2L Version

#### Change the version of an a2l file, while deleting any incompatible elements

`a2ltool input.a2l --a2lversion 1.5.1 --output downgraded.a2l`

#### Create a new A2L File with a specific version

`a2ltool --create --a2lversion 1.6.1 --output a2lver_161.a2l`

## Check

#### Check an a2lfile for consistency

`a2ltool input.a2l --check --strict`

#### Check for consistency and also verify that the a2l file matches the DWARF2 debug data

`a2ltool input.a2l --elffile sw.elf --update ADDRESSES --update-mode STRICT --check --strict`

## Response Files

#### Use response files containing command arguments

Assume that the file `a2ltool.rsp` exists and contains valid arguments for `a2ltool`.

`a2ltool @a2ltool.rsp`
