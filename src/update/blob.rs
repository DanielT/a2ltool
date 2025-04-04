use crate::debuginfo::DebugData;
use crate::symbol::SymbolInfo;
use a2lfile::{A2lObject, A2lObjectName, Blob, ItemList, Module};
use std::collections::HashSet;

use super::ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data};
use super::{
    A2lUpdateInfo, A2lUpdater, UpdateResult, cleanup_item_list, get_symbol_info,
    make_symbol_link_string, set_symbol_link,
};

// update all BLOB objects in a module
pub(crate) fn update_all_module_blobs(
    data: &mut A2lUpdater,
    info: &A2lUpdateInfo,
) -> Vec<UpdateResult> {
    let mut removed_items = HashSet::<String>::new();
    let mut blob_list = ItemList::new();
    let mut results = Vec::new();

    std::mem::swap(&mut data.module.blob, &mut blob_list);
    for mut blob in blob_list {
        let update_result = update_module_blob(&mut blob, info);
        if matches!(update_result, UpdateResult::SymbolNotFound { .. }) {
            if info.preserve_unknown {
                blob.start_address = 0;
                zero_if_data(&mut blob.if_data);
                data.module.blob.push(blob);
            } else {
                removed_items.insert(blob.get_name().to_string());
            }
        } else {
            data.module.blob.push(blob);
        }
        results.push(update_result);
    }
    cleanup_removed_blobs(data.module, &removed_items);

    results
}

// update a single BLOB object
fn update_module_blob(blob: &mut Blob, info: &A2lUpdateInfo<'_>) -> UpdateResult {
    match get_symbol_info(
        blob.get_name(),
        &blob.symbol_link,
        &blob.if_data,
        info.debug_data,
    ) {
        // match update_blob_address(&mut blob, debug_data) {
        Ok(sym_info) => {
            update_blob_address(blob, info.debug_data, &sym_info);

            update_ifdata_address(&mut blob.if_data, &sym_info.name, sym_info.address);

            if info.full_update {
                // update the data type of the BLOB object
                update_ifdata_type(&mut blob.if_data, sym_info.typeinfo);

                blob.size = sym_info.typeinfo.get_size() as u32;
                UpdateResult::Updated
            } else if info.strict_update {
                // a blob has no data type, but the blob size could be wrong
                if blob.size != sym_info.typeinfo.get_size() as u32 {
                    UpdateResult::InvalidDataType {
                        blocktype: "BLOB",
                        name: blob.get_name().to_string(),
                        line: blob.get_line(),
                    }
                } else {
                    UpdateResult::Updated
                }
            } else {
                // no data type update requested, and strict update is also not requested
                UpdateResult::Updated
            }
        }
        Err(errmsgs) => UpdateResult::SymbolNotFound {
            blocktype: "BLOB",
            name: blob.get_name().to_string(),
            line: blob.get_line(),
            errors: errmsgs,
        },
    }
}

// update the address of a BLOB object
fn update_blob_address<'dbg>(
    blob: &mut Blob,
    debug_data: &'dbg DebugData,
    sym_info: &SymbolInfo<'dbg>,
) {
    // make sure a valid SYMBOL_LINK exists
    let symbol_link_text = make_symbol_link_string(sym_info, debug_data);
    set_symbol_link(&mut blob.symbol_link, symbol_link_text);
    blob.start_address = sym_info.address as u32;
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
