use crate::{dwarf::DebugData, symbol::SymbolInfo};
use a2lfile::{A2lObject, Instance, Module};
use std::collections::HashSet;

use crate::update::{
    cleanup_removed_axis_pts, cleanup_removed_blobs, cleanup_removed_characteristics,
    cleanup_removed_measurements, get_symbol_info,
    ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data},
    log_update_errors, set_symbol_link, TypedefNames, TypedefReferrer, TypedefsRefInfo,
};

use super::{make_symbol_link_string, set_address_type, set_matrix_dim, UpdateInfo};

pub(crate) fn update_module_instances<'dbg>(
    info: &mut UpdateInfo<'_, 'dbg, '_>,
    nameset: &TypedefNames,
) -> (u32, u32, TypedefsRefInfo<'dbg>) {
    let mut removed_items = HashSet::<String>::new();
    let mut instance_list = Vec::new();
    let mut instance_updated: u32 = 0;
    let mut instance_not_updated: u32 = 0;
    let mut typedef_types = TypedefsRefInfo::new();
    std::mem::swap(&mut info.module.instance, &mut instance_list);
    for mut instance in instance_list {
        match get_symbol_info(
            &instance.name,
            &instance.symbol_link,
            &instance.if_data,
            info.debug_data,
        ) {
            // match update_instance_address(&mut instance, info.debug_data) {
            Ok(sym_info) => {
                update_instance_address(&mut instance, info.debug_data, &sym_info);
                update_ifdata_address(&mut instance.if_data, &sym_info.name, sym_info.address);

                let type_ref_valid = nameset.contains(&instance.type_ref);

                if info.full_update {
                    update_instance_datatype(
                        info,
                        &mut instance,
                        sym_info.typeinfo,
                        type_ref_valid,
                    );
                } else if info.strict_update {
                    // Verify that the data type of the INSTANCE object is still correct:
                    // Since update_instance_datatype does not modify any referenced data, it is
                    // possible to simply compare the instance before and after the update
                    let instance_copy = instance.clone();
                    update_instance_datatype(
                        info,
                        &mut instance,
                        sym_info.typeinfo,
                        type_ref_valid,
                    );
                    if instance != instance_copy {
                        info.log_msgs.push(format!(
                            "Error updating INSTANCE on line {}: data type has changed",
                            instance.get_line()
                        ));
                        instance_not_updated += 1;
                        continue;
                    }
                }

                if type_ref_valid {
                    // with a valid type reference, it is possible to update the underlying TYPEDEF_* object later on
                    // this is only done if enable_structures is set to true (from the command line)
                    let typedef_ref = instance.type_ref.clone();
                    let basetype = sym_info
                        .typeinfo
                        .get_pointer(&info.debug_data.types)
                        .map_or(sym_info.typeinfo, |(_, t)| t);

                    let basetype = basetype.get_arraytype().unwrap_or(basetype);
                    typedef_types.entry(typedef_ref).or_default().push((
                        Some(basetype),
                        TypedefReferrer::Instance(info.module.instance.len()),
                    ));

                    info.module.instance.push(instance);
                    instance_updated += 1;
                } else if !info.full_update || info.preserve_unknown {
                    // if full update is off, then the validity of the type reference is not checked
                    // alternatively, if preserve_unknown is on, then the instance is kept even if the type reference is invalid
                    // in either case the instance is "valid enough" to be kept
                    info.module.instance.push(instance);
                    instance_updated += 1;
                } else {
                    info.log_msgs.push(format!("Error updating INSTANCE on line {}: type ref {} does not refer to any TYPEDEF_*", instance.get_line(), instance.type_ref));
                    instance_not_updated += 1;
                }
            }
            Err(errmsgs) => {
                log_update_errors(info.log_msgs, errmsgs, "INSTANCE", instance.get_line());

                if info.preserve_unknown {
                    instance.start_address = 0;
                    zero_if_data(&mut instance.if_data);
                    typedef_types
                        .entry(instance.type_ref.clone())
                        .or_default()
                        .push((None, TypedefReferrer::Instance(info.module.instance.len())));
                    info.module.instance.push(instance);
                } else {
                    // item is removed implicitly, because it is not added back to the list
                    removed_items.insert(instance.name.clone());
                }
                instance_not_updated += 1;
            }
        }
    }
    cleanup_removed_instances(info.module, &removed_items);

    (instance_updated, instance_not_updated, typedef_types)
}

fn update_instance_datatype(
    info: &mut UpdateInfo,
    instance: &mut Instance,
    typeinfo: &crate::dwarf::TypeInfo,
    type_ref_valid: bool,
) {
    if type_ref_valid {
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

        // update the data type of the INSTANCE - this only uses the innnermost type
        let typeinfo_2 = typeinfo_1.get_arraytype().unwrap_or(typeinfo_1);
        update_ifdata_type(&mut instance.if_data, typeinfo_2);
    }
}

// update the address of an INSTANCE object
fn update_instance_address<'a>(
    instance: &mut Instance,
    debug_data: &'a DebugData,
    sym_info: &SymbolInfo<'a>,
) {
    // make sure a valid SYMBOL_LINK exists
    let symbol_link_text = make_symbol_link_string(&sym_info, debug_data);
    set_symbol_link(&mut instance.symbol_link, symbol_link_text);

    if instance.start_address == 0 {
        // if the start address was previously "0" then force it to be displayed as hex after the update
        instance.get_layout_mut().item_location.3 .1 = true;
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
