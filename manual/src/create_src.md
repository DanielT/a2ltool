## Creating items based on comments in source files

This mode provides compatibility with "Vector ASAP2 Creator".

Here special comments inside the source code of the software define a2l elements for the variables and structures in the code.

The command line option `--from-source` can be given zero or more times to specify a list of file names or file name patterns.
All file name patterns are fully expanded, and then the resulting list of files is read, in order to process special comments.

### File name patterns

File name patterns are an easy way to specify multiple file names.

A simple example is `*.c` - a list of every .c file in the current directory.
A more complex example is `src/**/*.h` - a list of all .h files located at any depth below the src directory.

### File types

a2ltool doesn't care about the type of the files that are processed, as long as they contain text. The syntax of the code inside the files is not evaluated at all, and C-style preprocessor directives are also ignored.
a2ltool only looks for strings that look like comments - blocks enclosed in `/* */` or lines starting with `//`.

### Example

#### Input

src/input_1.c

```text
/*
@@ MAIN_GROUP = main
@@ DESCRIPTION = "Main group description"
@@ END
*/

/*
@@ SYMBOL = test_case
@@ A2L_TYPE = MEASURE
@@ WRITEABLE
@@ alt_name
@@ DATA_TYPE = UBYTE 0x3f [3...40] [0...45]
@@ CONVERSION = LINEAR 2 3 "kkk" 8 4
@@ DESCRIPTION = "Test description"
@@ ALIAS = TestAlias
@@ BASE_OFFSET = 1
@@ GROUP = parent | TestGroup
@@ DIMENSION = 3 4 5 SPLIT USE_TEMPLATE "._%d_[%d]bla%dblub"
@@ ADDRESS = 0x12345678
@@ ADDRESS_EXTENSION = 0x10
@@ EVENT CCP = 0
@@ COLOR = 0xFF0000
@@ VAR_CRITERION = Variant
@@ LAYOUT = TestLayout
@@ BYTE_ORDER = INTEL
@@ END
*/

...
```

src/input_2.c

    /*
    @@ SUB_GROUP = sub
    @@ DESCRIPTION = "Sub group description"
    @@ END
    */

    /*
    @@ SYMBOL = param1
    @@ A2L_TYPE = PARAMETER
    @@ WRITEABLE
    @@ DATA_TYPE = FLOAT [0...100] [-10 ... 1000]
    @@ CONVERSION = FORMULA "x*2+3" INVERSE "(x-3)/2" "unit" 8 4
    @@ DESCRIPTION = "Parameter description"
    @@ ALIAS = ParamAlias
    @@ BASE_OFFSET = 2
    @@ GROUP IN = sub
    @@ DIMENSION = 10 SPLIT USE "_a" "_b" "_c" "_d" "_e" "_f" "_g" "_h" "_i" "_j"
    @@ ADDRESS = 0x87654321
    @@ ADDRESS_EXTENSION = 0x20
    @@ EVENT XCP = FIXED 1
    @@ COLOR = 0x00FF00
    @@ VAR_CRITERION = Variant
    @@ LAYOUT = ParamLayout
    @@ BYTE_ORDER = MOTOROLA
    @@ END
    */

    ...

src/input_3.c

    /*
    @@ CONVERSION = LinearConversion
    @@ A2L_TYPE = LINEAR 12 3
    @@ UNIT = "unit" 5 2
    @@ DESCRIPTION = "Linear conversion"
    @@ END
    */

    /*
    @@ CONVERSION = TableConversion
    @@ A2L_TYPE = TABLE
    @@ 0 0 "zero"
    @@ 10 10 "ten"
    @@ 20 20 "twenty"
    @@ DEFAULT_VALUE "unknown"
    @@ END
    */

src/input_4.c

    /*
    @@ SUB_STRUCTURE = SubStruct
    @@ STRUCTURE = abc
    @@ DATA_TYPE = STRUCTURE TypeName
    @@ DIMENSION = 3 SPLIT
    @@ BASE_OFFSET = 333
    @@ SIZE = 64
    @@ END
    */

    /*
    @@ ELEMENT = ElementName
    @@ STRUCTURE = abc | def
    @@ A2L_TYPE = MEASURE
    @@ DATA_TYPE = ULONG
    @@ END
    */

    /*
    @@ INSTANCE = InstanceName
    @@ STRUCTURE = abc
    @@ ADDRESS = 0x1234
    @@ DIMENSION = 3 4 SPLIT
    @@ SIZE = 9000
    @@ GROUP = GroupName
    @@ OVERWRITE x RANGE = [ -1 ... 1 ]
    @@ OVERWRITE abc | def | ElementName CONVERSION = LINEAR 2 3 "s"
    @@ END
    */

src/input_5.c

    /*
    @@ VAR_CRITERION = Variant
    @@ DESCRIPTION = "Variant description"
    @@ SELECTOR = MEASURE InputMeasurement
    @@   VARIANT = Apple 1 0x0
    @@   VARIANT = Orange 2 0x1000
    @@   VARIANT = Banana 3 0x2000
    @@ END
    */

#### Command

    a2ltool --create --from-source "src/*.c" --output out.a2l

### Syntax

Within the special comments each line must begin with the marker `@@`.

After stripping the `@@` marker, the remaining text is parsed as a `<definition>` according to the following grammar:

#### Definition
    
    <definition> ::=   <symbol>
                    | <element>
                    | <sub-structure>
                    | <instance>
                    | <conversion>
                    | <main-group>
                    | <sub-group>
                    | <variant-criterion>

#### Symbol

    <symbol> ::= "SYMBOL" "=" <identifier>
                 "A2L_TYPE" "=" <item-definition>
                 "END"

#### Element

    <element> ::= "ELEMENT" "=" <identifier>
                  "STRUCTURE" "=" <structure-path>
                  "A2L_TYPE" "=" <item-definition>
                  "END"
    
    <structure-path> ::= <identifier> ( | <identifier> )*

#### Item definition

    <item-definition> ::=   <axis-item-defition>
                          | <curve-item-defintion>
                          | <map-item-definition>
                          | <measure-item-definition>
                          | <parameter-item-definition>
                          | <string-item-definition>
                        
    <axis-item-defition> ::= "AXIS" [ <write-access> ] [ <identifier> ]
                             <data-type>
                             "LAYOUT" "=" <identifier>
                             "DIMENSION" "=" <dimension>
                             [ <input> ]
                             ( <acm-attribute> )*

    <curve-item-defintion> ::= "CURVE" [ <write-access> ] [ <identifier> ]
                               <data-type>
                               "LAYOUT" "=" <identifier>
                               ( <acm-attribute> )*
                               "X_AXIS" "=" <axis-definition>

    <map-item-definition> ::= "MAP" [ <write-access> ] [ <identifier> ]
                              <data-type>
                              "LAYOUT" "=" <identifier>
                              ( <acm-attribute> )*
                              "X_AXIS" "=" <axis-definition>
                              "Y_AXIS" "=" <axis-definition>

    <measure-item-definition> ::= "MEASURE" [ <write-access> ] [ <identifier> ]
                                  <data-type>
                                  ( <attribute> )*

    <parameter-item-definition> ::= "PARAMETER" [ <write-access> ] [ <identifier> ]
                                    <data-type>
                                    ( <attribute> )*

    <string-item-definition> ::= "STRING" <length> [ <write-access> ] [ <identifier> ]
                                 ( <string-attribute> )*

#### Item values

    <data-type> ::=   "DATA_TYPE" "=" "UBYTE" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "UWORD" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "ULONG" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "UINT64" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "SBYTE" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "SWORD" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "SLONG" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "INT64" [ <bitmask> ] [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "FLOAT" [ <range> ] [ <range> ]
                    | "DATA_TYPE" "=" "DOUBLE" [ <range> ] [ <range> ]

    <bitmask> ::= <value>

    <range> ::= "[" <value> "..." <value> "]"

    <write-access> ::= "WRITEABLE" | "READ_ONLY"

    <input> ::= "INPUT" "=" [ "INSTANCE_NAME" ] <identifier>

    <axis-defintion> ::=   "STANDARD" <data-type>
                           <dimension-attr>
                           [ <input> ]
                           [ <conversion-attr> ]
                         | "FIX" ( <value> )+
                           [ <input> ]
                           [ <conversion-attr> ]
                         | "FIX" <range> [ "," <value> ]
                           [ <input> ]
                           [ <conversion-attr> ]
                         | "COMMON" [ "INSTANCE_NAME" ] <identifier>

#### Attribute groups

    <attribute> ::=   <address-attr>
                    | <address-extension-attr>
                    | <alias-attr>
                    | <base-offset-attr>
                    | <byte-order-attr>
                    | <color-attr>
                    | <conversion-attr>
                    | <description-attr>
                    | <dimension-attr> <split>
                    | <event-attr>
                    | <group-attr>
                    | <layout-attr>
                    | <var-criterion-attr>

    <acm-attribute> ::=   <address-attr>
                        | <address-extension-attr>
                        | <alias-attr>
                        | <base-offset-attr>
                        | <byte-order-attr>
                        | <conversion-attr>
                        | <description-attr>
                        | <group-attr>
                        | <var-criterion-attr>

    <string-attribute> ::=   <address-attr>
                           | <address-extension-attr>
                           | <alias-attr>
                           | <base-offset-attr>
                           | <description-attr>
                           | <dimension-attr> <split>
                           | <group-attr>
                           | <var-criterion-attr>

#### Attribute definition

    <address-attr> ::= "ADDRESS" "=" <value>

    <address-extension-attr> ::= "ADDRESS_EXTENSION" "=" <value>

    <alias-attr> ::= "ALIAS" "=" <identifier>

    <base-offset-attr> ::= "BASE_OFFSET" "=" <VALUE>

    <byte-order-attr> ::= "BYTE_ORDER" "=" ( "INTEL" | "MOTOROLA" )

    <color-attr> ::= "COLOR" "=" <value>

    <conversion-attr> ::=   "CONVERSION" "=" <identifier>
                            [[ <length> ] <digits> ]
                          | "CONVERSION" "=" "LINEAR"
                            <factor> <offset> <unit>  [[ <length> ] <digits> ]
                          | "CONVERSION" "=" "FORMULA"
                            <string> [ "INVERSE" <string> ]
                            <unit> [[ <length> ] <digits> ]
                          | "CONVERSION" "=" "TABLE"
                            ( value [<value>] <string> )+
                            [ "DEFAULT_VALUE" <string> ]
                            [ "FORMAT" <length> <digits> ]
                          | "UNIT" "=" [[ <length> ] <digits> ]

    <description-attr> ::= "DESCRIPTION" "=" <string>

    <dimension-attr> ::= "DIMENSION" "=" <value>
                         [<value>] [<value>] [<value>] [<value>]

    <event-attr> ::=   "EVENT" "CCP" "=" <value>
                     | "EVENT" "XCP" "=" "FIXED" <value>
                     | "EVENT" "XCP" "=" "VARIABLE" ( <value> )+
                     | "EVENT" "XCP" "=" "DEFAULT" <value>

    <group-attr> ::= "GROUP" [ "IN" | "OUT" | "DEF" ] "="
                     <identifier> ( | <identifier> )*

    <layout-attr> ::= "LAYOUT" "=" <identifier>

    <var-criterion-attr> ::= "VAR_CRITERION" "=" <identifier>

    <split> ::=   "SPLIT"
                | "SPLIT" "USE" ( <string> )+
                | "SPLIT" "USE_TEMPLATE" <string>

#### Sub-Structure

    <sub-structure> ::= "SUB_STRUCTURE" "=" <identifier>
                        "STRUCTURE" "=" <structure-path>
                        [ "DATA_TYPE" "=" "STRUCTURE <identifier> ]
                        ( <structure-attribute> )*
                        END

    <structure-attribute> ::=   <base-offset-attr>
                              | <dimension-attr>
                              | <size-attr>

    <size-attr> ::= "SIZE" "=" <value>

#### Instance

    <instance> ::= "INSTANCE" "=" <identifier> [ <identifier> ]
                   "STRUCTURE" "=" <identifier>
                   [ <address-attr> ]
                   [ <dimension-attr> [ <split> ]]
                   [ <size-attr> ]
                   [ <group-attr> ]
                   ( <overwrite> )*
                   "END"

    <overwrite> ::= "OVERWRITE" <element-spec> <overwrite-details>

    <overwrite-details> ::=   <alias-attr>
                            | <color-attr>
                            | <conversion-attr>
                            | <description-attr>
                            | <group-attr>
                            | "RANGE" = <range>

    <element-spec> ::= [ <structure-path> | ] <identifier>

#### Conversion

    <conversion> ::= "CONVERSION" "=" <identifier>
                     "A2L_TYPE" = "=" <conversion-type>
                     [ "UNIT" "=" <string> <length> <digits> ]
                     [ "DESCRIPTION" "=" <string> ]
                     "END"

    <conversion-type> ::=   "LINEAR" <factor> <offset>
                          | "FORMULA" <string> [ "INVERSE" <string> ]
                          | "TABLE" ( value [<value>] <string> )+
                            [ "DEFAULT_VALUE" <string> ]

#### Main group

    <main-group> ::= "MAIN_GROUP" "=" <identifier>
                     [ <description-attr> ]
                     "END"

#### Sub group

    <sub-group> ::= "SUB_GROUP" "=" <identifier>
                    [ <description-attr> ]
                    "END"

#### Variant criterion

    <variant-criterion> ::= "VAR_CRITERION" "=" <identifier>
                            [ <description-attr> ]
                            "SELECTOR" = ( "PARAMETER" | "MEASURE" ) <identifier>
                            ( <variant> )*
                            "END"

    <variant> ::= "VARIANT" "=" <identifier> <value> <value>

### Notes

a2ltool only performs minimal consistency checks automatically while creating items from source comments.

For example, the added items might reference COMPU_METHODs or RECORD_LAYOUTs by name, and these are not guaranteed to exist.

A full sanity check of the file can be performed using the option `--check`, which can be passed together with `--from-source`.
