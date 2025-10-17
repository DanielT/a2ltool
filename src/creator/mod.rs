use a2lfile::{
    A2lFile, A2lObject, A2lObjectName, A2lObjectNameSetter, AnyTypedef, ByteOrderEnum,
    CoeffsLinear, CompuTabRef, ConversionType, DataType, Format, Formula, FormulaInv, Measurement,
    Module, PhysUnit,
};
use std::collections::HashSet;
use std::fmt::Write;
use std::{collections::HashMap, ffi::OsString};

use crate::A2lVersion;

mod parser;
mod scanner;

#[derive(Debug)]
enum Definition {
    Symbol(SymbolDefinition),
    Element(ElementDefinition),
    SubGroup(SubGroupDefinition),
    MainGroup(MainGroupDefinition),
    Conversion(ConversionDefinition),
    Instance(InstanceDefinition),
    VarCriterion(VarCriterionDefinition),
}

#[derive(Debug)]
struct SymbolDefinition {
    symbol_name: String,
    a2l_name: String,
    config: ItemConfig,
}

#[derive(Debug, Clone)]
struct ElementDefinition {
    symbol_name: String,
    a2l_name: String,
    structure: Vec<String>,
    config: ItemConfig,
}

#[derive(Debug, Clone)]
enum ItemConfig {
    Measure(MeasureCfg),
    Parameter(ParameterCfg),
    CurveMap(CurveMapCfg),
    Axis(AxisCfg),
    String(StringCfg),
    SubStructure(SubStructureCfg),
}

#[derive(Debug, Clone)]
struct MeasureCfg {
    write_access: Option<bool>,
    datatype: DataType,
    bitmask: Option<u64>,
    range: Option<(f64, f64)>, // basic range
    // MEASUREMENT does not have an extended range setting
    attributes: Attributes,
}

#[derive(Debug, Clone)]
struct ParameterCfg {
    write_access: Option<bool>,
    datatype: DataType,
    bitmask: Option<u64>,
    range: Option<(f64, f64)>,          // basic range
    extended_range: Option<(f64, f64)>, // extended range
    attributes: Attributes,
}

#[derive(Debug, Clone)]
struct CurveMapCfg {
    write_access: Option<bool>,
    datatype: DataType,
    bitmask: Option<u64>,
    range: Option<(f64, f64)>,          // basic range
    extended_range: Option<(f64, f64)>, // extended range
    layout: String,
    attributes: MapAttributes,
    x_axis: Box<AxisInfo>,
    y_axis: Option<Box<AxisInfo>>, // available for Map, not used for Curve
}

#[derive(Debug, Clone)]
struct AxisCfg {
    write_access: Option<bool>,
    datatype: DataType,
    range: Option<(f64, f64)>,          // basic range
    extended_range: Option<(f64, f64)>, // extended range
    layout: String,
    dimension: Vec<u32>,
    input_signal: Option<String>,
    input_is_instance: bool,
    attributes: MapAttributes,
}

#[derive(Debug, Clone)]
struct StringCfg {
    length: u32,
    write_access: Option<bool>,
    attributes: StringAttributes,
}

#[derive(Debug, Clone)]
struct SubStructureCfg {
    data_type_struct: Option<String>,
    attributes: StructAttributes,
}

#[derive(Debug)]
struct SubGroupDefinition {
    name: String,
    description: Option<String>,
}

#[derive(Debug)]
struct MainGroupDefinition {
    name: String,
    description: Option<String>,
}

#[derive(Debug)]
struct ConversionDefinition {
    name: String,
    unit: Option<Unit>,
    description: Option<String>,
    config: ConversionConfig,
}

#[derive(Debug)]
enum ConversionConfig {
    Linear(LinearCfg),
    Formula(FormulaCfg),
    Table(TableCfg),
}

#[derive(Debug)]
struct LinearCfg {
    factor: f64,
    offset: f64,
}

#[derive(Debug)]
struct FormulaCfg {
    formula: String,
    inverse_formula: Option<String>,
}

#[derive(Debug)]
struct TableCfg {
    rows: Vec<TableRow>,
    default_value: Option<String>,
}

#[derive(Debug)]
struct InstanceDefinition {
    name: String,
    a2l_name: Option<String>,
    structure_name: String,
    address: Option<u32>,
    dimension: Vec<u32>,
    split: Option<SplitType>,
    _size: Option<u32>, // unused: instance size could be used for address offset calculation
    group: Option<GroupAttribute>,
    overwrites: Vec<Overwrite>,
}

#[derive(Debug)]
struct Overwrite {
    element_path: Vec<String>,
    details: OverwriteSpec,
}

#[derive(Debug)]
enum OverwriteSpec {
    Conversion(ConversionAttribute),
    Description(String),
    Alias(String),
    #[allow(unused)] // color attributes can be defined, but are not created in the a2l file
    Color(u32),
    GroupAssignment(GroupAttribute),
    Range(f64, f64),
}

#[derive(Debug)]
struct VarCriterionDefinition {
    name: String,
    description: Option<String>,
    selector_type: SelectorType,
    selector: String,
    variants: Vec<Variant>,
}

#[derive(Debug)]
struct Variant {
    name: String,
    selector_value: u32,
    offset: u32,
}

#[derive(Debug)]
enum SelectorType {
    Measure,
    Parameter,
}

#[derive(Debug, Default, Clone)]
struct Attributes {
    address: Option<u32>,
    address_ext: Option<u32>,
    alias: Option<String>,
    base_offset: Option<u32>,
    byte_order: Option<ByteOrderEnum>,
    color: Option<u32>,
    conversion: Option<ConversionAttribute>,
    description: Option<String>,
    dimension: Vec<u32>,
    event: Option<EventType>,
    group: Vec<GroupAttribute>,
    layout: Option<String>,
    split: Option<SplitType>,
    var_criterion: Option<String>,
}

// reduced set of attributes used by strings
#[derive(Debug, Default, Clone)]
struct StringAttributes {
    address: Option<u32>,
    address_ext: Option<u32>,
    alias: Option<String>,
    base_offset: Option<u32>,
    description: Option<String>,
    dimension: Vec<u32>,
    group: Vec<GroupAttribute>,
    split: Option<SplitType>,
    var_criterion: Option<String>,
}

// reduced set of attributes used by Maps, Curves, etc.
#[derive(Debug, Default, Clone)]
struct MapAttributes {
    address: Option<u32>,
    address_ext: Option<u32>,
    alias: Option<String>,
    base_offset: Option<u32>,
    byte_order: Option<ByteOrderEnum>,
    conversion: Option<ConversionAttribute>,
    description: Option<String>,
    group: Vec<GroupAttribute>,
    var_criterion: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct StructAttributes {
    dimension: Vec<u32>,
    base_offset: Option<u32>,
    size: Option<u32>,
    split: Option<SplitType>,
}

#[derive(Debug, Clone)]
enum GroupAttribute {
    In(Vec<String>),
    Out(Vec<String>),
    Def(Vec<String>),
    Std(Vec<String>),
}

#[derive(Debug, Clone)]
enum ConversionAttribute {
    Linear {
        factor: f64,
        offset: f64,
        unit: String,
        length: Option<u64>,
        digits: Option<u64>,
    },
    Formula {
        formula: String,
        inverse_formula: Option<String>,
        unit: String,
        length: Option<u64>,
        digits: Option<u64>,
    },
    Table {
        rows: Vec<TableRow>,
        default_value: Option<String>,
        format_values: Option<(u64, u64)>,
    },
    Reference {
        name: String,
        length: Option<u64>,
        digits: Option<u64>,
    },
    Unit {
        name: String,
        length: Option<u64>,
        digits: Option<u64>,
    },
}

#[derive(Debug, Clone)]
struct TableRow {
    value1: f64,
    value2: Option<f64>,
    text: String,
}

#[derive(Debug)]
struct Unit {
    name: String,
    length: u64,
    digits: u64,
}

#[derive(Debug, Clone)]
enum SplitType {
    Auto,
    Manual(Vec<String>),
    Template(String),
}

/* at the moment CCP / XCP events are not created, but they're defined so that the data model is complete */
#[allow(unused)]
#[derive(Debug, Clone)]
enum EventType {
    Ccp(u32),
    XcpFixed(u32),
    XcpVariable(Vec<u32>),
    XcpDefault(u32),
}

#[derive(Debug, Clone)]
enum AxisInfo {
    Standard {
        datatype: DataType,
        range: Option<(f64, f64)>,          // basic range
        extended_range: Option<(f64, f64)>, // extended range
        dimension: Vec<u32>,
        input_signal: Option<String>,
        input_is_instance: bool,
        conversion: Option<ConversionAttribute>,
    },
    FixList {
        axis_points: Vec<f64>,
        input_signal: Option<String>,
        input_is_instance: bool,
        conversion: Option<ConversionAttribute>,
    },
    FixRange {
        range_min: f64,
        range_max: f64,
        range_step: Option<f64>,
        input_signal: Option<String>,
        input_is_instance: bool,
        conversion: Option<ConversionAttribute>,
    },
    Common {
        ref_name: String,
        is_instance: bool,
    },
}

#[derive(Debug)]
struct Creator<'a2l> {
    module: &'a2l mut Module,
    main_group: String,
    main_group_description: Option<String>,
    sub_groups: HashMap<String, String>, // map: group name to description
    structures: HashMap<String, Structure>, // map: structure name to structure definition
    names: Vec<String>,                  // list of all used A2L names to check for duplicates
    version: A2lVersion,
    deferred_var_characteristic: Vec<(String, u32)>,
    var_criterion: HashMap<String, VarCriterionDefinition>,
    new_arrays: bool,
    enable_structures: bool,
    created_typedefs: HashSet<String>, // set of typedef names that were created by this code, to distinguish from pre-existing typedefs
    messages: Vec<String>,
    warnings: usize,
    errors: usize,
}

#[derive(Debug, Clone)]
struct Structure {
    elements: Vec<ElementDefinition>,
}

#[derive(Debug, Clone)]
struct InstanceElement<'a> {
    instance_name: &'a str,
    struct_path: &'a [String],
    instance_group: &'a Option<GroupAttribute>,
    overwrites: &'a Vec<Overwrite>,
}

struct SplitIterator<'a> {
    dimensions: &'a [u32],
    split: &'a SplitType,
    limit: u32,
    current_value: u32,
    base_a2l_name: &'a str,
    base_symbol_name: &'a str,
    use_new_arrays: bool,
}

// In "ASAP2 Creator" the prefix is configurable, but defaults to @@
// in practice it is always @@, so lets hardcode it for now.
static COMMENT_PREFIX: &[u8] = b"@@";

/// Create items in the given A2lFile from the specified source files.
///
/// Returns either Ok(warnings_count, log_messages) or Err(error_messages)
pub(crate) fn create_items_from_sources<'a>(
    a2l_file: &mut A2lFile,
    source_file_patterns: impl Iterator<Item = &'a OsString>,
    target_group: Option<String>,
    enable_structures: bool,
    force_old_arrays: bool,
) -> Result<(usize, Vec<String>), Vec<String>> {
    // This function will handle the creation of items from the source file
    // and return the count of inserted items along with any log messages.
    let mut creator = Creator::new(a2l_file, target_group, enable_structures, force_old_arrays);

    for source_file_pattern in source_file_patterns {
        // try to expand the pattern using glob, if the input is valid unicode, and if glob understands the pattern
        let expanded_filenames = if let Some(source_str) = source_file_pattern.to_str() {
            match glob::glob(source_str) {
                Ok(glob_iter) => glob_iter
                    .filter_map(Result::ok)
                    .map(OsString::from)
                    .collect::<Vec<_>>(),
                Err(pattern_error) => {
                    // glob pattern is invalid: log the error, and then try to proceed with the input as a single filename
                    creator.error(format!(
                        "Failed to expand glob pattern '{source_str}': {pattern_error}"
                    ));
                    vec![source_file_pattern.clone()]
                }
            }
        } else {
            // input is not valid unicode, so it can't be processed with glob: just use it as a single filename
            vec![source_file_pattern.clone()]
        };

        // if no files were found, log an error and continue with the next pattern
        if expanded_filenames.is_empty() {
            creator.warn(format!(
                "Warning: No files matched the pattern '{}'",
                source_file_pattern.to_string_lossy()
            ));
        }

        // for each expanded filename, try to read and process the file
        for source_file in expanded_filenames {
            let data = match std::fs::read(&source_file) {
                Ok(data) => data,
                Err(error) => {
                    creator.error(format!(
                        "Error reading source file '{}': {error}",
                        source_file.to_string_lossy(),
                    ));
                    continue;
                }
            };

            creator.messages.push(format!(
                "Processing source file '{}'",
                source_file.to_string_lossy()
            ));
            creator.process_file(&data);
        }
    }

    if creator.errors > 0 {
        Err(creator.messages)
    } else {
        Ok((creator.warnings, creator.messages))
    }
}

fn deftokens_to_string(definition_tokens: &[&[u8]]) -> String {
    definition_tokens
        .iter()
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

impl<'a2l> Creator<'a2l> {
    fn new(
        a2l_file: &'a2l mut A2lFile,
        target_group: Option<String>,
        enable_structures: bool,
        force_old_arrays: bool,
    ) -> Self {
        let main_group = target_group.unwrap_or_else(|| "CREATED".to_string());

        let version = A2lVersion::from(&*a2l_file);

        let module = &mut a2l_file.project.module[0];
        let mut names = module.characteristic.keys().cloned().collect::<Vec<_>>();
        names.extend(module.measurement.keys().cloned());
        names.extend(module.axis_pts.keys().cloned());
        names.extend(module.blob.keys().cloned());
        names.extend(module.instance.keys().cloned());

        Creator {
            module,
            main_group,
            main_group_description: None,
            sub_groups: HashMap::new(),
            structures: HashMap::new(),
            names,
            version,
            deferred_var_characteristic: Vec::new(),
            var_criterion: HashMap::new(),
            // new array indexing (e.g. [2]) is usable in 1.7.0 (possibly also 1.6.1?) and later
            new_arrays: !force_old_arrays && version >= A2lVersion::V1_7_0,
            // TYPEDEF_STRUCTURE and INSTANCE are only usable in 1.7.1 and later
            enable_structures: enable_structures && version >= A2lVersion::V1_7_1,
            created_typedefs: HashSet::new(),
            messages: Vec::new(),
            warnings: 0,
            errors: 0,
        }
    }

    fn process_file(&mut self, data: &[u8]) {
        let comment_scanner = scanner::CommentScanner::new(COMMENT_PREFIX);
        let creator_definitions = comment_scanner.scan_comments(data);

        for (offset, definition_tokens) in creator_definitions {
            let parse_result = parser::parse_definition(&definition_tokens);
            match parse_result {
                Ok(Some(definition)) => {
                    let def_result = self.process_definition(definition);
                    if let Err(error) = def_result {
                        let def_str: String = deftokens_to_string(&definition_tokens);
                        self.error(format!(
                        "Error processing definition at offset {offset}: {error} in definition: {def_str}"
                    ));
                    }
                }
                Ok(None) => {
                    // No definition recognized: no problem, just skip it
                }
                Err(error) => {
                    let def_text: String = deftokens_to_string(&definition_tokens);
                    self.error(format!(
                    "Error parsing definition at offset {offset}: {error} in definition: {def_text}",
                ));
                }
            }
        }
    }

    fn warn(&mut self, message: String) {
        self.warnings += 1;
        self.messages.push(message);
    }

    fn error(&mut self, message: String) {
        self.errors += 1;
        self.messages.push(message);
    }

    fn process_definition(&mut self, definition: Definition) -> Result<(), String> {
        match definition {
            Definition::Symbol(symbol_def) => self.process_item_definition(
                symbol_def.symbol_name,
                symbol_def.a2l_name,
                &symbol_def.config,
                None,
            ),
            Definition::SubGroup(sub_group) => {
                if let Some(description) = sub_group.description {
                    self.sub_groups.insert(sub_group.name, description);
                }
                Ok(())
            }
            Definition::MainGroup(main_group) => {
                self.main_group = main_group.name;
                self.main_group_description = main_group.description;
                Ok(())
            }
            Definition::Conversion(conversion) => self.process_conversion_definition(conversion),
            Definition::Element(element_def) => {
                self.update_struct(element_def);
                Ok(())
            }
            Definition::Instance(instance) => self.process_instance_definitions(instance),
            Definition::VarCriterion(var_criterion) => {
                self.process_var_criterion_definition(var_criterion)
            }
        }
    }

    /// process an item definition
    ///
    /// an item is either a SYMBOL or an ELEMENT, both of which use the ItemConfig to describe their properties
    fn process_item_definition(
        &mut self,
        symbol_name: String,
        a2l_name: String,
        config: &ItemConfig,
        instance_element: Option<&InstanceElement>,
    ) -> Result<(), String> {
        // Dispatch based on the item config
        self.check_a2l_name(&a2l_name)?;
        match config {
            ItemConfig::Measure(measure_cfg) => {
                self.create_measure_objects(a2l_name, symbol_name, measure_cfg, instance_element);
                Ok(())
            }
            ItemConfig::Parameter(parameter_cfg) => {
                self.create_parameter_objects(
                    a2l_name,
                    symbol_name,
                    parameter_cfg,
                    instance_element,
                );
                Ok(())
            }
            ItemConfig::CurveMap(curve_map_cfg) => {
                self.create_curve_map_object(a2l_name, symbol_name, curve_map_cfg, instance_element)
            }
            ItemConfig::Axis(axis_cfg) => {
                self.create_axis_object(a2l_name, symbol_name, axis_cfg, instance_element);
                Ok(())
            }
            ItemConfig::String(string_cfg) => {
                self.create_string_objects(a2l_name, symbol_name, string_cfg, instance_element);
                Ok(())
            }
            ItemConfig::SubStructure(sub_structure_cfg) => self.create_sub_structures(
                a2l_name,
                symbol_name,
                sub_structure_cfg,
                instance_element,
            ),
        }
    }

    fn check_a2l_name(&self, a2l_name: &str) -> Result<(), String> {
        if self.names.contains(&a2l_name.to_string()) {
            Err(format!("A2L name '{}' already exists", a2l_name))
        } else {
            Ok(())
        }
    }

    /// Create measurement objects from the configuration
    ///
    /// If the measure config has multiple dimensions and the split attribute is set,
    /// this function will create separate measurement objects for each dimension.
    fn create_measure_objects(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &MeasureCfg,
        instance_element: Option<&InstanceElement>,
    ) {
        if !config.attributes.dimension.is_empty()
            && let Some(split) = &config.attributes.split
        {
            // Split is set, create separate measurement objects for each dimension
            for (split_a2l_name, split_symbol_name) in SplitIterator::new(
                &config.attributes.dimension,
                split,
                &a2l_name,
                &symbol_name,
                self.new_arrays,
            ) {
                if self.check_a2l_name(&split_a2l_name).is_ok() {
                    self.create_measure_object(
                        split_a2l_name,
                        split_symbol_name,
                        config,
                        instance_element,
                        true,
                    );
                }
            }
        } else {
            // No split, create a single measurement object
            self.create_measure_object(a2l_name, symbol_name, config, instance_element, false)
        }
    }

    /// Create a single measurement object from the configuration
    /// This could be a split measurement, in which case `ignore_dimensions` is true.
    fn create_measure_object(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &MeasureCfg,
        instance_element: Option<&InstanceElement>,
        ignore_dimensions: bool,
    ) {
        // Create the measure object in the module
        let description =
            choose_description(config.attributes.description.as_deref(), instance_element);
        let datatype = config.datatype;
        let conversion = choose_conversion(&config.attributes.conversion, instance_element);
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(&a2l_name, conversion);
        let (lower_limit, upper_limit) = choose_range(&config.range, instance_element, &datatype);
        let address = config.attributes.address.unwrap_or(0);

        let mut meas = Measurement::new(
            a2l_name,
            description.to_string(),
            datatype,
            conversion_name,
            1,   // resolution is currently not used by any software
            0.0, // accuracy is currently not used by any software
            lower_limit,
            upper_limit,
        );
        let mut ecu_address = a2lfile::EcuAddress::new(address);
        ecu_address.get_layout_mut().item_location.0.1 = true; // set the "is hexadecimal" flag of the address to true
        meas.ecu_address = Some(ecu_address);
        meas.phys_unit = unit;
        meas.format = format;

        if let Some(address_ext) = config.attributes.address_ext {
            meas.ecu_address_extension =
                Some(a2lfile::EcuAddressExtension::new(address_ext as i16));
        }

        if let Some(bitmask) = config.bitmask {
            meas.bit_mask = Some(a2lfile::BitMask::new(bitmask));
        }
        if let Some(alias) = choose_alias(&config.attributes.alias, instance_element) {
            meas.display_identifier = Some(a2lfile::DisplayIdentifier::new(alias.to_string()));
        }

        let base_offset = config.attributes.base_offset.unwrap_or(0);
        if self.version < A2lVersion::V1_6_0 {
            // SYMBOL_LINK is not available, so we need to create an IF_DATA CANAPE_EXT instead
            meas.if_data.push(create_canape_ext(
                &symbol_name,
                address,
                config.attributes.address_ext,
                &base_offset,
            ));
        } else {
            // set SYMBOL_LINK
            meas.symbol_link = Some(a2lfile::SymbolLink::new(symbol_name, base_offset as i32));
        }

        if let Some(true) = config.write_access {
            meas.read_write = Some(a2lfile::ReadWrite::new());
        }

        if !ignore_dimensions && !config.attributes.dimension.is_empty() {
            let mut matrix_dim = a2lfile::MatrixDim::new();
            for dim in &config.attributes.dimension {
                matrix_dim.dim_list.push(*dim as u16);
            }
            if self.version < A2lVersion::V1_7_0 {
                // Ensure 3 dimensions are always present in old versions
                while matrix_dim.dim_list.len() < 3 {
                    matrix_dim.dim_list.push(1);
                }
                matrix_dim.dim_list.truncate(3);
            }
            meas.matrix_dim = Some(matrix_dim);
        }

        if let Some(byte_order) = config.attributes.byte_order {
            meas.byte_order = Some(a2lfile::ByteOrder::new(byte_order));
        }

        if let Some(layout) = &config.attributes.layout {
            match layout.as_str() {
                "ROW_DIR" => {
                    meas.layout = Some(a2lfile::Layout::new(a2lfile::IndexMode::RowDir));
                }
                "COLUMN_DIR" => {
                    meas.layout = Some(a2lfile::Layout::new(a2lfile::IndexMode::ColumnDir));
                }
                _ => {}
            }
        }

        self.handle_group_assignment(
            instance_element,
            &config.attributes.group,
            meas.get_name(),
            true,
        );

        self.module.measurement.push(meas);
    }

    /// Create parameter objects from the configuration
    ///
    /// If the parameter config has multiple dimensions and the split attribute is set,
    /// this function will create separate parameter objects for each dimension.
    fn create_parameter_objects(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &ParameterCfg,
        instance_element: Option<&InstanceElement>,
    ) {
        if !config.attributes.dimension.is_empty()
            && let Some(split) = &config.attributes.split
        {
            // Split is set, create separate parameter objects for each dimension
            for (split_a2l_name, split_symbol_name) in SplitIterator::new(
                &config.attributes.dimension,
                split,
                &a2l_name,
                &symbol_name,
                self.new_arrays,
            ) {
                if self.check_a2l_name(&split_a2l_name).is_ok() {
                    self.create_parameter_object(
                        split_a2l_name,
                        split_symbol_name,
                        config,
                        instance_element,
                        true,
                    );
                }
            }
        } else {
            // No split, create a single parameter object
            self.create_parameter_object(a2l_name, symbol_name, config, instance_element, false)
        }
    }

    fn create_parameter_object(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &ParameterCfg,
        instance_element: Option<&InstanceElement>,
        ignore_dimensions: bool,
    ) {
        // Create the characteristic object in the module
        let description =
            choose_description(config.attributes.description.as_deref(), instance_element);
        let datatype = config.datatype;
        let conversion = choose_conversion(&config.attributes.conversion, instance_element);
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(&a2l_name, conversion);
        let (lower_limit, upper_limit) = choose_range(&config.range, instance_element, &datatype);
        let address = config.attributes.address.unwrap_or(0);

        let chara_type = if !ignore_dimensions && !config.attributes.dimension.is_empty() {
            a2lfile::CharacteristicType::ValBlk
        } else {
            a2lfile::CharacteristicType::Value
        };

        let record_layout = if let Some(layout) = &config.attributes.layout {
            layout.clone()
        } else {
            self.create_default_record_layout(&datatype)
        };

        let mut characteristic = a2lfile::Characteristic::new(
            a2l_name.clone(),
            description.to_string(),
            chara_type,
            address,
            record_layout,
            0.0,
            conversion_name,
            lower_limit,
            upper_limit,
        );
        characteristic.get_layout_mut().item_location.3.1 = true; // set the "is hexadecimal" flag of the address to true
        characteristic.phys_unit = unit;
        characteristic.format = format;

        if let Some(address_ext) = config.attributes.address_ext {
            characteristic.ecu_address_extension =
                Some(a2lfile::EcuAddressExtension::new(address_ext as i16));
        }

        if let Some(bitmask) = config.bitmask {
            characteristic.bit_mask = Some(a2lfile::BitMask::new(bitmask));
        }
        if let Some(alias) = choose_alias(&config.attributes.alias, instance_element) {
            characteristic.display_identifier =
                Some(a2lfile::DisplayIdentifier::new(alias.to_string()));
        }

        let base_offset = config.attributes.base_offset.unwrap_or(0);
        if self.version < A2lVersion::V1_6_0 {
            // SYMBOL_LINK is not available, so we need to create an IF_DATA CANAPE_EXT instead
            characteristic.if_data.push(create_canape_ext(
                &symbol_name,
                address,
                config.attributes.address_ext,
                &base_offset,
            ));
        } else {
            // set SYMBOL_LINK
            characteristic.symbol_link =
                Some(a2lfile::SymbolLink::new(symbol_name, base_offset as i32));
        }

        if let Some(false) = config.write_access {
            characteristic.read_only = Some(a2lfile::ReadOnly::new());
        }

        if !ignore_dimensions && !config.attributes.dimension.is_empty() {
            let mut matrix_dim = a2lfile::MatrixDim::new();
            for dim in &config.attributes.dimension {
                matrix_dim.dim_list.push(*dim as u16);
            }
            if self.version < A2lVersion::V1_7_0 {
                // Ensure 3 dimensions are always present in old versions
                while matrix_dim.dim_list.len() < 3 {
                    matrix_dim.dim_list.push(1);
                }
                matrix_dim.dim_list.truncate(3);
            }
            characteristic.matrix_dim = Some(matrix_dim);
        }

        if let Some((lower, upper)) = config.extended_range {
            characteristic.extended_limits = Some(a2lfile::ExtendedLimits::new(lower, upper));
        }

        if let Some(byte_order) = config.attributes.byte_order {
            characteristic.byte_order = Some(a2lfile::ByteOrder::new(byte_order));
        }

        self.handle_group_assignment(
            instance_element,
            &config.attributes.group,
            characteristic.get_name(),
            false,
        );

        self.module.characteristic.push(characteristic);

        // create a VAR_CHARACTERISTIC that references the named VAR_CRITERION
        if let Some(var_criterion_name) = &config.attributes.var_criterion {
            self.create_var_characteristic(a2l_name, var_criterion_name, address);
        }
    }

    fn create_curve_map_object(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &CurveMapCfg,
        instance_element: Option<&InstanceElement>,
    ) -> Result<(), String> {
        // Create the characteristic object in the module
        let description = config.attributes.description.as_deref().unwrap_or("");
        let datatype = config.datatype;
        let conversion = choose_conversion(&config.attributes.conversion, instance_element);
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(&a2l_name, conversion);
        let (lower_limit, upper_limit) = config.range.unwrap_or_else(|| datatype_limits(&datatype));
        let address = config.attributes.address.unwrap_or(0);

        let chara_type = if config.y_axis.is_some() {
            a2lfile::CharacteristicType::Map
        } else {
            a2lfile::CharacteristicType::Curve
        };

        let mut characteristic = a2lfile::Characteristic::new(
            a2l_name.clone(),
            description.to_string(),
            chara_type,
            address,
            config.layout.clone(),
            0.0,
            conversion_name,
            lower_limit,
            upper_limit,
        );

        // if this is an instance element, then a base name may be needed for the input signal
        let base_name = instance_element.and_then(|_| make_base_name(&a2l_name));
        let x_axis_name = format!("{}.XAxis", a2l_name);
        characteristic.axis_descr.push(self.create_axis_descr(
            &config.x_axis,
            &x_axis_name,
            base_name,
        )?);
        if let Some(y_axis) = &config.y_axis {
            let y_axis_name = format!("{}.YAxis", a2l_name);
            characteristic.axis_descr.push(self.create_axis_descr(
                y_axis,
                &y_axis_name,
                base_name,
            )?);
        }

        characteristic.get_layout_mut().item_location.3.1 = true; // set the "is hexadecimal" flag of the address to true
        characteristic.phys_unit = unit;
        characteristic.format = format;

        if let Some(address_ext) = config.attributes.address_ext {
            characteristic.ecu_address_extension =
                Some(a2lfile::EcuAddressExtension::new(address_ext as i16));
        }

        if let Some(bitmask) = config.bitmask {
            characteristic.bit_mask = Some(a2lfile::BitMask::new(bitmask));
        }
        if let Some(alias) = &config.attributes.alias {
            characteristic.display_identifier =
                Some(a2lfile::DisplayIdentifier::new(alias.clone()));
        }

        let base_offset = config.attributes.base_offset.unwrap_or(0);
        if self.version < A2lVersion::V1_6_0 {
            // SYMBOL_LINK is not available, so we need to create an IF_DATA CANAPE_EXT instead
            characteristic.if_data.push(create_canape_ext(
                &symbol_name,
                address,
                config.attributes.address_ext,
                &base_offset,
            ));
        } else {
            // set SYMBOL_LINK
            characteristic.symbol_link =
                Some(a2lfile::SymbolLink::new(symbol_name, base_offset as i32));
        }

        if let Some(false) = config.write_access {
            characteristic.read_only = Some(a2lfile::ReadOnly::new());
        }

        if let Some((lower, upper)) = config.extended_range {
            characteristic.extended_limits = Some(a2lfile::ExtendedLimits::new(lower, upper));
        }

        if let Some(byte_order) = config.attributes.byte_order {
            characteristic.byte_order = Some(a2lfile::ByteOrder::new(byte_order));
        }

        self.handle_group_assignment(
            instance_element,
            &config.attributes.group,
            characteristic.get_name(),
            false,
        );

        self.module.characteristic.push(characteristic);

        // create a VAR_CHARACTERISTIC that references the named VAR_CRITERION
        if let Some(var_criterion_name) = &config.attributes.var_criterion {
            self.create_var_characteristic(a2l_name, var_criterion_name, address);
        }
        Ok(())
    }

    fn create_axis_object(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &AxisCfg,
        instance_element: Option<&InstanceElement<'_>>,
    ) {
        let description =
            choose_description(config.attributes.description.as_deref(), instance_element);
        let address = config.attributes.address.unwrap_or(0);
        // if this is an instance element, then a base name may be needed for the input signal
        let base_name = instance_element.and_then(|_| make_base_name(&a2l_name));
        let input =
            build_input_signal_name(base_name, &config.input_signal, config.input_is_instance);

        let conversion = choose_conversion(&config.attributes.conversion, instance_element);
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(&a2l_name, conversion);
        let (lower_limit, upper_limit) =
            choose_range(&config.range, instance_element, &config.datatype);

        let mut axis_pts = a2lfile::AxisPts::new(
            a2l_name.to_string(),
            description.to_string(),
            address,
            input,
            config.layout.clone(),
            0.0,
            conversion_name,
            config.dimension[0] as u16,
            lower_limit,
            upper_limit,
        );
        axis_pts.phys_unit = unit;
        axis_pts.format = format;

        if let Some(address_ext) = config.attributes.address_ext {
            axis_pts.ecu_address_extension =
                Some(a2lfile::EcuAddressExtension::new(address_ext as i16));
        }

        if let Some(alias) = choose_alias(&config.attributes.alias, instance_element) {
            axis_pts.display_identifier = Some(a2lfile::DisplayIdentifier::new(alias.to_string()));
        }

        let base_offset = config.attributes.base_offset.unwrap_or(0);
        if self.version < A2lVersion::V1_6_0 {
            // SYMBOL_LINK is not available, so we need to create an IF_DATA CANAPE_EXT instead
            axis_pts.if_data.push(create_canape_ext(
                &symbol_name,
                address,
                config.attributes.address_ext,
                &base_offset,
            ));
        } else {
            // set SYMBOL_LINK
            axis_pts.symbol_link = Some(a2lfile::SymbolLink::new(symbol_name, base_offset as i32));
        }

        if let Some(false) = config.write_access {
            axis_pts.read_only = Some(a2lfile::ReadOnly::new());
        }

        if let Some((lower, upper)) = config.extended_range {
            axis_pts.extended_limits = Some(a2lfile::ExtendedLimits::new(lower, upper));
        }

        if let Some(byte_order) = config.attributes.byte_order {
            axis_pts.byte_order = Some(a2lfile::ByteOrder::new(byte_order));
        }

        self.handle_group_assignment(
            instance_element,
            &config.attributes.group,
            axis_pts.get_name(),
            false,
        );

        self.module.axis_pts.push(axis_pts);

        // create a VAR_CHARACTERISTIC that references the named VAR_CRITERION
        if let Some(var_criterion_name) = &config.attributes.var_criterion {
            self.create_var_characteristic(a2l_name, var_criterion_name, address);
        }
    }

    fn create_string_objects(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &StringCfg,
        instance_element: Option<&InstanceElement<'_>>,
    ) {
        if !config.attributes.dimension.is_empty()
            && let Some(split) = &config.attributes.split
        {
            // Split is set, create separate measurement objects for each dimension
            for (split_a2l_name, split_symbol_name) in SplitIterator::new(
                &config.attributes.dimension,
                split,
                &a2l_name,
                &symbol_name,
                self.new_arrays,
            ) {
                if self.check_a2l_name(&split_a2l_name).is_ok() {
                    self.create_string_object(
                        split_a2l_name,
                        split_symbol_name,
                        config,
                        instance_element,
                    );
                }
            }
        } else {
            // No split, create a single measurement object
            self.create_string_object(a2l_name, symbol_name, config, instance_element);
        }
    }

    fn create_string_object(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        config: &StringCfg,
        instance_element: Option<&InstanceElement<'_>>,
    ) {
        // Create the characteristic object in the module
        let description =
            choose_description(config.attributes.description.as_deref(), instance_element);
        let address = config.attributes.address.unwrap_or(0);

        let record_layout = self.create_default_record_layout(&DataType::Ubyte);

        let mut characteristic = a2lfile::Characteristic::new(
            a2l_name.clone(),
            description.to_string(),
            a2lfile::CharacteristicType::Ascii,
            address,
            record_layout,
            0.0,
            "NO_COMPU_METHOD".to_string(),
            0.0,
            255.0,
        );
        characteristic.get_layout_mut().item_location.3.1 = true; // set the "is hexadecimal" flag of the address to true
        characteristic.number = Some(a2lfile::Number::new(config.length as u16));

        if let Some(address_ext) = config.attributes.address_ext {
            characteristic.ecu_address_extension =
                Some(a2lfile::EcuAddressExtension::new(address_ext as i16));
        }

        if let Some(alias) = choose_alias(&config.attributes.alias, instance_element) {
            characteristic.display_identifier =
                Some(a2lfile::DisplayIdentifier::new(alias.to_string()));
        }

        let base_offset = config.attributes.base_offset.unwrap_or(0);
        if self.version < A2lVersion::V1_6_0 {
            // SYMBOL_LINK is not available, so we need to create an IF_DATA CANAPE_EXT instead
            characteristic.if_data.push(create_canape_ext(
                &symbol_name,
                address,
                config.attributes.address_ext,
                &base_offset,
            ));
        } else {
            // set SYMBOL_LINK
            characteristic.symbol_link =
                Some(a2lfile::SymbolLink::new(symbol_name, base_offset as i32));
        }

        if let Some(false) = config.write_access {
            characteristic.read_only = Some(a2lfile::ReadOnly::new());
        }

        self.handle_group_assignment(
            instance_element,
            &config.attributes.group,
            characteristic.get_name(),
            false,
        );

        self.module.characteristic.push(characteristic);

        // create a VAR_CHARACTERISTIC that references the named VAR_CRITERION
        if let Some(var_criterion_name) = &config.attributes.var_criterion {
            self.create_var_characteristic(a2l_name, var_criterion_name, address);
        }
    }

    /// Create instance objects based on the configuration
    ///
    /// If the config has multiple dimensions and the split attribute is set,
    /// this function will create separate instance objects for each dimension.
    fn process_instance_definitions(&mut self, instance: InstanceDefinition) -> Result<(), String> {
        if self.enable_structures {
            // create an INSTANCE that depends on a TYPEDEF_STRUCTURE
            self.process_instance_definitions_as_structures(instance)
        } else {
            // create separate objects (CHARACTERISTIC, MEASUREMENT, AXIS_PTS) for each element in the structure
            self.process_instance_definitions_separately(instance)
        }
    }

    //#########################################################################
    // Methods for processing instance definitions as separate objects
    //
    // This is the "old" way of doing things, without TYPEDEF_STRUCTURE and INSTANCE
    // - process_instance_definitions_separately
    // - create_sub_structures
    // - create_sub_structure_items
    //#########################################################################

    /// Process instance definitions by creating separate objects for each element in the structure
    fn process_instance_definitions_separately(
        &mut self,
        instance: InstanceDefinition,
    ) -> Result<(), String> {
        // if the a2l name is not set explicitly then it is identical to the symbol name
        let a2l_name = instance.a2l_name.clone().unwrap_or(instance.name.clone());
        let symbol_name = instance.name.clone();

        let sub_structure_cfg = &SubStructureCfg {
            data_type_struct: Some(instance.structure_name.clone()),
            attributes: StructAttributes::default(),
        };
        // creating an instance is _almost_ the same as creating a sub-structure
        // key difference: the instance_name must be extended with an array index if the instance has multiple dimensions
        // create_sub_structures does not do this
        if !instance.dimension.is_empty() {
            let split = if let Some(split) = &instance.split {
                split
            } else {
                // split is mandatory for instances when --enable-structures is not set
                self.warn(format!(
                    "Warning: Multi-dimensional instance '{}' must have the SPLIT attribute.",
                    instance.name
                ));
                &SplitType::Auto
            };
            let struct_path = &[instance.structure_name];
            // Split is set, create separate sets of instance objects for each dimension
            for (split_a2l_name, split_symbol_name) in SplitIterator::new(
                &instance.dimension,
                split,
                &a2l_name,
                &symbol_name,
                self.new_arrays,
            ) {
                let mut instance_element = InstanceElement {
                    instance_name: &a2l_name,
                    struct_path,
                    instance_group: &instance.group,
                    overwrites: &instance.overwrites,
                };
                let split_a2l_name_copy = split_a2l_name.clone();
                instance_element.instance_name = &split_a2l_name_copy;
                if self.check_a2l_name(&split_a2l_name).is_ok() {
                    let result = self.create_sub_structure_items(
                        split_a2l_name,
                        split_symbol_name,
                        sub_structure_cfg,
                        Some(&instance_element),
                    );
                    if let Err(error) = result {
                        self.error(error);
                    }
                }
            }
            Ok(())
        } else {
            // No split, instantiate objects for a single instance
            let instance_element = InstanceElement {
                instance_name: &a2l_name,
                struct_path: &[instance.structure_name],
                instance_group: &instance.group,
                overwrites: &instance.overwrites,
            };
            self.create_sub_structure_items(
                a2l_name.clone(),
                symbol_name,
                sub_structure_cfg,
                Some(&instance_element),
            )
        }
    }

    /// Create sub-structure objects based on the configuration
    fn create_sub_structures(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        sub_structure_cfg: &SubStructureCfg,
        instance_element: Option<&InstanceElement>,
    ) -> Result<(), String> {
        // Create the sub-structures based on the configuration
        if !sub_structure_cfg.attributes.dimension.is_empty()
            && let Some(split) = &sub_structure_cfg.attributes.split
        {
            // Split is set, create separate sets of instance objects for each dimension
            for (split_a2l_name, split_symbol_name) in SplitIterator::new(
                &sub_structure_cfg.attributes.dimension,
                split,
                &a2l_name,
                &symbol_name,
                self.new_arrays,
            ) {
                if self.check_a2l_name(&split_a2l_name).is_ok() {
                    let result = self.create_sub_structure_items(
                        split_a2l_name,
                        split_symbol_name,
                        sub_structure_cfg,
                        instance_element,
                    );
                    if let Err(error) = result {
                        self.error(error);
                    }
                }
            }
            Ok(())
        } else {
            // No split, instantiate objects for a single instance
            self.create_sub_structure_items(
                a2l_name,
                symbol_name,
                sub_structure_cfg,
                instance_element,
            )
        }
    }

    fn create_sub_structure_items(
        &mut self,
        a2l_name: String,
        symbol_name: String,
        sub_structure_cfg: &SubStructureCfg,
        instance_element: Option<&InstanceElement>,
    ) -> Result<(), String> {
        let Some(instance_element) = instance_element else {
            // impossible, sub-structures are always part of an instance
            return Err("Impossible: sub-structures are always part of an instance".into());
        };

        // determine the full structure name, either from the config or from the instance path
        let full_struct_name = sub_structure_cfg
            .data_type_struct
            .clone()
            .unwrap_or_else(|| instance_element.struct_path.join("."));

        // find the structure for the instance
        let Some(structure) = self.structures.get(&full_struct_name).cloned() else {
            return Err(format!(
                "Structure '{}' not found for instance '{}'",
                full_struct_name, instance_element.instance_name
            ));
        };

        for struct_item in &structure.elements {
            let mut new_struct_path = struct_item.structure.clone();
            new_struct_path.push(struct_item.symbol_name.clone());
            // create full names for the item. Ex: Item c in struct a.b -> a.b.c
            let full_symbol_name = format!("{symbol_name}.{}", struct_item.symbol_name);
            let full_a2l_name = format!("{a2l_name}.{}", struct_item.a2l_name);
            // create a new InstanceElement which has the correct struct_path - all other elements are copied
            let instance_sub_element = InstanceElement {
                struct_path: &new_struct_path,
                ..*instance_element
            };
            let result = self.process_item_definition(
                full_symbol_name,
                full_a2l_name,
                &struct_item.config,
                Some(&instance_sub_element),
            );
            if let Err(error) = result {
                self.error(format!(
                    "Error processing item '{}' in instance '{}': {error}",
                    struct_item.a2l_name, instance_element.instance_name
                ));
            }
        }

        Ok(())
    }

    //#########################################################################
    // Methods to create TYPEDEF_STRUCTURE and INSTANCE objects

    fn process_instance_definitions_as_structures(
        &mut self,
        instance: InstanceDefinition,
    ) -> Result<(), String> {
        // Note:
        // There is a mismatch in the expressiveness of A2L and the configuration format.
        // Even though an A2L file could have an INSTANCE of a simple item, e.g. an INSTANCE of a TYPEDEF_MEASUREMENT,
        // the configuration format cannot express this.
        // As a result we know here that the INSTANCE always refers to a TYPEDEF_STRUCTURE.

        // if the a2l name is not set explicitly then it is identical to the symbol name
        let a2l_name = instance.a2l_name.clone().unwrap_or(instance.name.clone());
        self.check_a2l_name(&a2l_name)?;

        let symbol_name = instance.name.clone();
        let address = instance.address.unwrap_or(0);

        // create INSTANCE
        let mut instance_obj = a2lfile::Instance::new(
            a2l_name.clone(),
            String::new(),
            instance.structure_name.clone(),
            address,
        );

        // the definition format for INSTANCEs does not allow a BASE_OFFSET to be specified
        // additionally, INSTANCEs only exist in A2L versions >= 1.7.0, so there is no need to handle old versions here
        instance_obj.symbol_link = Some(a2lfile::SymbolLink::new(symbol_name, 0));

        if !instance.dimension.is_empty() {
            let mut matrix_dim = a2lfile::MatrixDim::new();
            for dim in &instance.dimension {
                matrix_dim.dim_list.push(*dim as u16);
            }
            instance_obj.matrix_dim = Some(matrix_dim);
        }

        // Handle OVERWRITE definitions
        for overwrite_def in &instance.overwrites {
            let ov_path = overwrite_def.element_path.join(".");
            if !instance_obj.overwrite.contains_key(&ov_path) {
                // the definition format does not provide an axis number, so 0 is hard-coded here
                let ov = a2lfile::Overwrite::new(ov_path.clone(), 0);
                instance_obj.overwrite.push(ov);
            }
            let overwrite = instance_obj.overwrite.get_mut(&ov_path).unwrap();
            match &overwrite_def.details {
                OverwriteSpec::Conversion(conversion_attribute) => {
                    let parent_name = format!("{a2l_name}.{ov_path}");
                    let (conversion_name, _, _) =
                        self.handle_conversion_attribute(&parent_name, Some(conversion_attribute));
                    overwrite.conversion = Some(a2lfile::Conversion::new(conversion_name));
                }
                OverwriteSpec::Range(lower, upper) => {
                    let limits = a2lfile::Limits::new(*lower, *upper);
                    overwrite.limits = Some(limits);
                }
                OverwriteSpec::Description(_)
                | OverwriteSpec::Alias(_)
                | OverwriteSpec::Color(_)
                | OverwriteSpec::GroupAssignment(_) => { /* not supported in this mode */ }
            }
        }

        self.module.instance.push(instance_obj);

        // create TYPEDEF_STRUCTURE for the INSTANCE if it does not already exist
        if !self
            .module
            .typedef_structure
            .contains_key(&instance.structure_name)
        {
            self.verify_structure(&instance.structure_name)?;
            self.create_typedef_structure(&instance.structure_name, None)?;
        } else if !self.created_typedefs.contains(&instance.structure_name) {
            // already exists, but was not created by us
            // this is not supposed to happen, and we don't know that the existing structure is compatible
            return Err(format!(
                "TYPEDEF_STRUCTURE '{}' already exists",
                instance.structure_name
            ));
        }

        Ok(())
    }

    /// Verify that the structure is valid according to the A2L rule for TYPEDEF_STRUCTUREs
    /// It must not contain both measurement and parameter items (including in sub-structures)
    fn verify_structure(&self, struct_name: &str) -> Result<(u32, u32), String> {
        let mut measurements = 0;
        let mut characteristics = 0;

        let Some(structure_def) = self.structures.get(struct_name) else {
            return Err(format!("Structure '{}' not found", struct_name));
        };

        for item in &structure_def.elements {
            match &item.config {
                ItemConfig::Measure(_) => measurements += 1,
                ItemConfig::Parameter(_) => characteristics += 1,
                ItemConfig::CurveMap(_) => characteristics += 1,
                ItemConfig::Axis(_) => characteristics += 1,
                ItemConfig::String(_) => characteristics += 1,
                ItemConfig::SubStructure(sub_struct_cfg) => {
                    let mut struct_path = item.structure.clone();
                    struct_path.push(item.symbol_name.clone());
                    let sub_struct_name = sub_struct_cfg
                        .data_type_struct
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| struct_path.join("."));
                    let (sub_measurements, sub_characteristics) =
                        self.verify_structure(&sub_struct_name)?;
                    measurements += sub_measurements;
                    characteristics += sub_characteristics;
                }
            }
        }
        if measurements > 0 && characteristics > 0 {
            return Err(format!(
                "Structure '{}' contains both MEASURE and PARAMETER items, which is not allowed",
                struct_name
            ));
        }

        Ok((measurements, characteristics))
    }

    fn create_typedef_structure(
        &mut self,
        struct_name: &str,
        size: Option<u32>,
    ) -> Result<(), String> {
        let Some(structure_def) = self.structures.get(struct_name).cloned() else {
            return Err(format!("Structure '{}' not found", struct_name));
        };

        let size = size.unwrap_or(self.estimate_size(&structure_def));

        // first, create the TYPEDEF_STRUCTURE, with all of its STRUCTURE_COMPONENTs
        let mut typedef_structure =
            a2lfile::TypedefStructure::new(struct_name.to_string(), String::new(), size);

        for item in &structure_def.elements {
            let mut item_path = item.structure.clone();
            item_path.push(item.symbol_name.clone());
            let item_fullname = item_path.join(".");
            match &item.config {
                ItemConfig::Measure(_) => {
                    let struct_item =
                        a2lfile::StructureComponent::new(item.a2l_name.clone(), item_fullname, 0);
                    typedef_structure.structure_component.push(struct_item);
                }
                ItemConfig::Parameter(_) => {
                    let struct_item =
                        a2lfile::StructureComponent::new(item.a2l_name.clone(), item_fullname, 0);
                    typedef_structure.structure_component.push(struct_item);
                }
                ItemConfig::CurveMap(_) => {
                    let struct_item =
                        a2lfile::StructureComponent::new(item.a2l_name.clone(), item_fullname, 0);
                    typedef_structure.structure_component.push(struct_item);
                }
                ItemConfig::Axis(_) => {
                    let struct_item =
                        a2lfile::StructureComponent::new(item.a2l_name.clone(), item_fullname, 0);
                    typedef_structure.structure_component.push(struct_item);
                }
                ItemConfig::String(_) => {
                    let struct_item =
                        a2lfile::StructureComponent::new(item.a2l_name.clone(), item_fullname, 0);
                    typedef_structure.structure_component.push(struct_item);
                }
                ItemConfig::SubStructure(sub_structure_cfg) => {
                    let sub_struct_name = sub_structure_cfg
                        .data_type_struct
                        .as_ref()
                        .cloned()
                        .unwrap_or(item_fullname);
                    let mut struct_item = a2lfile::StructureComponent::new(
                        item.a2l_name.clone(),
                        sub_struct_name.clone(),
                        0,
                    );
                    if !sub_structure_cfg.attributes.dimension.is_empty() {
                        let mut matrix_dim = a2lfile::MatrixDim::new();
                        for dim in &sub_structure_cfg.attributes.dimension {
                            matrix_dim.dim_list.push(*dim as u16);
                        }
                        struct_item.matrix_dim = Some(matrix_dim);
                    }
                    typedef_structure.structure_component.push(struct_item);
                }
            }
        }

        self.module.typedef_structure.push(typedef_structure);
        self.created_typedefs.insert(struct_name.to_string());

        // second, create all the TYPEDEFs for the items in the structure
        for item in &structure_def.elements {
            let mut item_path = item.structure.clone();
            item_path.push(item.symbol_name.clone());
            let item_fullname = item_path.join(".");
            match &item.config {
                ItemConfig::Measure(measure_cfg) => {
                    self.create_typedef_measurement(&item_fullname, measure_cfg)?;
                }
                ItemConfig::Parameter(parameter_cfg) => {
                    self.create_typedef_characteristic(&item_fullname, parameter_cfg)?;
                }
                ItemConfig::CurveMap(curve_map_cfg) => {
                    self.create_typedef_characteristic_map(&item_fullname, curve_map_cfg)?;
                }
                ItemConfig::Axis(axis_cfg) => {
                    self.create_typedef_axis(&item_fullname, axis_cfg)?;
                }
                ItemConfig::String(string_cfg) => {
                    self.create_typedef_characteristic_string(&item_fullname, string_cfg)?;
                }
                ItemConfig::SubStructure(sub_structure_cfg) => {
                    let sub_struct_name = sub_structure_cfg
                        .data_type_struct
                        .as_ref()
                        .cloned()
                        .unwrap_or(item_fullname);
                    if !self.module.typedef_structure.contains_key(&sub_struct_name) {
                        // no need to verify the structure again, it was already verified as part of the instance
                        self.create_typedef_structure(
                            &sub_struct_name,
                            sub_structure_cfg.attributes.size,
                        )?;
                    } else if !self.created_typedefs.contains(&sub_struct_name) {
                        // already exists, but was not created by us
                        // this is not supposed to happen, and we don't know that the existing structure is compatible
                        return Err(format!(
                            "TYPEDEF_STRUCTURE '{}' already exists",
                            sub_struct_name
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    fn create_typedef_measurement(
        &mut self,
        full_name: &str,
        config: &MeasureCfg,
    ) -> Result<(), String> {
        if self.module.typedef_measurement.contains_key(full_name)
            && !self.created_typedefs.contains(full_name)
        {
            // already exists, but was not created by us
            return Err(format!("TYPEDEF_MEASUREMENT '{full_name}' already exists"));
        }

        self.check_a2l_name(full_name)?;

        let description = config.attributes.description.as_deref().unwrap_or("");
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(full_name, config.attributes.conversion.as_ref());
        let (lower_limit, upper_limit) = config
            .range
            .unwrap_or_else(|| datatype_limits(&config.datatype));

        let mut td_measurement = a2lfile::TypedefMeasurement::new(
            full_name.to_string(),
            description.to_string(),
            config.datatype,
            conversion_name,
            0,
            0.0,
            lower_limit,
            upper_limit,
        );

        td_measurement.phys_unit = unit;
        td_measurement.format = format;
        td_measurement.bit_mask = config.bitmask.map(a2lfile::BitMask::new);
        td_measurement.byte_order = config.attributes.byte_order.map(a2lfile::ByteOrder::new);

        if !config.attributes.dimension.is_empty() {
            let mut matrix_dim = a2lfile::MatrixDim::new();
            for dim in &config.attributes.dimension {
                matrix_dim.dim_list.push(*dim as u16);
            }
            td_measurement.matrix_dim = Some(matrix_dim);
        }
        self.module.typedef_measurement.push(td_measurement);

        Ok(())
    }

    fn create_typedef_characteristic(
        &mut self,
        full_name: &str,
        config: &ParameterCfg,
    ) -> Result<(), String> {
        if self.module.typedef_characteristic.contains_key(full_name)
            && !self.created_typedefs.contains(full_name)
        {
            // already exists, but was not created by us
            // this is not supposed to happen, and we don't know that the existing structure is compatible
            return Err(format!(
                "TYPEDEF_CHARACTERISTIC '{full_name}' already exists"
            ));
        }

        self.check_a2l_name(full_name)?;

        let description = config.attributes.description.as_deref().unwrap_or("");
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(full_name, config.attributes.conversion.as_ref());
        let (lower_limit, upper_limit) = config
            .range
            .unwrap_or_else(|| datatype_limits(&config.datatype));

        let record_layout = if let Some(layout) = &config.attributes.layout {
            layout.clone()
        } else {
            self.create_default_record_layout(&config.datatype)
        };

        let mut td_characteristic = a2lfile::TypedefCharacteristic::new(
            full_name.to_string(),
            description.to_string(),
            a2lfile::CharacteristicType::Value,
            record_layout,
            0.0,
            conversion_name,
            lower_limit,
            upper_limit,
        );

        td_characteristic.phys_unit = unit;
        td_characteristic.format = format;
        td_characteristic.bit_mask = config.bitmask.map(a2lfile::BitMask::new);
        td_characteristic.byte_order = config.attributes.byte_order.map(a2lfile::ByteOrder::new);

        if let Some((lower, upper)) = config.extended_range {
            td_characteristic.extended_limits = Some(a2lfile::ExtendedLimits::new(lower, upper));
        }

        if !config.attributes.dimension.is_empty() {
            let mut matrix_dim = a2lfile::MatrixDim::new();
            for dim in &config.attributes.dimension {
                matrix_dim.dim_list.push(*dim as u16);
            }
            td_characteristic.matrix_dim = Some(matrix_dim);
        }
        self.module.typedef_characteristic.push(td_characteristic);

        Ok(())
    }

    fn create_typedef_characteristic_map(
        &mut self,
        full_name: &str,
        config: &CurveMapCfg,
    ) -> Result<(), String> {
        if self.module.typedef_characteristic.contains_key(full_name)
            && !self.created_typedefs.contains(full_name)
        {
            // already exists, but was not created by us
            // this is not supposed to happen, and we don't know that the existing structure is compatible
            return Err(format!(
                "TYPEDEF_CHARACTERISTIC '{full_name}' already exists"
            ));
        }

        self.check_a2l_name(full_name)?;

        let description = config.attributes.description.as_deref().unwrap_or("");
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(full_name, config.attributes.conversion.as_ref());
        let (lower_limit, upper_limit) = config
            .range
            .unwrap_or_else(|| datatype_limits(&config.datatype));

        let mut td_characteristic = a2lfile::TypedefCharacteristic::new(
            full_name.to_string(),
            description.to_string(),
            a2lfile::CharacteristicType::Map,
            config.layout.clone(),
            0.0,
            conversion_name,
            lower_limit,
            upper_limit,
        );

        td_characteristic.phys_unit = unit;
        td_characteristic.format = format;
        td_characteristic.bit_mask = config.bitmask.map(a2lfile::BitMask::new);
        td_characteristic.byte_order = config.attributes.byte_order.map(a2lfile::ByteOrder::new);

        if let Some((lower, upper)) = config.extended_range {
            td_characteristic.extended_limits = Some(a2lfile::ExtendedLimits::new(lower, upper));
        }

        let base_name = Some("THIS");
        let x_axis_name = format!("{}.XAxis", full_name);
        td_characteristic.axis_descr.push(self.create_axis_descr(
            &config.x_axis,
            &x_axis_name,
            base_name,
        )?);
        if let Some(y_axis) = &config.y_axis {
            let y_axis_name = format!("{}.YAxis", full_name);
            td_characteristic.axis_descr.push(self.create_axis_descr(
                y_axis,
                &y_axis_name,
                base_name,
            )?);
        }

        self.module.typedef_characteristic.push(td_characteristic);

        Ok(())
    }

    fn create_typedef_characteristic_string(
        &mut self,
        full_name: &str,
        config: &StringCfg,
    ) -> Result<(), String> {
        if self.module.typedef_characteristic.contains_key(full_name)
            && !self.created_typedefs.contains(full_name)
        {
            // already exists, but was not created by us
            return Err(format!(
                "TYPEDEF_CHARACTERISTIC '{full_name}' already exists"
            ));
        }

        self.check_a2l_name(full_name)?;

        let description = config.attributes.description.as_deref().unwrap_or("");
        let record_layout = self.create_default_record_layout(&DataType::Ubyte);

        let mut td_characteristic = a2lfile::TypedefCharacteristic::new(
            full_name.to_string(),
            description.to_string(),
            a2lfile::CharacteristicType::Ascii,
            record_layout,
            0.0,
            "NO_COMPU_METHOD".to_string(),
            0.0,
            255.0,
        );
        td_characteristic.number = Some(a2lfile::Number::new(config.length as u16));

        if !config.attributes.dimension.is_empty() {
            let mut matrix_dim = a2lfile::MatrixDim::new();
            for dim in &config.attributes.dimension {
                matrix_dim.dim_list.push(*dim as u16);
            }
            td_characteristic.matrix_dim = Some(matrix_dim);
        }
        self.module.typedef_characteristic.push(td_characteristic);

        Ok(())
    }

    fn create_typedef_axis(&mut self, a2l_name: &str, config: &AxisCfg) -> Result<(), String> {
        if self.module.typedef_axis.contains_key(a2l_name)
            && !self.created_typedefs.contains(a2l_name)
        {
            // already exists, but was not created by us
            return Err(format!("TYPEDEF_AXIS '{a2l_name}' already exists"));
        }

        self.check_a2l_name(a2l_name)?;

        let description = config.attributes.description.as_deref().unwrap_or("");
        let (conversion_name, unit, format) =
            self.handle_conversion_attribute(a2l_name, config.attributes.conversion.as_ref());
        let (lower_limit, upper_limit) = config
            .range
            .unwrap_or_else(|| datatype_limits(&config.datatype));
        let base_name = Some("THIS");
        let input =
            build_input_signal_name(base_name, &config.input_signal, config.input_is_instance);

        let mut td_axis = a2lfile::TypedefAxis::new(
            a2l_name.to_string(),
            description.to_string(),
            input,
            config.layout.clone(),
            0.0,
            conversion_name,
            config.dimension[0] as u16,
            lower_limit,
            upper_limit,
        );

        td_axis.phys_unit = unit;
        td_axis.format = format;
        td_axis.byte_order = config.attributes.byte_order.map(a2lfile::ByteOrder::new);

        if let Some((lower, upper)) = config.extended_range {
            td_axis.extended_limits = Some(a2lfile::ExtendedLimits::new(lower, upper));
        }

        self.module.typedef_axis.push(td_axis);

        Ok(())
    }

    // try to estimate the size of a structure in bytes
    // this has all several problems: not all elements are necessarily defined at all, and
    // the ordering in the original program is unknown, so we don't know about padding/alignment,
    // and finally, it is not possible to get the axis dimensions of curve/map when the axis is a
    // reference to an external item.
    fn estimate_size(&self, structure: &Structure) -> u32 {
        let mut total_size: u32 = 0;
        for item in &structure.elements {
            match &item.config {
                ItemConfig::Measure(cfg) => {
                    total_size += Self::datatype_size(&cfg.datatype)
                        * cfg.attributes.dimension.iter().product::<u32>();
                }
                ItemConfig::Parameter(cfg) => {
                    total_size += Self::datatype_size(&cfg.datatype)
                        * cfg.attributes.dimension.iter().product::<u32>();
                }
                ItemConfig::CurveMap(cfg) => {
                    let x_axis_dim = Self::get_axis_dimension(&cfg.x_axis).unwrap_or(1);
                    let y_axis_dim = cfg
                        .y_axis
                        .as_ref()
                        .and_then(|y| Self::get_axis_dimension(y))
                        .unwrap_or(1);
                    total_size += Self::datatype_size(&cfg.datatype) * x_axis_dim * y_axis_dim;
                }
                ItemConfig::Axis(cfg) => {
                    total_size +=
                        Self::datatype_size(&cfg.datatype) * cfg.dimension.iter().product::<u32>();
                }
                ItemConfig::String(cfg) => {
                    total_size += cfg.length * cfg.attributes.dimension.iter().product::<u32>();
                }
                ItemConfig::SubStructure(sub_struct_cfg) => {
                    let sub_struct_name = sub_struct_cfg
                        .data_type_struct
                        .as_ref()
                        .cloned()
                        .unwrap_or_else(|| {
                            let mut path = item.structure.clone();
                            path.push(item.symbol_name.clone());
                            path.join(".")
                        });
                    if let Some(sub_structure) = self.structures.get(&sub_struct_name) {
                        let sub_structure_size = self.estimate_size(sub_structure);
                        let dim_product =
                            sub_struct_cfg.attributes.dimension.iter().product::<u32>();
                        total_size += sub_structure_size * dim_product;
                    }
                }
            }
        }
        total_size
    }

    fn get_axis_dimension(axis: &AxisInfo) -> Option<u32> {
        match axis {
            AxisInfo::Standard { dimension, .. } => Some(dimension[0]),
            AxisInfo::FixList { axis_points, .. } => Some(axis_points.len() as u32),
            AxisInfo::FixRange {
                range_min,
                range_max,
                range_step,
                ..
            } => {
                let range_step = range_step.unwrap_or(1.0);
                if range_step > 0.0 {
                    Some(((*range_max - *range_min) / range_step).floor() as u32 + 1)
                } else {
                    None
                }
            }
            AxisInfo::Common { .. } => None, // unknown!
        }
    }

    fn datatype_size(datatype: &DataType) -> u32 {
        match datatype {
            DataType::Ubyte | DataType::Sbyte => 1,
            DataType::Uword | DataType::Sword | DataType::Float16Ieee => 2,
            DataType::Ulong | DataType::Slong | DataType::Float32Ieee => 4,
            DataType::AUint64 | DataType::AInt64 | DataType::Float64Ieee => 8,
        }
    }

    //#########################################################################

    /// Handle the conversion attribute and create necessary COMPU_METHOD entries
    fn handle_conversion_attribute(
        &mut self,
        parent: &str,
        conv_attr: Option<&ConversionAttribute>,
    ) -> (String, Option<PhysUnit>, Option<Format>) {
        // The parent could be multi-dimensional and use the SPLIT attribute, e.g. "parent[x]"
        // We don't want to create COMPU_METHODs for each dimension, so we'll remove any array index at the end.
        let parent = if parent.ends_with(']')
            && let Some(pos) = parent.rfind('[')
        {
            &parent[..pos]
        } else {
            parent
        };

        // Additionally, if we're creating a COMPU_METHOD for an element inside a structure,
        // then parent name might be something like "MyInstance[x].MyItem", so we'd prefer to remove anything before the last dot.
        // The full name is only used for disambiguation in case of conflicts.
        let mut parent_short = parent;
        if let Some(pos) = parent_short.rfind('.') {
            parent_short = &parent_short[pos + 1..];
        }

        if let Some(conv) = conv_attr {
            let (mut compu_method, unit, format) = match conv {
                ConversionAttribute::Linear {
                    factor,
                    offset,
                    unit,
                    length,
                    digits,
                } => {
                    // Handle linear conversion
                    let conv_name = format!("{parent_short}.Conversion");
                    let format = build_format(length, digits).unwrap_or("%.3".to_string());
                    let mut compu_method = a2lfile::CompuMethod::new(
                        conv_name.clone(),
                        String::new(),
                        ConversionType::Linear,
                        format,
                        unit.clone(),
                    );
                    compu_method.coeffs_linear = Some(CoeffsLinear::new(*factor, *offset));

                    (compu_method, None, None)
                }
                ConversionAttribute::Formula {
                    formula,
                    inverse_formula,
                    unit,
                    length,
                    digits,
                } => {
                    // Handle formula conversion
                    let conv_name = format!("{parent_short}.Conversion");
                    let format = build_format(length, digits).unwrap_or("%.3".to_string());
                    let mut compu_method = a2lfile::CompuMethod::new(
                        conv_name.clone(),
                        String::new(),
                        ConversionType::Form,
                        format,
                        unit.clone(),
                    );
                    let mut formula = Formula::new(formula.clone());
                    if let Some(inv) = inverse_formula {
                        formula.formula_inv = Some(FormulaInv::new(inv.clone()));
                    }
                    compu_method.formula = Some(formula);

                    (compu_method, None, None)
                }
                ConversionAttribute::Table {
                    rows,
                    default_value,
                    format_values,
                } => {
                    // Handle table conversion
                    let conv_name = format!("{parent_short}.Conversion");
                    let mut compu_method = a2lfile::CompuMethod::new(
                        conv_name.clone(),
                        String::new(),
                        ConversionType::TabVerb,
                        "%.0".to_string(),
                        String::new(),
                    );
                    compu_method.compu_tab_ref = Some(CompuTabRef::new(conv_name.clone()));

                    self.create_compu_method_table(&conv_name, rows, default_value);

                    let format = if let Some((length, digits)) = format_values {
                        let fmt = format!("%{length}.{digits}");
                        Some(Format::new(fmt.clone()))
                    } else {
                        None
                    };

                    (compu_method, None, format)
                }
                ConversionAttribute::Reference {
                    name,
                    length,
                    digits,
                } => {
                    // Handle reference to an existing conversion
                    let format = build_format(length, digits).map(Format::new);
                    return (name.clone(), None, format);
                }
                ConversionAttribute::Unit {
                    name,
                    length,
                    digits,
                } => {
                    // Handle unit conversion
                    let unit = PhysUnit::new(name.clone());
                    let format = build_format(length, digits).map(Format::new);
                    return ("NO_COMPU_METHOD".to_string(), Some(unit), format);
                }
            };

            // if a conflicting COMPU_METHOD already exists using the short name, then rename the new one
            // using the full parent name
            if let Some(existing_cm) = self.module.compu_method.get(compu_method.get_name())
                && existing_cm != &compu_method
            {
                compu_method.set_name(format!("{parent}.Conversion"));
            }
            let cm_name = compu_method.get_name().to_string();
            if !self.module.compu_method.contains_key(&cm_name) {
                // only add if it does not already exist. We'll use the existing one even if it is different
                self.module.compu_method.push(compu_method.clone());
            }

            (cm_name, unit, format)
        } else {
            ("NO_COMPU_METHOD".to_string(), None, None)
        }
    }

    fn create_compu_method_table(
        &mut self,
        conv_name: &str,
        rows: &Vec<TableRow>,
        default_value: &Option<String>,
    ) {
        let is_vtab = rows.iter().any(|r| r.value2.is_some());
        if is_vtab {
            if !self.module.compu_vtab_range.contains_key(conv_name) {
                let mut compu_vtab_range = a2lfile::CompuVtabRange::new(
                    conv_name.to_string(),
                    String::new(),
                    rows.len() as u16,
                );
                for row in rows {
                    let value2 = row.value2.unwrap_or(row.value1);
                    compu_vtab_range
                        .value_triples
                        .push(a2lfile::ValueTriplesStruct::new(
                            row.value1,
                            value2,
                            row.text.clone(),
                        ));
                }
                if let Some(def_val) = default_value {
                    compu_vtab_range.default_value =
                        Some(a2lfile::DefaultValue::new(def_val.clone()));
                }
                self.module.compu_vtab_range.push(compu_vtab_range);
            }
        } else if !self.module.compu_vtab.contains_key(conv_name) {
            let mut compu_vtab = a2lfile::CompuVtab::new(
                conv_name.to_string(),
                String::new(),
                ConversionType::TabVerb,
                rows.len() as u16,
            );
            for row in rows {
                compu_vtab
                    .value_pairs
                    .push(a2lfile::ValuePairsStruct::new(row.value1, row.text.clone()));
            }
            if let Some(def_val) = default_value {
                compu_vtab.default_value = Some(a2lfile::DefaultValue::new(def_val.clone()));
            }
            self.module.compu_vtab.push(compu_vtab);
        }
    }

    /// Update the structure for an element definition
    fn update_struct(&mut self, element_def: ElementDefinition) {
        // complication: for simple sub-structures which are not arrays, the SUB_STRUCTURE definition may be omitted
        // in this case we'll have to create that here too
        for i in 0..(element_def.structure.len() - 1) {
            // For element_def.structure = ["level1", "level2", "level3"] and element_def.symbol_name = "value"
            // we need to ensure that "level1", "level1.level2" and "level1.level2.level3" exist,
            // so that we can add "value" to the structure "level1.level2.level3"
            let sub_struct_name = element_def.structure[0..=i].join(".");
            // ensure that the structure at this level exists
            let sub_struct = self
                .structures
                .entry(sub_struct_name.clone())
                .or_insert_with(|| Structure {
                    elements: Vec::new(),
                });
            // if the parent structure does not already contain a SubStructureCfg for the child element
            // then we need to create one
            if !sub_struct.elements.iter().any(|e| {
                matches!(e.config, ItemConfig::SubStructure(_))
                    && e.symbol_name == element_def.structure[i + 1]
            }) {
                // add a SubStructureCfg in the new structure
                sub_struct.elements.push(ElementDefinition {
                    a2l_name: element_def.structure[i + 1].clone(),
                    symbol_name: element_def.structure[i + 1].clone(),
                    structure: element_def.structure[0..=i].to_vec(),
                    config: ItemConfig::SubStructure(SubStructureCfg {
                        data_type_struct: None,
                        attributes: StructAttributes::default(),
                    }),
                });
            }
            if !self.structures.contains_key(&sub_struct_name) {
                // create a new structure entry
                let mut structure = Structure {
                    elements: Vec::new(),
                };
                // create a SubStructureCfg in the new structure
                structure.elements.push(ElementDefinition {
                    a2l_name: element_def.structure[i + 1].clone(),
                    symbol_name: element_def.structure[i + 1].clone(),
                    structure: element_def.structure[0..=i].to_vec(),
                    config: ItemConfig::SubStructure(SubStructureCfg {
                        data_type_struct: None,
                        attributes: StructAttributes::default(),
                    }),
                });
                self.structures.insert(sub_struct_name.clone(), structure);
            }
        }

        // get the existing struct entry in the structures map or create a new one
        let struct_entry = self
            .structures
            .entry(element_def.structure.join("."))
            .or_insert_with(|| Structure {
                elements: Vec::new(),
            });

        // warn about duplicate elements
        if let Some(pos) = struct_entry
            .elements
            .iter()
            .position(|e| e.symbol_name == element_def.symbol_name)
        {
            // usage of self.warn runs into trouble with the borrow checker
            self.messages.push(format!(
                "Warning: Element '{}' in structure '{}' is redefined. Previous definition will be overwritten.",
                element_def.symbol_name,
                element_def.structure.join("."),
            ));
            self.warnings += 1;
            struct_entry.elements.remove(pos);
        }

        // insert the element definition into the structure
        struct_entry.elements.push(element_def);
    }

    /// create an entry in a group
    ///
    /// If the group doesn't exist yet, then it is created together with any parent groups.
    /// Newly created groups might have descriptions that were set using SUB_GROUP
    fn create_group_entry(&mut self, group_spec: &[String], item_name: &str, is_measurement: bool) {
        if group_spec.is_empty() {
            return;
        }

        // create or update the main group
        if !self.module.group.contains_key(&self.main_group) {
            let desc = self.main_group_description.clone().unwrap_or_default();
            let mut group = a2lfile::Group::new(self.main_group.clone(), desc);
            group.root = Some(a2lfile::Root::new());
            self.module.group.push(group);
        }
        let main_group = self.module.group.get_mut(&self.main_group).unwrap();
        let sg_list = main_group.sub_group.get_or_insert(a2lfile::SubGroup::new());
        if !sg_list.identifier_list.contains(&group_spec[0]) {
            sg_list.identifier_list.push(group_spec[0].to_string());
        }

        // create each sub-group
        for idx in 0..group_spec.len() {
            let group_name = &group_spec[idx];
            if !self.module.group.contains_key(group_name) {
                let desc = self.sub_groups.get(group_name).cloned().unwrap_or_default();
                let group = a2lfile::Group::new(group_name.clone(), desc);
                self.module.group.push(group);
            }
            let group = self.module.group.get_mut(group_name).unwrap();
            if idx < group_spec.len() - 1 {
                let sub_group_name = &group_spec[idx + 1];
                let sg_list = group.sub_group.get_or_insert(a2lfile::SubGroup::new());
                if !sg_list.identifier_list.contains(sub_group_name) {
                    sg_list.identifier_list.push(sub_group_name.clone());
                }
            }
        }

        // add a REF_MEASUREMENT or REF_CHARACTERISTIC to the final group
        let dest_group_name = &group_spec[group_spec.len() - 1];
        let group = self.module.group.get_mut(dest_group_name).unwrap();
        if is_measurement {
            let ref_meas = group
                .ref_measurement
                .get_or_insert(a2lfile::RefMeasurement::new());
            ref_meas.identifier_list.push(item_name.to_string());
        } else {
            let ref_char = group
                .ref_characteristic
                .get_or_insert(a2lfile::RefCharacteristic::new());
            ref_char.identifier_list.push(item_name.to_string());
        }
    }

    /// Create a default record layout for the given datatype if it doesn't exist yet
    ///
    /// The default record layouts always use row-major order for array values.
    fn create_default_record_layout(&mut self, datatype: &DataType) -> String {
        let name = format!("__{datatype}_Z");
        if !self.module.record_layout.contains_key(&name) {
            let mut layout = a2lfile::RecordLayout::new(name.clone());
            let fnc_values = a2lfile::FncValues::new(
                1,
                *datatype,
                a2lfile::IndexMode::RowDir,
                a2lfile::AddrType::Direct,
            );
            layout.fnc_values = Some(fnc_values);
            self.module.record_layout.push(layout);
        }
        name
    }

    fn create_axis_descr(
        &mut self,
        axis_info: &AxisInfo,
        context_name: &str,
        base_name: Option<&str>,
    ) -> Result<a2lfile::AxisDescr, String> {
        match axis_info {
            AxisInfo::Standard {
                datatype,
                range,
                extended_range,
                dimension,
                input_signal,
                input_is_instance,
                conversion,
            } => {
                let (lower_limit, upper_limit) = range.unwrap_or_else(|| datatype_limits(datatype));
                let input = build_input_signal_name(base_name, input_signal, *input_is_instance);

                let (conversion_name, unit, format) =
                    self.handle_conversion_attribute(context_name, conversion.as_ref());

                let mut axis_descr = a2lfile::AxisDescr::new(
                    a2lfile::AxisDescrAttribute::StdAxis,
                    input,
                    conversion_name,
                    dimension[0] as u16,
                    lower_limit,
                    upper_limit,
                );
                axis_descr.phys_unit = unit;
                axis_descr.format = format;

                if let Some((lower, upper)) = extended_range {
                    axis_descr.extended_limits = Some(a2lfile::ExtendedLimits::new(*lower, *upper));
                }

                Ok(axis_descr)
            }
            AxisInfo::FixList {
                axis_points,
                input_signal,
                input_is_instance,
                conversion,
            } => {
                let lower_limit = axis_points[0];
                let upper_limit = axis_points[axis_points.len() - 1];
                let input = build_input_signal_name(base_name, input_signal, *input_is_instance);

                let (conversion_name, unit, format) =
                    self.handle_conversion_attribute(context_name, conversion.as_ref());

                let mut axis_descr = a2lfile::AxisDescr::new(
                    a2lfile::AxisDescrAttribute::FixAxis,
                    input,
                    conversion_name,
                    axis_points.len() as u16,
                    lower_limit,
                    upper_limit,
                );
                axis_descr.phys_unit = unit;
                axis_descr.format = format;

                let mut fix_axis_par_list = a2lfile::FixAxisParList::new();
                fix_axis_par_list.axis_pts_value_list = axis_points.clone();
                axis_descr.fix_axis_par_list = Some(fix_axis_par_list);

                Ok(axis_descr)
            }
            AxisInfo::FixRange {
                range_min,
                range_max,
                range_step,
                input_signal,
                input_is_instance,
                conversion,
            } => {
                let range_step = range_step.unwrap_or(1.0);
                let num_axis_points = ((*range_max - *range_min) / range_step).floor() as u16 + 1;
                let input = build_input_signal_name(base_name, input_signal, *input_is_instance);

                let (conversion_name, unit, format) =
                    self.handle_conversion_attribute(context_name, conversion.as_ref());

                let mut axis_descr = a2lfile::AxisDescr::new(
                    a2lfile::AxisDescrAttribute::FixAxis,
                    input,
                    conversion_name,
                    num_axis_points,
                    *range_min,
                    *range_max,
                );
                axis_descr.phys_unit = unit;
                axis_descr.format = format;

                // if the float values of range_min and range_step are actually integers, then we can use FixAxisParDist
                // note: the restriction to integers was fixed in a2l version 1.7, but we'll keep it for compatibility with older versions
                if *range_min == (*range_min as i16) as f64
                    && range_step == (range_step as i16) as f64
                {
                    let fix_axis_par_dist =
                        a2lfile::FixAxisParDist::new(*range_min, range_step, num_axis_points);
                    axis_descr.fix_axis_par_dist = Some(fix_axis_par_dist);
                } else {
                    // otherwise we need to use FixAxisParList
                    let mut list = a2lfile::FixAxisParList::new();
                    list.axis_pts_value_list = (0..num_axis_points)
                        .map(|i| *range_min + (i as f64 * range_step))
                        .collect();
                    axis_descr.fix_axis_par_list = Some(list);
                }
                Ok(axis_descr)
            }
            AxisInfo::Common {
                ref_name,
                is_instance,
            } => {
                let full_ref_name = if *is_instance && let Some(instance_name) = &base_name {
                    format!("{instance_name}.{ref_name}")
                } else {
                    ref_name.clone()
                };

                let mut axis_descr =
                    if let Some(axis_pts) = self.module.axis_pts.get(&full_ref_name) {
                        a2lfile::AxisDescr::new(
                            a2lfile::AxisDescrAttribute::ComAxis,
                            axis_pts.input_quantity.clone(),
                            axis_pts.conversion.clone(),
                            axis_pts.max_axis_points,
                            axis_pts.lower_limit,
                            axis_pts.upper_limit,
                        )
                    } else if let Some(typedef_axis) =
                        self.get_typedef_axis_for_axis_descr(ref_name, *is_instance, context_name)
                    {
                        a2lfile::AxisDescr::new(
                            a2lfile::AxisDescrAttribute::ComAxis,
                            typedef_axis.input_quantity.clone(),
                            typedef_axis.conversion.clone(),
                            typedef_axis.max_axis_points,
                            typedef_axis.lower_limit,
                            typedef_axis.upper_limit,
                        )
                    } else {
                        return Err(format!(
                            "Referenced axis '{}' of '{context_name}' not found",
                            full_ref_name
                        ));
                    };

                let axis_pts_ref = a2lfile::AxisPtsRef::new(full_ref_name);
                axis_descr.axis_pts_ref = Some(axis_pts_ref);

                Ok(axis_descr)
            }
        }
    }

    /// Process a conversion definition
    ///
    /// Create a new COMPU_METHOD for the conversion defined in the input
    fn process_conversion_definition(
        &mut self,
        conversion: ConversionDefinition,
    ) -> Result<(), String> {
        if self.module.compu_method.contains_key(&conversion.name) {
            return Err(format!("COMPU_METHOD '{}' already exists", conversion.name));
        }

        // basic compu_method settings
        let description = conversion.description.unwrap_or_default();
        let conv_type = match conversion.config {
            ConversionConfig::Linear(_) => ConversionType::Linear,
            ConversionConfig::Formula(_) => ConversionType::Form,
            ConversionConfig::Table(_) => ConversionType::TabVerb,
        };
        let (unit_name, format_str) = if let Some(unit) = &conversion.unit {
            let format_str = format!("%{}.{}", unit.length, unit.digits);
            (unit.name.clone(), format_str)
        } else {
            (String::new(), "%.3".to_string())
        };
        let mut compu_method = a2lfile::CompuMethod::new(
            conversion.name.clone(),
            description,
            conv_type,
            format_str,
            unit_name,
        );

        // config-dependent settings
        match conversion.config {
            ConversionConfig::Linear(linear_cfg) => {
                let coeffs_linear =
                    a2lfile::CoeffsLinear::new(linear_cfg.factor, linear_cfg.offset);
                compu_method.coeffs_linear = Some(coeffs_linear);
            }
            ConversionConfig::Formula(formula_cfg) => {
                let mut formula = a2lfile::Formula::new(formula_cfg.formula);
                if let Some(inv_formula) = formula_cfg.inverse_formula {
                    formula.formula_inv = Some(a2lfile::FormulaInv::new(inv_formula));
                }
                compu_method.formula = Some(formula);
            }
            ConversionConfig::Table(table_cfg) => {
                self.create_compu_method_table(
                    &conversion.name,
                    &table_cfg.rows,
                    &table_cfg.default_value,
                );
                compu_method.compu_tab_ref =
                    Some(a2lfile::CompuTabRef::new(conversion.name.clone()));
            }
        }

        self.module.compu_method.push(compu_method);

        Ok(())
    }

    fn process_var_criterion_definition(
        &mut self,
        var_criterion_def: VarCriterionDefinition,
    ) -> Result<(), String> {
        let variant_coding = self
            .module
            .variant_coding
            .get_or_insert(a2lfile::VariantCoding::new());

        let name = var_criterion_def.name.clone();
        if variant_coding.var_criterion.contains_key(&name) {
            return Err(format!("VAR_CRITERION '{name}' already exists",));
        }

        let description = var_criterion_def.description.as_deref().unwrap_or("");
        let mut var_criterion =
            a2lfile::VarCriterion::new(var_criterion_def.name.clone(), description.to_string());

        match var_criterion_def.selector_type {
            SelectorType::Measure => {
                var_criterion.var_measurement = Some(a2lfile::VarMeasurement::new(
                    var_criterion_def.selector.clone(),
                ));
            }
            SelectorType::Parameter => {
                var_criterion.var_selection_characteristic = Some(
                    a2lfile::VarSelectionCharacteristic::new(var_criterion_def.selector.clone()),
                );
            }
        }
        var_criterion.value_list = var_criterion_def
            .variants
            .iter()
            .map(|variant| &variant.name)
            .cloned()
            .collect();
        var_criterion.get_layout_mut().item_location.2 = vec![1]; // move the value list of the VAR_CRITERION to a separate line

        variant_coding.var_criterion.push(var_criterion);

        // build a COMPU_METHOD for the selector of the VAR_CRITERION using the defined variants
        let rows = var_criterion_def
            .variants
            .iter()
            .map(|variant| TableRow {
                value1: variant.selector_value as f64,
                value2: None,
                text: variant.name.clone(),
            })
            .collect::<Vec<_>>();
        let conversion_definition = ConversionDefinition {
            name: format!("{name}.Selector.Conversion"),
            unit: None,
            description: None,
            config: ConversionConfig::Table(TableCfg {
                rows,
                default_value: None,
            }),
        };
        let _ = self.process_conversion_definition(conversion_definition);

        // keep the var criterion definition so that it can be used later
        self.var_criterion
            .insert(var_criterion_def.name.clone(), var_criterion_def);

        // try to create deferred VAR_CHARACTERISTIC
        let deferred_var_characteristic = std::mem::take(&mut self.deferred_var_characteristic);
        for (a2l_name, address) in deferred_var_characteristic {
            self.create_var_characteristic(a2l_name, &name, address);
        }

        Ok(())
    }

    fn create_var_characteristic(
        &mut self,
        a2l_name: String,
        var_criterion_name: &str,
        address: u32,
    ) {
        let Some(var_criterion_def) = self.var_criterion.get(var_criterion_name) else {
            // named VAR_CRITERION doesn't exist (yet?) - defer creation of VAR_CHARACTERISTIC
            self.deferred_var_characteristic
                .push((var_criterion_name.to_string(), address));
            return;
        };

        let variant_coding = self
            .module
            .variant_coding
            .get_or_insert(a2lfile::VariantCoding::new());

        let addresses = var_criterion_def
            .variants
            .iter()
            .map(|variant| address + variant.offset)
            .collect::<Vec<_>>();

        let mut var_characteristic = a2lfile::VarCharacteristic::new(a2l_name);
        var_characteristic
            .criterion_name_list
            .push(var_criterion_name.to_string());
        let mut var_address = a2lfile::VarAddress::new();
        var_address.address_list = addresses;

        // set the "is hexadecimal" flag of each address to true. Additionally, the first value should be offset by one line
        let mut layout = vec![(0, true); var_address.address_list.len()];
        layout[0].0 = 1;
        var_address.get_layout_mut().item_location.0 = layout;

        var_characteristic.var_address = Some(var_address);
        variant_coding.var_characteristic.push(var_characteristic);
    }

    // group assignment gets a bit complex for elements of instances.
    // In order of precedence:
    // 1) if the instance sets an overwrite for the element, then this has precedence
    // 2) if the INSTANCE defines a group, then the elements inherit this group
    // 3) The element itself may define zero or more groups
    fn handle_group_assignment<'a>(
        &mut self,
        instance_element: Option<&InstanceElement<'a>>,
        group_attributes: &[GroupAttribute],
        item_name: &str,
        is_input: bool,
    ) {
        if let Some(group_attr) = get_overwrite_group(instance_element) {
            let group_spec = match group_attr {
                GroupAttribute::In(g)
                | GroupAttribute::Out(g)
                | GroupAttribute::Def(g)
                | GroupAttribute::Std(g) => g,
            };
            self.create_group_entry(group_spec, item_name, is_input);
        } else if let Some(instance_element) = instance_element
            && let Some(group_attr) = instance_element.instance_group
        {
            let group_spec = match group_attr {
                GroupAttribute::In(g)
                | GroupAttribute::Out(g)
                | GroupAttribute::Def(g)
                | GroupAttribute::Std(g) => g,
            };
            self.create_group_entry(group_spec, item_name, is_input);
        } else {
            for group_attr in group_attributes {
                let group_spec = match group_attr {
                    GroupAttribute::In(g)
                    | GroupAttribute::Out(g)
                    | GroupAttribute::Def(g)
                    | GroupAttribute::Std(g) => g,
                };
                self.create_group_entry(group_spec, item_name, is_input);
            }
        }
    }

    fn get_typedef_axis_for_axis_descr(
        &self,
        ref_name: &str,
        is_instance: bool,  // set if the definition has the INSTANCE_NAME flag
        context_name: &str, // this is the full name of the parent item
    ) -> Option<&a2lfile::TypedefAxis> {
        // context_name is something like "base_struct.characteristic.XAxis" or "base_struct.sub_struct.characteristic.XAxis"
        // in any case we'll need to strip two levels to get the name of the containing structure
        let name_components = if is_instance {
            let mut c = context_name.split(".").collect::<Vec<_>>();
            // remove the last two components, which are the characteristic and the axis itself
            c.pop();
            c.pop();
            // add the ref_name as the last component
            c.push(ref_name);
            c
        } else {
            context_name.split(".").collect::<Vec<_>>()
        };

        let typedefs = self.module.typedefs();
        let mut current_typedef = typedefs.get(name_components[0])?;
        let mut pos = 1;
        while pos < name_components.len()
            && let AnyTypedef::TypedefStructure(td_struct) = current_typedef
        {
            let next_typename = &td_struct
                .structure_component
                .get(name_components[pos])?
                .component_type;
            current_typedef = typedefs.get(next_typename)?;
            pos += 1;
        }

        if pos == name_components.len()
            && let AnyTypedef::TypedefAxis(td_axis) = current_typedef
        {
            Some(td_axis)
        } else {
            None
        }
    }
}

fn make_base_name(parent_name: &str) -> Option<&str> {
    let pos = parent_name.rfind(".")?;
    Some(&parent_name[..pos])
}

fn build_input_signal_name(
    base_name: Option<&str>,
    input_signal: &Option<String>,
    is_instanced: bool,
) -> String {
    if let Some(input_signal) = input_signal {
        if is_instanced && let Some(base_name) = base_name {
            format!("{base_name}.{input_signal}")
        } else {
            input_signal.clone()
        }
    } else {
        "NO_INPUT_QUANTITY".to_string()
    }
}

/// choose betweeen a range supplied by the configuration and a range override provided by the instance (if any)
///
/// if there is an instance-specific override, it takes precedence
fn choose_range(
    config_range: &Option<(f64, f64)>,
    instance_element: Option<&InstanceElement<'_>>,
    datatype: &DataType,
) -> (f64, f64) {
    if let Some(overwrite_range) = get_overwrite_range(instance_element) {
        overwrite_range
    } else if let Some((lower_limit, upper_limit)) = config_range {
        (*lower_limit, *upper_limit)
    } else {
        datatype_limits(datatype)
    }
}

/// choose between a description supplied by the configuration and a description override provided by the instance (if any)
///
/// if there is an instance-supplied description, it takes precedence
fn choose_description<'a>(
    config: Option<&'a str>,
    instance_element: Option<&InstanceElement<'a>>,
) -> &'a str {
    if let Some(overwrite_desc) = get_overwrite_description(instance_element) {
        overwrite_desc
    } else {
        config.unwrap_or("")
    }
}

/// choose between a conversion supplied by the configuration and a conversion override provided by the instance (if any)
///
/// if there is an instance-specific override, it takes precedence
fn choose_conversion<'a>(
    config: &'a Option<ConversionAttribute>,
    instance_element: Option<&InstanceElement<'a>>,
) -> Option<&'a ConversionAttribute> {
    if let Some(overwrite_conv) = get_overwrite_conversion(instance_element) {
        Some(overwrite_conv)
    } else {
        config.as_ref()
    }
}

/// choose between an alias supplied by the configuration and an alias override provided by the instance (if any)
///
/// if there is an instance-specific override, it takes precedence
fn choose_alias<'a>(
    config: &'a Option<String>,
    instance_element: Option<&InstanceElement<'a>>,
) -> Option<&'a str> {
    if let Some(overwrite_alias) = get_overwrite_alias(instance_element) {
        Some(overwrite_alias)
    } else {
        config.as_deref()
    }
}

/// build a format string for the given length and digits
/// this has the form %{length}.{digits} or %.{digits}
fn build_format(length: &Option<u64>, digits: &Option<u64>) -> Option<String> {
    match (length, digits) {
        (Some(l), Some(d)) => Some(format!("%{l}.{d}")),
        (None, Some(d)) => Some(format!("%.{d}")),
        _ => None,
    }
}

fn datatype_limits(datatype: &DataType) -> (f64, f64) {
    match datatype {
        DataType::AInt64 => (i64::MIN as f64, i64::MAX as f64),
        DataType::Slong => (i32::MIN as f64, i32::MAX as f64),
        DataType::Sword => (i16::MIN as f64, i16::MAX as f64),
        DataType::Sbyte => (i8::MIN as f64, i8::MAX as f64),
        DataType::AUint64 => (0f64, u64::MAX as f64),
        DataType::Ulong => (0f64, u32::MAX as f64),
        DataType::Uword => (0f64, u16::MAX as f64),
        DataType::Ubyte => (0f64, u8::MAX as f64),
        DataType::Float16Ieee => (-6.5504e+4_f64, 6.5504e+4_f64), // rust support for f16 is currently experimental, so the constants are not yet available
        DataType::Float32Ieee => (f32::MIN as f64, f32::MAX as f64),
        DataType::Float64Ieee => (f64::MIN, f64::MAX),
    }
}

fn create_canape_ext(
    symbol_name: &str,
    address: u32,
    address_ext: Option<u32>,
    base_offset: &u32,
) -> a2lfile::IfData {
    let mut canape_ext = crate::ifdata::CanapeExt::new(100);
    let address_ext = address_ext.unwrap_or(0);
    canape_ext.link_map = Some(crate::ifdata::LinkMap::new(
        symbol_name.to_string(),
        address as i32,
        address_ext as u16,
        0,
        *base_offset as i32,
        0,
        0,
        0,
    ));
    let mut ifdata_content = crate::ifdata::A2mlVector::new();
    ifdata_content.canape_ext = Some(canape_ext);
    let mut ifdata = a2lfile::IfData::new();
    ifdata_content.store_to_ifdata(&mut ifdata);
    ifdata
}

impl<'a> SplitIterator<'a> {
    fn new(
        dimensions: &'a [u32],
        split: &'a SplitType,
        a2l_name: &'a str,
        symbol_name: &'a str,
        use_new_arrays: bool,
    ) -> Self {
        let limit = dimensions.iter().product::<u32>();
        SplitIterator {
            dimensions,
            split,
            base_a2l_name: a2l_name,
            base_symbol_name: symbol_name,
            limit,
            current_value: 0,
            use_new_arrays,
        }
    }
}

/// iterate over all components of a multi-dimensional object
/// e.g. var[0][0], var[0][1], var[1][0], var[1][1] for a 2x2 array
///
/// This iterator will yield all combinations of indices for the given dimensions.
///
/// The key complication is that the a2l names don't necessarily need to use normal indexes "[x]",
/// but can use user-supplied suffixes or custom format strings instead.
impl Iterator for SplitIterator<'_> {
    // returns the split a2lname and the split symbol name
    type Item = (String, String);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_value >= self.limit {
            return None;
        }

        // decompose self.current_value into individual indexes in each of the item's dimensions
        let mut indices = vec![0; self.dimensions.len()];
        let mut rem = self.current_value;
        // going backward over the list of array dimensions, divide and keep the remainder
        for idx in (0..self.dimensions.len()).rev() {
            indices[idx] = rem % self.dimensions[idx];
            rem /= self.dimensions[idx];
        }
        debug_assert!(rem == 0);
        // Implement the logic to split the dimensions and create the (String, String) pairs

        // create the index string for the symbol name. this is simple, since it only uses [x][y][z] style indexing
        let idxstr = indices.iter().fold(String::new(), |mut output, val| {
            if self.use_new_arrays {
                let _ = write!(output, "[{val}]");
            } else {
                let _ = write!(output, "._{val}_");
            }
            output
        });
        let symbol_name = format!("{}{idxstr}", self.base_symbol_name);

        // this is the complicated part, where the a2l names are generated
        let result = match self.split {
            SplitType::Auto => {
                // auto: nothing fancy was specified, so we can use the same index string that was used for the symbol name
                let a2l_name = format!("{}{idxstr}", self.base_a2l_name);
                Some((a2l_name, symbol_name))
            }
            SplitType::Manual(names) => {
                // a list of suffixes exists, which are used in-order
                // for multi-dimensional arrays, we use row-major order
                if self.current_value < names.len() as u32 {
                    let postfix = names[self.current_value as usize].clone();
                    let a2l_name = format!("{}{postfix}", self.base_a2l_name);
                    Some((a2l_name, symbol_name))
                } else {
                    // if we run out of suffixes before we run out of dimensions, then the iteration is done early
                    None
                }
            }
            SplitType::Template(template) => {
                // a template string exists, which is applied to the current list of indices
                // this template must contain one format specifier "%_" for each index
                let postfix = apply_template(template, &indices)?;
                let a2l_name = format!("{}{postfix}", self.base_a2l_name);
                Some((a2l_name, symbol_name))
            }
        };
        self.current_value += 1;
        result
    }
}

/// Apply a split template to a list of indices
/// The template must have one format specifier for each index, which is one of
///   %d - decimal integer
///   %x - hexadecimal integer using lowercase
///   %X - hexadecimal integer using uppercase
///   %c - lowercase character from a to z; indices > 26 return None
///   %C - uppercase character from A to Z; indices > 26 return None
/// other characters in the template are copied to the output as-is
fn apply_template(template: &str, indices: &[u32]) -> Option<String> {
    let mut chars_iter = template.chars();
    let mut output = String::with_capacity(template.len());
    let mut current_index = 0;

    while let Some(c) = chars_iter.next() {
        if c == '%' {
            let idx = indices.get(current_index)?;
            match chars_iter.next() {
                Some('d') => {
                    output.push_str(&idx.to_string());
                    current_index += 1;
                }
                Some('x') => {
                    output.push_str(&format!("{:x}", idx));
                    current_index += 1;
                }
                Some('X') => {
                    output.push_str(&format!("{:X}", idx));
                    current_index += 1;
                }
                Some('c') => {
                    if *idx < 26 {
                        output.push((b'a' + *idx as u8) as char);
                    } else {
                        return None;
                    }
                    current_index += 1;
                }
                Some('C') => {
                    if *idx < 26 {
                        output.push((b'A' + *idx as u8) as char);
                    } else {
                        return None;
                    }
                    current_index += 1;
                }
                None => output.push('%'), // stray '%' at end of string
                _ => return None,
            }
        } else {
            output.push(c);
        }
    }
    Some(output)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_apply_template() {
        let template = "a_%d_b_%x_c_%C";
        let indices = vec![1, 2, 3];
        let result = apply_template(template, &indices);
        assert_eq!(result, Some("a_1_b_2_c_D".into()));
    }
}

fn get_overwrite_conversion<'a>(
    instance_element: Option<&InstanceElement<'a>>,
) -> Option<&'a ConversionAttribute> {
    let instance_element = instance_element?;
    instance_element.overwrites.iter().find_map(|ov_spec| {
        if let OverwriteSpec::Conversion(conv) = &ov_spec.details
            && instance_element.struct_path[1..] == ov_spec.element_path
        {
            Some(conv)
        } else {
            None
        }
    })
}

fn get_overwrite_description<'a>(
    instance_element: Option<&InstanceElement<'a>>,
) -> Option<&'a str> {
    let instance_element = instance_element?;
    instance_element.overwrites.iter().find_map(|ov_spec| {
        if let OverwriteSpec::Description(desc) = &ov_spec.details
            && instance_element.struct_path[1..] == ov_spec.element_path
        {
            Some(desc.as_str())
        } else {
            None
        }
    })
}

fn get_overwrite_alias<'a>(instance_element: Option<&InstanceElement<'a>>) -> Option<&'a str> {
    let instance_element = instance_element?;
    instance_element.overwrites.iter().find_map(|ov_spec| {
        if let OverwriteSpec::Alias(alias) = &ov_spec.details
            && instance_element.struct_path[1..] == ov_spec.element_path
        {
            Some(alias.as_str())
        } else {
            None
        }
    })
}

fn get_overwrite_group<'a>(
    instance_element: Option<&InstanceElement<'a>>,
) -> Option<&'a GroupAttribute> {
    let instance_element = instance_element?;
    instance_element.overwrites.iter().find_map(|ov_spec| {
        if let OverwriteSpec::GroupAssignment(group) = &ov_spec.details
            && instance_element.struct_path[1..] == ov_spec.element_path
        {
            Some(group)
        } else {
            None
        }
    })
}

fn get_overwrite_range<'a>(instance_element: Option<&InstanceElement<'a>>) -> Option<(f64, f64)> {
    let instance_element = instance_element?;
    instance_element.overwrites.iter().find_map(|ov_spec| {
        if let OverwriteSpec::Range(lower, upper) = &ov_spec.details
            && instance_element.struct_path[1..] == ov_spec.element_path
        {
            Some((*lower, *upper))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use a2lfile::CharacteristicType;

    #[test]
    fn measurement() {
        let input = br#"
        /*
        @@ SYMBOL = MeasurementName
        @@ A2L_TYPE = MEASURE
        @@ OtherMeasurementName
        @@ DATA_TYPE = UBYTE 0x3f [3...40]
        @@ DESCRIPTION = "Test description"
        @@ ADDRESS = 0xD00D
        @@ ADDRESS_EXTENSION = 3
        @@ DIMENSION = 10
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let measurement = module.measurement.get("OtherMeasurementName").unwrap();
        let Some(symbol_link) = &measurement.symbol_link else {
            panic!("missing symbol link");
        };
        assert_eq!(symbol_link.symbol_name, "MeasurementName");
        assert_eq!(measurement.bit_mask.as_ref().unwrap().mask, 0x3f);
        assert_eq!(measurement.lower_limit, 3.0);
        assert_eq!(measurement.upper_limit, 40.0);
        assert_eq!(measurement.ecu_address.as_ref().unwrap().address, 0xD00D);
        assert_eq!(
            measurement
                .ecu_address_extension
                .as_ref()
                .unwrap()
                .extension,
            3
        );
        assert_eq!(measurement.matrix_dim.as_ref().unwrap().dim_list, vec![10]);
    }

    #[test]
    fn measurement_array() {
        let input = br#"
        /*
        @@ SYMBOL = MeasurementArrayName
        @@ A2L_TYPE = MEASURE
        @@ WRITEABLE
        @@ DATA_TYPE = FLOAT [33.3...9876]
        @@ DESCRIPTION = "Test description"
        @@ GROUP = parent | TestGroup
        @@ DIMENSION = 3 4 SPLIT
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        assert_eq!(module.measurement.len(), 12);
        let measurement = module
            .measurement
            .get("MeasurementArrayName[2][1]")
            .unwrap();
        let Some(symbol_link) = &measurement.symbol_link else {
            panic!("missing symbol link");
        };
        assert_eq!(symbol_link.symbol_name, "MeasurementArrayName[2][1]");
        assert_eq!(measurement.lower_limit, 33.3);
        assert_eq!(measurement.upper_limit, 9876.0);
        assert!(measurement.read_write.is_some());
    }

    #[test]
    fn parameter() {
        let input = br#"
        /*
        @@ SYMBOL = ParameterName
        @@ A2L_TYPE = PARAMETER
        @@ DATA_TYPE = SLONG 0xFFFFFFFF [ -1000...1000 ]
        @@ DESCRIPTION = "Test description"
        @@ GROUP = parent | TestGroup
        @@ ADDRESS = 0xF000
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let characteristic = module.characteristic.get("ParameterName").unwrap();
        let Some(symbol_link) = &characteristic.symbol_link else {
            panic!("missing symbol link");
        };
        assert_eq!(symbol_link.symbol_name, "ParameterName");
        assert_eq!(characteristic.bit_mask.as_ref().unwrap().mask, 0xFFFFFFFF);
        assert_eq!(characteristic.lower_limit, -1000.0);
        assert_eq!(characteristic.upper_limit, 1000.0);
        assert_eq!(characteristic.address, 0xF000);
    }

    #[test]
    fn parameter_array() {
        let input = br#"
        /*
        @@ SYMBOL = ParameterArrayName
        @@ A2L_TYPE = PARAMETER
        @@ DATA_TYPE = SLONG 0x1 [ -1000...1000 ]
        @@ DESCRIPTION = "Test description"
        @@ GROUP = parent | TestGroup
        @@ DIMENSION = 3 4 SPLIT USE "_A" "_B" "_C" "_D" "_E" "_F" "_G" "_H" "_I" "_J" "_K" "_L"
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let characteristic = module.characteristic.get("ParameterArrayName_J").unwrap();
        let Some(symbol_link) = &characteristic.symbol_link else {
            panic!("missing symbol link");
        };
        assert_eq!(symbol_link.symbol_name, "ParameterArrayName[2][1]");
        assert_eq!(characteristic.bit_mask.as_ref().unwrap().mask, 0x1);
        assert_eq!(characteristic.lower_limit, -1000.0);
        assert_eq!(characteristic.upper_limit, 1000.0);
    }

    #[test]
    fn curve_parameter() {
        let input = br#"
        /*
        @@ SYMBOL = CurveName
        @@ A2L_TYPE = CURVE
        @@ WRITEABLE
        @@ DATA_TYPE = DOUBLE [0...100] [-10 ... 1000]
        @@ LAYOUT = MapLayout
        @@ CONVERSION = FORMULA "x*2+3" INVERSE "(x-3)/2" "unit" 8 4
        @@ DESCRIPTION = "Map description"
        @@ ALIAS = MapAlias
        @@ BASE_OFFSET = 2
        @@ GROUP OUT = parent | MapGroup
        @@ ADDRESS = 0x87654321
        @@ ADDRESS_EXTENSION = 0x20
        @@ BYTE_ORDER = MOTOROLA
        @@ X_AXIS = STANDARD
        @@   DATA_TYPE = SBYTE
        @@   DIMENSION = 10
        @@   INPUT = InputSignal
        @@   CONVERSION = LINEAR 1 0 "unit" 8 4
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let characteristic = module.characteristic.get("CurveName").unwrap();
        let Some(symbol_link) = &characteristic.symbol_link else {
            panic!("missing symbol link");
        };
        assert_eq!(symbol_link.symbol_name, "CurveName");
        assert_eq!(characteristic.address, 0x87654321);
        assert_eq!(
            characteristic
                .ecu_address_extension
                .as_ref()
                .unwrap()
                .extension,
            0x20
        );
        assert_eq!(
            characteristic.byte_order.as_ref().unwrap().byte_order,
            ByteOrderEnum::MsbFirst
        );
        assert_eq!(characteristic.conversion, "CurveName.Conversion");

        assert_eq!(characteristic.axis_descr.len(), 1);
        assert_eq!(characteristic.axis_descr[0].conversion, "XAxis.Conversion");
        assert_eq!(characteristic.axis_descr[0].input_quantity, "InputSignal");

        assert!(module.compu_method.contains_key("CurveName.Conversion"));
        assert!(module.compu_method.contains_key("XAxis.Conversion"));
    }

    #[test]
    fn map_parameter() {
        let input = br#"
        /*
        @@ SYMBOL = MapName
        @@ A2L_TYPE = MAP
        @@ DATA_TYPE = DOUBLE
        @@ LAYOUT = MapLayout
        @@ X_AXIS = FIX  1.11 22.5 999
        @@   INPUT = InputSignalX
        @@   CONVERSION = PredefinedConversion
        @@ Y_AXIS = FIX [0 ... 10], 2
        @@   INPUT = InputSignalY
        @@   CONVERSION = LINEAR 1 0 "unit" 8 4
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let characteristic = module.characteristic.get("MapName").unwrap();
        assert_eq!(characteristic.characteristic_type, CharacteristicType::Map);

        assert_eq!(characteristic.axis_descr.len(), 2);
        assert_eq!(
            characteristic.axis_descr[0].conversion,
            "PredefinedConversion"
        );
        assert_eq!(characteristic.axis_descr[0].input_quantity, "InputSignalX");
        assert_eq!(characteristic.axis_descr[1].conversion, "YAxis.Conversion");
        assert_eq!(characteristic.axis_descr[1].input_quantity, "InputSignalY");
    }

    #[test]
    fn curve_with_external_axis() {
        let input = br#"
        /*
        @@ SYMBOL = AxisName
        @@ A2L_TYPE = AXIS
        @@ DATA_TYPE = SWORD
        @@ LAYOUT = AxisLayout
        @@ DIMENSION = 3
        @@ END
        */;

        /*
        @@ SYMBOL = CurveName
        @@ A2L_TYPE = CURVE
        @@ DATA_TYPE = UWORD
        @@ LAYOUT = CurveLayout
        @@ X_AXIS = COMMON AxisName
        @@ END
        */"#;
        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = creator.module;
        assert!(module.characteristic.contains_key("CurveName"));
        let curve = module.characteristic.get("CurveName").unwrap();
        assert_eq!(curve.characteristic_type, CharacteristicType::Curve);
        assert_eq!(curve.axis_descr.len(), 1);
        assert_eq!(curve.axis_descr[0].input_quantity, "NO_INPUT_QUANTITY");
        assert_eq!(
            curve.axis_descr[0]
                .axis_pts_ref
                .as_ref()
                .unwrap()
                .axis_points,
            "AxisName"
        );
    }

    #[test]
    fn string_parameter() {
        let input = br#"
        /*
        @@ SYMBOL = StringParameterName
        @@ A2L_TYPE = STRING 100
        @@ DESCRIPTION = "Test description"
        @@ GROUP = parent | TestGroup
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = creator.module;
        assert!(module.characteristic.contains_key("StringParameterName"));
        let string_param = module.characteristic.get("StringParameterName").unwrap();
        assert_eq!(string_param.characteristic_type, CharacteristicType::Ascii);
        assert_eq!(string_param.number.as_ref().unwrap().number, 100);
        assert_eq!(string_param.long_identifier, "Test description");
    }

    #[test]
    fn string_array() {
        let input = br#"
        /*
        @@ SYMBOL = StringArrayName
        @@ A2L_TYPE = STRING 50
        @@ DIMENSION = 5 SPLIT USE "_A" "_B" "_C" "_D" "_E"
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        assert_eq!(module.characteristic.len(), 5);
        let string_param = module.characteristic.get("StringArrayName_C").unwrap();
        assert_eq!(string_param.characteristic_type, CharacteristicType::Ascii);
        assert_eq!(string_param.number.as_ref().unwrap().number, 50);
    }

    #[test]
    fn axis() {
        let input = br#"
        /*
        @@ SYMBOL = AxisName
        @@ A2L_TYPE = AXIS
        @@ READ_ONLY
        @@ AxisNameA2l
        @@ DATA_TYPE = SWORD [0...100] [-10 ... 1000]
        @@ LAYOUT = AxisLayout
        @@ DIMENSION = 3
        @@ INPUT = AxisInput
        @@ CONVERSION = TABLE
        @@   0 "Low"
        @@   1 "Medium"
        @@   2 "High"
        @@ DESCRIPTION = "Axis description"
        @@ ALIAS = AxisAlias
        @@ BASE_OFFSET = 2
        @@ GROUP OUT = parent | AxisGroup
        @@ ADDRESS = 0x87654321
        @@ ADDRESS_EXTENSION = 0x20
        @@ VAR_CRITERION = variant_axis
        @@ BYTE_ORDER = INTEL
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let axis_pts = module.axis_pts.get("AxisNameA2l").unwrap();
        let Some(symbol_link) = &axis_pts.symbol_link else {
            panic!("missing symbol link");
        };
        assert_eq!(symbol_link.symbol_name, "AxisName");
        assert_eq!(axis_pts.address, 0x87654321);
        assert_eq!(
            axis_pts.ecu_address_extension.as_ref().unwrap().extension,
            0x20
        );
        assert_eq!(
            axis_pts.byte_order.as_ref().unwrap().byte_order,
            ByteOrderEnum::MsbLast
        );
        assert_eq!(axis_pts.conversion, "AxisNameA2l.Conversion");

        assert!(module.compu_method.contains_key("AxisNameA2l.Conversion"));
        assert!(module.compu_method.contains_key("AxisNameA2l.Conversion"));
    }

    #[test]
    fn conversion_table() {
        let input = br#"
        /*
        @@ CONVERSION = TableConversion
        @@ A2L_TYPE = TABLE
        @@   0 "Fire"
        @@   1 "Water"
        @@   2 "Earth"
        @@ DEFAULT_VALUE "Void"
        @@ UNIT = "$?" 0 0
        @@ DESCRIPTION = "Table conversion description"
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = creator.module;
        assert!(module.compu_method.contains_key("TableConversion"));
        let compu_method = module.compu_method.get("TableConversion").unwrap();
        assert_eq!(
            compu_method.conversion_type,
            a2lfile::ConversionType::TabVerb
        );
        assert_eq!(compu_method.unit, "$?");
        assert_eq!(compu_method.long_identifier, "Table conversion description");

        assert!(module.compu_vtab.contains_key("TableConversion"));
        let compu_vtab = module.compu_vtab.get("TableConversion").unwrap();
        assert_eq!(compu_vtab.value_pairs.len(), 3);
        assert_eq!(compu_vtab.value_pairs[0].in_val, 0.0);
        assert_eq!(compu_vtab.value_pairs[0].out_val, "Fire");
        assert_eq!(compu_vtab.value_pairs[1].in_val, 1.0);
        assert_eq!(compu_vtab.value_pairs[1].out_val, "Water");
        assert_eq!(compu_vtab.value_pairs[2].in_val, 2.0);
        assert_eq!(compu_vtab.value_pairs[2].out_val, "Earth");
        assert_eq!(
            compu_vtab.default_value.as_ref().unwrap().display_string,
            "Void"
        );
    }

    #[test]
    fn conversion_linear() {
        let input = br#"
        /*
        @@ CONVERSION = LinearConversion
        @@ A2L_TYPE = LINEAR 2.5 5.1
        @@ UNIT = "m/s" 6 2
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = creator.module;
        assert!(module.compu_method.contains_key("LinearConversion"));
        let compu_method = module.compu_method.get("LinearConversion").unwrap();
        assert_eq!(
            compu_method.conversion_type,
            a2lfile::ConversionType::Linear
        );
        assert_eq!(compu_method.unit, "m/s");
        assert_eq!(compu_method.coeffs_linear.as_ref().unwrap().a, 2.5);
        assert_eq!(compu_method.coeffs_linear.as_ref().unwrap().b, 5.1);
    }

    #[test]
    fn structure() {
        let input = br#"
        struct OuterStruct {
            /*
            @@ ELEMENT = x
            @@ STRUCTURE = OuterStruct
            @@ A2L_TYPE = MEASURE
            @@ DATA_TYPE = SLONG
            @@ END
            */
            int x;

            struct InnerStruct1 {
                /*
                @@ ELEMENT = y
                @@ STRUCTURE = OuterStruct | inner1
                @@ A2L_TYPE = MEASURE
                @@ DATA_TYPE = ULONG
                @@ END
                */
                unsigned int y;
            } inner1;

            /*
            @@ SUB_STRUCTURE = inner2
            @@ STRUCTURE = OuterStruct
            @@ DIMENSION = 5 SPLIT
            @@ END
            */
            struct InnerStruct2 {
                /*
                @@ ELEMENT = z
                @@ STRUCTURE = OuterStruct | inner2
                @@ A2L_TYPE = MEASURE
                @@ DATA_TYPE = ULONG
                @@ END
                */
                unsigned int z;
            } inner2[5];
        };

        /*
        @@ INSTANCE = var1
        @@ STRUCTURE = OuterStruct
        @@ END
        */
        struct OuterStruct var1;

        /*
        @@ INSTANCE = var2
        @@ STRUCTURE = OuterStruct
        @@ DIMENSION = 2 SPLIT
        @@ END
        */
        struct OuterStruct var2[2];
        "#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        assert_eq!(creator.structures.len(), 3);
        assert!(creator.structures.contains_key("OuterStruct"));
        assert!(creator.structures.contains_key("OuterStruct.inner1"));
        assert!(creator.structures.contains_key("OuterStruct.inner2"));

        let outerstruct = creator.structures.get("OuterStruct").unwrap();
        assert_eq!(outerstruct.elements.len(), 3);

        let module = creator.module;
        // The structure defines a total of 7 MEASUREs, which are instantiated 3 times,
        // so there should now be 21 MEASUREMENTs in the module
        assert_eq!(module.measurement.len(), 21);

        assert!(module.measurement.contains_key("var1.x"));
        assert!(module.measurement.contains_key("var1.inner2[4].z"));
        assert!(module.measurement.contains_key("var2[1].inner1.y"));
    }

    #[test]
    fn main_group() {
        let input = br#"
        /*
        @@ MAIN_GROUP = MainGroup
        @@ DESCRIPTION = "Main group description"
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        // the main group is only created once an item is created that belongs to it
        let module = creator.module;
        assert!(module.group.is_empty());
        // the creator stores the main group information until it is needed
        assert_eq!(creator.main_group, "MainGroup");
        assert_eq!(
            creator.main_group_description.as_deref(),
            Some("Main group description")
        );
    }

    #[test]
    fn sub_group() {
        let input = br#"
        /*
        @@ SUB_GROUP = SubGroup
        @@ DESCRIPTION = "Sub group description"
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        // the sub group is only created once an item is created that belongs to it
        let module = creator.module;
        assert!(module.group.is_empty());
        // the creator stores the sub group information until it is needed
        assert_eq!(creator.sub_groups.len(), 1);
        assert_eq!(
            creator.sub_groups.get("SubGroup").as_deref().unwrap(),
            "Sub group description"
        );
    }

    #[test]
    fn var_criterion() {
        let input = br#"
        /*
        @@ VAR_CRITERION = Variant
        @@ DESCRIPTION = "Variant description"
        @@ SELECTOR = MEASURE InputMeasurement
        @@   VARIANT = Apple 1 0x0
        @@   VARIANT = Orange 2 0x1000
        @@   VARIANT = Banana 3 0x2000
        @@ END
        */

        /*
        @@ SYMBOL = VariantCodedParam
        @@ A2L_TYPE = PARAMETER
        @@ DATA_TYPE = UBYTE
        @@ VAR_CRITERION = Variant
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = creator.module;
        assert!(module.variant_coding.is_some());
        let variant_coding = module.variant_coding.as_ref().unwrap();
        let var_criterion = variant_coding.var_criterion.get("Variant").unwrap();
        assert_eq!(
            var_criterion.var_measurement.as_ref().unwrap().name,
            "InputMeasurement"
        );
        assert!(
            module
                .compu_method
                .contains_key("Variant.Selector.Conversion")
        );
        assert_eq!(variant_coding.var_characteristic.len(), 1);
        let var_char = variant_coding
            .var_characteristic
            .get("VariantCodedParam")
            .unwrap();
        assert_eq!(var_char.criterion_name_list[0], "Variant");
    }

    #[test]
    fn old_symbol_link() {
        let input = br#"
        /*
        @@ SYMBOL = MeasurementName
        @@ A2L_TYPE = MEASURE
        @@ DATA_TYPE = UBYTE
        @@ END
        */"#;

        let mut a2l_file = a2lfile::new();
        // Set the ASAP2 version to 1.5, which does not have SYMBOL_LINK
        if let Some(ver) = a2l_file.asap2_version.as_mut() {
            ver.version_no = 1;
            ver.upgrade_no = 50;
        }

        let mut creator = Creator::new(&mut a2l_file, None, false, false);
        creator.process_file(input);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let module = &a2l_file.project.module[0];
        let measurement = module.measurement.get("MeasurementName").unwrap();
        assert!(measurement.symbol_link.is_none());
        assert_eq!(measurement.if_data.len(), 1);
    }

    #[test]
    fn instance_test_no_structs() {
        // generate elements from an instance with enable_structures = false
        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, false, false);

        let data = std::fs::read("fixtures/a2l/from_source_input.txt").unwrap();
        creator.process_file(&data);
        assert_eq!(creator.warnings, 2); // Note: missing SPLIT attribute on the two INSTANCE definitions
        assert_eq!(creator.errors, 0);

        let (expected_a2l, _) = a2lfile::load("fixtures/a2l/from_source.a2l", None, false).unwrap();

        let module = &a2l_file.project.module[0];
        let expected_module = &expected_a2l.project.module[0];

        assert_eq!(
            module.characteristic.len(),
            expected_module.characteristic.len()
        );
        for name in expected_module.characteristic.keys() {
            assert!(
                module.characteristic.contains_key(name),
                "Missing CHARACTERISTIC for {name}"
            );
        }

        assert_eq!(module.measurement.len(), expected_module.measurement.len());
        for name in expected_module.measurement.keys() {
            assert!(
                module.measurement.contains_key(name),
                "Missing MEASUREMENT for {name}"
            );
        }

        assert_eq!(module.axis_pts.len(), expected_module.axis_pts.len());
        for name in expected_module.axis_pts.keys() {
            assert!(
                module.axis_pts.contains_key(name),
                "Missing AXIS_PTS for {name}"
            );
        }

        assert_eq!(
            module.compu_method.len(),
            expected_module.compu_method.len()
        );
        for name in expected_module.compu_method.keys() {
            assert!(
                module.compu_method.contains_key(name),
                "Missing COMP_METHOD for {name}"
            );
        }

        assert_eq!(module.compu_vtab.len(), expected_module.compu_vtab.len());
        for name in expected_module.compu_vtab.keys() {
            assert!(
                module.compu_vtab.contains_key(name),
                "Missing COMP_VTAB for {name}"
            );
        }

        // no INSTANCEs should be present
        assert!(module.instance.is_empty());
        assert!(expected_module.instance.is_empty());
    }

    #[test]
    fn instance_test_with_structs() {
        // generate elements from an instance with enable_structures = true
        let mut a2l_file = a2lfile::new();
        let mut creator = Creator::new(&mut a2l_file, None, true, false);

        let data = std::fs::read("fixtures/a2l/from_source_input.txt").unwrap();
        creator.process_file(&data);
        assert_eq!(creator.warnings, 0);
        assert_eq!(creator.errors, 0);

        let (expected_a2l, _) =
            a2lfile::load("fixtures/a2l/from_source_structs.a2l", None, false).unwrap();
        let module = &a2l_file.project.module[0];
        let expected_module = &expected_a2l.project.module[0];

        // no conventional elements should be present
        assert!(module.characteristic.is_empty());
        assert!(expected_module.characteristic.is_empty());
        assert!(module.measurement.is_empty());
        assert!(expected_module.measurement.is_empty());
        assert!(module.axis_pts.is_empty());
        assert!(expected_module.axis_pts.is_empty());

        // all elements should be present as INSTANCEs and TYPEDEFs
        assert_eq!(module.instance.len(), expected_module.instance.len());
        for name in expected_module.instance.keys() {
            assert!(
                module.instance.contains_key(name),
                "Missing INSTANCE for {name}"
            );
        }

        assert_eq!(
            module.typedef_structure.len(),
            expected_module.typedef_structure.len()
        );

        assert_eq!(
            module.typedef_characteristic.len(),
            expected_module.typedef_characteristic.len()
        );

        assert_eq!(
            module.typedef_measurement.len(),
            expected_module.typedef_measurement.len()
        );

        assert_eq!(
            module.typedef_axis.len(),
            expected_module.typedef_axis.len()
        );

        assert_eq!(
            module.compu_method.len(),
            expected_module.compu_method.len()
        );
    }
}
