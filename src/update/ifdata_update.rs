use crate::debuginfo::{DbgDataType, TypeInfo};
use crate::ifdata;
use a2lfile::{A2lObject, IfData};

// check if there is a CANAPE_EXT in the IF_DATA vec and update it if it exists
pub(crate) fn update_ifdata_address(ifdata_vec: &mut Vec<IfData>, symbol_name: &str, address: u64) {
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
            link_map.get_layout_mut().item_location.1 .1 = true;
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
pub(crate) fn update_ifdata_type(ifdata_vec: &mut Vec<IfData>, datatype: &TypeInfo) {
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

fn update_ifdata_type_canape_ext(canape_ext: &mut ifdata::CanapeExt, typeinfo: &TypeInfo) {
    if let Some(link_map) = &mut canape_ext.link_map {
        match &typeinfo.datatype {
            DbgDataType::Uint8 => {
                link_map.datatype = 0x87;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Uint16 => {
                link_map.datatype = 0x8f;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Uint32 => {
                link_map.datatype = 0x9f;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Uint64 => {
                link_map.datatype = 0xbf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Sint8 => {
                link_map.datatype = 0xc7;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Sint16 => {
                link_map.datatype = 0xcf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Sint32 => {
                link_map.datatype = 0xdf;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Sint64 => {
                link_map.datatype = 0xff;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Float => {
                link_map.datatype = 0x01;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Double => {
                link_map.datatype = 0x02;
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Enum { size, signed, .. } => {
                match (*size, *signed) {
                    (1, false) => link_map.datatype = 0x87, // 0x40 | 0x07 -> unsigned, 8 bits
                    (1, true) => link_map.datatype = 0xc7,  // 0xC0 | 0x07 -> signed, 8 bits
                    (2, false) => link_map.datatype = 0x8f, // 0x40 | 0x0f -> unsigned, 16 bits
                    (2, true) => link_map.datatype = 0xcf,  // 0xC0 | 0x0f -> signed, 16 bits
                    (4, false) => link_map.datatype = 0x9f, // 0x40 | 0x1f -> unsigned, 32 bits
                    (4, true) => link_map.datatype = 0xdf,  // 0xC0 | 0x1f -> signed, 32 bits
                    (8, false) => link_map.datatype = 0xbf, // 0x40 | 0x3f -> unsigned, 64 bits
                    (8, true) => link_map.datatype = 0xff,  // 0xC0 | 0x3f -> signed, 64 bits
                    _ => link_map.datatype = 0,
                }
                link_map.bit_offset = 0;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Bitfield {
                basetype,
                bit_offset,
                bit_size,
            } => {
                let signed: u16 = match &basetype.datatype {
                    DbgDataType::Sint8
                    | DbgDataType::Sint16
                    | DbgDataType::Sint32
                    | DbgDataType::Sint64 => 0x40,
                    _ => 0x0,
                };
                link_map.datatype = 0x80 | signed | (bit_size - 1);
                link_map.bit_offset = *bit_offset;
                link_map.datatype_valid = 1;
            }
            DbgDataType::Array { arraytype, .. } => {
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
            DbgDataType::Uint8 | DbgDataType::Sint8 => dp_blob.size = 1,
            DbgDataType::Uint16 | DbgDataType::Sint16 => dp_blob.size = 2,
            DbgDataType::Float | DbgDataType::Uint32 | DbgDataType::Sint32 => {
                dp_blob.size = 4;
            }
            DbgDataType::Double | DbgDataType::Uint64 | DbgDataType::Sint64 => {
                dp_blob.size = 8;
            }
            DbgDataType::Enum { size, .. } => dp_blob.size = *size as u32,
            DbgDataType::Array { arraytype, .. } => {
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
                }
            } else if let Some(asap1b_ccp) = &mut decoded_ifdata.asap1b_ccp {
                if let Some(dp_blob) = &mut asap1b_ccp.dp_blob {
                    dp_blob.address_extension = 0;
                    dp_blob.base_address = 0;
                }
            }
            decoded_ifdata.store_to_ifdata(ifdata);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    static A2L_TEXT_CANAPE_EXT: &str = r#"
/begin PROJECT Project ""
  /begin MODULE Module ""
    /begin MEASUREMENT Meas "" SBYTE NO_COMPU_METHOD 0 0 0 2
      /begin IF_DATA CANAPE_EXT 100
        LINK_MAP "text" 0xFF 0x0 0 0x0 0 0x0 0x0
      /end IF_DATA
    /end MEASUREMENT
  /end MODULE
/end PROJECT"#;

    static A2L_TEXT_ASAP_CCP1B: &str = r#"
/begin PROJECT Project ""
  /begin MODULE Module ""
    /begin MEASUREMENT Meas "" SBYTE NO_COMPU_METHOD 0 0 0 2
      /begin IF_DATA ASAP1B_CCP 
        DP_BLOB 0x0 0xFF 3 
      /end IF_DATA
    /end MEASUREMENT
  /end MODULE
/end PROJECT"#;

    static TYPEINFO_UINT32: TypeInfo = TypeInfo {
        name: None,
        unit_idx: 0,
        datatype: DbgDataType::Uint32,
        dbginfo_offset: 0,
    };

    fn test_setup(input: &str) -> a2lfile::A2lFile {
        let mut log_msgs = Vec::new();
        let mut a2l = a2lfile::load_from_string(
            input,
            Some(ifdata::A2MLVECTOR_TEXT.to_string()),
            &mut log_msgs,
            false,
        )
        .unwrap();
        let module = &mut a2l.project.module[0];
        let ifdata = &module.measurement[0].if_data[0];
        assert!(ifdata::A2mlVector::load_from_ifdata(ifdata).is_some());
        a2l
    }

    #[test]
    fn test_update_ifdata_address_canape_ext() {
        let mut a2l = test_setup(A2L_TEXT_CANAPE_EXT);
        let module = &mut a2l.project.module[0];

        update_ifdata_address(&mut module.measurement[0].if_data, "symbol", 0x1234);
        let decoded_ifdata =
            ifdata::A2mlVector::load_from_ifdata(&module.measurement[0].if_data[0]).unwrap();
        let canape_ext = decoded_ifdata.canape_ext.unwrap();
        let link_map = canape_ext.link_map.unwrap();
        assert_eq!(link_map.address, 0x1234);
    }

    #[test]
    fn test_update_ifdata_type_canape_ext() {
        let mut a2l = test_setup(A2L_TEXT_CANAPE_EXT);
        let module = &mut a2l.project.module[0];

        update_ifdata_type(&mut module.measurement[0].if_data, &TYPEINFO_UINT32);
        let decoded_ifdata =
            ifdata::A2mlVector::load_from_ifdata(&module.measurement[0].if_data[0]).unwrap();
        let canape_ext = decoded_ifdata.canape_ext.unwrap();
        let link_map = canape_ext.link_map.unwrap();
        assert_eq!(link_map.datatype, 0x9f);
    }

    #[test]
    fn test_zero_ifdata_canape_ext() {
        let mut a2l = test_setup(A2L_TEXT_CANAPE_EXT);
        let module = &mut a2l.project.module[0];

        zero_if_data(&mut module.measurement[0].if_data);
        let decoded_ifdata =
            ifdata::A2mlVector::load_from_ifdata(&module.measurement[0].if_data[0]).unwrap();
        let canape_ext = decoded_ifdata.canape_ext.unwrap();
        let link_map = canape_ext.link_map.unwrap();
        assert_eq!(link_map.address, 0);
        assert_eq!(link_map.datatype, 0);
        assert_eq!(link_map.bit_offset, 0);
        assert_eq!(link_map.datatype_valid, 0);
    }

    #[test]
    fn test_update_ifdata_address_asap1b_ccp() {
        let mut a2l = test_setup(A2L_TEXT_ASAP_CCP1B);
        let module = &mut a2l.project.module[0];

        update_ifdata_address(&mut module.measurement[0].if_data, "symbol", 0x1234);
        let decoded_ifdata =
            ifdata::A2mlVector::load_from_ifdata(&module.measurement[0].if_data[0]).unwrap();
        let asap1b_ccp = decoded_ifdata.asap1b_ccp.unwrap();
        let dp_blob = asap1b_ccp.dp_blob.unwrap();
        assert_eq!(dp_blob.base_address, 0x1234);
    }

    #[test]
    fn test_update_ifdata_type_asap1b_ccp() {
        let mut a2l = test_setup(A2L_TEXT_ASAP_CCP1B);
        let module = &mut a2l.project.module[0];

        update_ifdata_type(&mut module.measurement[0].if_data, &TYPEINFO_UINT32);
        let decoded_ifdata =
            ifdata::A2mlVector::load_from_ifdata(&module.measurement[0].if_data[0]).unwrap();
        let asap1b_ccp = decoded_ifdata.asap1b_ccp.unwrap();
        let dp_blob = asap1b_ccp.dp_blob.unwrap();
        assert_eq!(dp_blob.size, 4);
    }

    #[test]
    fn test_zero_ifdata_asap1b_ccp() {
        let mut a2l = test_setup(A2L_TEXT_ASAP_CCP1B);
        let module = &mut a2l.project.module[0];

        zero_if_data(&mut module.measurement[0].if_data);
        let decoded_ifdata =
            ifdata::A2mlVector::load_from_ifdata(&module.measurement[0].if_data[0]).unwrap();
        let asap1b_ccp = decoded_ifdata.asap1b_ccp.unwrap();
        let dp_blob = asap1b_ccp.dp_blob.unwrap();
        assert_eq!(dp_blob.base_address, 0);
    }
}
