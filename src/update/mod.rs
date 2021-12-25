use super::dwarf::{DebugData, TypeInfo};
use super::ifdata;
use a2lfile::*;
use std::collections::HashSet;

mod axis_pts;
mod blob;
mod characteristic;
pub mod enums;
mod ifdata_update;
mod instance;
mod measurement;
mod record_layout;

use crate::datatype::*;
use crate::symbol::*;
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
    log_msgs: &mut Vec<String>,
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
            log_msgs,
            preserve_unknown,
            &mut reclayout_info
        );
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all MEASUREMENTs
        let (updated, not_updated) = update_module_measurements(
            module,
            debug_data,
            log_msgs,
            preserve_unknown,
            use_new_matrix_dim
        );
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all CHARACTERISTICs
        let (updated, not_updated) = update_module_characteristics(
            module,
            debug_data,
            log_msgs,
            preserve_unknown,
            &mut reclayout_info
        );
        summary.characteristic_updated += updated;
        summary.characteristic_not_updated += not_updated;

        // update all BLOBs
        let (updated, not_updated) = update_module_blobs(
            module,
            debug_data,
            log_msgs,
            preserve_unknown
        );
        summary.blob_updated += updated;
        summary.blob_not_updated += not_updated;

        // update all INSTANCEs
        let (updated, not_updated) = update_module_instances(
            module,
            debug_data,
            log_msgs,
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
    ifdata_vec: &[IfData],
    debug_data: &'a DebugData
) -> Result<(u64, &'a TypeInfo, String), Vec<String>> {
    let mut symbol_link_errmsg = None;
    let mut ifdata_errmsg = None;
    let mut object_name_errmsg = None;
    // preferred: get symbol information from a SYMBOL_LINK attribute
    if let Some(symbol_link) = opt_symbol_link {
        match find_symbol(&symbol_link.symbol_name, debug_data) {
            Ok((addr, typeinfo)) => {
                return Ok((addr, typeinfo, symbol_link.symbol_name.clone()))
            }
            Err(errmsg) => symbol_link_errmsg = Some(errmsg)
        };
    }

    // second option: get symbol information from a CANAPE_EXT block inside of IF_DATA.
    // The content of IF_DATA can be different for each tool vendor, but the blocks used
    // by the Vector tools are understood by some other software.
    if let Some(ifdata_symbol_name) = get_symbol_name_from_ifdata(ifdata_vec) {
        match find_symbol(&ifdata_symbol_name, debug_data) {
            Ok((addr, typeinfo)) => {
                return Ok((addr, typeinfo, ifdata_symbol_name))
            }
            Err(errmsg) => ifdata_errmsg = Some(errmsg)
        };
    }

    // If there is no SYMBOL_LINK and no (usable) IF_DATA, then maybe the object name is also the symbol name
    if opt_symbol_link.is_none() {
        match find_symbol(name, debug_data) {
            Ok((addr, typeinfo)) => {
                return Ok((addr, typeinfo, name.to_string()))
            }
            Err(errmsg) => object_name_errmsg = Some(errmsg)
        };
    }

    // all attempts to get a matching symbol from the debug info have failed
    // construct an array of (unique) error messages
    let mut errorstrings = Vec::<String>::new();
    if let Some(errmsg) = symbol_link_errmsg {
        errorstrings.push(errmsg)
    }
    if let Some(errmsg) = ifdata_errmsg {
        // no duplicates wanted
        if !errorstrings.contains(&errmsg) {
            errorstrings.push(errmsg)
        }
    }
    if let Some(errmsg) = object_name_errmsg {
        // no duplicates wanted
        if !errorstrings.contains(&errmsg) {
            errorstrings.push(errmsg)
        }
    }
    Err(errorstrings)
}


fn log_update_errors(errorlog: &mut Vec<String>, errmsgs: Vec<String>, blockname: &str, line: u32) {
    for msg in errmsgs {
        errorlog.push(format!("Error updating {} on line {}: {}", blockname, line, msg));
    }
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
fn get_symbol_name_from_ifdata(ifdata_vec: &[IfData]) -> Option<String> {
    for ifdata in ifdata_vec {
        if let Some(decoded) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = decoded.canape_ext {
                if let Some(link_map) = canape_ext.link_map {
                    return Some(link_map.symbol_name);
                }
            }
        }
    }
    None
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

