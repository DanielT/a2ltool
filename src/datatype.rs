use crate::dwarf::{DwarfDataType, TypeInfo};
use a2lfile::DataType;

// map the datatypes from the elf_info to a2l datatypes
// the only really relevant cases are for the integer, floating point and enum types
// all other types cannot be sensibly measured / calibrated anyway
pub(crate) fn get_a2l_datatype(typeinfo: &TypeInfo) -> DataType {
    match &typeinfo.datatype {
        DwarfDataType::Uint8 => DataType::Ubyte,
        DwarfDataType::Uint16 => DataType::Uword,
        DwarfDataType::Uint32 => DataType::Ulong,
        DwarfDataType::Uint64 => DataType::AUint64,
        DwarfDataType::Sint8 => DataType::Sbyte,
        DwarfDataType::Sint16 => DataType::Sword,
        DwarfDataType::Sint32 => DataType::Slong,
        DwarfDataType::Sint64 => DataType::AInt64,
        DwarfDataType::Float => DataType::Float32Ieee,
        DwarfDataType::Double => DataType::Float64Ieee,
        DwarfDataType::Bitfield { basetype, .. } => get_a2l_datatype(basetype),
        DwarfDataType::Pointer(size, _) => {
            if *size == 8 {
                DataType::AUint64
            } else {
                DataType::Ulong
            }
        }
        DwarfDataType::Enum { size, .. } | DwarfDataType::Other(size) => match *size {
            8 => DataType::AUint64,
            4 => DataType::Ulong,
            2 => DataType::Uword,
            _ => DataType::Ubyte,
        },
        DwarfDataType::Array { arraytype, .. } => get_a2l_datatype(arraytype),
        _ => DataType::Ubyte,
    }
}

pub(crate) fn get_type_limits(
    typeinfo: &TypeInfo,
    default_lower: f64,
    default_upper: f64,
) -> (f64, f64) {
    let (new_lower_limit, new_upper_limit) = match &typeinfo.datatype {
        DwarfDataType::Array { arraytype, .. } => {
            get_type_limits(arraytype, default_lower, default_upper)
        }
        DwarfDataType::Bitfield {
            bit_size, basetype, ..
        } => {
            let raw_range: u64 = 1 << bit_size;
            match &basetype.datatype {
                DwarfDataType::Sint8
                | DwarfDataType::Sint16
                | DwarfDataType::Sint32
                | DwarfDataType::Sint64 => {
                    let lower = -((raw_range / 2) as f64);
                    let upper = (raw_range / 2) as f64;
                    (lower, upper)
                }
                _ => (0f64, raw_range as f64),
            }
        }
        DwarfDataType::Double => (f64::MIN, f64::MAX),
        DwarfDataType::Float => (f64::from(f32::MIN), f64::from(f32::MAX)),
        DwarfDataType::Uint8 => (f64::from(u8::MIN), f64::from(u8::MAX)),
        DwarfDataType::Uint16 => (f64::from(u16::MIN), f64::from(u16::MAX)),
        DwarfDataType::Uint32 => (f64::from(u32::MIN), f64::from(u32::MAX)),
        DwarfDataType::Uint64 => (u64::MIN as f64, u64::MAX as f64),
        DwarfDataType::Sint8 => (f64::from(i8::MIN), f64::from(i8::MAX)),
        DwarfDataType::Sint16 => (f64::from(i16::MIN), f64::from(i16::MAX)),
        DwarfDataType::Sint32 => (f64::from(i32::MIN), f64::from(i32::MAX)),
        DwarfDataType::Sint64 => (i64::MIN as f64, i64::MAX as f64),
        DwarfDataType::Enum { enumerators, .. } => {
            let lower = enumerators.iter().map(|val| val.1).min().unwrap_or(0) as f64;
            let upper = enumerators.iter().map(|val| val.1).max().unwrap_or(0) as f64;
            (lower, upper)
        }
        _ => (default_lower, default_upper),
    };
    (new_lower_limit, new_upper_limit)
}
