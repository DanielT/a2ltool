use crate::debuginfo::{DbgDataType, TypeInfo};
use a2lfile::DataType;

// map the datatypes from the elf_info to a2l datatypes
// the only really relevant cases are for the integer, floating point and enum types
// all other types cannot be sensibly measured / calibrated anyway
pub(crate) fn get_a2l_datatype(typeinfo: &TypeInfo) -> DataType {
    match &typeinfo.datatype {
        DbgDataType::Uint8 => DataType::Ubyte,
        DbgDataType::Uint16 => DataType::Uword,
        DbgDataType::Uint32 => DataType::Ulong,
        DbgDataType::Uint64 => DataType::AUint64,
        DbgDataType::Sint8 => DataType::Sbyte,
        DbgDataType::Sint16 => DataType::Sword,
        DbgDataType::Sint32 => DataType::Slong,
        DbgDataType::Sint64 => DataType::AInt64,
        DbgDataType::Float => DataType::Float32Ieee,
        DbgDataType::Double => DataType::Float64Ieee,
        DbgDataType::Bitfield { basetype, .. } => get_a2l_datatype(basetype),
        DbgDataType::Pointer(size, _) => {
            if *size == 8 {
                DataType::AUint64
            } else {
                DataType::Ulong
            }
        }
        DbgDataType::Enum { size, signed, .. } => {
            if *signed {
                match *size {
                    8 => DataType::AInt64,
                    4 => DataType::Slong,
                    2 => DataType::Sword,
                    _ => DataType::Sbyte,
                }
            } else {
                match *size {
                    8 => DataType::AUint64,
                    4 => DataType::Ulong,
                    2 => DataType::Uword,
                    _ => DataType::Ubyte,
                }
            }
        }
        DbgDataType::Other(size) => match *size {
            8 => DataType::AUint64,
            4 => DataType::Ulong,
            2 => DataType::Uword,
            _ => DataType::Ubyte,
        },
        DbgDataType::Array { arraytype, .. } => get_a2l_datatype(arraytype),
        _ => DataType::Ubyte,
    }
}

pub(crate) fn get_type_limits(
    typeinfo: &TypeInfo,
    default_lower: f64,
    default_upper: f64,
) -> (f64, f64) {
    let (new_lower_limit, new_upper_limit) = match &typeinfo.datatype {
        DbgDataType::Array { arraytype, .. } => {
            get_type_limits(arraytype, default_lower, default_upper)
        }
        DbgDataType::Bitfield {
            bit_size, basetype, ..
        } => {
            let raw_range: u64 = 1 << bit_size;
            match &basetype.datatype {
                DbgDataType::Sint8
                | DbgDataType::Sint16
                | DbgDataType::Sint32
                | DbgDataType::Sint64 => {
                    let lower = -((raw_range / 2) as f64);
                    let upper = (raw_range / 2) as f64;
                    (lower, upper)
                }
                _ => (0f64, raw_range as f64),
            }
        }
        DbgDataType::Double => (f64::MIN, f64::MAX),
        DbgDataType::Float => (f64::from(f32::MIN), f64::from(f32::MAX)),
        DbgDataType::Uint8 => (f64::from(u8::MIN), f64::from(u8::MAX)),
        DbgDataType::Uint16 => (f64::from(u16::MIN), f64::from(u16::MAX)),
        DbgDataType::Uint32 => (f64::from(u32::MIN), f64::from(u32::MAX)),
        DbgDataType::Uint64 => (u64::MIN as f64, u64::MAX as f64),
        DbgDataType::Sint8 => (f64::from(i8::MIN), f64::from(i8::MAX)),
        DbgDataType::Sint16 => (f64::from(i16::MIN), f64::from(i16::MAX)),
        DbgDataType::Sint32 => (f64::from(i32::MIN), f64::from(i32::MAX)),
        DbgDataType::Sint64 => (i64::MIN as f64, i64::MAX as f64),
        DbgDataType::Enum { enumerators, .. } => {
            let lower = enumerators.iter().map(|val| val.1).min().unwrap_or(0) as f64;
            let upper = enumerators.iter().map(|val| val.1).max().unwrap_or(0) as f64;
            (lower, upper)
        }
        _ => (default_lower, default_upper),
    };
    (new_lower_limit, new_upper_limit)
}
