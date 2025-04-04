use crate::{
    debuginfo::{DebugData, TypeInfo},
    symbol::SymbolInfo,
};
use a2lfile::{A2lObject, A2lObjectName, Instance, ItemList, Module};
use std::collections::HashSet;

use crate::update::{
    A2lUpdateInfo, A2lUpdater, TypedefNames, TypedefReferrer, TypedefsRefInfo, UpdateResult,
    cleanup_removed_axis_pts, cleanup_removed_blobs, cleanup_removed_characteristics,
    cleanup_removed_measurements, get_symbol_info,
    ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data},
    make_symbol_link_string, set_address_type, set_matrix_dim, set_symbol_link,
};

// update all INSTANCE objects in a module
pub(crate) fn update_all_module_instances<'dbg>(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo<'dbg>,
    nameset: &TypedefNames,
) -> (Vec<UpdateResult>, TypedefsRefInfo<'dbg>) {
    let mut removed_items = HashSet::<String>::new();
    let mut typedef_types = TypedefsRefInfo::new();
    let mut results = Vec::new();

    let mut instance_list = ItemList::new();
    std::mem::swap(&mut data.module.instance, &mut instance_list);
    for mut instance in instance_list {
        let (update_result, opt_typeinfo) = update_module_instance(&mut instance, info, nameset);

        // prepare the typedef map entry for the instance
        let entry = typedef_types.entry(instance.type_ref.clone());
        let len = data.module.instance.len();
        let typedef_map_value = (opt_typeinfo, TypedefReferrer::Instance(len));

        if matches!(update_result, UpdateResult::SymbolNotFound { .. }) {
            if info.preserve_unknown {
                instance.start_address = 0;
                zero_if_data(&mut instance.if_data);
                data.module.instance.push(instance);
                // the typedef_map_value is a dummy value here, whose typeinfo is None
                // this makes sure that the corresponding TYPEDEF_* object is retained.
                // This only matters if enable_structures is set, since TYPEDEFS are not modified otherwise.
                entry.or_default().push(typedef_map_value);
            } else {
                removed_items.insert(instance.get_name().to_string());
            }
        } else {
            data.module.instance.push(instance);
            // store the typeinfo and the index of the INSTANCE object to enable updating the TYPEDEF_* object later
            entry.or_default().push(typedef_map_value);
        }
        results.push(update_result);
    }
    cleanup_removed_instances(data.module, &removed_items);

    (results, typedef_types)
}

// update a single INSTANCE object
fn update_module_instance<'dbg>(
    instance: &mut Instance,
    info: &A2lUpdateInfo<'dbg>,
    nameset: &TypedefNames,
) -> (UpdateResult, Option<&'dbg TypeInfo>) {
    match get_symbol_info(
        instance.get_name(),
        &instance.symbol_link,
        &instance.if_data,
        info.debug_data,
    ) {
        // match update_instance_address(&mut instance, info.debug_data) {
        Ok(sym_info) => {
            update_instance_address(instance, info.debug_data, &sym_info);
            update_ifdata_address(&mut instance.if_data, &sym_info.name, sym_info.address);

            let type_ref_valid = nameset.contains(&instance.type_ref);

            // save the typeinfo associated with the TYPEDEF_* object.
            // Do this even for invalid type references, because the TYPEDEF_* object might be added later.
            let basetype = sym_info
                .typeinfo
                .get_pointer(&info.debug_data.types)
                .map_or(sym_info.typeinfo, |(_, t)| t);

            let basetype = basetype.get_arraytype().unwrap_or(basetype);

            if info.full_update {
                if type_ref_valid {
                    update_instance_datatype(info, instance, sym_info.typeinfo);
                }
                (UpdateResult::Updated, Some(basetype))
            } else if info.strict_update {
                // Verify that the data type of the INSTANCE object is still correct:
                // Since update_instance_datatype does not modify any referenced data, it is
                // possible to simply compare the instance before and after the update
                let mut instance_copy = instance.clone();
                if type_ref_valid {
                    update_instance_datatype(info, &mut instance_copy, sym_info.typeinfo);
                }
                if *instance != instance_copy {
                    let result = UpdateResult::InvalidDataType {
                        blocktype: "INSTANCE",
                        name: instance.get_name().to_string(),
                        line: instance.get_line(),
                    };
                    (result, Some(basetype))
                } else {
                    (UpdateResult::Updated, Some(basetype))
                }
            } else {
                // The address of the INSTANCE object has been updated, and no update of the data type was requested
                (UpdateResult::Updated, Some(basetype))
            }
        }
        Err(errmsgs) => {
            let result = UpdateResult::SymbolNotFound {
                blocktype: "INSTANCE",
                name: instance.get_name().to_string(),
                line: instance.get_line(),
                errors: errmsgs,
            };
            (result, None)
        }
    }
}

fn update_instance_datatype(
    info: &A2lUpdateInfo,
    instance: &mut Instance,
    typeinfo: &crate::debuginfo::TypeInfo,
) {
    // Each INSTANCE can have:
    // - an ADDRESS_TYPE, which means that it is a pointer to some data
    // - a MATRIX_DIM, meaning this instance is an array of some data
    // when ADDRESS_TYPE and MATRIX_DIM are present at the same time, the INSTANCE represents
    // a pointer to an array, not an array of pointers.
    //
    // Therefore the typeinfo should be transformed to the base data type by first unwrapping
    // one pointer (if any), and then getting an array element type (if any)
    // More complicted constructions like pointers to pointers, arrays of pointers, etc. can not be represented directly
    set_address_type(&mut instance.address_type, typeinfo);
    let typeinfo_1 = typeinfo
        .get_pointer(&info.debug_data.types)
        .map_or(typeinfo, |(_, t)| t);

    set_matrix_dim(&mut instance.matrix_dim, typeinfo_1, true);

    // update the data type of the INSTANCE - this only uses the innermost type
    let typeinfo_2 = typeinfo_1.get_arraytype().unwrap_or(typeinfo_1);
    update_ifdata_type(&mut instance.if_data, typeinfo_2);
}

// update the address of an INSTANCE object
fn update_instance_address<'a>(
    instance: &mut Instance,
    debug_data: &'a DebugData,
    sym_info: &SymbolInfo<'a>,
) {
    // make sure a valid SYMBOL_LINK exists
    let symbol_link_text = make_symbol_link_string(sym_info, debug_data);
    set_symbol_link(&mut instance.symbol_link, symbol_link_text);

    if instance.start_address == 0 {
        // if the start address was previously "0" then force it to be displayed as hex after the update
        instance.get_layout_mut().item_location.3.1 = true;
    }
    instance.start_address = sym_info.address as u32;
}

pub(crate) fn cleanup_removed_instances(module: &mut Module, removed_items: &HashSet<String>) {
    // INSTANCEs can take the place of AXIS_PTS, BLOBs, CHARACTERISTICs or MEASUREMENTs, depending on which kind of TYPEDEF the instance is based on
    cleanup_removed_axis_pts(module, removed_items);
    cleanup_removed_blobs(module, removed_items);
    cleanup_removed_characteristics(module, removed_items);
    cleanup_removed_measurements(module, removed_items);
}
