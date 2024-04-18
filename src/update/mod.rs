use crate::dwarf::{make_simple_unit_name, DebugData, TypeInfo};
use crate::{ifdata, A2lVersion};
use a2lfile::{
    A2lFile, A2lObject, AddrType, AddressType, BitMask, EcuAddress, IfData, MatrixDim, Module,
    SymbolLink,
};
use std::collections::{HashMap, HashSet};

mod axis_pts;
mod blob;
mod characteristic;
pub mod enums;
mod ifdata_update;
mod instance;
mod measurement;
mod record_layout;
pub(crate) mod typedef;

use crate::datatype::{get_a2l_datatype, get_type_limits};
use crate::dwarf::DwarfDataType;
use crate::symbol::{find_symbol, SymbolInfo};
use axis_pts::*;
use blob::{cleanup_removed_blobs, update_module_blobs};
use characteristic::*;
use instance::update_module_instances;
use measurement::*;
use record_layout::*;
use typedef::update_module_typedefs;

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

#[derive(Debug, Clone)]
pub(crate) enum TypedefReferrer {
    Instance(usize),
    StructureComponent(String, String),
}

pub(crate) struct TypedefNames {
    axis: HashSet<String>,
    blob: HashSet<String>,
    characteristic: HashSet<String>,
    measurement: HashSet<String>,
    structure: HashSet<String>,
}

type TypedefsRefInfo<'a> = HashMap<String, Vec<(Option<&'a TypeInfo>, TypedefReferrer)>>;

// perform an address update.
// This update can be destructive (any object that cannot be updated will be discarded)
// or non-destructive (addresses of invalid objects will be set to zero).
pub(crate) fn update_addresses(
    a2l_file: &mut A2lFile,
    debug_data: &DebugData,
    log_msgs: &mut Vec<String>,
    preserve_unknown: bool,
    enable_structures: bool,
) -> UpdateSumary {
    let version = A2lVersion::from(&*a2l_file);

    let mut summary = UpdateSumary::new();
    for module in &mut a2l_file.project.module {
        let mut reclayout_info = RecordLayoutInfo::build(module);

        // update all AXIS_PTS
        let (updated, not_updated) = update_module_axis_pts(
            module,
            debug_data,
            log_msgs,
            preserve_unknown,
            version,
            &mut reclayout_info,
        );
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all MEASUREMENTs
        let (updated, not_updated) =
            update_module_measurements(module, debug_data, log_msgs, preserve_unknown, version);
        summary.measurement_updated += updated;
        summary.measurement_not_updated += not_updated;

        // update all CHARACTERISTICs
        let (updated, not_updated) = update_module_characteristics(
            module,
            debug_data,
            log_msgs,
            preserve_unknown,
            version,
            &mut reclayout_info,
        );
        summary.characteristic_updated += updated;
        summary.characteristic_not_updated += not_updated;

        // update all BLOBs
        let (updated, not_updated) =
            update_module_blobs(module, debug_data, log_msgs, preserve_unknown);
        summary.blob_updated += updated;
        summary.blob_not_updated += not_updated;

        let typedef_names = TypedefNames::new(module);

        // update all INSTANCEs
        let (updated, not_updated, typedef_ref_info) = update_module_instances(
            module,
            debug_data,
            log_msgs,
            preserve_unknown,
            &typedef_names,
        );
        summary.instance_updated += updated;
        summary.instance_not_updated += not_updated;

        if enable_structures {
            update_module_typedefs(
                module,
                debug_data,
                log_msgs,
                preserve_unknown,
                typedef_ref_info,
                typedef_names,
                &mut reclayout_info,
            );
        }
    }

    summary
}

// try to get the symbol name used in the elf file, and find its address and type
fn get_symbol_info<'a>(
    name: &str,
    opt_symbol_link: &Option<SymbolLink>,
    ifdata_vec: &[IfData],
    debug_data: &'a DebugData,
) -> Result<SymbolInfo<'a>, Vec<String>> {
    let mut symbol_link_errmsg = None;
    let mut ifdata_errmsg = None;
    let mut object_name_errmsg = None;
    // preferred: get symbol information from a SYMBOL_LINK attribute
    if let Some(symbol_link) = opt_symbol_link {
        match find_symbol(&symbol_link.symbol_name, debug_data) {
            Ok(sym_info) => return Ok(sym_info),
            Err(errmsg) => symbol_link_errmsg = Some(errmsg),
        };
    }

    // second option: get symbol information from a CANAPE_EXT block inside of IF_DATA.
    // The content of IF_DATA can be different for each tool vendor, but the blocks used
    // by the Vector tools are understood by some other software.
    if let Some(ifdata_symbol_name) = get_symbol_name_from_ifdata(ifdata_vec) {
        match find_symbol(&ifdata_symbol_name, debug_data) {
            Ok(sym_info) => return Ok(sym_info),
            Err(errmsg) => ifdata_errmsg = Some(errmsg),
        };
    }

    // If there is no SYMBOL_LINK and no (usable) IF_DATA, then maybe the object name is also the symbol name
    if opt_symbol_link.is_none() {
        match find_symbol(name, debug_data) {
            Ok(sym_info) => return Ok(sym_info),
            Err(errmsg) => object_name_errmsg = Some(errmsg),
        };
    }

    // all attempts to get a matching symbol from the debug info have failed
    // construct an array of (unique) error messages
    let mut errorstrings = Vec::<String>::new();
    if let Some(errmsg) = symbol_link_errmsg {
        errorstrings.push(errmsg);
    }
    if let Some(errmsg) = ifdata_errmsg {
        // no duplicates wanted
        if !errorstrings.contains(&errmsg) {
            errorstrings.push(errmsg);
        }
    }
    if let Some(errmsg) = object_name_errmsg {
        // no duplicates wanted
        if !errorstrings.contains(&errmsg) {
            errorstrings.push(errmsg);
        }
    }
    Err(errorstrings)
}

fn log_update_errors(errorlog: &mut Vec<String>, errmsgs: Vec<String>, blockname: &str, line: u32) {
    for msg in errmsgs {
        errorlog.push(format!("Error updating {blockname} on line {line}: {msg}"));
    }
}

pub(crate) fn make_symbol_link_string(sym_info: &SymbolInfo, debug_data: &DebugData) -> String {
    let mut name = sym_info.name.to_string();
    if !sym_info.is_unique {
        if let Some(funcname) = &sym_info.function_name {
            name.push_str("{Function:");
            name.push_str(funcname);
            name.push('}');
        }
        for ns in sym_info.namespaces {
            name.push_str("{Namespace:");
            name.push_str(ns);
            name.push('}');
        }
        if let Some(unit_name) = make_simple_unit_name(debug_data, sym_info.unit_idx) {
            name.push_str("{CompileUnit:");
            name.push_str(&unit_name);
            name.push('}');
        }
        name.push_str("{Namespace:Global}");
    }
    name
}

// update or create a SYMBOL_LINK for the given symbol name
pub(crate) fn set_symbol_link(opt_symbol_link: &mut Option<SymbolLink>, symbol_name: String) {
    if let Some(symbol_link) = opt_symbol_link {
        symbol_link.symbol_name = symbol_name;
    } else {
        *opt_symbol_link = Some(SymbolLink::new(symbol_name, 0));
    }
}

// update the MATRIX_DIM of a MEASUREMENT or CHARACTERISTIC
pub(crate) fn set_matrix_dim(
    opt_matrix_dim: &mut Option<MatrixDim>,
    typeinfo: &TypeInfo,
    new_format: bool,
) {
    let mut matrix_dim_values = Vec::new();
    let mut cur_typeinfo = typeinfo;
    // compilers can represent multi-dimensional arrays in two different ways:
    // either as nested arrays, each with one dimension, or as one array with multiple dimensions
    while let DwarfDataType::Array { dim, arraytype, .. } = &cur_typeinfo.datatype {
        for val in dim {
            matrix_dim_values.push(u16::try_from(*val).unwrap_or(u16::MAX));
        }
        cur_typeinfo = &**arraytype;
    }

    if matrix_dim_values.is_empty() {
        // current type is not an array, so delete the MATRIX_DIM
        *opt_matrix_dim = None;
    } else {
        if !new_format {
            // in the file versions before 1.70, MATRIX_DIM must have exactly 3 values
            // starting with 1.70 any nonzero number of values is permitted
            while matrix_dim_values.len() < 3 {
                matrix_dim_values.push(1);
            }
            matrix_dim_values.truncate(3);
        }
        let matrix_dim = opt_matrix_dim.get_or_insert(MatrixDim::new());
        matrix_dim.dim_list = matrix_dim_values;
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

// CHARACTERISTIC and MEASUREMENT objects contain a BIT_MASK for bitfield elements
// it will be created/updated/deleted here, depending on the new data type of the variable
pub(crate) fn set_bitmask(opt_bitmask: &mut Option<BitMask>, typeinfo: &TypeInfo) {
    if let DwarfDataType::Bitfield {
        bit_offset,
        bit_size,
        ..
    } = &typeinfo.datatype
    {
        // make sure we don't panic for bit_size = 32
        let wide_mask: u64 = ((1 << bit_size) - 1) << bit_offset;
        let mask: u32 = wide_mask.try_into().unwrap_or(0xffff_ffff);
        if let Some(bit_mask) = opt_bitmask {
            bit_mask.mask = mask;
        } else {
            let mut bm = BitMask::new(mask);
            bm.get_layout_mut().item_location.0 = (0, true); // write bitmask as hex by default
            *opt_bitmask = Some(bm);
        }
    } else {
        *opt_bitmask = None;
    }
}

/// set or delete the `ADDRESS_TYPE`
pub(crate) fn set_address_type(address_type_opt: &mut Option<AddressType>, newtype: &TypeInfo) {
    if let DwarfDataType::Pointer(ptsize, _) = &newtype.datatype {
        let address_type = address_type_opt.get_or_insert(AddressType::new(AddrType::Direct));
        address_type.address_type = match ptsize {
            1 => AddrType::Pbyte,
            2 => AddrType::Pword,
            4 => AddrType::Plong,
            8 => AddrType::Plonglong,
            _ => AddrType::Direct,
        };
    } else {
        *address_type_opt = None;
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

// generate adjusted min and max limits based on the datatype.
// since the updater code has no knowledge how the data is handled in the application it
// is only possible to shrink existing limits, but not expand them
fn adjust_limits(typeinfo: &TypeInfo, old_lower_limit: f64, old_upper_limit: f64) -> (f64, f64) {
    let (mut new_lower_limit, mut new_upper_limit) =
        get_type_limits(typeinfo, old_lower_limit, old_upper_limit);

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
            instance_updated: 0,
        }
    }
}

impl TypedefNames {
    pub(crate) fn new(module: &Module) -> Self {
        Self {
            axis: module
                .typedef_axis
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            blob: module
                .typedef_blob
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            characteristic: module
                .typedef_characteristic
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            measurement: module
                .typedef_measurement
                .iter()
                .map(|item| item.name.clone())
                .collect(),
            structure: module
                .typedef_structure
                .iter()
                .map(|item| item.name.clone())
                .collect(),
        }
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.structure.contains(name)
            || self.measurement.contains(name)
            || self.characteristic.contains(name)
            || self.blob.contains(name)
            || self.axis.contains(name)
    }
}
