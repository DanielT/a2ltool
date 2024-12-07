use crate::debuginfo::{make_simple_unit_name, DebugData, TypeInfo};
use crate::{ifdata, A2lVersion};
use a2lfile::{
    A2lFile, A2lObject, AddrType, AddressType, BitMask, CompuMethod, EcuAddress, IfData, MatrixDim,
    Module, SymbolLink,
};
use instance::update_all_module_instances;
use std::collections::{HashMap, HashSet};
use std::ops::AddAssign;

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
use crate::debuginfo::DbgDataType;
use crate::symbol::{find_symbol, SymbolInfo};
use axis_pts::*;
use blob::{cleanup_removed_blobs, update_all_module_blobs};
use characteristic::*;
use measurement::*;
use record_layout::*;
use typedef::update_module_typedefs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateType {
    Full,
    Addresses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateMode {
    Default,
    Strict,
    Preserve,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq)]
enum UpdateResult {
    Updated,
    SymbolNotFound {
        blocktype: &'static str,
        name: String,
        line: u32,
        errors: Vec<String>,
    },
    InvalidDataType {
        blocktype: &'static str,
        name: String,
        line: u32,
    },
}

// the data used by the a2l update has been split into two parts.
// The A2lUpdateInfo struct contains the data that is constant for the whole update process.
#[derive(Debug)]
pub(crate) struct A2lUpdateInfo<'dbg> {
    pub(crate) debug_data: &'dbg DebugData,
    pub(crate) preserve_unknown: bool,
    pub(crate) strict_update: bool,
    pub(crate) full_update: bool,
    pub(crate) version: A2lVersion,
    pub(crate) enable_structures: bool,
    pub(crate) compu_method_index: HashMap<String, usize>,
}

// This struct contains the data that is modified / updated during the a2l update process.
#[derive(Debug)]
pub(crate) struct A2lUpdater<'a2l> {
    module: &'a2l mut Module,
    reclayout_info: RecordLayoutInfo,
}

type TypedefsRefInfo<'a> = HashMap<String, Vec<(Option<&'a TypeInfo>, TypedefReferrer)>>;

// perform an address update.
// This update can be destructive (any object that cannot be updated will be discarded)
// or non-destructive (addresses of invalid objects will be set to zero).
pub(crate) fn update_a2l(
    a2l_file: &mut A2lFile,
    debug_data: &DebugData,
    log_msgs: &mut Vec<String>,
    update_type: UpdateType,
    update_mode: UpdateMode,
    enable_structures: bool,
) -> (UpdateSumary, bool) {
    let version = A2lVersion::from(&*a2l_file);
    let mut summary = UpdateSumary::new();
    let mut strict_error = false;
    for module in &mut a2l_file.project.module {
        let (mut data, update_info) = init_update(
            debug_data,
            module,
            version,
            update_type,
            update_mode,
            enable_structures,
        );
        let (module_summary, module_strict_error) = run_update(&mut data, &update_info, log_msgs);
        summary += module_summary;
        strict_error |= module_strict_error;
    }
    (summary, strict_error)
}

pub fn init_update<'a2l, 'dbg>(
    debug_data: &'dbg DebugData,
    module: &'a2l mut Module,
    version: A2lVersion,
    update_type: UpdateType,
    update_mode: UpdateMode,
    enable_structures: bool,
) -> (A2lUpdater<'a2l>, A2lUpdateInfo<'dbg>) {
    let preserve_unknown = update_mode == UpdateMode::Preserve;
    let strict_update = update_mode == UpdateMode::Strict;
    let full_update = update_type == UpdateType::Full;
    let reclayout_info = RecordLayoutInfo::build(module);

    let compu_method_index = module
        .compu_method
        .iter()
        .enumerate()
        .map(|(idx, item)| (item.name.clone(), idx))
        .collect::<HashMap<_, _>>();
    (
        A2lUpdater {
            module,
            reclayout_info,
        },
        A2lUpdateInfo {
            debug_data,
            preserve_unknown,
            strict_update,
            full_update,
            version,
            enable_structures,
            compu_method_index,
        },
    )
}

fn run_update(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo,
    log_msgs: &mut Vec<String>,
) -> (UpdateSumary, bool) {
    let mut summary = UpdateSumary::new();
    let mut strict_error = false;

    // update all AXIS_PTS
    let result = update_all_module_axis_pts(data, info);
    strict_error |= result.iter().any(|r| r != &UpdateResult::Updated);
    let (updated, not_updated) = log_update_results(log_msgs, &result);
    summary.axis_pts_updated += updated;
    summary.axis_pts_not_updated += not_updated;

    // update all MEASUREMENTs
    let results = update_all_module_measurements(data, info);
    strict_error |= results.iter().any(|r| r != &UpdateResult::Updated);
    let (updated, not_updated) = log_update_results(log_msgs, &results);
    summary.measurement_updated += updated;
    summary.measurement_not_updated += not_updated;

    // update all CHARACTERISTICs
    let results = update_all_module_characteristics(data, info);
    strict_error |= results.iter().any(|r| r != &UpdateResult::Updated);
    let (updated, not_updated) = log_update_results(log_msgs, &results);
    summary.characteristic_updated += updated;
    summary.characteristic_not_updated += not_updated;

    // update all BLOBs
    let results = update_all_module_blobs(data, info);
    strict_error |= results.iter().any(|r| r != &UpdateResult::Updated);
    let (updated, not_updated) = log_update_results(log_msgs, &results);
    summary.blob_updated += updated;
    summary.blob_not_updated += not_updated;

    let typedef_names = TypedefNames::new(data.module);

    // update all INSTANCEs
    let (update_result, typedef_ref_info) = update_all_module_instances(data, info, &typedef_names);
    strict_error |= results.iter().any(|r| r != &UpdateResult::Updated);
    let (updated, not_updated) = log_update_results(log_msgs, &update_result);
    summary.instance_updated += updated;
    summary.instance_not_updated += not_updated;

    if info.full_update && info.enable_structures {
        update_module_typedefs(
            info,
            data.module,
            log_msgs,
            typedef_ref_info,
            typedef_names,
            &mut data.reclayout_info,
        );
    }

    (summary, strict_error)
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

fn log_update_results(errorlog: &mut Vec<String>, results: &[UpdateResult]) -> (u32, u32) {
    let mut updated = 0;
    let mut not_updated = 0;
    for result in results {
        match result {
            UpdateResult::Updated => updated += 1,
            UpdateResult::SymbolNotFound {
                blocktype,
                name,
                line,
                errors,
            } => {
                for err in errors {
                    errorlog.push(format!(
                        "Error updating {blocktype} {name} on line {line}: {err}",
                    ));
                }
                log_update_errors(errorlog, errors.clone(), blocktype, *line);
                not_updated += 1;
            }
            UpdateResult::InvalidDataType {
                blocktype,
                name,
                line,
            } => {
                errorlog.push(format!(
                    "Error updating {blocktype} {name} on line {line}: data type has changed",
                ));
                updated += 1;
            }
        }
    }

    (updated, not_updated)
}

pub(crate) fn make_symbol_link_string(sym_info: &SymbolInfo, debug_data: &DebugData) -> String {
    let mut name = sym_info.name.to_string();
    let mut has_discriminiant = false;
    if !sym_info.is_unique {
        if let Some(funcname) = &sym_info.function_name {
            name.push_str("{Function:");
            name.push_str(funcname);
            name.push('}');
            has_discriminiant = true;
        }
        for ns in sym_info.namespaces {
            name.push_str("{Namespace:");
            name.push_str(ns);
            name.push('}');
            has_discriminiant = true;
        }
        if let Some(unit_name) = make_simple_unit_name(debug_data, sym_info.unit_idx) {
            name.push_str("{CompileUnit:");
            name.push_str(&unit_name);
            name.push('}');
            has_discriminiant = true;
        }
        if has_discriminiant {
            // adding the tag {Namespace:Global} only makes sense if there are other tags
            name.push_str("{Namespace:Global}");
        }
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
    while let DbgDataType::Array { dim, arraytype, .. } = &cur_typeinfo.datatype {
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
        if ecu_address.address == 0 {
            // force hex output for the address, if the address was set as "0" (decimal)
            ecu_address.get_layout_mut().item_location.0 .1 = true;
        }
        ecu_address.address = address as u32;
    } else {
        *opt_ecu_address = Some(EcuAddress::new(address as u32));
    }
}

// CHARACTERISTIC and MEASUREMENT objects contain a BIT_MASK for bitfield elements
// it will be created/updated/deleted here, depending on the new data type of the variable
pub(crate) fn set_bitmask(opt_bitmask: &mut Option<BitMask>, typeinfo: &TypeInfo) {
    if let DbgDataType::Bitfield {
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
        // if there was a bitmask already configured, it is probably an unexplicit bitfield (a bit
        // mask is configured in the a2l, but in the code, it is an integer with hardcoded shift and
        // mask), so we should not remove the bitmask from the a2l otherwise the configuration will
        // be lost
        //*opt_bitmask = None;
    }
}

/// set or delete the `ADDRESS_TYPE`
pub(crate) fn set_address_type(address_type_opt: &mut Option<AddressType>, newtype: &TypeInfo) {
    if let DbgDataType::Pointer(ptsize, _) = &newtype.datatype {
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
fn adjust_limits(
    typeinfo: &TypeInfo,
    old_lower_limit: f64,
    old_upper_limit: f64,
    opt_compu_method: Option<&CompuMethod>,
) -> (f64, f64) {
    let (mut new_lower_limit, mut new_upper_limit) =
        get_type_limits(typeinfo, old_lower_limit, old_upper_limit);

    if let Some(cm) = opt_compu_method {
        match cm.conversion_type {
            a2lfile::ConversionType::Form => {
                // formula-based compu method - discard the type based limits and continue using the original limits
                // This is the sanest approach, since a2ltool does not implement a parser for mathematical expressions
                new_lower_limit = old_lower_limit;
                new_upper_limit = old_upper_limit;
            }
            a2lfile::ConversionType::Linear => {
                // for a linear compu method, the limits are physical values
                // f(x)=ax + b; PHYS = f(INT)
                if let Some(c) = &cm.coeffs_linear {
                    if c.a >= 0.0 {
                        new_lower_limit = c.a * new_lower_limit + c.b;
                        new_upper_limit = c.a * new_upper_limit + c.b;
                    } else {
                        // factor a is negative, so the lower and upper limits are swapped
                        new_upper_limit = c.a * new_lower_limit + c.b;
                        new_lower_limit = c.a * new_upper_limit + c.b;
                    }
                }
            }
            a2lfile::ConversionType::RatFunc => {
                // f(x)=(ax^2 + bx + c)/(dx^2 + ex + f); INT = f(PHYS)
                if let Some(c) = &cm.coeffs {
                    // we're only handling the simple linear case here
                    if c.a == 0.0 && c.d == 0.0 && c.e == 0.0 && c.f != 0.0 {
                        // now the rational function is reduced to
                        //   y = (bx + c) / f
                        // which can be inverted to
                        //   x = (fy - c) / b
                        let func = |y: f64| (c.f / c.b) * y - (c.c / c.b);
                        new_lower_limit = func(new_lower_limit);
                        new_upper_limit = func(new_upper_limit);
                        if new_lower_limit > new_upper_limit {
                            std::mem::swap(&mut new_lower_limit, &mut new_upper_limit);
                        }
                    } else {
                        // complex formula:
                        // revert the limits to the input values, so that they don't get adjusted
                        new_lower_limit = old_lower_limit;
                        new_upper_limit = old_upper_limit;
                    }
                }
            }
            a2lfile::ConversionType::Identical
            | a2lfile::ConversionType::TabIntp
            | a2lfile::ConversionType::TabNointp
            | a2lfile::ConversionType::TabVerb => {
                // identical and all table-based compu methods have direct int-to-phys mapping
                // no need to adjust the calculated limits
            }
        }
    }

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

    // safety check: the limits may not be infinite.
    // This could happen if the compu method multiplies an f64 datatype limit with any number > 1
    if new_lower_limit == -f64::INFINITY {
        new_lower_limit = f64::MIN;
    } else if new_lower_limit == f64::INFINITY {
        new_lower_limit = f64::MAX;
    }
    if new_upper_limit == -f64::INFINITY {
        new_upper_limit = f64::MIN;
    } else if new_upper_limit == f64::INFINITY {
        new_upper_limit = f64::MAX;
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

impl AddAssign for UpdateSumary {
    fn add_assign(&mut self, other: Self) {
        self.axis_pts_not_updated += other.axis_pts_not_updated;
        self.axis_pts_updated += other.axis_pts_updated;
        self.blob_not_updated += other.blob_not_updated;
        self.blob_updated += other.blob_updated;
        self.characteristic_not_updated += other.characteristic_not_updated;
        self.characteristic_updated += other.characteristic_updated;
        self.measurement_not_updated += other.measurement_not_updated;
        self.measurement_updated += other.measurement_updated;
        self.instance_not_updated += other.instance_not_updated;
        self.instance_updated += other.instance_updated;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        debuginfo::{DbgDataType, TypeInfo},
        A2lVersion,
    };
    use a2lfile::{Coeffs, CoeffsLinear, CompuMethod, ConversionType};
    use std::ffi::OsString;

    #[test]
    fn test_adjust_limits() {
        let typeinfo = TypeInfo {
            name: None,
            unit_idx: 0,
            datatype: DbgDataType::Uint8,
            dbginfo_offset: 0,
        };
        let mut compu_method = CompuMethod::new(
            "name".to_string(),
            "".to_string(),
            ConversionType::Linear,
            "".to_string(),
            "".to_string(),
        );
        compu_method.coeffs_linear = Some(CoeffsLinear::new(0.1, 10.0));

        let (lower, upper) = adjust_limits(&typeinfo, 0.0, 100.0, Some(&compu_method));
        assert_eq!(lower, 10.0);
        assert_eq!(upper, 35.5);

        // see issue #32: the calculated value range for a uint8 variable can be much larger than 0-255
        let typeinfo = TypeInfo {
            name: None,
            unit_idx: 0,
            datatype: DbgDataType::Uint8,
            dbginfo_offset: 0,
        };
        let mut compu_method = CompuMethod::new(
            "name".to_string(),
            "".to_string(),
            ConversionType::RatFunc,
            "".to_string(),
            "".to_string(),
        );
        compu_method.coeffs = Some(Coeffs::new(0., 0.025, 0., 0., 0., 1.0));

        let (lower, upper) = adjust_limits(&typeinfo, 0.0, 0.0, Some(&compu_method));
        assert_eq!(lower, 0.0);
        assert_eq!(upper, 10200.0);

        // for some RAT_FUNC compu method parameters, the limit calculation can go to infinity.
        // Even the calculation order is a concern here, since multiplication before division
        // could cause this even if the end result should be smaller than f64::MAX
        let typeinfo = TypeInfo {
            name: None,
            unit_idx: 0,
            datatype: DbgDataType::Double,
            dbginfo_offset: 0,
        };
        let mut compu_method = CompuMethod::new(
            "name".to_string(),
            "".to_string(),
            ConversionType::RatFunc,
            "".to_string(),
            "".to_string(),
        );
        compu_method.coeffs = Some(Coeffs::new(0., 4.0, 0., 0., 0., 2.0));

        let (lower, upper) = adjust_limits(&typeinfo, f64::MIN, f64::MAX, Some(&compu_method));
        assert_ne!(lower, f64::MIN);
        assert_ne!(upper, f64::MAX);
    }

    fn test_setup(a2l_name: &str) -> (crate::debuginfo::DebugData, a2lfile::A2lFile) {
        let mut log_msgs = Vec::new();
        let a2l = a2lfile::load(
            a2l_name,
            Some(ifdata::A2MLVECTOR_TEXT.to_string()),
            &mut log_msgs,
            true,
        )
        .unwrap();
        let debug_data = crate::debuginfo::DebugData::load_dwarf(
            &OsString::from("fixtures/bin/update_test.elf"),
            false,
        )
        .unwrap();
        (debug_data, a2l)
    }

    #[test]
    fn test_update_axis_pts_ok() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test1.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_axis_pts(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 3);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 3);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());

        // test full update
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Full,
            UpdateMode::Default,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_axis_pts(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 3);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 3);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());
    }

    #[test]
    fn test_update_axis_pts_bad() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test2.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );
        let result = update_all_module_axis_pts(&mut data, &info);
        assert_eq!(result.len(), 4);
        assert!(matches!(result[0], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[1], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[2], UpdateResult::Updated));
        assert!(matches!(result[3], UpdateResult::SymbolNotFound { .. }));
    }

    #[test]
    fn test_update_blob_ok() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test1.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_blobs(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 2);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 2);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());

        // test full update
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Full,
            UpdateMode::Default,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_blobs(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 2);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 2);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());
    }

    #[test]
    fn test_update_blob_bad() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test2.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );
        let result = update_all_module_blobs(&mut data, &info);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[1], UpdateResult::Updated));
        assert!(matches!(result[2], UpdateResult::SymbolNotFound { .. }));
    }

    #[test]
    fn test_update_characteristic_ok() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test1.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_characteristics(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 6);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 6);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());

        // test full update
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Full,
            UpdateMode::Default,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_characteristics(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 6);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 6);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());
    }

    #[test]
    fn test_update_characteristic_bad() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test2.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );
        let result = update_all_module_characteristics(&mut data, &info);
        assert_eq!(result.len(), 7);
        assert!(matches!(result[0], UpdateResult::InvalidDataType { .. }));
        // assert!(matches!(result[1], UpdateResult::InvalidDataType { .. })); // verify currently does not check the size in AXIS_DESCR
        assert!(matches!(result[2], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[3], UpdateResult::Updated));
        assert!(matches!(result[4], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[5], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[6], UpdateResult::SymbolNotFound { .. }));
    }

    #[test]
    fn test_update_instance_ok() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test1.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );

        let mut log_msgs = Vec::new();
        let typedef_names = TypedefNames::new(data.module);
        let (result, _) = update_all_module_instances(&mut data, &info, &typedef_names);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 1);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 1);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());

        // test full update
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Full,
            UpdateMode::Default,
            true,
        );

        let mut log_msgs = Vec::new();
        let typedef_names = TypedefNames::new(data.module);
        let (result, _) = update_all_module_instances(&mut data, &info, &typedef_names);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 1);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 1);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());
    }

    #[test]
    fn test_update_instance_bad() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test2.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );
        let typedef_names = TypedefNames::new(data.module);
        let (result, _) = update_all_module_instances(&mut data, &info, &typedef_names);
        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], UpdateResult::Updated));
        assert!(matches!(result[1], UpdateResult::Updated));
        assert!(matches!(result[2], UpdateResult::SymbolNotFound { .. }));
    }

    #[test]
    fn test_update_measurement_ok() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test1.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Full,
            UpdateMode::Default,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_measurements(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 6);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 6);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());

        // test full update
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Full,
            UpdateMode::Default,
            true,
        );

        let mut log_msgs = Vec::new();
        let result = update_all_module_measurements(&mut data, &info);
        assert!(result.iter().all(|r| r == &UpdateResult::Updated));
        assert_eq!(result.len(), 6);
        let (updated, not_updated) = log_update_results(&mut log_msgs, &result);
        assert_eq!(updated, 6);
        assert_eq!(not_updated, 0);
        assert!(log_msgs.is_empty());
    }

    #[test]
    fn test_update_measurement_bad() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test2.a2l");

        // test address only update, in strict mode
        let version = A2lVersion::from(&a2l);
        let (mut data, info) = init_update(
            &debug_data,
            &mut a2l.project.module[0],
            version,
            UpdateType::Addresses,
            UpdateMode::Strict,
            true,
        );
        let result = update_all_module_measurements(&mut data, &info);
        assert_eq!(result.len(), 7);
        assert!(matches!(result[0], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[1], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[2], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[3], UpdateResult::Updated));
        assert!(matches!(result[4], UpdateResult::Updated));
        assert!(matches!(result[5], UpdateResult::InvalidDataType { .. }));
        assert!(matches!(result[6], UpdateResult::SymbolNotFound { .. }));
    }

    #[test]
    fn test_update_a2l_ok() {
        let (debug_data, mut a2l) = test_setup("fixtures/a2l/update_test1.a2l");

        // test address only update, in strict mode
        let mut log_msgs = Vec::new();
        let (summary, strict_error) = update_a2l(
            &mut a2l,
            &debug_data,
            &mut log_msgs,
            UpdateType::Addresses,
            UpdateMode::Strict,
            false,
        );
        assert!(!strict_error);
        assert_eq!(summary.axis_pts_not_updated, 0);
        assert_eq!(summary.axis_pts_updated, 3);
        assert_eq!(summary.blob_not_updated, 0);
        assert_eq!(summary.blob_updated, 2);
        assert_eq!(summary.characteristic_not_updated, 0);
        assert_eq!(summary.characteristic_updated, 6);
        assert_eq!(summary.measurement_not_updated, 0);
        assert_eq!(summary.measurement_updated, 6);
        assert_eq!(summary.instance_not_updated, 0);
        assert_eq!(summary.instance_updated, 1);
        assert!(log_msgs.is_empty());

        // test full update
        let mut log_msgs = Vec::new();
        let (summary, _) = update_a2l(
            &mut a2l,
            &debug_data,
            &mut log_msgs,
            UpdateType::Full,
            UpdateMode::Default,
            false,
        );
        assert_eq!(summary.axis_pts_not_updated, 0);
        assert_eq!(summary.axis_pts_updated, 3);
        assert_eq!(summary.blob_not_updated, 0);
        assert_eq!(summary.blob_updated, 2);
        assert_eq!(summary.characteristic_not_updated, 0);
        assert_eq!(summary.characteristic_updated, 6);
        assert_eq!(summary.measurement_not_updated, 0);
        assert_eq!(summary.measurement_updated, 6);
        assert_eq!(summary.instance_not_updated, 0);
        assert_eq!(summary.instance_updated, 1);
        assert!(log_msgs.is_empty());
    }
}
