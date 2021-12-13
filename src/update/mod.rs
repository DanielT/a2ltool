use super::dwarf::{DebugData, TypeInfo};
use super::ifdata;
use a2lfile::*;
use std::collections::HashSet;

#[cfg(test)]
use std::collections::HashMap;

mod axis_pts;
mod blob;
mod characteristic;
pub mod enums;
mod ifdata_update;
mod instance;
mod measurement;
mod record_layout;

use crate::datatype::*;
use axis_pts::*;
use blob::*;
use characteristic::*;
use instance::*;
use measurement::*;
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
pub(crate) fn update_addresses(
    a2l_file: &mut A2lFile,
    debug_data: &DebugData,
    preserve_unknown: bool
) -> UpdateSumary {
    let use_new_matrix_dim = check_version_1_70(a2l_file);

    let mut summary = UpdateSumary::new();
    for module in &mut a2l_file.project.module {
        let mut reclayout_info = RecordLayoutInfo::build(module);

        // update all AXIS_PTS
        let (updated, not_updated) = update_module_axis_pts(
            module,
            debug_data,
            preserve_unknown,
            &mut reclayout_info
        );
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all MEASUREMENTs
        let (updated, not_updated) = update_module_measurements(
            module,
            debug_data,
            preserve_unknown,
            use_new_matrix_dim
        );
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all CHARACTERISTICs
        let (updated, not_updated) = update_module_characteristics(
            module,
            debug_data,
            preserve_unknown,
            &mut reclayout_info
        );
        summary.characteristic_updated += updated;
        summary.characteristic_not_updated += not_updated;

        // update all BLOBs
        let (updated, not_updated) = update_module_blobs(
            module,
            debug_data,
            preserve_unknown
        );
        summary.blob_updated += updated;
        summary.blob_not_updated += not_updated;

        // update all INSTANCEs
        let (updated, not_updated) = update_module_instances(
            module,
            debug_data,
            preserve_unknown
        );
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
    let components = split_symbol_components(varname);

    // find the symbol in the symbol table
    find_symbol_from_components(&components, debug_data)
}


fn find_symbol_from_components<'a>(components: &Vec<&str>, debug_data: &'a DebugData) -> Option<(u64, &'a TypeInfo)> {
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


// split the symbol into components
// e.g. "my_struct.array_field[5][6]" -> [ "my_struct", "array_field", "[5]", "[6]" ]
fn split_symbol_components(varname: &str) -> Vec<&str> {
    let mut components: Vec<&str> = Vec::new();

    for component in varname.split('.') {
        if let Some(idx) = component.find('[') {
            // "array_field[5][6]" -> "array_field", "[5][6]"
            let (name, indexstring) = component.split_at(idx);
            components.push(name);
            components.extend(indexstring.split_inclusive(']'));
        } else {
            components.push(component);
        }
    }

    components
}


// find the address and type of the current component of a symbol name
fn find_membertype<'a>(
    typeinfo: &'a TypeInfo,
    components: &Vec<&str>,
    component_index: usize,
    address: u64
) -> Option<(u64, &'a TypeInfo)> {
    if component_index >= components.len() {
        Some((address, typeinfo))
    } else {
        match typeinfo {
            TypeInfo::Class { members, .. } |
            TypeInfo::Struct { members, .. } |
            TypeInfo::Union { members, .. } => {
                if let Some((membertype, offset)) = members.get(components[component_index]) {
                    find_membertype(
                        membertype,
                        components,
                        component_index + 1,
                        address + offset
                    )
                } else {
                    None
                }
            }
            TypeInfo::Array { dim, stride, arraytype, .. } => {
                let mut multi_index = 0;
                for idx_pos in 0 .. dim.len() {
                    let arraycomponent = components.get(component_index + idx_pos)?;
                    let indexval = get_index(arraycomponent)?;
                    multi_index = multi_index * dim[idx_pos] as usize + indexval;
                }

                let elementaddr = address + (multi_index as u64 * stride);
                find_membertype(
                    arraytype,
                    components,
                    component_index + dim.len(),
                    elementaddr
                )
            }
            _ => Some((address, typeinfo))
        }
    }
}


// before ASAP2 1.7 array indices in symbol names could not written as [x], but only as _x_
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


// generate adjuste min and max limits based on the datatype.
// since the updater code has no knowledge how the data is handled in the application it
// is only possible to shrink existing limits, but not expand them
fn adjust_limits(typeinfo: &TypeInfo, old_lower_limit: f64, old_upper_limit: f64) -> (f64, f64) {
    let (mut new_lower_limit, mut new_upper_limit) = get_type_limits(typeinfo, old_lower_limit, old_upper_limit);

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

#[test]
fn test_split_symbol_components() {
    let result = split_symbol_components("my_struct.array_field[5][1]");
    assert_eq!(result.len(), 4);
    assert_eq!(result[0], "my_struct");
    assert_eq!(result[1], "array_field");
    assert_eq!(result[2], "[5]");
    assert_eq!(result[3], "[1]");

    let result2 = split_symbol_components("my_struct.array_field._5_._1_");
    assert_eq!(result2.len(), 4);
    assert_eq!(result2[0], "my_struct");
    assert_eq!(result2[1], "array_field");
    assert_eq!(result2[2], "_5_");
    assert_eq!(result2[3], "_1_");
}

#[test]
fn test_find_symbol_of_array() {
    let mut dbgdata = DebugData {
        types: HashMap::new(),
        variables: HashMap::new()
    };
    // global variable: uint32_t my_array[2]
    dbgdata.variables.insert(
        "my_array".to_string(),
        crate::dwarf::VarInfo { address: 0x1234, typeref: 1 });
    dbgdata.types.insert(
        1,
        TypeInfo::Array {
            arraytype: Box::new(TypeInfo::Uint32),
            dim: vec![2],
            size: 8, // total size of the array
            stride: 4
        }
    );

    // try the different array indexing notations
    let result1 = find_symbol("my_array._0_", &dbgdata);
    assert!(result1.is_some());
    // C-style notation is only allowed starting with ASAP2 version 1.7, before that the '[' and ']' are not allowed in names
    let result3 = find_symbol("my_array[0]", &dbgdata);
    assert!(result3.is_some());
}


#[test]
fn test_find_symbol_of_array_in_struct() {
    let mut dbgdata = DebugData {
        types: HashMap::new(),
        variables: HashMap::new()
    };
    // global variable defined in C like this:
    // struct {
    //        uint32_t array_item[2];
    // } my_struct;
    let mut structmembers: HashMap<String, (TypeInfo, u64)> = HashMap::new();
    structmembers.insert(
        "array_item".to_string(),
        (
            TypeInfo::Array {
                arraytype: Box::new(TypeInfo::Uint32),
                dim: vec![2],
                size: 8,
                stride: 4
            },
            0
        )
    );
    dbgdata.variables.insert(
        "my_struct".to_string(),
        crate::dwarf::VarInfo { address: 0xcafe00, typeref: 2 });
    dbgdata.types.insert(
        2,
        TypeInfo::Struct {
            members: structmembers,
            size: 4
        }
    );

    // try the different array indexing notations
    let result1 = find_symbol("my_struct.array_item._0_", &dbgdata);
    assert!(result1.is_some());
    // C-style notation is only allowed starting with ASAP2 version 1.7, before that the '[' and ']' are not allowed in names
    let result3 = find_symbol("my_struct.array_item[0]", &dbgdata);
    assert!(result3.is_some());
}
