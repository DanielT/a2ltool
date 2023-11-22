use crate::dwarf::{DebugData, TypeInfo};
use a2lfile::{A2lObject, Blob, Module};
use std::collections::HashSet;

use super::ifdata_update::{update_ifdata, zero_if_data};
use super::{cleanup_item_list, get_symbol_info, log_update_errors, set_symbol_link};

pub(crate) fn update_module_blobs(
    module: &mut Module,
    debug_data: &DebugData,
    log_msgs: &mut Vec<String>,
    preserve_unknown: bool,
) -> (u32, u32) {
    let mut removed_items = HashSet::<String>::new();
    let mut blob_list = Vec::new();
    let mut blob_updated: u32 = 0;
    let mut blob_not_updated: u32 = 0;
    std::mem::swap(&mut module.blob, &mut blob_list);
    for mut blob in blob_list {
        match update_blob_address(&mut blob, debug_data) {
            Ok(typeinfo) => {
                blob.size = typeinfo.get_size() as u32;
                module.blob.push(blob);
                blob_updated += 1;
            }
            Err(errmsgs) => {
                log_update_errors(log_msgs, errmsgs, "BLOB", blob.get_line());

                if preserve_unknown {
                    blob.start_address = 0;
                    zero_if_data(&mut blob.if_data);
                    module.blob.push(blob);
                } else {
                    // item is removed implicitly, because it is not added back to the list
                    removed_items.insert(blob.name.clone());
                }
                blob_not_updated += 1;
            }
        }
    }
    cleanup_removed_blobs(module, &removed_items);

    (blob_updated, blob_not_updated)
}

// update the address of a BLOB object
fn update_blob_address<'a>(
    blob: &mut Blob,
    debug_data: &'a DebugData,
) -> Result<&'a TypeInfo, Vec<String>> {
    match get_symbol_info(&blob.name, &blob.symbol_link, &blob.if_data, debug_data) {
        Ok((address, symbol_datatype, symbol_name)) => {
            // make sure a valid SYMBOL_LINK exists
            set_symbol_link(&mut blob.symbol_link, symbol_name.clone());
            blob.start_address = address as u32;
            update_ifdata(&mut blob.if_data, &symbol_name, symbol_datatype, address);

            Ok(symbol_datatype)
        }
        Err(errmsgs) => Err(errmsgs),
    }
}

pub(crate) fn cleanup_removed_blobs(module: &mut Module, removed_items: &HashSet<String>) {
    for transformer in &mut module.transformer {
        if let Some(transformer_in_objects) = &mut transformer.transformer_in_objects {
            cleanup_item_list(&mut transformer_in_objects.identifier_list, removed_items);
        }
        if let Some(transformer_out_objects) = &mut transformer.transformer_out_objects {
            cleanup_item_list(&mut transformer_out_objects.identifier_list, removed_items);
        }
    }

    // can these be in a GROUP?
}
