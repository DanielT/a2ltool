use crate::{datatype, dwarf::DebugData, insert, search};
use a2lfile::{A2lFile, AddrType, ByteOrderEnum, CharacteristicType, DataType};
use bin_file::{BinFile, IHexFormat, SRecordAddressLength};
use std::{ffi::OsString, fs::File, io::Write, path::Path};

#[derive(Debug)]
struct Calibration {
    symbol: String,
    value_repr: Option<String>,
    address: Option<u32>,
    size: Option<u16>,
    dim: Option<u16>,
    dtype: Option<DataType>,
    endianess: ByteOrderEnum,
}

#[derive(Debug, Clone, Copy)]
pub enum BinFileFormat {
    SREC,
    IHEX,
}

pub(crate) fn calibration_from_binary_to_csv(
    a2l_file: &mut A2lFile,
    elf_info: &Option<DebugData>,
    enable_structures: bool,
    default_endianess: &ByteOrderEnum,
    binfile: &BinFile,
    csv_file: &OsString,
    log_msgs: &mut Vec<String>,
) -> Result<bool, String> {
    let mut calibrations = read_calibrations_csv(csv_file, &default_endianess);
    calibration_symbols_load(
        &mut calibrations,
        a2l_file,
        elf_info,
        enable_structures,
        log_msgs,
    )?;
    read_calibration(&mut calibrations, &binfile, log_msgs)?;
    write_calibrations_csv(csv_file, &calibrations)?;
    Ok(true)
}

pub(crate) fn calibration_from_csv_to_binary(
    a2l_file: &mut A2lFile,
    elf_info: &Option<DebugData>,
    enable_structures: bool,
    default_endianess: &ByteOrderEnum,
    binfile: &mut BinFile,
    csv_file: &OsString,
    binary_file: &OsString,
    log_msgs: &mut Vec<String>,
) -> Result<bool, String> {
    let mut calibrations = read_calibrations_csv(csv_file, &default_endianess);
    calibration_symbols_load(
        &mut calibrations,
        a2l_file,
        elf_info,
        enable_structures,
        log_msgs,
    )?;
    write_calibration(&calibrations, binfile, log_msgs)?;
    save_binfile(binary_file, binfile, log_msgs)?;
    Ok(true)
}

pub(crate) fn guess_default_endianess(
    a2l_file: &A2lFile,
    elf_info: &Option<DebugData>,
) -> Option<ByteOrderEnum> {
    let mut default_order = None;
    for module in &a2l_file.project.module {
        if let Some(mod_common) = &module.mod_common {
            if let Some(byte_order) = &mod_common.byte_order {
                if default_order.is_none() {
                    default_order = Some(byte_order.byte_order.clone());
                } else if byte_order.byte_order != default_order.unwrap() {
                    panic!("Mixed BYTE_ORDER in MOD_COMMON not supported. Specify the --default_byte_order on the command line.")
                }
            }
        }
    }
    if default_order.is_none() {
        if let Some(debugdata) = elf_info {
            default_order = match debugdata.endian {
                object::Endianness::Little => Some(ByteOrderEnum::LittleEndian),
                object::Endianness::Big => Some(ByteOrderEnum::BigEndian),
            };
        }
    }
    default_order
}

fn read_calibrations_csv(
    csv_file: &OsString,
    default_endianess: &ByteOrderEnum,
) -> Vec<Calibration> {
    let mut ret: Vec<Calibration> = Vec::new();
    let text = std::fs::read_to_string(csv_file).expect("Cannot read CSV file");

    for line in text.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() > 0 {
            let f = fields[0].trim();
            if f.len() > 0 && !f.starts_with("#") {
                let mut cal = Calibration {
                    symbol: f.to_string(),
                    value_repr: None,
                    address: None,
                    size: None,
                    dim: None,
                    dtype: None,
                    endianess: default_endianess.clone(),
                };
                if fields.len() > 1 {
                    let f = fields[1].trim();
                    if f.len() > 0 {
                        cal.value_repr = Some(f.to_string());
                    }
                }
                ret.push(cal);
            }
        }
    }

    ret
}

fn write_calibrations_csv(
    csv_file: &OsString,
    calibrations: &Vec<Calibration>,
) -> Result<bool, String> {
    let mut calmap = std::collections::HashMap::new();
    for cal in calibrations {
        calmap.insert(&cal.symbol[..], cal);
    }

    let text = std::fs::read_to_string(csv_file).expect("Cannot read CSV file");
    let mut file = File::create(csv_file).expect("Cannot open CSV file for writing");

    for line in text.lines() {
        let mut bypass = true;
        let l = line.trim();
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() > 0 {
            let f = fields[0].trim();
            if f.len() > 0 && !f.starts_with("#") {
                if let Some(cal) = calmap.get(f) {
                    bypass = false;
                    writeln!(
                        file,
                        "{};{}",
                        (*cal).symbol,
                        (*cal).value_repr.as_ref().unwrap_or(&String::from(""))
                    )
                    .expect("Error writing CSV file");
                }
            }
        }
        if bypass {
            writeln!(file, "{}", l).expect("Error writing CSV file");
        }
    }

    Ok(true)
}

fn calibration_symbols_load(
    calibrations: &mut Vec<Calibration>,
    a2l_file: &mut A2lFile,
    elf_info: &Option<DebugData>,
    enable_structures: bool,
    log_msgs: &mut Vec<String>,
) -> Result<bool, String> {
    let mut characteristics = search::search_characteristics(a2l_file, &[".*"], log_msgs);

    if let Some(debugdata) = &elf_info {
        // Add the characteristics that are listed in the CSV file, but not in the A2L.
        let mut characteristic_symbols: Vec<&str> = Vec::new();
        for cal in &*calibrations {
            if !characteristics.contains_key(&cal.symbol) {
                characteristic_symbols.push(&cal.symbol);
            }
        }
        if !characteristic_symbols.is_empty() {
            insert::insert_items(
                a2l_file,
                debugdata,
                vec![],
                characteristic_symbols,
                Some("AUTO"),
                log_msgs,
                enable_structures,
            );

            characteristics = search::search_characteristics(a2l_file, &[".*"], log_msgs);
        }
    }

    let record_layouts = search::search_reord_layout(a2l_file, &[".*"], log_msgs);

    for cal in &mut *calibrations {
        if let Some(characteristic) = characteristics.get(&cal.symbol) {
            cal.address = Some(characteristic.address);
            if characteristic.byte_order.is_some() {
                cal.endianess = characteristic.byte_order.as_ref().unwrap().byte_order;
            }
            match characteristic.characteristic_type {
                CharacteristicType::Value => {
                    cal.dim = Some(1);
                }
                CharacteristicType::ValBlk => {
                    if let Some(matrix_dim) = &characteristic.matrix_dim {
                        cal.dim = Some(matrix_dim.dim_list.iter().product());
                    } else {
                        log_msgs.push(format!(
                            "Characteristic {} matrix dimension not found",
                            &cal.symbol
                        ));
                        continue;
                    }
                }
                _ => {
                    log_msgs.push(format!(
                        "Characteristic {} type {} not supported",
                        &cal.symbol, &characteristic.characteristic_type
                    ));
                    continue;
                }
            }
            if let Some(rl) = record_layouts.get(&characteristic.deposit) {
                if let Some(fnc_value) = &rl.fnc_values {
                    if fnc_value.position == 1 && fnc_value.address_type == AddrType::Direct {
                        cal.size = Some(datatype::get_datatype_size(&fnc_value.datatype));
                        cal.dtype = Some(fnc_value.datatype);
                    } else {
                        log_msgs.push(format!(
                            "Characteristic {} record layout not supported",
                            &cal.symbol
                        ));
                    }
                } else {
                    log_msgs.push(format!(
                        "Characteristic {} data type not found",
                        &cal.symbol
                    ));
                };
            };
        } else {
            log_msgs.push(format!("Symbol {} not found", &cal.symbol));
        }
    }

    Ok(true)
}

fn read_calibration(
    calibrations: &mut Vec<Calibration>,
    binfile: &BinFile,
    log_msgs: &mut Vec<String>,
) -> Result<bool, String> {
    log_msgs.push(format!("Reading calibrations from binary."));
    for cal in &mut *calibrations {
        cal.value_repr = None;
        if cal.address.is_some() && cal.dtype.is_some() && cal.size.is_some() && cal.dim.is_some() {
            let a = cal.address.unwrap() as usize;
            let s = cal.size.unwrap() as usize;
            let d = cal.dim.unwrap() as usize;
            let range = a..a + (s * d);
            let val = binfile.get_values_by_address_range(range);
            if let Some(val_vec) = val {
                match datatype::bytes_to_text(
                    &val_vec,
                    cal.dtype.as_ref().unwrap(),
                    d,
                    &cal.endianess,
                ) {
                    Ok(x) => {
                        log_msgs.push(format!("CAL: {}: {}", &cal.symbol, &x));
                        cal.value_repr = Some(x)
                    }
                    Err(e) => log_msgs.push(format!("ERROR decoding {}: {}", &cal.symbol, &e)),
                }
            } else {
                log_msgs.push(format!("ERROR reading {}", &cal.symbol));
            }
        }
    }

    Ok(true)
}

fn write_calibration(
    calibrations: &Vec<Calibration>,
    binfile: &mut BinFile,
    log_msgs: &mut Vec<String>,
) -> Result<bool, String> {
    log_msgs.push(format!("Writing calibrations to binary."));
    for cal in calibrations {
        if cal.address.is_some()
            && cal.dtype.is_some()
            && cal.size.is_some()
            && cal.dim.is_some()
            && cal.value_repr.is_some()
        {
            let a = cal.address.unwrap() as usize;
            let d = cal.dim.unwrap() as usize;
            match datatype::text_to_bytes(
                &cal.value_repr.as_ref().unwrap(),
                cal.dtype.as_ref().unwrap(),
                d,
                &cal.endianess,
            ) {
                Ok(val) => {
                    log_msgs.push(format!(
                        "CAL: {}: {}",
                        &cal.symbol,
                        &cal.value_repr.as_ref().unwrap()
                    ));
                    let _ = binfile.add_bytes(val, Some(a), true);
                }
                Err(e) => log_msgs.push(format!("ERROR encoding {}: {}", &cal.symbol, &e)),
            }
        } else {
            log_msgs.push(format!("ERROR writing {}", &cal.symbol));
        }
    }

    Ok(true)
}

fn guess_binfile_format(
    binary_file: &OsString,
) -> (
    Option<BinFileFormat>,
    Option<SRecordAddressLength>,
    Option<IHexFormat>,
) {
    let mut binfile_format: Option<BinFileFormat> = None;
    let mut srec_addr_len: Option<SRecordAddressLength> = None;
    let mut ihex_format: Option<IHexFormat> = None;

    if let Some(ext) = Path::new(binary_file)
        .extension()
        .and_then(|ext| ext.to_str())
    {
        let ext_lower = ext.to_lowercase();

        match ext_lower.as_str() {
            "srec" | "s19" | "s28" | "s37" => {
                binfile_format = Some(BinFileFormat::SREC);
                srec_addr_len = Some(SRecordAddressLength::Length32);
            }
            "hex" | "ihex" => {
                binfile_format = Some(BinFileFormat::IHEX);
                ihex_format = Some(IHexFormat::IHex32);
            }
            _ => {}
        };
    }

    (binfile_format, srec_addr_len, ihex_format)
}

fn save_binfile(
    binary_file: &OsString,
    binfile: &BinFile,
    _log_msgs: &mut Vec<String>,
) -> Result<bool, String> {
    let (binfile_format, srec_addr_len, ihex_format) = guess_binfile_format(binary_file);

    let text: Vec<String> = match binfile_format {
        Some(BinFileFormat::SREC) => binfile
            .to_srec(
                None,
                srec_addr_len.unwrap_or(SRecordAddressLength::Length32),
            )
            .unwrap(),
        Some(BinFileFormat::IHEX) => binfile
            .to_ihex(None, ihex_format.unwrap_or(IHexFormat::IHex32))
            .unwrap(),
        _ => {
            return Err(String::from("Unrecognized binary file format"));
        }
    };

    let mut file = File::create(binary_file).expect("Error opening binary file for write");
    for line in text {
        writeln!(file, "{}", line).expect("Error writing binary file");
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_calibrations_csv() {
        let csv_file = OsString::from("tests/calibrate/cal_test_1.csv");
        let default_endianess = ByteOrderEnum::LittleEndian;
        let calibrations = read_calibrations_csv(&csv_file, &default_endianess);

        assert_eq!(calibrations.len(), 5);
        assert_eq!(calibrations[0].symbol, "cal_sleep_time");
        assert_eq!(calibrations[0].value_repr, Some("250".to_string()));
        assert_eq!(calibrations[1].symbol, "cal_float");
        assert_eq!(calibrations[1].value_repr, Some("8.8888".to_string()));
        assert_eq!(calibrations[2].symbol, "cal_text");
        assert_eq!(calibrations[2].value_repr, Some("\"aaa\"".to_string()));
        assert_eq!(calibrations[3].symbol, "cal_sleep_counts");
        assert_eq!(calibrations[3].value_repr, Some("(2,4,6)".to_string()));
    }

    #[test]
    fn test_read_calibration() {
        let default_endianess = ByteOrderEnum::LittleEndian;
        let mut calibrations = vec![
            Calibration {
                symbol: "cal_sleep_time".to_string(),
                value_repr: None,
                address: Some(0xc34c),
                size: Some(4),
                dim: Some(1),
                dtype: Some(DataType::Ulong),
                endianess: default_endianess,
            },
            Calibration {
                symbol: "cal_double".to_string(),
                value_repr: None,
                address: Some(0xc320),
                size: Some(8),
                dim: Some(2),
                dtype: Some(DataType::Float64Ieee),
                endianess: default_endianess,
            },
        ];

        let bin_file_path = OsString::from("tests/calibrate/cal_test_1.hex");
        let binfile = BinFile::from_file(&bin_file_path).expect("Cannot read binary file");
        let mut log_msgs = Vec::new();

        let result = read_calibration(&mut calibrations, &binfile, &mut log_msgs);
        assert!(result.is_ok());

        assert_eq!(calibrations[0].symbol, "cal_sleep_time");
        assert_eq!(
            calibrations[0]
                .value_repr
                .as_ref()
                .expect("Undefined value")
                .parse::<u32>()
                .expect("Value is not a number"),
            100u32
        );

        assert_eq!(calibrations[1].symbol, "cal_double");
        let text = calibrations[1]
            .value_repr
            .as_ref()
            .expect("Undefined value");
        assert!(text.starts_with('(') && text.ends_with(')'));
        let numbers: Vec<&str> = text[1..text.len() - 1].split(',').collect();
        assert_eq!(numbers.len(), 2);
        let tolerance = 1e-6;
        assert!(
            (numbers[0].parse::<f64>().expect("Value is not a number") - 1.1111111).abs()
                < tolerance,
            "Values are not equal within the tolerance"
        );
        assert!(
            (numbers[1].parse::<f64>().expect("Value is not a number") - 2.222222).abs()
                < tolerance,
            "Values are not equal within the tolerance"
        );
    }

    #[test]
    fn calibrate_test() {
        let default_endianess = ByteOrderEnum::LittleEndian;
        let a2l_path = OsString::from("tests/calibrate/cal_test_1.a2l");
        let binary_start_path = OsString::from("tests/calibrate/cal_test_1.hex");
        let csv_write_path = OsString::from("tests/calibrate/cal_test_1.csv");
        let tmp_dir = tempfile::tempdir().expect("Failed to create temporary directory");
        let binary_end_path = OsString::from(tmp_dir.path().join("test.hex"));
        let csv_read_start_path = OsString::from(tmp_dir.path().join("csv_start.csv"));
        let csv_read_end_path = OsString::from(tmp_dir.path().join("csv_end.csv"));

        let mut a2l_log_msgs = Vec::new();
        let mut a2l_file =
            a2lfile::load(&a2l_path, None, &mut a2l_log_msgs, false).expect("Cannot read A2L file");

        let mut log_msgs = Vec::new();

        std::fs::copy(&csv_write_path, &csv_read_start_path).expect("Failed to copy CSV file");
        std::fs::copy(&csv_write_path, &csv_read_end_path).expect("Failed to copy CSV file");

        let res = calibration_from_binary_to_csv(
            &mut a2l_file,
            &None,
            false,
            &default_endianess,
            &BinFile::from_file(&binary_start_path).expect("Cannot read binary file"),
            &csv_read_start_path,
            &mut log_msgs,
        );
        assert!(res.is_ok());

        let res = calibration_from_csv_to_binary(
            &mut a2l_file,
            &None,
            false,
            &default_endianess,
            &mut BinFile::from_file(&binary_start_path).expect("Cannot read binary file"),
            &csv_write_path,
            &binary_end_path,
            &mut log_msgs,
        );
        assert!(res.is_ok());

        let res = calibration_from_binary_to_csv(
            &mut a2l_file,
            &None,
            false,
            &default_endianess,
            &BinFile::from_file(&binary_end_path).expect("Cannot read binary file"),
            &csv_read_end_path,
            &mut log_msgs,
        );
        assert!(res.is_ok());

        let mut calibrations_start =
            read_calibrations_csv(&csv_read_start_path, &default_endianess);
        calibration_symbols_load(
            &mut calibrations_start,
            &mut a2l_file,
            &None,
            false,
            &mut log_msgs,
        )
        .expect("Error loading calibrations metadata");
        let mut calibrations_write = read_calibrations_csv(&csv_write_path, &default_endianess);
        calibration_symbols_load(
            &mut calibrations_write,
            &mut a2l_file,
            &None,
            false,
            &mut log_msgs,
        )
        .expect("Error loading calibrations metadata");
        let mut calibrations_end = read_calibrations_csv(&csv_read_end_path, &default_endianess);
        calibration_symbols_load(
            &mut calibrations_end,
            &mut a2l_file,
            &None,
            false,
            &mut log_msgs,
        )
        .expect("Error loading calibrations metadata");

        assert_eq!(calibrations_start.len(), calibrations_end.len());
        assert_eq!(calibrations_start.len(), calibrations_write.len());

        for i in 0..calibrations_start.len() {
            assert_eq!(calibrations_start[i].symbol, calibrations_end[i].symbol);
            if calibrations_start[i].symbol != "cal_text" {
                assert_eq!(
                    calibrations_write[i].value_repr,
                    calibrations_end[i].value_repr
                );
                assert_ne!(
                    calibrations_start[i].value_repr,
                    calibrations_end[i].value_repr
                );
            } else {
                assert_eq!(
                    datatype::text_to_bytes(
                        calibrations_write[i].value_repr.as_ref().unwrap(),
                        calibrations_write[i].dtype.as_ref().unwrap(),
                        calibrations_write[i].dim.unwrap().into(),
                        &default_endianess
                    ),
                    datatype::text_to_bytes(
                        calibrations_end[i].value_repr.as_ref().unwrap(),
                        calibrations_end[i].dtype.as_ref().unwrap(),
                        calibrations_end[i].dim.unwrap().into(),
                        &default_endianess
                    )
                );
                assert_ne!(
                    datatype::text_to_bytes(
                        calibrations_start[i].value_repr.as_ref().unwrap(),
                        calibrations_start[i].dtype.as_ref().unwrap(),
                        calibrations_start[i].dim.unwrap().into(),
                        &default_endianess
                    ),
                    datatype::text_to_bytes(
                        calibrations_end[i].value_repr.as_ref().unwrap(),
                        calibrations_end[i].dtype.as_ref().unwrap(),
                        calibrations_end[i].dim.unwrap().into(),
                        &default_endianess
                    )
                );
            }
        }
    }
}
