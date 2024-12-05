use crate::debuginfo::pdb::typereader::{DbgDataType, TypeInfo, TypeReaderData};

// unfortunately, the PDB crate does not provide the constants for the built-in types
/// No type
const BUILTIN_TYPE_NOTYPE: u32 = 0x0000;
/// Absolute symbol
const BUILTIN_TYPE_ABS_SYMBOL: u32 = 0x0001;
/// Segment type
const BUILTIN_TYPE_SEGMENT: u32 = 0x0002;
/// Void type
const BUILTIN_TYPE_VOID: u32 = 0x0003;
/// BASIC 8 byte currency value
const BUILTIN_TYPE_CURRENCY: u32 = 0x0004;
/// BASIC string (near)
const BUILTIN_TYPE_NBASICSTR: u32 = 0x0005;
/// BASIC string (far)
const BUILTIN_TYPE_FBASICSTR: u32 = 0x0006;
/// type not translated by cvpack
const BUILTIN_TYPE_NOTTRANS: u32 = 0x0007;
/// OLE/COM HRESULT
const BUILTIN_TYPE_HRESULT: u32 = 0x0008;
/// 8 bit signed character
const BUILTIN_TYPE_CHAR: u32 = 0x0010;
/// 16 bit signed integer
const BUILTIN_TYPE_SHORT: u32 = 0x0011;
/// 32 bit signed integer
const BUILTIN_TYPE_LONG: u32 = 0x0012;
/// 64 bit signed integer
const BUILTIN_TYPE_QUAD: u32 = 0x0013;
/// 128 bit signed integer
const BUILTIN_TYPE_OCT: u32 = 0x0014;
/// 8 bit unsigned character
const BUILTIN_TYPE_UCHAR: u32 = 0x0020;
/// 16 bit unsigned integer
const BUILTIN_TYPE_USHORT: u32 = 0x0021;
/// 32 bit unsigned integer
const BUILTIN_TYPE_ULONG: u32 = 0x0022;
/// 64 bit unsigned integer
const BUILTIN_TYPE_UQUAD: u32 = 0x0023;
/// 128 bit unsigned integer
const BUILTIN_TYPE_UOCT: u32 = 0x0024;
/// 8 bit boolean
const BUILTIN_TYPE_BOOL08: u32 = 0x0030;
/// 16 bit boolean
const BUILTIN_TYPE_BOOL16: u32 = 0x0031;
/// 32 bit boolean
const BUILTIN_TYPE_BOOL32: u32 = 0x0032;
/// 64 bit boolean
const BUILTIN_TYPE_BOOL64: u32 = 0x0033;
/// 32 bit floating point
const BUILTIN_TYPE_REAL32: u32 = 0x0040;
/// 64 bit floating point
const BUILTIN_TYPE_REAL64: u32 = 0x0041;
/// 80 bit floating point
const BUILTIN_TYPE_REAL80: u32 = 0x0042;
/// 128 bit floating point
const BUILTIN_TYPE_REAL128: u32 = 0x0043;
/// 48 bit floating point
const BUILTIN_TYPE_REAL48: u32 = 0x0044;
/// 32 bit floating point (partial precision)
const BUILTIN_TYPE_REAL32PP: u32 = 0x0045;
/// 16 bit floating point
const BUILTIN_TYPE_REAL16: u32 = 0x0046;
/// 32 bit complex value
const BUILTIN_TYPE_COMPLEX32: u32 = 0x0050;
/// 64 bit complex value
const BUILTIN_TYPE_COMPLEX64: u32 = 0x0051;
/// 80 bit complex value
const BUILTIN_TYPE_COMPLEX80: u32 = 0x0052;
/// 128 bit complex value
const BUILTIN_TYPE_COMPLEX128: u32 = 0x0053;
/// bit
const BUILTIN_TYPE_BIT: u32 = 0x0060;
/// Pascal CHAR
const BUILTIN_TYPE_PASCHAR: u32 = 0x0061;
/// 32 bit boolean (0=false, -1=true)
const BUILTIN_TYPE_BOOL32FF: u32 = 0x0062;
/// 8 bit signed integer
const BUILTIN_TYPE_INT8: u32 = 0x0068;
/// 8 bit unsigned integer
const BUILTIN_TYPE_UINT8: u32 = 0x0069;
/// really a char - 8 bit
const BUILTIN_TYPE_RCHAR: u32 = 0x0070;
/// 16 bit unicode character
const BUILTIN_TYPE_WCHAR: u32 = 0x0071;
/// 16 bit signed integer
const BUILTIN_TYPE_INT16: u32 = 0x0072;
/// 16 bit unsigned integer
const BUILTIN_TYPE_UINT16: u32 = 0x0073;
/// 32 bit signed integer
const BUILTIN_TYPE_INT32: u32 = 0x0074;
/// 32 bit unsigned integer
const BUILTIN_TYPE_UINT32: u32 = 0x0075;
/// 64 bit signed integer
const BUILTIN_TYPE_INT64: u32 = 0x0076;
/// 64 bit unsigned integer
const BUILTIN_TYPE_UINT64: u32 = 0x0077;
/// 128 bit signed integer
const BUILTIN_TYPE_INT128: u32 = 0x0078;
/// 128 bit unsigned integer
const BUILTIN_TYPE_UINT128: u32 = 0x0079;
/// 16 bit unicode character
const BUILTIN_TYPE_CHAR16: u32 = 0x007a;
/// 32 bit unicode character
const BUILTIN_TYPE_CHAR32: u32 = 0x007b;

// pointer types - the x:y notation indicates the use of segmented addressing (DOS)
/// not a pointer
const BUILTIN_PTR_NONE: u32 = 0x00;
/// 16 bit near pointer
const BUILTIN_PTR_NEAR: u32 = 0x01;
/// 16:16 far pointer - unsupported
const BUILTIN_PTR_FAR: u32 = 0x02;
/// 16:16 huge pointer - unsupported
const BUILTIN_PTR_HUGE: u32 = 0x03;
/// 32 bit pointer
const BUILTIN_PTR_32: u32 = 0x04;
/// 16:32 far pointer - unsupported
const BUILTIN_PTR_32FAR: u32 = 0x05;
/// 64 bit pointer
const BUILTIN_PTR_64: u32 = 0x06;

const BUILTIN_LIMIT: u32 = 0x1000;

pub(crate) fn is_builtin_type(type_index: u32) -> bool {
    type_index < BUILTIN_LIMIT
}

pub(crate) fn read_builtin_type(
    type_index: u32,
    typereader_data: &mut TypeReaderData,
) -> Result<(), String> {
    /* built-in types have index values below 0x1000.
    In this case the index value can be split into an upper and lower byte, where the
    lower byte indicates the type, and the upper byte indicates "not a pointer", "near pointer", "far pointer", etc. */
    let subtype = type_index & 0xff;
    let pointer_type = (type_index >> 8) & 0xff;

    let datatype = match subtype {
        BUILTIN_TYPE_NOTYPE
        | BUILTIN_TYPE_ABS_SYMBOL
        | BUILTIN_TYPE_SEGMENT
        | BUILTIN_TYPE_CURRENCY
        | BUILTIN_TYPE_NBASICSTR
        | BUILTIN_TYPE_FBASICSTR
        | BUILTIN_TYPE_NOTTRANS
        | BUILTIN_TYPE_PASCHAR => {
            // unsupported types will be represented as uint8
            // This allows the variables to be inserted in the a2l, and address update works as expected.
            // If the user wants to measure/adjust such a variable, they can modify the a2l file by hand.
            DbgDataType::Uint8
        }
        BUILTIN_TYPE_VOID => DbgDataType::Other(0),
        BUILTIN_TYPE_CHAR | BUILTIN_TYPE_RCHAR | BUILTIN_TYPE_INT8 => DbgDataType::Sint8,
        BUILTIN_TYPE_SHORT | BUILTIN_TYPE_INT16 => DbgDataType::Sint16,
        BUILTIN_TYPE_LONG | BUILTIN_TYPE_INT32 => DbgDataType::Sint32,
        BUILTIN_TYPE_QUAD | BUILTIN_TYPE_INT64 => DbgDataType::Sint64,

        BUILTIN_TYPE_UCHAR | BUILTIN_TYPE_UINT8 | BUILTIN_TYPE_BOOL08 => DbgDataType::Uint8,

        BUILTIN_TYPE_USHORT | BUILTIN_TYPE_WCHAR | BUILTIN_TYPE_UINT16 | BUILTIN_TYPE_BOOL16
        | BUILTIN_TYPE_CHAR16 => DbgDataType::Uint16,

        BUILTIN_TYPE_HRESULT
        | BUILTIN_TYPE_ULONG
        | BUILTIN_TYPE_UINT32
        | BUILTIN_TYPE_BOOL32
        | BUILTIN_TYPE_BOOL32FF
        | BUILTIN_TYPE_CHAR32 => DbgDataType::Uint32,

        BUILTIN_TYPE_UQUAD | BUILTIN_TYPE_UINT64 | BUILTIN_TYPE_BOOL64 => DbgDataType::Uint64,

        BUILTIN_TYPE_REAL32 => DbgDataType::Float,
        BUILTIN_TYPE_REAL64 => DbgDataType::Double,

        BUILTIN_TYPE_REAL80 | BUILTIN_TYPE_COMPLEX80 => {
            // a2l does not support 80 bit floating point numbers or complex numbers
            DbgDataType::Other(10)
        }
        BUILTIN_TYPE_OCT
        | BUILTIN_TYPE_INT128
        | BUILTIN_TYPE_UOCT
        | BUILTIN_TYPE_UINT128
        | BUILTIN_TYPE_REAL128
        | BUILTIN_TYPE_COMPLEX128 => {
            // a2l does not support any 128 bit types
            DbgDataType::Other(16)
        }
        BUILTIN_TYPE_REAL48 => {
            // a2l does not support 48 bit floating point numbers
            DbgDataType::Other(6)
        }
        BUILTIN_TYPE_REAL32PP | BUILTIN_TYPE_COMPLEX32 => {
            // a2l does not support 32 bit partial precision floating point numbers or complex numbers
            DbgDataType::Other(4)
        }
        BUILTIN_TYPE_REAL16 => {
            // a2l does not support 16 bit floating point numbers
            DbgDataType::Other(2)
        }
        BUILTIN_TYPE_COMPLEX64 => {
            // a2l does not support 64 bit complex numbers
            DbgDataType::Other(8)
        }
        BUILTIN_TYPE_BIT => {
            todo!("not sure how to handle bit type - width?");
        }
        _ => {
            return Err(format!("Unknown built-in type: {subtype}"));
        }
    };
    typereader_data.types.insert(
        subtype as usize,
        TypeInfo {
            datatype,
            name: None,
            unit_idx: 0,
            dbginfo_offset: 0,
        },
    );

    let pointer_datatype = match pointer_type {
        BUILTIN_PTR_NONE => None,
        BUILTIN_PTR_NEAR => return Err("Near pointers are not supported".to_string()),
        BUILTIN_PTR_FAR => return Err("Far pointers are not supported".to_string()),
        BUILTIN_PTR_HUGE => return Err("Huge pointers are not supported".to_string()),
        BUILTIN_PTR_32 => Some(DbgDataType::Pointer(4, subtype as usize)),
        BUILTIN_PTR_32FAR => return Err("32-bit far pointers are not supported".to_string()),
        BUILTIN_PTR_64 => Some(DbgDataType::Pointer(8, subtype as usize)),
        _ => {
            return Err(format!(
                "Unknown built-in pointer type: {pointer_type} for type {type_index:04x}"
            ))
        }
    };
    if let Some(pointer_datatype) = pointer_datatype {
        typereader_data.types.insert(
            type_index as usize,
            TypeInfo {
                datatype: pointer_datatype,
                name: None,
                unit_idx: 0,
                dbginfo_offset: 0,
            },
        );
    }

    Ok(())
}
