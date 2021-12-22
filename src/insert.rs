use std::collections::HashMap;
use a2lfile::*;

use crate::dwarf::*;
use crate::datatype::*;
use crate::update::enums;
use regex::Regex;


enum ItemType {
    Measurement(usize),
    Characteristic(usize),
    Instance(usize),
    Blob(usize),
    AxisPts(usize)
}

pub(crate) fn insert_items(
    a2l_file: &mut A2lFile,
    debugdata: &DebugData,
    measurement_symbols: Vec<&str>,
    characteristic_symbols: Vec<&str>
) {
    let module = &mut a2l_file.project.module[0];
    let (name_map, sym_map) = build_maps(&module);

    for measure_sym in measurement_symbols {
        insert_measurement(module, debugdata, measure_sym, &name_map, &sym_map);
    }

    for characteristic_sym in characteristic_symbols {
        insert_characteristic(module, debugdata, characteristic_sym, &name_map, &sym_map);
    }
}


// create a new MEASUREMENT for the given symbol
fn insert_measurement(
    module: &mut Module,
    debugdata: &DebugData,
    measure_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>
) {
    // get info about the symbol from the debug data
    match crate::symbol::find_symbol(measure_sym, debugdata) {
        Ok((address, typeinfo)) => {
            insert_measurement_sym(module, measure_sym, name_map, sym_map, typeinfo, address);
        }
        Err(errmsg) => {
            println!("Symbol {} could not be added: {}", measure_sym, errmsg);
        }
    }
}


fn insert_measurement_sym(
    module: &mut Module,
    measure_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>,
    typeinfo: &TypeInfo,
    address: u64
) -> bool {
    // Abort if a MEASUREMENT for this symbol already exists. Warn if any other reference to the symbol exists
    let item_name = match make_unique_measurement_name(module, sym_map, measure_sym, name_map) {
        Some(value) => value,
        None => return false,
    };

    let datatype = get_a2l_datatype(typeinfo);
    let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
    let mut new_measurement = Measurement::new(
        item_name,
        format!("measurement for symbol {}",
        measure_sym),
        datatype,
        "NO_COMPU_METHOD".to_string(),
        0,
        0f64,
        lower_limit,
        upper_limit
    );
    let mut ecu_address = EcuAddress::new(address as u32);
    ecu_address.get_layout_mut().item_location.0.1 = true;
    new_measurement.ecu_address = Some(ecu_address);
    new_measurement.symbol_link = Some(SymbolLink::new(measure_sym.to_string(), 0));
    if let TypeInfo::Enum{typename, enumerators, ..} = typeinfo {
        new_measurement.conversion = typename.to_owned();
        enums::cond_create_enum_conversion(module, typename, enumerators);
    }
    module.measurement.push(new_measurement);

    true
}


// Add a new CHARACTERISTIC for the given symbol
fn insert_characteristic(
    module: &mut Module,
    debugdata: &DebugData,
    characteristic_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>
) {
    // get info about the symbol from the debug data
    match crate::symbol::find_symbol(characteristic_sym, debugdata) {
        Ok((address, typeinfo)) => {
            insert_characteristic_sym(module, characteristic_sym, name_map, sym_map, typeinfo, address);
        }
        Err(errmsg) => {
            println!("Symbol {} could not be added: {}", characteristic_sym, errmsg);
        }
    }
}


fn insert_characteristic_sym(
    module: &mut Module,
    characteristic_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>,
    typeinfo: &TypeInfo,
    address: u64
) -> bool {
    let item_name = match make_unique_characteristic_name(module, sym_map, characteristic_sym, name_map) {
        Some(value) => value,
        None => return false,
    };
    let datatype = get_a2l_datatype(typeinfo);
    let recordlayout_name = format!("__{}_Z", datatype.to_string());
    let mut new_characteristic = match typeinfo {
        TypeInfo::Class{..} |
        TypeInfo::Union{..} |
        TypeInfo::Struct{..} => {
            // Structs cannot be handled at all in this code. In some cases structs can be used by CHARACTERISTICs,
            // but in that case the struct represents function values together with axis info.
            // Much more information regarding which struct member has which use would be required
            println!("  Don't know how to add a CHARACTERISTIC for a struct. Please add a struct member instead.");
            return false;
        }
        TypeInfo::Array{arraytype, dim, ..} => {
            // an array is turned into a CHARACTERISTIC of type VAL_BLK, and needs a MATRIX_DIM sub-element
            let (lower_limit, upper_limit) = get_type_limits(arraytype, f64::MIN, f64::MAX);
            let mut newitem = Characteristic::new(
                item_name,
                format!("characterisitic for {}", characteristic_sym),
                CharacteristicType::ValBlk,
                address as u32,
                recordlayout_name.to_owned(),
                0f64,
                "NO_COMPU_METHOD".to_string(),
                lower_limit,
                upper_limit
            );
            let mut matrix_dim = MatrixDim::new();
            // dim[0] always exists
            matrix_dim.dim_list.push(dim[0] as u16);
            // for compat with 1.61 and previous, "1" is set as the arry dimension for y and z if dim[1] and dim[2] don't exist
            matrix_dim.dim_list.push(*dim.get(1).unwrap_or_else(|| &1) as u16);
            matrix_dim.dim_list.push(*dim.get(2).unwrap_or_else(|| &1) as u16);
            newitem.matrix_dim = Some(matrix_dim);
            newitem
        }
        TypeInfo::Enum{typename, enumerators, ..} => {
            // CHARACTERISTICs for enums get a COMPU_METHOD and COMPU_VTAB providing translation of values to text
            let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
            enums::cond_create_enum_conversion(module, typename, enumerators);
            Characteristic::new(
                item_name,
                format!("characterisitic for {}", characteristic_sym),
                CharacteristicType::Value,
                address as u32,
                recordlayout_name.to_owned(),
                0f64,
                typename.to_string(),
                lower_limit,
                upper_limit
            )
        }
        _ => {
            // any other data type: create a basic CHARACTERISTIC
            let (lower_limit, upper_limit) = get_type_limits(typeinfo, f64::MIN, f64::MAX);
            Characteristic::new(
                item_name,
                format!("characteristic for {}", characteristic_sym),
                CharacteristicType::Value,
                address as u32,
                recordlayout_name.to_owned(),
                0f64,
                "NO_COMPU_METHOD".to_string(),
                lower_limit,
                upper_limit
            )
        }
    };
    // enable hex mode for the address (item 3 in the CHARACTERISTIC)
    new_characteristic.get_layout_mut().item_location.3.1 = true;

    // create a SYMBOL_LINK
    new_characteristic.symbol_link = Some(SymbolLink::new(characteristic_sym.to_string(), 0));

    // insert the CHARACTERISTIC into the module's list
    module.characteristic.push(new_characteristic);

    // create a RECORD_LAYOUT for the CHARACTERISTIC if it doesn't exist yet
    // the used naming convention (__<type>_Z) matches default naming used by Vector tools
    let mut recordlayout = RecordLayout::new(recordlayout_name.to_owned());
    // set item 0 (name) to use an offset of 0 lines, i.e. no line break after /begin RECORD_LAYOUT
    recordlayout.get_layout_mut().item_location.0 = 0;
    recordlayout.fnc_values = Some(FncValues::new(1, datatype, IndexMode::RowDir, AddrType::Direct));
    // search through all existing record layouts and only add the new one if it doesn't exist yet
    if module.record_layout.iter().find(|&rl| rl.name == recordlayout_name).is_none() {
        module.record_layout.push(recordlayout);
    }

    true
}


fn make_unique_measurement_name(
    module: &Module,
    sym_map: &HashMap<String, ItemType>,
    measure_sym: &str,
    name_map: &HashMap<String, ItemType>
) -> Option<String> {
    let item_name = match sym_map.get(measure_sym) {
        Some(ItemType::Measurement(idx)) => {
            println!("  MEASUREMENT {} already references symbol {}. It will not be added again.",
                module.measurement[*idx].name, measure_sym);
            return None;
        }
        Some(ItemType::Characteristic(idx)) => {
            println!("  CHARACTERISTIC {} already references symbol {}.",
                module.characteristic[*idx].name, measure_sym);
            format!("MEASUREMENT.{}", measure_sym)
        }
        Some(ItemType::Instance(idx)) => {
            println!("  INSTANCE {} already references symbol {}.",
                module.instance[*idx].name, measure_sym);
            format!("MEASUREMENT.{}", measure_sym)
        }
        Some(ItemType::Blob(idx)) => {
            println!("  BLOB {} already references symbol {}.",
                module.blob[*idx].name, measure_sym);
            format!("MEASUREMENT.{}", measure_sym)
        }
        Some(ItemType::AxisPts(idx)) => {
            println!("  AXIS_PTS {} already references symbol {}.",
                module.axis_pts[*idx].name, measure_sym);
            format!("MEASUREMENT.{}", measure_sym)
        }
        None => {
            measure_sym.to_string()
        }
    };
    if let Some(_) = name_map.get(&item_name) {
        println!("  The item name {} is already in use. No MEASUREMENT will be added for symbol {}.", item_name, measure_sym);
        return None;
    }
    Some(item_name)
}


fn make_unique_characteristic_name(
    module: &Module,
    sym_map: &HashMap<String, ItemType>,
    characteristic_sym: &str,
    name_map: &HashMap<String, ItemType>
) -> Option<String> {
    let item_name = match sym_map.get(characteristic_sym) {
        Some(ItemType::Characteristic(idx)) => {
            println!("  CHARACTERISTIC {} already references symbol {}. It will not be added again.",
                module.characteristic[*idx].name, characteristic_sym);
            return None;
        }
        Some(ItemType::Measurement(idx)) => {
            println!("  MEASUREMENT {} already references symbol {}.",
                module.measurement[*idx].name, characteristic_sym);
            format!("CHARACTERISTIC.{}", characteristic_sym)
        }
        Some(ItemType::Instance(idx)) => {
            println!("  INSTANCE {} already references symbol {}.",
                module.instance[*idx].name, characteristic_sym);
            format!("CHARACTERISTIC.{}", characteristic_sym)
        }
        Some(ItemType::Blob(idx)) => {
            println!("  BLOB {} already references symbol {}.",
                module.blob[*idx].name, characteristic_sym);
            format!("CHARACTERISTIC.{}", characteristic_sym)
        }
        Some(ItemType::AxisPts(idx)) => {
            println!("  AXIS_PTS {} already references symbol {}.",
                module.axis_pts[*idx].name, characteristic_sym);
            format!("CHARACTERISTIC.{}", characteristic_sym)
        }
        None => {
            characteristic_sym.to_string()
        }
    };
    if let Some(_) = name_map.get(&item_name) {
        println!("  The item name {} is already in use. No CHARACTERISTIC will be added for symbol {}.", item_name, characteristic_sym);
        return None;
    }
    Some(item_name)
}


fn build_maps(module: &&mut Module) -> (HashMap<String, ItemType>, HashMap<String, ItemType>) {
    let mut name_map = HashMap::<String, ItemType>::new();
    let mut sym_map = HashMap::<String, ItemType>::new();
    for (idx, chr) in module.characteristic.iter().enumerate() {
        name_map.insert(chr.name.to_owned(), ItemType::Characteristic(idx));
        if let Some(sym_link) = &chr.symbol_link {
            sym_map.insert(sym_link.symbol_name.to_owned(), ItemType::Characteristic(idx));
        }
    }
    for (idx, meas) in module.measurement.iter().enumerate() {
        name_map.insert(meas.name.to_owned(), ItemType::Measurement(idx));
        if let Some(sym_link) = &meas.symbol_link {
            sym_map.insert(sym_link.symbol_name.to_owned(), ItemType::Measurement(idx));
        }
    }
    for (idx, inst) in module.instance.iter().enumerate() {
        name_map.insert(inst.name.to_owned(), ItemType::Instance(idx));
        if let Some(sym_link) = &inst.symbol_link {
            sym_map.insert(sym_link.symbol_name.to_owned(), ItemType::Instance(idx));
        }
    }
    for (idx, blob) in module.blob.iter().enumerate() {
        name_map.insert(blob.name.to_owned(), ItemType::Blob(idx));
        if let Some(sym_link) = &blob.symbol_link {
            sym_map.insert(sym_link.symbol_name.to_owned(), ItemType::Blob(idx));
        }
    }
    for (idx, axis_pts) in module.axis_pts.iter().enumerate() {
        name_map.insert(axis_pts.name.to_owned(), ItemType::AxisPts(idx));
        if let Some(sym_link) = &axis_pts.symbol_link {
            sym_map.insert(sym_link.symbol_name.to_owned(), ItemType::AxisPts(idx));
        }
    }

    (name_map, sym_map)
}


pub(crate) fn insert_ranges(
    a2l_file: &mut A2lFile,
    debugdata: &DebugData,
    measurement_ranges: Vec<(u64, u64)>,
    characteristic_ranges: Vec<(u64, u64)>
) {
    let module = &mut a2l_file.project.module[0];
    let (name_map, sym_map) = build_maps(&module);

    for (symbol_name, symbol_type, address) in debugdata.iter() {
        match symbol_type {
            Some(TypeInfo::Array{..}) |
            Some(TypeInfo::Struct{..}) |
            Some(TypeInfo::Union{..}) |
            Some(TypeInfo::Class{..}) => {
                // don't insert complex types directly. Their individual members will be inserted instead
            }
            Some(typeinfo) => {
                // insert the symbol as a measurement if it's address is within any of the given ranges
                for (lower_limit, upper_limit) in &measurement_ranges {
                    if *lower_limit <= address && address < *upper_limit {
                        insert_measurement_sym(module, &symbol_name, &name_map, &sym_map, typeinfo, address);
                    }
                }

                // insert the symbol as a characteristic if it's address is within any of the given ranges
                for (lower_limit, upper_limit) in &characteristic_ranges {
                    if *lower_limit <= address && address < *upper_limit {
                        insert_characteristic_sym(module, &symbol_name, &name_map, &sym_map, typeinfo, address);
                    }
                }
            }
            None => {
                // no type info, can't insert this symbol
            }
        }
    }
}


pub(crate) fn insert_regex(
    a2l_file: &mut A2lFile,
    debugdata: &DebugData,
    measurement_regexes: Vec<&str>,
    characteristic_regexes: Vec<&str>
) {
    // compile the regular expressions
    let mut compiled_meas_re = Vec::new();
    let mut compiled_char_re = Vec::new();
    for expr in measurement_regexes {
        match Regex::new(expr) {
            Ok(compiled_re) => compiled_meas_re.push(compiled_re),
            Err(error) => println!("Invalid regex \"{}\": {}", expr, error)
        }
    }
    for expr in characteristic_regexes {
        match Regex::new(expr) {
            Ok(compiled_re) => compiled_char_re.push(compiled_re),
            Err(error) => println!("Invalid regex \"{}\": {}", expr, error)
        }
    }

    let module = &mut a2l_file.project.module[0];
    let (name_map, sym_map) = build_maps(&module);

    for (symbol_name, symbol_type, address) in debugdata.iter() {
        match symbol_type {
            Some(TypeInfo::Array{..}) |
            Some(TypeInfo::Struct{..}) |
            Some(TypeInfo::Union{..}) |
            Some(TypeInfo::Class{..}) => {
                // don't insert complex types directly. Their individual members will be inserted instead
            }
            Some(typeinfo) => {
                // insert the symbol as a measurement if it's address is within any of the given ranges
                for re in &compiled_meas_re {
                    if re.is_match(&symbol_name) {
                        insert_measurement_sym(module, &symbol_name, &name_map, &sym_map, typeinfo, address);
                    }
                }

                // insert the symbol as a characteristic if it's address is within any of the given ranges
                for re in &compiled_char_re {
                    if re.is_match(&symbol_name) {
                        insert_characteristic_sym(module, &symbol_name, &name_map, &sym_map, typeinfo, address);
                    }
                }
            }
            None => {
                // no type info, can't insert this symbol
            }
        }
    }
}
