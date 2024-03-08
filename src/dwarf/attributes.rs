use super::{DebugDataReader, UnitList};
use gimli::{DebugAddrBase, DebuggingInformationEntry, EndianSlice, RunTimeEndian, UnitHeader};

type SliceType<'a> = EndianSlice<'a, RunTimeEndian>;
type OptionalAttribute<'data> = Option<gimli::AttributeValue<SliceType<'data>>>;

// try to get the attribute of the type attrtype for the DIE
pub(crate) fn get_attr_value<'unit>(
    entry: &DebuggingInformationEntry<'_, 'unit, SliceType, usize>,
    attrtype: gimli::DwAt,
) -> OptionalAttribute<'unit> {
    entry.attr_value(attrtype).unwrap_or(None)
}

// get a name as a String from a DW_AT_name attribute
pub(crate) fn get_name_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
    dwarf: &gimli::Dwarf<EndianSlice<RunTimeEndian>>,
    unit_header: &gimli::UnitHeader<EndianSlice<RunTimeEndian>>,
) -> Result<String, String> {
    let name_attr = get_attr_value(entry, gimli::constants::DW_AT_name)
        .ok_or_else(|| "failed to get name attribute".to_string())?;
    match name_attr {
        gimli::AttributeValue::String(slice) => {
            if let Ok(utf8string) = slice.to_string() {
                // could not demangle, but successfully converted the slice to utf8
                return Ok(utf8string.to_owned());
            }
            Err(format!("could not decode {slice:#?} as a utf-8 string"))
        }
        gimli::AttributeValue::DebugStrRef(str_offset) => {
            match dwarf.debug_str.get_str(str_offset) {
                Ok(slice) => {
                    if let Ok(utf8string) = slice.to_string() {
                        // could not demangle, but successfully converted the slice to utf8
                        return Ok(utf8string.to_owned());
                    }
                    Err(format!("could not decode {slice:#?} as a utf-8 string"))
                }
                Err(err) => Err(err.to_string()),
            }
        }
        gimli::AttributeValue::DebugStrOffsetsIndex(index) => {
            let unit = dwarf.unit(*unit_header).unwrap();
            let offset = dwarf
                .debug_str_offsets
                .get_str_offset(unit.encoding().format, unit.str_offsets_base, index)
                .unwrap();
            match dwarf.debug_str.get_str(offset) {
                Ok(slice) => {
                    if let Ok(utf8string) = slice.to_string() {
                        // could not demangle, but successfully converted the slice to utf8
                        return Ok(utf8string.to_owned());
                    }
                    Err(format!("could not decode {slice:#?} as a utf-8 string"))
                }
                Err(err) => Err(err.to_string()),
            }
        }
        _ => Err(format!("invalid name attribute type {name_attr:#?}")),
    }
}

// get a type reference as an offset relative to the start of .debug_info from a DW_AT_type attribute
// it the type reference is a UnitRef (relative to the unit header) it will be converted first
pub(crate) fn get_typeref_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
    unit: &UnitHeader<SliceType>,
) -> Result<usize, String> {
    let type_attr = get_attr_value(entry, gimli::constants::DW_AT_type)
        .ok_or_else(|| "failed to get type reference attribute".to_string())?;
    match type_attr {
        gimli::AttributeValue::UnitRef(unitoffset) => {
            Ok(unitoffset.to_debug_info_offset(unit).unwrap().0)
        }
        gimli::AttributeValue::DebugInfoRef(infooffset) => Ok(infooffset.0),
        gimli::AttributeValue::DebugTypesRef(_typesig) => {
            // .debug_types was added in DWARF v4 and removed again in v5.
            // silently ignore references to the .debug_types section
            // this is unlikely to matter as few compilers ever bothered with .debug_types
            // (for example gcc supports this, but support is only enabled if the user requests this explicitly)
            Err("unsupported reference to a .debug_types entry (Dwarf 4)".to_string())
        }
        _ => Err(format!("unsupported type reference: {type_attr:#?}")),
    }
}

// get the address of a variable from a DW_AT_location attribute
// The DW_AT_location contains an Exprloc expression that allows the address to be calculated
// in complex ways, so the expression must be evaluated in order to get the address
pub(crate) fn get_location_attribute(
    debug_data_reader: &DebugDataReader,
    entry: &DebuggingInformationEntry<SliceType, usize>,
    encoding: gimli::Encoding,
    current_unit: usize,
) -> Option<u64> {
    let loc_attr = get_attr_value(entry, gimli::constants::DW_AT_location)?;
    if let gimli::AttributeValue::Exprloc(expression) = loc_attr {
        evaluate_exprloc(debug_data_reader, expression, encoding, current_unit)
    } else {
        None
    }
}

// get the address offset of a struct member from a DW_AT_data_member_location attribute
pub(crate) fn get_data_member_location_attribute(
    debug_data_reader: &DebugDataReader,
    entry: &DebuggingInformationEntry<SliceType, usize>,
    encoding: gimli::Encoding,
    current_unit: usize,
) -> Option<u64> {
    let loc_attr = get_attr_value(entry, gimli::constants::DW_AT_data_member_location)?;
    match loc_attr {
        gimli::AttributeValue::Exprloc(expression) => {
            evaluate_exprloc(debug_data_reader, expression, encoding, current_unit)
        }
        gimli::AttributeValue::Udata(val) => Some(val),
        gimli::AttributeValue::Data1(val) => Some(val as u64),
        gimli::AttributeValue::Data2(val) => Some(val as u64),
        gimli::AttributeValue::Data4(val) => Some(val as u64),
        gimli::AttributeValue::Data8(val) => Some(val),
        other => {
            println!("unexpected data_member_location attribute: {other:?}");
            None
        }
    }
}

// get the element size stored in the DW_AT_byte_size attribute
pub(crate) fn get_byte_size_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<u64> {
    let byte_size_attr = get_attr_value(entry, gimli::constants::DW_AT_byte_size)?;
    match byte_size_attr {
        gimli::AttributeValue::Sdata(byte_size) => Some(byte_size as u64),
        gimli::AttributeValue::Udata(byte_size) => Some(byte_size),
        gimli::AttributeValue::Data1(byte_size) => Some(byte_size as u64),
        gimli::AttributeValue::Data2(byte_size) => Some(byte_size as u64),
        gimli::AttributeValue::Data4(byte_size) => Some(byte_size as u64),
        gimli::AttributeValue::Data8(byte_size) => Some(byte_size),
        _ => None,
    }
}

// get the encoding of a variable from the DW_AT_encoding attribute
pub(crate) fn get_encoding_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<gimli::DwAte> {
    let encoding_attr = get_attr_value(entry, gimli::constants::DW_AT_encoding)?;
    if let gimli::AttributeValue::Encoding(enc) = encoding_attr {
        Some(enc)
    } else {
        None
    }
}

// get the upper bound of an array from the DW_AT_upper_bound attribute
pub(crate) fn get_upper_bound_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<u64> {
    let ubound_attr = get_attr_value(entry, gimli::constants::DW_AT_upper_bound)?;
    if let gimli::AttributeValue::Udata(ubound) = ubound_attr {
        Some(ubound)
    } else {
        None
    }
}

// get the byte stride of an array from the DW_AT_upper_bound attribute
// this attribute is only present if the stride is different from the element size
pub(crate) fn get_byte_stride_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<u64> {
    let stride_attr = get_attr_value(entry, gimli::constants::DW_AT_byte_stride)?;
    match stride_attr {
        gimli::AttributeValue::Sdata(stride) => Some(stride as u64),
        gimli::AttributeValue::Udata(stride) => Some(stride),
        gimli::AttributeValue::Data1(stride) => Some(stride as u64),
        gimli::AttributeValue::Data2(stride) => Some(stride as u64),
        gimli::AttributeValue::Data4(stride) => Some(stride as u64),
        gimli::AttributeValue::Data8(stride) => Some(stride),
        _ => None,
    }
}

// get the const value of an enumerator from the DW_AT_const_value attribute
pub(crate) fn get_const_value_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<i64> {
    let constval_attr = get_attr_value(entry, gimli::constants::DW_AT_const_value)?;
    match constval_attr {
        gimli::AttributeValue::Sdata(value) => Some(value),
        gimli::AttributeValue::Udata(value) => Some(value as i64),
        gimli::AttributeValue::Data1(bit_offset) => Some(bit_offset as i64),
        gimli::AttributeValue::Data2(bit_offset) => Some(bit_offset as i64),
        gimli::AttributeValue::Data4(bit_offset) => Some(bit_offset as i64),
        gimli::AttributeValue::Data8(bit_offset) => Some(bit_offset as i64),
        _ => None,
    }
}

// get the bit size of a variable from the DW_AT_bit_size attribute
// this attribute is only present if the variable is in a bitfield
pub(crate) fn get_bit_size_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<u64> {
    let bit_size_attr = get_attr_value(entry, gimli::constants::DW_AT_bit_size)?;
    if let gimli::AttributeValue::Udata(bit_size) = bit_size_attr {
        Some(bit_size)
    } else {
        None
    }
}

// get the bit offset of a variable from the DW_AT_bit_offset attribute
// this attribute is only present if the variable is in a bitfield
pub(crate) fn get_bit_offset_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<u64> {
    let data_bit_offset_attr = get_attr_value(entry, gimli::constants::DW_AT_bit_offset)?;
        // DW_AT_bit_offset: up to Dwarf 3
        // DW_AT_data_bit_offset: Dwarf 4 and following
        match data_bit_offset_attr {
        gimli::AttributeValue::Sdata(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Udata(bit_offset) => Some(bit_offset),
            gimli::AttributeValue::Data1(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Data2(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Data4(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Data8(bit_offset) => Some(bit_offset),
            _ => None,
    }
}

// get the bit offset of a variable from the DW_AT_data_bit_offset attribute
// this attribute is only present if the variable is in a bitfield
pub(crate) fn get_data_bit_offset_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<u64> {
    let data_bit_offset_attr = get_attr_value(entry, gimli::constants::DW_AT_data_bit_offset)?;
        // DW_AT_bit_offset: up to Dwarf 3
        // DW_AT_data_bit_offset: Dwarf 4 and following
        match data_bit_offset_attr {
        gimli::AttributeValue::Sdata(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Udata(bit_offset) => Some(bit_offset),
            gimli::AttributeValue::Data1(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Data2(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Data4(bit_offset) => Some(bit_offset as u64),
            gimli::AttributeValue::Data8(bit_offset) => Some(bit_offset),
            _ => None,
    }
}

pub(crate) fn get_specification_attribute<'data, 'abbrev, 'unit>(
    entry: &'data DebuggingInformationEntry<SliceType, usize>,
    unit: &'unit UnitHeader<EndianSlice<'data, RunTimeEndian>>,
    abbrev: &'abbrev gimli::Abbreviations,
) -> Option<DebuggingInformationEntry<'abbrev, 'unit, EndianSlice<'data, RunTimeEndian>, usize>> {
    let specification_attr = get_attr_value(entry, gimli::constants::DW_AT_specification)?;
    match specification_attr {
        gimli::AttributeValue::UnitRef(unitoffset) => {
            if let Ok(specification_entry) = unit.entry(abbrev, unitoffset) {
                Some(specification_entry)
            } else {
                None
            }
        }
        gimli::AttributeValue::DebugInfoRef(_) => {
            // presumably, a debugger could also generate a DebugInfo ref instead on a UnitRef
            // parsing this would take info that we don't have here, e.g. the unit headers and abbreviations of all units
            // fortunately I have not seen a compiler generate this variation yet
            None
        }
        _ => None,
    }
}

pub(crate) fn get_abstract_origin_attribute<'data, 'abbrev, 'unit>(
    entry: &'data DebuggingInformationEntry<SliceType, usize>,
    unit: &'unit UnitHeader<EndianSlice<'data, RunTimeEndian>>,
    abbrev: &'abbrev gimli::Abbreviations,
) -> Option<DebuggingInformationEntry<'abbrev, 'unit, EndianSlice<'data, RunTimeEndian>, usize>> {
    let origin_attr = get_attr_value(entry, gimli::constants::DW_AT_abstract_origin)?;
    match origin_attr {
        gimli::AttributeValue::UnitRef(unitoffset) => {
            if let Ok(origin_entry) = unit.entry(abbrev, unitoffset) {
                Some(origin_entry)
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn get_addr_base_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
) -> Option<DebugAddrBase> {
    let origin_attr = get_attr_value(entry, gimli::constants::DW_AT_addr_base)?;
    match origin_attr {
        gimli::AttributeValue::DebugAddrBase(addr_base) => Some(addr_base),
        _ => None,
    }
}

// evaluate an exprloc expression to get a variable address or struct member offset
fn evaluate_exprloc(
    debug_data_reader: &DebugDataReader,
    expression: gimli::Expression<EndianSlice<RunTimeEndian>>,
    encoding: gimli::Encoding,
    current_unit: usize,
) -> Option<u64> {
    let mut evaluation = expression.evaluation(encoding);
    evaluation.set_object_address(0);
    evaluation.set_initial_value(0);
    evaluation.set_max_iterations(100);
    let mut eval_result = evaluation.evaluate().unwrap();
    while eval_result != gimli::EvaluationResult::Complete {
        match eval_result {
            gimli::EvaluationResult::RequiresRelocatedAddress(address) => {
                // assume that there is no relocation
                // this would be a bad bet on PC, but on embedded controllers where A2l files are used this is the standard
                eval_result = evaluation.resume_with_relocated_address(address).unwrap();
            }
            gimli::EvaluationResult::RequiresFrameBase => {
                // a variable in the stack frame of a function. Not useful in the conext of A2l files, where we only care about global values
                return None;
            }
            gimli::EvaluationResult::RequiresRegister { .. } => {
                // the value is relative to a register (e.g. the stack base)
                // this means it cannot be referenced at a unique global address and is not suitable for use in a2l
                return None;
            }
            gimli::EvaluationResult::RequiresIndexedAddress { index, .. } => {
                let (unit_header, abbrev) = &debug_data_reader.units[current_unit];
                let address_size = unit_header.address_size();
                let mut entries = unit_header.entries(abbrev);
                let (_, entry) = entries.next_dfs().ok()??;
                let base = get_addr_base_attribute(entry)?;
                let addr = debug_data_reader
                    .dwarf
                    .debug_addr
                    .get_address(address_size, base, index)
                    .ok()?;
                eval_result = evaluation.resume_with_indexed_address(addr).unwrap();
            }
            _other => {
                // there are a lot of other types of address expressions that can only be evaluated by a debugger while a program is running
                // none of these can be handled in the a2lfile use-case.
                return None;
            }
        };
    }
    let result = evaluation.result();
    if let gimli::Piece {
        location: gimli::Location::Address { address },
        ..
    } = result[0]
    {
        Some(address)
    } else {
        None
    }
}

// get a DW_AT_type attribute and return the number of the unit in which the type is located
// as well as an entries_tree iterator that can iterate over the DIEs of the type
pub(crate) fn get_type_attribute(
    entry: &DebuggingInformationEntry<SliceType, usize>,
    unit_list: &UnitList<'_>,
    current_unit: usize,
) -> Result<(usize, gimli::UnitOffset), String> {
    match get_attr_value(entry, gimli::constants::DW_AT_type) {
        Some(gimli::AttributeValue::DebugInfoRef(dbginfo_offset)) => {
            if let Some(unit_idx) = unit_list.get_unit(dbginfo_offset.0) {
                let (unit, _) = &unit_list[unit_idx];
                let unit_offset = dbginfo_offset.to_unit_offset(unit).unwrap();
                Ok((unit_idx, unit_offset))
            } else {
                Err("invalid debug info ref".to_string())
            }
        }
        Some(gimli::AttributeValue::UnitRef(unit_offset)) => Ok((current_unit, unit_offset)),
        _ => Err("failed to get DIE tree".to_string()),
    }
}
