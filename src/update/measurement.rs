use crate::dwarf::DwarfDataType;
use crate::dwarf::{DebugData, TypeInfo};
use crate::A2lVersion;
use a2lfile::{A2lObject, Measurement, Module};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::update::{
    adjust_limits, cleanup_item_list,
    enums::{cond_create_enum_conversion, update_enum_compu_methods},
    get_a2l_datatype, get_symbol_info,
    ifdata_update::{update_ifdata, zero_if_data},
    log_update_errors, set_bitmask, set_matrix_dim, set_measurement_ecu_address, set_symbol_link,
};

use super::set_address_type;

pub(crate) fn update_module_measurements(
    module: &mut Module,
    debug_data: &DebugData,
    log_msgs: &mut Vec<String>,
    preserve_unknown: bool,
    version: A2lVersion,
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
                    update_content(
                        module,
                        debug_data,
                        &mut measurement,
                        typeinfo,
                        &mut enum_convlist,
                        version >= A2lVersion::V1_7_0,
                    );

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
                        removed_items.insert(measurement.name.clone());
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
pub(crate) fn update_content<'enumlist, 'typeinfo: 'enumlist>(
    module: &mut Module,
    debug_data: &'typeinfo DebugData,
    measurement: &mut Measurement,
    typeinfo: &'typeinfo TypeInfo,
    enum_convlist: &'enumlist mut HashMap<String, &'typeinfo TypeInfo>,
    use_new_matrix_dim: bool,
) {
    // handle pointers - only allowed for version 1.7.0+ (the caller should take care of this precondition)
    set_address_type(&mut measurement.address_type, typeinfo);
    let typeinfo = typeinfo
        .get_pointer(&debug_data.types)
        .map_or(typeinfo, |(_, t)| t);

    // handle arrays and unwrap the typeinfo
    set_matrix_dim(&mut measurement.matrix_dim, typeinfo, use_new_matrix_dim);
    measurement.array_size = None;
    let typeinfo = typeinfo.get_arraytype().unwrap_or(typeinfo);

    if let DwarfDataType::Enum { enumerators, .. } = &typeinfo.datatype {
        if measurement.conversion == "NO_COMPU_METHOD" {
            measurement.conversion = typeinfo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_compu_method", measurement.name));
        }
        cond_create_enum_conversion(module, &measurement.conversion, enumerators);
        enum_convlist.insert(measurement.conversion.clone(), typeinfo);
    }

    let (ll, ul) = adjust_limits(typeinfo, measurement.lower_limit, measurement.upper_limit);
    measurement.lower_limit = ll;
    measurement.upper_limit = ul;

    measurement.datatype = get_a2l_datatype(typeinfo);
    set_bitmask(&mut measurement.bit_mask, typeinfo);
}

// update the address of a MEASUREMENT object
fn update_measurement_address<'a>(
    measurement: &mut Measurement,
    debug_data: &'a DebugData,
) -> Result<&'a TypeInfo, Vec<String>> {
    match get_symbol_info(
        &measurement.name,
        &measurement.symbol_link,
        &measurement.if_data,
        debug_data,
    ) {
        Ok(sym_info) => {
            // make sure a valid SYMBOL_LINK exists
            set_symbol_link(&mut measurement.symbol_link, sym_info.name.clone());
            set_measurement_ecu_address(&mut measurement.ecu_address, sym_info.address);
            update_ifdata(
                &mut measurement.if_data,
                &sym_info.name,
                sym_info.typeinfo,
                sym_info.address,
            );

            Ok(sym_info.typeinfo)
        }
        Err(errmsgs) => Err(errmsgs),
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
