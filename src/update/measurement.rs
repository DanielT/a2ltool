use crate::A2lVersion;
use crate::debuginfo::DbgDataType;
use crate::debuginfo::{DebugData, TypeInfo};
use crate::symbol::SymbolInfo;
use a2lfile::{A2lObject, A2lObjectName, ItemList, Measurement, Module};
use std::collections::HashMap;
use std::collections::HashSet;

use crate::update::{
    A2lUpdater, adjust_limits, cleanup_item_list,
    enums::{cond_create_enum_conversion, update_enum_compu_methods},
    get_a2l_datatype, get_symbol_info,
    ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data},
    set_bitmask, set_matrix_dim, set_measurement_ecu_address, set_symbol_link,
};

use super::{A2lUpdateInfo, UpdateResult, make_symbol_link_string, set_address_type};

pub(crate) fn update_all_module_measurements(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo,
) -> Vec<UpdateResult> {
    let mut removed_items = HashSet::<String>::new();
    let mut enum_convlist = HashMap::<String, &TypeInfo>::new();
    let mut measurement_list = ItemList::new();
    let mut results = Vec::new();

    std::mem::swap(&mut data.module.measurement, &mut measurement_list);
    for mut measurement in measurement_list {
        let update_result =
            update_module_measurement(&mut measurement, info, data, &mut enum_convlist);
        if matches!(update_result, UpdateResult::SymbolNotFound { .. }) {
            if info.preserve_unknown {
                measurement.ecu_address = None;
                zero_if_data(&mut measurement.if_data);
                data.module.measurement.push(measurement);
            } else {
                removed_items.insert(measurement.get_name().to_string());
            }
        } else {
            data.module.measurement.push(measurement);
        }
        results.push(update_result);
    }

    // update COMPU_VTABs and COMPU_VTAB_RANGEs based on the data types used in MEASUREMENTs
    update_enum_compu_methods(data.module, &enum_convlist);
    cleanup_removed_measurements(data.module, &removed_items);

    results
}

fn update_module_measurement<'dbg>(
    measurement: &mut Measurement,
    info: &A2lUpdateInfo<'dbg>,
    data: &mut A2lUpdater<'_>,
    enum_convlist: &mut HashMap<String, &'dbg TypeInfo>,
) -> UpdateResult {
    if measurement.var_virtual.is_none() {
        // only MEASUREMENTS that are not VIRTUAL can be updated
        match get_symbol_info(
            measurement.get_name(),
            &measurement.symbol_link,
            &measurement.if_data,
            info.debug_data,
        ) {
            // match update_measurement_address(&mut measurement, info.debug_data, info.version) {
            Ok(sym_info) => {
                update_measurement_address(measurement, info.debug_data, info.version, &sym_info);

                update_ifdata_address(&mut measurement.if_data, &sym_info.name, sym_info.address);

                if info.full_update {
                    // update the data type of the MEASUREMENT object
                    update_ifdata_type(&mut measurement.if_data, sym_info.typeinfo);

                    // update all the information instide a MEASUREMENT
                    update_measurement_datatype(
                        info,
                        data.module,
                        measurement,
                        sym_info.typeinfo,
                        enum_convlist,
                    );

                    UpdateResult::Updated
                } else if info.strict_update {
                    // verify that the data type of the MEASUREMENT object is still correct
                    verify_measurement_datatype(info, data.module, measurement, sym_info.typeinfo)
                } else {
                    // no type update, but the address was updated
                    UpdateResult::Updated
                }
            }
            Err(errmsgs) => UpdateResult::SymbolNotFound {
                blocktype: "MEASUREMENT",
                name: measurement.get_name().to_string(),
                line: measurement.get_line(),
                errors: errmsgs,
            },
        }
    } else {
        // VIRTUAL MEASUREMENTS don't have an address, and don't need to be updated
        UpdateResult::Updated
    }
}

// update the address of a MEASUREMENT object
fn update_measurement_address<'dbg>(
    measurement: &mut Measurement,
    debug_data: &'dbg DebugData,
    version: A2lVersion,
    sym_info: &SymbolInfo<'dbg>,
) {
    if version >= A2lVersion::V1_6_0 {
        // make sure a valid SYMBOL_LINK exists
        let symbol_link_text = make_symbol_link_string(sym_info, debug_data);
        set_symbol_link(&mut measurement.symbol_link, symbol_link_text);
    } else {
        measurement.symbol_link = None;
    }

    set_measurement_ecu_address(&mut measurement.ecu_address, sym_info.address);
}

// update datatype, limits and dimension of a MEASUREMENT
fn update_measurement_datatype<'enumlist, 'typeinfo: 'enumlist>(
    info: &A2lUpdateInfo<'typeinfo>,
    module: &mut Module,
    measurement: &mut Measurement,
    typeinfo: &'typeinfo TypeInfo,
    enum_convlist: &'enumlist mut HashMap<String, &'typeinfo TypeInfo>,
) {
    // handle pointers - only allowed for version 1.7.0+ (the caller should take care of this precondition)
    set_address_type(&mut measurement.address_type, typeinfo);
    let typeinfo = typeinfo
        .get_pointer(&info.debug_data.types)
        .map_or(typeinfo, |(_, t)| t);

    // handle arrays and unwrap the typeinfo
    let use_new_matrix_dim = info.version >= A2lVersion::V1_7_0;
    set_matrix_dim(&mut measurement.matrix_dim, typeinfo, use_new_matrix_dim);
    measurement.array_size = None;
    let typeinfo = typeinfo.get_arraytype().unwrap_or(typeinfo);

    if let DbgDataType::Enum { enumerators, .. } = &typeinfo.datatype {
        if measurement.conversion == "NO_COMPU_METHOD" {
            measurement.conversion = typeinfo
                .name
                .clone()
                .unwrap_or_else(|| format!("{}_compu_method", measurement.get_name()));
        }
        cond_create_enum_conversion(module, &measurement.conversion, enumerators);
        enum_convlist.insert(measurement.conversion.clone(), typeinfo);
    }

    let opt_compu_method = module.compu_method.get(&measurement.conversion);
    let (ll, ul) = adjust_limits(
        typeinfo,
        measurement.lower_limit,
        measurement.upper_limit,
        opt_compu_method,
    );
    measurement.lower_limit = ll;
    measurement.upper_limit = ul;

    measurement.datatype = get_a2l_datatype(typeinfo);
    set_bitmask(&mut measurement.bit_mask, typeinfo);
}

fn verify_measurement_datatype<'enumlist, 'typeinfo: 'enumlist>(
    info: &A2lUpdateInfo<'typeinfo>,
    module: &Module,
    measurement: &Measurement,
    typeinfo: &'typeinfo TypeInfo,
) -> UpdateResult {
    // handle pointers - only allowed for version 1.7.0+ (the caller should take care of this precondition)
    let mut dummy_address_type = measurement.address_type.clone();
    set_address_type(&mut dummy_address_type, typeinfo);
    let typeinfo = typeinfo
        .get_pointer(&info.debug_data.types)
        .map_or(typeinfo, |(_, t)| t);

    // handle arrays and unwrap the typeinfo
    let use_new_matrix_dim = info.version >= A2lVersion::V1_7_0;
    let mut dummy_matrix_dim = measurement.matrix_dim.clone();
    set_matrix_dim(&mut dummy_matrix_dim, typeinfo, use_new_matrix_dim);
    let typeinfo = typeinfo.get_arraytype().unwrap_or(typeinfo);

    let mut bad_conversion = false;
    if let DbgDataType::Enum { .. } = &typeinfo.datatype {
        if measurement.conversion == "NO_COMPU_METHOD" {
            // the type is enum, so there should be a conversion method, but there is none
            bad_conversion = true;
        }
    }

    let opt_compu_method = module.compu_method.get(&measurement.conversion);
    let (ll, ul) = adjust_limits(
        typeinfo,
        measurement.lower_limit,
        measurement.upper_limit,
        opt_compu_method,
    );

    let computed_datatype = get_a2l_datatype(typeinfo);
    let mut dummy_bitmask = measurement.bit_mask.clone();
    set_bitmask(&mut dummy_bitmask, typeinfo);

    if dummy_address_type != measurement.address_type
        || dummy_matrix_dim != measurement.matrix_dim
        || dummy_bitmask != measurement.bit_mask
        || ll != measurement.lower_limit
        || ul != measurement.upper_limit
        || computed_datatype != measurement.datatype
        || bad_conversion
    {
        // the information based on the data type of the MEASUREMENT is not correct
        UpdateResult::InvalidDataType {
            blocktype: "MEASUREMENT",
            name: measurement.get_name().to_string(),
            line: measurement.get_line(),
        }
    } else {
        UpdateResult::Updated
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
