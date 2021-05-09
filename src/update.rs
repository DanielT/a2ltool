use std::collections::HashMap;

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


// perform a destructive address update: any object that cannot be updated will be discarded
pub(crate) fn update_addresses(a2l_file: &mut A2lFile, elf_info: &DebugData, preserve_unknown: bool) -> UpdateSumary {
    let mut summary = UpdateSumary::new();
    for module in &mut a2l_file.project.module {
        let mut enum_convlist = HashMap::<String, &TypeInfo>::new();

        // update all MEASUREMENTs
        let mut measurement_list = Vec::new();
        std::mem::swap(&mut module.measurement, &mut measurement_list);
        for mut measurement in measurement_list {
            if let Some(typeinfo) = update_measurement_address(&mut measurement, elf_info) {
                if let TypeInfo::Enum{..} = typeinfo {
                    enum_convlist.insert(measurement.conversion.clone(), typeinfo);
                }

                module.measurement.push(measurement);
                summary.measurement_updated += 1;
            } else {
                if preserve_unknown {
                    measurement.ecu_address = None;
                    zero_if_data(&mut measurement.if_data);
                    module.measurement.push(measurement);
                }
                summary.measurement_not_updated += 1;
            }
        }

        // update all CHARACTERISTICs
        let mut characteristic_list = Vec::new();
        std::mem::swap(&mut module.characteristic, &mut characteristic_list);
        for mut characteristic in characteristic_list {
            if let Some(typeinfo) = update_characteristic_address(&mut characteristic, elf_info) {
                if let TypeInfo::Enum{..} = typeinfo {
                    enum_convlist.insert(characteristic.conversion.clone(), typeinfo);
                }

                module.characteristic.push(characteristic);
                summary.characteristic_updated += 1;
            } else {
                if preserve_unknown {
                    characteristic.address = 0;
                    zero_if_data(&mut characteristic.if_data);
                    module.characteristic.push(characteristic);
                }
                summary.characteristic_not_updated += 1;
            }
        }

        // update all AXIS_PTS
        let mut axis_pts_list = Vec::new();
        std::mem::swap(&mut module.axis_pts, &mut axis_pts_list);
        for mut axis_pts in axis_pts_list {
            if let Some(typeinfo) = update_axis_pts_address(&mut axis_pts, elf_info) {
                if let TypeInfo::Enum{..} = typeinfo {
                    enum_convlist.insert(axis_pts.conversion.clone(), typeinfo);
                }

                module.axis_pts.push(axis_pts);
                summary.axis_pts_updated += 1;
            } else {
                if preserve_unknown {
                    axis_pts.address = 0;
                    zero_if_data(&mut axis_pts.if_data);
                    module.axis_pts.push(axis_pts);
                }
                summary.axis_pts_not_updated += 1;
            }
        }

        // update all BLOBs
        let mut blob_list = Vec::new();
        std::mem::swap(&mut module.blob, &mut blob_list);
        for mut blob in blob_list {
            if let Some(typeinfo) = update_blob_address(&mut blob, elf_info) {
                blob.size = typeinfo.get_size() as u32;
                module.blob.push(blob);
                summary.blob_updated += 1;
            } else {
                if preserve_unknown {
                    blob.start_address = 0;
                    zero_if_data(&mut blob.if_data);
                    module.blob.push(blob);
                }
                summary.blob_not_updated += 1;
            }
        }

        // update all INSTANCEs
        let mut instance_list = Vec::new();
        std::mem::swap(&mut module.instance, &mut instance_list);
        for mut instance in instance_list {
            if let Some((_typedef_ref, _typeinfo)) = update_instance_address(&mut instance, elf_info) {
                // possible extension: validate the referenced TYPEDEF_x that this INSTANCE is based on by comparing it to typeinfo

                module.instance.push(instance);
                summary.instance_updated += 1;
            } else {
                if preserve_unknown {
                    instance.start_address = 0;
                    zero_if_data(&mut instance.if_data);
                    module.instance.push(instance);
                }
                summary.instance_not_updated += 1;
            }
        }


        // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs etc.
        update_enum_compu_methods(module, &enum_convlist);
    }

    summary
}


// update the address of a MEASUREMENT object
fn update_measurement_address<'a>(measurement: &mut Measurement, elf_info: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&measurement.name, &measurement.symbol_link, &measurement.if_data, elf_info);

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


// update the address of a CHARACTERISTIC
fn update_characteristic_address<'a>(characteristic: &mut Characteristic, elf_info: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&characteristic.name, &characteristic.symbol_link, &characteristic.if_data, elf_info);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut characteristic.symbol_link, symbol_name.clone());
        characteristic.address = address as u32;
        set_measurement_bitmask(&mut characteristic.bit_mask, symbol_datatype);
        update_ifdata(&mut characteristic.if_data, symbol_name, symbol_datatype, address);

        // todo? should probably modify characteristic.deposit if the data type changes

        Some(symbol_datatype)
    } else {
        None
    }
}


// update the address of an AXIS_PTS object
fn update_axis_pts_address<'a>(axis_pts: &mut AxisPts, elf_info: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&axis_pts.name, &axis_pts.symbol_link, &axis_pts.if_data, elf_info);

    if let Some((address, symbol_datatype)) = symbol_info {
        // make sure a valid SYMBOL_LINK exists
        set_symbol_link(&mut axis_pts.symbol_link, symbol_name.clone());
        axis_pts.address = address as u32;
        update_ifdata(&mut axis_pts.if_data, symbol_name, symbol_datatype, address);

        // todo? should probably modify axis_pts.deposit_record if the data type changes

        Some(symbol_datatype)
    } else {
        None
    }
}


// update the address of a BLOB object
fn update_blob_address<'a>(blob: &mut Blob, elf_info: &'a DebugData) -> Option<&'a TypeInfo> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&blob.name, &blob.symbol_link, &blob.if_data, elf_info);

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
fn update_instance_address<'a>(instance: &mut Instance, elf_info: &'a DebugData) -> Option<(String, &'a TypeInfo)> {
    let (symbol_info, symbol_name) =
        get_symbol_info(&instance.name, &instance.symbol_link, &instance.if_data, elf_info);

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
    elf_info: &'a DebugData
) -> (Option<(u64, &'a TypeInfo)>, String) {
    let mut symbol_info = None;
    let mut symbol_name = "".to_string();

    // preferred: get symbol information from a SYMBOL_LINK attribute
    if let Some(symbol_link) = opt_symbol_link {
        symbol_name = symbol_link.symbol_name.clone();
        symbol_info = find_symbol(&symbol_name, elf_info);
    }

    // second option: get symbol information from a CANAPE_EXT block inside of IF_DATA.
    // The content of IF_DATA can be different for each tool vendor, but the blocks used
    // by the Vector tools are understood by some other software.
    if symbol_info.is_none() {
        if let Some(ifdata_symbol_name) = get_symbol_name_from_ifdata(ifdata_vec) {
            symbol_name = ifdata_symbol_name;
            symbol_info = find_symbol(&symbol_name, elf_info);
        }
    }

    // If there is no SYMBOL_LINK and no (usable) IF_DATA, hen maybe the object name is also the symol name
    if symbol_info.is_none() && opt_symbol_link.is_none() {
        symbol_name = name.to_string();
        symbol_info = find_symbol(&symbol_name, elf_info);
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
fn find_symbol<'a>(varname: &str, elf_info: &'a DebugData) -> Option<(u64, &'a TypeInfo)> {
    // split the a2l symbol name: e.g. "motortune.param._0_" -> ["motortune", "param", "_0_"]
    let components: Vec<&str> = varname.split('.').collect();
    // the first component of the symbol name is the name of the global variable.
    if let Some(varinfo) = elf_info.variables.get(components[0]) {
        // we also need the type in order to resolve struct members, etc.
        if let Some(vartype) = elf_info.types.get(&varinfo.typeref) {
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
                            link_map.datatype = 0x02; // ???
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
            }
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
            }
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
