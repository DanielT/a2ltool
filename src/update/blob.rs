use crate::dwarf::DebugData;
use crate::symbol::SymbolInfo;
use a2lfile::{A2lObject, Blob, Module};
use std::collections::HashSet;

use super::ifdata_update::{update_ifdata_address, update_ifdata_type, zero_if_data};
use super::{
    cleanup_item_list, get_symbol_info, log_update_errors, make_symbol_link_string,
    set_symbol_link, UpdateInfo,
};

pub(crate) fn update_module_blobs(info: &mut UpdateInfo) -> (u32, u32) {
    let mut removed_items = HashSet::<String>::new();
    let mut blob_list = Vec::new();
    let mut blob_updated: u32 = 0;
    let mut blob_not_updated: u32 = 0;
    std::mem::swap(&mut info.module.blob, &mut blob_list);
    for mut blob in blob_list {
        match get_symbol_info(
            &blob.name,
            &blob.symbol_link,
            &blob.if_data,
            info.debug_data,
        ) {
            // match update_blob_address(&mut blob, debug_data) {
            Ok(sym_info) => {
                update_blob_address(&mut blob, info.debug_data, &sym_info);

                update_ifdata_address(&mut blob.if_data, &sym_info.name, sym_info.address);

                if info.full_update {
                    // update the data type of the BLOB object
                    update_ifdata_type(&mut blob.if_data, sym_info.typeinfo);

                    blob.size = sym_info.typeinfo.get_size() as u32;
                } else if info.strict_update {
                    // a blob has no data type, but the blob size could be wrong
                    if blob.size != sym_info.typeinfo.get_size() as u32 {
                        log_update_errors(
                            info.log_msgs,
                            vec![format!(
                                "BLOB size mismatch: expected {}, got {}",
                                blob.size,
                                sym_info.typeinfo.get_size()
                            )],
                            "BLOB",
                            blob.get_line(),
                        );
                        blob.size = sym_info.typeinfo.get_size() as u32;
                    }
                }

                info.module.blob.push(blob);
                blob_updated += 1;
            }
            Err(errmsgs) => {
                log_update_errors(info.log_msgs, errmsgs, "BLOB", blob.get_line());

                if info.preserve_unknown {
                    blob.start_address = 0;
                    zero_if_data(&mut blob.if_data);
                    info.module.blob.push(blob);
                } else {
                    // item is removed implicitly, because it is not added back to the list
                    removed_items.insert(blob.name.clone());
                }
                blob_not_updated += 1;
            }
        }
    }
    cleanup_removed_blobs(info.module, &removed_items);

    (blob_updated, blob_not_updated)
}

// update the address of a BLOB object
fn update_blob_address<'dbg>(
    blob: &mut Blob,
    debug_data: &'dbg DebugData,
    sym_info: &SymbolInfo<'dbg>,
) {
    // make sure a valid SYMBOL_LINK exists
    let symbol_link_text = make_symbol_link_string(&sym_info, debug_data);
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
