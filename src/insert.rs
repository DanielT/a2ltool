use a2lfile::{
    A2lFile, A2lObject, AddrType, BitMask, Characteristic, CharacteristicType, EcuAddress, FncValues, Group,
    IndexMode, MatrixDim, Measurement, Module, RecordLayout, RefCharacteristic, RefMeasurement,
    Root, SymbolLink,
};
use std::collections::HashMap;

use crate::datatype::{get_a2l_datatype, get_type_limits};
use crate::dwarf::{DebugData, TypeInfo};
use crate::update::enums;
use regex::Regex;

enum ItemType {
    Measurement(usize),
    Characteristic(usize),
    Instance,
    Blob,
    AxisPts,
}

pub(crate) fn insert_items(
    a2l_file: &mut A2lFile,
    debugdata: &DebugData,
    measurement_symbols: Vec<&str>,
    characteristic_symbols: Vec<&str>,
    target_group: Option<&str>,
    log_msgs: &mut Vec<String>,
) {
    let module = &mut a2l_file.project.module[0];
    let (mut name_map, sym_map) = build_maps(&module);
    let mut characteristic_list = vec![];
    let mut measurement_list = vec![];

    for measure_sym in measurement_symbols {
        match insert_measurement(module, debugdata, measure_sym, &name_map, &sym_map) {
            Ok(measure_name) => {
                log_msgs.push(format!("Inserted MEASUREMENT {measure_name}"));
                name_map.insert(
                    measure_name.clone(),
                    ItemType::Measurement(module.measurement.len() - 1),
                );
                measurement_list.push(measure_name);
            }
            Err(errmsg) => {
                log_msgs.push(format!("Insert skipped: {errmsg}"));
            }
        }
    }

    for characteristic_sym in characteristic_symbols {
        match insert_characteristic(module, debugdata, characteristic_sym, &name_map, &sym_map) {
            Ok(characteristic_name) => {
                log_msgs.push(format!("Inserted CHARACTERISTIC {characteristic_name}"));
                name_map.insert(
                    characteristic_name.clone(),
                    ItemType::Characteristic(module.characteristic.len() - 1),
                );
                characteristic_list.push(characteristic_name);
            }
            Err(errmsg) => {
                log_msgs.push(format!("Insert skipped: {errmsg}"));
            }
        }
    }

    if let Some(group_name) = target_group {
        create_or_update_group(module, group_name, characteristic_list, measurement_list);
    }
}

// create a new MEASUREMENT for the given symbol
fn insert_measurement(
    module: &mut Module,
    debugdata: &DebugData,
    measure_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>,
) -> Result<String, String> {
    // get info about the symbol from the debug data
    match crate::symbol::find_symbol(measure_sym, debugdata) {
        Ok((true_name, address, typeinfo)) => insert_measurement_sym(
            module,
            measure_sym,
            true_name,
            name_map,
            sym_map,
            typeinfo,
            address,
        ),
        Err(errmsg) => Err(format!("Symbol {measure_sym} could not be added: {errmsg}")),
    }
}

fn insert_measurement_sym(
    module: &mut Module,
    measure_sym: &str,
    true_name: String,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>,
    typeinfo: &TypeInfo,
    address: u64,
) -> Result<String, String> {
    // Abort if a MEASUREMENT for this symbol already exists. Warn if any other reference to the symbol exists
    let item_name = make_unique_measurement_name(module, sym_map, measure_sym, name_map)?;

    let datatype = get_a2l_datatype(typeinfo);
    let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
    let mut new_measurement = Measurement::new(
        item_name.clone(),
        format!("measurement for symbol {measure_sym}"),
        datatype,
        "NO_COMPU_METHOD".to_string(),
        0,
        0f64,
        lower_limit,
        upper_limit,
    );
    // create an ECU_ADDRESS attribute, and set it to hex display mode
    let mut ecu_address = EcuAddress::new(address as u32);
    ecu_address.get_layout_mut().item_location.0 .1 = true;
    new_measurement.ecu_address = Some(ecu_address);
    // create a SYMBOL_LINK attribute
    new_measurement.symbol_link = Some(SymbolLink::new(true_name, 0));
    match typeinfo {
        TypeInfo::Enum {
            typename,
            enumerators,
            ..
        } => {
            // create a conversion table for enums
            new_measurement.conversion = typename.to_owned();
            enums::cond_create_enum_conversion(module, typename, enumerators);
        }
        TypeInfo::Bitfield {
            bit_offset,
            bit_size,
            ..
        } => {
            // create a BIT_MASK for bitfields
            let bitmask = ((1 << bit_size) - 1) << bit_offset;
            let mut bm = BitMask::new(bitmask);
            bm.get_layout_mut().item_location.0.1 = true;
            new_measurement.bit_mask = Some(bm);
        }
        _ => {}
    }
    module.measurement.push(new_measurement);

    Ok(item_name)
}

// Add a new CHARACTERISTIC for the given symbol
fn insert_characteristic(
    module: &mut Module,
    debugdata: &DebugData,
    characteristic_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>,
) -> Result<String, String> {
    // get info about the symbol from the debug data
    match crate::symbol::find_symbol(characteristic_sym, debugdata) {
        Ok((true_name, address, typeinfo)) => insert_characteristic_sym(
            module,
            characteristic_sym,
            true_name,
            name_map,
            sym_map,
            typeinfo,
            address,
        ),
        Err(errmsg) => Err(format!(
            "Symbol {characteristic_sym} could not be added: {errmsg}"
        )),
    }
}

fn insert_characteristic_sym(
    module: &mut Module,
    characteristic_sym: &str,
    true_name: String,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>,
    typeinfo: &TypeInfo,
    address: u64,
) -> Result<String, String> {
    let item_name = make_unique_characteristic_name(module, sym_map, characteristic_sym, name_map)?;

    let datatype = get_a2l_datatype(typeinfo);
    let recordlayout_name = format!("__{datatype}_Z");
    let mut new_characteristic = match typeinfo {
        TypeInfo::Class { .. } | TypeInfo::Union { .. } | TypeInfo::Struct { .. } => {
            // Structs cannot be handled at all in this code. In some cases structs can be used by CHARACTERISTICs,
            // but in that case the struct represents function values together with axis info.
            // Much more information regarding which struct member has which use would be required
            return Err("Don't know how to add a CHARACTERISTIC for a struct. Please add a struct member instead.".to_string());
        }
        TypeInfo::Array { arraytype, dim, .. } => {
            // an array is turned into a CHARACTERISTIC of type VAL_BLK, and needs a MATRIX_DIM sub-element
            let (lower_limit, upper_limit) = get_type_limits(arraytype, f64::MIN, f64::MAX);
            let mut newitem = Characteristic::new(
                item_name.clone(),
                format!("characteristic for {characteristic_sym}"),
                CharacteristicType::ValBlk,
                address as u32,
                recordlayout_name.clone(),
                0f64,
                "NO_COMPU_METHOD".to_string(),
                lower_limit,
                upper_limit,
            );
            let mut matrix_dim = MatrixDim::new();
            // dim[0] always exists
            matrix_dim.dim_list.push(dim[0] as u16);
            // for compat with 1.61 and previous, "1" is set as the arry dimension for y and z if dim[1] and dim[2] don't exist
            matrix_dim.dim_list.push(*dim.get(1).unwrap_or(&1) as u16);
            matrix_dim.dim_list.push(*dim.get(2).unwrap_or(&1) as u16);
            newitem.matrix_dim = Some(matrix_dim);
            newitem
        }
        TypeInfo::Enum {
            typename,
            enumerators,
            ..
        } => {
            // CHARACTERISTICs for enums get a COMPU_METHOD and COMPU_VTAB providing translation of values to text
            let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
            enums::cond_create_enum_conversion(module, typename, enumerators);
            Characteristic::new(
                item_name.clone(),
                format!("characteristic for {characteristic_sym}"),
                CharacteristicType::Value,
                address as u32,
                recordlayout_name.clone(),
                0f64,
                typename.to_string(),
                lower_limit,
                upper_limit,
            )
        }
        TypeInfo::Bitfield { bit_offset, bit_size, .. } => {
            let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
            let mut new_characteristic = Characteristic::new(
                item_name.clone(),
                format!("characteristic for {characteristic_sym}"),
                CharacteristicType::Value,
                address as u32,
                recordlayout_name.clone(),
                0f64,
                "NO_COMPU_METHOD".to_string(),
                lower_limit,
                upper_limit,
            );
            // create a BIT_MASK
            let bitmask = ((1 << bit_size) - 1) << bit_offset;
            let mut bm = BitMask::new(bitmask);
            bm.get_layout_mut().item_location.0.1 = true;
            new_characteristic.bit_mask = Some(bm);
            new_characteristic
        }
        _ => {
            // any other data type: create a basic CHARACTERISTIC
            let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
            Characteristic::new(
                item_name.clone(),
                format!("characteristic for {characteristic_sym}"),
                CharacteristicType::Value,
                address as u32,
                recordlayout_name.clone(),
                0f64,
                "NO_COMPU_METHOD".to_string(),
                lower_limit,
                upper_limit,
            )
        }
    };
    // enable hex mode for the address (item 3 in the CHARACTERISTIC)
    new_characteristic.get_layout_mut().item_location.3 .1 = true;

    // create a SYMBOL_LINK
    new_characteristic.symbol_link = Some(SymbolLink::new(true_name, 0));

    // insert the CHARACTERISTIC into the module's list
    module.characteristic.push(new_characteristic);

    // create a RECORD_LAYOUT for the CHARACTERISTIC if it doesn't exist yet
    // the used naming convention (__<type>_Z) matches default naming used by Vector tools
    let mut recordlayout = RecordLayout::new(recordlayout_name.clone());
    // set item 0 (name) to use an offset of 0 lines, i.e. no line break after /begin RECORD_LAYOUT
    recordlayout.get_layout_mut().item_location.0 = 0;
    recordlayout.fnc_values = Some(FncValues::new(
        1,
        datatype,
        IndexMode::RowDir,
        AddrType::Direct,
    ));
    // search through all existing record layouts and only add the new one if it doesn't exist yet
    if !module
        .record_layout
        .iter()
        .any(|rl| rl.name == recordlayout_name)
    {
        module.record_layout.push(recordlayout);
    }

    Ok(item_name)
}

fn make_unique_measurement_name(
    module: &Module,
    sym_map: &HashMap<String, ItemType>,
    measure_sym: &str,
    name_map: &HashMap<String, ItemType>,
) -> Result<String, String> {
    // ideally the item name is the symbol name.
    // if the symbol is a demangled c++ symbol, then it might contain a "::", e.g. namespace::variable
    let cleaned_sym = measure_sym.replace("::", "__");

    // If an object of a different type already has this name, add the prefix "CHARACTERISTIC."
    let item_name = match sym_map.get(&cleaned_sym) {
        Some(ItemType::Measurement(idx)) => {
            return Err(format!(
                "MEASUREMENT {} already references symbol {}.",
                module.measurement[*idx].name, measure_sym
            ))
        }
        Some(
            ItemType::Characteristic(_)
            | ItemType::Instance
            | ItemType::Blob
            | ItemType::AxisPts,
        ) => {
            format!("MEASUREMENT.{cleaned_sym}")
        }
        None => cleaned_sym,
    };
    // fail if the name still isn't unique
    if name_map.get(&item_name).is_some() {
        return Err(format!("MEASUREMENT {item_name} already exists."));
    }
    Ok(item_name)
}

fn make_unique_characteristic_name(
    module: &Module,
    sym_map: &HashMap<String, ItemType>,
    characteristic_sym: &str,
    name_map: &HashMap<String, ItemType>,
) -> Result<String, String> {
    // ideally the item name is the symbol name.
    // if the symbol is a demangled c++ symbol, then it might contain a "::", e.g. namespace::variable
    let cleaned_sym = characteristic_sym.replace("::", "__");

    // If an object of a different type already has this name, add the prefix "CHARACTERISTIC."
    let item_name = match sym_map.get(&cleaned_sym) {
        Some(ItemType::Characteristic(idx)) => {
            return Err(format!(
                "CHARACTERISTIC {} already references symbol {}.",
                module.characteristic[*idx].name, characteristic_sym
            ))
        }
        Some(
            ItemType::Measurement(_)
            | ItemType::Instance
            | ItemType::Blob
            | ItemType::AxisPts,
        ) => {
            format!("CHARACTERISTIC.{cleaned_sym}")
        }
        None => cleaned_sym,
    };
    // fail if the name still isn't unique
    if name_map.get(&item_name).is_some() {
        return Err(format!("CHARACTERISTIC {item_name} already exists."));
    }
    Ok(item_name)
}

fn build_maps(module: &&mut Module) -> (HashMap<String, ItemType>, HashMap<String, ItemType>) {
    let mut name_map = HashMap::<String, ItemType>::new();
    let mut sym_map = HashMap::<String, ItemType>::new();
    for (idx, chr) in module.characteristic.iter().enumerate() {
        name_map.insert(chr.name.clone(), ItemType::Characteristic(idx));
        if let Some(sym_link) = &chr.symbol_link {
            sym_map.insert(sym_link.symbol_name.clone(), ItemType::Characteristic(idx));
        }
    }
    for (idx, meas) in module.measurement.iter().enumerate() {
        name_map.insert(meas.name.clone(), ItemType::Measurement(idx));
        if let Some(sym_link) = &meas.symbol_link {
            sym_map.insert(sym_link.symbol_name.clone(), ItemType::Measurement(idx));
        }
    }
    for inst in &module.instance {
        name_map.insert(inst.name.clone(), ItemType::Instance);
        if let Some(sym_link) = &inst.symbol_link {
            sym_map.insert(sym_link.symbol_name.clone(), ItemType::Instance);
        }
    }
    for blob in &module.blob {
        name_map.insert(blob.name.clone(), ItemType::Blob);
        if let Some(sym_link) = &blob.symbol_link {
            sym_map.insert(sym_link.symbol_name.clone(), ItemType::Blob);
        }
    }
    for axis_pts in &module.axis_pts {
        name_map.insert(axis_pts.name.clone(), ItemType::AxisPts);
        if let Some(sym_link) = &axis_pts.symbol_link {
            sym_map.insert(sym_link.symbol_name.clone(), ItemType::AxisPts);
        }
    }

    (name_map, sym_map)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_many(
    a2l_file: &mut A2lFile,
    debugdata: &DebugData,
    measurement_ranges: &[(u64, u64)],
    characteristic_ranges: &[(u64, u64)],
    measurement_regexes: Vec<&str>,
    characteristic_regexes: Vec<&str>,
    target_group: Option<&str>,
    log_msgs: &mut Vec<String>,
) {
    // compile the regular expressions
    let mut compiled_meas_re = Vec::new();
    let mut compiled_char_re = Vec::new();
    for expr in measurement_regexes {
        match Regex::new(expr) {
            Ok(compiled_re) => compiled_meas_re.push(compiled_re),
            Err(error) => println!("Invalid regex \"{expr}\": {error}"),
        }
    }
    for expr in characteristic_regexes {
        match Regex::new(expr) {
            Ok(compiled_re) => compiled_char_re.push(compiled_re),
            Err(error) => println!("Invalid regex \"{expr}\": {error}"),
        }
    }
    let minor_ver = a2l_file.asap2_version.as_ref().map_or(50, |v| v.upgrade_no);
    let use_new_arrays = minor_ver >= 70;
    let module = &mut a2l_file.project.module[0];
    let (name_map, sym_map) = build_maps(&module);
    let mut insert_meas_count = 0u32;
    let mut insert_chara_count = 0u32;
    let mut characteristic_list = vec![];
    let mut measurement_list = vec![];

    for (symbol_name, symbol_type, address) in debugdata.iter(use_new_arrays) {
        match symbol_type {
            Some(
                TypeInfo::Array { .. }
                | TypeInfo::Struct { .. }
                | TypeInfo::Union { .. }
                | TypeInfo::Class { .. },
            ) => {
                // don't insert complex types directly. Their individual members will be inserted instead
            }
            _ => {
                // get the type of the symbol, or default to uint8 if no type could be loaded for this symbol
                let typeinfo = symbol_type.unwrap_or(&TypeInfo::Uint8);

                // insert if the address is inside a given range, or if a regex matches the symbol name
                if is_insert_requested(address, &symbol_name, measurement_ranges, &compiled_meas_re)
                {
                    match insert_measurement_sym(
                        module,
                        &symbol_name,
                        symbol_name.clone(),
                        &name_map,
                        &sym_map,
                        typeinfo,
                        address,
                    ) {
                        Ok(measurement_name) => {
                            log_msgs.push(format!(
                                "Inserted MEASUREMENT {measurement_name} (0x{address:08x})"
                            ));
                            measurement_list.push(measurement_name);
                            insert_meas_count += 1;
                        }
                        Err(errmsg) => {
                            log_msgs.push(format!("Skipped: {errmsg}"));
                        }
                    }
                }

                // insert if the address is inside a given range, or if a regex matches the symbol name
                if is_insert_requested(
                    address,
                    &symbol_name,
                    characteristic_ranges,
                    &compiled_char_re,
                ) {
                    match insert_characteristic_sym(
                        module,
                        &symbol_name,
                        symbol_name.clone(),
                        &name_map,
                        &sym_map,
                        typeinfo,
                        address,
                    ) {
                        Ok(characteristic_name) => {
                            log_msgs.push(format!(
                                "Inserted CHARACTERISTIC {characteristic_name} (0x{address:08x})"
                            ));
                            characteristic_list.push(characteristic_name);
                            insert_chara_count += 1;
                        }
                        Err(errmsg) => {
                            log_msgs.push(format!("Skipped: {errmsg}"));
                        }
                    }
                }
            }
        }
    }

    if let Some(group_name) = target_group {
        create_or_update_group(module, group_name, characteristic_list, measurement_list);
    }

    if insert_meas_count > 0 {
        log_msgs.push(format!("Inserted {insert_meas_count} MEASUREMENTs"));
    }
    if insert_chara_count > 0 {
        log_msgs.push(format!("Inserted {insert_chara_count} CHARACTERISTICs"));
    }
}

fn is_insert_requested(
    address: u64,
    symbol_name: &str,
    addr_ranges: &[(u64, u64)],
    name_regexes: &[Regex],
) -> bool {
    // insert the symbol if its address is within any of the given ranges
    addr_ranges
        .iter()
        .any(|(lower, upper)| *lower <= address && address < *upper)
    // alternatively insert the symbol if its name is matched by any regex
    || name_regexes
        .iter()
        .any(|re| re.is_match(symbol_name))
}

fn create_or_update_group(
    module: &mut Module,
    group_name: &str,
    characteristic_list: Vec<String>,
    measurement_list: Vec<String>,
) {
    // try to find an existing group with the given name
    let existing_group = module.group.iter_mut().find(|grp| grp.name == group_name);

    let group: &mut Group = if let Some(grp) = existing_group {
        grp
    } else {
        // create a new group
        let mut group = Group::new(group_name.to_string(), String::new());
        // the group is not a sub-group of some other group, so it gets the ROOT attribute
        group.root = Some(Root::new());
        module.group.push(group);
        let len = module.group.len();
        &mut module.group[len - 1]
    };

    // add all characteristics to the REF_CHARACTERISTIC block in the group
    if !characteristic_list.is_empty() {
        if group.ref_characteristic.is_none() {
            group.ref_characteristic = Some(RefCharacteristic::new());
        }
        if let Some(ref_characteristic) = &mut group.ref_characteristic {
            for new_characteristic in characteristic_list {
                ref_characteristic.identifier_list.push(new_characteristic);
            }
        }
    }

    // add all measurements to the REF_MEASUREMENT block in the group
    if !measurement_list.is_empty() {
        if group.ref_measurement.is_none() {
            group.ref_measurement = Some(RefMeasurement::new());
        }
        if let Some(ref_measurement) = &mut group.ref_measurement {
            for new_measurement in measurement_list {
                ref_measurement.identifier_list.push(new_measurement);
            }
        }
    }
}
