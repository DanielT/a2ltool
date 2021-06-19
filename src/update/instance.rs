use std::collections::HashSet;
use a2lfile::*;
use crate::dwarf::*;

use super::ifdata_update::*;
use super::*;


pub(crate) fn update_module_instances(module: &mut Module, debug_data: &DebugData, preserve_unknown: bool) -> (u32, u32) {
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


pub(crate) fn cleanup_removed_instances(module: &mut Module, removed_items: &HashSet<String>) {
    // INSTANCEs can take the place of AXIS_PTS, BLOBs, CHARACTERISTICs or MEASUREMENTs, depending on which kind of TYPEDEF the instance is based on
    cleanup_removed_axis_pts(module, removed_items);
    cleanup_removed_blobs(module, removed_items);
    cleanup_removed_characteristics(module, removed_items);
    cleanup_removed_measurements(module, removed_items);
}