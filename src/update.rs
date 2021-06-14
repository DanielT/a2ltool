use std::collections::{HashMap, HashSet};

use super::dwarf::{DebugData, TypeInfo};
use super::ifdata;
use a2lfile::*;


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


#[derive(Debug)]
struct RecordLayoutInfo {
    idxmap: HashMap<String, usize>,
    refcount: Vec<usize>
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


fn update_module_axis_pts(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool, recordlayout_info: &mut RecordLayoutInfo) -> (u32, u32) {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut axis_pts_list = Vec::new();
    let mut axis_pts_updated: u32 = 0;
    let mut axis_pts_not_updated: u32 = 0;

    std::mem::swap(&mut module.axis_pts, &mut axis_pts_list);
    for mut axis_pts in axis_pts_list {
        if let Some(typeinfo) = update_axis_pts_address(&mut axis_pts, debug_data) {
            // the variable used for the axis should be a 1-dimensional array, or a struct containing a 1-dimensional array
            // if the type is a struct, then the AXIS_PTS_X inside the referenced RECORD_LAYOUT tells us which member of the struct to use.
            let member_id = get_axis_pts_x_memberid(module, recordlayout_info, &axis_pts.deposit_record);
            if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {

                match inner_typeinfo {
                    TypeInfo::Array{dim, arraytype, ..} => {
                        // update max_axis_points to match the size of the array
                        if dim.len() >= 1 {
                            axis_pts.max_axis_points = dim[0] as u16;
                        }
                        if let TypeInfo::Enum{typename, ..} = &**arraytype {
                            // an array of enums? it could be done...
                            if axis_pts.conversion == "NO_COMPU_METHOD" {
                                axis_pts.conversion = typename.to_owned();
                            }
                            cond_create_enum_conversion(module, &axis_pts.conversion);
                            enum_convlist.insert(axis_pts.conversion.clone(), arraytype);
                        }
                    }
                    TypeInfo::Enum{..} => {
                        // likely not useful, because what purpose would an axis consisting of a single enum value serve?
                        enum_convlist.insert(axis_pts.conversion.clone(), typeinfo);
                    }
                    _ => {}
                }

                let (ll, ul) = adjust_limits(inner_typeinfo, axis_pts.lower_limit, axis_pts.upper_limit);
                axis_pts.lower_limit = ll;
                axis_pts.upper_limit = ul;
            }

            // update the data type in the referenced RECORD_LAYOUT
            axis_pts.deposit_record = update_record_layout(module, recordlayout_info, &axis_pts.deposit_record, typeinfo);

            // put the updated AXIS_PTS back on the module's list
            module.axis_pts.push(axis_pts);
            axis_pts_updated += 1;
        } else {
            if preserve_unknown {
                axis_pts.address = 0;
                zero_if_data(&mut axis_pts.if_data);
                module.axis_pts.push(axis_pts);
            } else {
                // item is removed implicitly, because it is not added back to the list
                removed_items.insert(axis_pts.name.to_owned());
            }
            axis_pts_not_updated += 1;
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
    update_enum_compu_methods(module, &enum_convlist);
    cleanup_removed_axis_pts(module, &removed_items);

    (axis_pts_updated, axis_pts_not_updated)
}


fn update_module_characteristics(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool, recordlayout_info: &mut RecordLayoutInfo) -> (u32, u32) {
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut removed_items = HashSet::<String>::new();
    let mut characteristic_list = Vec::new();
    let mut characteristic_updated: u32 = 0;
    let mut characteristic_not_updated: u32 = 0;

    // store the max_axis_points of each AXIS_PTS, so that the AXIS_DESCRs inside of CHARACTERISTICS can be updated to match
    let axis_pts_dim: HashMap::<String, u16> = module.axis_pts.iter().map(|item| (item.name.to_owned(), item.max_axis_points)).collect();

    std::mem::swap(&mut module.characteristic, &mut characteristic_list);
    for mut characteristic in characteristic_list {
        if let Some(typeinfo) = update_characteristic_address(&mut characteristic, debug_data) {

            let member_id = get_fnc_values_memberid(module, recordlayout_info, &characteristic.deposit);
            if let Some(inner_typeinfo) = get_inner_type(typeinfo, member_id) {
                if let TypeInfo::Enum{typename, ..} = inner_typeinfo {
                    if characteristic.conversion == "NO_COMPU_METHOD" {
                        characteristic.conversion = typename.to_owned();
                    }
                    cond_create_enum_conversion(module, &characteristic.conversion);
                    enum_convlist.insert(characteristic.conversion.clone(), typeinfo);
                }

                let (ll, ul) = adjust_limits(inner_typeinfo, characteristic.lower_limit, characteristic.upper_limit);
                characteristic.lower_limit = ll;
                characteristic.upper_limit = ul;
            }

            // get the position information for each axis from the associated record layout
            // information for some axes could be missing in the record layout, for eaxmple if an external axis is referenced with AXIS_PTS_REF
            let mut axis_positions = Vec::<Option<u16>>::new();
            if let Some(idx) = recordlayout_info.idxmap.get(&characteristic.deposit) {
                let rl = &module.record_layout[*idx];
                let itemrefs = [&rl.axis_pts_x, &rl.axis_pts_y, &rl.axis_pts_z, &rl.axis_pts_4, &rl.axis_pts_5];
                for itemref in &itemrefs {
                    if let Some(axisinfo) = itemref {
                        // axis information for this axis exists
                        axis_positions.push(Some(axisinfo.position));
                    } else {
                        // no axis information found
                        axis_positions.push(None);
                    }
                }
            }

            // update the max_axis_points of axis descriptions
            for (idx, axis_descr) in characteristic.axis_descr.iter_mut().enumerate() {
                if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref {
                    // external axis, using AXIS_PTS_REF
                    if let Some(max_axis_pts) = axis_pts_dim.get(&axis_pts_ref.axis_points) {
                        axis_descr.max_axis_points = *max_axis_pts;
                    }
                } else if idx <= 5 {
                    // an internal axis, using info from the typeinfo and the record layout
                    if let Some(position) = axis_positions[idx] {
                        if let Some(TypeInfo::Array{dim, ..}) = get_inner_type(typeinfo, position) {
                            axis_descr.max_axis_points = dim[0] as u16;
                        }
                    }
                }
            }

            // update the data type in the referenced RECORD_LAYOUT
            characteristic.deposit = update_record_layout(module, recordlayout_info, &characteristic.deposit, typeinfo);

            module.characteristic.push(characteristic);
            characteristic_updated += 1;
        } else {
            if preserve_unknown {
                characteristic.address = 0;
                zero_if_data(&mut characteristic.if_data);
                module.characteristic.push(characteristic);
            } else {
                // item is removed implicitly, because it is not added back to the list
                removed_items.insert(characteristic.name.to_owned());
            }
            characteristic_not_updated += 1;
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
    update_enum_compu_methods(module, &enum_convlist);
    cleanup_removed_characteristics(module, &removed_items);

    (characteristic_updated, characteristic_not_updated)
}


fn update_module_measurements(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool, use_new_matrix_dim: bool) -> (u32, u32) {
    let mut removed_items = HashSet::<String>::new();
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut measurement_list = Vec::new();
    let mut measurement_updated: u32 = 0;
    let mut measurement_not_updated: u32 = 0;

    std::mem::swap(&mut module.measurement, &mut measurement_list);
    for mut measurement in measurement_list {
        if let Some(typeinfo) = update_measurement_address(&mut measurement, debug_data) {
            if let TypeInfo::Enum{typename, ..} = typeinfo {
                if measurement.conversion == "NO_COMPU_METHOD" {
                    measurement.conversion = typename.to_owned();
                }
                cond_create_enum_conversion(module, &measurement.conversion);
                enum_convlist.insert(measurement.conversion.clone(), typeinfo);
            }

            let (ll, ul) = adjust_limits(typeinfo, measurement.lower_limit, measurement.upper_limit);
            measurement.lower_limit = ll;
            measurement.upper_limit = ul;
            update_matrix_dim(&mut measurement.matrix_dim, typeinfo, use_new_matrix_dim);

            // ARRAY_SIZE is replaced by MATRIX_DIM
            measurement.array_size = None;

            module.measurement.push(measurement);
            measurement_updated += 1;
        } else {
            if preserve_unknown {
                measurement.ecu_address = None;
                zero_if_data(&mut measurement.if_data);
                module.measurement.push(measurement);
            } else {
                // item is removed implicitly, because it is not added back to the list
                removed_items.insert(measurement.name.to_owned());
            }
            measurement_not_updated += 1;
        }
    }


    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
    update_enum_compu_methods(module, &enum_convlist);
    cleanup_removed_measurements(module, &removed_items);

    (measurement_updated, measurement_not_updated)
}


fn update_module_blobs(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool) -> (u32, u32) {
    let mut removed_items = HashSet::<String>::new();
    let mut blob_list = Vec::new();
    let mut blob_updated: u32 = 0;
    let mut blob_not_updated: u32 = 0;
    std::mem::swap(&mut module.blob, &mut blob_list);
    for mut blob in blob_list {
        if let Some(typeinfo) = update_blob_address(&mut blob, debug_data) {
            blob.size = typeinfo.get_size() as u32;
            module.blob.push(blob);
            blob_updated += 1;
        } else {
            if preserve_unknown {
                blob.start_address = 0;
                zero_if_data(&mut blob.if_data);
                module.blob.push(blob);
            } else {
                // item is removed implicitly, because it is not added back to the list
                removed_items.insert(blob.name.to_owned());
            }
            blob_not_updated += 1;
        }
    }
    cleanup_removed_blobs(module, &removed_items);

    (blob_updated, blob_not_updated)
}


fn update_module_instances(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool) -> (u32, u32) {
    let mut removed_items = HashSet::<String>::new();
    let mut instance_list = Vec::new();
    let mut instance_updated: u32 = 0;
    let mut instance_not_updated: u32 = 0;
    std::mem::swap(&mut module.instance, &mut instance_list);
    for mut instance in instance_list {
        if let Some((_typedef_ref, _typeinfo)) = update_instance_address(&mut instance, debug_data) {
            // possible extension: validate the referenced TYPEDEF_x that this INSTANCE is based on by comparing it to typeinfo

            module.instance.push(instance);
            instance_updated += 1;
        } else {
            if preserve_unknown {
                instance.start_address = 0;
                zero_if_data(&mut instance.if_data);
                module.instance.push(instance);
            } else {
                // item is removed implicitly, because it is not added back to the list
                removed_items.insert(instance.name.to_owned());
            }
            instance_not_updated += 1;
        }
    }
    cleanup_removed_instances(module, &removed_items);

    (instance_updated, instance_not_updated)
}


// check if the file version is >= 1.70
fn check_version_1_70(a2l_file: &A2lFile) -> bool {
    if let Some(ver) = &a2l_file.asap2_version {
        ver.version_no > 1 || (ver.version_no == 1 && ver.upgrade_no >= 70)
    } else {
        false
    }
}


// update the address of a MEASUREMENT object
fn update_measurement_address<'a>(measurement: &mut Measurement, debug_data: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&measurement.name, &measurement.symbol_link, &measurement.if_data, debug_data);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut measurement.symbol_link, symbol_name.clone());
        set_measurement_ecu_address(&mut measurement.ecu_address, address);
        measurement.datatype = get_a2l_datatype(symbol_datatype);
        set_measurement_bitmask(&mut measurement.bit_mask, symbol_datatype);
        update_ifdata(&mut measurement.if_data, symbol_name, symbol_datatype, address);

        Some(symbol_datatype)
    } else {
        None
    }
}


// update the MATRIX_DIM of a MEASUREMENT
fn update_matrix_dim(opt_matrix_dim: &mut Option<MatrixDim>, typeinfo: &TypeInfo, new_format: bool) {
    let mut matrix_dim_values = Vec::new();
    let mut cur_typeinfo = typeinfo;
    // compilers can represent multi-dimensional arrays in two different ways:
    // either as nested arrays, each with one dimension, or as one array with multiple dimensions
    while let TypeInfo::Array{dim, arraytype, ..} = cur_typeinfo {
        for val in dim {
            matrix_dim_values.push(*val as u16);
        }
        cur_typeinfo = &**arraytype;
    }

    if matrix_dim_values.len() == 0 {
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
        let mut matrix_dim = opt_matrix_dim.get_or_insert(MatrixDim::new());
        matrix_dim.dim_list = matrix_dim_values;
    }
}


// update the address of a CHARACTERISTIC
fn update_characteristic_address<'a>(characteristic: &mut Characteristic, debug_data: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&characteristic.name, &characteristic.symbol_link, &characteristic.if_data, debug_data);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut characteristic.symbol_link, symbol_name.clone());
        characteristic.address = address as u32;
        set_measurement_bitmask(&mut characteristic.bit_mask, symbol_datatype);
        update_ifdata(&mut characteristic.if_data, symbol_name, symbol_datatype, address);

        Some(symbol_datatype)
    } else {
        None
    }
}


// update the address of an AXIS_PTS object
fn update_axis_pts_address<'a>(axis_pts: &mut AxisPts, debug_data: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&axis_pts.name, &axis_pts.symbol_link, &axis_pts.if_data, debug_data);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut axis_pts.symbol_link, symbol_name.clone());
        axis_pts.address = address as u32;
        update_ifdata(&mut axis_pts.if_data, symbol_name, symbol_datatype, address);

        Some(symbol_datatype)
    } else {
        None
    }
}


// update the address of a BLOB object
fn update_blob_address<'a>(blob: &mut Blob, debug_data: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&blob.name, &blob.symbol_link, &blob.if_data, debug_data);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut blob.symbol_link, symbol_name.clone());
        blob.start_address = address as u32;
        update_ifdata(&mut blob.if_data, symbol_name, symbol_datatype, address);

        Some(symbol_datatype)
    } else {
        None
    }
}


// update the address of an INSTANCE object
fn update_instance_address<'a>(instance: &mut Instance, debug_data: &'a DebugData) -> Option<(String, &'a TypeInfo)> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&instance.name, &instance.symbol_link, &instance.if_data, debug_data);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut instance.symbol_link, symbol_name.clone());
        instance.start_address = address as u32;
        update_ifdata(&mut instance.if_data, symbol_name, symbol_datatype, address);

        Some((instance.type_ref.to_owned(), symbol_datatype))
    } else {
        None
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


fn get_axis_pts_x_memberid(module: &Module, recordlayout_info: &RecordLayoutInfo, recordlayout_name: &str) -> u16 {
    let mut memberid = 1;
    if let Some(rl_idx) = recordlayout_info.idxmap.get(recordlayout_name) {
        if let Some(axis_pts_x) = &module.record_layout[*rl_idx].axis_pts_x {
            memberid = axis_pts_x.position;
            // the position identifiers inside of a RECORD_LAYOUT start at 1, but I have some files that contain zero
            if memberid == 0 {
                memberid = 1;
            }
        }
    }
    memberid
}


fn get_fnc_values_memberid(module: &Module, recordlayout_info: &RecordLayoutInfo, recordlayout_name: &str) -> u16 {
    let mut memberid = 1;
    if let Some(rl_idx) = recordlayout_info.idxmap.get(recordlayout_name) {
        if let Some(fnc_values) = &module.record_layout[*rl_idx].fnc_values {
            memberid = fnc_values.position;
            // the position identifiers inside of a RECORD_LAYOUT start at 1, but I have some files that contain zero
            if memberid == 0 {
                memberid = 1;
            }
        }
    }
    memberid
}


fn get_inner_type(typeinfo: &TypeInfo, memberid: u16) -> Option<&TypeInfo> {
    // memberid is (supposed to) start counting at 1, but array indexing is based on 0
    let id = if memberid > 0 {
        (memberid - 1) as usize
    } else {
        0
    };

    match typeinfo {
        TypeInfo::Struct { members, ..} => {
            let mut membervec: Vec<(&TypeInfo, u64)> = members.values().map(|(membertype, offset)| (membertype, *offset)). collect();
            membervec.sort_by(|(_, offset_a), (_, offset_b)| offset_a.cmp(offset_b));
            if id < membervec.len() {
                Some(membervec[id].0)
            } else {
                None
            }
        }
        _ => {
            if id == 0 {
                Some(typeinfo)
            } else {
                None
            }
        }
    }
}


// map the datatypes from the elf_info to a2l datatypes
// the only really relevant cases are for the integer, floating point and enum types
// all other types cannot be sensibly measured / calibrated anyway
fn get_a2l_datatype(datatype: &TypeInfo) -> DataType {
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


// check if there is a CANAPE_EXT in the IF_DATA vec and update it if it exists
fn update_ifdata(ifdata_vec: &mut Vec<IfData>, symbol_name: String, datatype: &TypeInfo, address: u64) {
    for ifdata in ifdata_vec {
        if let Some(mut decoded_ifdata) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = &mut decoded_ifdata.canape_ext {
                if let Some (link_map) = &mut canape_ext.link_map {
                    link_map.address = address as i32;
                    link_map.symbol_name = symbol_name.clone();
                    match datatype {
                        TypeInfo::Uint8 => {
                            link_map.datatype = 0x87;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Uint16 => {
                            link_map.datatype = 0x8f;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Uint32 => {
                            link_map.datatype = 0x9f;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Uint64 => {
                            link_map.datatype = 0xbf;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Sint8 => {
                            link_map.datatype = 0xc7;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Sint16 => {
                            link_map.datatype = 0xcf;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Sint32 => {
                            link_map.datatype = 0xdf;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Sint64 => {
                            link_map.datatype = 0xff;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Float => {
                            link_map.datatype = 0x01;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Double => {
                            link_map.datatype = 0x02;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Enum { size, .. } => {
                            match *size {
                                1 => link_map.datatype = 0x87,
                                2 => link_map.datatype = 0x8f,
                                4 => link_map.datatype = 0x8f,
                                8 => link_map.datatype = 0xbf,
                                _ => link_map.datatype = 0,
                            }
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 1;
                        }
                        TypeInfo::Bitfield { basetype, bit_offset, bit_size } => {
                            let signed: u16 = match **basetype {
                                TypeInfo::Sint8 |
                                TypeInfo::Sint16 |
                                TypeInfo::Sint32 |
                                TypeInfo::Sint64 => 0x40,
                                _ => 0x0
                            };
                            link_map.datatype = 0x80 | signed | (bit_size - 1);
                            link_map.bit_offset = *bit_offset;
                            link_map.datatype_valid = 1;
                        }
                        _ => {
                            link_map.datatype = 0;
                            link_map.bit_offset = 0;
                            link_map.datatype_valid = 0;
                        }
                    }

                    decoded_ifdata.store_to_ifdata(ifdata);
                }
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
                    dp_blob.address_extension = 0;
                    dp_blob.base_address = address as u32;

                    match datatype {
                        TypeInfo::Uint8 |
                        TypeInfo::Sint8 => dp_blob.size = 1,
                        TypeInfo::Uint16 |
                        TypeInfo::Sint16 => dp_blob.size = 2,
                        TypeInfo::Float |
                        TypeInfo::Uint32 |
                        TypeInfo::Sint32 => dp_blob.size = 4,
                        TypeInfo::Double |
                        TypeInfo::Uint64 |
                        TypeInfo::Sint64 => dp_blob.size = 8,
                        TypeInfo::Enum {size, ..} => dp_blob.size = *size as u32,
                        _ => {
                            // size is not set because we don't know
                            // for example if the datatype is Struct, then the record_layout must be taken into the calculation
                            // rather than do that, the size is left unchanged, since it will most often already be correct
                        }
                    }

                    decoded_ifdata.store_to_ifdata(ifdata);
                }
            }
        } else {
            println!("failed to decode ifdata: {:#?}", ifdata);
        }
    }
}


// zero out incorrect information in IF_DATA for MEASUREMENTs / CHARACTERISTICs / AXIS_PTS that were not found during update
fn zero_if_data(ifdata_vec: &mut Vec<IfData>) {
    for ifdata in ifdata_vec {
        if let Some(mut decoded_ifdata) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = &mut decoded_ifdata.canape_ext {
                if let Some (link_map) = &mut canape_ext.link_map {
                    // remove address and data type information, but keep the symbol name
                    link_map.address = 0;
                    link_map.datatype = 0;
                    link_map.bit_offset = 0;
                    link_map.datatype_valid = 0;

                    decoded_ifdata.store_to_ifdata(ifdata);
                }
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
                    dp_blob.address_extension = 0;
                    dp_blob.base_address = 0;
                }
            }
        }
    }
}


// create a COMPU_METHOD and a COMPU_VTAB for the typename of an enum
fn cond_create_enum_conversion(module: &mut Module, typename: &str) {
    let compu_method_find = module.compu_method.iter().find(|item| item.name == typename);
    if compu_method_find.is_none() {
        let mut new_compu_method = CompuMethod::new(
            typename.to_string(),
            format!("Conversion table for enum {}", typename),
            ConversionType::TabVerb,
            "%.4".to_string(),
            "".to_string()
        );
        new_compu_method.compu_tab_ref = Some(CompuTabRef::new(typename.to_string()));
        module.compu_method.push(new_compu_method);

        let compu_vtab_find = module.compu_vtab.iter().find(|item| item.name == typename);
        let compu_vtab_range_find = module.compu_vtab_range.iter().find(|item| item.name == typename);

        if compu_vtab_find.is_none() && compu_vtab_range_find.is_none() {
            module.compu_vtab.push(CompuVtab::new(
                typename.to_string(),
                format!("Conversion table for enum {}", typename),
                ConversionType::TabVerb,
                0 // will be updated by update_enum_compu_methods, which will also add the actual enum values
            ));
        }
    }
}


// every MEASUREMENT, CHARACTERISTIC and AXIS_PTS object can reference a COMPU_METHOD which describes the conversion of values
// in some cases the the COMPU_METHOS in turn references a COMPU_VTAB to provide number to string mapping and display named values
// These COMPU_VTAB objects are typically based on an enum in the original software.
// By following the chain from the MEASUREMENT (etc.), we know what type is associated with the COMPU_VTAB and can add or
// remove enumerators to match the software
fn update_enum_compu_methods(module: &mut Module, enum_convlist: &HashMap<String, &TypeInfo>) {
    // enum_convlist: a table of COMPU_METHODS and the associated types (filtered to contain only enums)
    // if the list is empty then there is nothing to do
    if enum_convlist.len() == 0 {
        return;
    }

    // follow the chain of objects and build a list of COMPU_TAB_REF references with their associated enum types
    let mut enum_compu_tab = HashMap::new();
    for compu_method in &module.compu_method {
        if let Some(typeinfo) = enum_convlist.get(&compu_method.name) {
            if let Some(compu_tab) = &compu_method.compu_tab_ref {
                enum_compu_tab.insert(compu_tab.conversion_table.clone(), *typeinfo);
            }
        }
    }

    // check all COMPU_VTABs in the module to see if we know of an associated enum type
    for compu_vtab in &mut module.compu_vtab {
        if let Some(TypeInfo::Enum{enumerators, ..}) = enum_compu_tab.get(&compu_vtab.name) {
            // TabVerb is the only permitted conversion type for a compu_vtab
            compu_vtab.conversion_type = ConversionType::TabVerb;

            // if compu_vtab has more entries than the enum, delete the extras
            while compu_vtab.value_pairs.len() > enumerators.len() {
                compu_vtab.value_pairs.pop();
            }
            // if compu_vtab has less entries than the enum, append some dummy entries
            while compu_vtab.value_pairs.len() < enumerators.len() {
                compu_vtab.value_pairs.push(ValuePairsStruct::new(0f64, "dummy".to_string()));
            }
            compu_vtab.number_value_pairs = enumerators.len() as u16;

            // overwrite the current compu_vtab entries with the values from the enum
            for (idx, (name, value)) in enumerators.iter().enumerate() {
                compu_vtab.value_pairs[idx].in_val = *value as f64;
                compu_vtab.value_pairs[idx].out_val = name.clone();
            }
        }
    }

    // do the same for COMPU_VTAB_RANGE, because the enum could also be stored as a COMPU_VTAB_RANGE where min = max for all entries
    for compu_vtab_range in &mut module.compu_vtab_range {
        if let Some(TypeInfo::Enum{enumerators, ..}) = enum_compu_tab.get(&compu_vtab_range.name) {
            // if compu_vtab_range has more entries than the enum, delete the extras
            while compu_vtab_range.value_triples.len() > enumerators.len() {
                compu_vtab_range.value_triples.pop();
            }
            // if compu_vtab_range has less entries than the enum, append some dummy entries
            while compu_vtab_range.value_triples.len() < enumerators.len() {
                compu_vtab_range.value_triples.push(ValueTriplesStruct::new(0f64, 0f64, "dummy".to_string()));
            }
            compu_vtab_range.number_value_triples = enumerators.len() as u16;

            // overwrite the current compu_vtab_range entries with the values from the enum
            for (idx, (name, value)) in enumerators.iter().enumerate() {
                compu_vtab_range.value_triples[idx].in_val_min = *value as f64;
                compu_vtab_range.value_triples[idx].in_val_max = *value as f64;
                compu_vtab_range.value_triples[idx].out_val = name.clone();
            }
        }
    }
}


// when update runs without preserve, AXIS_PTS be removed from the module
// AXIS_PTS are only referenced through CHARACTERISTIC > AXIS_DESCR > AXIS_PTS_REF
fn cleanup_removed_axis_pts(module: &mut Module, removed_items: &HashSet<String>) {
    if removed_items.len() == 0 {
        return;
    }

    for characteristic in &mut module.characteristic {
        for axis_descr in &mut characteristic.axis_descr {
            if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref {
                if removed_items.get(&axis_pts_ref.axis_points).is_some() {
                    axis_descr.axis_pts_ref = None;
                }
            }
        }
    }

    for typedef_characteristic in &mut module.typedef_characteristic {
        for axis_descr in &mut typedef_characteristic.axis_descr {
            if let Some(axis_pts_ref) = &axis_descr.axis_pts_ref {
                if removed_items.get(&axis_pts_ref.axis_points).is_some() {
                    axis_descr.axis_pts_ref = None;
                }
            }
        }
    }
}


// when update runs without preserve, CHARACTERISTICs could be removed from the module
// these items should also be removed from the identifier lists in GROUPs and FUNCTIONs
fn cleanup_removed_characteristics(module: &mut Module, removed_items: &HashSet<String>) {
    if removed_items.len() == 0 {
        return;
    }

    for group in &mut module.group {
        if let Some(ref_characteristic) = &mut group.ref_characteristic {
            cleanup_item_list(&mut ref_characteristic.identifier_list, &removed_items);
            if ref_characteristic.identifier_list.len() == 0 {
                group.ref_characteristic = None;
            }
        }
    }

    for function in &mut module.function {
        if let Some(def_characteristic) = &mut function.def_characteristic {
            cleanup_item_list(&mut def_characteristic.identifier_list, &removed_items);
            if def_characteristic.identifier_list.len() == 0 {
                function.def_characteristic = None;
            }
        }
        if let Some(ref_characteristic) = &mut function.ref_characteristic {
            cleanup_item_list(&mut ref_characteristic.identifier_list, &removed_items);
            if ref_characteristic.identifier_list.len() == 0 {
                function.ref_characteristic = None;
            }
        }
    }
}

// when update runs without preserve some MEASUREMENTs could be removed
// these items should also be removed from the identifier lists in GROUPs, FUNCTIONs, etc
fn cleanup_removed_measurements(module: &mut Module, removed_items: &HashSet<String>) {
    if removed_items.len() == 0 {
        return;
    }

    for group in &mut module.group {
        if let Some(ref_measurement) = &mut group.ref_measurement {
            cleanup_item_list(&mut ref_measurement.identifier_list, &removed_items);
            if ref_measurement.identifier_list.len() == 0 {
                group.ref_measurement = None;
            }
        }
    }

    for function in &mut module.function {
        if let Some(in_measurement) = &mut function.in_measurement {
            cleanup_item_list(&mut in_measurement.identifier_list, &removed_items);
            if in_measurement.identifier_list.len() == 0 {
                function.in_measurement = None;
            }
        }
        if let Some(loc_measurement) = &mut function.loc_measurement {
            cleanup_item_list(&mut loc_measurement.identifier_list, &removed_items);
            if loc_measurement.identifier_list.len() == 0 {
                function.loc_measurement = None;
            }
        }
        if let Some(out_measurement) = &mut function.out_measurement {
            cleanup_item_list(&mut out_measurement.identifier_list, &removed_items);
            if out_measurement.identifier_list.len() == 0 {
                function.out_measurement = None;
            }
        }
    }

    for characteristic in &mut module.characteristic {
        for axis_descr in &mut characteristic.axis_descr {
            if removed_items.get(&axis_descr.input_quantity).is_some() {
                axis_descr.input_quantity = "NO_INPUT_QUANTITY".to_string();
            }
        }

        if let Some(comparison_quantity) = &characteristic.comparison_quantity {
            if removed_items.get(&comparison_quantity.name).is_some() {
                characteristic.comparison_quantity = None;
            }
        }
    }

    for typedef_characteristic in &mut module.typedef_characteristic {
        for axis_descr in &mut typedef_characteristic.axis_descr {
            if removed_items.get(&axis_descr.input_quantity).is_some() {
                axis_descr.input_quantity = "NO_INPUT_QUANTITY".to_string();
            }
        }
    }

    for axis_pts in &mut module.axis_pts {
        if removed_items.get(&axis_pts.input_quantity).is_some() {
            axis_pts.input_quantity = "NO_INPUT_QUANTITY".to_string();
        }
    }

    for typedef_axis in &mut module.typedef_axis {
        if removed_items.get(&typedef_axis.input_quantity).is_some() {
            typedef_axis.input_quantity = "NO_INPUT_QUANTITY".to_string();
        }
    }
}


fn cleanup_removed_blobs(module: &mut Module, removed_items: &HashSet<String>) {
    for transformer in &mut module.transformer {
        if let Some(transformer_in_objects) = &mut transformer.transformer_in_objects {
            cleanup_item_list(&mut transformer_in_objects.identifier_list, &removed_items);
        }
        if let Some(transformer_out_objects) = &mut transformer.transformer_out_objects {
            cleanup_item_list(&mut transformer_out_objects.identifier_list, &removed_items);
        }
    }

    // can these be in a GROUP?
}


fn cleanup_removed_instances(module: &mut Module, removed_items: &HashSet<String>) {
    // INSTANCEs can take the place of AXIS_PTS, BLOBs, CHARACTERISTICs or MEASUREMENTs, depending on which kind of TYPEDEF the instance is based on
    cleanup_removed_axis_pts(module, removed_items);
    cleanup_removed_blobs(module, removed_items);
    cleanup_removed_characteristics(module, removed_items);
    cleanup_removed_measurements(module, removed_items);
}


fn cleanup_item_list(item_list: &mut Vec<String>, removed_items: &HashSet<String>) {
    let mut new_list = Vec::<String>::new();
    std::mem::swap(item_list, &mut new_list);

    for item in new_list {
        if removed_items.get(&item).is_none() {
            item_list.push(item);
        }
    }
}


fn update_record_layout(module: &mut Module, recordlayout_info: &mut RecordLayoutInfo, name: &str, typeinfo: &TypeInfo) -> String {
    if let Some(idx_ref) = recordlayout_info.idxmap.get(name) {
        let idx = *idx_ref;
        let mut new_reclayout = module.record_layout[idx].clone();

        // FNC_VALUES - required in record layouts used by a CHARACTERISTIC
        if let Some(fnc_values) = &mut new_reclayout.fnc_values {
            if let Some(itemtype) = get_inner_type(typeinfo, fnc_values.position) {
                let new_datatype = get_a2l_datatype(itemtype);
                if new_datatype != fnc_values.datatype {
                    // try to update the name based on the datatype, e.g. __UBYTE_S to __ULONG_S
                    new_reclayout.name = new_reclayout.name.replacen(&fnc_values.datatype.to_string(), &new_datatype.to_string(), 1);
                    fnc_values.datatype = new_datatype;
                }
            }
        }

        // AXIS_PTS_X - required in record layouts used by an AXIS_PTS, optional for CHARACTERISTIC
        if let Some(axis_pts_x) = &mut new_reclayout.axis_pts_x {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_x.position) {
                axis_pts_x.datatype = get_a2l_datatype(itemtype);
                if let TypeInfo::Array {dim, ..} = itemtype {
                    // FIX_NO_AXIS_PTS_X
                    if let Some(fix_no_axis_pts_x) = &mut new_reclayout.fix_no_axis_pts_x {
                        fix_no_axis_pts_x.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_X
        if let Some(no_axis_pts_x) = &mut new_reclayout.no_axis_pts_x {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_x.position) {
                no_axis_pts_x.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_Y
        if let Some(axis_pts_y) = &mut new_reclayout.axis_pts_y {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_y.position) {
                axis_pts_y.datatype = get_a2l_datatype(itemtype);
                if let TypeInfo::Array {dim, ..} = itemtype {
                    // FIX_NO_AXIS_PTS_Y
                    if let Some(fix_no_axis_pts_y) = &mut new_reclayout.fix_no_axis_pts_y {
                        fix_no_axis_pts_y.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_Y
        if let Some(no_axis_pts_y) = &mut new_reclayout.no_axis_pts_y {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_y.position) {
                no_axis_pts_y.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_Z
        if let Some(axis_pts_z) = &mut new_reclayout.axis_pts_z {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_z.position) {
                axis_pts_z.datatype = get_a2l_datatype(itemtype);
                if let TypeInfo::Array {dim, ..} = itemtype {
                    // FIX_NO_AXIS_PTS_Z
                    if let Some(fix_no_axis_pts_z) = &mut new_reclayout.fix_no_axis_pts_z {
                        fix_no_axis_pts_z.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_Z
        if let Some(no_axis_pts_z) = &mut new_reclayout.no_axis_pts_z {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_z.position) {
                no_axis_pts_z.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_4
        if let Some(axis_pts_4) = &mut new_reclayout.axis_pts_4 {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_4.position) {
                axis_pts_4.datatype = get_a2l_datatype(itemtype);
                if let TypeInfo::Array {dim, ..} = itemtype {
                    // FIX_NO_AXIS_PTS_4
                    if let Some(fix_no_axis_pts_4) = &mut new_reclayout.fix_no_axis_pts_4 {
                        fix_no_axis_pts_4.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_4
        if let Some(no_axis_pts_4) = &mut new_reclayout.no_axis_pts_4 {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_4.position) {
                no_axis_pts_4.datatype = get_a2l_datatype(itemtype);
            }
        }

        // AXIS_PTS_5
        if let Some(axis_pts_5) = &mut new_reclayout.axis_pts_5 {
            if let Some(itemtype) = get_inner_type(typeinfo, axis_pts_5.position) {
                axis_pts_5.datatype = get_a2l_datatype(itemtype);
                if let TypeInfo::Array {dim, ..} = itemtype {
                    // FIX_NO_AXIS_PTS_5
                    if let Some(fix_no_axis_pts_5) = &mut new_reclayout.fix_no_axis_pts_5 {
                        fix_no_axis_pts_5.number_of_axis_points = dim[0] as u16;
                    }
                }
            }
        }
        // NO_AXIS_PTS_5
        if let Some(no_axis_pts_5) = &mut new_reclayout.no_axis_pts_5 {
            if let Some(itemtype) = get_inner_type(typeinfo, no_axis_pts_5.position) {
                no_axis_pts_5.datatype = get_a2l_datatype(itemtype);
            }
        }

        if module.record_layout[idx] == new_reclayout {
            // no changes were made, return the name of the existing record layout and don't use the cloned version
            name.to_owned()
        } else {
            // try to find an existing record_layout with these parameters
            if let Some((existing_idx, existing_reclayout)) = module.record_layout.iter().enumerate().find(
                |&(_, item)| compare_rl_content(&new_reclayout, item)
            ) {
                // there already is a record layout with these parameters
                recordlayout_info.refcount[idx] -= 1;
                recordlayout_info.refcount[existing_idx] += 1;
                existing_reclayout.name.to_owned()
            } else {
                if recordlayout_info.refcount[idx] == 1 {
                    // the original record layout only has one reference; that means we can simply overwrite it with the modified data
                    if module.record_layout[idx].name != new_reclayout.name {
                        // the name has changed, so idxmap needs to be fixed
                        recordlayout_info.idxmap.remove(&module.record_layout[idx].name);
                        recordlayout_info.idxmap.insert(new_reclayout.name.to_owned(), idx);
                    }
                    module.record_layout[idx] = new_reclayout;
                    module.record_layout[idx].name.to_owned()
                } else {
                    // the original record layout has multiple users, so it's reference count decreases by one and the new record layout is added to the list
                    recordlayout_info.refcount[idx] -= 1;
                    new_reclayout.name = make_unique_reclayout_name(new_reclayout.name, recordlayout_info);
                    recordlayout_info.refcount.push(1);
                    recordlayout_info.idxmap.insert(new_reclayout.name.to_owned(), module.record_layout.len());
                    module.record_layout.push(new_reclayout);
                    module.record_layout.last().unwrap().name.to_owned()
                }
            }
        }
    } else {
        // the record layout name used in the CHARACTERISTIC does not refer to a valid record layout
        // this can only be fixed manually, so continue using the invalid name here
        name.to_owned()
    }
}


fn make_unique_reclayout_name(initial_name: String, recordlayout_info: &RecordLayoutInfo) -> String {
    if recordlayout_info.idxmap.get(&initial_name).is_some() {
        // the record layout name already exists. Now we want to extend the name to make it unique
        // e.g. BASIC_RECORD_LAYOUT to BASIC_RECORD_LAYOUT_UPDATED
        // if there are multiple BASIC_RECORD_LAYOUT_UPDATED we want to continue with BASIC_RECORD_LAYOUT_UPDATED.2, .3 , etc
        // instead of BASIC_RECORD_LAYOUT_UPDATED_UPDATED
        let basename =
        if let Some(pos) = initial_name.find("_UPDATED") {
            let end_of_updated = pos + "_UPDATED".len();
            if end_of_updated == initial_name.len() || initial_name[end_of_updated..].starts_with(".") {
                initial_name[..end_of_updated].to_string()
            } else {
                format!("{}_UPDATED", initial_name)
            }
        } else {
            format!("{}_UPDATED", initial_name)
        };
        let mut outname = basename.clone();
        let mut counter = 1;
        while recordlayout_info.idxmap.get(&outname).is_some() {
            counter += 1;
            outname = format!("{}.{}", basename, counter);
        }
        outname
    } else {
        initial_name
    }
}


// compare two record layouts, but without considering the name
fn compare_rl_content(a: &RecordLayout, b: &RecordLayout) -> bool {
    a.alignment_byte == b.alignment_byte &&
    a.alignment_float16_ieee == b.alignment_float16_ieee &&
    a.alignment_float32_ieee == b.alignment_float32_ieee &&
    a.alignment_float64_ieee == b.alignment_float64_ieee &&
    a.alignment_int64 == b.alignment_int64 &&
    a.alignment_long == b.alignment_long &&
    a.alignment_word == b.alignment_word &&
    a.axis_pts_x == b.axis_pts_x &&
    a.axis_pts_y == b.axis_pts_y &&
    a.axis_pts_z == b.axis_pts_z &&
    a.axis_pts_4 == b.axis_pts_4 &&
    a.axis_pts_5 == b.axis_pts_5 &&
    a.axis_rescale_x == b.axis_rescale_x &&
    a.axis_rescale_y == b.axis_rescale_y &&
    a.axis_rescale_z == b.axis_rescale_z &&
    a.axis_rescale_4 == b.axis_rescale_4 &&
    a.axis_rescale_5 == b.axis_rescale_5 &&
    a.dist_op_x == b.dist_op_x &&
    a.dist_op_y == b.dist_op_y &&
    a.dist_op_z == b.dist_op_z &&
    a.dist_op_4 == b.dist_op_4 &&
    a.dist_op_5 == b.dist_op_5 &&
    a.fix_no_axis_pts_x == b.fix_no_axis_pts_x &&
    a.fix_no_axis_pts_y == b.fix_no_axis_pts_y &&
    a.fix_no_axis_pts_z == b.fix_no_axis_pts_z &&
    a.fix_no_axis_pts_4 == b.fix_no_axis_pts_4 &&
    a.fix_no_axis_pts_5 == b.fix_no_axis_pts_5 &&
    a.fnc_values == b.fnc_values &&
    a.identification == b.identification &&
    a.no_axis_pts_x == b.no_axis_pts_x &&
    a.no_axis_pts_y == b.no_axis_pts_y &&
    a.no_axis_pts_z == b.no_axis_pts_z &&
    a.no_axis_pts_4 == b.no_axis_pts_4 &&
    a.no_axis_pts_5 == b.no_axis_pts_5 &&
    a.no_rescale_x == b.no_rescale_x &&
    a.no_rescale_y == b.no_rescale_y &&
    a.no_rescale_z == b.no_rescale_z &&
    a.no_rescale_4 == b.no_rescale_4 &&
    a.no_rescale_5 == b.no_rescale_5 &&
    a.offset_x == b.offset_x &&
    a.offset_y == b.offset_y &&
    a.offset_z == b.offset_z &&
    a.offset_4 == b.offset_4 &&
    a.offset_5 == b.offset_5 &&
    a.reserved == b.reserved &&
    a.rip_addr_w == b.rip_addr_w &&
    a.rip_addr_x == b.rip_addr_x &&
    a.rip_addr_y == b.rip_addr_y &&
    a.rip_addr_z == b.rip_addr_z &&
    a.rip_addr_4 == b.rip_addr_4 &&
    a.rip_addr_5 == b.rip_addr_5 &&
    a.shift_op_x == b.shift_op_x &&
    a.shift_op_y == b.shift_op_y &&
    a.shift_op_z == b.shift_op_z &&
    a.shift_op_4 == b.shift_op_4 &&
    a.shift_op_5 == b.shift_op_5 &&
    a.src_addr_x == b.src_addr_x &&
    a.src_addr_y == b.src_addr_y &&
    a.src_addr_z == b.src_addr_z &&
    a.src_addr_4 == b.src_addr_4 &&
    a.src_addr_5 == b.src_addr_5 &&
    a.static_address_offsets == b.static_address_offsets &&
    a.static_record_layout == b.static_record_layout
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


impl RecordLayoutInfo {
    pub(crate) fn build(module: &Module) -> Self {
        let idxmap: HashMap<String, usize> = module.record_layout.iter()
            .enumerate()
            .map(|(idx, rl)| (rl.name.to_owned(), idx))
            .collect();
        let mut refcount = Vec::<usize>::with_capacity(module.record_layout.len());
        refcount.resize(module.record_layout.len(), 0);
        for ap in &module.axis_pts {
            if let Some(idx) = idxmap.get(&ap.deposit_record) {
                refcount[*idx] += 1;
            }
        }
        for chr in &module.characteristic {
            if let Some(idx) = idxmap.get(&chr.deposit) {
                refcount[*idx] += 1;
            }
        }

        Self {
            idxmap,
            refcount
        }
    }
}