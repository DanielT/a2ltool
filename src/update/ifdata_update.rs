use crate::dwarf::TypeInfo;
use crate::ifdata;
use a2lfile::*;


// check if there is a CANAPE_EXT in the IF_DATA vec and update it if it exists
pub(crate) fn update_ifdata(ifdata_vec: &mut Vec<IfData>, symbol_name: String, datatype: &TypeInfo, address: u64) {
    for ifdata in ifdata_vec {
        if let Some(mut decoded_ifdata) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = &mut decoded_ifdata.canape_ext {
                update_ifdata_canape_ext(canape_ext, address, &symbol_name, datatype);
                decoded_ifdata.store_to_ifdata(ifdata);
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                update_ifdata_asap1b_ccp(asap1b_ccp, address, datatype);
                decoded_ifdata.store_to_ifdata(ifdata);
            }
        }
    }
}


fn update_ifdata_canape_ext(canape_ext: &mut ifdata::CanapeExt, address: u64, symbol_name: &String, datatype: &TypeInfo) {
    if let Some (link_map) = &mut canape_ext.link_map {
        link_map.address = address as i32;
        link_map.symbol_name = symbol_name.clone();
        match datatype {
            TypeInfo::Uint8 => {
                link_map.datatype = 0x87;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Uint16 => {
                link_map.datatype = 0x8f;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Uint32 => {
                link_map.datatype = 0x9f;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Uint64 => {
                link_map.datatype = 0xbf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Sint8 => {
                link_map.datatype = 0xc7;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Sint16 => {
                link_map.datatype = 0xcf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Sint32 => {
                link_map.datatype = 0xdf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Sint64 => {
                link_map.datatype = 0xff;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Float => {
                link_map.datatype = 0x01;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Double => {
                link_map.datatype = 0x02;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Enum { size, .. } => {
                match *size {
                    1 => link_map.datatype = 0x87,
                    2 => link_map.datatype = 0x8f,
                    4 => link_map.datatype = 0x8f,
                    8 => link_map.datatype = 0xbf,
                    _ => link_map.datatype = 0,
                }
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            TypeInfo::Bitfield { basetype, bit_offset, bit_size } => {
                let signed: u16 = match **basetype {
                    TypeInfo::Sint8 |
                    TypeInfo::Sint16 |
                    TypeInfo::Sint32 |
                    TypeInfo::Sint64 => 0x40,
                    _ => 0x0
                };
                link_map.datatype = 0x80 | signed | (bit_size - 1);
                link_map.bit_offset = *bit_offset;
                link_map.datatype_valid = 1;
            }
            _ => {
                link_map.datatype = 0;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 0;
            }
        }
    }
}


fn update_ifdata_asap1b_ccp(asap1b_ccp: &mut ifdata::Asap1bCcp, address: u64, datatype: &TypeInfo) {
    if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
        dp_blob.address_extension = 0;
        dp_blob.base_address = address as u32;

        match datatype {
            TypeInfo::Uint8 |
            TypeInfo::Sint8 => dp_blob.size = 1,
            TypeInfo::Uint16 |
            TypeInfo::Sint16 => dp_blob.size = 2,
            TypeInfo::Float |
            TypeInfo::Uint32 |
            TypeInfo::Sint32 => dp_blob.size = 4,
            TypeInfo::Double |
            TypeInfo::Uint64 |
            TypeInfo::Sint64 => dp_blob.size = 8,
            TypeInfo::Enum {size, ..} => dp_blob.size = *size as u32,
            _ => {
                // size is not set because we don't know
                // for example if the datatype is Struct, then the record_layout must be taken into the calculation
                // rather than do that, the size is left unchanged, since it will most often already be correct
            }
        }
    }
}


// zero out incorrect information in IF_DATA for MEASUREMENTs / CHARACTERISTICs / AXIS_PTS that were not found during update
pub(crate) fn zero_if_data(ifdata_vec: &mut Vec<IfData>) {
    for ifdata in ifdata_vec {
        if let Some(mut decoded_ifdata) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = &mut decoded_ifdata.canape_ext {
                if let Some (link_map) = &mut canape_ext.link_map {
                    // remove address and data type information, but keep the symbol name
                    link_map.address = 0;
                    link_map.datatype = 0;
                    link_map.bit_offset = 0;
                    link_map.datatype_valid = 0;

                    decoded_ifdata.store_to_ifdata(ifdata);
                }
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
                    dp_blob.address_extension = 0;
                    dp_blob.base_address = 0;
                }
            }
        }
    }
}
