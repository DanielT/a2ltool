use crate::dwarf::TypeInfo;
use a2lfile::DataType;

// map the datatypes from the elf_info to a2l datatypes
// the only really relevant cases are for the integer, floating point and enum types
// all other types cannot be sensibly measured / calibrated anyway
pub(crate) fn get_a2l_datatype(datatype: &TypeInfo) -> DataType {
    match datatype {
        TypeInfo::Uint8 => DataType::Ubyte,
        TypeInfo::Uint16 => DataType::Uword,
        TypeInfo::Uint32 => DataType::Ulong,
        TypeInfo::Uint64 => DataType::AUint64,
        TypeInfo::Sint8 => DataType::Sbyte,
        TypeInfo::Sint16 => DataType::Sword,
        TypeInfo::Sint32 => DataType::Slong,
        TypeInfo::Sint64 => DataType::AInt64,
        TypeInfo::Float => DataType::Float32Ieee,
        TypeInfo::Double => DataType::Float64Ieee,
        TypeInfo::Bitfield { basetype, .. } => get_a2l_datatype(basetype),
        TypeInfo::Pointer(size) => {
            if *size == 8 {
                DataType::AUint64
            } else {
                DataType::Ulong
            }
        }
        TypeInfo::Enum { size, .. } | TypeInfo::Other(size) => match *size {
            8 => DataType::AUint64,
            4 => DataType::Ulong,
            2 => DataType::Uword,
            _ => DataType::Ubyte,
        },
        TypeInfo::Array { arraytype, .. } => get_a2l_datatype(arraytype),
        _ => DataType::Ubyte,
    }
}

pub(crate) fn get_type_limits(
    typeinfo: &TypeInfo,
    default_lower: f64,
    default_upper: f64,
) -> (f64, f64) {
    let (new_lower_limit, new_upper_limit) = match typeinfo {
        TypeInfo::Array { arraytype, .. } => {
            get_type_limits(arraytype, default_lower, default_upper)
        }
        TypeInfo::Bitfield {
            bit_size, basetype, ..
        } => {
            let raw_range: u64 = 1 << bit_size;
            match &**basetype {
                TypeInfo::Sint8 | TypeInfo::Sint16 | TypeInfo::Sint32 | TypeInfo::Sint64 => {
                    let lower = -((raw_range / 2) as f64);
                    let upper = (raw_range / 2) as f64;
                    (lower, upper)
                }
                _ => (0f64, raw_range as f64),
            }
        }
        TypeInfo::Double => (f64::MIN, f64::MAX),
        TypeInfo::Float => (f64::from(f32::MIN), f64::from(f32::MAX)),
        TypeInfo::Uint8 => (f64::from(u8::MIN), f64::from(u8::MAX)),
        TypeInfo::Uint16 => (f64::from(u16::MIN), f64::from(u16::MAX)),
        TypeInfo::Uint32 => (f64::from(u32::MIN), f64::from(u32::MAX)),
        TypeInfo::Uint64 => (u64::MIN as f64, u64::MAX as f64),
        TypeInfo::Sint8 => (f64::from(i8::MIN), f64::from(i8::MAX)),
        TypeInfo::Sint16 => (f64::from(i16::MIN), f64::from(i16::MAX)),
        TypeInfo::Sint32 => (f64::from(i32::MIN), f64::from(i32::MAX)),
        TypeInfo::Sint64 => (i64::MIN as f64, i64::MAX as f64),
        TypeInfo::Enum { enumerators, .. } => {
            let lower = enumerators.iter().map(|val| val.1).min().unwrap_or(0) as f64;
            let upper = enumerators.iter().map(|val| val.1).max().unwrap_or(0) as f64;
            (lower, upper)
        }
        _ => (default_lower, default_upper),
    };
    (new_lower_limit, new_upper_limit)
}
