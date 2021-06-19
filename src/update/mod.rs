use std::collections::HashSet;
use super::dwarf::{DebugData, TypeInfo};
use super::ifdata;
use a2lfile::*;

mod axis_pts;
mod characteristic;
mod measurement;
mod blob;
mod instance;
mod enums;
mod record_layout;
mod ifdata_update;

use axis_pts::*;
use characteristic::*;
use measurement::*;
use blob::*;
use instance::*;
use record_layout::*;

pub(crate) struct UpdateSumary {
    pub(crate) measurement_updated: u32,
    pub(crate) measurement_not_updated: u32,
    pub(crate) characteristic_updated: u32,
    pub(crate) characteristic_not_updated: u32,
    pub(crate) axis_pts_updated: u32,
    pub(crate) axis_pts_not_updated: u32,
    pub(crate) blob_updated: u32,
    pub(crate) blob_not_updated: u32,
    pub(crate) instance_updated: u32,
    pub(crate) instance_not_updated: u32,
}



// perform an address update.
// This update can be destructive (any object that cannot be updated will be discarded)
// or non-destructive (addresses of invalid objects will be set to zero).
pub(crate) fn update_addresses(a2l_file: &mut A2lFile, debug_data: &DebugData, preserve_unknown: bool) -> UpdateSumary {
    let use_new_matrix_dim = check_version_1_70(a2l_file);

    let mut summary = UpdateSumary::new();
    for module in &mut a2l_file.project.module {
        let mut reclayout_info = RecordLayoutInfo::build(module);

        // update all AXIS_PTS
        let (updated, not_updated) = update_module_axis_pts(module, debug_data, preserve_unknown, &mut reclayout_info);
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all MEASUREMENTs
        let (updated, not_updated) = update_module_measurements(module, debug_data, preserve_unknown, use_new_matrix_dim);
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all CHARACTERISTICs
        let (updated, not_updated) = update_module_characteristics(module, debug_data, preserve_unknown, &mut reclayout_info);
        summary.characteristic_updated += updated;
        summary.characteristic_not_updated += not_updated;

        // update all BLOBs
        let (updated, not_updated) = update_module_blobs(module, debug_data, preserve_unknown);
        summary.blob_updated += updated;
        summary.blob_not_updated += not_updated;

        // update all INSTANCEs
        let (updated, not_updated) = update_module_instances(module, debug_data, preserve_unknown);
        summary.blob_updated += updated;
        summary.blob_not_updated += not_updated;
    }

    summary
}


// check if the file version is >= 1.70
fn check_version_1_70(a2l_file: &A2lFile) -> bool {
    if let Some(ver) = &a2l_file.asap2_version {
        ver.version_no > 1 || (ver.version_no == 1 && ver.upgrade_no >= 70)
    } else {
        false
    }
}


// try to get the symbol name used in the elf file, and find its address and type
fn get_symbol_info<'a>(
    name: &str,
    opt_symbol_link: &Option<SymbolLink>,
    ifdata_vec: &Vec<IfData>,
    debug_data: &'a DebugData
) -> (Option<(u64, &'a TypeInfo)>, String) {
    let mut symbol_info = None;
    let mut symbol_name = "".to_string();

    // preferred: get symbol information from a SYMBOL_LINK attribute
    if let Some(symbol_link) = opt_symbol_link {
        symbol_name = symbol_link.symbol_name.clone();
        symbol_info = find_symbol(&symbol_name, debug_data);
    }

    // second option: get symbol information from a CANAPE_EXT block inside of IF_DATA.
    // The content of IF_DATA can be different for each tool vendor, but the blocks used
    // by the Vector tools are understood by some other software.
    if symbol_info.is_none() {
        if let Some(ifdata_symbol_name) = get_symbol_name_from_ifdata(ifdata_vec) {
            symbol_name = ifdata_symbol_name;
            symbol_info = find_symbol(&symbol_name, debug_data);
        }
    }

    // If there is no SYMBOL_LINK and no (usable) IF_DATA, then maybe the object name is also the symol name
    if symbol_info.is_none() && opt_symbol_link.is_none() {
        symbol_name = name.to_string();
        symbol_info = find_symbol(&symbol_name, debug_data);
    }
    
    (symbol_info, symbol_name)
}


// update or create a SYMBOL_LINK for the given symbol name
fn set_symbol_link(opt_symbol_link: &mut Option<SymbolLink>, symbol_name: String) {
    if let Some(symbol_link) = opt_symbol_link {
        symbol_link.symbol_name = symbol_name;
    } else {
        *opt_symbol_link = Some(SymbolLink::new(symbol_name, 0));
    }
}


// MEASUREMENT objects put the address in an optional keyword, ECU_ADDRESS.
// this is created or updated here
fn set_measurement_ecu_address(opt_ecu_address: &mut Option<EcuAddress>, address: u64) {
    if let Some(ecu_address) = opt_ecu_address {
        ecu_address.address = address as u32;
    } else {
        *opt_ecu_address = Some(EcuAddress::new(address as u32));
    }
}


// A MEASUREMENT object contains a BITMASK for bitfield elements
// it will be created/updated/deleted here, depending on the new data type of the variable
fn set_measurement_bitmask(opt_bitmask: &mut Option<BitMask>, datatype: &TypeInfo) {
    if let TypeInfo::Bitfield { bit_offset, bit_size, ..} = datatype {
        let mask = ((1 << bit_size) - 1) << bit_offset;
        if let Some(bit_mask) = opt_bitmask {
            bit_mask.mask = mask;
        } else {
            *opt_bitmask = Some(BitMask::new(mask));
        }
    } else {
        *opt_bitmask = None;
    }
}


// Try to get a symbol name from an IF_DATA object.
// specifically the pseudo-standard CANAPE_EXT could be present and contain symbol information
fn get_symbol_name_from_ifdata(ifdata_vec: &Vec<IfData>) -> Option<String> {
    for ifdata in ifdata_vec {
        if let Some(decoded) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = decoded.canape_ext {
                if let Some(link_map) = canape_ext.link_map {
                    return Some(link_map.symbol_name.to_owned());
                }
            }
        }
    }
    None
}


// find a symbol in the elf_info data structure that was derived from the DWARF debug info in the elf file
fn find_symbol<'a>(varname: &str, debug_data: &'a DebugData) -> Option<(u64, &'a TypeInfo)> {
    // split the a2l symbol name: e.g. "motortune.param._0_" -> ["motortune", "param", "_0_"]
    let components: Vec<&str> = varname.split('.').collect();
    // the first component of the symbol name is the name of the global variable.
    if let Some(varinfo) = debug_data.variables.get(components[0]) {
        // we also need the type in order to resolve struct members, etc.
        if let Some(vartype) = debug_data.types.get(&varinfo.typeref) {
            // all further components of the symbol name are struct/union members or array indices
            find_membertype(vartype, components, 1, varinfo.address)
        } else {
            // this exists for completeness, but shouldn't happen with a correctly generated elffile
            // if the variable is present in the elffile, then the type should also be present
            if components.len() == 1 {
                Some((varinfo.address, &TypeInfo::Uint8))
            } else {
                None
            }
        }
    } else {
        None
    }
}


// find the address and type of the current component of a symbol name
fn find_membertype<'a>(typeinfo: &'a TypeInfo, components: Vec<&str>, component_index: usize, address: u64) -> Option<(u64, &'a TypeInfo)> {
    if component_index >= components.len() {
        Some((address, typeinfo))
    } else {
        match typeinfo {
            TypeInfo::Struct { members, .. } |
            TypeInfo::Union { members, .. } => {
                if let Some((membertype, offset)) = members.get(components[component_index]) {
                    find_membertype(membertype, components, component_index + 1, address + offset)
                } else {
                    None
                }
            }
            TypeInfo::Array { dim, stride, arraytype, .. } => {
                let mut multi_index = 0;
                for idx_pos in 0 .. dim.len() {
                    let indexval = get_index(components[component_index + idx_pos])?;
                    multi_index = multi_index * dim[idx_pos] as usize + indexval;
                }

                let elementaddr = address + (multi_index as u64 * stride);
                find_membertype(arraytype, components, component_index + dim.len(), elementaddr)
            }
            _ => Some((address, typeinfo))
        }
    }
}


// for some reason array indices in symbol names in a2l files are not written as [x], but as _x_
// this function will get the numerical index for either representation
fn get_index(idxstr: &str) -> Option<usize> {
    if (idxstr.starts_with('_') && idxstr.ends_with('_')) ||
       (idxstr.starts_with('[') && idxstr.ends_with(']')) {
        let idxstrlen = idxstr.len();
        match idxstr[1..idxstrlen-1].parse() {
            Ok(val) => Some(val),
            Err(_) => None
        }
    } else {
        None
    }
}


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
        TypeInfo::Bitfield { basetype, ..} => get_a2l_datatype(basetype),
        TypeInfo::Pointer(size) => {
            if *size == 8 {
                DataType::AUint64
            } else {
                DataType::Ulong
            }
        }
        TypeInfo::Enum { size, .. } |
        TypeInfo::Other(size) => {
            match *size {
                8 => DataType::AUint64,
                4 => DataType::Ulong,
                2 => DataType::Uword,
                1 | _ => DataType::Ubyte
            }
        }
        TypeInfo::Array { arraytype, .. } => {
            get_a2l_datatype(arraytype)
        }
        _ => DataType::Ubyte
    }
}


// generate adjuste min and max limits based on the datatype.
// since the updater code has no knowledge how the data is handled in the application it
// is only possible to shrink existing limits, but not expand them
fn adjust_limits(typeinfo: &TypeInfo, old_lower_limit: f64, old_upper_limit: f64) -> (f64, f64) {
    let (mut new_lower_limit, mut new_upper_limit) = match typeinfo {
        TypeInfo::Array {arraytype,..} => adjust_limits(arraytype, old_lower_limit, old_upper_limit),
        TypeInfo::Bitfield {bit_size, basetype, ..} => {
            let raw_range: u64 = 1 << bit_size;
            match &**basetype {
                TypeInfo::Sint8 |
                TypeInfo::Sint16 |
                TypeInfo::Sint32 |
                TypeInfo::Sint64 => {
                    let lower = -((raw_range / 2) as f64);
                    let upper = (raw_range / 2) as f64;
                    (lower, upper)
                }
                _ => (0f64, raw_range as f64)
            }
        }
        TypeInfo::Double => (f64::MIN, f64::MAX),
        TypeInfo::Float => (f32::MIN as f64, f32::MAX as f64),
        TypeInfo::Uint8 => (u8::MIN as f64, u8::MAX as f64),
        TypeInfo::Uint16 => (u16::MIN as f64, u16::MAX as f64),
        TypeInfo::Uint32 => (u32::MIN as f64, u32::MAX as f64),
        TypeInfo::Uint64 => (u64::MIN as f64, u64::MAX as f64),
        TypeInfo::Sint8 => (i8::MIN as f64, i8::MAX as f64),
        TypeInfo::Sint16 => (i16::MIN as f64, i16::MAX as f64),
        TypeInfo::Sint32 => (i32::MIN as f64, i32::MAX as f64),
        TypeInfo::Sint64 => (i64::MIN as f64, i64::MAX as f64),
        TypeInfo::Enum {enumerators, ..} => {
            let lower = enumerators.iter().map(|val| val.1).min().unwrap_or_else(|| 0) as f64;
            let upper = enumerators.iter().map(|val| val.1).max().unwrap_or_else(|| 0) as f64;
            (lower, upper)
        }
        _ => (old_lower_limit, old_upper_limit)
    };

    // if non-zero limits exist, then the limits can only shrink, but not grow
    // if the limits are both zero, then the maximum range allowed by the datatype is used
    if old_lower_limit != 0f64 || old_upper_limit != 0f64 {
        if new_lower_limit < old_lower_limit {
            new_lower_limit = old_lower_limit;
        }
        if new_upper_limit > old_upper_limit {
            new_upper_limit = old_upper_limit;
        }
    }

    (new_lower_limit, new_upper_limit)
}


// remove the identifiers in removed_items from the item_list
fn cleanup_item_list(item_list: &mut Vec<String>, removed_items: &HashSet<String>) {
    let mut new_list = Vec::<String>::new();
    std::mem::swap(item_list, &mut new_list);

    for item in new_list {
        if removed_items.get(&item).is_none() {
            item_list.push(item);
        }
    }
}


impl UpdateSumary {
    fn new() -> Self {
        Self {
            axis_pts_not_updated: 0,
            axis_pts_updated: 0,
            blob_not_updated: 0,
            blob_updated: 0,
            characteristic_not_updated: 0,
            characteristic_updated: 0,
            measurement_not_updated: 0,
            measurement_updated: 0,
            instance_not_updated: 0,
            instance_updated: 0
        }
    }
}

