use crate::dwarf::{DwarfDataType, TypeInfo};
use crate::ifdata;
use a2lfile::{A2lObject, IfData};

// check if there is a CANAPE_EXT in the IF_DATA vec and update it if it exists
pub(crate) fn update_ifdata_address(
    ifdata_vec: &mut Vec<IfData>,
    symbol_name: &str,
    address: u64,
) {
    for ifdata in ifdata_vec {
        if let Some(mut decoded_ifdata) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = &mut decoded_ifdata.canape_ext {
                update_ifdata_address_canape_ext(canape_ext, address, symbol_name);
                decoded_ifdata.store_to_ifdata(ifdata);
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                update_ifdata_address_asap1b_ccp(asap1b_ccp, address);
                decoded_ifdata.store_to_ifdata(ifdata);
            }
        }
    }
}


fn update_ifdata_address_canape_ext(
    canape_ext: &mut ifdata::CanapeExt,
    address: u64,
    symbol_name: &str,
) {
    if let Some(link_map) = &mut canape_ext.link_map {
        if link_map.address == 0 {
            // if the address was previously "0" then force it to be displayed as hex after the update
            link_map.get_layout_mut().item_location.1.1 = true;
        }
        link_map.address = address as i32;
        link_map.symbol_name = symbol_name.to_string();
        // these can be set to valid values later on by update_ifdata_type_canape_ext
        link_map.datatype = 0;
        link_map.bit_offset = 0;
        link_map.datatype_valid = 0;
    }
}


fn update_ifdata_address_asap1b_ccp(asap1b_ccp: &mut ifdata::Asap1bCcp, address: u64) {
    if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
        dp_blob.address_extension = 0;
        dp_blob.base_address = address as u32;
        dp_blob.size = 0;
    }
}


// check if there is a CANAPE_EXT in the IF_DATA vec and update it if it exists
pub(crate) fn update_ifdata_type(
    ifdata_vec: &mut Vec<IfData>,
    datatype: &TypeInfo,
) {
    for ifdata in ifdata_vec {
        if let Some(mut decoded_ifdata) = ifdata::A2mlVector::load_from_ifdata(ifdata) {
            if let Some(canape_ext) = &mut decoded_ifdata.canape_ext {
                update_ifdata_type_canape_ext(canape_ext, datatype);
                decoded_ifdata.store_to_ifdata(ifdata);
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                update_ifdata_type_asap1b_ccp(asap1b_ccp, datatype);
                decoded_ifdata.store_to_ifdata(ifdata);
            }
        }
    }
}

fn update_ifdata_type_canape_ext(
    canape_ext: &mut ifdata::CanapeExt,
    typeinfo: &TypeInfo,
) {
    if let Some(link_map) = &mut canape_ext.link_map {
        match &typeinfo.datatype {
            DwarfDataType::Uint8 => {
                link_map.datatype = 0x87;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Uint16 => {
                link_map.datatype = 0x8f;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Uint32 => {
                link_map.datatype = 0x9f;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Uint64 => {
                link_map.datatype = 0xbf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Sint8 => {
                link_map.datatype = 0xc7;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Sint16 => {
                link_map.datatype = 0xcf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Sint32 => {
                link_map.datatype = 0xdf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Sint64 => {
                link_map.datatype = 0xff;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Float => {
                link_map.datatype = 0x01;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Double => {
                link_map.datatype = 0x02;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Enum { size, .. } => {
                match *size {
                    1 => link_map.datatype = 0x87, // 0x40 | 0x07 -> unsigned, 8 bits
                    2 => link_map.datatype = 0x8f, // 0x40 | 0x0f -> unsigned, 16 bits
                    4 => link_map.datatype = 0x9f, // 0x40 | 0x1f -> unsigned, 32 bits
                    8 => link_map.datatype = 0xbf, // 0x40 | 0x3f -> unsigned, 64 bits
                    _ => link_map.datatype = 0,
                }
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Bitfield {
                basetype,
                bit_offset,
                bit_size,
            } => {
                let signed: u16 = match &basetype.datatype {
                    DwarfDataType::Sint8
                    | DwarfDataType::Sint16
                    | DwarfDataType::Sint32
                    | DwarfDataType::Sint64 => 0x40,
                    _ => 0x0,
                };
                link_map.datatype = 0x80 | signed | (bit_size - 1);
                link_map.bit_offset = *bit_offset;
                link_map.datatype_valid = 1;
            }
            DwarfDataType::Array { arraytype, .. } => {
                update_ifdata_type_canape_ext(canape_ext, arraytype);
            }
            _ => {
                link_map.datatype = 0;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 0;
            }
        }
    }
}

fn update_ifdata_type_asap1b_ccp(asap1b_ccp: &mut ifdata::Asap1bCcp, typeinfo: &TypeInfo) {
    if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
        match &typeinfo.datatype {
            DwarfDataType::Uint8 | DwarfDataType::Sint8 => dp_blob.size = 1,
            DwarfDataType::Uint16 | DwarfDataType::Sint16 => dp_blob.size = 2,
            DwarfDataType::Float | DwarfDataType::Uint32 | DwarfDataType::Sint32 => {
                dp_blob.size = 4;
            }
            DwarfDataType::Double | DwarfDataType::Uint64 | DwarfDataType::Sint64 => {
                dp_blob.size = 8;
            }
            DwarfDataType::Enum { size, .. } => dp_blob.size = *size as u32,
            DwarfDataType::Array { arraytype, .. } => {
                update_ifdata_type_asap1b_ccp(asap1b_ccp, arraytype);
            }
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
                if let Some(link_map) = &mut canape_ext.link_map {
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
