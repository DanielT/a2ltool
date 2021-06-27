use std::collections::HashMap;
use a2lfile::*;

use crate::dwarf::*;
use crate::datatype::*;
use crate::update::enums;


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
    let varinfo = if let Some(varinfo) = debugdata.variables.get(measure_sym) {
        varinfo
    } else {
        println!("Symbol {} was not found in the elf file. It cannot be added.", measure_sym);
        return;
    };

    let typeinfo = if let Some(typeinfo) = debugdata.types.get(&varinfo.typeref) {
        typeinfo
    } else {
        println!("Symbol {} exists in the elf file, but the associated type info could not be loaded. It cannot be added.", measure_sym);
        return;
    };

    // Abort if a MEASUREMENT for this symbol already exists. Warn if any other reference to the symbol exists
    let item_name = match sym_map.get(measure_sym) {
        Some(ItemType::Measurement(idx)) => {
            println!("  MEASUREMENT {} already references symbol {}. It will not be added again.", module.measurement[*idx].name, measure_sym);
            return;
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

    // the item name must be unique
    if let Some(_) = name_map.get(&item_name) {
        println!("  The item name {} is already in use. No MEASUREMENT will be added for symbol {}.", item_name, measure_sym);
        return;
    }

    // create the MEASUREMENT
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
    // The measurement stores the address in the optional ECU_ADDRESS element
    let mut ecu_address = EcuAddress::new(varinfo.address as u32);
    // enable hex mode for the ecu address
    ecu_address.get_layout_mut().item_location.0.1 = true;
    new_measurement.ecu_address = Some(ecu_address);    
    new_measurement.symbol_link = Some(SymbolLink::new(measure_sym.to_string(), 0));
    if let TypeInfo::Enum{typename, enumerators, ..} = typeinfo {
        new_measurement.conversion = typename.to_owned();
        enums::cond_create_enum_conversion(module, typename, enumerators);
    }

    module.measurement.push(new_measurement);
}


// Add a new CHARACTERISTIC for the given symbol
fn insert_characteristic(
    module: &mut Module,
    debugdata: &DebugData,
    characteristic_sym: &str,
    name_map: &HashMap<String, ItemType>,
    sym_map: &HashMap<String, ItemType>
) {
    let varinfo = if let Some(varinfo) = debugdata.variables.get(characteristic_sym) {
        varinfo
    } else {
        println!("Symbol {} was not found in the elf file. It cannot be added.", characteristic_sym);
        return;
    };

    let typeinfo = if let Some(typeinfo) = debugdata.types.get(&varinfo.typeref) {
        typeinfo
    } else {
        println!("Symbol {} exists in the elf file, but the associated type info could not be loaded. It cannot be added.", characteristic_sym);
        return;
    };

    let item_name = match sym_map.get(characteristic_sym) {
        Some(ItemType::Characteristic(idx)) => {
            println!("  CHARACTERISTIC {} already references symbol {}. It will not be added again.", module.characteristic[*idx].name, characteristic_sym);
            return;
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
        return;
    }

    // create a new CHARACTERISTIC
    // there are some different variations, based on the type of the symbol
    let datatype = get_a2l_datatype(typeinfo);
    let recordlayout_name = format!("__{}_Z", datatype.to_string());
    let mut new_characteristic = match typeinfo {
        TypeInfo::Class{..} |
        TypeInfo::Struct{..} => {
            // structs cannot be handled at all in this code. In some cases structs can be used by CHARACTERISTICs,
            // but in that case the struct represents function values together with axis info.
            // much more information regarding which struct member has which use would be required
            println!("  Don't know how to add a CHARACTERISTIC for a struct. Please add a struct member instead.");
            return;
        }
        TypeInfo::Array{arraytype, dim, ..} => {
            // an array is turned into a CHARACTERISTIC of type VAL_BLK, and needs a MATRIX_DIM sub-element
            let (lower_limit, upper_limit) = get_type_limits(arraytype, f64::MIN, f64::MAX);
            let mut newitem = Characteristic::new(
                item_name,
                format!("characterisitic for {}", characteristic_sym),
                CharacteristicType::ValBlk,
                varinfo.address as u32,
                recordlayout_name.to_owned(),
                f64::MAX,
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
                varinfo.address as u32,
                recordlayout_name.to_owned(),
                f64::MAX,
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
                format!("characterisitic for {}", characteristic_sym),
                CharacteristicType::Value,
                varinfo.address as u32,
                recordlayout_name.to_owned(),
                f64::MAX,
                "NO_COMPU_METHOD".to_string(),
                lower_limit,
                upper_limit
            )
        }
    };
    // enable hex mode for the address
    new_characteristic.get_layout_mut().item_location.3.1 = true;
    new_characteristic.symbol_link = Some(SymbolLink::new(characteristic_sym.to_string(), 0));
    module.characteristic.push(new_characteristic);

    // create a RECORD_LAYOUT for the CHARACTERISTIC if it doesn't exist yet
    // the used naming convention (__<type>_Z) matches default naming used by Vector tools
    let mut recordlayout = RecordLayout::new(recordlayout_name.to_owned());
    recordlayout.get_layout_mut().item_location.0 = 0; // item 0 (name) has an offset of 0 lines, i.e. no line break after /begin RECORD_LAYOUT
    recordlayout.fnc_values = Some(FncValues::new(1, datatype, IndexMode::RowDir, AddrType::Direct));
    // search through all existing record layouts and only add the new one if it doesn't exist yet
    if module.record_layout.iter().find(|&rl| rl.name == recordlayout_name).is_none() {
        module.record_layout.push(recordlayout);
    }
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
