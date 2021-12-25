use std::collections::HashMap;
use std::collections::HashSet;
use a2lfile::*;
use crate::dwarf::*;

use super::enums::*;
use super::ifdata_update::*;
use super::*;


pub(crate) fn update_module_measurements(
    module: &mut Module,
    debug_data: &DebugData,
    log_msgs: &mut Vec<String>,
    preserve_unknown: bool,
    use_new_matrix_dim: bool
) -> (u32, u32) {
    let mut removed_items = HashSet::<String>::new();
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut measurement_list = Vec::new();
    let mut measurement_updated: u32 = 0;
    let mut measurement_not_updated: u32 = 0;

    std::mem::swap(&mut module.measurement, &mut measurement_list);
    for mut measurement in measurement_list {
        if measurement.var_virtual.is_none() {
            // only MEASUREMENTS that are not VIRTUAL can be updated
            match update_measurement_address(&mut measurement, debug_data) {
                Ok(typeinfo) => {
                    // update all the information instide a MEASUREMENT
                    update_measurement_information(module, &mut measurement, typeinfo, &mut enum_convlist, use_new_matrix_dim);

                    module.measurement.push(measurement);
                    measurement_updated += 1;
                }
                Err(errmsgs) => {
                    log_update_errors(log_msgs, errmsgs, "MEASUREMENT", measurement.get_line());

                    if preserve_unknown {
                        measurement.ecu_address = None;
                        zero_if_data(&mut measurement.if_data);
                        module.measurement.push(measurement);
                    } else {
                        // item is removed implicitly, because it is not added back to the list
                        // but we need to track the name of the removed item so that references to it can be deleted
                        removed_items.insert(measurement.name.to_owned());
                    }
                    measurement_not_updated += 1;
                }
            }
        } else {
            // VIRTUAL MEASUREMENTS don't need an address
            module.measurement.push(measurement);
        }
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs
    update_enum_compu_methods(module, &enum_convlist);
    cleanup_removed_measurements(module, &removed_items);

    (measurement_updated, measurement_not_updated)
}


// update datatype, limits and dimension of a MEASURMENT
fn update_measurement_information<'enumlist, 'typeinfo: 'enumlist>(
    module: &mut Module,
    measurement: &mut Measurement,
    typeinfo: &'typeinfo TypeInfo,
    enum_convlist: &'enumlist mut HashMap<String, &'typeinfo TypeInfo>,
    use_new_matrix_dim: bool
) {
    if let TypeInfo::Enum{typename, enumerators, ..} = typeinfo {
        if measurement.conversion == "NO_COMPU_METHOD" {
            measurement.conversion = typename.to_owned();
        }
        cond_create_enum_conversion(module, &measurement.conversion, enumerators);
        enum_convlist.insert(measurement.conversion.clone(), typeinfo);
    }

    let (ll, ul) = adjust_limits(typeinfo, measurement.lower_limit, measurement.upper_limit);
    measurement.lower_limit = ll;
    measurement.upper_limit = ul;

    update_matrix_dim(&mut measurement.matrix_dim, typeinfo, use_new_matrix_dim);
    measurement.array_size = None;
}


// update the address of a MEASUREMENT object
fn update_measurement_address<'a>(
    measurement: &mut Measurement,
    debug_data: &'a DebugData
) -> Result<&'a TypeInfo, Vec<String>> {
    match get_symbol_info(
        &measurement.name,
        &measurement.symbol_link,
        &measurement.if_data,
        debug_data
    ) {
        Ok((address, symbol_datatype, symbol_name)) => {
            // make sure a valid SYMBOL_LINK exists
            set_symbol_link(&mut measurement.symbol_link, symbol_name.clone());
            set_measurement_ecu_address(&mut measurement.ecu_address, address);
            measurement.datatype = get_a2l_datatype(symbol_datatype);
            set_measurement_bitmask(&mut measurement.bit_mask, symbol_datatype);
            update_ifdata(&mut measurement.if_data, symbol_name, symbol_datatype, address);

            Ok(symbol_datatype)
        }
        Err(errmsgs) => Err(errmsgs)
    }
}


// update the MATRIX_DIM of a MEASUREMENT
fn update_matrix_dim(
    opt_matrix_dim: &mut Option<MatrixDim>,
    typeinfo: &TypeInfo,
    new_format: bool
) {
    let mut matrix_dim_values = Vec::new();
    let mut cur_typeinfo = typeinfo;
    // compilers can represent multi-dimensional arrays in two different ways:
    // either as nested arrays, each with one dimension, or as one array with multiple dimensions
    while let TypeInfo::Array { dim, arraytype, .. } = cur_typeinfo {
        for val in dim {
            matrix_dim_values.push(*val as u16);
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
        let mut matrix_dim = opt_matrix_dim.get_or_insert(MatrixDim::new());
        matrix_dim.dim_list = matrix_dim_values;
    }
}


// when update runs without preserve some MEASUREMENTs could be removed
// these items should also be removed from the identifier lists in GROUPs, FUNCTIONs, etc
pub(crate) fn cleanup_removed_measurements(module: &mut Module, removed_items: &HashSet<String>) {
    if removed_items.is_empty() {
        return;
    }

    for group in &mut module.group {
        if let Some(ref_measurement) = &mut group.ref_measurement {
            cleanup_item_list(&mut ref_measurement.identifier_list, removed_items);
            if ref_measurement.identifier_list.is_empty() {
                group.ref_measurement = None;
            }
        }
    }

    for function in &mut module.function {
        if let Some(in_measurement) = &mut function.in_measurement {
            cleanup_item_list(&mut in_measurement.identifier_list, removed_items);
            if in_measurement.identifier_list.is_empty() {
                function.in_measurement = None;
            }
        }
        if let Some(loc_measurement) = &mut function.loc_measurement {
            cleanup_item_list(&mut loc_measurement.identifier_list, removed_items);
            if loc_measurement.identifier_list.is_empty() {
                function.loc_measurement = None;
            }
        }
        if let Some(out_measurement) = &mut function.out_measurement {
            cleanup_item_list(&mut out_measurement.identifier_list, removed_items);
            if out_measurement.identifier_list.is_empty() {
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
